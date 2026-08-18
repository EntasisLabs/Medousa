use std::collections::HashSet;
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use locus_core_rs::NodeStore;
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use stasis::application::orchestration::tool_registry::{StasisTool, ToolRegistry};
use stasis::domain::runtime::job_attempt::JobAttemptOutcome;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::prelude::{RuntimeBackend, RuntimeComposition, StasisError};
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};

use crate::capability_catalog::{
    CapabilityListResponse, CapabilityRegistry, CapabilityResolveResponse, CapabilitySearchMatch,
    CapabilitySearchResponse,
};
use crate::engine_context::{
    EngineExecutionLane, LaneSafetyActionClass, validate_lane_action, validate_lane_policy_profile,
};
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::{
    GraphemeCompactionModelTarget, maybe_compact_output_to_sttp,
};
use crate::mcp_gateway_api::{McpDiscoverRequest, McpInvokeRequest, McpTurnContext, McpTurnLane};
use crate::mcp_gateway_client::McpGatewayClient;
use crate::mcp_turn_token::mint_mcp_turn_token;
use crate::process_once;
use crate::recurring_delivery::{
    DeliveryResolveContext, RecurringDeliverySpec, ambient_from_turn_scope,
    bind_recurring_delivery_spec_for_registration,
};
use crate::recurring_feed::{RecurringFeedSpec, bind_recurring_feed_spec_for_registration};
use crate::recurring_schedule::RecurringScheduleSpec;
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::runtime_job_spec::ToolJobSpec;
use crate::tui::runtime_services::{
    build_tool_loop_pipeline_for_target, build_tui_runtime_services,
};
use crate::turn_continuation::{
    self, ContinuationAwaitMode, continuation_tool_metadata, wire_turn_child_job,
};
use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};

const COGNITION_JOB_ENQUEUE_ID: ToolId = ToolId::new("cognition_job_enqueue");
const COGNITION_GRAPHEME_RUN_ID: ToolId = ToolId::new("cognition_grapheme_run");
const COGNITION_GRAPHEME_MODULES_ID: ToolId = ToolId::new("cognition_grapheme_modules");
const COGNITION_GRAPHEME_MODULES_INFO_ID: ToolId = ToolId::new("cognition_grapheme_modules_info");
const COGNITION_GRAPHEME_MODULES_OPS_ID: ToolId = ToolId::new("cognition_grapheme_modules_ops");
const COGNITION_GRAPHEME_EXAMPLES_ID: ToolId = ToolId::new("cognition_grapheme_examples");
const COGNITION_GRAPHEME_CLI_RUN_ID: ToolId = ToolId::new("cognition_grapheme_cli_run");
const COGNITION_GRAPHEME_PROMOTE_TO_JOB_ID: ToolId =
    ToolId::new("cognition_grapheme_promote_to_job");
const COGNITION_GRAPHEME_PROMOTE_TO_RECURRING_ID: ToolId =
    ToolId::new("cognition_grapheme_promote_to_recurring");
const COGNITION_GRAPHEME_PROMOTE_LAST_RUN_TO_RECURRING_ID: ToolId =
    ToolId::new("cognition_grapheme_promote_last_run_to_recurring");
const COGNITION_MCP_INVOKE_ID: ToolId = ToolId::new("cognition_mcp_invoke");

async fn run_grapheme_cli(args: Vec<String>) -> stasis::prelude::Result<Value> {
    let cmdline = format!("grapheme {}", args.join(" "));
    let output = tokio::task::spawn_blocking(move || Command::new("grapheme").args(&args).output())
        .await
        .map_err(|e| StasisError::PortFailure(format!("grapheme cli task join error: {e}")))
        .and_then(|res| {
            res.map_err(|e| {
                StasisError::PortFailure(format!("failed to execute grapheme cli: {e}"))
            })
        })?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(json!({
        "command": cmdline,
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "succeeded": output.status.success()
    }))
}

fn grapheme_inline_payload_source(payload_ref: &str) -> Option<&str> {
    payload_ref.strip_prefix("grapheme:inline:")
}

fn truncate_for_error(text: &str, max_chars: usize) -> String {
    let out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}

pub(crate) async fn run_grapheme_via_runtime(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
    causation: &str,
) -> stasis::prelude::Result<Value> {
    let job_id = format!("cognition-gph-runtime-{}", Uuid::new_v4().simple());
    let now = Utc::now();

    let job = ToolJobSpec::new(
        job_id.clone(),
        "default",
        "workflow.grapheme.run",
        format!("grapheme:inline:{source}"),
        causation,
        "sttp:in:cognition:grapheme:runtime",
        now,
    )
    .build();

    runtime.enqueue_job(job).await?;

    let _ = process_once(runtime, causation)
        .await
        .map_err(|e| StasisError::PortFailure(format!("runtime process_once failed: {e}")))?;

    let attempts = runtime.as_ref().list_job_attempts(&job_id).await?;

    let last = attempts.last().ok_or_else(|| {
        StasisError::PortFailure(
            "runtime preflight did not produce a job attempt for grapheme source".to_string(),
        )
    })?;

    let succeeded = last.outcome == JobAttemptOutcome::Succeeded;
    let diagnostics = last
        .diagnostics
        .as_deref()
        .and_then(|d| serde_json::from_str::<Value>(d).ok())
        .unwrap_or_else(|| json!({ "raw": last.diagnostics.clone().unwrap_or_default() }));

    Ok(json!({
        "mode": "runtime",
        "job_id": job_id,
        "succeeded": succeeded,
        "attempt_outcome": format!("{:?}", last.outcome),
        "execution_id": last.execution_id,
        "diagnostics": diagnostics
    }))
}

pub(crate) async fn validate_grapheme_source_for_schedule(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
) -> stasis::prelude::Result<Value> {
    let result = run_grapheme_via_runtime(runtime, source, "cognition_tui_preflight").await?;
    let succeeded = result
        .get("succeeded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let diagnostics_value = result
        .get("diagnostics")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let diagnostics_preview = truncate_for_error(
        &serde_json::to_string_pretty(&diagnostics_value).unwrap_or_else(|_| "{}".to_string()),
        1600,
    );

    Ok(json!({
        "validated": succeeded,
        "mode": "runtime_preflight",
        "job_id": result.get("job_id").cloned().unwrap_or(Value::Null),
        "execution_id": result.get("execution_id").cloned().unwrap_or(Value::Null),
        "attempt_outcome": result.get("attempt_outcome").cloned().unwrap_or(Value::Null),
        "diagnostics": diagnostics_value,
        "diagnostics_preview": diagnostics_preview
    }))
}

async fn remember_last_grapheme_source(source: &str) {
    if let Some(context) = crate::agent_runtime::execution_context::active_turn_execution_context()
    {
        context.remember_grapheme_source(source);
    }
}

async fn read_last_grapheme_source() -> Option<String> {
    crate::agent_runtime::execution_context::active_turn_execution_context()
        .and_then(|context| context.last_grapheme_source())
        .map(|source| source.to_string())
}

async fn emit_compaction_observability(
    event_tx: &mpsc::Sender<TuiEvent>,
    tool_name: &str,
    output: &Value,
    raw_output_bytes: Option<usize>,
) {
    let trigger_bytes = std::env::var("MEDOUSA_GRAPHEME_COMPACTION_TRIGGER_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(24 * 1024)
        .max(1024);
    let inline_notice_enabled = std::env::var("MEDOUSA_GRAPHEME_COMPACTION_INLINE_NOTICE")
        .ok()
        .map(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(true);

    if output
        .get("mode")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        != "sttp_compaction"
    {
        if inline_notice_enabled && let Some(size) = raw_output_bytes {
            let _ = event_tx
                .send(TuiEvent::UiNotice(format!(
                    "◈ sttp_compaction tool={} status=inline bytes={} trigger_bytes={}",
                    tool_name, size, trigger_bytes
                )))
                .await;
        }
        return;
    }

    let status = output
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let artifact_id = output
        .get("original_artifact_ref")
        .and_then(|value| value.get("artifact_id"))
        .and_then(|value| value.as_str())
        .unwrap_or("n/a");
    let chunk_count = output
        .get("chunking")
        .and_then(|value| value.get("chunk_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let summaries_count = output
        .get("summarization")
        .and_then(|value| value.get("summaries_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let failure_count = output
        .get("summarization")
        .and_then(|value| value.get("failure_count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let elapsed_ms = output
        .get("summarization")
        .and_then(|value| value.get("elapsed_ms"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    let _ = event_tx
        .send(TuiEvent::UiNotice(format!(
            "◈ sttp_compaction tool={} status={} artifact={} chunks={} summaries={} failures={} elapsed_ms={}",
            tool_name, status, artifact_id, chunk_count, summaries_count, failure_count, elapsed_ms
        )))
        .await;
}

// ── CognitionJobEnqueueTool ──────────────────────────────────────────────────

pub struct CognitionJobEnqueueTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionJobEnqueueTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct JobEnqueueInput {
    /// The job handler identifier, e.g. 'workflow.grapheme.run'
    #[schemars(required, with = "String")]
    job_type: Option<String>,
    /// Serialized job payload. For grapheme: 'grapheme:inline:<source>'. For JSON payloads: serialized JSON string.
    #[schemars(required, with = "String")]
    payload_ref: Option<String>,
    /// Optional human-readable note about the intent of this job
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

impl<'de> Deserialize<'de> for JobEnqueueInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            job_type: CompatOption<String>,
            #[serde(default)]
            payload_ref: CompatOption<String>,
            #[serde(default)]
            note: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            job_type: input.job_type.into_option(),
            payload_ref: input.payload_ref.into_option(),
            note: input.note.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum JobEnqueueOutput {
    Rejected {
        status: String,
        reason: String,
        job_type: String,
        policy_message: String,
        validation: ExternalJson,
        note: String,
    },
    Enqueued {
        job_id: String,
        status: String,
        note: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        continuation: Option<ExternalJson>,
    },
}

#[medousa_tool(id = COGNITION_JOB_ENQUEUE_ID)]
impl CognitionJobEnqueueTool {
    /// Persist a job into the Stasis runtime for durable background execution. Use this to schedule work: grapheme scripts, orchestration patterns, memory operations, or any registered workflow handler. Valid job_type values: workflow.grapheme.run, workflow.grapheme.echo, workflow.stasis.orchestration.sequential, workflow.stasis.orchestration.concurrent, workflow.stasis.orchestration.handoff, workflow.stasis.agent_session, workflow.stasis.prompt, openshell.sandbox.run.
    async fn invoke_typed(
        &self,
        input: JobEnqueueInput,
    ) -> stasis::prelude::Result<JobEnqueueOutput> {
        let job_type = input.job_type.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition_job_enqueue: job_type is required".to_string())
        })?;
        let payload_ref = input.payload_ref.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition_job_enqueue: payload_ref is required".to_string())
        })?;

        if job_type == "workflow.grapheme.run" {
            let source = grapheme_inline_payload_source(payload_ref).ok_or_else(|| {
                StasisError::PortFailure(
                    "policy violation: workflow.grapheme.run payload_ref must use grapheme:inline:<source>"
                        .to_string(),
                )
            })?;
            let validation = validate_grapheme_source_for_schedule(&self.runtime, source).await?;
            if !validation
                .get("validated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return Ok(JobEnqueueOutput::Rejected {
                    status: "rejected".to_string(),
                    reason: "invalid_grapheme_source".to_string(),
                    job_type: "workflow.grapheme.run".to_string(),
                    policy_message: "Refused scheduling: Grapheme source failed runtime preflight."
                        .to_string(),
                    validation: ExternalJson::new(validation),
                    note: input.note.clone().unwrap_or_default(),
                });
            }
        }

        let job_id = format!("cognition-{}", Uuid::new_v4().simple());
        let now = Utc::now();

        let mut job = ToolJobSpec::new(
            job_id.clone(),
            "default",
            job_type,
            payload_ref,
            "cognition_tui",
            "sttp:in:cognition:enqueue",
            now,
        )
        .build();

        if let Some(scope) =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope).await
        {
            wire_turn_child_job(
                &mut job,
                &scope,
                COGNITION_JOB_ENQUEUE_ID.as_str(),
                job_type,
                ContinuationAwaitMode::Async,
            )
            .await;
        }

        self.runtime.enqueue_job(job).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::JobEnqueued {
                job_id: job_id.clone(),
                job_type: job_type.to_string(),
            })
            .await;

        let continuation =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await
                .map(|scope| {
                    ExternalJson::new(continuation_tool_metadata(
                        &scope,
                        &job_id,
                        ContinuationAwaitMode::Async,
                    ))
                });

        Ok(JobEnqueueOutput::Enqueued {
            job_id,
            status: "enqueued".to_string(),
            note: input.note.unwrap_or_default(),
            continuation,
        })
    }
}

// ── CognitionGraphemeRunTool ─────────────────────────────────────────────────

pub struct CognitionGraphemeRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    model_target: GraphemeCompactionModelTarget,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionGraphemeRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        session_id: String,
        model_target: GraphemeCompactionModelTarget,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            session_id,
            model_target,
            turn_scope,
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeRunInput {
    /// Complete Grapheme source code. Imports under 'grapheme/*' are allowed by default.
    #[schemars(required, with = "String")]
    source: Option<String>,
}

impl<'de> Deserialize<'de> for GraphemeRunInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            source: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            source: input.source.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_GRAPHEME_RUN_ID)]
impl CognitionGraphemeRunTool {
    /// Execute a Grapheme script synchronously and return the result. Grapheme is a typed workflow scripting language. Built-in modules in the 'grapheme/*' namespace are allowed by default (for example core, web). Scripts run sandboxed with guardrails enforced. Example source: import core from "grapheme/core"
    /// query Run { core.echo(message: "hello") { state { current } } }
    async fn invoke_typed(&self, input: GraphemeRunInput) -> stasis::prelude::Result<ExternalJson> {
        let source = input.source.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition_grapheme_run: source is required".to_string())
        })?;

        remember_last_grapheme_source(source).await;

        let job_id = format!("cognition-gph-{}", Uuid::new_v4().simple());
        let now = Utc::now();

        let mut job = ToolJobSpec::new(
            job_id.clone(),
            "default",
            "workflow.grapheme.run",
            format!("grapheme:inline:{source}"),
            "cognition_tui",
            "sttp:in:cognition:grapheme",
            now,
        )
        .build();

        if let Some(scope) =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope).await
        {
            wire_turn_child_job(
                &mut job,
                &scope,
                COGNITION_GRAPHEME_RUN_ID.as_str(),
                "workflow.grapheme.run",
                ContinuationAwaitMode::Sync,
            )
            .await;
        }

        let continuation_meta =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await
                .map(|scope| {
                    continuation_tool_metadata(&scope, &job_id, ContinuationAwaitMode::Sync)
                });

        self.runtime.enqueue_job(job).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: "cognition_grapheme_run".to_string(),
                input_summary: source.chars().take(60).collect(),
            })
            .await;

        let runtime_ref = Arc::clone(&self.runtime);
        let mut raw_output = match process_once(&runtime_ref, "cognition_tui").await {
            Ok(_) => {
                let attempts = runtime_ref.list_job_attempts(&job_id).await;

                match attempts {
                    Ok(list) => {
                        if let Some(last) = list.last() {
                            let succeeded = last.outcome == JobAttemptOutcome::Succeeded;
                            let execution_id = last.execution_id.clone();
                            let diagnostics = last.diagnostics.as_deref().map(|d| {
                                serde_json::from_str::<Value>(d)
                                    .unwrap_or_else(|_| json!({ "raw": d }))
                            });

                            let _ = self
                                .event_tx
                                .send(TuiEvent::JobProcessed {
                                    job_id: job_id.clone(),
                                    succeeded,
                                    execution_id: execution_id.clone(),
                                })
                                .await;

                            if succeeded {
                                let _ = turn_continuation::turn_continuation_store()
                                    .mark_consumed(&job_id)
                                    .await;
                            }

                            json!({
                                "job_id": job_id,
                                "status": if succeeded { "succeeded" } else { "failed" },
                                "execution_id": execution_id,
                                "attempt_outcome": format!("{:?}", last.outcome),
                                "diagnostics": diagnostics,
                            })
                        } else {
                            let _ = self
                                .event_tx
                                .send(TuiEvent::JobProcessed {
                                    job_id: job_id.clone(),
                                    succeeded: false,
                                    execution_id: None,
                                })
                                .await;

                            json!({
                                "job_id": job_id,
                                "status": "failed",
                                "execution_id": Value::Null,
                                "attempt_outcome": "NoAttempt",
                                "diagnostics": {
                                    "raw": "workflow.grapheme.run produced no job attempt; runtime may have failed before attempt persistence"
                                },
                            })
                        }
                    }
                    Err(err) => {
                        let _ = self
                            .event_tx
                            .send(TuiEvent::JobProcessed {
                                job_id: job_id.clone(),
                                succeeded: false,
                                execution_id: None,
                            })
                            .await;

                        json!({
                            "job_id": job_id,
                            "status": "failed",
                            "execution_id": Value::Null,
                            "attempt_outcome": "AttemptReadFailed",
                            "diagnostics": {
                                "raw": format!("failed to read runtime attempts: {err}")
                            },
                        })
                    }
                }
            }
            Err(err) => {
                let _ = self
                    .event_tx
                    .send(TuiEvent::JobProcessed {
                        job_id: job_id.clone(),
                        succeeded: false,
                        execution_id: None,
                    })
                    .await;

                json!({
                    "job_id": job_id,
                    "status": "failed",
                    "execution_id": Value::Null,
                    "attempt_outcome": "RuntimeProcessFailed",
                    "diagnostics": {
                        "raw": format!("runtime process_once failed: {err}")
                    },
                })
            }
        };
        if let Some(meta) = continuation_meta
            && let Some(obj) = raw_output.as_object_mut()
        {
            obj.insert("continuation".to_string(), meta);
        }
        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await?;
        let serialized_raw_output =
            serde_json::to_string(&raw_output).unwrap_or_else(|_| raw_output.to_string());

        let output = maybe_compact_output_to_sttp(
            COGNITION_GRAPHEME_RUN_ID.as_str(),
            &session_id,
            raw_output,
            &self.model_target,
        )
        .await?;
        emit_compaction_observability(
            &self.event_tx,
            COGNITION_GRAPHEME_RUN_ID.as_str(),
            &output,
            Some(serialized_raw_output.len()),
        )
        .await;
        Ok(ExternalJson::new(output))
    }
}

pub use crate::memory_tools::{
    CognitionMemoryCalibrateTool, CognitionMemoryContextTool, CognitionMemoryEvictTool,
    CognitionMemoryListTool, CognitionMemoryMoodsTool, CognitionMemoryRecallTool,
    CognitionMemorySchemaTool, CognitionMemoryStoreTool, CognitionMemoryTagsTool,
};

// ── Grapheme CLI Discovery/Run Tools (Phase A) ─────────────────────────────

pub struct CognitionGraphemeModulesSearchTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemeModulesSearchTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeModulesSearchInput {
    /// Search query, e.g. web
    #[schemars(required, with = "String")]
    query: Option<String>,
}

impl<'de> Deserialize<'de> for GraphemeModulesSearchInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            query: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            query: input.query.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_GRAPHEME_MODULES_ID)]
impl CognitionGraphemeModulesSearchTool {
    /// Search Grapheme modules by query. Mirrors: grapheme modules search <query> --yaml
    async fn invoke_typed(
        &self,
        input: GraphemeModulesSearchInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let query = input.query.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition_grapheme_modules: query is required".to_string())
        })?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_MODULES_ID.as_str().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let result = run_grapheme_cli(vec![
            "modules".to_string(),
            "search".to_string(),
            query.to_string(),
            "--yaml".to_string(),
        ])
        .await?;
        Ok(ExternalJson::new(result))
    }
}

pub struct CognitionGraphemeModulesInfoTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemeModulesInfoTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeModulesInfoInput {
    /// Module id, e.g. web
    #[schemars(required, with = "String")]
    module: Option<String>,
}

impl<'de> Deserialize<'de> for GraphemeModulesInfoInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            module: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            module: input.module.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_GRAPHEME_MODULES_INFO_ID)]
impl CognitionGraphemeModulesInfoTool {
    /// Inspect Grapheme module metadata. Mirrors: grapheme modules info <module> --yaml
    async fn invoke_typed(
        &self,
        input: GraphemeModulesInfoInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let module = input.module.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_modules_info: module is required".to_string(),
            )
        })?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_MODULES_INFO_ID.as_str().to_string(),
                input_summary: module.to_string(),
            })
            .await;

        let result = run_grapheme_cli(vec![
            "modules".to_string(),
            "info".to_string(),
            module.to_string(),
            "--yaml".to_string(),
        ])
        .await?;
        Ok(ExternalJson::new(result))
    }
}

pub struct CognitionGraphemeModulesOpsTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemeModulesOpsTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, JsonSchema)]
pub struct GraphemeModulesOpsInput {
    /// Module or op query, e.g. web
    #[schemars(required, with = "String")]
    query: Option<String>,
}

impl<'de> Deserialize<'de> for GraphemeModulesOpsInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            query: CompatOption<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            query: input.query.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_GRAPHEME_MODULES_OPS_ID)]
impl CognitionGraphemeModulesOpsTool {
    /// Inspect Grapheme module operations. Mirrors: grapheme modules ops <query> --yaml
    async fn invoke_typed(
        &self,
        input: GraphemeModulesOpsInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let query = input.query.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_modules_ops: query is required".to_string(),
            )
        })?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_MODULES_OPS_ID.as_str().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let result = run_grapheme_cli(vec![
            "modules".to_string(),
            "ops".to_string(),
            query.to_string(),
            "--yaml".to_string(),
        ])
        .await?;
        Ok(ExternalJson::new(result))
    }
}

pub struct CognitionGraphemeExamplesTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemeExamplesTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphemeExamplesActionInput {
    List,
    Show,
}

impl GraphemeExamplesActionInput {
    fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Show => "show",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemeExamplesInput {
    /// list or show
    action: GraphemeExamplesActionInput,
    /// Example name for action=show
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[medousa_tool(id = COGNITION_GRAPHEME_EXAMPLES_ID)]
impl CognitionGraphemeExamplesTool {
    /// List or show Grapheme examples. action=list|show
    async fn invoke_typed(
        &self,
        input: GraphemeExamplesInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let action = input.action.as_str();
        let args = match action {
            "show" => {
                let name = input.name.as_deref().ok_or_else(|| {
                    StasisError::PortFailure(
                        "cognition_grapheme_examples: name is required for action=show".to_string(),
                    )
                })?;
                vec!["examples".to_string(), "show".to_string(), name.to_string()]
            }
            _ => vec!["examples".to_string(), "list".to_string()],
        };

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_EXAMPLES_ID.as_str().to_string(),
                input_summary: action.to_string(),
            })
            .await;

        Ok(ExternalJson::new(run_grapheme_cli(args).await?))
    }
}

pub struct CognitionGraphemeCliRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    model_target: GraphemeCompactionModelTarget,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionGraphemeCliRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        session_id: String,
        model_target: GraphemeCompactionModelTarget,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            session_id,
            model_target,
            turn_scope,
        }
    }
}

fn default_tools_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemeCliRunInput {
    /// Complete Grapheme script source
    #[schemars(required, with = "String")]
    source: Option<String>,
    /// Deprecated compatibility flag; runtime mode always returns JSON
    #[serde(default = "default_tools_true")]
    #[schemars(default = "default_tools_true")]
    json: bool,
    /// Deprecated compatibility flag; ignored in runtime mode
    #[serde(default = "default_tools_true")]
    #[schemars(default = "default_tools_true")]
    stream_steps: bool,
    /// Deprecated compatibility flag; ignored in runtime mode
    #[serde(default)]
    #[schemars(default)]
    native_modules: bool,
}

#[medousa_tool(id = COGNITION_GRAPHEME_CLI_RUN_ID)]
impl CognitionGraphemeCliRunTool {
    /// Run grapheme code through Stasis runtime workflow execution (workflow.grapheme.run) using the same path as scheduled jobs.
    async fn invoke_typed(
        &self,
        input: GraphemeCliRunInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        let source = input.source.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition_grapheme_cli_run: source is required".to_string())
        })?;

        remember_last_grapheme_source(source).await;
        let use_json = input.json;
        let stream_steps = input.stream_steps;
        let native_modules = input.native_modules;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_CLI_RUN_ID.as_str().to_string(),
                input_summary: source.chars().take(60).collect(),
            })
            .await;

        let mut result =
            run_grapheme_via_runtime(&self.runtime, source, "cognition_tui.cli_run").await?;
        result["requested_flags"] = json!({
            "json": use_json,
            "stream_steps": stream_steps,
            "native_modules": native_modules
        });
        result["notes"] = json!([
            "Executed via Stasis runtime workflow path (not external grapheme CLI)",
            "Compatibility flags accepted but not used by runtime executor"
        ]);

        let serialized_raw_output =
            serde_json::to_string(&result).unwrap_or_else(|_| result.to_string());
        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await?;

        let output = maybe_compact_output_to_sttp(
            COGNITION_GRAPHEME_CLI_RUN_ID.as_str(),
            &session_id,
            result,
            &self.model_target,
        )
        .await?;
        emit_compaction_observability(
            &self.event_tx,
            COGNITION_GRAPHEME_CLI_RUN_ID.as_str(),
            &output,
            Some(serialized_raw_output.len()),
        )
        .await;
        Ok(ExternalJson::new(output))
    }
}

pub struct CognitionGraphemePromoteToJobTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionGraphemePromoteToJobTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

fn default_tools_queue() -> String {
    "default".to_string()
}

fn default_tools_priority() -> i64 {
    100
}

fn default_tools_one_u64() -> u64 {
    1
}

fn default_tools_one_i64() -> i64 {
    1
}

#[derive(Debug, JsonSchema)]
pub struct GraphemePromoteToJobInput {
    /// Complete Grapheme source
    #[schemars(required, with = "String")]
    source: Option<String>,
    /// Runtime queue
    #[schemars(default = "default_tools_queue")]
    queue: String,
    /// Job priority
    #[schemars(default = "default_tools_priority")]
    priority: i64,
    /// Max job attempts
    #[schemars(with = "i64", default = "default_tools_one_i64")]
    max_attempts: u64,
}

impl<'de> Deserialize<'de> for GraphemePromoteToJobInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            source: CompatOption<String>,
            #[serde(default = "default_tools_queue")]
            queue: String,
            #[serde(default = "default_tools_priority")]
            priority: i64,
            #[serde(default = "default_tools_one_u64")]
            max_attempts: u64,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            source: input.source.into_option(),
            queue: input.queue,
            priority: input.priority,
            max_attempts: input.max_attempts,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GraphemePromoteToJobOutput {
    Rejected {
        status: String,
        reason: String,
        job_type: String,
        policy_message: String,
        validation: ExternalJson,
    },
    Enqueued {
        job_id: String,
        job_type: String,
        queue: String,
        status: String,
        validation: ExternalJson,
        #[serde(skip_serializing_if = "Option::is_none")]
        continuation: Option<ExternalJson>,
    },
}

#[medousa_tool(id = COGNITION_GRAPHEME_PROMOTE_TO_JOB_ID)]
impl CognitionGraphemePromoteToJobTool {
    /// Promote Grapheme source to a durable one-off runtime job (workflow.grapheme.run).
    async fn invoke_typed(
        &self,
        input: GraphemePromoteToJobInput,
    ) -> stasis::prelude::Result<GraphemePromoteToJobOutput> {
        let source = input.source.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_promote_to_job: source is required".to_string(),
            )
        })?;

        remember_last_grapheme_source(source).await;
        let validation = validate_grapheme_source_for_schedule(&self.runtime, source).await?;
        if !validation
            .get("validated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Ok(GraphemePromoteToJobOutput::Rejected {
                status: "rejected".to_string(),
                reason: "invalid_grapheme_source".to_string(),
                job_type: "workflow.grapheme.run".to_string(),
                policy_message: "Refused promotion: Grapheme source failed runtime preflight."
                    .to_string(),
                validation: ExternalJson::new(validation),
            });
        }

        let job_id = format!("cognition-promote-job-{}", Uuid::new_v4().simple());
        let now = Utc::now();

        let mut job = ToolJobSpec::new(
            job_id.clone(),
            input.queue.clone(),
            "workflow.grapheme.run",
            format!("grapheme:inline:{source}"),
            "cognition_tui.promote",
            "sttp:in:cognition:grapheme:promote",
            now,
        )
        .priority(input.priority as i32)
        .max_attempts(input.max_attempts as u32)
        .build();

        if let Some(scope) =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope).await
        {
            wire_turn_child_job(
                &mut job,
                &scope,
                COGNITION_GRAPHEME_PROMOTE_TO_JOB_ID.as_str(),
                "workflow.grapheme.run",
                ContinuationAwaitMode::Async,
            )
            .await;
        }

        self.runtime.enqueue_job(job).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::JobEnqueued {
                job_id: job_id.clone(),
                job_type: "workflow.grapheme.run".to_string(),
            })
            .await;

        let continuation =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await
                .map(|scope| {
                    ExternalJson::new(continuation_tool_metadata(
                        &scope,
                        &job_id,
                        ContinuationAwaitMode::Async,
                    ))
                });
        Ok(GraphemePromoteToJobOutput::Enqueued {
            job_id,
            job_type: "workflow.grapheme.run".to_string(),
            queue: input.queue,
            status: "enqueued".to_string(),
            validation: ExternalJson::new(validation),
            continuation,
        })
    }
}

pub struct CognitionGraphemePromoteToRecurringTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionGraphemePromoteToRecurringTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

fn default_tools_timezone() -> String {
    "UTC".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemePromoteToRecurringInput {
    /// Complete Grapheme source
    #[schemars(required, with = "String")]
    source: Option<String>,
    /// 7-field cron: sec min hour day-of-month month day-of-week year (e.g. 0 0 */4 * * * *)
    #[schemars(required, with = "String")]
    cron_expr: Option<String>,
    /// IANA timezone
    #[serde(default = "default_tools_timezone")]
    #[schemars(default = "default_tools_timezone")]
    timezone: String,
    /// Runtime queue
    #[serde(default = "default_tools_queue")]
    #[schemars(default = "default_tools_queue")]
    queue: String,
    /// Optional recurring id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    /// Jitter seconds
    #[serde(default)]
    #[schemars(default)]
    jitter_seconds: i64,
    /// Max attempts per materialized job
    #[serde(default = "default_tools_one_u64")]
    #[schemars(with = "i64", default = "default_tools_one_i64")]
    max_attempts: u64,
    /// Enabled schedule
    #[serde(default = "default_tools_true")]
    #[schemars(default = "default_tools_true")]
    enabled: bool,
    /// Set next_run_at=now
    #[serde(default)]
    #[schemars(default)]
    start_immediately: bool,
    /// Where to push each successful run (independent of current UI channel). 7-field cron required separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "RecurringDeliverySpec",
        skip_serializing_if = "Option::is_none"
    )]
    delivery: Option<RecurringDeliverySpec>,
    /// Environment feed ids to publish each materialized run terminal event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "RecurringFeedSpec", skip_serializing_if = "Option::is_none")]
    feeds: Option<RecurringFeedSpec>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GraphemePromoteToRecurringOutput {
    Rejected {
        status: String,
        reason: String,
        job_type: String,
        policy_message: String,
        validation: ExternalJson,
    },
    Registered {
        recurring_id: String,
        job_type: String,
        queue: String,
        cron_expr: String,
        timezone: String,
        enabled: bool,
        start_immediately: bool,
        status: String,
        delivery_bound: bool,
        feeds_bound: bool,
        validation: ExternalJson,
    },
}

#[medousa_tool(id = COGNITION_GRAPHEME_PROMOTE_TO_RECURRING_ID)]
impl CognitionGraphemePromoteToRecurringTool {
    /// Promote Grapheme source to a durable recurring schedule (register_recurring).
    async fn invoke_typed(
        &self,
        input: GraphemePromoteToRecurringInput,
    ) -> stasis::prelude::Result<GraphemePromoteToRecurringOutput> {
        let source = input.source.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_promote_to_recurring: source is required".to_string(),
            )
        })?;
        let cron_expr = input.cron_expr.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_promote_to_recurring: cron_expr is required".to_string(),
            )
        })?;

        remember_last_grapheme_source(source).await;
        let validation = validate_grapheme_source_for_schedule(&self.runtime, source).await?;
        if !validation
            .get("validated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Ok(GraphemePromoteToRecurringOutput::Rejected {
                status: "rejected".to_string(),
                reason: "invalid_grapheme_source".to_string(),
                job_type: "workflow.grapheme.run".to_string(),
                policy_message:
                    "Refused recurring registration: Grapheme source failed runtime preflight."
                        .to_string(),
                validation: ExternalJson::new(validation),
            });
        }

        let recurring_id = input
            .id
            .clone()
            .unwrap_or_else(|| format!("recur-gph-{}", Uuid::new_v4().simple()));

        let now = Utc::now();
        let payload_template_ref = format!("grapheme:inline:{source}");

        let definition = RecurringScheduleSpec::new(
            recurring_id.clone(),
            input.queue.clone(),
            "workflow.grapheme.run",
            payload_template_ref,
            cron_expr.to_string(),
            input.timezone.clone(),
        )
        .jitter_seconds(input.jitter_seconds)
        .enabled(input.enabled)
        .max_attempts(input.max_attempts as u32)
        .start_immediately(input.start_immediately)
        .build(now)?;

        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_spec_for_registration(
            &recurring_id,
            cron_expr,
            &input.timezone,
            input.delivery.as_ref(),
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id: fallback_session_id.clone(),
            },
        )
        .await?;
        let (feeds_bound, _) =
            bind_recurring_feed_spec_for_registration(&recurring_id, input.feeds.as_ref()).await?;

        self.runtime.register_recurring(definition).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_PROMOTE_TO_RECURRING_ID
                    .as_str()
                    .to_string(),
                input_summary: format!("{} @ {}", recurring_id, cron_expr),
            })
            .await;

        Ok(GraphemePromoteToRecurringOutput::Registered {
            recurring_id,
            job_type: "workflow.grapheme.run".to_string(),
            queue: input.queue,
            cron_expr: cron_expr.to_string(),
            timezone: input.timezone,
            enabled: input.enabled,
            start_immediately: input.start_immediately,
            status: "registered".to_string(),
            delivery_bound,
            feeds_bound,
            validation: ExternalJson::new(validation),
        })
    }
}

pub struct CognitionGraphemePromoteLastRunToRecurringTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionGraphemePromoteLastRunToRecurringTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemePromoteLastRunInput {
    /// 7-field cron: sec min hour day-of-month month day-of-week year (e.g. 0 0 */4 * * * *)
    #[schemars(required, with = "String")]
    cron_expr: Option<String>,
    /// IANA timezone
    #[serde(default = "default_tools_timezone")]
    #[schemars(default = "default_tools_timezone")]
    timezone: String,
    /// Runtime queue
    #[serde(default = "default_tools_queue")]
    #[schemars(default = "default_tools_queue")]
    queue: String,
    /// Optional recurring id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    /// Jitter seconds
    #[serde(default)]
    #[schemars(default)]
    jitter_seconds: i64,
    /// Max attempts per materialized job
    #[serde(default = "default_tools_one_u64")]
    #[schemars(with = "i64", default = "default_tools_one_i64")]
    max_attempts: u64,
    /// Enabled schedule
    #[serde(default = "default_tools_true")]
    #[schemars(default = "default_tools_true")]
    enabled: bool,
    /// Set next_run_at=now
    #[serde(default)]
    #[schemars(default)]
    start_immediately: bool,
    /// Optional source override; if omitted, uses last remembered source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    /// Where to push each successful run (independent of current UI channel). 7-field cron required separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "RecurringDeliverySpec",
        skip_serializing_if = "Option::is_none"
    )]
    delivery: Option<RecurringDeliverySpec>,
    /// Environment feed ids to publish each materialized run terminal event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "RecurringFeedSpec", skip_serializing_if = "Option::is_none")]
    feeds: Option<RecurringFeedSpec>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GraphemePromoteLastRunOutput {
    Rejected {
        status: String,
        reason: String,
        job_type: String,
        policy_message: String,
        used_remembered_source: bool,
        validation: ExternalJson,
    },
    Registered {
        recurring_id: String,
        job_type: String,
        queue: String,
        cron_expr: String,
        timezone: String,
        enabled: bool,
        start_immediately: bool,
        used_remembered_source: bool,
        status: String,
        delivery_bound: bool,
        feeds_bound: bool,
        validation: ExternalJson,
    },
}

#[medousa_tool(id = COGNITION_GRAPHEME_PROMOTE_LAST_RUN_TO_RECURRING_ID)]
impl CognitionGraphemePromoteLastRunToRecurringTool {
    /// Promote the last executed Grapheme source to recurring schedule. You can also provide source explicitly.
    async fn invoke_typed(
        &self,
        input: GraphemePromoteLastRunInput,
    ) -> stasis::prelude::Result<GraphemePromoteLastRunOutput> {
        let cron_expr = input.cron_expr.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_promote_last_run_to_recurring: cron_expr is required"
                    .to_string(),
            )
        })?;

        let used_remembered_source = input.source.is_none();
        let source = if let Some(src) = input.source.as_deref() {
            src.to_string()
        } else {
            read_last_grapheme_source().await.ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_promote_last_run_to_recurring: no remembered source; run cognition_grapheme_cli_run first or provide source".to_string(),
                )
            })?
        };
        let validation = validate_grapheme_source_for_schedule(&self.runtime, &source).await?;
        if !validation
            .get("validated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return Ok(GraphemePromoteLastRunOutput::Rejected {
                status: "rejected".to_string(),
                reason: "invalid_grapheme_source".to_string(),
                job_type: "workflow.grapheme.run".to_string(),
                policy_message: "Refused recurring registration from last run: Grapheme source failed runtime preflight."
                    .to_string(),
                used_remembered_source,
                validation: ExternalJson::new(validation),
            });
        }

        let recurring_id = input
            .id
            .clone()
            .unwrap_or_else(|| format!("recur-gph-{}", Uuid::new_v4().simple()));

        let now = Utc::now();
        let payload_template_ref = format!("grapheme:inline:{source}");

        let definition = RecurringScheduleSpec::new(
            recurring_id.clone(),
            input.queue.clone(),
            "workflow.grapheme.run",
            payload_template_ref,
            cron_expr.to_string(),
            input.timezone.clone(),
        )
        .jitter_seconds(input.jitter_seconds)
        .enabled(input.enabled)
        .max_attempts(input.max_attempts as u32)
        .start_immediately(input.start_immediately)
        .build(now)?;

        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_spec_for_registration(
            &recurring_id,
            cron_expr,
            &input.timezone,
            input.delivery.as_ref(),
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id: fallback_session_id.clone(),
            },
        )
        .await?;
        let (feeds_bound, _) =
            bind_recurring_feed_spec_for_registration(&recurring_id, input.feeds.as_ref()).await?;

        self.runtime.register_recurring(definition).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_PROMOTE_LAST_RUN_TO_RECURRING_ID
                    .as_str()
                    .to_string(),
                input_summary: format!("{} @ {}", recurring_id, cron_expr),
            })
            .await;

        Ok(GraphemePromoteLastRunOutput::Registered {
            recurring_id,
            job_type: "workflow.grapheme.run".to_string(),
            queue: input.queue,
            cron_expr: cron_expr.to_string(),
            timezone: input.timezone,
            enabled: input.enabled,
            start_immediately: input.start_immediately,
            used_remembered_source,
            status: "registered".to_string(),
            delivery_bound,
            feeds_bound,
            validation: ExternalJson::new(validation),
        })
    }
}

const COGNITION_UTILITY_TIME_NOW_ID: ToolId = ToolId::new("cognition_utility_time_now");
const COGNITION_UTILITY_DAY_OF_WEEK_ID: ToolId = ToolId::new("cognition_utility_day_of_week");
const COGNITION_UTILITY_UUID_ID: ToolId = ToolId::new("cognition_utility_uuid");
const COGNITION_RUNTIME_RECURRING_PREVIEW_ID: ToolId =
    ToolId::new("cognition_runtime_recurring_preview");
const COGNITION_RUNTIME_JOBS_STATUS_ID: ToolId = ToolId::new("cognition_runtime_jobs_status");

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UtilityTimeNowInput {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UtilityTimeNowOutput {
    pub utc_rfc3339: String,
    pub local_rfc3339: String,
    pub weekday: String,
    pub unix_seconds: i64,
    pub unix_millis: i64,
    pub local_offset_seconds: i32,
}

pub struct CognitionUtilityTimeNowTool;

#[medousa_tool(id = COGNITION_UTILITY_TIME_NOW_ID)]
impl CognitionUtilityTimeNowTool {
    /// Return current time in UTC and local timezone, including weekday and unix timestamp.
    async fn invoke_typed(
        &self,
        _input: UtilityTimeNowInput,
    ) -> stasis::prelude::Result<UtilityTimeNowOutput> {
        let now_utc = Utc::now();
        let now_local = Local::now();

        Ok(UtilityTimeNowOutput {
            utc_rfc3339: now_utc.to_rfc3339(),
            local_rfc3339: now_local.to_rfc3339(),
            weekday: now_local.weekday().to_string(),
            unix_seconds: now_utc.timestamp(),
            unix_millis: now_utc.timestamp_millis(),
            local_offset_seconds: now_local.offset().local_minus_utc(),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UtilityDayOfWeekInput {
    /// Optional date in YYYY-MM-DD
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub date: CompatOption<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UtilityDayOfWeekOutput {
    pub date: String,
    pub weekday: String,
    pub weekday_number_from_monday: u32,
    pub weekday_number_from_sunday: u32,
}

pub struct CognitionUtilityDayOfWeekTool;

#[medousa_tool(id = COGNITION_UTILITY_DAY_OF_WEEK_ID)]
impl CognitionUtilityDayOfWeekTool {
    /// Return weekday for a YYYY-MM-DD date, or for today when date is omitted.
    async fn invoke_typed(
        &self,
        input: UtilityDayOfWeekInput,
    ) -> stasis::prelude::Result<UtilityDayOfWeekOutput> {
        let date = if let Some(date_str) = input.date.into_option() {
            NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(|e| {
                StasisError::PortFailure(format!(
                    "cognition_utility_day_of_week: invalid date '{}': {}",
                    date_str, e
                ))
            })?
        } else {
            Local::now().date_naive()
        };

        Ok(UtilityDayOfWeekOutput {
            date: date.format("%Y-%m-%d").to_string(),
            weekday: date.weekday().to_string(),
            weekday_number_from_monday: date.weekday().number_from_monday(),
            weekday_number_from_sunday: date.weekday().number_from_sunday(),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UtilityUuidInput {
    /// Optional prefix for derived keys
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub prefix: CompatOption<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UtilityUuidOutput {
    pub uuid: String,
    pub uuid_simple: String,
    pub correlation_id: String,
    pub trace_id: String,
    pub idempotency_key: String,
}

pub struct CognitionUtilityUuidTool;

#[medousa_tool(id = COGNITION_UTILITY_UUID_ID)]
impl CognitionUtilityUuidTool {
    /// Generate UUID helper values for correlation, trace, and idempotency keys.
    async fn invoke_typed(
        &self,
        input: UtilityUuidInput,
    ) -> stasis::prelude::Result<UtilityUuidOutput> {
        let id = Uuid::new_v4();
        let prefix = input
            .prefix
            .into_option()
            .unwrap_or_else(|| "cognition".to_string());

        Ok(UtilityUuidOutput {
            uuid: id.to_string(),
            uuid_simple: id.simple().to_string(),
            correlation_id: format!("{}-{}", prefix, id.simple()),
            trace_id: format!("{}-trace-{}", prefix, id.simple()),
            idempotency_key: format!("idem-{}-{}", prefix, id.simple()),
        })
    }
}

pub struct CognitionRuntimeJobStatusTool {
    runtime: Arc<RuntimeComposition>,
}

pub struct CognitionRuntimeRecurringPreviewTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeRecurringPreviewTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

fn default_runtime_timezone() -> String {
    "UTC".to_string()
}

#[derive(Debug, JsonSchema)]
pub struct RuntimeRecurringPreviewInput {
    /// Cron expression to validate
    #[schemars(required, with = "String")]
    cron_expr: Option<String>,
    /// IANA timezone
    #[schemars(default = "default_runtime_timezone")]
    timezone: String,
    /// How many future runs to preview (1-20, default 5)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1, max = 20),
        skip_serializing_if = "Option::is_none"
    )]
    count: Option<usize>,
    /// Optional RFC3339 UTC start timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    start_at: Option<String>,
}

impl<'de> Deserialize<'de> for RuntimeRecurringPreviewInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            cron_expr: CompatOption<String>,
            #[serde(default)]
            timezone: CompatOption<String>,
            #[serde(default)]
            count: CompatOption<usize>,
            #[serde(default)]
            start_at: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            cron_expr: input.cron_expr.into_option(),
            timezone: input
                .timezone
                .into_option()
                .unwrap_or_else(default_runtime_timezone),
            count: input.count.into_option(),
            start_at: input.start_at.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringPreviewEntry {
    run_at_utc: String,
    unix_seconds: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringPreviewOutput {
    valid: bool,
    cron_expr: String,
    timezone: String,
    start_at_utc: String,
    count: usize,
    preview: Vec<RuntimeRecurringPreviewEntry>,
}

#[medousa_tool(id = COGNITION_RUNTIME_RECURRING_PREVIEW_ID)]
impl CognitionRuntimeRecurringPreviewTool {
    /// Validate cron/timezone configuration and preview upcoming recurring run times.
    async fn invoke_typed(
        &self,
        input: RuntimeRecurringPreviewInput,
    ) -> stasis::prelude::Result<RuntimeRecurringPreviewOutput> {
        let cron_expr = input.cron_expr.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_runtime_recurring_preview: cron_expr is required".to_string(),
            )
        })?;
        let timezone = input.timezone;
        let count = input.count.unwrap_or(5).clamp(1, 20);

        let base_time = if let Some(start_at) = input.start_at.as_deref() {
            DateTime::parse_from_rfc3339(start_at)
                .map_err(|e| {
                    StasisError::PortFailure(format!(
                        "cognition_runtime_recurring_preview: invalid start_at '{}': {}",
                        start_at, e
                    ))
                })?
                .with_timezone(&Utc)
        } else {
            Utc::now()
        };

        let definition = RecurringScheduleSpec::new(
            "preview-only",
            "default",
            "workflow.grapheme.run",
            "grapheme:inline:preview",
            cron_expr.clone(),
            timezone.clone(),
        )
        .build(base_time)?;

        let mut cursor = base_time;
        let mut preview = Vec::with_capacity(count);

        for _ in 0..count {
            let next_run = definition.compute_next_run_at(cursor)?;
            preview.push(RuntimeRecurringPreviewEntry {
                run_at_utc: next_run.to_rfc3339(),
                unix_seconds: next_run.timestamp(),
            });
            cursor = next_run + Duration::seconds(1);
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{cron_expr} @ {timezone}"),
            })
            .await;

        Ok(RuntimeRecurringPreviewOutput {
            valid: true,
            cron_expr,
            timezone,
            start_at_utc: base_time.to_rfc3339(),
            count,
            preview,
        })
    }
}

impl CognitionRuntimeJobStatusTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[derive(Debug, JsonSchema)]
pub struct RuntimeJobStatusInput {
    /// Runtime job identifier
    #[schemars(required, with = "String")]
    job_id: Option<String>,
}

impl<'de> Deserialize<'de> for RuntimeJobStatusInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            job_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            job_id: input.job_id.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeJobAttemptSummary {
    attempt: u32,
    outcome: String,
    execution_id: Option<String>,
    #[schemars(with = "String")]
    started_at: DateTime<Utc>,
    #[schemars(with = "String")]
    finished_at: DateTime<Utc>,
    diagnostics: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeJobStatusOutput {
    job_id: String,
    attempt_count: usize,
    latest_outcome: String,
    latest_execution_id: Option<String>,
    latest_diagnostics: Option<String>,
    attempts: Vec<RuntimeJobAttemptSummary>,
}

#[medousa_tool(id = COGNITION_RUNTIME_JOBS_STATUS_ID)]
impl CognitionRuntimeJobStatusTool {
    /// Inspect job attempts and latest execution status for a given job_id.
    async fn invoke_typed(
        &self,
        input: RuntimeJobStatusInput,
    ) -> stasis::prelude::Result<RuntimeJobStatusOutput> {
        let job_id = input.job_id.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_runtime_jobs_status: job_id is required".to_string(),
            )
        })?;

        let attempts = self.runtime.list_job_attempts(&job_id).await?;

        let last = attempts.last();
        let latest_outcome = last
            .map(|a| format!("{:?}", a.outcome))
            .unwrap_or_else(|| "Unknown".to_string());
        let execution_id = last.and_then(|a| a.execution_id.clone());
        let diagnostics = last.and_then(|a| a.diagnostics.clone());

        let attempts_summary = attempts
            .iter()
            .map(|attempt| RuntimeJobAttemptSummary {
                attempt: attempt.attempt_number,
                outcome: format!("{:?}", attempt.outcome),
                execution_id: attempt.execution_id.clone(),
                started_at: attempt.started_at,
                finished_at: attempt.finished_at,
                diagnostics: attempt.diagnostics.clone(),
            })
            .collect();

        Ok(RuntimeJobStatusOutput {
            job_id,
            attempt_count: attempts.len(),
            latest_outcome,
            latest_execution_id: execution_id,
            latest_diagnostics: diagnostics,
            attempts: attempts_summary,
        })
    }
}

// ── Capability catalog tools (Phase A) ───────────────────────────────────────

const COGNITION_CAPABILITY_RESOLVE_ID: ToolId = ToolId::new("cognition_capability_resolve");
const COGNITION_CAPABILITY_LIST_ID: ToolId = ToolId::new("cognition_capability_list");
const COGNITION_CAPABILITY_SEARCH_ID: ToolId = ToolId::new("cognition_capability_search");
const COGNITION_MCP_DISCOVER_ID: ToolId = ToolId::new("cognition_mcp_discover");
const COGNITION_MCP_SERVERS_ID: ToolId = ToolId::new("cognition_mcp_servers");

pub struct CognitionCapabilityResolveTool {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCapabilityResolveTool {
    pub fn new(
        capability_registry: Arc<RwLock<CapabilityRegistry>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            capability_registry,
            event_tx,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CapabilityResolveInput {
    /// Capability id, e.g. document_search
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    capability: CompatOption<String>,
    /// Optional fuzzy query when capability id is unknown
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    query: CompatOption<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum CapabilityResolveOutput {
    Resolved(CapabilityResolveResponse),
    NoMatch {
        capability: Option<String>,
        matches: Vec<CapabilitySearchMatch>,
        message: String,
    },
}

#[medousa_tool(id = COGNITION_CAPABILITY_RESOLVE_ID)]
impl CognitionCapabilityResolveTool {
    /// Resolve a capability intent to Grapheme and MCP implementations.
    async fn invoke_typed(
        &self,
        input: CapabilityResolveInput,
    ) -> stasis::prelude::Result<CapabilityResolveOutput> {
        let capability_value = input.capability.into_option();
        let query_value = input.query.into_option();
        let capability_id = capability_value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let query = query_value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if capability_id.is_none() && query.is_none() {
            return Err(StasisError::PortFailure(
                "cognition.capability.resolve: capability or query is required".to_string(),
            ));
        }

        let summary = capability_id.clone().or(query.clone()).unwrap_or_default();
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_CAPABILITY_RESOLVE_ID.as_str().to_string(),
                input_summary: summary,
            })
            .await;

        let registry = self.capability_registry.read().await;
        if let Some(capability_id) = capability_id {
            let response = registry.resolve(&capability_id).ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "cognition.capability.resolve: unknown capability '{capability_id}'"
                ))
            })?;
            return Ok(CapabilityResolveOutput::Resolved(response));
        }

        let search = registry.search(query.as_deref().unwrap_or_default(), 1);
        let Some(first) = search.matches.first() else {
            return Ok(CapabilityResolveOutput::NoMatch {
                capability: None,
                matches: search.matches,
                message: "no capabilities matched query".to_string(),
            });
        };

        let response = registry.resolve(&first.capability).ok_or_else(|| {
            StasisError::PortFailure(format!(
                "cognition.capability.resolve: matched capability '{}' but resolve failed",
                first.capability
            ))
        })?;
        Ok(CapabilityResolveOutput::Resolved(response))
    }
}

pub struct CognitionCapabilityListTool {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
}

impl CognitionCapabilityListTool {
    pub fn new(capability_registry: Arc<RwLock<CapabilityRegistry>>) -> Self {
        Self {
            capability_registry,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CapabilityListInput {
    /// Optional capability id prefix filter
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    prefix: CompatOption<String>,
    /// Max entries (default 50)
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
}

#[medousa_tool(id = COGNITION_CAPABILITY_LIST_ID)]
impl CognitionCapabilityListTool {
    /// List registered capability intents in the Medousa capability catalog.
    async fn invoke_typed(
        &self,
        input: CapabilityListInput,
    ) -> stasis::prelude::Result<CapabilityListResponse> {
        let prefix_value = input.prefix.into_option();
        let prefix = prefix_value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = input.limit.into_option().unwrap_or(50).clamp(1, 200);

        let registry = self.capability_registry.read().await;
        let mut response = registry.list();
        if let Some(prefix) = prefix {
            response
                .capabilities
                .retain(|entry| entry.id.starts_with(prefix));
        }
        response.capabilities.truncate(limit);
        Ok(response)
    }
}

pub struct CognitionCapabilitySearchTool {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCapabilitySearchTool {
    pub fn new(
        capability_registry: Arc<RwLock<CapabilityRegistry>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            capability_registry,
            event_tx,
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct CapabilitySearchInput {
    /// Search query
    #[schemars(required, with = "String")]
    query: Option<String>,
    /// Max matches (default 10)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

impl<'de> Deserialize<'de> for CapabilitySearchInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            query: CompatOption<String>,
            #[serde(default)]
            limit: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            query: input.query.into_option(),
            limit: input.limit.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_CAPABILITY_SEARCH_ID)]
impl CognitionCapabilitySearchTool {
    /// Keyword search capability intents by query, alias, or keywords.
    async fn invoke_typed(
        &self,
        input: CapabilitySearchInput,
    ) -> stasis::prelude::Result<CapabilitySearchResponse> {
        let query = input.query.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition.capability.search: query is required".to_string())
        })?;
        let limit = input.limit.unwrap_or(10).clamp(1, 50);

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_CAPABILITY_SEARCH_ID.as_str().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let registry = self.capability_registry.read().await;
        let response = registry.search(query, limit);
        Ok(response)
    }
}

pub struct CognitionMcpDiscoverTool {
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMcpDiscoverTool {
    pub fn new(
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            gateway_client,
            session_id: session_id.into(),
            turn_scope,
            event_tx,
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct McpDiscoverInput {
    /// Search query
    #[schemars(required, with = "String")]
    query: Option<String>,
    /// Optional MCP server id filter
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    server_id: Option<String>,
    /// Max matches (default 20)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

impl<'de> Deserialize<'de> for McpDiscoverInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            query: CompatOption<String>,
            #[serde(default)]
            server_id: CompatOption<String>,
            #[serde(default)]
            limit: CompatOption<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            query: input.query.into_option(),
            server_id: input.server_id.into_option(),
            limit: input.limit.into_option(),
        })
    }
}

#[medousa_tool(id = COGNITION_MCP_DISCOVER_ID)]
impl CognitionMcpDiscoverTool {
    /// Search external MCP tools via the MCP Client gateway.
    async fn invoke_typed(&self, input: McpDiscoverInput) -> stasis::prelude::Result<ExternalJson> {
        let query = input.query.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition.mcp.discover: query is required".to_string())
        })?;
        let server_id = input
            .server_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let limit = input.limit.unwrap_or(20).clamp(1, 100);

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_MCP_DISCOVER_ID.as_str().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await?;
        let turn_context = build_agent_mcp_turn_context(&session_id);
        let request = McpDiscoverRequest {
            query: query.to_string(),
            server_id,
            limit,
            turn_context,
        };

        let response = self
            .gateway_client
            .discover(&request)
            .await
            .map_err(|error| {
                StasisError::PortFailure(format!("cognition.mcp.discover: {error}"))
            })?;

        serde_json::to_value(response)
            .map(ExternalJson::new)
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "cognition.mcp.discover: failed to encode response: {error}"
                ))
            })
    }
}

pub struct CognitionMcpInvokeTool {
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMcpInvokeTool {
    pub fn new(
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            gateway_client,
            session_id: session_id.into(),
            turn_scope,
            event_tx,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct McpInvokeObject(Value);

impl JsonSchema for McpInvokeObject {
    fn schema_name() -> String {
        "McpInvokeObject".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::Object.into()),
            ..SchemaObject::default()
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct McpInvokeInput {
    #[schemars(required, with = "String")]
    server_id: Option<String>,
    #[schemars(required, with = "String")]
    tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "McpInvokeObject", skip_serializing_if = "Option::is_none")]
    input: Option<McpInvokeObject>,
    /// Optional pre-minted turn token
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    turn_token: Option<String>,
    /// Set true after the operator approves a prior approval_required response
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    approval_granted: Option<bool>,
}

#[medousa_tool(id = COGNITION_MCP_INVOKE_ID)]
impl CognitionMcpInvokeTool {
    /// Invoke an external MCP tool via the MCP Client gateway.
    async fn invoke_typed(&self, input: McpInvokeInput) -> stasis::prelude::Result<ExternalJson> {
        let server_id = input.server_id.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition.mcp.invoke: server_id is required".to_string())
        })?;
        let tool_name = input.tool_name.as_deref().ok_or_else(|| {
            StasisError::PortFailure("cognition.mcp.invoke: tool_name is required".to_string())
        })?;
        let tool_input = input
            .input
            .map(|input| input.0)
            .unwrap_or_else(|| json!({}));

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_MCP_INVOKE_ID.as_str().to_string(),
                input_summary: format!("{server_id}.{tool_name}"),
            })
            .await;

        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await?;
        let turn_context = build_agent_mcp_turn_context(&session_id);
        let turn_token = if let Some(token) = input.turn_token {
            Some(token)
        } else {
            mint_mcp_turn_token(&turn_context).map_err(|error| {
                StasisError::PortFailure(format!("cognition.mcp.invoke: {error}"))
            })?
        };
        let operator_approval_granted = input.approval_granted;

        let request = McpInvokeRequest {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            input: tool_input,
            turn_context,
            turn_token,
            operator_approval_granted,
        };

        let response = self
            .gateway_client
            .invoke(&request)
            .await
            .map_err(|error| StasisError::PortFailure(format!("cognition.mcp.invoke: {error}")))?;

        if !response.ok
            && let Some(error) = response.error.as_ref()
            && error.code == "approval_required"
        {
            let _ = self
                .event_tx
                .send(TuiEvent::ApprovalRequired {
                    server_id: server_id.to_string(),
                    tool_name: tool_name.to_string(),
                    reason: error.message.clone(),
                })
                .await;
        }

        serde_json::to_value(response)
            .map(ExternalJson::new)
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "cognition.mcp.invoke: failed to encode response: {error}"
                ))
            })
    }
}

pub struct CognitionMcpServersTool {
    gateway_client: Arc<McpGatewayClient>,
}

impl CognitionMcpServersTool {
    pub fn new(gateway_client: Arc<McpGatewayClient>) -> Self {
        Self { gateway_client }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct McpServersInput {}

#[medousa_tool(id = COGNITION_MCP_SERVERS_ID)]
impl CognitionMcpServersTool {
    /// List MCP servers known to the MCP Client gateway.
    async fn invoke_typed(&self, _input: McpServersInput) -> stasis::prelude::Result<ExternalJson> {
        let response =
            self.gateway_client.list_servers().await.map_err(|error| {
                StasisError::PortFailure(format!("cognition.mcp.servers: {error}"))
            })?;
        serde_json::to_value(response)
            .map(ExternalJson::new)
            .map_err(|error| {
                StasisError::PortFailure(format!(
                    "cognition.mcp.servers: failed to encode response: {error}"
                ))
            })
    }
}

fn build_agent_mcp_turn_context(session_id: &str) -> McpTurnContext {
    McpTurnContext {
        turn_id: format!("tool-{}", Uuid::new_v4()),
        session_id: session_id.to_string(),
        user_id: crate::identity_memory::resolve_identity_user_id(None),
        channel_id: crate::identity_memory::resolve_identity_channel_id(Some("interactive")),
        lane: McpTurnLane::Interactive,
        policy_profile: Some("interactive".to_string()),
    }
}

#[derive(Clone)]
pub(crate) struct PolicyAwareToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    allowed_module_ops: HashSet<String>,
    lane: EngineExecutionLane,
}

impl PolicyAwareToolRegistry {
    pub(crate) fn new(
        inner: Arc<dyn ToolRegistry>,
        allowed_module_ops: Vec<String>,
        lane: EngineExecutionLane,
    ) -> Self {
        let allowed_module_ops = allowed_module_ops
            .into_iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect::<HashSet<_>>();

        Self {
            inner,
            allowed_module_ops,
            lane,
        }
    }

    fn enforce_lane_safety(&self, tool_name: &str, input: &Value) -> stasis::prelude::Result<()> {
        if let Some(action) = lane_safety_action_for_tool_call(tool_name, input)
            && let Err(reason) = validate_lane_action(self.lane, action)
        {
            return Err(StasisError::PortFailure(format!(
                "lane safety violation: {reason}"
            )));
        }

        let policy_profile = tool_policy_profile_for_tool_call(input);
        if let Err(reason) = validate_lane_policy_profile(self.lane, policy_profile) {
            return Err(StasisError::PortFailure(format!(
                "lane safety violation: {reason}"
            )));
        }

        Ok(())
    }

    fn enforce_allowed_modules(
        &self,
        tool_name: &str,
        input: &Value,
    ) -> stasis::prelude::Result<()> {
        if self.allowed_module_ops.is_empty() {
            return Ok(());
        }

        let referenced_ops = referenced_module_ops_for_tool_call(tool_name, input)?;
        if referenced_ops.is_empty() {
            return Ok(());
        }

        let mut blocked = referenced_ops
            .into_iter()
            .filter(|op| !self.allowed_module_ops.contains(&op.to_ascii_lowercase()))
            .collect::<Vec<_>>();
        blocked.sort();
        blocked.dedup();

        if blocked.is_empty() {
            return Ok(());
        }

        let blocked_list = blocked.join(", ");
        let allowed_list = self
            .allowed_module_ops
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        Err(StasisError::PortFailure(format!(
            "policy violation: blocked Grapheme module operation(s): {blocked_list}. allowed operations: {allowed_list}"
        )))
    }
}

#[async_trait]
impl ToolRegistry for PolicyAwareToolRegistry {
    async fn list_tools(&self) -> stasis::prelude::Result<Vec<genai::chat::Tool>> {
        self.inner.list_tools().await
    }

    async fn invoke_tool(&self, tool_name: &str, input: Value) -> stasis::prelude::Result<Value> {
        self.enforce_lane_safety(tool_name, &input)?;
        self.enforce_allowed_modules(tool_name, &input)?;
        self.inner.invoke_tool(tool_name, input).await
    }
}

fn lane_safety_action_for_tool_call(
    tool_name: &str,
    input: &Value,
) -> Option<LaneSafetyActionClass> {
    match tool_name {
        "cognition_runtime_mutate" => {
            let resource = input
                .get("resource")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let action = input
                .get("action")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            match (resource, action) {
                ("job", "enqueue") | ("workflow", "run") => {
                    Some(LaneSafetyActionClass::InteractiveIngress)
                }
                ("recurring", "register") | ("workflow", "schedule") => {
                    Some(LaneSafetyActionClass::RecurringRegistration)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn tool_policy_profile_for_tool_call(input: &Value) -> Option<&str> {
    input.get("policy_profile").and_then(|value| value.as_str())
}

fn referenced_module_ops_for_tool_call(
    tool_name: &str,
    input: &Value,
) -> stasis::prelude::Result<Vec<String>> {
    match tool_name {
        "cognition_runtime_mutate" => {
            if let Some(script) = input.get("script").and_then(|value| value.as_str())
                && !script.trim().is_empty()
            {
                return Ok(extract_module_ops_from_source(script));
            }
            let job_type = input
                .get("job_type")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if job_type != "workflow.grapheme.run" {
                return Ok(Vec::new());
            }
            let payload_ref = input
                .get("payload_ref")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "policy violation: payload_ref is required for workflow.grapheme.run"
                            .to_string(),
                    )
                })?;
            let source = grapheme_inline_payload_source(payload_ref).ok_or_else(|| {
                StasisError::PortFailure(
                    "policy violation: workflow.grapheme.run payload_ref must use grapheme:inline:<source>"
                        .to_string(),
                )
            })?;
            Ok(extract_module_ops_from_source(source))
        }
        "cognition_grapheme_run"
        | "cognition_grapheme_cli_run"
        | "cognition_grapheme_promote_to_job"
        | "cognition_grapheme_promote_to_recurring" => {
            let source = input
                .get("source")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    StasisError::PortFailure(format!(
                        "policy violation: {tool_name} requires source for module allowlist enforcement"
                    ))
                })?;
            Ok(extract_module_ops_from_source(source))
        }
        "cognition_grapheme_promote_last_run_to_recurring" => {
            let source = input
                .get("source")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "policy violation: source is required for promote_last_run_to_recurring when module allowlist is enabled"
                            .to_string(),
                    )
                })?;
            Ok(extract_module_ops_from_source(source))
        }
        "cognition_job_enqueue" => {
            let job_type = input
                .get("job_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if job_type != "workflow.grapheme.run" {
                return Ok(Vec::new());
            }

            let payload_ref = input
                .get("payload_ref")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "policy violation: payload_ref is required for workflow.grapheme.run"
                            .to_string(),
                    )
                })?;

            let source = grapheme_inline_payload_source(payload_ref).ok_or_else(|| {
                StasisError::PortFailure(
                    "policy violation: workflow.grapheme.run payload_ref must use grapheme:inline:<source>"
                        .to_string(),
                )
            })?;
            Ok(extract_module_ops_from_source(source))
        }
        "cognition_capability" => {
            let op = input
                .get("op")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !op.eq_ignore_ascii_case("invoke") {
                return Ok(Vec::new());
            }
            if let Some(script) = input.get("script").and_then(|value| value.as_str()) {
                return Ok(extract_module_ops_from_source(script));
            }
            if let Some(template) = input.get("template").and_then(|value| value.as_str()) {
                let params = input.get("params").cloned().unwrap_or_else(|| json!({}));
                let source = crate::bridge_tools::render_grapheme_template(template, &params)?;
                return Ok(extract_module_ops_from_source(&source));
            }
            Ok(Vec::new())
        }
        "cognition_grapheme_template_run" => {
            let template = input.get("template").and_then(|v| v.as_str()).ok_or_else(|| {
                StasisError::PortFailure(
                    "policy violation: cognition_grapheme_template_run requires template for module allowlist enforcement"
                        .to_string(),
                )
            })?;
            let params = input.get("params").cloned().unwrap_or_else(|| json!({}));
            let source = crate::bridge_tools::render_grapheme_template(template, &params)?;
            Ok(extract_module_ops_from_source(&source))
        }
        "cognition_capability_invoke" => {
            if let Some(source) = input.get("source").and_then(|v| v.as_str()) {
                return Ok(extract_module_ops_from_source(source));
            }
            Ok(Vec::new())
        }
        _ => Ok(Vec::new()),
    }
}

pub fn extract_module_ops_from_source(source: &str) -> Vec<String> {
    let mut ops = Vec::new();
    let chars = source.chars().collect::<Vec<_>>();
    let mut idx = 0usize;

    while idx < chars.len() {
        if !chars[idx].is_ascii_alphabetic() && chars[idx] != '_' {
            idx += 1;
            continue;
        }

        let start = idx;
        idx += 1;
        while idx < chars.len() && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '_') {
            idx += 1;
        }
        let left = chars[start..idx].iter().collect::<String>();

        if idx >= chars.len() || chars[idx] != '.' {
            continue;
        }
        idx += 1;

        if idx >= chars.len() || (!chars[idx].is_ascii_alphabetic() && chars[idx] != '_') {
            continue;
        }

        let right_start = idx;
        idx += 1;
        while idx < chars.len() && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '_') {
            idx += 1;
        }
        let right = chars[right_start..idx].iter().collect::<String>();

        let mut lookahead = idx;
        while lookahead < chars.len() && chars[lookahead].is_ascii_whitespace() {
            lookahead += 1;
        }

        if lookahead < chars.len() && chars[lookahead] == '(' {
            ops.push(format!("{left}.{right}"));
        }
    }

    ops.sort();
    ops.dedup();
    ops
}

// ── Registry builder ─────────────────────────────────────────────────────────

pub struct TuiRuntime {
    pub runtime: Arc<RuntimeComposition>,
    pub tool_loop_pipeline: MedousaToolLoopPipeline,
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub tool_catalog: Arc<crate::typed_tools::ToolCatalog>,
    pub capability_registry: Arc<RwLock<CapabilityRegistry>>,
    pub mcp_gateway_client: Arc<McpGatewayClient>,
    pub workflow_registry: Arc<crate::workflow::WorkflowRegistry>,
    pub locus_store: Arc<dyn NodeStore>,
    pub semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    pub medousa_identity_store: Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>,
    pub identity_memory_store: Arc<dyn IdentityMemoryStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations:
        Arc<dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations>,
    pub client_registry: crate::client_tools::ClientRegistry,
    pub execution_registry: crate::agent_runtime::execution_context::TurnExecutionRegistry,
    pub worker_scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
}

impl TuiRuntime {
    pub fn tool_loop_pipeline_for_target(
        &self,
        provider: &str,
        model: &str,
        base_url: Option<&str>,
    ) -> MedousaToolLoopPipeline {
        build_tool_loop_pipeline_for_target(provider, model, base_url, self.tool_registry.clone())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn build_tui_runtime(
    backend: RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
    workshop_operator_identity: bool,
    event_tx: mpsc::Sender<TuiEvent>,
) -> anyhow::Result<TuiRuntime> {
    build_tui_runtime_services(
        backend,
        provider,
        model,
        base_url,
        allowed_grapheme_modules,
        session_id,
        workshop_operator_identity,
        event_tx,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use genai::chat::Tool;
    use serde_json::json;
    use stasis::application::orchestration::tool_registry::{StasisTool, ToolRegistry};

    use super::{
        CognitionUtilityDayOfWeekTool, CognitionUtilityTimeNowTool, CognitionUtilityUuidTool,
        EngineExecutionLane, PolicyAwareToolRegistry, UtilityDayOfWeekInput, UtilityTimeNowInput,
        UtilityUuidInput, extract_module_ops_from_source, referenced_module_ops_for_tool_call,
    };

    #[derive(Default)]
    struct PassthroughToolRegistry;

    #[async_trait]
    impl ToolRegistry for PassthroughToolRegistry {
        async fn list_tools(&self) -> stasis::prelude::Result<Vec<Tool>> {
            Ok(Vec::new())
        }

        async fn invoke_tool(
            &self,
            tool_name: &str,
            _input: serde_json::Value,
        ) -> stasis::prelude::Result<serde_json::Value> {
            Ok(json!({ "status": "ok", "tool_name": tool_name }))
        }
    }

    #[test]
    fn extracts_dotted_module_ops_from_source_calls() {
        let source = r#"
            query Run {
                websearch.search(query: "rust") { items { title } }
                http.fetch(url: "https://example.com") { status }
                // not a call token
                helper.value
            }
        "#;

        let ops = extract_module_ops_from_source(source);
        assert_eq!(ops, vec!["http.fetch", "websearch.search"]);
    }

    #[test]
    fn detects_module_ops_for_grapheme_run_tool() {
        let input = json!({
            "source": "query Run { websearch.search(query: \"x\") { ok } }"
        });

        let ops = referenced_module_ops_for_tool_call("cognition_grapheme_run", &input)
            .expect("ops should parse");

        assert_eq!(ops, vec!["websearch.search"]);
    }

    #[test]
    fn detects_module_ops_for_capability_invoke_script() {
        let input = json!({
            "op": "invoke",
            "source": "grapheme",
            "script": "query Run { websearch.search(query: \"x\") { ok } }"
        });

        let ops = referenced_module_ops_for_tool_call("cognition_capability", &input)
            .expect("ops should parse");

        assert_eq!(ops, vec!["websearch.search"]);
    }

    #[test]
    fn detects_module_ops_for_runtime_mutate_script() {
        let input = json!({
            "resource": "job",
            "action": "enqueue",
            "script": "query Run { websearch.search(query: \"x\") { ok } }"
        });

        let ops = referenced_module_ops_for_tool_call("cognition_runtime_mutate", &input)
            .expect("ops should parse");

        assert_eq!(ops, vec!["websearch.search"]);
    }

    #[test]
    fn requires_source_for_promote_last_run_when_policy_active() {
        let input = json!({
            "cron_expr": "*/5 * * * *"
        });

        let result = referenced_module_ops_for_tool_call(
            "cognition_grapheme_promote_last_run_to_recurring",
            &input,
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn interactive_registry_allows_recurring_registration_tools() {
        let inner: Arc<dyn ToolRegistry> = Arc::new(PassthroughToolRegistry);
        let registry =
            PolicyAwareToolRegistry::new(inner, Vec::new(), EngineExecutionLane::Interactive);

        let result = registry
            .invoke_tool(
                "cognition_runtime_mutate",
                json!({
                    "resource": "recurring",
                    "action": "register",
                    "script": "query Run { websearch.search(query: \"rust\") { ok } }",
                    "cron_expr": "*/5 * * * *"
                }),
            )
            .await
            .expect("interactive lane should allow recurring registration by default");

        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn scheduled_registry_allows_recurring_registration_tools() {
        let inner: Arc<dyn ToolRegistry> = Arc::new(PassthroughToolRegistry);
        let registry =
            PolicyAwareToolRegistry::new(inner, Vec::new(), EngineExecutionLane::Scheduled);

        let result = registry
            .invoke_tool(
                "cognition_runtime_mutate",
                json!({
                    "resource": "recurring",
                    "action": "register",
                    "script": "query Run { websearch.search(query: \"rust\") { ok } }",
                    "cron_expr": "*/5 * * * *"
                }),
            )
            .await
            .expect("scheduled lane should allow recurring registration action");

        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn typed_utility_handlers_preserve_outputs_and_legacy_optional_inputs() {
        let weekday = CognitionUtilityDayOfWeekTool
            .invoke_typed(UtilityDayOfWeekInput {
                date: Some("2026-08-09".to_string()).into(),
            })
            .await
            .expect("known weekday");
        assert_eq!(weekday.date, "2026-08-09");
        assert_eq!(weekday.weekday, "Sun");
        assert_eq!(weekday.weekday_number_from_monday, 7);
        assert_eq!(weekday.weekday_number_from_sunday, 1);

        let error = CognitionUtilityDayOfWeekTool
            .invoke_typed(UtilityDayOfWeekInput {
                date: Some("not-a-date".to_string()).into(),
            })
            .await
            .expect_err("invalid date");
        assert!(
            error
                .to_string()
                .contains("cognition_utility_day_of_week: invalid date 'not-a-date'")
        );

        let legacy_weekday =
            StasisTool::invoke(&CognitionUtilityDayOfWeekTool, json!({ "date": 20260809 }))
                .await
                .expect("legacy non-string date remains equivalent to omission");
        assert!(legacy_weekday["weekday"].is_string());

        let uuid = CognitionUtilityUuidTool
            .invoke_typed(UtilityUuidInput {
                prefix: Some("phase-two".to_string()).into(),
            })
            .await
            .expect("typed UUID");
        assert!(uuid.correlation_id.starts_with("phase-two-"));
        assert!(uuid.trace_id.starts_with("phase-two-trace-"));
        assert!(uuid.idempotency_key.starts_with("idem-phase-two-"));
        assert_eq!(uuid.uuid_simple.len(), 32);

        let legacy_uuid = StasisTool::invoke(&CognitionUtilityUuidTool, json!({ "prefix": false }))
            .await
            .expect("legacy non-string prefix remains equivalent to omission");
        assert!(
            legacy_uuid["correlation_id"]
                .as_str()
                .is_some_and(|value| value.starts_with("cognition-"))
        );

        let time = CognitionUtilityTimeNowTool
            .invoke_typed(UtilityTimeNowInput {})
            .await
            .expect("typed current time");
        assert!(!time.utc_rfc3339.is_empty());
        assert!(!time.local_rfc3339.is_empty());
        assert!(time.unix_millis / 1_000 >= time.unix_seconds - 1);
    }
}
