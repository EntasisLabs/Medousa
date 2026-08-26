//! In-process deployment of the Medousa daemon for native mobile hosts.
//!
//! This module is a composition root, not a second runtime. It binds the
//! existing daemon authority, Stasis control plane, session store, turn owner,
//! ticket registry, durable journal, and production foreground loop to a
//! trusted co-located client.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use genai::chat::ChatMessage;
use medousa_engine::{TurnPipelineHandle, TurnStreamRegistryPort};
use medousa_runtime::{
    CredentialedAiChatClient, CredentialedAiChatConfig, DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS,
    MAX_REQUEST_PROMPT_CHARS, MedousaToolLoopPipeline, ModelResponseCompleted,
    ModelResponseEventPort, RuntimePortFuture, RuntimePorts, ToolLoopCompletionGate,
    ToolRunEventPort, ToolRunFinish, ToolRunStart, TurnPresentationPort,
};
use medousa_types::daemon_api::{
    AgentModeId, AgentModeSource, CancelActiveSessionTurnResponse, ContinuationStatusResponse,
    CreateSessionResponse, CreateUserProfileResponse, DaemonStatsResponse, DeliveryHealthResponse,
    GraphemeModuleDetailResponse, GraphemeModuleOpsResponse, GraphemeModulesListResponse,
    GraphemeRunResponse, GraphemeScriptDetailResponse, GraphemeScriptsListQuery,
    GraphemeScriptsListResponse, HealthResponse, InteractiveTurnResponse, ListUserProfilesResponse,
    LocusNodeDetailResponse, LocusNodesListResponse, LocusNodesQuery, LocusTagsListResponse,
    LocusTagsQuery, RecurringDefinitionEntry, RecurringListResponse, RegisterRecurringResponse,
    SessionAgentModeResponse, SessionCodeBindingResponse, SetActiveUserProfileResponse,
    VaultBacklinksResponse, VaultChangesQuery, VaultChangesResponse, VaultDeleteResponse,
    VaultFileContentResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultNotesQuery,
    VaultRootsResponse, VaultSearchQuery, VaultSearchResponse, VaultTagsListResponse,
    VaultTagsQuery, VaultTrashListResponse, VaultTrashRestoreResponse, VaultWriteRequest,
    VaultWriteResponse,
};
use medousa_types::environment::{
    CustomViewComponentStatus, CustomViewSurfaceStatus, EnvironmentPendingResponse,
    EnvironmentSpecPutRequest, EnvironmentSpecResponse, EnvironmentStatusResponse, SurfaceKind,
};
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::secrets::InstallationId;
use medousa_types::session::{ConversationTurn, SessionHistorySummary, TranscriptEntry};
#[cfg(test)]
use medousa_types::turn_stream::TurnStreamEventV2;
use medousa_types::turn_stream::{
    TurnCompletionOutcomeV3, TurnStreamEnvelopeV2, TurnStreamEnvelopeV3, TurnStreamEventV3,
};
use medousa_types::turn_ticket::{TurnTicket, TurnTicketMode, TurnTicketPhase};
use medousa_types::{
    GraphemeAllowlistResponse, GraphemeAllowlistUpdateRequest, GraphemeCompileRequest,
    GraphemeCompileResponse, GraphemeLifecycleResponse, GraphemeModuleLoadRequest,
    GraphemeModuleLoadResponse, GraphemeScriptDeleteResponse, GraphemeScriptSaveRequest,
    GraphemeScriptSaveResponse,
};
use serde_json::{Value, json};
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolLoopExecutionRequest,
};
use stasis::application::orchestration::tool_registry::{StasisTool, ToolRegistry};
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
use stasis::prelude::{RuntimeBackend, RuntimeComposition, RuntimeFactory, RuntimeSdk};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
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
const EMBEDDED_RUNTIME_EVENT_CAPACITY: usize = 64;
const EMBEDDED_TOOL_PARAM_LIMIT: usize = 6;
const EMBEDDED_TOOL_VALUE_CHARS: usize = 120;

fn embedded_system_prompt() -> String {
    static PROMPT: OnceLock<String> = OnceLock::new();
    PROMPT
        .get_or_init(|| {
            let policy = crate::prompt_policy::compile_sttp_policy(
                crate::prompt_policy::SttpPolicySelection::new(
                    crate::prompt_policy::SttpPolicyMode::General,
                    crate::prompt_policy::SttpPolicyActor::Host,
                ),
            )
            .expect("built-in embedded STTP policy must compile")
            .rendered;
            format!(
                "{policy}\n\n[MEDOUSA_HUD]\nsurface=personal_mobile\n\
                 catalog_tool=cognition_tools_discover\n\
                 web_tool=cognition_web_search"
            )
        })
        .clone()
}

fn embedded_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "credential",
        "authorization",
        "api_key",
    ]
    .iter()
    .any(|marker| key.contains(marker))
}

fn embedded_bounded_text(value: &str) -> (String, bool) {
    let mut chars = value.chars();
    let text = chars
        .by_ref()
        .take(EMBEDDED_TOOL_VALUE_CHARS)
        .collect::<String>();
    let truncated = chars.next().is_some();
    (text, truncated)
}

fn embedded_tool_value(key: &str, value: &Value) -> (String, bool) {
    if embedded_sensitive_key(key) {
        return ("[redacted]".to_string(), false);
    }
    match value {
        Value::String(value) => embedded_bounded_text(value),
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            embedded_bounded_text(&value.to_string())
        }
        Value::Array(values) => (format!("[{} items]", values.len()), false),
        Value::Object(values) => (format!("{{{} fields}}", values.len()), false),
    }
}

fn embedded_tool_input_params(input: &Value) -> Vec<medousa_types::daemon_api::ToolInputParam> {
    let Some(object) = input.as_object() else {
        return Vec::new();
    };
    object
        .iter()
        .take(EMBEDDED_TOOL_PARAM_LIMIT)
        .map(|(key, value)| {
            let (value, truncated) = embedded_tool_value(key, value);
            medousa_types::daemon_api::ToolInputParam {
                key: key.clone(),
                value,
                truncated,
            }
        })
        .collect()
}

fn embedded_tool_input_summary(tool_name: &str, input: &Value) -> String {
    for key in ["query", "task", "prompt", "action", "intent", "path", "url"] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return embedded_bounded_text(trimmed).0;
            }
        }
    }
    tool_name.to_string()
}

fn embedded_tool_output_summary(_output: &Value) -> Option<String> {
    // Tool outputs remain model evidence. The embedded presentation path does
    // not surface arbitrary payload values without the full receipt/redaction
    // services used by a server daemon.
    None
}

fn embedded_tool_status(output: &Value) -> &'static str {
    if output
        .get("ok")
        .and_then(Value::as_bool)
        .is_some_and(|ok| !ok)
        || output.get("error").is_some()
    {
        "failed"
    } else {
        "succeeded"
    }
}

#[derive(Debug)]
struct EmbeddedActiveTextSegment {
    segment_id: String,
    model_round: usize,
    markdown: String,
}

#[derive(Debug)]
struct EmbeddedTextState {
    model_round: usize,
    next_ordinal: usize,
    active: Option<EmbeddedActiveTextSegment>,
    committed_markdown: Vec<String>,
}

impl Default for EmbeddedTextState {
    fn default() -> Self {
        Self {
            model_round: 1,
            next_ordinal: 0,
            active: None,
            committed_markdown: Vec::new(),
        }
    }
}

struct EmbeddedChronologicalTurn {
    turn_id: String,
    pipeline: TurnPipelineHandle,
    parts: std::sync::Mutex<crate::turn_parts::TurnPartsAccumulator>,
    text: std::sync::Mutex<EmbeddedTextState>,
}

impl EmbeddedChronologicalTurn {
    fn new(turn_id: &str, pipeline: TurnPipelineHandle) -> Self {
        Self {
            turn_id: turn_id.to_string(),
            pipeline,
            parts: std::sync::Mutex::new(crate::turn_parts::TurnPartsAccumulator::default()),
            text: std::sync::Mutex::new(EmbeddedTextState::default()),
        }
    }

    async fn publish(
        &self,
        event: TurnStreamEventV3,
    ) -> Result<(), medousa_engine::TurnPipelineError> {
        if matches!(
            event,
            TurnStreamEventV3::ContentAppend { .. } | TurnStreamEventV3::ReasoningAppend { .. }
        ) {
            self.pipeline.admit_v3(event).await
        } else {
            self.pipeline.emit_v3(event).await.map(|_| ())
        }
    }

    async fn content_delta(&self, text: String) -> Result<(), medousa_engine::TurnPipelineError> {
        if text.is_empty() {
            return Ok(());
        }
        let (started, append) = {
            let mut state = self
                .text
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let started = if state.active.is_none() {
                state.next_ordinal = state.next_ordinal.saturating_add(1);
                let segment_id = format!("{}:text:{}", self.turn_id, state.next_ordinal);
                let model_round = state.model_round;
                state.active = Some(EmbeddedActiveTextSegment {
                    segment_id: segment_id.clone(),
                    model_round,
                    markdown: String::new(),
                });
                Some(TurnStreamEventV3::AssistantTextStarted {
                    segment_id,
                    model_round,
                })
            } else {
                None
            };
            let active = state.active.as_mut().expect("active text initialized");
            active.markdown.push_str(&text);
            let append = TurnStreamEventV3::ContentAppend {
                segment_id: active.segment_id.clone(),
                text,
            };
            (started, append)
        };
        if let Some(started) = started {
            self.publish(started).await?;
        }
        self.publish(append).await
    }

    async fn reasoning_delta(&self, text: String) -> Result<(), medousa_engine::TurnPipelineError> {
        if text.is_empty() {
            return Ok(());
        }
        if let Ok(mut parts) = self.parts.lock() {
            parts.push_reasoning_delta(&text);
        }
        self.publish(TurnStreamEventV3::ReasoningAppend { text })
            .await
    }

    async fn commit_active(
        &self,
        advance_model_round: bool,
    ) -> Result<(), medousa_engine::TurnPipelineError> {
        let committed = {
            let mut state = self
                .text
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let active = state.active.take();
            if advance_model_round {
                state.model_round = state.model_round.saturating_add(1);
            }
            active
                .filter(|segment| !segment.markdown.is_empty())
                .inspect(|segment| {
                    state.committed_markdown.push(segment.markdown.clone());
                })
        };
        let Some(committed) = committed else {
            return Ok(());
        };
        if let Ok(mut parts) = self.parts.lock() {
            parts.commit_text_segment(
                &committed.markdown,
                Some(&committed.segment_id),
                Some(committed.model_round),
            );
        }
        self.publish(TurnStreamEventV3::AssistantTextCommitted {
            segment_id: committed.segment_id,
        })
        .await
    }

    async fn tool_started(
        &self,
        tool_run_id: String,
        event: ToolRunStart,
    ) -> Result<(), medousa_engine::TurnPipelineError> {
        self.commit_active(false).await?;
        let input_summary = embedded_tool_input_summary(&event.tool_name, &event.tool_input);
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_started(
                &tool_run_id,
                &event.tool_name,
                &input_summary,
                event.tool_round,
            );
        }
        self.publish(TurnStreamEventV3::ToolStarted {
            tool_run_id,
            tool_name: event.tool_name,
            input_summary,
            input_params: embedded_tool_input_params(&event.tool_input),
            tool_round: event.tool_round,
        })
        .await
    }

    async fn tool_finished(
        &self,
        event: ToolRunFinish,
    ) -> Result<(), medousa_engine::TurnPipelineError> {
        let invocation = event.invocation;
        let input_summary =
            embedded_tool_input_summary(&invocation.tool_name, &invocation.tool_input);
        let output_summary = embedded_tool_output_summary(&invocation.tool_output);
        let status = embedded_tool_status(&invocation.tool_output).to_string();
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_finished(
                &event.tool_run_id,
                &status,
                output_summary.clone(),
                Vec::new(),
            );
        }
        self.publish(TurnStreamEventV3::ToolFinished {
            tool_run_id: event.tool_run_id,
            tool_name: invocation.tool_name,
            status,
            input_summary,
            input_params: embedded_tool_input_params(&invocation.tool_input),
            output_summary,
            tool_round: event.tool_round,
            artifact_refs: Vec::new(),
        })
        .await
    }

    async fn progress(
        &self,
        message: String,
        tool_names: Vec<String>,
    ) -> Result<(), medousa_engine::TurnPipelineError> {
        if let Ok(mut parts) = self.parts.lock() {
            parts.archive_progress_note(&message);
        }
        self.publish(TurnStreamEventV3::Progress {
            message,
            tool_names,
        })
        .await
    }

    fn aggregate_text(&self) -> String {
        self.text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .committed_markdown
            .join("\n\n")
    }

    async fn terminal_body(
        &self,
        fallback: &str,
    ) -> Result<String, medousa_engine::TurnPipelineError> {
        self.commit_active(false).await?;
        if self.aggregate_text().trim().is_empty() && !fallback.trim().is_empty() {
            self.content_delta(fallback.to_string()).await?;
            self.commit_active(false).await?;
        }
        Ok(self.aggregate_text())
    }

    fn set_model_receipt(&self, provider: &str, model: &str) {
        if let Ok(mut parts) = self.parts.lock() {
            parts.set_model_receipt(provider, model);
        }
    }

    fn partial_tool_names(&self) -> Vec<String> {
        self.parts
            .lock()
            .map(|parts| {
                parts
                    .preview_tool_runs()
                    .into_iter()
                    .filter_map(|part| match part {
                        medousa_types::turn::TurnPart::ToolRun { tool_name, .. } => Some(tool_name),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn has_partial_timeline(&self) -> bool {
        !self.aggregate_text().trim().is_empty() || !self.partial_tool_names().is_empty()
    }

    fn finalize_turn(
        &self,
        body: String,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) -> ConversationTurn {
        match self.parts.lock() {
            Ok(mut parts) => parts.finalize_chronological_turn(body, tool_names, answer_state),
            Err(_) => {
                ConversationTurn::plain("assistant", body, Utc::now(), tool_names, answer_state)
            }
        }
    }
}

enum EmbeddedRuntimeEvent {
    ModelResponseCompleted {
        event: ModelResponseCompleted,
        response_text: Option<String>,
        acknowledged: oneshot::Sender<()>,
    },
    ToolStarted {
        tool_run_id: String,
        event: ToolRunStart,
    },
    ToolFinished(ToolRunFinish),
    Notice(String),
    Progress {
        message: String,
        tool_names: Vec<String>,
    },
}

#[derive(Clone)]
struct EmbeddedModelResponseEvents {
    tx: mpsc::Sender<EmbeddedRuntimeEvent>,
}

impl ModelResponseEventPort for EmbeddedModelResponseEvents {
    fn completed(&self, event: ModelResponseCompleted) -> RuntimePortFuture<()> {
        self.completed_with_text(event, None)
    }

    fn completed_with_text(
        &self,
        event: ModelResponseCompleted,
        response_text: Option<String>,
    ) -> RuntimePortFuture<()> {
        let tx = self.tx.clone();
        Box::pin(async move {
            let (acknowledged, wait) = oneshot::channel();
            if tx
                .send(EmbeddedRuntimeEvent::ModelResponseCompleted {
                    event,
                    response_text,
                    acknowledged,
                })
                .await
                .is_ok()
            {
                let _ = wait.await;
            }
        })
    }
}

#[derive(Clone)]
struct EmbeddedToolRunEvents {
    tx: mpsc::Sender<EmbeddedRuntimeEvent>,
}

impl ToolRunEventPort for EmbeddedToolRunEvents {
    fn started(&self, event: ToolRunStart) -> RuntimePortFuture<String> {
        let tx = self.tx.clone();
        Box::pin(async move {
            let tool_run_id = format!("tr-{}", Uuid::new_v4().simple());
            let _ = tx
                .send(EmbeddedRuntimeEvent::ToolStarted {
                    tool_run_id: tool_run_id.clone(),
                    event,
                })
                .await;
            tool_run_id
        })
    }

    fn finished(&self, event: ToolRunFinish) -> RuntimePortFuture<()> {
        let tx = self.tx.clone();
        Box::pin(async move {
            let _ = tx.send(EmbeddedRuntimeEvent::ToolFinished(event)).await;
        })
    }
}

#[derive(Clone)]
struct EmbeddedTurnPresentation {
    tx: mpsc::Sender<EmbeddedRuntimeEvent>,
}

impl TurnPresentationPort for EmbeddedTurnPresentation {
    fn notice(&self, message: String) -> RuntimePortFuture<()> {
        let tx = self.tx.clone();
        Box::pin(async move {
            let _ = tx.send(EmbeddedRuntimeEvent::Notice(message)).await;
        })
    }

    fn turn_progress(
        &self,
        _stream_turn_id: u64,
        message: String,
        tool_names: Vec<String>,
    ) -> RuntimePortFuture<()> {
        let tx = self.tx.clone();
        Box::pin(async move {
            let _ = tx
                .send(EmbeddedRuntimeEvent::Progress {
                    message,
                    tool_names,
                })
                .await;
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedSuspendReport {
    pub cancellation_requested: usize,
    pub remaining_turns: usize,
    pub timed_out: bool,
}

/// Services made available to a deployment's tool-registry recipe.
///
/// These are outbound ports and shared runtime services, not a deployment
/// identity. A mobile, desktop, browser, or test host may select any recipe
/// compatible with the services it can provide.
#[derive(Clone)]
pub struct EmbeddedToolRegistryBindings {
    pub runtime: Arc<RuntimeComposition>,
    pub locus: crate::locus_service::LocusService,
    pub locus_store: Arc<dyn locus_core_rs::NodeStore>,
    pub semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations: Arc<dyn MemoryOperations>,
}

/// An unfinished registry assembled by a deployment recipe.
///
/// The runtime adds its FSM tools before finalizing the one exact catalog.
pub struct EmbeddedToolRegistryAssembly {
    registrar: crate::typed_tools::ToolRegistrar,
    catalog_handles: Vec<crate::typed_tools::ToolCatalogHandle>,
}

impl EmbeddedToolRegistryAssembly {
    pub fn new(placements: crate::typed_tools::ToolPlacementIndex) -> Self {
        Self {
            registrar: crate::typed_tools::ToolRegistrar::new(placements),
            catalog_handles: Vec::new(),
        }
    }

    pub fn registrar(&mut self) -> &mut crate::typed_tools::ToolRegistrar {
        &mut self.registrar
    }

    pub fn initialize_handle_after_finish(
        &mut self,
        handle: crate::typed_tools::ToolCatalogHandle,
    ) {
        self.catalog_handles.push(handle);
    }

    fn finish(
        mut self,
    ) -> StasisResult<(Arc<dyn ToolRegistry>, Arc<crate::typed_tools::ToolCatalog>)> {
        use crate::typed_tools::ToolRegistration as _;

        self.registrar.register_tool(EmbeddedTurnControlTool)?;
        let (registry, catalog) = self.registrar.finish();
        for handle in self.catalog_handles {
            handle
                .initialize(catalog.clone())
                .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        }
        Ok((Arc::new(registry), catalog))
    }
}

/// Composition port for selecting business tools without forking runtime logic.
pub trait EmbeddedToolRegistryRecipe: Send + Sync {
    fn assemble(
        &self,
        bindings: EmbeddedToolRegistryBindings,
    ) -> StasisResult<EmbeddedToolRegistryAssembly>;
}

struct EmptyEmbeddedToolRegistryRecipe;

impl EmbeddedToolRegistryRecipe for EmptyEmbeddedToolRegistryRecipe {
    fn assemble(
        &self,
        _bindings: EmbeddedToolRegistryBindings,
    ) -> StasisResult<EmbeddedToolRegistryAssembly> {
        Ok(EmbeddedToolRegistryAssembly::new(
            crate::typed_tools::ToolPlacementIndex::default(),
        ))
    }
}

struct EmbeddedTurnControlTool;

#[async_trait::async_trait]
impl StasisTool for EmbeddedTurnControlTool {
    fn name(&self) -> &'static str {
        medousa_runtime::turn_control::COGNITION_TURN
    }

    fn description(&self) -> Option<&'static str> {
        Some("Control this foreground turn: update the user, request input, checkpoint, or finish.")
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
                        "turn.request_input",
                        "turn.checkpoint",
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
            Some("turn.finish") => Ok(json!({ "ok": true })),
            Some("turn.update_user" | "turn.request_input" | "turn.checkpoint")
                if input
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|message| !message.trim().is_empty()) =>
            {
                Ok(json!({ "ok": true }))
            }
            Some("turn.update_user" | "turn.request_input" | "turn.checkpoint") => Err(
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
    tool_registry_recipe: Arc<dyn EmbeddedToolRegistryRecipe>,
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
            tool_registry_recipe: Arc::new(EmptyEmbeddedToolRegistryRecipe),
            foreground_turn_timeout: DEFAULT_FOREGROUND_TURN_TIMEOUT,
            max_live_turns: 1,
        }
    }

    /// Supply the deployment recipe that assembles business tools from the
    /// runtime's outbound services.
    pub fn with_tool_registry_recipe(
        mut self,
        recipe: Arc<dyn EmbeddedToolRegistryRecipe>,
    ) -> Self {
        self.tool_registry_recipe = recipe;
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
            .field("tool_registry", &"deployment-recipe")
            .field("foreground_turn_timeout", &self.foreground_turn_timeout)
            .field("max_live_turns", &self.max_live_turns)
            .finish()
    }
}

/// One in-process deployment of `medousa_daemon`.
pub struct EmbeddedDaemon {
    root: PathBuf,
    environment_hub: crate::environment_store::EnvironmentHub,
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
        crate::capability_catalog::configure_capabilities_manifest_path(
            root.join("capabilities.toml"),
        )
        .map_err(|error| anyhow!(error))?;
        crate::grapheme_script::configure_grapheme_script_root(root.join("grapheme-scripts"))
            .map_err(|error| anyhow!(error))?;
        let sandbox_files = crate::store_root::StoreRoot::open(&root)
            .context("open embedded daemon root capability")?;
        let environment_path = crate::store_root::StorePath::parse("environment")
            .context("validate embedded environment path")?;
        let environment_files = Arc::new(
            sandbox_files
                .open_or_create_subroot(&environment_path)
                .context("derive embedded environment capability")?,
        );
        let environment_hub =
            crate::environment_store::EnvironmentHub::new_with_store(environment_files);
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
        let tool_assembly = config
            .tool_registry_recipe
            .assemble(EmbeddedToolRegistryBindings {
                runtime: runtime.clone(),
                locus: locus_service.clone(),
                locus_store: locus_memory.node_store.clone(),
                semantic_index: locus_memory.semantic_index.clone(),
                memory_reader: memory_reader.clone(),
                memory_writer: memory_writer.clone(),
                memory_operations: memory_operations.clone(),
            })
            .context("assemble deployment tool registry")?;
        let (tool_registry, _tool_catalog) = tool_assembly
            .finish()
            .context("finalize runtime tool catalog")?;
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
            environment_hub,
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
        inference_prompt: String,
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
        let chronological = EmbeddedChronologicalTurn::new(&turn_id, pipeline.clone());

        if chronological
            .publish(TurnStreamEventV3::Status {
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
        let provider = context.route().provider().to_string();
        let model = context.route().model().to_string();
        chronological.set_model_receipt(&provider, &model);
        let _ = chronological
            .publish(TurnStreamEventV3::ModelReceipt { provider, model })
            .await;

        let execution_ref =
            match crate::workshop_authority::execution_ref(session_id.as_str(), &turn_id) {
                Ok(value) => value,
                Err(error) => {
                    self.finish_with_error(
                        &turn_id,
                        &chronological,
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
                &chronological,
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
            user_prompt: inference_prompt,
            system_prompt: Some(embedded_system_prompt()),
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
        let (runtime_tx, mut runtime_rx) = mpsc::channel(EMBEDDED_RUNTIME_EVENT_CAPACITY);
        let runtime_ports = RuntimePorts::new()
            .with_model_response_events(Arc::new(EmbeddedModelResponseEvents {
                tx: runtime_tx.clone(),
            }))
            .with_tool_run_events(Arc::new(EmbeddedToolRunEvents {
                tx: runtime_tx.clone(),
            }))
            .with_turn_presentation(Arc::new(EmbeddedTurnPresentation {
                tx: runtime_tx.clone(),
            }));
        let mut completion_gate = ToolLoopCompletionGate::new_for_execution(
            0,
            runtime_ports,
            DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS,
        );
        let outcome = {
            let execution = with_turn_execution_context(
                context.clone(),
                tool_loop.execute_with_stream_prior_messages_max_rounds(
                    request,
                    prior_messages,
                    Some(&delta_tx),
                    DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS,
                    Some(&mut completion_gate),
                    None,
                ),
            );
            tokio::pin!(execution);

            loop {
                tokio::select! {
                    biased;
                    () = context.cancellation().cancelled() => break ForegroundOutcome::Cancelled,
                    delta = delta_rx.recv() => {
                        let Some(delta) = delta else { continue; };
                        if let Err(error) = emit_provider_delta(&chronological, delta).await {
                            break ForegroundOutcome::Failed(error.to_string());
                        }
                    }
                    runtime_event = runtime_rx.recv() => {
                        let Some(runtime_event) = runtime_event else { continue; };
                        if let Err(error) = emit_embedded_runtime_event(&chronological, runtime_event).await {
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
                                termination_reason: response.termination_reason,
                            },
                            Err(error) => ForegroundOutcome::Failed(error.to_string()),
                        };
                    }
                }
            }
        };
        drop(delta_tx);
        while let Ok(delta) = delta_rx.try_recv() {
            if emit_provider_delta(&chronological, delta).await.is_err() {
                break;
            }
        }
        drop(runtime_tx);
        while let Ok(runtime_event) = runtime_rx.try_recv() {
            if emit_embedded_runtime_event(&chronological, runtime_event)
                .await
                .is_err()
            {
                break;
            }
        }

        match outcome {
            ForegroundOutcome::Completed {
                text,
                tool_names,
                termination_reason,
            } => {
                let completion_outcome = embedded_completion_outcome(&termination_reason);
                let answer_state = embedded_answer_state(completion_outcome);
                let body = match chronological.terminal_body(&text).await {
                    Ok(body) => body,
                    Err(error) => {
                        self.finish_with_error(
                            &turn_id,
                            &chronological,
                            "could not finalize the assistant response",
                            &error.to_string(),
                        )
                        .await;
                        self.turn_stream_port.mark_stream_closed(&turn_id).await;
                        drop(lease);
                        return;
                    }
                };
                let assistant_turn = chronological.finalize_turn(
                    body.clone(),
                    tool_names.clone(),
                    answer_state.map(str::to_string),
                );
                match self
                    .session_store
                    .append_transcript_batch(
                        &session_id,
                        &[TranscriptAppend::native(
                            assistant_turn,
                            Some(execution_ref.clone()),
                        )],
                    )
                    .await
                {
                    Ok(_) => {
                        if chronological
                            .publish(TurnStreamEventV3::TurnCompleted {
                                outcome: completion_outcome,
                                aggregate_text: body,
                                tool_names,
                                operator_message: None,
                                debug_message: None,
                            })
                            .await
                            .is_ok()
                        {
                            note_stream_event(
                                &self.turn_tickets,
                                &turn_id,
                                "turn_completed",
                                embedded_ticket_phase(completion_outcome),
                                true,
                            )
                            .await;
                        }
                    }
                    Err(error) => {
                        self.finish_with_error(
                            &turn_id,
                            &chronological,
                            "could not persist the assistant turn",
                            &error.to_string(),
                        )
                        .await;
                    }
                }
            }
            ForegroundOutcome::Cancelled => {
                let _ = chronological.commit_active(false).await;
                let _ = self
                    .persist_partial_timeline(
                        &session_id,
                        &execution_ref,
                        &chronological,
                        "cancelled",
                    )
                    .await;
                let _ = chronological
                    .publish(TurnStreamEventV3::Error {
                        operator_message: "foreground turn cancelled".to_string(),
                        debug_message: None,
                    })
                    .await;
                let _ = chronological
                    .publish(TurnStreamEventV3::TurnCompleted {
                        outcome: TurnCompletionOutcomeV3::Cancelled,
                        aggregate_text: chronological.aggregate_text(),
                        tool_names: Vec::new(),
                        operator_message: Some("foreground turn cancelled".to_string()),
                        debug_message: None,
                    })
                    .await;
                mark_cancelled(&self.turn_tickets, &turn_id).await;
            }
            ForegroundOutcome::Failed(error) => {
                let _ = chronological.commit_active(false).await;
                let _ = self
                    .persist_partial_timeline(&session_id, &execution_ref, &chronological, "failed")
                    .await;
                self.finish_with_error(&turn_id, &chronological, "foreground turn failed", &error)
                    .await;
            }
        }

        self.turn_stream_port.mark_stream_closed(&turn_id).await;
        drop(lease);
    }

    async fn persist_partial_timeline(
        &self,
        session_id: &SessionId,
        execution_ref: &medousa_types::session::ExecutionRef,
        chronological: &EmbeddedChronologicalTurn,
        answer_state: &str,
    ) -> Result<()> {
        if !chronological.has_partial_timeline() {
            return Ok(());
        }
        let body = chronological
            .terminal_body("")
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        let tool_names = chronological.partial_tool_names();
        let turn = chronological.finalize_turn(body, tool_names, Some(answer_state.to_string()));
        self.session_store
            .append_transcript_batch(
                session_id,
                &[TranscriptAppend::native(turn, Some(execution_ref.clone()))],
            )
            .await
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(())
    }

    async fn finish_with_error(
        &self,
        turn_id: &str,
        chronological: &EmbeddedChronologicalTurn,
        operator_message: &str,
        debug_message: &str,
    ) {
        tracing::warn!(turn_id, error = %debug_message, "{operator_message}");
        let _ = chronological.commit_active(false).await;
        let _ = chronological
            .publish(TurnStreamEventV3::Error {
                operator_message: operator_message.to_string(),
                debug_message: Some(debug_message.to_string()),
            })
            .await;
        let _ = chronological
            .publish(TurnStreamEventV3::TurnCompleted {
                outcome: TurnCompletionOutcomeV3::Failed,
                aggregate_text: chronological.aggregate_text(),
                tool_names: Vec::new(),
                operator_message: Some(operator_message.to_string()),
                debug_message: Some(debug_message.to_string()),
            })
            .await;
        note_stream_event(&self.turn_tickets, turn_id, "turn_completed", "error", true).await;
    }
}

enum ForegroundOutcome {
    Completed {
        text: String,
        tool_names: Vec<String>,
        termination_reason: String,
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

    pub async fn environment_spec(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentSpecResponse> {
        self.require(Capability::AdminRuntime)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id);
        let record = self.daemon.environment_hub.get(&profile_id).await?;
        Ok(EnvironmentSpecResponse {
            spec: record.spec,
            revision: record.revision,
        })
    }

    pub async fn put_environment_spec(
        &self,
        request: EnvironmentSpecPutRequest,
    ) -> Result<EnvironmentSpecResponse> {
        self.require(Capability::AdminRuntime)?;
        let errors = validate_environment_spec(&request.spec);
        if !errors.is_empty() {
            bail!(errors.join("; "));
        }
        let record = self
            .daemon
            .environment_hub
            .put(request.spec, "user")
            .await?;
        Ok(EnvironmentSpecResponse {
            spec: record.spec,
            revision: record.revision,
        })
    }

    pub async fn environment_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentPendingResponse> {
        self.require(Capability::AdminRuntime)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id);
        Ok(EnvironmentPendingResponse {
            pending: self.daemon.environment_hub.pending(&profile_id).await,
        })
    }

    pub async fn apply_environment_pending(
        &self,
        profile_id: Option<&str>,
    ) -> Result<EnvironmentSpecResponse> {
        self.require(Capability::AdminRuntime)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id);
        let record = self
            .daemon
            .environment_hub
            .apply_pending(&profile_id)
            .await?;
        Ok(EnvironmentSpecResponse {
            spec: record.spec,
            revision: record.revision,
        })
    }

    pub async fn dismiss_environment_pending(&self, profile_id: Option<&str>) -> Result<()> {
        self.require(Capability::AdminRuntime)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id);
        self.daemon.environment_hub.clear_pending(&profile_id).await;
        Ok(())
    }

    pub async fn environment_status(
        &self,
        profile_id: Option<&str>,
        surface_id: Option<&str>,
    ) -> Result<EnvironmentStatusResponse> {
        self.require(Capability::AdminRuntime)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id);
        let record = self.daemon.environment_hub.get(&profile_id).await?;
        let visible_surface_ids = record
            .spec
            .layout_presets
            .as_ref()
            .and_then(|presets| {
                presets.iter().find(|preset| preset.active).or_else(|| {
                    record
                        .spec
                        .active_preset_id
                        .as_deref()
                        .and_then(|id| presets.iter().find(|preset| preset.id == id))
                })
            })
            .map(|preset| preset.surfaces.as_slice())
            .unwrap_or_default();
        let surface_filter = surface_id.map(str::trim).filter(|id| !id.is_empty());
        let mut nav_orphan_count = 0;
        let custom_surfaces = record
            .spec
            .surfaces
            .iter()
            .filter(|surface| surface.kind == SurfaceKind::Custom)
            .filter(|surface| surface_filter.is_none_or(|id| surface.id == id))
            .map(|surface| {
                let nav_visible = visible_surface_ids.contains(&surface.id);
                if !nav_visible {
                    nav_orphan_count += 1;
                }
                let components = record
                    .spec
                    .components
                    .iter()
                    .filter(|component| component.surface_id == surface.id)
                    .map(|component| CustomViewComponentStatus {
                        component_id: component.id.clone(),
                        artifact_id: None,
                        feeds: component.feeds.clone(),
                        runtime: None,
                    })
                    .collect::<Vec<_>>();
                let mut subscribed_feed_ids = components
                    .iter()
                    .flat_map(|component| component.feeds.iter().cloned())
                    .collect::<Vec<_>>();
                subscribed_feed_ids.sort();
                subscribed_feed_ids.dedup();
                CustomViewSurfaceStatus {
                    surface_id: surface.id.clone(),
                    label: surface.label.clone(),
                    nav_visible,
                    components,
                    subscribed_feed_ids,
                    feed_status: Vec::new(),
                    feed_mismatches: Vec::new(),
                    recurring_bindings: Vec::new(),
                    layout_root: surface.layout_root.clone(),
                }
            })
            .collect();
        let pending_proposal = self
            .daemon
            .environment_hub
            .pending(&profile_id)
            .await
            .is_some();
        Ok(EnvironmentStatusResponse {
            profile_id,
            revision: record.revision,
            active_preset_id: record.spec.active_preset_id,
            pending_proposal,
            custom_surfaces,
            feed_mismatch_count: 0,
            nav_orphan_count,
            hints: Vec::new(),
        })
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

    pub async fn runtime_stats(&self) -> Result<DaemonStatsResponse> {
        self.require(Capability::WorkshopRead)?;
        let snapshot = RuntimeSdk::new(self.daemon.runtime.as_ref().clone())
            .stats_snapshot(5000)
            .await
            .map_err(anyhow::Error::new)?;
        Ok(crate::daemon_runtime::stats_response(
            snapshot,
            crate::daemon_runtime::DaemonStatsObservation {
                last_tick_at_utc: None,
                active_turn_executions: self.daemon.executions.live_count(),
                active_turn_executions_high_water: self.daemon.executions.high_water(),
                missing_turn_context_invocations:
                    crate::execution_context::missing_turn_context_invocations(),
            },
        ))
    }

    pub async fn runtime_delivery_status(&self) -> Result<DeliveryHealthResponse> {
        self.require(Capability::WorkshopRead)?;
        let pending_job_deliveries = RuntimeSdk::new(self.daemon.runtime.as_ref().clone())
            .pending_outbox_count(5000)
            .await
            .map_err(anyhow::Error::new)?;
        Ok(DeliveryHealthResponse {
            endpoint_id: "medousa.internal.outbox".to_string(),
            endpoint_seeded: false,
            endpoint_target: String::new(),
            deliver_webhook_auth_configured: false,
            pending_job_deliveries,
            last_delivery_at_utc: None,
            last_delivery_latency_ms: None,
        })
    }

    pub fn runtime_continuation_status(&self) -> Result<ContinuationStatusResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(ContinuationStatusResponse {
            pending_count: 0,
            consumed_count: 0,
            resumed_count: 0,
            dead_letter_pending_count: 0,
            total_count: 0,
            last_resume_at_utc: None,
            last_resume_child_job_id: None,
            last_resume_turn_correlation_id: None,
        })
    }

    pub fn grapheme_list_modules(&self) -> Result<GraphemeModulesListResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(crate::grapheme_api::list_modules())
    }

    pub fn grapheme_get_module(&self, module_id: &str) -> Result<GraphemeModuleDetailResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::grapheme_api::get_module(module_id).map_err(anyhow::Error::msg)
    }

    pub fn grapheme_get_module_ops(
        &self,
        module_id: &str,
        query: Option<&str>,
    ) -> Result<GraphemeModuleOpsResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(crate::grapheme_api::get_module_ops(module_id, query))
    }

    pub fn grapheme_list_scripts(
        &self,
        query: GraphemeScriptsListQuery,
    ) -> Result<GraphemeScriptsListResponse> {
        self.require(Capability::ContentRead)?;
        Ok(crate::grapheme_api::list_scripts(query))
    }

    pub fn grapheme_get_script(&self, script_id: &str) -> Result<GraphemeScriptDetailResponse> {
        self.require(Capability::ContentRead)?;
        crate::grapheme_api::get_script(script_id).map_err(anyhow::Error::msg)
    }

    pub async fn grapheme_run_source(&self, source: &str) -> Result<GraphemeRunResponse> {
        self.require(Capability::AdminExecute)?;
        crate::grapheme_api::run_source(&self.daemon.runtime, source)
            .await
            .map_err(anyhow::Error::msg)
    }

    pub async fn grapheme_get_allowlist(&self) -> Result<GraphemeAllowlistResponse> {
        self.require(Capability::AdminRuntime)?;
        Ok(crate::grapheme_workshop::get_allowlist().await)
    }

    pub async fn grapheme_update_allowlist(
        &self,
        request: GraphemeAllowlistUpdateRequest,
    ) -> Result<GraphemeAllowlistResponse> {
        self.require(Capability::AdminRuntime)?;
        crate::grapheme_workshop::update_allowlist(request)
            .await
            .map_err(anyhow::Error::msg)
    }

    pub fn grapheme_save_script(
        &self,
        request: GraphemeScriptSaveRequest,
    ) -> Result<GraphemeScriptSaveResponse> {
        self.require(Capability::ContentWrite)?;
        crate::grapheme_workshop::save_script(request).map_err(anyhow::Error::msg)
    }

    pub fn grapheme_delete_script(&self, script_id: &str) -> Result<GraphemeScriptDeleteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::grapheme_workshop::delete_script(script_id).map_err(anyhow::Error::msg)
    }

    pub fn grapheme_rename_script(
        &self,
        script_id: &str,
        name: &str,
    ) -> Result<GraphemeScriptSaveResponse> {
        self.require(Capability::ContentWrite)?;
        crate::grapheme_workshop::rename_script(script_id, name).map_err(anyhow::Error::msg)
    }

    pub async fn grapheme_compile_source(
        &self,
        request: GraphemeCompileRequest,
    ) -> Result<GraphemeCompileResponse> {
        self.require(Capability::AdminExecute)?;
        crate::grapheme_workshop::compile_source(request)
            .await
            .map_err(anyhow::Error::msg)
    }

    pub async fn grapheme_load_module(
        &self,
        request: GraphemeModuleLoadRequest,
    ) -> Result<GraphemeModuleLoadResponse> {
        self.require(Capability::AdminRuntime)?;
        crate::grapheme_workshop::load_wasm_module(request)
            .await
            .map_err(anyhow::Error::msg)
    }

    pub async fn grapheme_lifecycle(&self) -> Result<GraphemeLifecycleResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(crate::grapheme_workshop::lifecycle_events().await)
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
        self.start_turn_with_presentation_context(
            session_id,
            prompt,
            identity_user_id,
            channel_surface,
            None,
            None,
        )
        .await
    }

    pub async fn start_turn_with_presentation_context(
        &self,
        session_id: &str,
        prompt: impl Into<String>,
        identity_user_id: Option<String>,
        channel_surface: Option<String>,
        voice_preset_id: Option<String>,
        voice_appendix: Option<String>,
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
        let inference_prompt = medousa_runtime::append_voice_preset_hint(
            &prompt,
            voice_preset_id.as_deref(),
            voice_appendix.as_deref(),
        );
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
                .execute_foreground_turn(lease, prompt, inference_prompt, prior_messages, stream)
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
            .filter_map(|event| {
                match crate::sse_turn_projection::sequenced_to_v2_optional(&event) {
                    Ok(Some(envelope)) => Some(Ok(envelope)),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                }
            })
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
            .filter_map(|event| {
                match crate::sse_turn_projection::sequenced_to_v2_optional(&event) {
                    Ok(Some(envelope)) => Some(Ok(envelope)),
                    Ok(None) => None,
                    Err(error) => Some(Err(anyhow::Error::msg(error))),
                }
            })
            .collect()
    }

    pub async fn subscribe_turn_v3(
        &self,
        turn_id: &str,
        since: u64,
    ) -> Result<EmbeddedTurnStreamV3> {
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
            .filter_map(|event| crate::sse_turn_projection::sequenced_to_v3(&event).ok())
            .collect();
        Ok(EmbeddedTurnStreamV3 {
            replay,
            live,
            last_seq: since,
        })
    }

    pub async fn replay_turn_v3(
        &self,
        turn_id: &str,
        since: u64,
    ) -> Result<Vec<TurnStreamEnvelopeV3>> {
        self.require(Capability::ContentRead)?;
        self.daemon.ensure_turn_stream(turn_id).await?;
        let log = turn_stream_log(&self.daemon.turn_streams, turn_id)
            .await
            .ok_or_else(|| anyhow!("turn journal is unavailable"))?;
        Ok(log
            .snapshot_since(since)
            .into_iter()
            .filter_map(|event| crate::sse_turn_projection::sequenced_to_v3(&event).ok())
            .collect())
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

/// Replay-first native V3 stream for the co-located Medousa client.
pub struct EmbeddedTurnStreamV3 {
    replay: VecDeque<TurnStreamEnvelopeV3>,
    live: TurnEventSubscription,
    last_seq: u64,
}

impl EmbeddedTurnStreamV3 {
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub async fn recv(&mut self) -> Result<Option<TurnStreamEnvelopeV3>> {
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
                    self.last_seq = event.seq();
                    let Some(envelope) = event.v3 else {
                        continue;
                    };
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
                    let event_seq = event.seq();
                    let envelope = match event.v2 {
                        Some(envelope) => envelope,
                        None => {
                            self.last_seq = event_seq;
                            let Some(v1) = event.v1 else {
                                continue;
                            };
                            crate::sse_turn_projection::v1_to_v2(&v1).map_err(anyhow::Error::msg)?
                        }
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
    chronological: &EmbeddedChronologicalTurn,
    delta: StreamDelta,
) -> Result<(), medousa_engine::TurnPipelineError> {
    match delta {
        StreamDelta::Content(text) => chronological.content_delta(text).await?,
        StreamDelta::Reasoning(text) => chronological.reasoning_delta(text).await?,
        StreamDelta::ThoughtSignature(_) => {}
    }
    Ok(())
}

async fn emit_embedded_runtime_event(
    chronological: &EmbeddedChronologicalTurn,
    event: EmbeddedRuntimeEvent,
) -> Result<(), medousa_engine::TurnPipelineError> {
    match event {
        EmbeddedRuntimeEvent::ModelResponseCompleted {
            event,
            response_text,
            acknowledged,
        } => {
            let needs_fallback = chronological
                .text
                .lock()
                .map(|state| {
                    state
                        .active
                        .as_ref()
                        .is_none_or(|segment| segment.markdown.is_empty())
                })
                .unwrap_or(false);
            if needs_fallback
                && let Some(text) = response_text.filter(|text| !text.trim().is_empty())
            {
                chronological.content_delta(text).await?;
            }
            let result = chronological.commit_active(true).await;
            let _ = acknowledged.send(());
            debug_assert!(event.model_round > 0);
            result
        }
        EmbeddedRuntimeEvent::ToolStarted { tool_run_id, event } => {
            chronological.tool_started(tool_run_id, event).await
        }
        EmbeddedRuntimeEvent::ToolFinished(event) => chronological.tool_finished(event).await,
        EmbeddedRuntimeEvent::Notice(message) => {
            chronological
                .publish(TurnStreamEventV3::Status {
                    phase: "orchestration".to_string(),
                    operator_message: None,
                    debug_message: Some(message),
                })
                .await
        }
        EmbeddedRuntimeEvent::Progress {
            message,
            tool_names,
        } => chronological.progress(message, tool_names).await,
    }
}

fn embedded_completion_outcome(termination_reason: &str) -> TurnCompletionOutcomeV3 {
    match termination_reason {
        "cognition_turn_checkpoint" => TurnCompletionOutcomeV3::Checkpointed,
        medousa_runtime::TOOL_ROUND_BUDGET_EXHAUSTED_REASON | "stuck_text_only_continue" => {
            TurnCompletionOutcomeV3::FuseExhausted
        }
        _ => TurnCompletionOutcomeV3::Completed,
    }
}

fn embedded_answer_state(outcome: TurnCompletionOutcomeV3) -> Option<&'static str> {
    match outcome {
        TurnCompletionOutcomeV3::Checkpointed => Some("checkpoint"),
        TurnCompletionOutcomeV3::NeedsInput => Some("needs_input"),
        TurnCompletionOutcomeV3::FuseExhausted => Some("fuse_exhausted"),
        TurnCompletionOutcomeV3::Failed => Some("failed"),
        TurnCompletionOutcomeV3::Cancelled => Some("cancelled"),
        TurnCompletionOutcomeV3::Completed => None,
    }
}

fn embedded_ticket_phase(outcome: TurnCompletionOutcomeV3) -> &'static str {
    match outcome {
        TurnCompletionOutcomeV3::Completed => "done",
        TurnCompletionOutcomeV3::NeedsInput => "awaiting_operator",
        TurnCompletionOutcomeV3::Checkpointed => "handoff",
        TurnCompletionOutcomeV3::Failed
        | TurnCompletionOutcomeV3::Cancelled
        | TurnCompletionOutcomeV3::FuseExhausted => "error",
    }
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
    const GRAPHEME_REPLY: &str = "The portable Grapheme workflow completed on the phone daemon.";
    const GRAPHEME_SOURCE: &str = r#"import core from "grapheme/core"

query MobileProbe {
    core.echo(message: "embedded phase four") {
        state { current }
    }
}
"#;

    #[test]
    fn embedded_prompt_uses_general_sttp_and_tool_hud() {
        let prompt = embedded_system_prompt();
        assert!(prompt.contains("p1_core(.99)"));
        assert!(prompt.contains("p2_mode_general(.99)"));
        assert!(prompt.contains("p3_actor_host(.99)"));
        assert!(prompt.contains("[MEDOUSA_HUD]"));
        assert!(prompt.contains("catalog_tool=cognition_tools_discover"));
        assert!(prompt.contains("web_tool=cognition_web_search"));
    }

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
                1 => pending::<StasisResult<ChatResponse>>().await,
                2 => Ok(tool_response(
                    "cognition_grapheme_run",
                    json!({ "source": GRAPHEME_SOURCE }),
                )),
                3 => Ok(tool_response(
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

    async fn collect_v3_to_eof(mut stream: EmbeddedTurnStreamV3) -> Vec<TurnStreamEnvelopeV3> {
        let mut events = Vec::new();
        while let Some(event) = stream.recv().await.expect("read embedded v3 stream") {
            events.push(event);
        }
        events
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
        let daemon = EmbeddedDaemon::boot(
            EmbeddedDaemonConfig::with_chat_client(
                sandbox.path(),
                installation_id.clone(),
                "openai",
                "embedded-test-model",
                chat.clone(),
            )
            .with_tool_registry_recipe(Arc::new(
                crate::mobile_tool_registry::PersonalMobileToolRegistryRecipe,
            )),
        )
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

        let initial_environment = client
            .environment_spec(None)
            .await
            .expect("read embedded environment");
        let mut edited_environment = initial_environment.spec;
        edited_environment
            .layout_presets
            .as_mut()
            .and_then(|presets| presets.iter_mut().find(|preset| preset.active))
            .expect("active embedded layout")
            .surfaces
            .retain(|surface_id| surface_id != "web");
        let saved_environment = client
            .put_environment_spec(EnvironmentSpecPutRequest {
                spec: edited_environment,
            })
            .await
            .expect("save embedded environment");
        assert!(saved_environment.revision > initial_environment.revision);
        assert!(
            !saved_environment
                .spec
                .layout_presets
                .as_ref()
                .and_then(|presets| presets.iter().find(|preset| preset.active))
                .expect("saved active embedded layout")
                .surfaces
                .iter()
                .any(|surface_id| surface_id == "web")
        );
        assert!(sandbox.path().join("environment").is_dir());

        let runtime_stats = client.runtime_stats().await.expect("read runtime stats");
        assert_eq!(runtime_stats.active_turn_executions, 0);
        assert_eq!(runtime_stats.recurring_definitions, 0);
        assert!(
            !client
                .runtime_delivery_status()
                .await
                .expect("read embedded delivery stats")
                .endpoint_seeded
        );
        assert_eq!(
            client
                .runtime_continuation_status()
                .expect("read embedded continuation stats")
                .total_count,
            0
        );

        let modules = client
            .grapheme_list_modules()
            .expect("list embedded Grapheme modules");
        assert!(
            modules
                .modules
                .iter()
                .any(|module| module.module_id == "core")
        );
        assert!(
            !client
                .grapheme_get_allowlist()
                .await
                .expect("read embedded Grapheme allowlist")
                .enforce
        );
        client
            .grapheme_compile_source(GraphemeCompileRequest {
                source: GRAPHEME_SOURCE.to_string(),
                mode: Some("check".to_string()),
            })
            .await
            .expect("compile Grapheme source through embedded daemon");
        client
            .grapheme_run_source(GRAPHEME_SOURCE)
            .await
            .expect("run Grapheme source through embedded daemon");
        let saved_script = client
            .grapheme_save_script(GraphemeScriptSaveRequest {
                name: "Mobile Probe".to_string(),
                body: GRAPHEME_SOURCE.to_string(),
                id: Some("mobile-probe".to_string()),
                modules: vec!["core".to_string()],
                tags: vec!["mobile".to_string()],
                intent: Some("embedded runtime probe".to_string()),
                source_session_id: None,
            })
            .expect("save embedded Grapheme script");
        assert_eq!(saved_script.script.id, "mobile-probe");
        assert_eq!(
            client
                .grapheme_get_script("mobile-probe")
                .expect("load embedded Grapheme script")
                .body_preview,
            GRAPHEME_SOURCE.trim()
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
            crate::mobile_tool_registry::PERSONAL_MOBILE_TOOL_NAMES
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
        assert!(events.iter().all(|event| event.turn_id == accepted.turn_id));
        assert!(events.windows(2).all(|pair| pair[0].seq < pair[1].seq));
        let final_text = FIRST_REPLY.to_string();
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
        let v3 = collect_v3_to_eof(
            client
                .subscribe_turn_v3(&accepted.turn_id, 0)
                .await
                .expect("subscribe native v3 foreground turn"),
        )
        .await;
        assert!(v3.iter().enumerate().all(|(index, event)| {
            event.turn_id == accepted.turn_id && event.seq == index as u64 + 1
        }));
        assert_eq!(
            v3.iter()
                .filter_map(|event| match &event.event {
                    TurnStreamEventV3::AssistantTextStarted { model_round, .. } => {
                        Some(*model_round)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            v3.iter()
                .filter_map(|event| match &event.event {
                    TurnStreamEventV3::ContentAppend { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![FIRST_REPLY]
        );
        assert!(matches!(
            v3.last().map(|event| &event.event),
            Some(TurnStreamEventV3::TurnCompleted {
                outcome: TurnCompletionOutcomeV3::Completed,
                aggregate_text,
                ..
            }) if aggregate_text == &final_text
        ));

        let transcript = client
            .load_transcript_entries(&session.session_id)
            .expect("load daemon transcript");
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript[0].entry_seq, 1);
        assert_eq!(transcript[0].turn.role, "user");
        assert_eq!(transcript[1].entry_seq, 2);
        assert_eq!(transcript[1].turn.role, "assistant");
        assert_eq!(transcript[1].turn.content, final_text);
        assert_eq!(
            transcript[1]
                .turn
                .parts
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter_map(|part| match part {
                    medousa_types::turn::TurnPart::Text { model_round, .. } => *model_round,
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![1]
        );
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
        wait_until(|| chat.calls() >= 2).await;
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

        let rebooted = EmbeddedDaemon::boot(
            EmbeddedDaemonConfig::with_chat_client(
                sandbox.path(),
                installation_id.clone(),
                "openai",
                "embedded-test-model",
                chat,
            )
            .with_tool_registry_recipe(Arc::new(
                crate::mobile_tool_registry::PersonalMobileToolRegistryRecipe,
            )),
        )
        .await
        .expect("reboot embedded daemon from its sandbox");
        assert_eq!(rebooted.authority_id(), &authority_id);
        assert_eq!(rebooted.cluster_node().node_id, node_id);
        let rebooted_client = rebooted.local_client();
        assert!(
            !rebooted_client
                .environment_spec(None)
                .await
                .expect("reload embedded environment")
                .spec
                .layout_presets
                .as_ref()
                .and_then(|presets| presets.iter().find(|preset| preset.active))
                .expect("reloaded active embedded layout")
                .surfaces
                .iter()
                .any(|surface_id| surface_id == "web")
        );
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
            Some(TurnStreamEventV2::Error { .. })
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
