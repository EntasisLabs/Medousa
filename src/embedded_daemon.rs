//! In-process deployment of the Medousa daemon for native mobile hosts.
//!
//! This module is a composition root, not a second runtime. It binds the
//! existing daemon authority, Stasis control plane, session store, turn owner,
//! ticket registry, durable journal, and production foreground loop to a
//! trusted co-located client.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use genai::chat::ChatMessage;
use medousa_engine::{TurnPipelineHandle, TurnStreamRegistryPort};
use medousa_runtime::{
    CredentialedAiChatClient, CredentialedAiChatConfig, MAX_REQUEST_PROMPT_CHARS,
    MedousaToolLoopPipeline,
};
use medousa_types::daemon_api::{
    AgentModeId, AgentModeSource, CancelActiveSessionTurnResponse, CreateSessionResponse,
    InteractiveTurnResponse, SessionAgentModeResponse, SessionCodeBindingResponse,
};
use medousa_types::secrets::InstallationId;
use medousa_types::session::{ConversationTurn, SessionHistorySummary, TranscriptEntry};
use medousa_types::turn_stream::{TurnStreamEnvelopeV2, TurnStreamEventV2};
use medousa_types::turn_ticket::{TurnTicket, TurnTicketMode, TurnTicketPhase};
use serde_json::json;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolLoopExecutionRequest,
};
use stasis::application::orchestration::tool_registry::{
    InMemoryToolRegistry, StasisTool, ToolRegistry,
};
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::domain::runtime::cluster_node::{
    ClusterNode, ClusterNodeHeartbeat, ClusterNodeRole, NewClusterNode,
};
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};
use stasis::ports::outbound::runtime::cluster_node_store::ClusterNodeStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory, StasisRuntimeBuilder};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub use medousa_runtime::{CredentialProvider, ProviderCredential, ProviderCredentialError};

use crate::execution_context::{
    ProviderRoute, SurfaceCapabilities, TurnExecutionContext, TurnExecutionRegistry,
    with_turn_execution_context,
};
use crate::persistent_locus::build_persistent_locus_memory;
use crate::request_principal::{Capability, RequestPrincipal, TransportClass};
use crate::session_storage::{SessionId, new_session_id};
use crate::session_store::{
    SessionStore, SurrealSessionStore, TranscriptAppend, configure_file_session_root,
    set_session_store,
};
use crate::turn_event_channel::TurnEventSubscription;
use crate::turn_pipeline_output::{TurnJournalOutput, daemon_turn_pipeline_budget};
use crate::turn_scope::TurnContinuationScope;
use crate::turn_stream_registry::{
    TurnStreamEntry, TurnStreamRegistry, TurnStreamRegistryPortAdapter, new_turn_stream_registry,
    turn_stream_log,
};
use crate::turn_ticket::{
    ActiveSessionTurnResponse, TurnTicketRegistry, cancel_interactive_for_session,
    get_active_interactive_turn, mark_cancelled, new_registry, note_stream_event, prompt_preview,
    register_turn,
};

const EMBEDDED_STREAM_SCHEME: &str = "medousa-embedded://turn";
const EMBEDDED_NODE_LEASE_SECONDS: i64 = 300;
const DEFAULT_FOREGROUND_TURN_TIMEOUT: Duration = Duration::from_secs(180);
const STREAM_DELTA_CAPACITY: usize = 128;

/// Mobile capability layer over the daemon's injected tool registry.
///
/// The canonical turn-control protocol is always daemon-owned and cannot be
/// replaced by a host-supplied registry. Every other admitted tool delegates
/// to the registry selected by the embedded deployment composition.
struct EmbeddedToolRegistry {
    delegate: Arc<dyn ToolRegistry>,
    turn_control: InMemoryToolRegistry,
}

impl EmbeddedToolRegistry {
    fn new(delegate: Arc<dyn ToolRegistry>) -> StasisResult<Self> {
        let turn_control = InMemoryToolRegistry::default();
        turn_control.register_tool(EmbeddedTurnControlTool)?;
        Ok(Self {
            delegate,
            turn_control,
        })
    }
}

#[async_trait::async_trait]
impl ToolRegistry for EmbeddedToolRegistry {
    async fn list_tools(&self) -> StasisResult<Vec<genai::chat::Tool>> {
        let mut tools = self.delegate.list_tools().await?;
        tools.retain(|tool| tool.name.as_str() != medousa_runtime::turn_control::COGNITION_TURN);
        tools.extend(self.turn_control.list_tools().await?);
        Ok(tools)
    }

    async fn invoke_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> StasisResult<serde_json::Value> {
        if tool_name.trim() == medousa_runtime::turn_control::COGNITION_TURN {
            self.turn_control.invoke_tool(tool_name, input).await
        } else {
            self.delegate.invoke_tool(tool_name, input).await
        }
    }
}

struct EmbeddedTurnControlTool;

#[async_trait::async_trait]
impl StasisTool for EmbeddedTurnControlTool {
    fn name(&self) -> &'static str {
        medousa_runtime::turn_control::COGNITION_TURN
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Control this foreground turn: update the user, checkpoint for input, prepare a final response, or finish.",
        )
    }

    fn input_schema(&self) -> Option<serde_json::Value> {
        Some(json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "turn.update_user",
                        "turn.checkpoint",
                        "turn.prepare_final",
                        "turn.finish"
                    ]
                },
                "message": { "type": "string" },
                "awaiting": { "type": "string" },
                "reason": { "type": "string" }
            },
            "additionalProperties": false
        }))
    }

    async fn invoke(&self, input: serde_json::Value) -> StasisResult<serde_json::Value> {
        let action = input
            .get("action")
            .and_then(serde_json::Value::as_str)
            .map(str::trim);
        match action {
            Some("turn.prepare_final") => Ok(json!({ "ok": true })),
            Some("turn.update_user" | "turn.checkpoint" | "turn.finish")
                if input
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|message| !message.trim().is_empty()) =>
            {
                Ok(json!({ "ok": true }))
            }
            Some("turn.update_user" | "turn.checkpoint" | "turn.finish") => Err(
                StasisError::PortFailure("turn-control message must be non-empty".to_string()),
            ),
            _ => Err(StasisError::PortFailure(
                "turn-control action is outside the embedded foreground capability ceiling"
                    .to_string(),
            )),
        }
    }
}

/// Deployment configuration assembled by the native host.
///
/// The chat client already owns its credential-provider boundary; secret
/// material is never retained here or accepted from the UI request.
pub struct EmbeddedDaemonConfig {
    root: PathBuf,
    installation_id: InstallationId,
    provider: String,
    model: String,
    chat_client: Arc<dyn AiChatClient>,
    tool_registry: Arc<dyn ToolRegistry>,
    foreground_turn_timeout: Duration,
    max_live_turns: usize,
}

impl EmbeddedDaemonConfig {
    /// Bind the portable explicit-credential adapter to a host credential port.
    pub fn credentialed(
        root: impl Into<PathBuf>,
        installation_id: InstallationId,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
        credentials: Arc<dyn CredentialProvider>,
    ) -> Result<Self> {
        let provider = provider.into();
        let model = model.into();
        let ai_config = CredentialedAiChatConfig::new(provider.clone(), model.clone(), base_url)
            .context("invalid embedded inference configuration")?;
        let chat_client = Arc::new(
            CredentialedAiChatClient::new(ai_config, credentials)
                .context("initialize embedded inference client")?,
        );
        Ok(Self::with_chat_client(
            root,
            installation_id,
            provider,
            model,
            chat_client,
        ))
    }

    /// Bind an existing Stasis inference port. Useful for alternate native
    /// adapters and deterministic daemon integration tests.
    pub fn with_chat_client(
        root: impl Into<PathBuf>,
        installation_id: InstallationId,
        provider: impl Into<String>,
        model: impl Into<String>,
        chat_client: Arc<dyn AiChatClient>,
    ) -> Self {
        Self {
            root: root.into(),
            installation_id,
            provider: provider.into(),
            model: model.into(),
            chat_client,
            tool_registry: Arc::new(InMemoryToolRegistry::default()),
            foreground_turn_timeout: DEFAULT_FOREGROUND_TURN_TIMEOUT,
            max_live_turns: 1,
        }
    }

    /// Supply the daemon's already-filtered mobile tool registry.
    pub fn with_tool_registry(mut self, tool_registry: Arc<dyn ToolRegistry>) -> Self {
        self.tool_registry = tool_registry;
        self
    }

    pub fn with_foreground_turn_timeout(mut self, timeout: Duration) -> Self {
        self.foreground_turn_timeout = timeout.max(Duration::from_secs(1));
        self
    }

    pub fn with_max_live_turns(mut self, max_live_turns: usize) -> Self {
        self.max_live_turns = max_live_turns.max(1);
        self
    }
}

impl std::fmt::Debug for EmbeddedDaemonConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmbeddedDaemonConfig")
            .field("root", &self.root)
            .field("installation_id", &self.installation_id)
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("chat_client", &"REDACTED")
            .field("tool_registry", &"capability-filtered")
            .field("foreground_turn_timeout", &self.foreground_turn_timeout)
            .field("max_live_turns", &self.max_live_turns)
            .finish()
    }
}

/// One in-process deployment of `medousa_daemon`.
pub struct EmbeddedDaemon {
    root: PathBuf,
    authority_id: medousa_types::session::AuthorityId,
    local_credential_id: Arc<str>,
    provider: Arc<str>,
    model: Arc<str>,
    chat_client: Arc<dyn AiChatClient>,
    tool_registry: Arc<dyn ToolRegistry>,
    session_store: Arc<dyn SessionStore>,
    _runtime: RuntimeComposition,
    _locus_memory: Arc<stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore>,
    cluster_node_store: Arc<dyn ClusterNodeStore>,
    cluster_node: ClusterNode,
    turn_streams: TurnStreamRegistry,
    turn_stream_port: TurnStreamRegistryPortAdapter,
    turn_tickets: TurnTicketRegistry,
    executions: TurnExecutionRegistry,
    foreground_turn_timeout: Duration,
    suspended: AtomicBool,
}

impl EmbeddedDaemon {
    /// Boot the daemon against one app-sandbox root.
    pub async fn boot(config: EmbeddedDaemonConfig) -> Result<Arc<Self>> {
        let root = prepare_root(&config.root).await?;
        configure_file_session_root(root.join("history")).map_err(|error| anyhow!(error))?;

        let turn_log_root = root.join(medousa_engine::TURN_LOG_DIR);
        medousa_engine::configure_log_root(turn_log_root.clone());
        if medousa_engine::default_log_root() != turn_log_root {
            bail!("turn journal root was already configured for another daemon deployment");
        }

        let authority_id = crate::workshop_authority::initialize(&config.installation_id)
            .map_err(|error| anyhow!(error))?
            .clone();

        let surreal_path = root.join("runtime.surrealkv");
        let surreal_path = surreal_path
            .to_str()
            .ok_or_else(|| anyhow!("embedded daemon root is not valid UTF-8"))?
            .to_string();
        let backend = RuntimeBackend::surreal_kv(surreal_path, "medousa", "runtime");
        let runtime = StasisRuntimeBuilder::new(backend)
            .with_chat_client(config.chat_client.clone())
            .build()
            .await
            .context("boot embedded Stasis runtime")?;

        let (session_store, locus_memory): (
            Arc<dyn SessionStore>,
            Arc<stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore>,
        ) = match &runtime {
            RuntimeComposition::Surreal(runtime) => {
                let store = Arc::new(SurrealSessionStore::new(runtime.job_store.db()));
                store
                    .ensure_schema()
                    .await
                    .context("initialize embedded session schema")?;
                let locus_memory = build_persistent_locus_memory(runtime.job_store.db())
                    .await
                    .context("initialize embedded Locus memory")?;
                (store, locus_memory)
            }
            RuntimeComposition::InMemory(_) => {
                bail!("embedded daemon requires its SurrealKV persistence backend")
            }
        };
        set_session_store(session_store.clone());

        let cluster_node_store = RuntimeFactory::resolve_cluster_node_store(&runtime, None);
        let cluster_node = register_or_heartbeat_node(
            cluster_node_store.as_ref(),
            &config.installation_id,
            &authority_id,
        )
        .await?;

        let turn_streams = new_turn_stream_registry();
        let turn_stream_port = TurnStreamRegistryPortAdapter::new(turn_streams.clone());
        let local_credential_id: Arc<str> = Arc::from(format!(
            "embedded-home:{}",
            config.installation_id.storage_key().as_str()
        ));

        Ok(Arc::new(Self {
            root,
            authority_id,
            local_credential_id,
            provider: Arc::from(config.provider),
            model: Arc::from(config.model),
            chat_client: config.chat_client,
            tool_registry: Arc::new(
                EmbeddedToolRegistry::new(config.tool_registry)
                    .context("initialize embedded tool capability layer")?,
            ),
            session_store,
            _runtime: runtime,
            _locus_memory: locus_memory,
            cluster_node_store,
            cluster_node,
            turn_streams,
            turn_stream_port,
            turn_tickets: new_registry(),
            executions: TurnExecutionRegistry::new(config.max_live_turns),
            foreground_turn_timeout: config.foreground_turn_timeout,
            suspended: AtomicBool::new(false),
        }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn authority_id(&self) -> &medousa_types::session::AuthorityId {
        &self.authority_id
    }

    pub fn cluster_node(&self) -> &ClusterNode {
        &self.cluster_node
    }

    /// Issue the only local-root client admitted by the in-process bridge.
    pub fn local_client(self: &Arc<Self>) -> EmbeddedDaemonClient {
        EmbeddedDaemonClient {
            daemon: self.clone(),
            principal: RequestPrincipal::local_app(
                self.local_credential_id.clone(),
                TransportClass::Loopback,
            ),
        }
    }

    /// Cancel foreground work before iOS suspends the process.
    pub fn suspend(&self) -> usize {
        self.suspended.store(true, Ordering::Release);
        self.executions.cancel_all()
    }

    /// Re-advertise the same Stasis node after returning to foreground.
    pub async fn resume(&self) -> Result<()> {
        let heartbeat = ClusterNodeHeartbeat {
            node_id: self.cluster_node.node_id.clone(),
            heartbeat_at: Utc::now(),
            lease_ttl_seconds: EMBEDDED_NODE_LEASE_SECONDS,
            queue_ownership: Some(self.cluster_node.queue_ownership.clone()),
            capability_tags: Some(self.cluster_node.capability_tags.clone()),
            metadata: self.cluster_node.metadata.clone(),
        };
        self.cluster_node_store
            .heartbeat(heartbeat)
            .await
            .context("heartbeat embedded Stasis node")?
            .ok_or_else(|| anyhow!("embedded Stasis node registration is missing"))?;
        self.suspended.store(false, Ordering::Release);
        Ok(())
    }

    pub fn live_turn_count(&self) -> usize {
        self.executions.live_count()
    }

    async fn ensure_turn_stream(&self, turn_id: &str) -> Result<TurnStreamEntry> {
        if !self.turn_stream_port.has_stream(turn_id).await
            && !self.turn_stream_port.register_stream(turn_id).await
        {
            bail!("failed to open daemon turn stream '{turn_id}'");
        }
        let entry = self
            .turn_streams
            .read()
            .await
            .get(turn_id)
            .cloned()
            .ok_or_else(|| anyhow!("daemon turn stream '{turn_id}' is unavailable"))?;
        if entry.log.is_committed() {
            entry.channel.mark_closed();
        }
        Ok(entry)
    }

    async fn execute_foreground_turn(
        self: Arc<Self>,
        lease: crate::execution_context::TurnExecutionLease,
        prompt: String,
        prior_messages: Vec<ChatMessage>,
        stream: TurnStreamEntry,
    ) {
        let context = lease.context().clone();
        let turn_id = context.turn_id().to_string();
        let session_id = context.session_id().clone();
        let pipeline = TurnPipelineHandle::spawn(
            &turn_id,
            stream.log.replay_fence(),
            daemon_turn_pipeline_budget(),
            Arc::new(TurnJournalOutput::new(
                stream.channel.clone(),
                stream.log.clone(),
            )),
        );

        if pipeline
            .emit(TurnStreamEventV2::Status {
                phase: "accepted".to_string(),
                operator_message: Some("foreground turn accepted".to_string()),
                debug_message: None,
            })
            .await
            .is_err()
        {
            self.turn_stream_port.mark_stream_closed(&turn_id).await;
            return;
        }
        note_stream_event(&self.turn_tickets, &turn_id, "status", "accepted", false).await;
        let _ = pipeline
            .emit(TurnStreamEventV2::ModelReceipt {
                provider: self.provider.to_string(),
                model: self.model.to_string(),
            })
            .await;

        let execution_ref =
            match crate::workshop_authority::execution_ref(session_id.as_str(), &turn_id) {
                Ok(value) => value,
                Err(error) => {
                    self.finish_with_error(
                        &turn_id,
                        &pipeline,
                        "turn identity unavailable",
                        &error,
                    )
                    .await;
                    self.turn_stream_port.mark_stream_closed(&turn_id).await;
                    return;
                }
            };
        let user_turn =
            ConversationTurn::plain("user", prompt.clone(), Utc::now(), Vec::new(), None);
        if let Err(error) = self
            .session_store
            .append_transcript_batch(
                &session_id,
                &[TranscriptAppend::native(
                    user_turn,
                    Some(execution_ref.clone()),
                )],
            )
            .await
        {
            self.finish_with_error(
                &turn_id,
                &pipeline,
                "could not persist the user turn",
                &error.to_string(),
            )
            .await;
            self.turn_stream_port.mark_stream_closed(&turn_id).await;
            return;
        }

        let prompt_pipeline = PromptExecutionPipeline::new(self.chat_client.clone());
        let tool_loop = MedousaToolLoopPipeline::new(prompt_pipeline, self.tool_registry.clone());
        let request = ToolLoopExecutionRequest {
            user_prompt: prompt,
            system_prompt: None,
            context: PromptExecutionContext {
                correlation_id: Some(context.correlation_id().to_string()),
                model_hint: Some(self.model.to_string()),
                ..PromptExecutionContext::default()
            },
            tool_name: String::new(),
            tool_input: json!({}),
            tool_call_mode: ToolCallMode::Auto,
        };
        let (delta_tx, mut delta_rx) = mpsc::channel(STREAM_DELTA_CAPACITY);
        let outcome = {
            let execution = with_turn_execution_context(
                context.clone(),
                tool_loop.execute_with_stream_prior_messages(
                    request,
                    prior_messages,
                    Some(&delta_tx),
                ),
            );
            tokio::pin!(execution);

            loop {
                tokio::select! {
                    biased;
                    () = context.cancellation().cancelled() => break ForegroundOutcome::Cancelled,
                    delta = delta_rx.recv() => {
                        let Some(delta) = delta else { continue; };
                        if let Err(error) = emit_provider_delta(&pipeline, delta).await {
                            break ForegroundOutcome::Failed(error.to_string());
                        }
                    }
                    result = &mut execution => {
                        break match result {
                            Ok(response) => ForegroundOutcome::Completed {
                                text: response.text,
                                tool_names: response
                                    .tool_invocations
                                    .into_iter()
                                    .map(|invocation| invocation.tool_name)
                                    .collect(),
                            },
                            Err(error) => ForegroundOutcome::Failed(error.to_string()),
                        };
                    }
                }
            }
        };
        drop(delta_tx);
        while let Ok(delta) = delta_rx.try_recv() {
            if emit_provider_delta(&pipeline, delta).await.is_err() {
                break;
            }
        }

        match outcome {
            ForegroundOutcome::Completed { text, tool_names } => {
                let assistant_turn = ConversationTurn::plain(
                    "assistant",
                    text.clone(),
                    Utc::now(),
                    tool_names.clone(),
                    None,
                );
                match self
                    .session_store
                    .append_transcript_batch(
                        &session_id,
                        &[TranscriptAppend::native(
                            assistant_turn,
                            Some(execution_ref),
                        )],
                    )
                    .await
                {
                    Ok(_) => {
                        if pipeline
                            .emit(TurnStreamEventV2::Final { text, tool_names })
                            .await
                            .is_ok()
                        {
                            note_stream_event(&self.turn_tickets, &turn_id, "final", "done", true)
                                .await;
                        }
                    }
                    Err(error) => {
                        self.finish_with_error(
                            &turn_id,
                            &pipeline,
                            "could not persist the assistant turn",
                            &error.to_string(),
                        )
                        .await;
                    }
                }
            }
            ForegroundOutcome::Cancelled => {
                let _ = pipeline
                    .emit(TurnStreamEventV2::Error {
                        operator_message: "foreground turn cancelled".to_string(),
                        debug_message: None,
                    })
                    .await;
                mark_cancelled(&self.turn_tickets, &turn_id).await;
            }
            ForegroundOutcome::Failed(error) => {
                self.finish_with_error(&turn_id, &pipeline, "foreground turn failed", &error)
                    .await;
            }
        }

        self.turn_stream_port.mark_stream_closed(&turn_id).await;
        drop(lease);
    }

    async fn finish_with_error(
        &self,
        turn_id: &str,
        pipeline: &TurnPipelineHandle,
        operator_message: &str,
        debug_message: &str,
    ) {
        tracing::warn!(turn_id, error = %debug_message, "{operator_message}");
        let _ = pipeline
            .emit(TurnStreamEventV2::Error {
                operator_message: operator_message.to_string(),
                debug_message: None,
            })
            .await;
        note_stream_event(&self.turn_tickets, turn_id, "error", "error", true).await;
    }
}

enum ForegroundOutcome {
    Completed {
        text: String,
        tool_names: Vec<String>,
    },
    Cancelled,
    Failed(String),
}

/// Trusted client handle issued only by the co-located daemon bridge.
#[derive(Clone)]
pub struct EmbeddedDaemonClient {
    daemon: Arc<EmbeddedDaemon>,
    principal: RequestPrincipal,
}

impl EmbeddedDaemonClient {
    pub fn principal(&self) -> &RequestPrincipal {
        &self.principal
    }

    pub fn authority_id(&self) -> &medousa_types::session::AuthorityId {
        &self.daemon.authority_id
    }

    pub fn inference_provider(&self) -> &str {
        &self.daemon.provider
    }

    pub fn inference_model(&self) -> &str {
        &self.daemon.model
    }

    pub fn create_session(&self) -> Result<CreateSessionResponse> {
        self.require(Capability::WorkshopInteract)?;
        let session_id = new_session_id();
        Ok(CreateSessionResponse {
            authority_id: self.daemon.authority_id.clone(),
            session_id: session_id.to_string(),
            catalog: "single".to_string(),
            display_name: None,
            member_profile_ids: Vec::new(),
            agent_profile_id: None,
        })
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionHistorySummary>> {
        self.require(Capability::ContentRead)?;
        Ok(self
            .daemon
            .session_store
            .list_history_sessions(limit.clamp(1, 1000)))
    }

    pub fn load_history(&self, session_id: &str) -> Result<Vec<ConversationTurn>> {
        self.require(Capability::ContentRead)?;
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        Ok(self.daemon.session_store.load_history(&session_id))
    }

    pub fn load_transcript_entries(&self, session_id: &str) -> Result<Vec<TranscriptEntry>> {
        self.require(Capability::ContentRead)?;
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        Ok(self
            .daemon
            .session_store
            .load_transcript_entries(&session_id))
    }

    /// Fresh embedded sessions use the daemon's canonical general-mode
    /// selection until the persisted mode-state service joins this profile.
    pub fn session_agent_mode(&self, session_id: &str) -> Result<SessionAgentModeResponse> {
        self.require(Capability::WorkshopRead)?;
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        Ok(SessionAgentModeResponse {
            session_id: session_id.to_string(),
            selected_mode: None,
            task_lease: None,
            effective_mode: AgentModeId::General,
            effective_source: AgentModeSource::Default,
            revision: 0,
            updated_at_utc: None,
        })
    }

    /// Code work is outside the first mobile capability ceiling, so a fresh
    /// embedded session has no daemon-side Forge binding.
    pub fn session_code_binding(&self, session_id: &str) -> Result<SessionCodeBindingResponse> {
        self.require(Capability::WorkshopRead)?;
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        Ok(SessionCodeBindingResponse {
            session_id: session_id.to_string(),
            work_id: None,
            updated_at_utc: None,
        })
    }

    pub async fn start_turn(
        &self,
        session_id: &str,
        prompt: impl Into<String>,
    ) -> Result<InteractiveTurnResponse> {
        self.start_turn_with_context(session_id, prompt, None, None)
            .await
    }

    pub async fn start_turn_with_context(
        &self,
        session_id: &str,
        prompt: impl Into<String>,
        identity_user_id: Option<String>,
        channel_surface: Option<String>,
    ) -> Result<InteractiveTurnResponse> {
        self.require(Capability::WorkshopInteract)?;
        if self.daemon.suspended.load(Ordering::Acquire) {
            bail!("embedded daemon is suspended");
        }
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        let prompt = prompt.into();
        if prompt.trim().is_empty() {
            bail!("turn prompt cannot be empty");
        }
        if prompt.chars().count() > MAX_REQUEST_PROMPT_CHARS {
            bail!("turn prompt exceeds the foreground prompt limit");
        }

        let prior_messages =
            history_to_chat_messages(self.daemon.session_store.load_history(&session_id));
        let turn_id = format!("daemon-turn-{}", Uuid::new_v4().simple());
        let stream_url = format!("{EMBEDDED_STREAM_SCHEME}/{turn_id}/stream");
        let accepted_at_utc = Utc::now();
        register_turn(
            &self.daemon.turn_tickets,
            TurnTicket {
                turn_id: turn_id.clone(),
                session_id: session_id.to_string(),
                mode: TurnTicketMode::Interactive,
                phase: TurnTicketPhase::Accepted,
                stream_url: stream_url.clone(),
                prompt_preview: prompt_preview(&prompt),
                workspace_card_id: None,
                started_at: accepted_at_utc,
                updated_at: accepted_at_utc,
            },
        )
        .await
        .map_err(|error| anyhow!(error.message))?;

        let stream = match self.daemon.ensure_turn_stream(&turn_id).await {
            Ok(stream) => stream,
            Err(error) => {
                mark_cancelled(&self.daemon.turn_tickets, &turn_id).await;
                return Err(error);
            }
        };
        let cancellation = CancellationToken::new();
        let scope = TurnContinuationScope {
            turn_correlation_id: turn_id.clone(),
            session_id: session_id.to_string(),
            identity_user_id,
            original_prompt: prompt.clone(),
            delivery_target: None,
            provider: self.daemon.provider.to_string(),
            model: self.daemon.model.to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: true,
            supports_browser_host: false,
            channel_surface: channel_surface.or_else(|| Some("mobile".to_string())),
        };
        let context = TurnExecutionContext::new(
            turn_id.clone(),
            turn_id.clone(),
            session_id,
            self.principal.clone(),
            ProviderRoute::new(self.daemon.provider.clone(), self.daemon.model.clone()),
            SurfaceCapabilities {
                ui_artifacts: false,
                liquid_markdown: true,
                browser_host: false,
            },
            cancellation,
            Instant::now() + self.daemon.foreground_turn_timeout,
            scope,
        );
        let lease = match self.daemon.executions.admit(context) {
            Ok(lease) => lease,
            Err(error) => {
                mark_cancelled(&self.daemon.turn_tickets, &turn_id).await;
                return Err(anyhow!(error));
            }
        };

        let daemon = self.daemon.clone();
        tokio::spawn(async move {
            daemon
                .execute_foreground_turn(lease, prompt, prior_messages, stream)
                .await;
        });

        Ok(InteractiveTurnResponse {
            turn_id,
            accepted_at_utc,
            stream_url,
            stream_ready: true,
            fallback_to_local: false,
            fallback_reason: None,
            daemon_notice: None,
        })
    }

    pub async fn active_turn(&self, session_id: &str) -> Result<ActiveSessionTurnResponse> {
        self.require(Capability::WorkshopRead)?;
        SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        Ok(get_active_interactive_turn(&self.daemon.turn_tickets, session_id).await)
    }

    pub async fn cancel_active_turn(
        &self,
        session_id: &str,
    ) -> Result<CancelActiveSessionTurnResponse> {
        self.require(Capability::WorkshopInteract)?;
        let parsed = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        let Some(ticket) =
            cancel_interactive_for_session(&self.daemon.turn_tickets, session_id).await
        else {
            return Ok(CancelActiveSessionTurnResponse {
                cancelled: false,
                turn_id: None,
                message: "no active interactive turn".to_string(),
            });
        };
        let cancelled = self
            .daemon
            .executions
            .cancel_matching_turn(&parsed, &ticket.turn_id);
        Ok(CancelActiveSessionTurnResponse {
            cancelled,
            turn_id: Some(ticket.turn_id),
            message: if cancelled {
                "foreground turn cancellation requested".to_string()
            } else {
                "turn was no longer executing".to_string()
            },
        })
    }

    /// Subscribe before taking the replay fence; monotonic sequence filtering
    /// closes the replay/live race without inventing another stream cursor.
    pub async fn subscribe_turn(&self, turn_id: &str, since: u64) -> Result<EmbeddedTurnStream> {
        self.require(Capability::ContentRead)?;
        let entry = self.daemon.ensure_turn_stream(turn_id).await?;
        let live = entry
            .channel
            .try_subscribe()
            .ok_or_else(|| anyhow!("turn stream subscriber limit reached"))?;
        let replay = entry
            .log
            .snapshot_since(since)
            .into_iter()
            .map(|event| crate::sse_turn_projection::sequenced_to_v2(&event))
            .collect::<std::result::Result<VecDeque<_>, _>>()
            .map_err(|error| anyhow!(error))?;
        Ok(EmbeddedTurnStream {
            replay,
            live,
            last_seq: since,
        })
    }

    pub async fn replay_turn(
        &self,
        turn_id: &str,
        since: u64,
    ) -> Result<Vec<TurnStreamEnvelopeV2>> {
        self.require(Capability::ContentRead)?;
        self.daemon.ensure_turn_stream(turn_id).await?;
        let log = turn_stream_log(&self.daemon.turn_streams, turn_id)
            .await
            .ok_or_else(|| anyhow!("turn journal is unavailable"))?;
        log.snapshot_since(since)
            .into_iter()
            .map(|event| {
                crate::sse_turn_projection::sequenced_to_v2(&event).map_err(anyhow::Error::msg)
            })
            .collect()
    }

    fn require(&self, capability: Capability) -> Result<()> {
        if self.principal.capabilities().contains(capability) {
            Ok(())
        } else {
            bail!(
                "local daemon client lacks capability '{}'",
                capability.as_str()
            )
        }
    }
}

/// Replay-first stream over the daemon's canonical event channel.
pub struct EmbeddedTurnStream {
    replay: VecDeque<TurnStreamEnvelopeV2>,
    live: TurnEventSubscription,
    last_seq: u64,
}

impl EmbeddedTurnStream {
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub async fn recv(&mut self) -> Result<Option<TurnStreamEnvelopeV2>> {
        while let Some(event) = self.replay.pop_front() {
            if event.seq > self.last_seq {
                self.last_seq = event.seq;
                return Ok(Some(event));
            }
        }
        loop {
            match self.live.recv().await {
                Ok(event) => {
                    if event.seq() <= self.last_seq {
                        continue;
                    }
                    let envelope = match event.v2 {
                        Some(envelope) => envelope,
                        None => crate::sse_turn_projection::v1_to_v2(&event.v1)
                            .map_err(anyhow::Error::msg)?,
                    };
                    self.last_seq = envelope.seq;
                    return Ok(Some(envelope));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(None),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    bail!(
                        "turn stream lagged by {skipped} events; replay from sequence {}",
                        self.last_seq
                    )
                }
            }
        }
    }
}

async fn prepare_root(root: &Path) -> Result<PathBuf> {
    tokio::fs::create_dir_all(root)
        .await
        .with_context(|| format!("create embedded daemon root {}", root.display()))?;
    tokio::fs::canonicalize(root)
        .await
        .with_context(|| format!("resolve embedded daemon root {}", root.display()))
}

async fn register_or_heartbeat_node(
    store: &dyn ClusterNodeStore,
    installation_id: &InstallationId,
    authority_id: &medousa_types::session::AuthorityId,
) -> Result<ClusterNode> {
    let node_id = installation_id.to_string();
    let metadata = Some(
        serde_json::to_string(&json!({
            "product": "medousa_daemon",
            "deployment": "embedded",
            "authority_id": authority_id,
        }))
        .context("encode embedded Stasis node metadata")?,
    );
    let capability_tags = vec![
        "medousa.daemon".to_string(),
        "deployment.embedded".to_string(),
        "turn.foreground".to_string(),
        "persistence.surrealkv".to_string(),
    ];
    if store
        .get(&node_id)
        .await
        .context("read embedded Stasis node")?
        .is_some()
    {
        return store
            .heartbeat(ClusterNodeHeartbeat {
                node_id,
                heartbeat_at: Utc::now(),
                lease_ttl_seconds: EMBEDDED_NODE_LEASE_SECONDS,
                queue_ownership: Some(Vec::new()),
                capability_tags: Some(capability_tags),
                metadata,
            })
            .await
            .context("heartbeat embedded Stasis node")?
            .ok_or_else(|| anyhow!("embedded Stasis node disappeared during heartbeat"));
    }

    store
        .register(NewClusterNode {
            node_id,
            role: ClusterNodeRole::Worker,
            region: "local".to_string(),
            queue_ownership: Vec::new(),
            capability_tags,
            heartbeat_at: Utc::now(),
            lease_ttl_seconds: EMBEDDED_NODE_LEASE_SECONDS,
            metadata,
        })
        .await
        .context("register embedded Stasis node")
}

fn history_to_chat_messages(history: Vec<ConversationTurn>) -> Vec<ChatMessage> {
    history
        .into_iter()
        .filter_map(|turn| match turn.role.as_str() {
            "user" => Some(ChatMessage::user(turn.content)),
            "assistant" | "agent" => Some(ChatMessage::assistant(turn.content)),
            "system" => Some(ChatMessage::system(turn.content)),
            _ => None,
        })
        .collect()
}

async fn emit_provider_delta(
    pipeline: &TurnPipelineHandle,
    delta: StreamDelta,
) -> Result<(), medousa_engine::TurnPipelineError> {
    match delta {
        StreamDelta::Content(text) => {
            pipeline
                .emit(TurnStreamEventV2::ContentAppend { text })
                .await?;
        }
        StreamDelta::Reasoning(text) => {
            pipeline
                .emit(TurnStreamEventV2::ReasoningAppend { text })
                .await?;
        }
        StreamDelta::ThoughtSignature(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::atomic::AtomicUsize;

    use async_trait::async_trait;
    use genai::ModelIden;
    use genai::adapter::AdapterKind;
    use genai::chat::{ChatOptions, ChatRequest, ChatResponse, MessageContent};
    use stasis::domain::errors::Result as StasisResult;

    use super::*;
    use crate::request_principal::PrincipalKind;

    const INSTALLATION_ID: &str = "bf8907dd-0cad-4c60-995e-10f65aad16f1";
    const SECRET_CANARY: &str = "embedded-secret-must-never-escape";
    const FIRST_REPLY: &str = "The embedded daemon owns this foreground turn.";
    const SECOND_REPLY: &str = "The mobile shell remains only its privileged local client.";

    fn text_response(text: &str) -> ChatResponse {
        let model = ModelIden::from_static(AdapterKind::OpenAI, "embedded-test-model");
        ChatResponse {
            content: MessageContent::from(text.to_string()),
            reasoning_content: None,
            model_iden: model.clone(),
            provider_model_iden: model,
            stop_reason: None,
            usage: Default::default(),
            captured_raw_body: None,
            response_id: None,
        }
    }

    #[derive(Default)]
    struct LifecycleChatClient {
        calls: AtomicUsize,
    }

    impl LifecycleChatClient {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::Acquire)
        }

        async fn next(&self) -> StasisResult<ChatResponse> {
            match self.calls.fetch_add(1, Ordering::AcqRel) {
                0 => Ok(text_response(FIRST_REPLY)),
                1 => Ok(text_response(SECOND_REPLY)),
                _ => pending::<StasisResult<ChatResponse>>().await,
            }
        }
    }

    #[async_trait]
    impl AiChatClient for LifecycleChatClient {
        async fn complete(
            &self,
            _request: ChatRequest,
            _options: Option<&ChatOptions>,
        ) -> StasisResult<ChatResponse> {
            self.next().await
        }

        async fn complete_stream(
            &self,
            _request: ChatRequest,
            _options: Option<&ChatOptions>,
            chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
        ) -> StasisResult<ChatResponse> {
            let response = self.next().await?;
            if let (Some(tx), Some(text)) = (chunk_tx, response.first_text()) {
                tx.send(StreamDelta::Content(text.to_string()))
                    .await
                    .expect("embedded test stream receiver");
            }
            Ok(response)
        }
    }

    struct CanaryCredentialProvider;

    #[async_trait]
    impl CredentialProvider for CanaryCredentialProvider {
        async fn credential_for(
            &self,
            _provider: &str,
        ) -> std::result::Result<ProviderCredential, ProviderCredentialError> {
            ProviderCredential::new(SECRET_CANARY)
        }
    }

    async fn collect_to_eof(mut stream: EmbeddedTurnStream) -> Vec<TurnStreamEnvelopeV2> {
        let mut events = Vec::new();
        loop {
            let next = tokio::time::timeout(Duration::from_secs(15), stream.recv())
                .await
                .expect("embedded turn stream timed out")
                .expect("embedded turn stream failed");
            match next {
                Some(event) => events.push(event),
                None => return events,
            }
        }
    }

    async fn wait_until(mut predicate: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !predicate() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("embedded daemon state transition timed out");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn embedded_daemon_owns_turn_lifecycle_persistence_and_replay() {
        let sandbox = tempfile::tempdir().expect("embedded daemon sandbox");
        let installation_id = InstallationId::parse(INSTALLATION_ID).expect("installation id");

        let credentialed_config = EmbeddedDaemonConfig::credentialed(
            sandbox.path(),
            installation_id.clone(),
            "openai",
            "gpt-5.4-mini",
            None,
            Arc::new(CanaryCredentialProvider),
        )
        .expect("credentialed embedded config");
        let debug = format!("{credentialed_config:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(SECRET_CANARY));
        drop(credentialed_config);

        let chat = Arc::new(LifecycleChatClient::default());
        let daemon = EmbeddedDaemon::boot(EmbeddedDaemonConfig::with_chat_client(
            sandbox.path(),
            installation_id.clone(),
            "openai",
            "embedded-test-model",
            chat.clone(),
        ))
        .await
        .expect("boot embedded daemon");
        let authority_id = daemon.authority_id().clone();
        let node_id = daemon.cluster_node().node_id.clone();
        let client = daemon.local_client();

        assert_eq!(client.principal().kind(), PrincipalKind::LocalApp);
        assert_eq!(client.principal().transport(), TransportClass::Loopback);
        assert!(
            client
                .principal()
                .capabilities()
                .contains(Capability::AdminExecute)
        );

        let session = client.create_session().expect("create daemon session");
        assert_eq!(session.authority_id, authority_id);
        let accepted = client
            .start_turn(&session.session_id, "prove the mobile deployment boundary")
            .await
            .expect("start foreground turn");
        assert!(accepted.turn_id.starts_with("daemon-turn-"));
        assert_eq!(
            accepted.stream_url,
            format!("{EMBEDDED_STREAM_SCHEME}/{}/stream", accepted.turn_id)
        );

        let events = collect_to_eof(
            client
                .subscribe_turn(&accepted.turn_id, 0)
                .await
                .expect("subscribe foreground turn"),
        )
        .await;
        assert!(!events.is_empty());
        assert!(events.iter().enumerate().all(|(index, event)| {
            event.turn_id == accepted.turn_id && event.seq == index as u64 + 1
        }));
        let final_text = format!("{FIRST_REPLY}\n\n{SECOND_REPLY}");
        assert!(
            matches!(
                events.last().map(|event| &event.event),
                Some(TurnStreamEventV2::Final { text, tool_names })
                    if text == &final_text && tool_names.is_empty()
            ),
            "unexpected foreground events: {events:#?}"
        );

        let replay = client
            .replay_turn(&accepted.turn_id, 0)
            .await
            .expect("replay committed foreground turn");
        assert_eq!(
            serde_json::to_value(&replay).expect("serialize replay"),
            serde_json::to_value(&events).expect("serialize live events")
        );

        let transcript = client
            .load_transcript_entries(&session.session_id)
            .expect("load daemon transcript");
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript[0].entry_seq, 1);
        assert_eq!(transcript[0].turn.role, "user");
        assert_eq!(transcript[1].entry_seq, 2);
        assert_eq!(transcript[1].turn.role, "assistant");
        assert_eq!(transcript[1].turn.content, final_text);
        let user_execution = transcript[0]
            .caused_by
            .as_ref()
            .expect("user execution ref");
        let assistant_execution = transcript[1]
            .caused_by
            .as_ref()
            .expect("assistant execution ref");
        assert_eq!(user_execution, assistant_execution);
        assert_eq!(user_execution.authority_id, authority_id);
        assert_eq!(user_execution.session_id.as_str(), session.session_id);
        assert!(
            client
                .list_sessions(10)
                .expect("list daemon sessions")
                .iter()
                .any(|summary| summary.session_id == session.session_id && summary.turns == 2)
        );

        let cancelled = client
            .start_turn(&session.session_id, "this turn should be suspended")
            .await
            .expect("start cancellable foreground turn");
        wait_until(|| chat.calls() >= 3).await;
        assert_eq!(daemon.live_turn_count(), 1);
        assert_eq!(daemon.suspend(), 1);
        assert!(
            client
                .start_turn(&session.session_id, "suspended work must fail closed")
                .await
                .expect_err("suspended daemon accepted work")
                .to_string()
                .contains("suspended")
        );
        let cancelled_events = collect_to_eof(
            client
                .subscribe_turn(&cancelled.turn_id, 0)
                .await
                .expect("subscribe cancelled turn"),
        )
        .await;
        assert!(matches!(
            cancelled_events.last().map(|event| &event.event),
            Some(TurnStreamEventV2::Error {
                operator_message,
                debug_message: None,
            }) if operator_message == "foreground turn cancelled"
        ));
        wait_until(|| daemon.live_turn_count() == 0).await;
        assert!(
            !client
                .active_turn(&session.session_id)
                .await
                .expect("read cancelled turn state")
                .active
        );
        daemon.resume().await.expect("resume embedded daemon");

        drop(client);
        crate::session_store::reset_session_store_for_test();
        assert_eq!(Arc::strong_count(&daemon), 1);
        drop(daemon);
        tokio::time::sleep(Duration::from_millis(250)).await;

        let rebooted = EmbeddedDaemon::boot(EmbeddedDaemonConfig::with_chat_client(
            sandbox.path(),
            installation_id,
            "openai",
            "embedded-test-model",
            chat,
        ))
        .await
        .expect("reboot embedded daemon from its sandbox");
        assert_eq!(rebooted.authority_id(), &authority_id);
        assert_eq!(rebooted.cluster_node().node_id, node_id);
        let rebooted_client = rebooted.local_client();
        let rebooted_transcript = rebooted_client
            .load_transcript_entries(&session.session_id)
            .expect("reload transcript after reboot");
        assert_eq!(rebooted_transcript.len(), 3);
        assert_eq!(rebooted_transcript[2].turn.role, "user");
        let rebooted_replay = rebooted_client
            .replay_turn(&accepted.turn_id, 0)
            .await
            .expect("reload journal after reboot");
        assert_eq!(
            serde_json::to_value(rebooted_replay).expect("serialize reboot replay"),
            serde_json::to_value(events).expect("serialize original events")
        );
    }
}
