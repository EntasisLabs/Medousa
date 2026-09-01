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
use genai::adapter::AdapterKind;
use genai::chat::{
    BinarySource, ChatMessage, ChatOptions, ChatRequest, ChatResponse, ChatRole, ContentPart,
    MessageContent, StopReason, ToolCall, Usage,
};
use medousa_engine::{TurnPipelineHandle, TurnStreamRegistryPort};
use medousa_runtime::{
    CredentialedAiChatClient, CredentialedAiChatConfig, DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS,
    MAX_REQUEST_PROMPT_CHARS, MedousaToolLoopPipeline, ModelResponseCompleted,
    ModelResponseEventPort, RuntimePortFuture, RuntimePorts, ToolLoopCompletionGate,
    ToolRunEventPort, ToolRunFinish, ToolRunStart, TurnPresentationPort,
};
use medousa_types::component_runtime::{
    ComponentRuntimeEventsQuery, ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult,
};
use medousa_types::component_store::{
    ComponentStoreDeleteResponse, ComponentStoreGetResponse, ComponentStoreListResponse,
    ComponentStoreQuery, ComponentStoreSetRequest, ComponentStoreSetResponse,
};
use medousa_types::daemon_api::{
    AgentModeId, AgentModeListResponse, AgentModeProposalListResponse, AgentModeProposalResponse,
    AgentModeTransitionPolicy, ArtifactCommandRequest, ArtifactCommandResponse,
    ArtifactDeleteRequest, ArtifactDeleteResponse, ArtifactFetchRequest, ArtifactFetchResponse,
    ArtifactListUiRequest, ArtifactListUiResponse, ArtifactRetentionStatusResponse,
    ArtifactWriteRequest, ArtifactWriteResponse, BeginChatGptOAuthResponse,
    CancelActiveSessionTurnResponse, ChatGptModelListResponse, ChatGptOAuthStatusResponse,
    CompleteChatGptOAuthResponse, ContinuationStatusResponse, CreateManuscriptRequest,
    CreateSessionRequest, CreateSessionResponse, CreateUserProfileResponse, DaemonStatsResponse,
    DeleteRecurringResponse, DeliveryHealthResponse, DisconnectChatGptOAuthResponse,
    ExportUserProfileRequest, ExportUserProfileResponse, GraphemeModuleDetailResponse,
    GraphemeModuleOpsResponse, GraphemeModulesListResponse, GraphemeRunResponse,
    GraphemeScriptDetailResponse, GraphemeScriptsListQuery, GraphemeScriptsListResponse,
    HealthResponse, IdentityContextRequest, IdentityDigestPreviewResponse,
    IdentityExportMarkdownRequest, IdentityExportMarkdownResponse, IdentityRememberRequest,
    IdentityRememberResponse, ImportUserProfileRequest, ImportUserProfileResponse,
    InteractiveTurnResponse, ListUserProfilesResponse, LocusNodeDetailResponse,
    LocusNodesListResponse, LocusNodesQuery, LocusTagsListResponse, LocusTagsQuery,
    ManuscriptCatalogQuery, ManuscriptCatalogResponse, ManuscriptDetailResponse,
    ManuscriptImportRequest, ManuscriptImportResponse, MediaUploadResponse,
    RecurringDeliveryResponse, RecurringListQuery, RecurringListResponse, RecurringRunsQuery,
    RecurringRunsResponse, RegisterRecurringPromptRequest, RegisterRecurringResponse,
    SessionAgentModeResponse, SessionCodeBindingResponse, SessionDeleteResponse,
    SessionHistoryListResponse, SessionSetDisplayNameResponse, SetActiveUserProfileResponse,
    SetSessionAgentModeRequest, ToolHistoryListQuery, ToolHistoryListResponse,
    UpdateArtifactRetentionRequest, UpdateArtifactRetentionResponse, UpdateManuscriptRequest,
    UpdateRecurringRequest, UpdateRecurringResponse, VaultBacklinksResponse, VaultChangesQuery,
    VaultChangesResponse, VaultDeleteResponse, VaultFileContentResponse, VaultNoteContentResponse,
    VaultNotesListResponse, VaultNotesQuery, VaultRootsResponse, VaultSearchQuery,
    VaultSearchResponse, VaultTagsListResponse, VaultTagsQuery, VaultTrashListResponse,
    VaultTrashRestoreResponse, VaultWriteRequest, VaultWriteResponse, WorkflowDetailResponse,
    WorkflowFromSliceRequest, WorkflowFromSliceResponse, WorkflowPlanRequest, WorkflowPlanResponse,
    WorkflowRunRequest, WorkflowRunResponse, WorkflowRunsQuery, WorkflowRunsResponse,
    WorkflowScheduleRequest, WorkflowScheduleResponse, WorkflowsListQuery, WorkflowsListResponse,
};
use medousa_types::environment::{
    CustomViewComponentStatus, CustomViewSurfaceStatus, EnvironmentPendingResponse,
    EnvironmentSpecPutRequest, EnvironmentSpecResponse, EnvironmentStatusResponse, SurfaceKind,
};
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::feed::{FeedLatestGoodResponse, FeedTailResponse};
use medousa_types::secrets::InstallationId;
use medousa_types::session::{ConversationTurn, SessionHistorySummary, TranscriptEntry};
#[cfg(test)]
use medousa_types::turn_stream::TurnStreamEventV2;
use medousa_types::turn_stream::{
    TurnCompletionOutcomeV3, TurnStreamEnvelopeV2, TurnStreamEnvelopeV3, TurnStreamEventV3,
    WorkerAckKind,
};
use medousa_types::turn_ticket::{TurnTicket, TurnTicketMode, TurnTicketPhase};
use medousa_types::{
    CalendarDeleteResponse, CalendarExportResponse, CalendarImportRequest, CalendarImportResponse,
    CalendarListQuery, CalendarListResponse, CalendarWriteRequest, CalendarWriteResponse,
    CreatePromptStashRequest, DeletePromptStashResponse, DeriveSessionRequest,
    DeriveSessionResponse, GraphemeAllowlistResponse, GraphemeAllowlistUpdateRequest,
    GraphemeCompileRequest, GraphemeCompileResponse, GraphemeLifecycleResponse,
    GraphemeModuleLoadRequest, GraphemeModuleLoadResponse, GraphemeScriptDeleteResponse,
    GraphemeScriptSaveRequest, GraphemeScriptSaveResponse, PromptStash, PromptStashId,
    PromptStashListResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolLoopExecutionRequest,
};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::domain::runtime::cluster_node::{
    ClusterNode, ClusterNodeHeartbeat, ClusterNodeRole, NewClusterNode,
};
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
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

struct EmbeddedMcpPolicyEvaluator;

#[async_trait::async_trait]
impl medousa_mcp_gateway::McpPolicyEvaluator for EmbeddedMcpPolicyEvaluator {
    async fn evaluate(
        &self,
        request: &medousa_types::mcp_gateway_api::McpPolicyEvaluateRequest,
    ) -> anyhow::Result<medousa_types::mcp_gateway_api::McpPolicyEvaluateResponse> {
        Ok(crate::mcp_policy::evaluate_mcp_policy(request))
    }
}

const EMBEDDED_STREAM_SCHEME: &str = "medousa-embedded://turn";
const EMBEDDED_NODE_LEASE_SECONDS: i64 = 300;
const DEFAULT_FOREGROUND_TURN_TIMEOUT: Duration = Duration::from_secs(180);
const EMBEDDED_RECOVERY_MAX_JOBS: usize = 32;
const STREAM_DELTA_CAPACITY: usize = 128;
const EMBEDDED_RUNTIME_EVENT_CAPACITY: usize = 64;
const EMBEDDED_TOOL_PARAM_LIMIT: usize = 6;
const EMBEDDED_TOOL_VALUE_CHARS: usize = 120;

struct EmbeddedDelegationCompletionSink {
    turn_streams: TurnStreamRegistry,
    turn_stream_port: TurnStreamRegistryPortAdapter,
    turn_tickets: TurnTicketRegistry,
}

#[async_trait::async_trait]
impl crate::delegation::DelegationCompletionSink for EmbeddedDelegationCompletionSink {
    async fn deliver(&self, event: crate::delegation::DelegationCompletionEvent) -> Result<()> {
        let Some(stream) = self
            .turn_streams
            .read()
            .await
            .get(&event.source_turn_id)
            .cloned()
        else {
            return Ok(());
        };
        if stream.log.is_committed() {
            return Ok(());
        }
        let pipeline = TurnPipelineHandle::spawn(
            &event.source_turn_id,
            stream.log.replay_fence(),
            daemon_turn_pipeline_budget(),
            Arc::new(TurnJournalOutput::new(
                stream.channel.clone(),
                stream.log.clone(),
            )),
        );
        let chronological = EmbeddedChronologicalTurn::new(&event.source_turn_id, pipeline);
        let outcome = match event.status {
            stasis::domain::agent::turn_wait::TurnWaitStatus::Completed => {
                chronological
                    .publish(TurnStreamEventV3::WorkerSynthesis {
                        text: event.text.clone(),
                        tool_names: event.tool_names.clone(),
                        work_id: Some(event.work_id.clone()),
                    })
                    .await?;
                TurnCompletionOutcomeV3::Completed
            }
            stasis::domain::agent::turn_wait::TurnWaitStatus::Cancelled => {
                chronological
                    .publish(TurnStreamEventV3::Error {
                        operator_message: event.text.clone(),
                        debug_message: None,
                    })
                    .await?;
                TurnCompletionOutcomeV3::Cancelled
            }
            stasis::domain::agent::turn_wait::TurnWaitStatus::Failed
            | stasis::domain::agent::turn_wait::TurnWaitStatus::TimedOut => {
                chronological
                    .publish(TurnStreamEventV3::Error {
                        operator_message: event.text.clone(),
                        debug_message: None,
                    })
                    .await?;
                TurnCompletionOutcomeV3::Failed
            }
            stasis::domain::agent::turn_wait::TurnWaitStatus::Pending => return Ok(()),
        };
        chronological
            .publish(TurnStreamEventV3::TurnCompleted {
                outcome,
                aggregate_text: event.text,
                tool_names: event.tool_names,
                operator_message: None,
                debug_message: None,
            })
            .await?;
        note_stream_event(
            &self.turn_tickets,
            &event.source_turn_id,
            "turn_completed",
            embedded_ticket_phase(outcome),
            true,
        )
        .await;
        self.turn_stream_port
            .mark_stream_closed(&event.source_turn_id)
            .await;
        Ok(())
    }
}

fn embedded_system_prompt(agent_mode: AgentModeId) -> String {
    static PROMPT: OnceLock<String> = OnceLock::new();
    let policy = PROMPT
        .get_or_init(|| {
            crate::prompt_policy::compile_sttp_policy(
                crate::prompt_policy::SttpPolicySelection::new(
                    crate::prompt_policy::SttpPolicyMode::General,
                    crate::prompt_policy::SttpPolicyActor::Host,
                ),
            )
            .expect("built-in embedded STTP policy must compile")
            .rendered
        })
        .clone();
    let hud = if agent_mode == AgentModeId::Instant {
        format!(
            "[MEDOUSA_HUD]\nsurface=personal_mobile\nmode=instant\nweb_tool=cognition_web_search\n{}",
            crate::agent_mode_context::INSTANT_CAPABILITY_CONTEXT
        )
    } else {
        "[MEDOUSA_HUD]\nsurface=personal_mobile\ncatalog_tool=cognition_tools_discover\nweb_tool=cognition_web_search".to_string()
    };
    format!("{policy}\n\n{hud}")
}

#[derive(Clone)]
struct EmbeddedModeToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    allowlist: std::collections::HashSet<String>,
}

impl EmbeddedModeToolRegistry {
    fn instant(inner: Arc<dyn ToolRegistry>) -> Self {
        Self {
            inner,
            allowlist: crate::agent_mode_context::instant_tool_names(),
        }
    }
}

#[async_trait::async_trait]
impl ToolRegistry for EmbeddedModeToolRegistry {
    async fn list_tools(&self) -> StasisResult<Vec<genai::chat::Tool>> {
        Ok(self
            .inner
            .list_tools()
            .await?
            .into_iter()
            .filter(|tool| self.allowlist.contains(tool.name.as_str()))
            .collect())
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> StasisResult<Value> {
        if !self.allowlist.contains(tool_name) {
            return Err(StasisError::PortFailure(format!(
                "tool not loaded in the active agent mode: {tool_name}"
            )));
        }
        self.inner.invoke_tool(tool_name, input).await
    }
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
        let input_params = embedded_tool_input_params(&event.tool_input);
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_started_with_params(
                &tool_run_id,
                &event.tool_name,
                &input_summary,
                input_params.clone(),
                event.tool_round,
            );
        }
        self.publish(TurnStreamEventV3::ToolStarted {
            tool_run_id,
            tool_name: event.tool_name,
            input_summary,
            input_params,
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
        let ui_artifact =
            crate::ui_tool_output::ui_artifact_from_tool_output(&invocation.tool_output);
        let ui_scene = crate::ui_tool_output::scene_ops_from_tool_output(&invocation.tool_output);
        let previous_artifact_id = invocation
            .tool_output
            .get("previous_artifact_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let root_artifact_id = invocation
            .tool_output
            .get("root_artifact_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_finished(
                &event.tool_run_id,
                &status,
                output_summary.clone(),
                Vec::new(),
            );
            if let Some(artifact) = ui_artifact.as_ref() {
                if let Some(previous_artifact_id) = previous_artifact_id.as_deref() {
                    parts.replace_attachment_ref(
                        previous_artifact_id,
                        &artifact.artifact_id,
                        &artifact.mime,
                        &artifact.label,
                        artifact.byte_size,
                        Some(artifact.presentation.clone()),
                        artifact.height_px,
                    );
                } else {
                    parts.push_attachment_ref(
                        &artifact.artifact_id,
                        &artifact.mime,
                        &artifact.label,
                        artifact.byte_size,
                        Some(artifact.presentation.clone()),
                        artifact.height_px,
                    );
                }
            }
        }
        if let Some(artifact) = ui_artifact {
            if let Some(previous_artifact_id) = previous_artifact_id {
                self.publish(TurnStreamEventV3::ArtifactUpdated {
                    previous_artifact_id,
                    artifact,
                    root_artifact_id,
                })
                .await?;
            } else {
                self.publish(TurnStreamEventV3::ArtifactPresented { artifact })
                    .await?;
            }
        }
        if let Some(scene) = ui_scene {
            self.publish(TurnStreamEventV3::UiScene { scene }).await?;
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
        let terminal_text_is_new = self
            .text
            .lock()
            .map(|state| {
                let fallback = fallback.trim();
                !fallback.is_empty()
                    && state.committed_markdown.join("\n\n").trim() != fallback
                    && state
                        .committed_markdown
                        .last()
                        .is_none_or(|last| last.trim() != fallback)
            })
            .unwrap_or(false);
        if terminal_text_is_new {
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

/// Services made available to a deployment's tool-registry recipe.
///
/// These are outbound ports and shared runtime services, not a deployment
/// identity. A mobile, desktop, browser, or test host may select any recipe
/// compatible with the services it can provide.
#[derive(Clone)]
pub struct EmbeddedToolRegistryBindings {
    pub runtime: Arc<RuntimeComposition>,
    pub locus_store: Arc<dyn locus_core_rs::NodeStore>,
    pub semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations: Arc<dyn MemoryOperations>,
    pub identity_store: Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>,
    pub mcp_gateway_client: Arc<crate::mcp_gateway_client::McpGatewayClient>,
    pub provider: String,
    pub model: String,
    pub chat_client: Arc<dyn AiChatClient>,
    pub delegation_service: Option<Arc<crate::delegation::DelegationService>>,
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

    fn finish(self) -> StasisResult<(Arc<dyn ToolRegistry>, Arc<crate::typed_tools::ToolCatalog>)> {
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

/// Provider id used by native, in-process inference implementations.
pub const EMBEDDED_NATIVE_LOCAL_PROVIDER_ID: &str = "medousa-local";

/// Portable request sent across the native inference boundary.
///
/// The agent runtime remains the authority for transcript history and tool
/// execution. Native runtimes only receive model input and return generated
/// text or tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeChatRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<EmbeddedNativeChatMessage>,
    pub tools: Vec<EmbeddedNativeToolSpec>,
    pub options: EmbeddedNativeChatOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<EmbeddedNativeAttachment>,
    #[serde(default)]
    pub tool_calls: Vec<EmbeddedNativeToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeAttachment {
    pub content_type: String,
    pub name: Option<String>,
    pub base64: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeToolSpec {
    pub name: String,
    pub description: Option<String>,
    pub schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeChatOptions {
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedNativeChatResponse {
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<EmbeddedNativeToolCall>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmbeddedNativeInferenceEvent {
    Content { text: String },
    Reasoning { text: String },
}

/// Host-owned transport for a native inference runtime such as MLX on iOS.
#[async_trait::async_trait]
pub trait EmbeddedNativeInference: Send + Sync {
    async fn generate(
        &self,
        request: EmbeddedNativeChatRequest,
        events: Option<mpsc::Sender<EmbeddedNativeInferenceEvent>>,
    ) -> std::result::Result<EmbeddedNativeChatResponse, String>;
}

#[derive(Clone)]
struct EmbeddedNativeChatClient {
    model: String,
    inference: Arc<dyn EmbeddedNativeInference>,
}

impl EmbeddedNativeChatClient {
    fn new(model: String, inference: Arc<dyn EmbeddedNativeInference>) -> Self {
        Self { model, inference }
    }

    fn native_request(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> EmbeddedNativeChatRequest {
        let mut system_parts = request
            .system
            .into_iter()
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>();
        let messages = request
            .messages
            .into_iter()
            .filter_map(|message| {
                let mut content = String::new();
                let mut attachments = Vec::new();
                let mut tool_calls = Vec::new();
                let mut tool_call_id = None;
                for part in message.content {
                    match part {
                        ContentPart::Text(text) => content.push_str(&text),
                        ContentPart::ReasoningContent(text) => content.push_str(&text),
                        ContentPart::Binary(binary) => {
                            let (base64, url) = match binary.source {
                                BinarySource::Base64(value) => (Some(value.to_string()), None),
                                BinarySource::Url(value) => (None, Some(value)),
                            };
                            attachments.push(EmbeddedNativeAttachment {
                                content_type: binary.content_type,
                                name: binary.name,
                                base64,
                                url,
                            });
                        }
                        ContentPart::ToolCall(call) => {
                            tool_calls.push(EmbeddedNativeToolCall {
                                id: call.call_id,
                                name: call.fn_name,
                                arguments: call.fn_arguments,
                            });
                        }
                        ContentPart::ToolResponse(response) => {
                            tool_call_id.get_or_insert(response.call_id);
                            content.push_str(&response.content);
                        }
                        ContentPart::ThoughtSignature(_) | ContentPart::Custom(_) => {}
                    }
                }
                let role = match message.role {
                    ChatRole::System => "system",
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                    ChatRole::Tool => "tool",
                };
                if role == "system"
                    && attachments.is_empty()
                    && tool_calls.is_empty()
                    && tool_call_id.is_none()
                {
                    if !content.trim().is_empty() {
                        system_parts.push(content);
                    }
                    return None;
                }
                Some(EmbeddedNativeChatMessage {
                    role: role.to_string(),
                    content,
                    attachments,
                    tool_calls,
                    tool_call_id,
                })
            })
            .collect();
        let tools = request
            .tools
            .unwrap_or_default()
            .into_iter()
            .map(|tool| EmbeddedNativeToolSpec {
                name: tool.name.as_str().to_string(),
                description: tool.description,
                schema: tool.schema,
            })
            .collect();
        let native_options = options.cloned().unwrap_or_default();
        EmbeddedNativeChatRequest {
            model: self.model.clone(),
            system: (!system_parts.is_empty()).then(|| system_parts.join("\n\n")),
            messages,
            tools,
            options: EmbeddedNativeChatOptions {
                temperature: native_options.temperature,
                max_tokens: native_options.max_tokens,
                top_p: native_options.top_p,
                stop_sequences: native_options.stop_sequences,
                seed: native_options.seed,
            },
        }
    }

    fn chat_response(&self, response: EmbeddedNativeChatResponse) -> ChatResponse {
        let mut parts = Vec::new();
        if !response.content.is_empty() {
            parts.push(ContentPart::Text(response.content));
        }
        let has_tool_calls = !response.tool_calls.is_empty();
        parts.extend(response.tool_calls.into_iter().map(|call| {
            ContentPart::ToolCall(ToolCall {
                call_id: call.id,
                fn_name: call.name,
                fn_arguments: call.arguments,
                thought_signatures: None,
            })
        }));
        let model_iden = genai::ModelIden::new(AdapterKind::Ollama, self.model.clone());
        let prompt_tokens = response.prompt_tokens;
        let completion_tokens = response.completion_tokens;
        ChatResponse {
            content: MessageContent::from_parts(parts),
            reasoning_content: None,
            model_iden: model_iden.clone(),
            provider_model_iden: model_iden,
            stop_reason: response
                .stop_reason
                .map(StopReason::from)
                .or_else(|| has_tool_calls.then(|| StopReason::ToolCall("tool_calls".to_string()))),
            usage: Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens.zip(completion_tokens).map(|(a, b)| a + b),
                ..Usage::default()
            },
            captured_raw_body: None,
            response_id: None,
        }
    }
}

#[async_trait::async_trait]
impl AiChatClient for EmbeddedNativeChatClient {
    async fn complete(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
    ) -> StasisResult<ChatResponse> {
        let request = self.native_request(request, options);
        self.inference
            .generate(request, None)
            .await
            .map(|response| self.chat_response(response))
            .map_err(|error| StasisError::PortFailure(format!("native inference: {error}")))
    }

    async fn complete_stream(
        &self,
        request: ChatRequest,
        options: Option<&ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<ChatResponse> {
        let request = self.native_request(request, options);
        let (event_tx, mut event_rx) = mpsc::channel(64);
        let downstream = chunk_tx.cloned();
        let relay = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let Some(downstream) = downstream.as_ref() else {
                    continue;
                };
                let delta = match event {
                    EmbeddedNativeInferenceEvent::Content { text } => StreamDelta::Content(text),
                    EmbeddedNativeInferenceEvent::Reasoning { text } => {
                        StreamDelta::Reasoning(text)
                    }
                };
                downstream
                    .send(delta)
                    .await
                    .map_err(|_| StasisError::StreamClosed)?;
            }
            Ok::<(), StasisError>(())
        });
        let response = self
            .inference
            .generate(request, chunk_tx.map(|_| event_tx))
            .await;
        relay
            .await
            .map_err(|error| StasisError::PortFailure(format!("native stream relay: {error}")))??;
        response
            .map(|response| self.chat_response(response))
            .map_err(|error| StasisError::PortFailure(format!("native inference: {error}")))
    }
}

/// Deployment configuration assembled by the native host.
///
/// The configuration retains only the host credential-provider boundary;
/// secret material is never retained here or accepted from the UI request.
pub struct EmbeddedDaemonConfig {
    root: PathBuf,
    installation_id: InstallationId,
    provider: String,
    model: String,
    chat_client: Arc<dyn AiChatClient>,
    credential_provider: Option<Arc<dyn CredentialProvider>>,
    tui_defaults: Option<crate::session::TuiDefaults>,
    credentialed_chat_client: Option<CredentialedAiChatClient>,
    routed_chat_client: Option<EmbeddedRoutedChatClient>,
    chatgpt_oauth: Option<Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>>,
    mcp_oauth: Option<Arc<medousa_mcp_gateway::McpOAuthBroker>>,
    tool_registry_recipe: Arc<dyn EmbeddedToolRegistryRecipe>,
    foreground_turn_timeout: Duration,
    max_live_turns: usize,
    delegated_task_transport: Option<Arc<dyn crate::delegated_task::DelegatedTaskTransport>>,
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
        let credentialed_chat_client =
            CredentialedAiChatClient::new(ai_config, credentials.clone())
                .context("initialize embedded inference client")?;
        let chat_client: Arc<dyn AiChatClient> = Arc::new(credentialed_chat_client.clone());
        let mut config =
            Self::with_chat_client(root, installation_id, provider, model, chat_client);
        config.credentialed_chat_client = Some(credentialed_chat_client);
        config.credential_provider = Some(credentials);
        Ok(config)
    }

    /// Bind every portable provider route, including the canonical ChatGPT
    /// account transport, to the host's existing credential authorities.
    pub fn credentialed_with_chatgpt(
        root: impl Into<PathBuf>,
        installation_id: InstallationId,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
        credentials: Arc<dyn CredentialProvider>,
        chatgpt_oauth: Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>,
    ) -> Result<Self> {
        Self::credentialed_with_chatgpt_and_native(
            root,
            installation_id,
            provider,
            model,
            base_url,
            credentials,
            chatgpt_oauth,
            None,
        )
    }

    /// Bind portable credential routes plus an optional host-native local
    /// inference runtime. The native route is selected only for
    /// [`EMBEDDED_NATIVE_LOCAL_PROVIDER_ID`].
    #[allow(clippy::too_many_arguments)]
    pub fn credentialed_with_chatgpt_and_native(
        root: impl Into<PathBuf>,
        installation_id: InstallationId,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
        credentials: Arc<dyn CredentialProvider>,
        chatgpt_oauth: Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>,
        native_inference: Option<Arc<dyn EmbeddedNativeInference>>,
    ) -> Result<Self> {
        let routed_chat_client = EmbeddedRoutedChatClient::new(
            provider.into(),
            model.into(),
            base_url,
            credentials.clone(),
            chatgpt_oauth.clone(),
            native_inference,
        )?;
        let (provider, model) = routed_chat_client.route();
        let chat_client: Arc<dyn AiChatClient> = Arc::new(routed_chat_client.clone());
        let mut config =
            Self::with_chat_client(root, installation_id, provider, model, chat_client);
        config.routed_chat_client = Some(routed_chat_client);
        config.credential_provider = Some(credentials);
        config.chatgpt_oauth = Some(chatgpt_oauth);
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
            credential_provider: None,
            tui_defaults: None,
            credentialed_chat_client: None,
            routed_chat_client: None,
            chatgpt_oauth: None,
            mcp_oauth: None,
            tool_registry_recipe: Arc::new(EmptyEmbeddedToolRegistryRecipe),
            foreground_turn_timeout: DEFAULT_FOREGROUND_TURN_TIMEOUT,
            max_live_turns: 1,
            delegated_task_transport: None,
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

    pub fn with_mcp_oauth(mut self, oauth: Arc<medousa_mcp_gateway::McpOAuthBroker>) -> Self {
        self.mcp_oauth = Some(oauth);
        self
    }

    /// Supply the non-secret runtime preferences owned by the native host.
    pub fn with_tui_defaults(mut self, defaults: crate::session::TuiDefaults) -> Self {
        self.tui_defaults = Some(defaults);
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

    /// Supply host routing and authenticated transport for explicit
    /// daemon-to-daemon delegation. This does not create a binding.
    pub fn with_delegated_task_transport(
        mut self,
        transport: Arc<dyn crate::delegated_task::DelegatedTaskTransport>,
    ) -> Self {
        self.delegated_task_transport = Some(transport);
        self
    }
}

enum EmbeddedInferenceBinding {
    Credentialed(CredentialedAiChatClient),
    Routed(EmbeddedRoutedChatClient),
    Fixed { provider: Arc<str>, model: Arc<str> },
}

impl EmbeddedInferenceBinding {
    fn route(&self) -> (String, String) {
        match self {
            Self::Credentialed(client) => {
                let config = client.config();
                (config.provider().to_string(), config.model().to_string())
            }
            Self::Routed(client) => client.route(),
            Self::Fixed { provider, model } => (provider.to_string(), model.to_string()),
        }
    }

    fn reconfigure(
        &self,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<()> {
        match self {
            Self::Credentialed(client) => {
                let config = CredentialedAiChatConfig::new(provider, model, base_url)
                    .context("invalid embedded inference configuration")?;
                client.reconfigure(config);
                Ok(())
            }
            Self::Routed(client) => client.reconfigure(provider, model, base_url),
            Self::Fixed { .. } => bail!("embedded inference binding is not reconfigurable"),
        }
    }
}

#[derive(Clone)]
struct EmbeddedRoutedChatClient {
    active: Arc<std::sync::RwLock<EmbeddedActiveInference>>,
    credentials: Arc<dyn CredentialProvider>,
    chatgpt_oauth: Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>,
    native_inference: Option<Arc<dyn EmbeddedNativeInference>>,
}

#[derive(Clone)]
struct EmbeddedActiveInference {
    provider: String,
    model: String,
    client: Arc<dyn AiChatClient>,
}

impl EmbeddedRoutedChatClient {
    fn new(
        provider: String,
        model: String,
        base_url: Option<String>,
        credentials: Arc<dyn CredentialProvider>,
        chatgpt_oauth: Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>,
        native_inference: Option<Arc<dyn EmbeddedNativeInference>>,
    ) -> Result<Self> {
        let active = Self::build_route(
            provider,
            model,
            base_url,
            credentials.clone(),
            chatgpt_oauth.clone(),
            native_inference.clone(),
        )?;
        Ok(Self {
            active: Arc::new(std::sync::RwLock::new(active)),
            credentials,
            chatgpt_oauth,
            native_inference,
        })
    }

    fn build_route(
        provider: String,
        model: String,
        base_url: Option<String>,
        credentials: Arc<dyn CredentialProvider>,
        chatgpt_oauth: Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>,
        native_inference: Option<Arc<dyn EmbeddedNativeInference>>,
    ) -> Result<EmbeddedActiveInference> {
        if provider
            .trim()
            .eq_ignore_ascii_case(EMBEDDED_NATIVE_LOCAL_PROVIDER_ID)
        {
            if base_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
            {
                bail!("native embedded inference does not accept a base URL");
            }
            let model = validate_embedded_native_inference_route(model, base_url)?;
            let inference = native_inference
                .ok_or_else(|| anyhow!("native embedded inference is unavailable on this host"))?;
            return Ok(EmbeddedActiveInference {
                provider: EMBEDDED_NATIVE_LOCAL_PROVIDER_ID.to_string(),
                model: model.clone(),
                client: Arc::new(EmbeddedNativeChatClient::new(model, inference)),
            });
        }
        if provider
            .trim()
            .eq_ignore_ascii_case(crate::openai_codex_chat_client::OPENAI_CODEX_PROVIDER_ID)
        {
            let model = validate_chatgpt_inference_route(model, base_url)?;
            return Ok(EmbeddedActiveInference {
                provider: crate::openai_codex_chat_client::OPENAI_CODEX_PROVIDER_ID.to_string(),
                model: model.clone(),
                client: Arc::new(
                    crate::openai_codex_chat_client::OpenAiCodexChatClient::with_broker(
                        model.clone(),
                        chatgpt_oauth,
                    ),
                ),
            });
        }
        let config = CredentialedAiChatConfig::new(provider, model, base_url)
            .context("invalid embedded inference configuration")?;
        let provider = config.provider().to_string();
        let model = config.model().to_string();
        let client = Arc::new(
            CredentialedAiChatClient::new(config, credentials)
                .context("initialize embedded inference client")?,
        );
        Ok(EmbeddedActiveInference {
            provider,
            model,
            client,
        })
    }

    fn snapshot(&self) -> EmbeddedActiveInference {
        self.active
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn route(&self) -> (String, String) {
        let active = self.snapshot();
        (active.provider, active.model)
    }

    fn reconfigure(
        &self,
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<()> {
        let next = Self::build_route(
            provider.into(),
            model.into(),
            base_url,
            self.credentials.clone(),
            self.chatgpt_oauth.clone(),
            self.native_inference.clone(),
        )?;
        *self
            .active
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
        Ok(())
    }
}

/// Validate a host-native model route before persisting or activating it.
pub fn validate_embedded_native_inference_route(
    model: String,
    base_url: Option<String>,
) -> Result<String> {
    if base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        bail!("native embedded inference does not accept a base URL");
    }
    let model = model.trim().to_string();
    if model.is_empty() || model.len() > 512 || model.chars().any(char::is_control) {
        bail!("invalid native embedded inference model");
    }
    Ok(model)
}

#[async_trait::async_trait]
impl AiChatClient for EmbeddedRoutedChatClient {
    async fn complete(
        &self,
        request: genai::chat::ChatRequest,
        options: Option<&genai::chat::ChatOptions>,
    ) -> StasisResult<genai::chat::ChatResponse> {
        self.snapshot().client.complete(request, options).await
    }

    async fn complete_stream(
        &self,
        request: genai::chat::ChatRequest,
        options: Option<&genai::chat::ChatOptions>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> StasisResult<genai::chat::ChatResponse> {
        self.snapshot()
            .client
            .complete_stream(request, options, chunk_tx)
            .await
    }
}

fn validate_chatgpt_inference_route(model: String, base_url: Option<String>) -> Result<String> {
    let model = model.trim().to_string();
    if model.is_empty() || model.len() > 256 || model.chars().any(char::is_control) {
        bail!("invalid embedded ChatGPT model");
    }
    if base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        bail!("the ChatGPT account route does not accept a custom base URL");
    }
    Ok(model)
}

/// Validate a route before the native host commits it to workshop settings.
pub fn validate_credentialed_inference_route(
    provider: impl Into<String>,
    model: impl Into<String>,
    base_url: Option<String>,
) -> Result<()> {
    let provider = provider.into();
    let model = model.into();
    if provider
        .trim()
        .eq_ignore_ascii_case(crate::openai_codex_chat_client::OPENAI_CODEX_PROVIDER_ID)
    {
        validate_chatgpt_inference_route(model, base_url)?;
        return Ok(());
    }
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
            .field("credential_provider", &self.credential_provider.is_some())
            .field("tui_defaults", &self.tui_defaults.is_some())
            .field("mcp_oauth", &self.mcp_oauth.is_some())
            .field("tool_registry", &"deployment-recipe")
            .field("foreground_turn_timeout", &self.foreground_turn_timeout)
            .field("max_live_turns", &self.max_live_turns)
            .field(
                "delegated_task_transport",
                &self.delegated_task_transport.is_some(),
            )
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
    credential_provider: Option<Arc<dyn CredentialProvider>>,
    chatgpt_oauth: Option<Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>>,
    mcp_oauth: Option<Arc<medousa_mcp_gateway::McpOAuthBroker>>,
    chat_client: Arc<dyn AiChatClient>,
    mcp_gateway_client: Arc<crate::mcp_gateway_client::McpGatewayClient>,
    tool_registry: Arc<dyn ToolRegistry>,
    session_store: Arc<dyn SessionStore>,
    profile_registry: Arc<std::sync::RwLock<crate::user_profiles::UserProfileRegistry>>,
    locus_service: crate::locus_service::LocusService,
    memory_writer: Arc<dyn MemoryContextWriter>,
    memory_operations: Arc<dyn MemoryOperations>,
    identity_store: Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>,
    runtime: Arc<RuntimeComposition>,
    _locus_memory: Arc<stasis::infrastructure::memory::locus_node_store_factory::LocusMemoryStore>,
    cluster_node_store: Arc<dyn ClusterNodeStore>,
    cluster_node: ClusterNode,
    turn_streams: TurnStreamRegistry,
    turn_stream_port: TurnStreamRegistryPortAdapter,
    turn_tickets: TurnTicketRegistry,
    executions: TurnExecutionRegistry,
    foreground_turn_timeout: Duration,
    backgrounded: AtomicBool,
    lifecycle_epoch: AtomicU64,
    recovery_lock: AsyncMutex<()>,
    delegation_service: Option<Arc<crate::delegation::DelegationService>>,
}

impl EmbeddedDaemon {
    /// Boot the daemon against one app-sandbox root.
    pub async fn boot(config: EmbeddedDaemonConfig) -> Result<Arc<Self>> {
        let root = prepare_root(&config.root).await?;
        crate::paths::configure_deployment_data_dir(root.clone()).map_err(anyhow::Error::msg)?;
        if let Some(defaults) = config.tui_defaults.as_ref() {
            crate::session::save_tui_defaults(defaults);
        }
        configure_file_session_root(root.join("history")).map_err(|error| anyhow!(error))?;
        crate::capability_catalog::configure_capabilities_manifest_path(
            root.join("capabilities.toml"),
        )
        .map_err(|error| anyhow!(error))?;
        crate::grapheme_script::configure_grapheme_script_root(root.join("grapheme-scripts"))
            .map_err(|error| anyhow!(error))?;
        crate::agent_mode_state::configure_agent_mode_state_path(
            root.join("agent_mode_state.json"),
        )
        .map_err(anyhow::Error::msg)?;
        crate::media_store::configure_media_store_root(root.join("media"))
            .map_err(anyhow::Error::msg)?;
        crate::feed_store::configure_feed_store_root(root.join("feeds"))
            .map_err(anyhow::Error::msg)?;
        crate::identity_manuscript::configure_manuscript_roots(
            root.join("manuscripts/user"),
            root.join("manuscripts/project"),
        )
        .map_err(anyhow::Error::msg)?;
        crate::artifact_retention::configure_artifact_retention_settings_path(
            root.join("artifact_retention.json"),
        )
        .map_err(anyhow::Error::msg)?;
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
        let environment_hub = crate::environment_store::install_environment_hub(environment_hub);
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
        crate::session_meta_store::init_session_meta_store_with_runtime(&runtime).await;
        crate::verification_store::init_verification_store_with_runtime(&runtime).await;
        crate::session_catalog::init_session_catalog_with_runtime(&runtime).await;
        crate::channel_session_store::init_channel_session_store_with_runtime(&runtime).await;
        crate::artifact_store::init_artifact_store_with_runtime(&runtime).await;
        crate::component_store::init_component_store_with_runtime(&runtime).await;
        crate::integration_connection::init_integration_connection_from_runtime(&runtime).await;
        crate::component_runtime_store::init_component_runtime_with_runtime(&runtime).await;
        crate::turn_continuation::init_turn_continuation_store_with_runtime(&runtime).await;
        crate::recurring_delivery::init_recurring_delivery_store_with_runtime(&runtime).await;
        crate::recurring_feed::init_recurring_feed_store_with_runtime(&runtime).await;

        let session_store: Arc<dyn SessionStore> = match &runtime {
            RuntimeComposition::Surreal(_) => get_session_store(),
            RuntimeComposition::InMemory(_) => {
                bail!("embedded daemon requires its SurrealKV persistence backend")
            }
        };
        let memory =
            crate::runtime::memory_bundle::MemoryAdapterBundle::from_runtime_shell(&runtime)
                .await
                .context("initialize embedded memory adapters")?;
        let locus_memory = memory.locus_memory.clone();
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
        let memory_reader = memory.memory_reader.clone();
        let memory_writer = memory.memory_writer.clone();
        let memory_operations = memory.memory_operations.clone();
        let identity_store = memory.identity_store.clone();
        let locus_service = crate::locus_service::LocusService::new(
            locus_memory.node_store.clone(),
            locus_memory.semantic_index.clone(),
            memory_reader.clone(),
        );
        let runtime = Arc::new(runtime);
        let turn_streams = new_turn_stream_registry();
        let turn_stream_port = TurnStreamRegistryPortAdapter::new(turn_streams.clone());
        let turn_tickets = new_registry();
        let delegation_service = config
            .delegated_task_transport
            .clone()
            .map(|transport| {
                crate::delegation::install_delegation_runtime(
                    runtime.clone(),
                    authority_id.clone(),
                    session_store.clone(),
                    transport,
                )
            })
            .transpose()
            .context("install embedded delegation runtime")?;
        if let Some(service) = delegation_service.as_ref() {
            service.set_completion_sink(Arc::new(EmbeddedDelegationCompletionSink {
                turn_streams: turn_streams.clone(),
                turn_stream_port: turn_stream_port.clone(),
                turn_tickets: turn_tickets.clone(),
            }));
        }
        let mcp_config = Arc::new(
            medousa_mcp_gateway::McpGatewayFullConfig::from_env_and_args(&[]).remote_only(),
        );
        let mcp_invokes_enabled = mcp_config.invokes_enabled;
        let mut mcp_registry = medousa_mcp_gateway::ServerRegistry::with_policy_evaluator(
            mcp_config,
            Arc::new(EmbeddedMcpPolicyEvaluator),
        );
        if let Some(oauth) = config.mcp_oauth.clone() {
            mcp_registry = mcp_registry.with_oauth(oauth);
        }
        let mcp_registry = Arc::new(mcp_registry);
        let mcp_gateway_client = Arc::new(crate::mcp_gateway_client::McpGatewayClient::in_process(
            mcp_registry,
            mcp_invokes_enabled,
        ));
        let tool_assembly = config
            .tool_registry_recipe
            .assemble(EmbeddedToolRegistryBindings {
                runtime: runtime.clone(),
                locus_store: locus_memory.node_store.clone(),
                semantic_index: locus_memory.semantic_index.clone(),
                memory_reader: memory_reader.clone(),
                memory_writer: memory_writer.clone(),
                memory_operations: memory_operations.clone(),
                identity_store: memory.identity_store.clone(),
                mcp_gateway_client: mcp_gateway_client.clone(),
                provider: config.provider.clone(),
                model: config.model.clone(),
                chat_client: config.chat_client.clone(),
                delegation_service: delegation_service.clone(),
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
        let memory_operations_for_runtime = Some(memory_operations.clone());
        let identity_store_for_runtime = Some(memory.identity_store_dyn());
        match runtime.as_ref() {
            RuntimeComposition::InMemory(runtime) => {
                crate::daemon_runtime_handlers::register_daemon_runtime_handlers(
                    runtime,
                    &config.chat_client,
                    &tool_registry,
                    &workflow_engine,
                    &memory_reader,
                    &memory_writer_for_runtime,
                    &identity_store_for_runtime,
                    &memory_operations_for_runtime,
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
                    &identity_store_for_runtime,
                    &memory_operations_for_runtime,
                    &thread_store,
                    &cluster_node_store,
                )?;
            }
        }
        crate::artifact_maintenance_job::register_artifact_maintenance_handler(runtime.as_ref())
            .await
            .context("register embedded artifact maintenance handler")?;
        if let Err(error) =
            crate::artifact_retention::ensure_schedule_on_startup(runtime.as_ref()).await
        {
            // Retention is optional maintenance. A corrupt legacy schedule or
            // temporary persistence failure must never prevent Personal from
            // opening; the next boot/settings update can repair it.
            tracing::warn!(%error, "embedded artifact retention schedule unavailable");
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

        let local_credential_id: Arc<str> = Arc::from(format!(
            "embedded-home:{}",
            config.installation_id.storage_key().as_str()
        ));
        let inference = match (config.routed_chat_client, config.credentialed_chat_client) {
            (Some(client), _) => EmbeddedInferenceBinding::Routed(client),
            (None, Some(client)) => EmbeddedInferenceBinding::Credentialed(client),
            (None, None) => EmbeddedInferenceBinding::Fixed {
                provider: Arc::from(config.provider),
                model: Arc::from(config.model),
            },
        };

        let daemon = Arc::new(Self {
            root,
            environment_hub,
            authority_id,
            local_credential_id,
            inference,
            credential_provider: config.credential_provider,
            chatgpt_oauth: config.chatgpt_oauth,
            mcp_oauth: config.mcp_oauth,
            chat_client: config.chat_client,
            mcp_gateway_client,
            tool_registry,
            session_store,
            profile_registry,
            locus_service,
            memory_writer,
            memory_operations,
            identity_store,
            runtime,
            _locus_memory: locus_memory,
            cluster_node_store,
            cluster_node,
            turn_streams,
            turn_stream_port,
            turn_tickets,
            executions: TurnExecutionRegistry::new(config.max_live_turns),
            foreground_turn_timeout: config.foreground_turn_timeout,
            backgrounded: AtomicBool::new(false),
            lifecycle_epoch: AtomicU64::new(0),
            recovery_lock: AsyncMutex::new(()),
            delegation_service: delegation_service.clone(),
        });
        if let Some(service) = delegation_service {
            service
                .resume_pending()
                .await
                .context("resume embedded delegation drivers")?;
        }
        Ok(daemon)
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

    /// Close foreground admission while the mobile host is backgrounded.
    ///
    /// Existing executions remain owned by the runtime. The OS may continue,
    /// suspend, or terminate the process; app lifecycle never fabricates a
    /// turn cancellation.
    pub fn enter_background(&self) -> usize {
        self.backgrounded.store(true, Ordering::Release);
        self.lifecycle_epoch.fetch_add(1, Ordering::AcqRel);
        self.executions.live_count()
    }

    /// Re-advertise the same Stasis node and run its canonical durable-work
    /// reconciliation before foreground admission reopens.
    pub async fn resume(&self) -> Result<RuntimeRecoveryReport> {
        let _recovery = self.recovery_lock.lock().await;
        if !self.backgrounded.load(Ordering::Acquire) {
            return Ok(RuntimeRecoveryReport::default());
        }
        let lifecycle_epoch = self.lifecycle_epoch.load(Ordering::Acquire);
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
        if let Some(service) = self.delegation_service.as_ref() {
            service
                .resume_pending()
                .await
                .context("resume embedded delegation drivers after wake")?;
        }
        if lifecycle_epoch == self.lifecycle_epoch.load(Ordering::Acquire) {
            self.backgrounded.store(false, Ordering::Release);
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
        current_turn_user_message: ChatMessage,
        media_refs: Vec<medousa_types::daemon_api::MediaRef>,
        reasoning_effort: String,
        agent_mode: AgentModeId,
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
        let user_turn = crate::turn_parts::user_conversation_turn_with_media_and_speaker(
            prompt.clone(),
            &media_refs,
            context.legacy_scope().identity_user_id.as_deref(),
        );
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
        let tool_registry: Arc<dyn ToolRegistry> = if agent_mode == AgentModeId::Instant {
            Arc::new(EmbeddedModeToolRegistry::instant(
                self.tool_registry.clone(),
            ))
        } else {
            self.tool_registry.clone()
        };
        let tool_loop = MedousaToolLoopPipeline::new(prompt_pipeline, tool_registry);
        let mut prompt_context = crate::reasoning_effort::prompt_execution_context(
            context.route().model(),
            Some(&reasoning_effort),
        );
        prompt_context.correlation_id = Some(context.correlation_id().to_string());
        let request = ToolLoopExecutionRequest {
            user_prompt: inference_prompt,
            system_prompt: Some(embedded_system_prompt(agent_mode)),
            context: prompt_context,
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
                    Some(current_turn_user_message),
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
                            Ok(response) => {
                                let worker_work_id = medousa_runtime::turn_control::worker_spawn_from_invocations(
                                    &response.tool_invocations,
                                )
                                .map(|(work_id, _)| work_id);
                                ForegroundOutcome::Completed {
                                    text: response.text,
                                    tool_names: response
                                        .tool_invocations
                                        .into_iter()
                                        .map(|invocation| invocation.tool_name)
                                        .collect(),
                                    termination_reason: response.termination_reason,
                                    worker_work_id,
                                }
                            }
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

        let mut keep_stream_open = false;
        match outcome {
            ForegroundOutcome::Completed {
                text,
                tool_names,
                termination_reason,
                worker_work_id,
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
                let handoff_work_id = (termination_reason == "worker_spawned")
                    .then_some(worker_work_id)
                    .flatten();
                if let Some(work_id) = handoff_work_id.as_ref() {
                    keep_stream_open = chronological
                        .publish(TurnStreamEventV3::WorkerAck {
                            ack_kind: WorkerAckKind::Worker,
                            text: body.clone(),
                            tool_names: tool_names.clone(),
                            work_id: Some(work_id.clone()),
                        })
                        .await
                        .is_ok();
                    if keep_stream_open {
                        note_stream_event(
                            &self.turn_tickets,
                            &turn_id,
                            "worker_ack",
                            "worker_ack",
                            false,
                        )
                        .await;
                    }
                }
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
                        if handoff_work_id.is_none()
                            && chronological
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
                        keep_stream_open = false;
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

        if !keep_stream_open {
            self.turn_stream_port.mark_stream_closed(&turn_id).await;
        }
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
        worker_work_id: Option<String>,
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

    pub fn sync_tui_defaults(&self, defaults: crate::session::TuiDefaults) -> Result<()> {
        self.require(Capability::AdminRuntime)?;
        crate::session::save_tui_defaults(&defaults);
        Ok(())
    }

    fn chatgpt_oauth(&self) -> Result<&Arc<crate::chatgpt_oauth::ChatGptOAuthBroker>> {
        self.require(Capability::AdminRuntime)?;
        self.daemon
            .chatgpt_oauth
            .as_ref()
            .ok_or_else(|| anyhow!("ChatGPT account authentication is unavailable"))
    }

    pub fn chatgpt_oauth_status(&self) -> Result<ChatGptOAuthStatusResponse> {
        Ok(self.chatgpt_oauth()?.status())
    }

    pub async fn begin_chatgpt_oauth(&self) -> Result<BeginChatGptOAuthResponse> {
        self.chatgpt_oauth()?
            .begin()
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn complete_chatgpt_oauth(
        &self,
        login_id: &str,
    ) -> Result<CompleteChatGptOAuthResponse> {
        self.chatgpt_oauth()?
            .complete(login_id)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn refresh_chatgpt_oauth(&self) -> Result<ChatGptOAuthStatusResponse> {
        self.chatgpt_oauth()?
            .refresh()
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn disconnect_chatgpt_oauth(&self) -> Result<DisconnectChatGptOAuthResponse> {
        self.chatgpt_oauth()?
            .disconnect()
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn list_chatgpt_models(&self) -> Result<ChatGptModelListResponse> {
        self.chatgpt_oauth()?
            .list_models()
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn mcp_gateway_status(
        &self,
    ) -> Result<(
        medousa_types::mcp_gateway_api::McpGatewayHealthResponse,
        medousa_types::mcp_gateway_api::McpServersResponse,
    )> {
        self.require(Capability::WorkshopRead)?;
        let health = self.daemon.mcp_gateway_client.health().await?;
        let servers = self.daemon.mcp_gateway_client.list_servers().await?;
        Ok((health, servers))
    }

    pub async fn mcp_gateway_catalog(
        &self,
    ) -> Result<medousa_types::mcp_gateway_api::McpCatalogSyncResponse> {
        self.require(Capability::WorkshopRead)?;
        self.daemon.mcp_gateway_client.fetch_catalog().await
    }

    pub async fn mcp_oauth_status(
        &self,
        server_id: &str,
    ) -> Result<medousa_types::mcp_gateway_api::McpOAuthStatusResponse> {
        self.require(Capability::AdminRuntime)?;
        self.daemon.mcp_gateway_client.oauth_status(server_id).await
    }

    pub async fn begin_mcp_oauth(
        &self,
        request: medousa_types::mcp_gateway_api::BeginMcpOAuthRequest,
    ) -> Result<medousa_types::mcp_gateway_api::BeginMcpOAuthResponse> {
        self.require(Capability::AdminRuntime)?;
        self.daemon.mcp_gateway_client.begin_oauth(request).await
    }

    pub async fn complete_mcp_oauth(
        &self,
        request: medousa_types::mcp_gateway_api::CompleteMcpOAuthRequest,
    ) -> Result<medousa_types::mcp_gateway_api::CompleteMcpOAuthResponse> {
        self.require(Capability::AdminRuntime)?;
        self.daemon.mcp_gateway_client.complete_oauth(request).await
    }

    pub async fn refresh_mcp_oauth(
        &self,
        server_id: &str,
    ) -> Result<medousa_types::mcp_gateway_api::McpOAuthStatusResponse> {
        self.require(Capability::AdminRuntime)?;
        self.daemon
            .mcp_gateway_client
            .refresh_oauth(server_id)
            .await
    }

    pub async fn disconnect_mcp_oauth(
        &self,
        server_id: &str,
    ) -> Result<medousa_types::mcp_gateway_api::DisconnectMcpOAuthResponse> {
        self.require(Capability::AdminRuntime)?;
        self.daemon
            .mcp_gateway_client
            .disconnect_oauth(server_id)
            .await
    }

    pub async fn reconfigure_mcp_gateway(
        &self,
        config: medousa_mcp_gateway::McpGatewayFullConfig,
    ) -> Result<()> {
        self.require(Capability::AdminRuntime)?;
        let config = Arc::new(config.remote_only());
        let invokes_enabled = config.invokes_enabled;
        let mut registry = medousa_mcp_gateway::ServerRegistry::with_policy_evaluator(
            config,
            Arc::new(EmbeddedMcpPolicyEvaluator),
        );
        if let Some(oauth) = self.daemon.mcp_oauth.clone() {
            registry = registry.with_oauth(oauth);
        }
        let registry = Arc::new(registry);
        self.daemon
            .mcp_gateway_client
            .replace_in_process(registry, invokes_enabled)
            .await
    }

    pub async fn delegation_binding(&self) -> Result<Option<crate::delegation::DelegationBinding>> {
        self.require(Capability::AdminRuntime)?;
        let service = self
            .daemon
            .delegation_service
            .as_ref()
            .ok_or_else(|| anyhow!("delegation transport is not configured"))?;
        service.binding().await
    }

    pub async fn set_delegation_binding(
        &self,
        target: crate::delegation::DelegationTarget,
    ) -> Result<crate::delegation::DelegationBinding> {
        self.require(Capability::AdminRuntime)?;
        let service = self
            .daemon
            .delegation_service
            .as_ref()
            .ok_or_else(|| anyhow!("delegation transport is not configured"))?;
        service.bind(target).await
    }

    pub async fn clear_delegation_binding(&self) -> Result<bool> {
        self.require(Capability::AdminRuntime)?;
        let service = self
            .daemon
            .delegation_service
            .as_ref()
            .ok_or_else(|| anyhow!("delegation transport is not configured"))?;
        service.clear().await
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
        let mut advertised_capabilities = self
            .daemon
            .cluster_node
            .capability_tags
            .iter()
            .cloned()
            .chain(["transport.in-process", "mcp.remote-config"].map(str::to_string))
            .collect::<Vec<_>>();
        if self.daemon.chatgpt_oauth.is_some() {
            advertised_capabilities.push("auth.chatgpt-account".to_string());
        }
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

    pub async fn create_profile(
        &self,
        slug: &str,
        display_name: &str,
    ) -> Result<CreateUserProfileResponse> {
        self.require(Capability::AdminRuntime)?;
        let profile = {
            let mut registry = self
                .daemon
                .profile_registry
                .write()
                .map_err(|_| anyhow!("profile registry lock poisoned"))?;
            registry.create_profile(slug, display_name)?
        };
        crate::identity_memory::seed_workshop_profile_user(
            self.daemon.identity_store.as_ref(),
            &profile.profile_id,
        )
        .await?;
        let registry = self
            .daemon
            .profile_registry
            .read()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?;
        Ok(CreateUserProfileResponse {
            profile: profile.to_dto(),
            active_profile_id: registry.active_profile_id().to_string(),
            resolved_user_id: registry.resolve_active_user_id(),
        })
    }

    pub async fn identity_context(&self, request: IdentityContextRequest) -> Result<Value> {
        self.require(Capability::ProfileSelf)?;
        let user_id = request
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(self.active_profile_id()?);
        let persona_id = request
            .persona_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(crate::identity_memory::resolve_identity_persona_id);
        let channel_id = request
            .channel_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                crate::identity_memory::resolve_identity_channel_id(
                    request.policy_profile.as_deref(),
                )
            });
        let relationship_limit = request.relationship_limit.unwrap_or(8).clamp(1, 64);
        let mode =
            crate::identity_memory::parse_identity_context_mode_label(request.mode.as_deref());
        let service =
            stasis::application::use_cases::identity_memory_service::IdentityMemoryService::new(
                self.daemon.identity_store.clone() as Arc<dyn IdentityMemoryStore>,
            );
        let response = service
            .get_identity_context(&crate::identity_memory::build_identity_context_request(
                user_id,
                persona_id,
                channel_id,
                relationship_limit,
                mode,
            ))
            .await
            .map_err(anyhow::Error::new)?;
        serde_json::to_value(response).map_err(anyhow::Error::new)
    }

    pub async fn identity_remember(
        &self,
        request: IdentityRememberRequest,
    ) -> Result<IdentityRememberResponse> {
        self.require(Capability::ProfileSelf)?;
        let user_id = request
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(self.active_profile_id()?);
        let subject = request.subject.trim();
        let statement = request.statement.trim();
        if subject.is_empty() || statement.is_empty() {
            bail!("subject and statement are required");
        }
        let source = crate::identity_write_policy::parse_update_source(
            request.source.as_deref().or(Some("user_direct")),
        )
        .map_err(anyhow::Error::msg)?;
        let writer = crate::cognitive_identity_writer::CognitiveIdentityWriter::new(
            self.daemon.identity_store.clone(),
            Some(self.daemon.memory_writer.clone()),
        );
        let result = match request.fact_kind.trim().to_ascii_lowercase().as_str() {
            "preference" => {
                writer
                    .remember_preference(
                        &user_id,
                        subject,
                        Value::String(statement.to_string()),
                        source,
                        1.0,
                        "home teach medousa",
                    )
                    .await
            }
            "person" => {
                writer
                    .remember_contact(
                        &user_id,
                        subject,
                        statement,
                        &request.attributes,
                        &[],
                        source,
                        1.0,
                        "home teach medousa",
                    )
                    .await
            }
            "note" => {
                writer
                    .remember_note(
                        &user_id,
                        subject,
                        statement,
                        source,
                        1.0,
                        "home teach medousa",
                    )
                    .await
            }
            other => bail!("unsupported fact_kind '{other}', expected preference|person|note"),
        }
        .map_err(anyhow::Error::new)?;
        let message = if result.committed {
            format!("Remembered {subject}")
        } else if result.requires_confirmation {
            "Saved as a proposal — confirmation may be required".to_string()
        } else {
            "Could not commit this fact".to_string()
        };
        Ok(IdentityRememberResponse {
            committed: result.committed,
            requires_confirmation: result.requires_confirmation,
            proposal_ids: result.proposal_ids,
            digest_preview: result.digest_preview,
            message,
        })
    }

    pub async fn identity_digest_preview(
        &self,
        request: IdentityContextRequest,
    ) -> Result<IdentityDigestPreviewResponse> {
        self.require(Capability::ProfileSelf)?;
        let user_id = request
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(self.active_profile_id()?);
        let context = self
            .identity_context(IdentityContextRequest {
                user_id: Some(user_id.clone()),
                relationship_limit: Some(request.relationship_limit.unwrap_or(32).clamp(1, 64)),
                ..request
            })
            .await?;
        let context = serde_json::from_value::<
            stasis::ports::outbound::memory::identity_memory_models::GetIdentityContextResponse,
        >(context)?;
        let ranked = crate::identity_markdown::compile_identity_digest_preview(
            self.daemon.identity_store.as_ref(),
            Some(user_id.as_str()),
        )
        .await?;
        Ok(IdentityDigestPreviewResponse {
            digest_text: ranked.text,
            preference_count: context
                .user
                .as_ref()
                .map(|user| user.preferences.len())
                .unwrap_or(0),
            contact_count: context.contacts.len(),
            relationship_count: context.relationships.len(),
            claim_count: context.flattened_claims.len(),
        })
    }

    pub async fn identity_export_markdown(
        &self,
        request: IdentityExportMarkdownRequest,
    ) -> Result<IdentityExportMarkdownResponse> {
        self.require(Capability::ProfileSelf)?;
        let user_id = request
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(self.active_profile_id()?);
        let dir = self.daemon.root.join("identity-markdown");
        let written = crate::identity_markdown::write_identity_markdown_export(
            self.daemon.identity_store.as_ref(),
            Some(user_id.as_str()),
            &dir,
        )
        .await?;
        Ok(IdentityExportMarkdownResponse {
            export_dir: written.display().to_string(),
            files: vec![
                "SOUL.md".to_string(),
                "USER.md".to_string(),
                "PEOPLE.md".to_string(),
                "IDENTITY.md".to_string(),
            ],
        })
    }

    pub async fn export_profile(
        &self,
        request: ExportUserProfileRequest,
    ) -> Result<ExportUserProfileResponse> {
        self.require(Capability::ProfileSelf)?;
        let registry = self
            .daemon
            .profile_registry
            .read()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?
            .clone();
        let bundle = crate::profile_portability::export_profile_bundle(
            &registry,
            self.daemon.identity_store.as_ref(),
            self.daemon._locus_memory.node_store.clone(),
            &request.profile_id,
            request.session_limit,
            request.node_limit_per_session,
        )
        .await?;
        Ok(ExportUserProfileResponse { bundle })
    }

    pub async fn import_profile(
        &self,
        request: ImportUserProfileRequest,
    ) -> Result<ImportUserProfileResponse> {
        self.require(Capability::AdminRuntime)?;
        let mut registry = self
            .daemon
            .profile_registry
            .read()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?
            .clone();
        let summary = crate::profile_portability::import_profile_bundle(
            &mut registry,
            self.daemon.identity_store.as_ref(),
            self.daemon._locus_memory.node_store.clone(),
            &request.bundle,
            request.dry_run,
        )
        .await?;
        if !request.dry_run && summary.created_profile {
            *self
                .daemon
                .profile_registry
                .write()
                .map_err(|_| anyhow!("profile registry lock poisoned"))? = registry;
        }
        let message = if summary.dry_run {
            format!(
                "dry-run: would import {} locus nodes across {} sessions for {}",
                summary.locus_nodes_imported, summary.locus_sessions_touched, summary.profile_id
            )
        } else {
            format!(
                "imported {} locus nodes across {} sessions for {}",
                summary.locus_nodes_imported, summary.locus_sessions_touched, summary.profile_id
            )
        };
        Ok(ImportUserProfileResponse {
            dry_run: summary.dry_run,
            profile_id: summary.profile_id,
            created_profile: summary.created_profile,
            identity_user_imported: summary.identity_user_imported,
            contacts_imported: summary.contacts_imported,
            relationships_imported: summary.relationships_imported,
            locus_nodes_imported: summary.locus_nodes_imported,
            locus_sessions_touched: summary.locus_sessions_touched,
            message,
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

    fn active_profile_id(&self) -> Result<String> {
        if let Some(profile_id) = self.principal.profile_id() {
            return Ok(profile_id.to_string());
        }
        let registry = self
            .daemon
            .profile_registry
            .read()
            .map_err(|_| anyhow!("profile registry lock poisoned"))?;
        Ok(registry.resolve_active_user_id())
    }

    fn prompt_stash_store(&self) -> crate::prompt_stash::PromptStashStore {
        crate::prompt_stash::PromptStashStore::at(self.daemon.root.join("prompt_stashes.json"))
    }

    pub fn list_prompt_stashes(&self) -> Result<PromptStashListResponse> {
        self.require(Capability::ContentRead)?;
        let profile_id = self.active_profile_id()?;
        let stashes = self
            .prompt_stash_store()
            .list(&profile_id)
            .map_err(anyhow::Error::msg)?;
        Ok(PromptStashListResponse { stashes })
    }

    pub fn create_prompt_stash(&self, request: CreatePromptStashRequest) -> Result<PromptStash> {
        self.require(Capability::ContentWrite)?;
        if request
            .source_session
            .as_ref()
            .is_some_and(|source| source.authority_id != self.daemon.authority_id)
        {
            bail!("source session is not available");
        }
        let profile_id = self.active_profile_id()?;
        self.prompt_stash_store()
            .create(&profile_id, request)
            .map_err(anyhow::Error::msg)
    }

    pub fn delete_prompt_stash(&self, stash_id: &str) -> Result<DeletePromptStashResponse> {
        self.require(Capability::ContentWrite)?;
        let stash_id = PromptStashId::parse(stash_id).map_err(anyhow::Error::new)?;
        let profile_id = self.active_profile_id()?;
        self.prompt_stash_store()
            .delete(&profile_id, &stash_id)
            .map_err(anyhow::Error::msg)
    }

    pub fn list_calendar_events(&self, query: CalendarListQuery) -> Result<CalendarListResponse> {
        self.require(Capability::ContentRead)?;
        crate::calendar::CalendarService::list_events(query.path.as_deref(), query.from, query.to)
    }

    pub fn create_calendar_event(
        &self,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::calendar::CalendarService::create_event(request)
    }

    pub fn update_calendar_event(
        &self,
        uid: &str,
        request: &CalendarWriteRequest,
    ) -> Result<CalendarWriteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::calendar::CalendarService::update_event(uid, request)
    }

    pub fn delete_calendar_event(
        &self,
        uid: &str,
        path: Option<&str>,
    ) -> Result<CalendarDeleteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::calendar::CalendarService::delete_event(uid, path)
    }

    pub fn import_calendar(
        &self,
        request: &CalendarImportRequest,
    ) -> Result<CalendarImportResponse> {
        self.require(Capability::ContentWrite)?;
        crate::calendar::CalendarService::import(request)
    }

    pub fn export_calendar(&self, path: Option<&str>) -> Result<CalendarExportResponse> {
        self.require(Capability::ContentRead)?;
        crate::calendar::CalendarService::export(path)
    }

    pub fn artifact_command(
        &self,
        request: ArtifactCommandRequest,
    ) -> Result<ArtifactCommandResponse> {
        self.require(Capability::ContentRead)?;
        crate::session_storage::validate_session_id(&request.session_id)
            .map_err(anyhow::Error::new)?;
        crate::artifact_command_runtime::execute_artifact_command(request)
    }

    pub fn artifact_fetch(&self, request: ArtifactFetchRequest) -> Result<ArtifactFetchResponse> {
        self.require(Capability::ContentRead)?;
        crate::session_storage::validate_session_id(&request.session_id)
            .map_err(anyhow::Error::new)?;
        crate::artifact_command_runtime::execute_artifact_fetch(request)
    }

    pub fn artifact_list_ui(
        &self,
        request: ArtifactListUiRequest,
    ) -> Result<ArtifactListUiResponse> {
        self.require(Capability::ContentRead)?;
        if let Some(session_id) = request.session_id.as_deref() {
            crate::session_storage::validate_session_id(session_id).map_err(anyhow::Error::new)?;
        }
        crate::artifact_command_runtime::execute_artifact_list_ui(request)
    }

    pub fn artifact_write(&self, request: ArtifactWriteRequest) -> Result<ArtifactWriteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::session_storage::validate_session_id(&request.session_id)
            .map_err(anyhow::Error::new)?;
        crate::artifact_command_runtime::execute_artifact_write(request)
    }

    pub fn artifact_delete(
        &self,
        request: ArtifactDeleteRequest,
    ) -> Result<ArtifactDeleteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::session_storage::validate_session_id(&request.session_id)
            .map_err(anyhow::Error::new)?;
        crate::artifact_command_runtime::execute_artifact_delete(request)
    }

    pub async fn artifact_retention_status(&self) -> Result<ArtifactRetentionStatusResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::artifact_retention::get_status(self.daemon.runtime.as_ref())
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn update_artifact_retention(
        &self,
        request: UpdateArtifactRetentionRequest,
    ) -> Result<UpdateArtifactRetentionResponse> {
        self.require(Capability::AdminRuntime)?;
        crate::artifact_retention::update_settings(self.daemon.runtime.as_ref(), request)
            .await
            .map_err(anyhow::Error::new)
    }

    pub fn upload_media(
        &self,
        session_id: &str,
        bytes: &[u8],
        mime: &str,
        label: Option<&str>,
    ) -> Result<MediaUploadResponse> {
        self.require(Capability::ContentWrite)?;
        let session_id = SessionId::parse(session_id).map_err(anyhow::Error::new)?;
        crate::media_store::persist_user_media(session_id.as_str(), bytes, mime, label)
            .map_err(anyhow::Error::msg)
    }

    pub fn read_media(&self, session_id: &str, media_id: &str) -> Result<(String, Vec<u8>)> {
        self.require(Capability::ContentRead)?;
        let session_id = SessionId::parse(session_id).map_err(anyhow::Error::new)?;
        let media_id = media_id.trim();
        if media_id.is_empty() {
            bail!("media_id is required");
        }
        let record = crate::media_store::get_media_record(session_id.as_str(), media_id)
            .ok_or_else(|| anyhow!("media not found"))?;
        let bytes = crate::media_store::open_media_payload(&record).map_err(anyhow::Error::msg)?;
        Ok((record.mime, bytes))
    }

    pub async fn stt_status(&self) -> Result<crate::stt::SttStatusResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(match self.daemon.credential_provider.as_deref() {
            Some(credentials) => crate::stt::stt_status_with_credentials(credentials).await,
            None => crate::stt::stt_status(),
        })
    }

    pub async fn transcribe_audio(
        &self,
        audio_bytes: &[u8],
        mime_type: &str,
    ) -> Result<crate::stt::SttTranscribeResponse> {
        self.require(Capability::WorkshopInteract)?;
        let result = match self.daemon.credential_provider.as_ref() {
            Some(credentials) => {
                crate::stt::transcribe_audio_with_credentials(
                    audio_bytes,
                    mime_type,
                    credentials.clone(),
                )
                .await
            }
            None => crate::stt::transcribe_audio(audio_bytes, mime_type).await,
        };
        result.map_err(|failure| anyhow!(failure.operator_message))
    }

    pub async fn component_store_get(
        &self,
        component_id: String,
        key: Option<String>,
        profile_id: Option<String>,
    ) -> Result<ComponentStoreGetResponse> {
        self.require(Capability::ContentRead)?;
        crate::component_store_handlers::get_store(
            axum::extract::State(crate::component_store_handlers::ComponentStoreApiState),
            axum::extract::Path(component_id),
            axum::extract::Query(ComponentStoreQuery { profile_id, key }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn component_store_set(
        &self,
        component_id: String,
        key: String,
        value: Value,
        profile_id: Option<String>,
    ) -> Result<ComponentStoreSetResponse> {
        self.require(Capability::ContentWrite)?;
        crate::component_store_handlers::put_store_key(
            axum::extract::State(crate::component_store_handlers::ComponentStoreApiState),
            axum::extract::Path((component_id, key)),
            axum::Json(ComponentStoreSetRequest { value, profile_id }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn component_store_delete(
        &self,
        component_id: String,
        key: String,
        profile_id: Option<String>,
    ) -> Result<ComponentStoreDeleteResponse> {
        self.require(Capability::ContentWrite)?;
        crate::component_store_handlers::delete_store_key(
            axum::extract::State(crate::component_store_handlers::ComponentStoreApiState),
            axum::extract::Path((component_id, key)),
            axum::extract::Query(ComponentStoreQuery {
                profile_id,
                key: None,
            }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn component_store_list_keys(
        &self,
        component_id: String,
        profile_id: Option<String>,
    ) -> Result<ComponentStoreListResponse> {
        self.require(Capability::ContentRead)?;
        crate::component_store_handlers::list_store_keys(
            axum::extract::State(crate::component_store_handlers::ComponentStoreApiState),
            axum::extract::Path(component_id),
            axum::extract::Query(ComponentStoreQuery {
                profile_id,
                key: None,
            }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn component_runtime_append_events(
        &self,
        component_id: String,
        request: ComponentRuntimeEventsRequest,
    ) -> Result<ComponentRuntimeEventsResponse> {
        self.require(Capability::ContentWrite)?;
        crate::component_runtime_handlers::append_runtime_events(
            axum::extract::State(crate::component_runtime_handlers::ComponentRuntimeApiState),
            axum::extract::Path(component_id),
            axum::Json(request),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn component_runtime_tail_events(
        &self,
        component_id: String,
        profile_id: Option<String>,
        limit: Option<usize>,
    ) -> Result<ComponentRuntimeEventsTailResponse> {
        self.require(Capability::ContentRead)?;
        crate::component_runtime_handlers::tail_runtime_events(
            axum::extract::State(crate::component_runtime_handlers::ComponentRuntimeApiState),
            axum::extract::Path(component_id),
            axum::extract::Query(ComponentRuntimeEventsQuery { profile_id, limit }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn component_runtime_complete_probe(
        &self,
        component_id: String,
        probe_id: String,
        result: ComponentRuntimeProbeResult,
    ) -> Result<bool> {
        self.require(Capability::ContentWrite)?;
        crate::component_runtime_handlers::complete_probe(
            axum::extract::State(crate::component_runtime_handlers::ComponentRuntimeApiState),
            axum::extract::Path((component_id, probe_id)),
            axum::Json(result),
        )
        .await
        .map(|axum::Json(response)| response.get("ok").and_then(Value::as_bool).unwrap_or(true))
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn feed_tail(
        &self,
        feed_id: &str,
        profile_id: Option<String>,
        limit: Option<usize>,
    ) -> Result<FeedTailResponse> {
        self.require(Capability::ContentRead)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id.as_deref());
        medousa_types::authority_id::EnvironmentProfileId::parse(&profile_id)
            .map_err(anyhow::Error::new)?;
        let feed_id =
            medousa_types::authority_id::FeedId::parse(feed_id).map_err(anyhow::Error::new)?;
        let events = crate::feed_store::feed_store()
            .tail(
                &profile_id,
                feed_id.as_str(),
                limit.unwrap_or(20).clamp(1, 100),
            )
            .await;
        Ok(FeedTailResponse {
            feed_id: feed_id.to_string(),
            events,
        })
    }

    pub async fn feed_latest_good(
        &self,
        feed_id: &str,
        profile_id: Option<String>,
    ) -> Result<FeedLatestGoodResponse> {
        self.require(Capability::ContentRead)?;
        let profile_id = crate::environment_store::resolve_profile_id(profile_id.as_deref());
        medousa_types::authority_id::EnvironmentProfileId::parse(&profile_id)
            .map_err(anyhow::Error::new)?;
        let feed_id =
            medousa_types::authority_id::FeedId::parse(feed_id).map_err(anyhow::Error::new)?;
        crate::feed_store::feed_store()
            .latest_good(&profile_id, feed_id.as_str())
            .await
            .ok_or_else(|| anyhow!("no latest good result for feed '{feed_id}'"))
    }

    pub fn model_catalog_list(
        &self,
        provider: Option<String>,
        capability: Option<String>,
        q: Option<String>,
    ) -> Result<Value> {
        self.require(Capability::AdminRuntime)?;
        serde_json::to_value(crate::model_capability_registry::registry().list_catalog(
            crate::model_capability_registry::types::ModelCatalogListQuery {
                provider,
                capability,
                q,
            },
        ))
        .map_err(anyhow::Error::new)
    }

    pub fn model_capabilities(&self, provider: &str, model: &str) -> Result<Value> {
        self.require(Capability::AdminRuntime)?;
        serde_json::to_value(crate::model_capability_registry::registry().resolve(provider, model))
            .map_err(anyhow::Error::new)
    }

    pub async fn refresh_model_catalog(&self, providers: Option<Vec<String>>) -> Result<Value> {
        self.require(Capability::AdminRuntime)?;
        let registry = crate::model_capability_registry::registry();
        let response = match self.daemon.credential_provider.as_deref() {
            Some(credentials) => {
                registry
                    .refresh_with_credentials(providers, credentials)
                    .await
            }
            None => registry.refresh(providers).await,
        };
        serde_json::to_value(response).map_err(anyhow::Error::new)
    }

    pub async fn list_manuscripts(
        &self,
        query: ManuscriptCatalogQuery,
    ) -> Result<ManuscriptCatalogResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::manuscript_handlers::list_manuscripts_catalog(axum::extract::Query(query))
            .await
            .map(|axum::Json(response)| response)
            .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn get_manuscript(&self, manuscript_id: String) -> Result<ManuscriptDetailResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::manuscript_handlers::get_manuscript_detail(axum::extract::Path(manuscript_id))
            .await
            .map(|axum::Json(response)| response)
            .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn create_manuscript(
        &self,
        request: CreateManuscriptRequest,
    ) -> Result<ManuscriptDetailResponse> {
        self.require(Capability::ContentWrite)?;
        crate::manuscript_handlers::create_manuscript(axum::Json(request))
            .await
            .map(|axum::Json(response)| response)
            .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn update_manuscript(
        &self,
        manuscript_id: String,
        request: UpdateManuscriptRequest,
    ) -> Result<ManuscriptDetailResponse> {
        self.require(Capability::ContentWrite)?;
        crate::manuscript_handlers::patch_manuscript_detail(
            axum::extract::Path(manuscript_id),
            axum::Json(request),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn import_manuscripts(
        &self,
        request: ManuscriptImportRequest,
    ) -> Result<ManuscriptImportResponse> {
        self.require(Capability::ContentWrite)?;
        crate::manuscript_handlers::import_manuscripts(axum::Json(request))
            .await
            .map(|axum::Json(response)| response)
            .map_err(|(_, message)| anyhow!(message))
    }

    async fn capability_registry(&self) -> crate::capability_catalog::CapabilityRegistry {
        let mut registry = crate::capability_catalog::CapabilityRegistry::with_loaded_manifest();
        if let Ok(catalog) = self.daemon.mcp_gateway_client.fetch_catalog().await {
            registry.apply_mcp_catalog_sync(&catalog);
        }
        registry
    }

    pub async fn list_capabilities(&self) -> Result<Value> {
        self.require(Capability::WorkshopRead)?;
        serde_json::to_value(self.capability_registry().await.list()).map_err(anyhow::Error::new)
    }

    pub async fn get_capability(&self, capability_id: &str) -> Result<Value> {
        self.require(Capability::WorkshopRead)?;
        let capability_id = capability_id.trim();
        if capability_id.is_empty() {
            bail!("capability_id is required");
        }
        let registry = self.capability_registry().await;
        let response = registry
            .resolve(capability_id)
            .ok_or_else(|| anyhow!("unknown capability '{capability_id}'"))?;
        serde_json::to_value(response).map_err(anyhow::Error::new)
    }

    pub async fn reindex_capabilities(&self) -> Result<Value> {
        self.require(Capability::AdminRuntime)?;
        let (manifest, manifest_loaded_from_file) =
            crate::capability_catalog::load_capability_manifest();
        let mut registry = crate::capability_catalog::CapabilityRegistry::from_manifest(&manifest);
        let gateway_synced =
            if let Ok(catalog) = self.daemon.mcp_gateway_client.fetch_catalog().await {
                registry.apply_mcp_catalog_sync(&catalog);
                true
            } else {
                false
            };
        serde_json::to_value(crate::capability_catalog::CapabilityReindexResponse {
            capability_count: registry.list().capabilities.len(),
            binding_count: registry.binding_count(),
            manifest_path: Some(
                crate::capability_catalog::capabilities_manifest_path()
                    .display()
                    .to_string(),
            ),
            manifest_loaded_from_file,
            gateway_synced,
            now_utc: Utc::now(),
        })
        .map_err(anyhow::Error::new)
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
        self.list_recurring_schedules_filtered(RecurringListQuery::default())
            .await
    }

    pub async fn list_recurring_schedules_filtered(
        &self,
        query: RecurringListQuery,
    ) -> Result<RecurringListResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::recurring_handlers::list_recurring(self.daemon.runtime.as_ref(), query)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn register_prompt_schedule(
        &self,
        request: RegisterRecurringPromptRequest,
    ) -> Result<RegisterRecurringResponse> {
        self.require(Capability::AdminExecute)?;
        let manuscript_id = request
            .manuscript_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let manuscript = manuscript_id
            .as_deref()
            .map(crate::identity_manuscript::build_manuscript_context)
            .transpose()?;
        if let Some(context) = manuscript.as_ref() {
            crate::identity_manuscript::validate_manuscript_for_scheduled_lane(context)?;
        }
        let prompt = if let Some(context) = manuscript.as_ref() {
            crate::identity_manuscript::render_manuscript_task_prompt(
                context,
                Some(request.prompt.as_str()),
            )?
        } else {
            let prompt = request.prompt.trim();
            if prompt.is_empty() {
                bail!("prompt is required");
            }
            prompt.to_string()
        };
        let cron_expr = if request.cron_expr.trim().is_empty() {
            manuscript
                .as_ref()
                .and_then(|context| context.schedule_cron.clone())
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow!("cron_expr is required"))?
        } else {
            request.cron_expr.trim().to_string()
        };
        let timezone = request.timezone.as_deref().unwrap_or("UTC").trim();
        crate::recurring_delivery::validate_recurring_cron(&cron_expr, timezone)?;
        crate::engine_context::validate_lane_action(
            crate::engine_context::EngineExecutionLane::Scheduled,
            crate::engine_context::LaneSafetyActionClass::RecurringRegistration,
        )
        .map_err(anyhow::Error::msg)?;
        crate::engine_context::validate_lane_policy_profile(
            crate::engine_context::EngineExecutionLane::Scheduled,
            request.policy_profile.as_deref(),
        )
        .map_err(anyhow::Error::msg)?;

        let now = Utc::now();
        let recurring_id = request
            .id
            .unwrap_or_else(|| format!("medousa-recurring-{}", Uuid::new_v4().simple()));
        let queue = request.queue.unwrap_or_else(|| "default".to_string());
        let fallback_session_id = request
            .session_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let execution_mode = request
            .execution_mode
            .as_deref()
            .unwrap_or("agent_turn")
            .trim()
            .to_ascii_lowercase();

        let (job_type, payload_template_ref) = match execution_mode.as_str() {
            "prompt" => {
                let payload =
                    stasis::application::orchestration::runtime_job_payloads::PromptJobPayload {
                        user_prompt: crate::engine_context::compile_default_lane_prompt(
                            crate::engine_context::EngineExecutionLane::Scheduled,
                            &prompt,
                        ),
                        system_prompt: request.system_prompt.clone(),
                        policy_profile: request.policy_profile.clone().or_else(|| {
                            Some(
                                crate::engine_context::default_policy_profile_for_lane(
                                    crate::engine_context::EngineExecutionLane::Scheduled,
                                )
                                .to_string(),
                            )
                        }),
                        model_hint: request.model_hint.clone(),
                        reasoning_effort: None,
                        memory_policy: None,
                    };
                (
                    "workflow.stasis.prompt".to_string(),
                    payload.to_payload_ref()?,
                )
            }
            "agent_turn" | "agent-turn" => {
                let max_turns = manuscript
                    .as_ref()
                    .and_then(|context| context.max_tool_rounds);
                let payload = stasis::application::orchestration::runtime_job_payloads::AgentSessionJobPayload {
                    thread_id: Some(
                        manuscript_id
                            .as_deref()
                            .map(|id| format!("manuscript:{id}"))
                            .unwrap_or(fallback_session_id.clone()),
                    ),
                    initial_user_prompt: crate::engine_context::compile_default_lane_prompt(
                        crate::engine_context::EngineExecutionLane::Scheduled,
                        &prompt,
                    ),
                    participants: vec![stasis::application::orchestration::runtime_job_payloads::AgentSessionParticipantPayload {
                        agent_id: manuscript_id.clone().unwrap_or_else(|| "medousa".to_string()),
                        kind: stasis::application::orchestration::runtime_job_payloads::AgentParticipantKindPayload::LocalToolLoop,
                        system_prompt: request.system_prompt.clone(),
                        tool_name: "cognition_capability".to_string(),
                        tool_input: None,
                        endpoint_ref: None,
                        mcp_gateway_ref: None,
                        timeout_seconds: None,
                        poll_interval_seconds: None,
                    }],
                    policy_profile: request.policy_profile.clone().or_else(|| {
                        Some(crate::engine_context::default_policy_profile_for_lane(
                            crate::engine_context::EngineExecutionLane::Scheduled,
                        ).to_string())
                    }),
                    model_hint: request.model_hint.clone(),
                    reasoning_effort: None,
                    max_turns: Some(max_turns.unwrap_or(6).clamp(1, 10)),
                    tool_call_mode: Some(stasis::application::orchestration::runtime_job_payloads::AgentToolCallMode::Auto),
                    memory_policy: None,
                };
                (
                    "workflow.stasis.agent_session".to_string(),
                    payload.to_payload_ref()?,
                )
            }
            other => bail!("execution_mode={other} is invalid; use agent_turn or prompt"),
        };
        let payload_template_ref = crate::recurring_handlers::inject_display_name_into_payload(
            &payload_template_ref,
            request.display_name.as_deref(),
        );
        let definition = crate::recurring_schedule::RecurringScheduleSpec::new(
            recurring_id.clone(),
            queue.clone(),
            job_type,
            payload_template_ref,
            cron_expr,
            timezone.to_string(),
        )
        .jitter_seconds(request.jitter_seconds.unwrap_or(0))
        .enabled(request.enabled.unwrap_or(true))
        .max_attempts(request.max_attempts.unwrap_or(1))
        .build(now)?;
        crate::recurring_delivery::persist_recurring_delivery_binding(
            &recurring_id,
            &json!({ "delivery": request.delivery }),
            crate::recurring_delivery::DeliveryResolveContext {
                ambient: None,
                fallback_session_id,
            },
        )
        .await?;
        crate::recurring_feed::persist_recurring_feed_binding(
            &recurring_id,
            &json!({ "feeds": request.feeds }),
        )
        .await?;
        self.daemon
            .runtime
            .register_recurring(definition.clone())
            .await?;
        Ok(RegisterRecurringResponse {
            recurring_id,
            queue,
            next_run_at_utc: definition.next_run_at,
            cron_expr: definition.cron_expr,
            timezone: definition.timezone,
        })
    }

    pub async fn update_recurring_schedule(
        &self,
        recurring_id: &str,
        request: UpdateRecurringRequest,
    ) -> Result<UpdateRecurringResponse> {
        self.require(Capability::AdminExecute)?;
        crate::recurring_handlers::update_recurring(
            self.daemon.runtime.as_ref(),
            recurring_id,
            request,
        )
        .await
        .map_err(anyhow::Error::new)
    }

    pub async fn delete_recurring_schedule(
        &self,
        recurring_id: &str,
    ) -> Result<DeleteRecurringResponse> {
        self.require(Capability::AdminExecute)?;
        crate::recurring_handlers::delete_recurring(self.daemon.runtime.as_ref(), recurring_id)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn list_recurring_runs(
        &self,
        recurring_id: &str,
        query: RecurringRunsQuery,
    ) -> Result<RecurringRunsResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::recurring_handlers::list_recurring_runs(
            self.daemon.runtime.as_ref(),
            recurring_id,
            query,
        )
        .await
        .map_err(anyhow::Error::new)
    }

    pub async fn recurring_delivery(
        &self,
        recurring_id: &str,
    ) -> Result<RecurringDeliveryResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::recurring_handlers::get_recurring_delivery(recurring_id)
            .await
            .map_err(anyhow::Error::new)
    }

    pub async fn list_workflows(&self, limit: Option<usize>) -> Result<WorkflowsListResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::workflow_handlers::list_workflows(
            axum::extract::State(crate::workflow_handlers::WorkflowApiState {
                composition: self.daemon.runtime.clone(),
            }),
            axum::extract::Query(WorkflowsListQuery { limit }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn get_workflow(&self, workflow_id: String) -> Result<WorkflowDetailResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::workflow_handlers::get_workflow_detail(
            axum::extract::State(crate::workflow_handlers::WorkflowApiState {
                composition: self.daemon.runtime.clone(),
            }),
            axum::extract::Path(workflow_id),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn run_workflow(&self, request: WorkflowRunRequest) -> Result<WorkflowRunResponse> {
        self.require(Capability::AdminExecute)?;
        let response = crate::workflow_handlers::run_workflow(
            axum::extract::State(crate::workflow_handlers::WorkflowApiState {
                composition: self.daemon.runtime.clone(),
            }),
            axum::Json(request),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))?;
        let _ = reconcile_after_unavailability(
            self.daemon.runtime.as_ref(),
            &format!("{}:workflow", self.daemon.cluster_node.node_id),
            &self.daemon.cluster_node.node_id,
            EMBEDDED_RECOVERY_MAX_JOBS,
        )
        .await
        .context("start embedded workflow")?;
        Ok(response)
    }

    pub fn plan_workflow(&self, request: WorkflowPlanRequest) -> Result<WorkflowPlanResponse> {
        self.require(Capability::WorkshopInteract)?;
        Ok(crate::workflow_plan::plan_workflow_from_goal(&request))
    }

    pub async fn schedule_workflow(
        &self,
        request: WorkflowScheduleRequest,
    ) -> Result<WorkflowScheduleResponse> {
        self.require(Capability::AdminExecute)?;
        crate::workflow_handlers::schedule_workflow(
            axum::extract::State(crate::workflow_handlers::WorkflowApiState {
                composition: self.daemon.runtime.clone(),
            }),
            axum::Json(request),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub async fn list_workflow_runs(
        &self,
        workflow_id: String,
        limit: Option<usize>,
    ) -> Result<WorkflowRunsResponse> {
        self.require(Capability::WorkshopRead)?;
        crate::workflow_handlers::list_workflow_runs(
            axum::extract::State(crate::workflow_handlers::WorkflowApiState {
                composition: self.daemon.runtime.clone(),
            }),
            axum::extract::Path(workflow_id),
            axum::extract::Query(WorkflowRunsQuery { limit }),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))
    }

    pub fn list_tool_history(
        &self,
        query: ToolHistoryListQuery,
    ) -> Result<ToolHistoryListResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(crate::tool_history_index::list_tool_history_runs(&query))
    }

    pub async fn workflow_from_slice(
        &self,
        request: WorkflowFromSliceRequest,
    ) -> Result<WorkflowFromSliceResponse> {
        self.require(if request.run {
            Capability::AdminExecute
        } else {
            Capability::WorkshopInteract
        })?;
        let response = crate::tool_history_handlers::workflow_from_slice(
            axum::extract::State(crate::workflow_handlers::WorkflowApiState {
                composition: self.daemon.runtime.clone(),
            }),
            axum::Json(request),
        )
        .await
        .map(|axum::Json(response)| response)
        .map_err(|(_, message)| anyhow!(message))?;
        if response.workflow_id.is_some() {
            let _ = reconcile_after_unavailability(
                self.daemon.runtime.as_ref(),
                &format!("{}:workflow", self.daemon.cluster_node.node_id),
                &self.daemon.cluster_node.node_id,
                EMBEDDED_RECOVERY_MAX_JOBS,
            )
            .await
            .context("start promoted embedded workflow")?;
        }
        Ok(response)
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
        self.create_session_with_request(CreateSessionRequest {
            session_id: None,
            catalog: None,
            member_profile_ids: None,
            agent_profile_id: None,
            display_name: None,
        })
    }

    pub fn create_session_with_request(
        &self,
        request: CreateSessionRequest,
    ) -> Result<CreateSessionResponse> {
        self.require(Capability::WorkshopInteract)?;
        if request.session_id.is_some() {
            bail!("caller-supplied session_id is no longer supported");
        }
        if request
            .catalog
            .as_deref()
            .map(str::trim)
            .is_some_and(|catalog| !catalog.is_empty() && catalog != "single")
        {
            bail!("shared sessions require a Shared workshop");
        }
        if request
            .member_profile_ids
            .as_ref()
            .is_some_and(|members| !members.is_empty())
            || request
                .agent_profile_id
                .as_deref()
                .is_some_and(|profile| !profile.trim().is_empty())
        {
            bail!("member and agent profiles require a shared session");
        }
        let session_id = new_session_id();
        let display_name = request
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        crate::session_catalog::ensure_named_session(session_id.as_str(), display_name.clone());
        Ok(CreateSessionResponse {
            authority_id: self.daemon.authority_id.clone(),
            session_id: session_id.to_string(),
            catalog: "single".to_string(),
            display_name,
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

    pub fn list_sessions_page(
        &self,
        limit: usize,
        query: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<SessionHistoryListResponse> {
        self.require(Capability::ContentRead)?;
        let profile_id = self.active_profile_id()?;
        let page = crate::session::list_history_sessions_page_for_profile(
            Some(&profile_id),
            limit.clamp(1, 1000),
            query.map(str::trim).filter(|value| !value.is_empty()),
            cursor.map(str::trim).filter(|value| !value.is_empty()),
        );
        Ok(SessionHistoryListResponse {
            sessions: page.sessions,
            next_cursor: page.next_cursor,
        })
    }

    pub fn set_session_display_name(
        &self,
        session_id: &str,
        display_name: &str,
    ) -> Result<SessionSetDisplayNameResponse> {
        self.require(Capability::ContentWrite)?;
        let session_id = SessionId::parse(session_id).map_err(anyhow::Error::new)?;
        crate::session::set_session_display_name(session_id.as_str(), display_name)
            .map_err(anyhow::Error::msg)?;
        Ok(SessionSetDisplayNameResponse {
            session_id: session_id.to_string(),
            display_name: crate::session::get_session_display_name(session_id.as_str())
                .unwrap_or_else(|| display_name.trim().to_string()),
        })
    }

    pub async fn derive_session(
        &self,
        request: DeriveSessionRequest,
        idempotency_key: &str,
    ) -> Result<DeriveSessionResponse> {
        self.require(Capability::ContentWrite)?;
        crate::context_derivation::derive_session(&self.principal, request, idempotency_key)
            .await
            .map_err(|error| anyhow!(error.message))
    }

    pub async fn delete_session(
        &self,
        session_id: &str,
        purge_memory: bool,
    ) -> Result<SessionDeleteResponse> {
        self.require(Capability::ContentWrite)?;
        let summary = crate::session_lifecycle::delete_session(
            session_id,
            Some(self.daemon.memory_operations.clone()),
            &self.daemon.turn_tickets,
            Some(&self.daemon.turn_streams),
            purge_memory,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        Ok(SessionDeleteResponse {
            session_id: summary.session_id,
            deletion_id: summary.deletion_id,
            status: summary.status,
            deleted: summary.deleted,
            locus_purged: summary.locus_purged,
            locus_nodes_deleted: summary.locus_nodes_deleted,
            cancelled_active_turn: summary.cancelled_active_turn,
            surfaces: summary.surfaces,
        })
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

    pub fn load_transcript_entries_page(
        &self,
        session_id: &str,
        limit: usize,
        before_entry_seq: Option<u64>,
    ) -> Result<crate::session_store::TranscriptPage> {
        self.require(Capability::ContentRead)?;
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        Ok(self.daemon.session_store.load_transcript_entries_page(
            &session_id,
            limit,
            before_entry_seq,
        ))
    }

    pub fn session_agent_mode(&self, session_id: &str) -> Result<SessionAgentModeResponse> {
        self.require(Capability::WorkshopRead)?;
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        crate::agent_mode_state::get_session_mode(session_id.as_str()).map_err(anyhow::Error::msg)
    }

    pub fn list_agent_modes(&self) -> Result<AgentModeListResponse> {
        self.require(Capability::WorkshopRead)?;
        Ok(AgentModeListResponse {
            modes: vec![
                medousa_types::daemon_api::AgentModeAvailability {
                    mode: medousa_types::daemon_api::AgentModeId::General,
                    label: "General".to_string(),
                    available: true,
                    contract_revision: Some("general-v1".to_string()),
                    unavailable_reason: None,
                },
                medousa_types::daemon_api::AgentModeAvailability {
                    mode: medousa_types::daemon_api::AgentModeId::Instant,
                    label: "Instant".to_string(),
                    available: true,
                    contract_revision: Some(
                        crate::agent_mode_context::INSTANT_CONTRACT_REVISION.to_string(),
                    ),
                    unavailable_reason: None,
                },
                medousa_types::daemon_api::AgentModeAvailability {
                    mode: medousa_types::daemon_api::AgentModeId::Coder,
                    label: "Coder".to_string(),
                    available: false,
                    contract_revision: None,
                    unavailable_reason: Some(
                        "Code work needs a Shared workshop host with Forge, a shell, and project filesystem authority."
                            .to_string(),
                    ),
                },
            ],
        })
    }

    pub fn agent_mode_transition_policy(&self) -> Result<AgentModeTransitionPolicy> {
        self.require(Capability::WorkshopRead)?;
        Ok(crate::agent_mode_state::get_transition_policy())
    }

    pub fn set_agent_mode_transition_policy(
        &self,
        policy: AgentModeTransitionPolicy,
    ) -> Result<AgentModeTransitionPolicy> {
        self.require(Capability::AdminRuntime)?;
        crate::agent_mode_state::set_transition_policy(policy).map_err(anyhow::Error::msg)
    }

    pub fn set_session_agent_mode(
        &self,
        session_id: &str,
        request: SetSessionAgentModeRequest,
    ) -> Result<SessionAgentModeResponse> {
        self.require(Capability::ContentWrite)?;
        let session_id = SessionId::parse(session_id).map_err(anyhow::Error::new)?;
        crate::agent_mode_state::set_session_mode(session_id.as_str(), request)
            .map_err(anyhow::Error::msg)
    }

    pub fn list_agent_mode_proposals(
        &self,
        session_id: &str,
    ) -> Result<AgentModeProposalListResponse> {
        self.require(Capability::ContentRead)?;
        let session_id = SessionId::parse(session_id).map_err(anyhow::Error::new)?;
        crate::agent_mode_state::list_mode_proposals(session_id.as_str())
            .map_err(anyhow::Error::msg)
    }

    pub fn decide_agent_mode_proposal(
        &self,
        session_id: &str,
        proposal_id: &str,
        accept: bool,
    ) -> Result<AgentModeProposalResponse> {
        self.require(Capability::ContentWrite)?;
        let session_id = SessionId::parse(session_id).map_err(anyhow::Error::new)?;
        crate::agent_mode_state::decide_mode_proposal(session_id.as_str(), proposal_id, accept)
            .map_err(anyhow::Error::msg)
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
        self.start_turn_with_options(
            session_id,
            prompt,
            identity_user_id,
            channel_surface,
            voice_preset_id,
            voice_appendix,
            "standard".to_string(),
            "default".to_string(),
            Vec::new(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_turn_with_options(
        &self,
        session_id: &str,
        prompt: impl Into<String>,
        identity_user_id: Option<String>,
        channel_surface: Option<String>,
        voice_preset_id: Option<String>,
        voice_appendix: Option<String>,
        response_depth_mode: String,
        reasoning_effort: String,
        media_refs: Vec<medousa_types::daemon_api::MediaRef>,
    ) -> Result<InteractiveTurnResponse> {
        self.require(Capability::WorkshopInteract)?;
        if self.daemon.backgrounded.load(Ordering::Acquire) {
            bail!("embedded daemon is backgrounded");
        }
        let session_id = SessionId::parse(session_id).map_err(|error| anyhow!(error))?;
        let prompt = prompt.into();
        if prompt.trim().is_empty() && media_refs.is_empty() {
            bail!("turn prompt cannot be empty");
        }
        if prompt.chars().count() > MAX_REQUEST_PROMPT_CHARS {
            bail!("turn prompt exceeds the foreground prompt limit");
        }
        crate::media_store::validate_media_refs(session_id.as_str(), &media_refs)
            .map_err(anyhow::Error::msg)?;
        let (provider, model) = self.daemon.inference.route();
        let vision_plan = crate::media_vision::plan_turn_media(
            session_id.as_str(),
            &media_refs,
            &provider,
            &model,
        )
        .map_err(anyhow::Error::msg)?;
        let effective_prompt = crate::media_store::merge_media_refs_into_prompt(
            &prompt,
            session_id.as_str(),
            &media_refs,
            &vision_plan.merge_options,
        );
        let compiled_prompt = crate::engine_context::compile_context_prompt(
            crate::engine_context::ContextCompilerInput {
                lane: crate::engine_context::EngineExecutionLane::Interactive,
                user_prompt: &effective_prompt,
                response_depth_mode: &response_depth_mode,
                stage_route: None,
                recall_readiness: crate::engine_context::RecallReadiness::Missing,
            },
        )
        .compiled_prompt;
        let inference_prompt = medousa_runtime::append_voice_preset_hint(
            &crate::text_budget::truncate_text_for_budget(
                &compiled_prompt,
                MAX_REQUEST_PROMPT_CHARS,
            ),
            voice_preset_id.as_deref(),
            voice_appendix.as_deref(),
        );
        let current_turn_user_message = vision_plan.build_user_message(&inference_prompt);
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

        let agent_mode = crate::agent_mode_state::resolve_for_turn(session_id.as_str(), None).mode;
        let prior_messages = history_to_chat_messages(
            self.daemon.session_store.load_history(&session_id),
            agent_mode,
        );
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
        let scope = TurnContinuationScope {
            turn_correlation_id: turn_id.clone(),
            session_id: session_id.to_string(),
            identity_user_id: Some(identity_user_id),
            original_prompt: prompt.clone(),
            delivery_target: None,
            provider: provider.clone(),
            model: model.clone(),
            response_depth_mode: response_depth_mode.clone(),
            supports_ui_artifacts: true,
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
                ui_artifacts: true,
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
                .execute_foreground_turn(
                    lease,
                    prompt,
                    inference_prompt,
                    current_turn_user_message,
                    media_refs,
                    reasoning_effort,
                    agent_mode,
                    prior_messages,
                    stream,
                )
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

fn history_to_chat_messages(
    history: Vec<ConversationTurn>,
    agent_mode: AgentModeId,
) -> Vec<ChatMessage> {
    let Some(limits) = crate::agent_mode_context::context_limits_for_mode(agent_mode) else {
        return history
            .into_iter()
            .filter_map(conversation_turn_to_chat_message)
            .collect();
    };

    let mut remaining = limits.max_prior_total_chars;
    let mut messages = Vec::new();
    for mut turn in history.into_iter().rev().take(limits.hot_window_turns) {
        if remaining == 0 {
            break;
        }
        let message_budget = limits.max_single_prior_message_chars.min(remaining);
        turn.content = crate::text_budget::truncate_text_for_budget(&turn.content, message_budget);
        let message_chars = turn.content.chars().count();
        if let Some(message) = conversation_turn_to_chat_message(turn) {
            remaining = remaining.saturating_sub(message_chars);
            messages.push(message);
        }
    }
    messages.reverse();
    messages
}

fn conversation_turn_to_chat_message(turn: ConversationTurn) -> Option<ChatMessage> {
    match turn.role.as_str() {
        "user" => Some(ChatMessage::user(turn.content)),
        "assistant" | "agent" => Some(ChatMessage::assistant(turn.content)),
        "system" => Some(ChatMessage::system(turn.content)),
        _ => None,
    }
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
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use async_trait::async_trait;
    use genai::ModelIden;
    use genai::adapter::AdapterKind;
    use genai::chat::{ChatOptions, ChatRequest, ChatResponse, MessageContent, ToolCall};
    use medousa_engine::{
        TURN_PIPELINE_BYTE_CAPACITY, TurnPipelineEmission, TurnPipelineError, TurnPipelineOutput,
    };
    use stasis::domain::errors::Result as StasisResult;
    use stasis::domain::runtime::job::JobState;
    use tokio::sync::Semaphore;

    use super::*;
    use crate::request_principal::PrincipalKind;

    const INSTALLATION_ID: &str = crate::workshop_authority::TEST_INSTALLATION_ID;
    const SECRET_CANARY: &str = "embedded-secret-must-never-escape";
    const FIRST_REPLY: &str = "The embedded daemon owns this foreground turn.";
    const BACKGROUND_REPLY: &str = "The turn survived the app lifecycle transition.";
    const GRAPHEME_REPLY: &str = "The portable Grapheme workflow completed on the phone daemon.";
    const GRAPHEME_SOURCE: &str = r#"import core from "grapheme/core"

query MobileProbe {
    core.echo(message: "embedded phase four") {
        state { current }
    }
}
"#;

    struct DiscardTurnOutput;

    impl TurnPipelineOutput for DiscardTurnOutput {
        async fn publish(
            &self,
            _emission: TurnPipelineEmission,
        ) -> std::result::Result<(), TurnPipelineError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn embedded_terminal_tool_message_follows_earlier_interim_prose() {
        let pipeline = TurnPipelineHandle::spawn(
            "embedded-terminal-message",
            0,
            Arc::new(Semaphore::new(TURN_PIPELINE_BYTE_CAPACITY * 2)),
            Arc::new(DiscardTurnOutput),
        );
        let chronological =
            EmbeddedChronologicalTurn::new("embedded-terminal-message", pipeline.clone());

        chronological
            .content_delta("I found the likely cause.".into())
            .await
            .expect("publish interim prose");
        chronological
            .commit_active(true)
            .await
            .expect("commit interim prose");
        let body = chronological
            .terminal_body("The fix is ready.")
            .await
            .expect("commit terminal tool message");

        assert_eq!(body, "I found the likely cause.\n\nThe fix is ready.");
        pipeline.cancel();
    }

    #[test]
    fn embedded_prompt_uses_general_sttp_and_tool_hud() {
        let prompt = embedded_system_prompt(AgentModeId::General);
        assert!(prompt.contains("p1_core(.99)"));
        assert!(prompt.contains("p2_mode_general(.99)"));
        assert!(prompt.contains("p3_actor_host(.99)"));
        assert!(prompt.contains("[MEDOUSA_HUD]"));
        assert!(prompt.contains("catalog_tool=cognition_tools_discover"));
        assert!(prompt.contains("web_tool=cognition_web_search"));
    }

    #[test]
    fn instant_embedded_prompt_keeps_general_policy_with_smaller_hud() {
        let prompt = embedded_system_prompt(AgentModeId::Instant);
        assert!(prompt.contains("p2_mode_general(.99)"));
        assert!(prompt.contains("mode=instant"));
        assert!(prompt.contains("web_tool=cognition_web_search"));
        assert!(prompt.contains("capability_tool=cognition_capability"));
        assert!(prompt.contains("mcp_actions=mcp.find|mcp.invoke"));
        assert!(prompt.contains("schema_tool=cognition_schema"));
        assert!(!prompt.contains("catalog_tool=cognition_tools_discover"));
    }

    #[test]
    fn instant_embedded_history_only_loads_the_recent_window() {
        let history = (0..12)
            .map(|index| crate::turn_parts::user_conversation_turn(format!("turn {index}")))
            .collect();
        let messages = history_to_chat_messages(history, AgentModeId::Instant);
        assert_eq!(messages.len(), 6);
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
        background_reply: tokio::sync::Notify,
    }

    impl LifecycleChatClient {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::Acquire)
        }

        fn release_background_turn(&self) {
            self.background_reply.notify_one();
        }

        async fn next(&self) -> StasisResult<ChatResponse> {
            match self.calls.fetch_add(1, Ordering::AcqRel) {
                0 => Ok(text_response(FIRST_REPLY)),
                1 => {
                    self.background_reply.notified().await;
                    Ok(text_response(BACKGROUND_REPLY))
                }
                2 => Ok(tool_response(
                    "cognition_capability",
                    json!({ "action": "grapheme.invoke", "script": GRAPHEME_SOURCE }),
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

    struct EmptyChatGptStore;

    impl crate::chatgpt_oauth::ChatGptCredentialStore for EmptyChatGptStore {
        fn load_bundle(&self) -> std::result::Result<Option<String>, String> {
            Ok(None)
        }

        fn save_bundle(&self, _bundle: Option<&str>) -> std::result::Result<(), String> {
            Ok(())
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

    #[test]
    fn routed_binding_switches_between_api_key_and_chatgpt_accounts() {
        let broker = Arc::new(crate::chatgpt_oauth::ChatGptOAuthBroker::new(Arc::new(
            EmptyChatGptStore,
        )));
        let routed = EmbeddedRoutedChatClient::new(
            "openai".to_string(),
            "gpt-5.4-mini".to_string(),
            None,
            Arc::new(CanaryCredentialProvider),
            broker,
            None,
        )
        .expect("routed client");
        let binding = EmbeddedInferenceBinding::Routed(routed);

        binding
            .reconfigure("openai-codex", "gpt-5.6-sol", None)
            .expect("select ChatGPT account route");
        assert_eq!(
            binding.route(),
            ("openai-codex".to_string(), "gpt-5.6-sol".to_string())
        );

        binding
            .reconfigure("anthropic", "claude-sonnet-4-6", None)
            .expect("return to API-key route");
        assert_eq!(
            binding.route(),
            ("anthropic".to_string(), "claude-sonnet-4-6".to_string())
        );
    }

    #[derive(Default)]
    struct NativeInferenceProbe {
        requests: Mutex<Vec<EmbeddedNativeChatRequest>>,
    }

    #[async_trait]
    impl EmbeddedNativeInference for NativeInferenceProbe {
        async fn generate(
            &self,
            request: EmbeddedNativeChatRequest,
            events: Option<mpsc::Sender<EmbeddedNativeInferenceEvent>>,
        ) -> std::result::Result<EmbeddedNativeChatResponse, String> {
            self.requests
                .lock()
                .expect("native inference probe lock")
                .push(request);
            if let Some(events) = events {
                events
                    .send(EmbeddedNativeInferenceEvent::Content {
                        text: "Native ".to_string(),
                    })
                    .await
                    .map_err(|_| "native stream closed".to_string())?;
                events
                    .send(EmbeddedNativeInferenceEvent::Reasoning {
                        text: "checking locally".to_string(),
                    })
                    .await
                    .map_err(|_| "native stream closed".to_string())?;
            }
            Ok(EmbeddedNativeChatResponse {
                content: "Native answer".to_string(),
                tool_calls: vec![EmbeddedNativeToolCall {
                    id: "native-call-1".to_string(),
                    name: "lookup_weather".to_string(),
                    arguments: json!({ "city": "Phoenix" }),
                }],
                prompt_tokens: Some(12),
                completion_tokens: Some(7),
                stop_reason: Some("tool_calls".to_string()),
            })
        }
    }

    #[test]
    fn native_inference_hoists_system_messages_for_strict_templates() {
        let inference = Arc::new(NativeInferenceProbe::default());
        let client = EmbeddedNativeChatClient::new("qwen3.5-2b-4bit".to_string(), inference);
        let request = ChatRequest::new(vec![
            ChatMessage::system("Primary policy"),
            ChatMessage::user("Hello"),
            ChatMessage::system("Round control"),
        ])
        .with_system("Host policy");

        let native = client.native_request(request, None);

        assert_eq!(
            native.system.as_deref(),
            Some("Host policy\n\nPrimary policy\n\nRound control")
        );
        assert_eq!(native.messages.len(), 1);
        assert_eq!(native.messages[0].role, "user");
        assert_eq!(native.messages[0].content, "Hello");
    }

    #[tokio::test]
    async fn native_inference_preserves_messages_tools_options_and_streams() {
        let inference = Arc::new(NativeInferenceProbe::default());
        let client =
            EmbeddedNativeChatClient::new("gemma-4-e2b-it-4bit".to_string(), inference.clone());
        let previous_call = ToolCall {
            call_id: "previous-call".to_string(),
            fn_name: "lookup_weather".to_string(),
            fn_arguments: json!({ "city": "Tempe" }),
            thought_signatures: None,
        };
        let request = ChatRequest::new(vec![
            ChatMessage::user(MessageContent::from_parts(vec![
                ContentPart::Text("What is outside?".to_string()),
                ContentPart::from_binary_base64(
                    "image/png",
                    "aW1hZ2U=",
                    Some("window.png".to_string()),
                ),
            ])),
            ChatMessage::assistant(MessageContent::from_tool_calls(vec![previous_call.clone()])),
            ChatMessage::tool(MessageContent::from_tool_responses(vec![
                genai::chat::ToolResponse::from_tool_call(&previous_call, "sunny"),
            ])),
        ])
        .with_system("Keep the response private.")
        .with_tools([genai::chat::Tool::new("lookup_weather")
            .with_description("Read current conditions")
            .with_schema(json!({
                "type": "object",
                "properties": { "city": { "type": "string" } }
            }))]);
        let options = ChatOptions::default()
            .with_temperature(0.2)
            .with_top_p(0.8)
            .with_max_tokens(96)
            .with_stop_sequence("DONE");
        let (delta_tx, mut delta_rx) = mpsc::channel(4);

        let response = client
            .complete_stream(request, Some(&options), Some(&delta_tx))
            .await
            .expect("native inference response");

        assert!(matches!(
            delta_rx.recv().await,
            Some(StreamDelta::Content(value)) if value == "Native "
        ));
        assert!(matches!(
            delta_rx.recv().await,
            Some(StreamDelta::Reasoning(value)) if value == "checking locally"
        ));
        assert_eq!(response.first_text(), Some("Native answer"));
        let returned_call = response
            .content
            .parts()
            .iter()
            .find_map(ContentPart::as_tool_call)
            .expect("native tool call");
        assert_eq!(returned_call.call_id, "native-call-1");
        assert_eq!(returned_call.fn_arguments, json!({ "city": "Phoenix" }));
        assert_eq!(response.usage.total_tokens, Some(19));

        let requests = inference
            .requests
            .lock()
            .expect("native inference probe lock");
        let request = requests.first().expect("captured native request");
        assert_eq!(request.model, "gemma-4-e2b-it-4bit");
        assert_eq!(
            request.system.as_deref(),
            Some("Keep the response private.")
        );
        assert_eq!(request.messages[0].attachments[0].content_type, "image/png");
        assert_eq!(request.messages[1].tool_calls[0].id, "previous-call");
        assert_eq!(
            request.messages[2].tool_call_id.as_deref(),
            Some("previous-call")
        );
        assert_eq!(request.tools[0].name, "lookup_weather");
        assert_eq!(request.options.max_tokens, Some(96));
        assert_eq!(request.options.stop_sequences, ["DONE"]);
    }

    #[test]
    fn native_route_requires_a_native_host_and_rejects_base_urls() {
        let broker = Arc::new(crate::chatgpt_oauth::ChatGptOAuthBroker::new(Arc::new(
            EmptyChatGptStore,
        )));
        let missing_host = EmbeddedRoutedChatClient::new(
            EMBEDDED_NATIVE_LOCAL_PROVIDER_ID.to_string(),
            "gemma-4-e2b-it-4bit".to_string(),
            None,
            Arc::new(CanaryCredentialProvider),
            broker.clone(),
            None,
        );
        assert!(missing_host.is_err());

        let with_base_url = EmbeddedRoutedChatClient::new(
            EMBEDDED_NATIVE_LOCAL_PROVIDER_ID.to_string(),
            "gemma-4-e2b-it-4bit".to_string(),
            Some("http://127.0.0.1:7417/v1".to_string()),
            Arc::new(CanaryCredentialProvider),
            broker,
            Some(Arc::new(NativeInferenceProbe::default())),
        );
        assert!(with_base_url.is_err());
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
    async fn embedded_catalog_tracks_and_repairs_durable_transcripts() {
        let sandbox = tempfile::tempdir().expect("embedded catalog sandbox");
        let installation_id = InstallationId::parse(INSTALLATION_ID).expect("installation id");
        let daemon = EmbeddedDaemon::boot(
            EmbeddedDaemonConfig::with_chat_client(
                sandbox.path(),
                installation_id,
                "openai",
                "embedded-test-model",
                Arc::new(LifecycleChatClient::default()),
            )
            .with_tool_registry_recipe(Arc::new(
                crate::mobile_tool_registry::PersonalMobileToolRegistryRecipe,
            )),
        )
        .await
        .expect("boot embedded catalog daemon");
        let client = daemon.local_client();
        let session = client.create_session().expect("create embedded session");

        let accepted = client
            .start_turn(&session.session_id, "keep this conversation")
            .await
            .expect("start embedded turn");
        let events = collect_to_eof(
            client
                .subscribe_turn(&accepted.turn_id, 0)
                .await
                .expect("subscribe embedded turn"),
        )
        .await;
        assert!(matches!(
            events.last().map(|event| &event.event),
            Some(TurnStreamEventV2::Final { .. })
        ));
        let summary = client
            .list_sessions_page(10, None, None)
            .expect("list embedded sessions")
            .sessions
            .into_iter()
            .find(|summary| summary.session_id == session.session_id)
            .expect("new session remains in paginated history");
        assert_eq!(summary.turns, 2);
        assert!(summary.last_timestamp.is_some());

        // Recreate the pre-fix state and prove the one-time migration restores
        // the already-durable conversation without replaying the turn.
        let parsed_session_id = SessionId::parse(&session.session_id).expect("parse test session");
        crate::session_catalog::delete_catalog_row(&parsed_session_id)
            .expect("remove current catalog projection");
        crate::session_catalog::ensure_named_session(&session.session_id, None);
        assert_eq!(
            crate::session_catalog::turn_count(&session.session_id),
            Some(0)
        );
        let repair_marker = sandbox
            .path()
            .join(crate::session_catalog::EMBEDDED_TRANSCRIPT_PROJECTION_REPAIR_MARKER);
        if repair_marker.is_file() {
            std::fs::remove_file(&repair_marker).expect("reset projection repair marker");
        }
        crate::session_catalog::repair_embedded_transcript_projection_once()
            .expect("repair embedded transcript projection");
        let repaired = client
            .list_sessions_page(10, None, None)
            .expect("list repaired embedded sessions")
            .sessions
            .into_iter()
            .find(|summary| summary.session_id == session.session_id)
            .expect("repaired session remains in paginated history");
        assert_eq!(repaired.turns, 2);
        assert!(repaired.last_timestamp.is_some());
        assert!(repair_marker.is_file());

        drop(client);
        crate::session_store::reset_session_store_for_test();
        assert_eq!(Arc::strong_count(&daemon), 1);
        drop(daemon);
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
            .await
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
                .all(|name| tool_names.contains(name)),
            "missing expected mobile tools: {:?}",
            crate::mobile_tool_registry::PERSONAL_MOBILE_TOOL_NAMES
                .iter()
                .filter(|name| !tool_names.contains(name))
                .collect::<Vec<_>>()
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

        let background_turn = client
            .start_turn(
                &session.session_id,
                "this turn should survive backgrounding",
            )
            .await
            .expect("start foreground turn before backgrounding");
        wait_until(|| chat.calls() >= 2).await;
        assert_eq!(daemon.live_turn_count(), 1);
        assert_eq!(daemon.enter_background(), 1);
        assert_eq!(daemon.live_turn_count(), 1);
        assert!(
            client
                .start_turn(&session.session_id, "background admission must fail closed")
                .await
                .expect_err("backgrounded daemon accepted work")
                .to_string()
                .contains("backgrounded")
        );
        let wake = daemon.resume().await.expect("resume embedded daemon");
        assert_eq!(wake.materialized, 0);
        assert_eq!(daemon.live_turn_count(), 1);
        chat.release_background_turn();
        let background_events = collect_to_eof(
            client
                .subscribe_turn(&background_turn.turn_id, 0)
                .await
                .expect("reattach backgrounded turn"),
        )
        .await;
        assert!(matches!(
            background_events.last().map(|event| &event.event),
            Some(TurnStreamEventV2::Final { text, tool_names })
                if text == BACKGROUND_REPLY && tool_names.is_empty()
        ));
        assert_eq!(
            background_events
                .iter()
                .filter(|event| event.event.is_terminal())
                .count(),
            1
        );
        let transcript_after_background = client
            .load_transcript_entries(&session.session_id)
            .expect("load transcript after background resume");
        assert_eq!(transcript_after_background.len(), 4);
        assert_eq!(transcript_after_background[2].turn.role, "user");
        assert_eq!(transcript_after_background[3].turn.role, "assistant");
        assert_eq!(
            transcript_after_background[3]
                .caused_by
                .as_ref()
                .expect("resumed assistant execution")
                .execution_id
                .as_str(),
            background_turn.turn_id
        );
        assert!(
            !client
                .active_turn(&session.session_id)
                .await
                .expect("read resumed turn state")
                .active
        );

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
                    && tool_names.iter().any(|name| name == "cognition_capability")
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
        // Several missed ticks still materialize one catch-up job. Stasis
        // advances the definition from wake time instead of replaying every
        // interval the mobile process could not execute.
        due_definition.next_run_at = Utc::now() - chrono::Duration::days(3);
        daemon
            .runtime
            .save_recurring(due_definition)
            .await
            .expect("make embedded schedule due");

        assert_eq!(daemon.enter_background(), 0);
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

        assert_eq!(daemon.enter_background(), 0);
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
        assert_eq!(rebooted_transcript.len(), 7);
        assert_eq!(rebooted_transcript[2].turn.role, "user");
        assert_eq!(rebooted_transcript[3].turn.role, "assistant");
        assert_eq!(rebooted_transcript[3].turn.content, BACKGROUND_REPLY);
        assert_eq!(rebooted_transcript[4].turn.role, "user");
        assert_eq!(rebooted_transcript[5].turn.role, "assistant");
        assert_eq!(rebooted_transcript[5].turn.content, GRAPHEME_REPLY);
        assert_eq!(rebooted_transcript[6].turn.role, "assistant");
        assert_eq!(rebooted_transcript[6].turn.content, RECOVERED_REPLY);
        assert_eq!(
            rebooted_transcript[6]
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
        let rebooted_background_replay = rebooted_client
            .replay_turn(&background_turn.turn_id, 0)
            .await
            .expect("reload background-surviving journal after reboot");
        assert_eq!(
            rebooted_background_replay
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
            7,
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
