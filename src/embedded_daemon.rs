//! In-process deployment of the Medousa daemon for native mobile hosts.
//!
//! This module is a composition root, not a second runtime. It binds the
//! existing daemon authority, Stasis control plane, session store, turn owner,
//! ticket registry, durable journal, and production foreground loop to a
//! trusted co-located client.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    CreateUserProfileResponse, HealthResponse, InteractiveTurnResponse, ListUserProfilesResponse,
    LocusNodeDetailResponse, LocusNodesListResponse, LocusNodesQuery, LocusTagsListResponse,
    LocusTagsQuery, RecurringDefinitionEntry, RecurringListResponse, RegisterRecurringResponse,
    SessionAgentModeResponse, SessionCodeBindingResponse, SetActiveUserProfileResponse,
    VaultBacklinksResponse, VaultChangesQuery, VaultChangesResponse, VaultDeleteResponse,
    VaultFileContentResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultNotesQuery,
    VaultRootsResponse, VaultSearchQuery, VaultSearchResponse, VaultTagsListResponse,
    VaultTagsQuery, VaultTrashListResponse, VaultTrashRestoreResponse, VaultWriteRequest,
    VaultWriteResponse,
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
use stasis::infrastructure::memory::locus_memory_operations::LocusMemoryOperations;
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};
use stasis::ports::outbound::memory::memory_context_reader::MemoryContextReader;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use stasis::ports::outbound::runtime::cluster_node_store::ClusterNodeStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory};
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub use medousa_runtime::{CredentialProvider, ProviderCredential, ProviderCredentialError};

use crate::execution_context::{
    ProviderRoute, SurfaceCapabilities, TurnExecutionContext, TurnExecutionRegistry,
    with_turn_execution_context,
};
use crate::persistent_locus::build_persistent_locus_memory;
use crate::request_principal::{Capability, RequestPrincipal, TransportClass};
use crate::runtime_composition_ext::{
    RuntimeCompositionExt, RuntimeRecoveryReport, reconcile_after_unavailability,
};
use crate::session_storage::{SessionId, new_session_id};
use crate::session_store::{
    SessionStore, TranscriptAppend, configure_file_session_root, get_session_store,
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
const EMBEDDED_SUSPEND_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const EMBEDDED_RECOVERY_MAX_JOBS: usize = 32;
const STREAM_DELTA_CAPACITY: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedSuspendReport {
    pub cancellation_requested: usize,
    pub remaining_turns: usize,
    pub timed_out: bool,
}

/// Mobile capability layer over the daemon's injected tool registry.
///
/// The canonical turn-control and portable daemon tools are always daemon-owned
/// and cannot be replaced by a host-supplied registry. Additional admitted
/// tools delegate to the registry selected by the embedded composition.
struct EmbeddedToolRegistry {
    delegate: Arc<dyn ToolRegistry>,
    portable: InMemoryToolRegistry,
    turn_control: InMemoryToolRegistry,
}

impl EmbeddedToolRegistry {
    fn new(delegate: Arc<dyn ToolRegistry>, portable: InMemoryToolRegistry) -> StasisResult<Self> {
        let turn_control = InMemoryToolRegistry::default();
        turn_control.register_tool(EmbeddedTurnControlTool)?;
        Ok(Self {
            delegate,
            portable,
            turn_control,
        })
    }
}

#[async_trait::async_trait]
impl ToolRegistry for EmbeddedToolRegistry {
    async fn list_tools(&self) -> StasisResult<Vec<genai::chat::Tool>> {
        let mut tools = self.portable.list_tools().await?;
        let mut delegated = self.delegate.list_tools().await?;
        delegated.retain(|tool| {
            tool.name.as_str() != medousa_runtime::turn_control::COGNITION_TURN
                && !crate::portable_daemon_tools::PORTABLE_DAEMON_TOOL_NAMES
                    .contains(&tool.name.as_str())
        });
        tools.extend(delegated);
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
        } else if crate::portable_daemon_tools::PORTABLE_DAEMON_TOOL_NAMES
            .contains(&tool_name.trim())
        {
            self.portable.invoke_tool(tool_name, input).await
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
    credentialed_chat_client: Option<CredentialedAiChatClient>,
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
        let credentialed_chat_client = CredentialedAiChatClient::new(ai_config, credentials)
            .context("initialize embedded inference client")?;
        let chat_client: Arc<dyn AiChatClient> = Arc::new(credentialed_chat_client.clone());
        let mut config =
            Self::with_chat_client(root, installation_id, provider, model, chat_client);
        config.credentialed_chat_client = Some(credentialed_chat_client);
        Ok(config)
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
            credentialed_chat_client: None,
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

enum EmbeddedInferenceBinding {
    Credentialed(CredentialedAiChatClient),
    Fixed { provider: Arc<str>, model: Arc<str> },
}

impl EmbeddedInferenceBinding {
    fn route(&self) -> (String, String) {
        match self {
            Self::Credentialed(client) => {
                let config = client.config();
                (config.provider().to_string(), config.model().to_string())
            }
            Self::Fixed { provider, model } => (provider.to_string(), model.to_string()),
        }
    }

    fn reconfigure(
        &self,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<()> {
        let Self::Credentialed(client) = self else {
            bail!("embedded inference binding is not reconfigurable");
        };
        let config = CredentialedAiChatConfig::new(provider, model, base_url)
            .context("invalid embedded inference configuration")?;
        client.reconfigure(config);
        Ok(())
    }
}

/// Validate a route before the native host commits it to workshop settings.
pub fn validate_credentialed_inference_route(
    provider: impl Into<String>,
    model: impl Into<String>,
    base_url: Option<String>,
) -> Result<()> {
    CredentialedAiChatConfig::new(provider, model, base_url)
        .context("invalid embedded inference configuration")?;
    Ok(())
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
    inference: EmbeddedInferenceBinding,
    chat_client: Arc<dyn AiChatClient>,
    tool_registry: Arc<dyn ToolRegistry>,
    session_store: Arc<dyn SessionStore>,
    profile_registry: Arc<std::sync::RwLock<crate::user_profiles::UserProfileRegistry>>,
    locus_service: crate::locus_service::LocusService,
    memory_writer: Arc<dyn MemoryContextWriter>,
    runtime: Arc<RuntimeComposition>,
    _locus_memory: Arc<stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore>,
    cluster_node_store: Arc<dyn ClusterNodeStore>,
    cluster_node: ClusterNode,
    turn_streams: TurnStreamRegistry,
    turn_stream_port: TurnStreamRegistryPortAdapter,
    turn_tickets: TurnTicketRegistry,
    executions: TurnExecutionRegistry,
    foreground_turn_timeout: Duration,
    suspended: AtomicBool,
    lifecycle_epoch: AtomicU64,
    recovery_lock: AsyncMutex<()>,
}

impl EmbeddedDaemon {
    /// Boot the daemon against one app-sandbox root.
    pub async fn boot(config: EmbeddedDaemonConfig) -> Result<Arc<Self>> {
        let root = prepare_root(&config.root).await?;
        configure_file_session_root(root.join("history")).map_err(|error| anyhow!(error))?;
        let sandbox_files = crate::store_root::StoreRoot::open(&root)
            .context("open embedded daemon root capability")?;
        let vault_path =
            crate::store_root::StorePath::parse("vault").context("validate embedded vault path")?;
        let vault_files = Arc::new(
            sandbox_files
                .open_or_create_subroot(&vault_path)
                .context("derive embedded vault capability")?,
        );
        crate::vault::roots::configure_deployment_vault_root(root.join("vault"), vault_files)
            .context("configure embedded vault root")?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, || {
                crate::vault::vault_store().refresh_from_disk()
            })
            .await
            .context("initialize embedded vault")?;
        let profile_registry = Arc::new(std::sync::RwLock::new(
            crate::user_profiles::UserProfileRegistry::load_or_bootstrap_at(
                root.join("user_profiles.json"),
            ),
        ));
        crate::user_profiles::init_workshop_profile_registry(profile_registry.clone());

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
        crate::surreal_startup::ensure_runtime_backend_prerequisites(&backend)
            .context("prepare embedded SurrealKV runtime")?;
        let runtime = RuntimeFactory::build(backend)
            .await
            .context("boot embedded Stasis persistence shell")?;
        crate::stasis_surreal_schema::ensure_stasis_runtime_schema(&runtime)
            .await
            .context("initialize embedded Stasis schema")?;
        crate::session_store::init_session_store_with_runtime(&runtime)
            .await
            .context("initialize embedded session schema")?;

        let (session_store, locus_memory): (
            Arc<dyn SessionStore>,
            Arc<stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore>,
        ) = match &runtime {
            RuntimeComposition::Surreal(runtime) => {
                let locus_memory = build_persistent_locus_memory(runtime.job_store.db())
                    .await
                    .context("initialize embedded Locus memory")?;
                (get_session_store(), locus_memory)
            }
            RuntimeComposition::InMemory(_) => {
                bail!("embedded daemon requires its SurrealKV persistence backend")
            }
        };
        let turn_store = crate::turn_recovery::SessionStoreTurnStore::new(session_store.clone());
        for item in medousa_engine::recover_uncommitted(&turn_log_root) {
            let report =
                crate::turn_recovery::recover_journal_item(&turn_log_root, item, &turn_store)
                    .await
                    .context("recover interrupted embedded turn")?;
            tracing::info!(
                session_id = %report.session_id,
                turn_id = %report.turn_id,
                inserted = report.inserted,
                already_present = report.already_present,
                "reconciled interrupted embedded turn journal"
            );
        }
        let memory_reader: Arc<dyn MemoryContextReader> = Arc::new(
            stasis::infrastructure::memory::locus_context_reader::LocusContextReader::new(
                locus_memory.clone(),
            ),
        );
        let memory_writer: Arc<dyn MemoryContextWriter> =
            Arc::new(crate::locus_memory::MedousaLocusContextWriter::new(
                locus_memory.node_store.clone(),
                crate::locus_memory::resolve_locus_ingest_profile(),
            ));
        let memory_operations: Arc<dyn MemoryOperations> =
            Arc::new(LocusMemoryOperations::new(locus_memory.clone(), None));
        let locus_service = crate::locus_service::LocusService::new(
            locus_memory.node_store.clone(),
            locus_memory.semantic_index.clone(),
            memory_reader.clone(),
        );
        let runtime = Arc::new(runtime);
        let portable_tools = crate::portable_daemon_tools::build_portable_daemon_tool_registry(
            runtime.clone(),
            locus_service.clone(),
            memory_writer.clone(),
        )
        .context("initialize portable daemon tools")?;
        let tool_registry: Arc<dyn ToolRegistry> = Arc::new(
            EmbeddedToolRegistry::new(config.tool_registry.clone(), portable_tools)
                .context("initialize embedded tool capability layer")?,
        );
        let thread_store = RuntimeFactory::resolve_thread_store(runtime.as_ref(), None);
        let cluster_node_store = RuntimeFactory::resolve_cluster_node_store(runtime.as_ref(), None);
        let workflow_engine = RuntimeFactory::default_workflow_engine();
        let memory_reader = Some(memory_reader);
        let memory_writer_for_runtime = Some(memory_writer.clone());
        let memory_operations = Some(memory_operations);
        let identity_store = None;
        match runtime.as_ref() {
            RuntimeComposition::InMemory(runtime) => {
                crate::daemon_runtime_handlers::register_daemon_runtime_handlers(
                    runtime,
                    &config.chat_client,
                    &tool_registry,
                    &workflow_engine,
                    &memory_reader,
                    &memory_writer_for_runtime,
                    &identity_store,
                    &memory_operations,
                    &thread_store,
                    &cluster_node_store,
                )?;
            }
            RuntimeComposition::Surreal(runtime) => {
                crate::daemon_runtime_handlers::register_daemon_runtime_handlers(
                    runtime,
                    &config.chat_client,
                    &tool_registry,
                    &workflow_engine,
                    &memory_reader,
                    &memory_writer_for_runtime,
                    &identity_store,
                    &memory_operations,
                    &thread_store,
                    &cluster_node_store,
                )?;
            }
        }
        let cluster_node = register_or_heartbeat_node(
            cluster_node_store.as_ref(),
            &config.installation_id,
            &authority_id,
        )
        .await?;
        let boot_recovery = reconcile_after_unavailability(
            runtime.as_ref(),
            &format!("{}:boot", cluster_node.node_id),
            &cluster_node.node_id,
            EMBEDDED_RECOVERY_MAX_JOBS,
        )
        .await
        .context("reconcile embedded Stasis work at boot")?;
        if boot_recovery.materialized > 0 || !boot_recovery.processed_job_ids.is_empty() {
            tracing::info!(
                materialized = boot_recovery.materialized,
                processed = boot_recovery.processed_job_ids.len(),
                "reconciled embedded Stasis work at boot"
            );
        }

        let turn_streams = new_turn_stream_registry();
        let turn_stream_port = TurnStreamRegistryPortAdapter::new(turn_streams.clone());
        let local_credential_id: Arc<str> = Arc::from(format!(
            "embedded-home:{}",
            config.installation_id.storage_key().as_str()
        ));
        let inference = match config.credentialed_chat_client {
            Some(client) => EmbeddedInferenceBinding::Credentialed(client),
            None => EmbeddedInferenceBinding::Fixed {
                provider: Arc::from(config.provider),
                model: Arc::from(config.model),
            },
        };

        Ok(Arc::new(Self {
            root,
            authority_id,
            local_credential_id,
            inference,
            chat_client: config.chat_client,
            tool_registry,
            session_store,
            profile_registry,
            locus_service,
            memory_writer,
            runtime,
            _locus_memory: locus_memory,
            cluster_node_store,
            cluster_node,
            turn_streams,
            turn_stream_port,
            turn_tickets: new_registry(),
            executions: TurnExecutionRegistry::new(config.max_live_turns),
            foreground_turn_timeout: config.foreground_turn_timeout,
            suspended: AtomicBool::new(false),
            lifecycle_epoch: AtomicU64::new(0),
            recovery_lock: AsyncMutex::new(()),
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
        self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
        self.executions.cancel_all()
    }

    /// Let cancelled foreground owners publish their terminal event and release
    /// their exact execution leases, without exceeding the host deadline.
    pub async fn drain_suspended(
        &self,
        cancellation_requested: usize,
        timeout: Duration,
    ) -> EmbeddedSuspendReport {
        let idle = self.executions.wait_for_idle(timeout).await;
        let remaining_turns = self.executions.live_count();
        EmbeddedSuspendReport {
            cancellation_requested,
            remaining_turns,
            timed_out: !idle,
        }
    }

    pub async fn suspend_and_drain(&self, timeout: Duration) -> EmbeddedSuspendReport {
        let cancellation_requested = self.suspend();
        self.drain_suspended(cancellation_requested, timeout).await
    }

    /// Re-advertise the same Stasis node and run its canonical durable-work
    /// reconciliation before foreground admission reopens.
    pub async fn resume(&self) -> Result<RuntimeRecoveryReport> {
        let _recovery = self.recovery_lock.lock().await;
        if !self.suspended.load(Ordering::Acquire) {
            return Ok(RuntimeRecoveryReport::default());
        }
        let lifecycle_epoch = self.lifecycle_epoch.load(Ordering::Acquire);
        if !self
            .executions
            .wait_for_idle(EMBEDDED_SUSPEND_DRAIN_TIMEOUT)
            .await
        {
            bail!("embedded foreground turns did not drain before resume");
        }
        if lifecycle_epoch != self.lifecycle_epoch.load(Ordering::Acquire) {
            return Ok(RuntimeRecoveryReport::default());
        }

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
        let report = reconcile_after_unavailability(
            self.runtime.as_ref(),
            &format!("{}:wake", self.cluster_node.node_id),
            &self.cluster_node.node_id,
            EMBEDDED_RECOVERY_MAX_JOBS,
        )
        .await
        .context("reconcile embedded Stasis work after wake")?;
        if lifecycle_epoch == self.lifecycle_epoch.load(Ordering::Acquire) {
            self.suspended.store(false, Ordering::Release);
        }
        Ok(report)
    }

    pub fn live_turn_count(&self) -> usize {
        self.executions.live_count()
    }

    async fn ensure_turn_stream(&self, turn_id: &str) -> Result<TurnStreamEntry> {
        self.ensure_turn_stream_with_session(turn_id, None).await
    }

    async fn ensure_turn_stream_with_session(
        &self,
        turn_id: &str,
        session_id: Option<&str>,
    ) -> Result<TurnStreamEntry> {
        let registered = if self.turn_stream_port.has_stream(turn_id).await {
            true
        } else if let Some(session_id) = session_id {
            self.turn_stream_port
                .register_stream_for_session(turn_id, session_id)
                .await
        } else {
            self.turn_stream_port.register_stream(turn_id).await
        };
        if !registered {
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
                provider: context.route().provider().to_string(),
                model: context.route().model().to_string(),
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
                model_hint: Some(context.route().model().to_string()),
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

    pub fn inference_provider(&self) -> String {
        self.daemon.inference.route().0
    }

    pub fn inference_model(&self) -> String {
        self.daemon.inference.route().1
    }

    pub fn reconfigure_inference(
        &self,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<()> {
        self.require(Capability::AdminRuntime)?;
        self.daemon.inference.reconfigure(provider, model, base_url)
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        self.require(Capability::WorkshopRead)?;
        let tool_registry_count = self
            .daemon
            .tool_registry
            .list_tools()
            .await
            .context("read embedded tool registry")?
            .len();
        let advertised_capabilities = self
            .daemon
            .cluster_node
            .capability_tags
            .iter()
            .cloned()
            .chain(std::iter::once("transport.in-process".to_string()));
        let (active_profile_id, active_profile_display_name) = {
            let registry = self
                .daemon
                .profile_registry
                .read()
                .map_err(|_| anyhow!("profile registry lock poisoned"))?;
            let active_profile_id = registry.active_profile_id().to_string();
            let active_profile_display_name = registry
                .list_profiles()
                .into_iter()
                .find(|profile| profile.profile_id == active_profile_id)
                .map(|profile| profile.display_name)
                .unwrap_or_else(|| "Personal".to_string());
            (active_profile_id, active_profile_display_name)
        };
        Ok(crate::daemon_runtime::health_response(
            self.daemon.authority_id.clone(),
            "embedded",
            advertised_capabilities,
            crate::daemon_runtime::DaemonHealthSnapshot {
                backend: "surreal-kv".to_string(),
                worker_id: self.daemon.cluster_node.node_id.clone(),
                agent_runtime_version: crate::daemon_runtime::AGENT_RUNTIME_VERSION.to_string(),
                tool_registry_count,
                last_agent_turn_latency_ms: None,
                last_agent_turn_at_utc: None,
                active_profile_id,
                active_profile_display_name,
            },
        ))
    }

    pub fn list_profiles(&self) -> Result<ListUserProfilesResponse> {
        self.require(Capability::WorkshopRead)?;
        let registry = self
            .daemon
            .profile_registry
            .read()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?;
        Ok(ListUserProfilesResponse {
            profiles: registry
                .list_profiles()
                .into_iter()
                .map(|profile| profile.to_dto())
                .collect(),
            active_profile_id: registry.active_profile_id().to_string(),
            resolved_user_id: registry.resolve_active_user_id(),
        })
    }

    pub fn create_profile(
        &self,
        slug: &str,
        display_name: &str,
    ) -> Result<CreateUserProfileResponse> {
        self.require(Capability::AdminRuntime)?;
        let mut registry = self
            .daemon
            .profile_registry
            .write()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?;
        let profile = registry.create_profile(slug, display_name)?;
        Ok(CreateUserProfileResponse {
            profile: profile.to_dto(),
            active_profile_id: registry.active_profile_id().to_string(),
            resolved_user_id: registry.resolve_active_user_id(),
        })
    }

    pub fn set_active_profile(&self, profile_id: &str) -> Result<SetActiveUserProfileResponse> {
        self.require(Capability::AdminRuntime)?;
        let mut registry = self
            .daemon
            .profile_registry
            .write()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?;
        let resolved_user_id = registry.set_active_profile(profile_id)?;
        Ok(SetActiveUserProfileResponse {
            active_profile_id: registry.active_profile_id().to_string(),
            resolved_user_id,
        })
    }

    pub async fn list_locus_nodes(&self, query: LocusNodesQuery) -> Result<LocusNodesListResponse> {
        self.require(Capability::ContentRead)?;
        self.daemon
            .locus_service
            .list_nodes(query)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn list_locus_tags(&self, query: LocusTagsQuery) -> Result<LocusTagsListResponse> {
        self.require(Capability::ContentRead)?;
        self.daemon
            .locus_service
            .list_tags(query)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn get_locus_node(&self, sync_key: &str) -> Result<LocusNodeDetailResponse> {
        self.require(Capability::ContentRead)?;
        self.daemon
            .locus_service
            .get_node(sync_key)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn store_memory_context(
        &self,
        session_id: &str,
        raw_node: &str,
    ) -> Result<stasis::ports::outbound::memory::memory_models::MemoryStoreResponse> {
        self.require(Capability::ContentWrite)?;
        let session_id = crate::locus_memory::resolve_workshop_locus_session(session_id);
        self.daemon
            .memory_writer
            .store_context(
                &stasis::ports::outbound::memory::memory_models::MemoryStoreRequest {
                    session_id,
                    raw_node: raw_node.to_string(),
                },
            )
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn list_recurring_schedules(&self) -> Result<RecurringListResponse> {
        self.require(Capability::WorkshopRead)?;
        let recurring = self
            .daemon
            .runtime
            .list_recurring()
            .await
            .map_err(anyhow::Error::new)?
            .into_iter()
            .map(|definition| RecurringDefinitionEntry {
                recurring_id: definition.id,
                queue: definition.queue,
                job_type: definition.job_type.clone(),
                cron_expr: definition.cron_expr,
                timezone: definition.timezone,
                enabled: definition.enabled,
                next_run_at_utc: definition.next_run_at,
                last_run_at_utc: definition.last_run_at,
                manuscript_id: None,
                prompt_excerpt: None,
                display_name: None,
                execution_mode: (definition.job_type == "workflow.grapheme.run")
                    .then_some("grapheme".to_string()),
                delivery_label: None,
                last_run_status: None,
            })
            .collect::<Vec<_>>();
        Ok(RecurringListResponse {
            count: recurring.len(),
            recurring,
        })
    }

    /// Persist a portable Grapheme schedule in Stasis. Mobile lifecycle catch-up
    /// policy is deliberately handled by the daemon resume path, not a host timer.
    pub async fn register_grapheme_schedule(
        &self,
        source: &str,
        cron_expr: &str,
        timezone: &str,
        start_immediately: bool,
    ) -> Result<RegisterRecurringResponse> {
        self.require(Capability::AdminExecute)?;
        if source.trim().is_empty() {
            bail!("grapheme source is required");
        }
        let validation = crate::grapheme_runtime::validate_grapheme_source_for_schedule(
            &self.daemon.runtime,
            source,
        )
        .await
        .map_err(anyhow::Error::new)?;
        if !validation
            .get("validated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            bail!("grapheme source did not pass the Stasis runtime preflight");
        }

        let recurring_id = format!("embedded-grapheme-{}", Uuid::new_v4().simple());
        let definition = crate::recurring_schedule::RecurringScheduleSpec::new(
            recurring_id.clone(),
            "default",
            "workflow.grapheme.run",
            format!("grapheme:inline:{source}"),
            cron_expr,
            timezone,
        )
        .start_immediately(start_immediately)
        .build(Utc::now())
        .map_err(anyhow::Error::new)?;
        let response = RegisterRecurringResponse {
            recurring_id,
            queue: definition.queue.clone(),
            next_run_at_utc: definition.next_run_at,
            cron_expr: definition.cron_expr.clone(),
            timezone: definition.timezone.clone(),
        };
        self.daemon
            .runtime
            .register_recurring(definition)
            .await
            .map_err(anyhow::Error::new)?;
        Ok(response)
    }

    pub async fn list_vault_notes(&self, query: VaultNotesQuery) -> Result<VaultNotesListResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                Ok(crate::vault::VaultService::list_notes_paged(
                    query.prefix.as_deref(),
                    query.limit.unwrap_or(100),
                    query.tags.as_deref(),
                    query.tag_prefix.as_deref(),
                    query.cursor.as_deref(),
                    query.generation,
                ))
            })
            .await
    }

    pub async fn list_vault_changes(
        &self,
        query: VaultChangesQuery,
    ) -> Result<VaultChangesResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                Ok(crate::vault::VaultService::changes_since(
                    query.since_generation,
                    query.cursor.as_deref(),
                    query.limit.unwrap_or(200),
                ))
            })
            .await
    }

    pub async fn list_vault_tags(&self, query: VaultTagsQuery) -> Result<VaultTagsListResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                Ok(crate::vault::VaultService::list_tags(
                    query.prefix.as_deref(),
                    query.limit.unwrap_or(100),
                ))
            })
            .await
    }

    pub async fn get_vault_note(&self, path: String) -> Result<VaultNoteContentResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                crate::vault::VaultService::get_note(&path)
            })
            .await
    }

    pub async fn get_vault_file(&self, path: String) -> Result<VaultFileContentResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                crate::vault::VaultService::read_file(&path)
            })
            .await
    }

    pub async fn save_vault_note(
        &self,
        path: String,
        request: VaultWriteRequest,
        if_match: Option<String>,
    ) -> Result<VaultWriteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Mutation, move || {
                crate::vault::VaultService::write_note(Some(&path), &request, if_match.as_deref())
            })
            .await
    }

    pub async fn create_vault_note(
        &self,
        request: VaultWriteRequest,
    ) -> Result<VaultWriteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Mutation, move || {
                crate::vault::VaultService::create_note(&request)
            })
            .await
    }

    pub async fn delete_vault_note(&self, path: String) -> Result<VaultDeleteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Mutation, move || {
                crate::vault::VaultService::delete_note(&path)
            })
            .await
    }

    pub async fn search_vault(&self, query: VaultSearchQuery) -> Result<VaultSearchResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::SearchRebuild, move || {
                crate::vault::VaultService::search(
                    query.q.as_deref(),
                    query.limit.unwrap_or(20),
                    query.tags.as_deref(),
                )
            })
            .await
    }

    pub async fn vault_backlinks(&self, path: String) -> Result<VaultBacklinksResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                crate::vault::VaultService::backlinks(&path)
            })
            .await
    }

    pub fn list_vault_roots(&self) -> Result<VaultRootsResponse> {
        self.require(Capability::ContentRead)?;
        Ok(crate::vault::roots::list_vault_root_views())
    }

    pub fn set_active_vault_root(&self, root_id: &str) -> Result<VaultRootsResponse> {
        self.require(Capability::AdminRuntime)?;
        crate::vault::roots::set_active_vault_root(root_id)
    }

    pub fn add_vault_root(
        &self,
        label: &str,
        path: &str,
        id: Option<&str>,
    ) -> Result<VaultRootsResponse> {
        self.require(Capability::AdminRuntime)?;
        crate::vault::roots::add_vault_root(label, path, id)
    }

    pub async fn list_vault_trash(&self, limit: usize) -> Result<VaultTrashListResponse> {
        self.require(Capability::ContentRead)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Scan, move || {
                crate::vault::VaultService::list_trash(limit)
            })
            .await
    }

    pub async fn restore_vault_trash(&self, path: String) -> Result<VaultTrashRestoreResponse> {
        self.require(Capability::ContentWrite)?;
        crate::vault::io::vault_io()
            .run_anyhow(crate::vault::io::VaultIoClass::Mutation, move || {
                crate::vault::VaultService::restore_from_trash(&path)
            })
            .await
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
        let identity_user_id = {
            let registry = self
                .daemon
                .profile_registry
                .read()
                .map_err(|_| anyhow!("profile registry lock poisoned"))?;
            match identity_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(requested)
                    if registry
                        .list_profiles()
                        .iter()
                        .any(|profile| profile.profile_id == requested) =>
                {
                    requested.to_string()
                }
                Some(requested) => {
                    bail!(
                        "profile '{requested}' does not belong to embedded workshop authority '{}'",
                        self.daemon.authority_id
                    )
                }
                None => registry.resolve_active_user_id(),
            }
        };

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

        let stream = match self
            .daemon
            .ensure_turn_stream_with_session(&turn_id, Some(session_id.as_str()))
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                mark_cancelled(&self.daemon.turn_tickets, &turn_id).await;
                return Err(error);
            }
        };
        let cancellation = CancellationToken::new();
        let (provider, model) = self.daemon.inference.route();
        let scope = TurnContinuationScope {
            turn_correlation_id: turn_id.clone(),
            session_id: session_id.to_string(),
            identity_user_id: Some(identity_user_id),
            original_prompt: prompt.clone(),
            delivery_target: None,
            provider: provider.clone(),
            model: model.clone(),
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
            ProviderRoute::new(provider, model),
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
    use genai::chat::{ChatOptions, ChatRequest, ChatResponse, MessageContent, ToolCall};
    use stasis::domain::errors::Result as StasisResult;
    use stasis::domain::runtime::job::JobState;

    use super::*;
    use crate::request_principal::PrincipalKind;

    const INSTALLATION_ID: &str = crate::workshop_authority::TEST_INSTALLATION_ID;
    const SECRET_CANARY: &str = "embedded-secret-must-never-escape";
    const FIRST_REPLY: &str = "The embedded daemon owns this foreground turn.";
    const SECOND_REPLY: &str = "The mobile shell remains only its privileged local client.";
    const GRAPHEME_REPLY: &str = "The portable Grapheme workflow completed on the phone daemon.";
    const GRAPHEME_SOURCE: &str = r#"import core from "grapheme/core"

query MobileProbe {
    core.echo(message: "embedded phase four") {
        state { current }
    }
}
"#;

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

    fn tool_response(name: &str, arguments: serde_json::Value) -> ChatResponse {
        let model = ModelIden::from_static(AdapterKind::OpenAI, "embedded-test-model");
        ChatResponse {
            content: MessageContent::from_tool_calls(vec![ToolCall {
                call_id: format!("call-{name}"),
                fn_name: name.to_string(),
                fn_arguments: arguments,
                thought_signatures: None,
            }]),
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
                2 => pending::<StasisResult<ChatResponse>>().await,
                3 => Ok(tool_response(
                    "cognition_grapheme_run",
                    json!({ "source": GRAPHEME_SOURCE }),
                )),
                4 => Ok(tool_response(
                    medousa_runtime::turn_control::COGNITION_TURN,
                    json!({ "action": "turn.finish", "message": GRAPHEME_REPLY }),
                )),
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

    #[test]
    fn credentialed_binding_reconfigures_its_route() {
        let client = CredentialedAiChatClient::new(
            CredentialedAiChatConfig::new("openai", "gpt-5.4-mini", None).expect("initial route"),
            Arc::new(CanaryCredentialProvider),
        )
        .expect("credentialed client");
        let binding = EmbeddedInferenceBinding::Credentialed(client);

        binding
            .reconfigure(
                "openai",
                "gpt-4.1-mini",
                Some("https://gateway.example/v1".to_string()),
            )
            .expect("reconfigure route");

        assert_eq!(
            binding.route(),
            ("openai".to_string(), "gpt-4.1-mini".to_string())
        );
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

    fn assert_tree_excludes(root: &Path, needle: &[u8]) {
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            for entry in std::fs::read_dir(&path).expect("read embedded persistence tree") {
                let entry = entry.expect("embedded persistence entry");
                let file_type = entry.file_type().expect("embedded persistence file type");
                if file_type.is_dir() {
                    pending.push(entry.path());
                } else if file_type.is_file() {
                    let bytes =
                        std::fs::read(entry.path()).expect("read embedded persistence file");
                    assert!(
                        !bytes.windows(needle.len()).any(|window| window == needle),
                        "credential canary escaped into {}",
                        entry.path().display()
                    );
                }
            }
        }
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
        let delivery_endpoints =
            RuntimeFactory::resolve_delivery_endpoint_store(daemon.runtime.as_ref(), None);
        assert!(
            delivery_endpoints
                .get("embedded-schema-probe")
                .await
                .expect("read canonical delivery endpoint table")
                .is_none()
        );
        let authority_id = daemon.authority_id().clone();
        let node_id = daemon.cluster_node().node_id.clone();
        let client = daemon.local_client();

        let health = client.health().await.expect("read embedded daemon health");
        assert_eq!(health.runtime.authority_id, authority_id);
        assert_eq!(
            health.runtime.contract_revision,
            medousa_types::DAEMON_API_CONTRACT_REVISION
        );
        assert_eq!(
            health.runtime.base_schema_revision,
            crate::stasis_surreal_schema::DAEMON_PERSISTENCE_SCHEMA_REVISION
        );
        assert_eq!(health.runtime.deployment_profile, "embedded");
        assert!(
            health
                .runtime
                .advertised_capabilities
                .iter()
                .any(|capability| capability == "transport.in-process")
        );

        assert_eq!(client.principal().kind(), PrincipalKind::LocalApp);
        assert_eq!(client.principal().transport(), TransportClass::Loopback);
        assert!(
            client
                .principal()
                .capabilities()
                .contains(Capability::AdminExecute)
        );

        let initial_profiles = client.list_profiles().expect("list embedded profiles");
        assert_eq!(
            initial_profiles.active_profile_id,
            crate::user_profiles::DEFAULT_USER_ID
        );
        let mobile_profile = client
            .create_profile("mobile", "Mobile")
            .expect("create embedded profile");
        client
            .set_active_profile(&mobile_profile.profile.profile_id)
            .expect("activate embedded profile");
        assert_eq!(
            client
                .health()
                .await
                .expect("read profile-aware health")
                .active_profile_id,
            mobile_profile.profile.profile_id
        );

        let tools = daemon
            .tool_registry
            .list_tools()
            .await
            .expect("list embedded daemon tools");
        let tool_names = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            crate::portable_daemon_tools::PORTABLE_DAEMON_TOOL_NAMES
                .iter()
                .all(|name| tool_names.contains(name))
        );
        assert!(tool_names.iter().all(|name| {
            !["pty", "forge", "coder", "detamu"]
                .iter()
                .any(|blocked| name.contains(blocked))
        }));

        let session = client.create_session().expect("create daemon session");
        assert_eq!(session.authority_id, authority_id);
        assert!(
            client
                .start_turn_with_context(
                    &session.session_id,
                    "foreign profile must fail closed",
                    Some("user:foreign-workshop".to_string()),
                    None,
                )
                .await
                .expect_err("foreign workshop profile admitted")
                .to_string()
                .contains("does not belong")
        );

        let note_path = "phase-four/mobile-note.md";
        let note_body = "# Mobile note\n\nOwned by the embedded Personal workshop.";
        let note = client
            .create_vault_note(VaultWriteRequest {
                path: Some(note_path.to_string()),
                content: note_body.to_string(),
                session_id: Some(session.session_id.clone()),
                semantic_tags: Some(vec!["mobile".to_string(), "phase-four".to_string()]),
                auto_workshop_tags: true,
            })
            .await
            .expect("create embedded vault note");
        assert!(note.created);
        assert!(sandbox.path().join("vault").join(note_path).is_file());
        assert!(
            client
                .get_vault_note(note_path.to_string())
                .await
                .expect("read embedded vault note")
                .content
                .contains(note_body)
        );

        let locus_session =
            crate::locus_memory::resolve_workshop_locus_session(&session.session_id);
        let sttp = crate::locus_memory::CANONICAL_STTP_SCHEMA_EXAMPLE
            .split_once("\n\n")
            .expect("canonical STTP body")
            .1
            .replace("session-abc", &locus_session)
            .replace("parser hardening session", "embedded mobile memory");
        let stored = client
            .store_memory_context(&session.session_id, &sttp)
            .await
            .expect("store embedded Locus memory");
        assert!(
            stored.valid,
            "memory rejection: {:?}",
            stored.validation_error
        );
        let memories = client
            .list_locus_nodes(LocusNodesQuery {
                session_id: Some(locus_session.clone()),
                limit: Some(10),
                ..LocusNodesQuery::default()
            })
            .await
            .expect("list embedded Locus memory");
        assert!(
            memories
                .nodes
                .iter()
                .any(|node| node.context_summary.contains("embedded mobile memory"))
        );
        let accepted = client
            .start_turn(&session.session_id, "prove the mobile deployment boundary")
            .await
            .expect("start foreground turn");
        assert!(accepted.turn_id.starts_with("daemon-turn-"));
        assert_eq!(
            accepted.stream_url,
            format!("{EMBEDDED_STREAM_SCHEME}/{}/stream", accepted.turn_id)
        );
        assert_eq!(
            turn_stream_log(&daemon.turn_streams, &accepted.turn_id)
                .await
                .expect("accepted turn journal")
                .envelope()
                .surface
                .as_ref()
                .and_then(|surface| surface.channel_id.as_deref()),
            Some(session.session_id.as_str())
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
        let suspend_report = daemon.suspend_and_drain(Duration::from_secs(2)).await;
        assert_eq!(suspend_report.cancellation_requested, 1);
        assert_eq!(suspend_report.remaining_turns, 0);
        assert!(!suspend_report.timed_out);
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
        assert_eq!(
            cancelled_events
                .iter()
                .filter(|event| event.event.is_terminal())
                .count(),
            1
        );
        let transcript_after_cancel = client
            .load_transcript_entries(&session.session_id)
            .expect("load transcript after cancellation");
        assert_eq!(transcript_after_cancel.len(), 3);
        assert_eq!(transcript_after_cancel[2].turn.role, "user");
        assert_eq!(
            transcript_after_cancel[2]
                .caused_by
                .as_ref()
                .expect("cancelled user execution")
                .execution_id
                .as_str(),
            cancelled.turn_id
        );
        assert!(transcript_after_cancel.iter().all(|entry| {
            entry.turn.role == "user"
                || entry
                    .caused_by
                    .as_ref()
                    .is_none_or(|execution| execution.execution_id.as_str() != cancelled.turn_id)
        }));
        assert!(
            !client
                .active_turn(&session.session_id)
                .await
                .expect("read cancelled turn state")
                .active
        );
        let wake = daemon.resume().await.expect("resume embedded daemon");
        assert_eq!(wake.materialized, 0);

        let grapheme_turn = client
            .start_turn(&session.session_id, "run the portable Grapheme probe")
            .await
            .expect("start Grapheme-backed foreground turn");
        let grapheme_events = collect_to_eof(
            client
                .subscribe_turn(&grapheme_turn.turn_id, 0)
                .await
                .expect("subscribe Grapheme turn"),
        )
        .await;
        assert!(matches!(
            grapheme_events.last().map(|event| &event.event),
            Some(TurnStreamEventV2::Final { text, tool_names })
                if text == GRAPHEME_REPLY
                    && tool_names.iter().any(|name| name == "cognition_grapheme_run")
        ));

        let schedule = client
            .register_grapheme_schedule(GRAPHEME_SOURCE, "0 0 0 * * * *", "UTC", false)
            .await
            .expect("register embedded Grapheme schedule");
        assert!(
            client
                .list_recurring_schedules()
                .await
                .expect("list embedded schedules")
                .recurring
                .iter()
                .any(|entry| entry.recurring_id == schedule.recurring_id)
        );

        let mut due_definition = daemon
            .runtime
            .list_recurring()
            .await
            .expect("load embedded recurring definition")
            .into_iter()
            .find(|definition| definition.id == schedule.recurring_id)
            .expect("registered embedded recurring definition");
        due_definition.next_run_at = Utc::now() - chrono::Duration::seconds(1);
        daemon
            .runtime
            .save_recurring(due_definition)
            .await
            .expect("make embedded schedule due");

        let idle_suspend = daemon.suspend_and_drain(Duration::from_secs(2)).await;
        assert_eq!(idle_suspend.cancellation_requested, 0);
        assert!(!idle_suspend.timed_out);
        let schedule_wake = daemon
            .resume()
            .await
            .expect("reconcile due schedule after wake");
        assert_eq!(schedule_wake.materialized, 1);
        assert_eq!(schedule_wake.processed_job_ids.len(), 1);
        let resumed_definition = daemon
            .runtime
            .list_recurring()
            .await
            .expect("reload reconciled recurring definition")
            .into_iter()
            .find(|definition| definition.id == schedule.recurring_id)
            .expect("reconciled recurring definition");
        assert!(resumed_definition.last_run_at.is_some());
        assert!(resumed_definition.next_run_at > Utc::now());
        let succeeded_after_wake = daemon
            .runtime
            .list_jobs_by_state(JobState::Succeeded)
            .await
            .expect("list schedule jobs after wake")
            .into_iter()
            .filter(|job| job.correlation_id == schedule.recurring_id)
            .count();
        assert_eq!(succeeded_after_wake, 1);

        let second_idle_suspend = daemon.suspend_and_drain(Duration::from_secs(2)).await;
        assert!(!second_idle_suspend.timed_out);
        let second_wake = daemon.resume().await.expect("repeat wake reconciliation");
        assert_eq!(second_wake.materialized, 0);
        assert!(second_wake.processed_job_ids.is_empty());

        let mut restart_due_definition = daemon
            .runtime
            .list_recurring()
            .await
            .expect("load recurring definition before restart")
            .into_iter()
            .find(|definition| definition.id == schedule.recurring_id)
            .expect("recurring definition before restart");
        restart_due_definition.next_run_at = Utc::now() - chrono::Duration::seconds(1);
        daemon
            .runtime
            .save_recurring(restart_due_definition)
            .await
            .expect("make schedule due before restart");

        const RECOVERED_REPLY: &str = "Recovered exactly once from the interrupted journal.";
        let recovered_turn_id = "daemon-turn-embedded-recovery-canary";
        let recovered_envelope = medousa_engine::TurnEnvelope::new(
            recovered_turn_id,
            medousa_engine::Principal::operator(),
        )
        .with_surface(Some(medousa_engine::TurnSurface {
            channel_surface: Some("mobile".to_string()),
            channel_id: Some(session.session_id.clone()),
            user_id: None,
        }));
        let recovery_log = medousa_engine::TurnEventLog::open_in(
            sandbox.path().join(medousa_engine::TURN_LOG_DIR),
            recovered_envelope,
        )
        .expect("open interrupted turn journal");
        recovery_log
            .append(medousa_engine::TurnEvent::FinalResponse {
                text: RECOVERED_REPLY.to_string(),
                tool_names: Vec::new(),
                parts: Vec::new(),
                committed_at: Utc::now(),
            })
            .expect("append interrupted terminal turn");
        drop(recovery_log);

        drop(client);
        crate::session_store::reset_session_store_for_test();
        assert_eq!(Arc::strong_count(&daemon), 1);
        drop(daemon);
        tokio::time::sleep(Duration::from_millis(250)).await;
        std::fs::write(sandbox.path().join("runtime.surrealkv/LOCK"), b"stale")
            .expect("stale lock fixture");

        let rebooted = EmbeddedDaemon::boot(EmbeddedDaemonConfig::with_chat_client(
            sandbox.path(),
            installation_id.clone(),
            "openai",
            "embedded-test-model",
            chat,
        ))
        .await
        .expect("reboot embedded daemon from its sandbox");
        assert_eq!(rebooted.authority_id(), &authority_id);
        assert_eq!(rebooted.cluster_node().node_id, node_id);
        let rebooted_client = rebooted.local_client();
        assert_eq!(
            rebooted_client
                .list_profiles()
                .expect("reload embedded profiles")
                .active_profile_id,
            mobile_profile.profile.profile_id
        );
        assert!(
            rebooted_client
                .get_vault_note(note_path.to_string())
                .await
                .expect("reload embedded vault note")
                .content
                .contains(note_body)
        );
        assert!(
            rebooted_client
                .list_locus_nodes(LocusNodesQuery {
                    session_id: Some(locus_session),
                    limit: Some(10),
                    ..LocusNodesQuery::default()
                })
                .await
                .expect("reload embedded Locus memory")
                .retrieved
                > 0
        );
        assert!(
            rebooted_client
                .list_recurring_schedules()
                .await
                .expect("reload embedded schedules")
                .recurring
                .iter()
                .any(|entry| entry.recurring_id == schedule.recurring_id)
        );
        let succeeded_after_restart = rebooted
            .runtime
            .list_jobs_by_state(JobState::Succeeded)
            .await
            .expect("list schedule jobs after restart")
            .into_iter()
            .filter(|job| job.correlation_id == schedule.recurring_id)
            .count();
        assert_eq!(succeeded_after_restart, 2);
        let rebooted_transcript = rebooted_client
            .load_transcript_entries(&session.session_id)
            .expect("reload transcript after reboot");
        assert_eq!(rebooted_transcript.len(), 6);
        assert_eq!(rebooted_transcript[2].turn.role, "user");
        assert_eq!(rebooted_transcript[3].turn.role, "user");
        assert_eq!(rebooted_transcript[4].turn.role, "assistant");
        assert_eq!(rebooted_transcript[4].turn.content, GRAPHEME_REPLY);
        assert_eq!(rebooted_transcript[5].turn.role, "assistant");
        assert_eq!(rebooted_transcript[5].turn.content, RECOVERED_REPLY);
        assert_eq!(
            rebooted_transcript[5]
                .caused_by
                .as_ref()
                .expect("recovered execution identity")
                .execution_id
                .as_str(),
            recovered_turn_id
        );
        assert!(
            medousa_engine::recover_uncommitted(sandbox.path().join(medousa_engine::TURN_LOG_DIR))
                .is_empty()
        );
        let rebooted_replay = rebooted_client
            .replay_turn(&accepted.turn_id, 0)
            .await
            .expect("reload journal after reboot");
        assert_eq!(
            serde_json::to_value(rebooted_replay).expect("serialize reboot replay"),
            serde_json::to_value(events).expect("serialize original events")
        );
        let rebooted_cancelled_replay = rebooted_client
            .replay_turn(&cancelled.turn_id, 0)
            .await
            .expect("reload cancelled journal after reboot");
        assert_eq!(
            rebooted_cancelled_replay
                .iter()
                .filter(|event| event.event.is_terminal())
                .count(),
            1
        );

        drop(rebooted_client);
        crate::session_store::reset_session_store_for_test();
        assert_eq!(Arc::strong_count(&rebooted), 1);
        drop(rebooted);
        tokio::time::sleep(Duration::from_millis(250)).await;

        let credentialed_config = EmbeddedDaemonConfig::credentialed(
            sandbox.path(),
            installation_id,
            "openai",
            "embedded-credential-test",
            Some("https://127.0.0.1:9/v1".to_string()),
            Arc::new(CanaryCredentialProvider),
        )
        .expect("credentialed embedded reboot config")
        .with_foreground_turn_timeout(Duration::from_secs(5));
        let credentialed = EmbeddedDaemon::boot(credentialed_config)
            .await
            .expect("credentialed embedded reboot");
        let credentialed_client = credentialed.local_client();
        assert_eq!(
            credentialed_client
                .load_transcript_entries(&session.session_id)
                .expect("transcript before credential probe")
                .len(),
            6,
            "journal recovery must remain idempotent across another boot"
        );
        let credential_probe = credentialed_client
            .start_turn(
                &session.session_id,
                "exercise the Keychain credential boundary",
            )
            .await
            .expect("start credential-bound turn");
        let credential_events = collect_to_eof(
            credentialed_client
                .subscribe_turn(&credential_probe.turn_id, 0)
                .await
                .expect("subscribe credential-bound turn"),
        )
        .await;
        assert!(matches!(
            credential_events.last().map(|event| &event.event),
            Some(TurnStreamEventV2::Error {
                debug_message: None,
                ..
            })
        ));
        assert_eq!(
            credential_events
                .iter()
                .filter(|event| event.event.is_terminal())
                .count(),
            1
        );
        assert!(
            !serde_json::to_vec(&credential_events)
                .expect("serialize credential probe events")
                .windows(SECRET_CANARY.len())
                .any(|window| window == SECRET_CANARY.as_bytes())
        );

        drop(credentialed_client);
        crate::session_store::reset_session_store_for_test();
        assert_eq!(Arc::strong_count(&credentialed), 1);
        drop(credentialed);
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_tree_excludes(sandbox.path(), SECRET_CANARY.as_bytes());
    }
}
