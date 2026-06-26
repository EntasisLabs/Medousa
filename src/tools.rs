use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use locus_core_rs::NodeStore;
use serde_json::{Value, json};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use stasis::application::orchestration::tool_registry::{
    StasisTool, ToolRegistry,
};
use stasis::domain::runtime::job_attempt::JobAttemptOutcome;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::prelude::{
    BackoffPolicy, NewJob, RecurringDefinition,
    RuntimeBackend, RuntimeComposition, StasisError,
};
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};

use crate::capability_catalog::CapabilityRegistry;
use crate::engine_context::{
    EngineExecutionLane, LaneSafetyActionClass, validate_lane_action,
    validate_lane_policy_profile,
};
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::{
    GraphemeCompactionModelTarget, maybe_compact_output_to_sttp,
};
use crate::mcp_gateway_api::{McpDiscoverRequest, McpInvokeRequest, McpTurnContext, McpTurnLane};
use crate::mcp_gateway_client::McpGatewayClient;
use crate::mcp_turn_token::mint_mcp_turn_token;
use crate::process_once;
use crate::tui::runtime_services::{
    build_tool_loop_pipeline_for_target, build_tui_runtime_services,
};
use crate::recurring_delivery::{
    DeliveryResolveContext, ambient_from_turn_scope, bind_recurring_delivery_for_registration,
};
use crate::turn_continuation::{
    self, ContinuationAwaitMode, TurnContinuationScope, continuation_tool_metadata,
    wire_turn_child_job,
};

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

    let job = NewJob {
        id: job_id.clone(),
        queue: "default".to_string(),
        job_type: "workflow.grapheme.run".to_string(),
        payload_ref: format!("grapheme:inline:{source}"),
        priority: 100,
        max_attempts: 1,
        idempotency_key: format!("idem-{job_id}"),
        correlation_id: job_id.clone(),
        causation_id: causation.to_string(),
        trace_id: job_id.clone(),
        sttp_input_node_id: "sttp:in:cognition:grapheme:runtime".to_string(),
        scheduled_at: now,
        backoff_policy: BackoffPolicy::default(),
    };

    match &**runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
    }

    let _ = process_once(runtime, causation)
        .await
        .map_err(|e| StasisError::PortFailure(format!("runtime process_once failed: {e}")))?;

    let attempts = match &**runtime {
        RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(&job_id).await?,
        RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(&job_id).await?,
    };

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

static LAST_GRAPHEME_SOURCE: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn last_grapheme_source_store() -> &'static RwLock<Option<String>> {
    LAST_GRAPHEME_SOURCE.get_or_init(|| RwLock::new(None))
}

async fn remember_last_grapheme_source(source: &str) {
    let mut guard = last_grapheme_source_store().write().await;
    *guard = Some(source.to_string());
}

async fn read_last_grapheme_source() -> Option<String> {
    let guard = last_grapheme_source_store().read().await;
    guard.clone()
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
        if inline_notice_enabled {
            if let Some(size) = raw_output_bytes {
                let _ = event_tx
                    .send(TuiEvent::UiNotice(format!(
                        "◈ sttp_compaction tool={} status=inline bytes={} trigger_bytes={}",
                        tool_name, size, trigger_bytes
                    )))
                    .await;
            }
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
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionJobEnqueueTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionJobEnqueueTool {
    fn name(&self) -> &'static str {
        "cognition_job_enqueue"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Persist a job into the Stasis runtime for durable background execution. \
             Use this to schedule work: grapheme scripts, orchestration patterns, \
             memory operations, or any registered workflow handler. \
             Valid job_type values: workflow.grapheme.run, workflow.grapheme.echo, \
             workflow.stasis.orchestration.sequential, workflow.stasis.orchestration.concurrent, \
             workflow.stasis.orchestration.handoff, workflow.stasis.agent_session, \
             workflow.stasis.prompt, openshell.sandbox.run.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "job_type": {
                    "type": "string",
                    "description": "The job handler identifier, e.g. 'workflow.grapheme.run'"
                },
                "payload_ref": {
                    "type": "string",
                    "description": "Serialized job payload. For grapheme: 'grapheme:inline:<source>'. For JSON payloads: serialized JSON string."
                },
                "note": {
                    "type": "string",
                    "description": "Optional human-readable note about the intent of this job"
                }
            },
            "required": ["job_type", "payload_ref"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let job_type = input
            .get("job_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_job_enqueue: job_type is required".to_string())
            })?;
        let payload_ref = input
            .get("payload_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_job_enqueue: payload_ref is required".to_string(),
                )
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
                return Ok(json!({
                    "status": "rejected",
                    "reason": "invalid_grapheme_source",
                    "job_type": "workflow.grapheme.run",
                    "policy_message": "Refused scheduling: Grapheme source failed runtime preflight.",
                    "validation": validation,
                    "note": input.get("note").and_then(|v| v.as_str()).unwrap_or("")
                }));
            }
        }

        let job_id = format!("cognition-{}", Uuid::new_v4().simple());
        let now = Utc::now();

        let mut job = NewJob {
            id: job_id.clone(),
            queue: "default".to_string(),
            job_type: job_type.to_string(),
            payload_ref: payload_ref.to_string(),
            priority: 100,
            max_attempts: 1,
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            causation_id: "cognition_tui".to_string(),
            trace_id: job_id.clone(),
            sttp_input_node_id: "sttp:in:cognition:enqueue".to_string(),
            scheduled_at: now,
            backoff_policy: BackoffPolicy::default(),
        };

        if let Some(scope) = self.turn_scope.read().await.clone() {
            wire_turn_child_job(
                &mut job,
                &scope,
                self.name(),
                job_type,
                ContinuationAwaitMode::Async,
            )
            .await;
        }

        match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
            RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
        }

        let _ = self
            .event_tx
            .send(TuiEvent::JobEnqueued {
                job_id: job_id.clone(),
                job_type: job_type.to_string(),
            })
            .await;

        let mut response = json!({
            "job_id": job_id,
            "status": "enqueued",
            "note": input.get("note").and_then(|v| v.as_str()).unwrap_or("")
        });
        if let Some(scope) = self.turn_scope.read().await.clone() {
            if let Some(obj) = response.as_object_mut() {
                obj.insert(
                    "continuation".to_string(),
                    continuation_tool_metadata(
                        &scope,
                        &job_id,
                        ContinuationAwaitMode::Async,
                    ),
                );
            }
        }

        Ok(response)
    }
}

// ── CognitionGraphemeRunTool ─────────────────────────────────────────────────

pub struct CognitionGraphemeRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    model_target: GraphemeCompactionModelTarget,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionGraphemeRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        session_id: String,
        model_target: GraphemeCompactionModelTarget,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
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

#[async_trait]
impl StasisTool for CognitionGraphemeRunTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_run"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Execute a Grapheme script synchronously and return the result. \
             Grapheme is a typed workflow scripting language. Built-in modules in the \
             'grapheme/*' namespace are allowed by default (for example core, web). \
             Scripts run sandboxed with guardrails enforced. \
             Example source: import core from \"grapheme/core\"\nquery Run { \
             core.echo(message: \"hello\") { state { current } } }",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Complete Grapheme source code. Imports under 'grapheme/*' are allowed by default."
                }
            },
            "required": ["source"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_grapheme_run: source is required".to_string())
            })?;

        remember_last_grapheme_source(source).await;

        let job_id = format!("cognition-gph-{}", Uuid::new_v4().simple());
        let now = Utc::now();

        let mut job = NewJob {
            id: job_id.clone(),
            queue: "default".to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            payload_ref: format!("grapheme:inline:{source}"),
            priority: 100,
            max_attempts: 1,
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            causation_id: "cognition_tui".to_string(),
            trace_id: job_id.clone(),
            sttp_input_node_id: "sttp:in:cognition:grapheme".to_string(),
            scheduled_at: now,
            backoff_policy: BackoffPolicy::default(),
        };

        if let Some(scope) = self.turn_scope.read().await.clone() {
            wire_turn_child_job(
                &mut job,
                &scope,
                self.name(),
                "workflow.grapheme.run",
                ContinuationAwaitMode::Sync,
            )
            .await;
        }

        let continuation_meta = self.turn_scope.read().await.clone().map(|scope| {
            continuation_tool_metadata(&scope, &job_id, ContinuationAwaitMode::Sync)
        });

        match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
            RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
        }

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
                let attempts = match &*runtime_ref {
                    RuntimeComposition::InMemory(rt) => {
                        rt.job_attempt_store.list_by_job_id(&job_id).await
                    }
                    RuntimeComposition::Surreal(rt) => {
                        rt.job_attempt_store.list_by_job_id(&job_id).await
                    }
                };

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
        if let Some(meta) = continuation_meta {
            if let Some(obj) = raw_output.as_object_mut() {
                obj.insert("continuation".to_string(), meta);
            }
        }
        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await;
        let serialized_raw_output =
            serde_json::to_string(&raw_output).unwrap_or_else(|_| raw_output.to_string());

        let output = maybe_compact_output_to_sttp(
            self.name(),
            &session_id,
            raw_output,
            &self.model_target,
        )
        .await?;
        emit_compaction_observability(
            &self.event_tx,
            self.name(),
            &output,
            Some(serialized_raw_output.len()),
        )
        .await;
        Ok(output)
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

#[async_trait]
impl StasisTool for CognitionGraphemeModulesSearchTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_modules"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Search Grapheme modules by query. Mirrors: grapheme modules search <query> --yaml")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query, e.g. web" }
            },
            "required": ["query"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition_grapheme_modules: query is required".to_string())
        })?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        run_grapheme_cli(vec![
            "modules".to_string(),
            "search".to_string(),
            query.to_string(),
            "--yaml".to_string(),
        ])
        .await
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

#[async_trait]
impl StasisTool for CognitionGraphemeModulesInfoTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_modules_info"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Inspect Grapheme module metadata. Mirrors: grapheme modules info <module> --yaml")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "module": { "type": "string", "description": "Module id, e.g. web" }
            },
            "required": ["module"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let module = input
            .get("module")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_modules_info: module is required".to_string(),
                )
            })?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: module.to_string(),
            })
            .await;

        run_grapheme_cli(vec![
            "modules".to_string(),
            "info".to_string(),
            module.to_string(),
            "--yaml".to_string(),
        ])
        .await
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

#[async_trait]
impl StasisTool for CognitionGraphemeModulesOpsTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_modules_ops"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Inspect Grapheme module operations. Mirrors: grapheme modules ops <query> --yaml")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Module or op query, e.g. web" }
            },
            "required": ["query"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_modules_ops: query is required".to_string(),
            )
        })?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        run_grapheme_cli(vec![
            "modules".to_string(),
            "ops".to_string(),
            query.to_string(),
            "--yaml".to_string(),
        ])
        .await
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

#[async_trait]
impl StasisTool for CognitionGraphemeExamplesTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_examples"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List or show Grapheme examples. action=list|show")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "list or show",
                    "enum": ["list", "show"]
                },
                "name": {
                    "type": "string",
                    "description": "Example name for action=show"
                }
            },
            "required": ["action"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        let args = match action {
            "show" => {
                let name = input.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
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
                tool_name: self.name().to_string(),
                input_summary: action.to_string(),
            })
            .await;

        run_grapheme_cli(args).await
    }
}

pub struct CognitionGraphemeCliRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    model_target: GraphemeCompactionModelTarget,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionGraphemeCliRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        session_id: String,
        model_target: GraphemeCompactionModelTarget,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
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

#[async_trait]
impl StasisTool for CognitionGraphemeCliRunTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_cli_run"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Run grapheme code through Stasis runtime workflow execution (workflow.grapheme.run) using the same path as scheduled jobs.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Complete Grapheme script source" },
                "json": { "type": "boolean", "description": "Deprecated compatibility flag; runtime mode always returns JSON", "default": true },
                "stream_steps": { "type": "boolean", "description": "Deprecated compatibility flag; ignored in runtime mode", "default": true },
                "native_modules": { "type": "boolean", "description": "Deprecated compatibility flag; ignored in runtime mode", "default": false }
            },
            "required": ["source"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_cli_run: source is required".to_string(),
                )
            })?;

        remember_last_grapheme_source(source).await;
        let use_json = input.get("json").and_then(|v| v.as_bool()).unwrap_or(true);
        let stream_steps = input
            .get("stream_steps")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let native_modules = input
            .get("native_modules")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
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
        .await;

        let output =
            maybe_compact_output_to_sttp(self.name(), &session_id, result, &self.model_target)
                .await?;
        emit_compaction_observability(
            &self.event_tx,
            self.name(),
            &output,
            Some(serialized_raw_output.len()),
        )
        .await;
        Ok(output)
    }
}

pub struct CognitionGraphemePromoteToJobTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionGraphemePromoteToJobTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionGraphemePromoteToJobTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_promote_to_job"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Promote Grapheme source to a durable one-off runtime job (workflow.grapheme.run).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Complete Grapheme source" },
                "queue": { "type": "string", "description": "Runtime queue", "default": "default" },
                "priority": { "type": "integer", "description": "Job priority", "default": 100 },
                "max_attempts": { "type": "integer", "description": "Max job attempts", "default": 1 }
            },
            "required": ["source"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
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
            return Ok(json!({
                "status": "rejected",
                "reason": "invalid_grapheme_source",
                "job_type": "workflow.grapheme.run",
                "policy_message": "Refused promotion: Grapheme source failed runtime preflight.",
                "validation": validation
            }));
        }

        let queue = input
            .get("queue")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let priority = input
            .get("priority")
            .and_then(|v| v.as_i64())
            .unwrap_or(100) as i32;
        let max_attempts = input
            .get("max_attempts")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        let job_id = format!("cognition-promote-job-{}", Uuid::new_v4().simple());
        let now = Utc::now();

        let mut job = NewJob {
            id: job_id.clone(),
            queue: queue.to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            payload_ref: format!("grapheme:inline:{source}"),
            priority,
            max_attempts,
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            causation_id: "cognition_tui.promote".to_string(),
            trace_id: job_id.clone(),
            sttp_input_node_id: "sttp:in:cognition:grapheme:promote".to_string(),
            scheduled_at: now,
            backoff_policy: BackoffPolicy::default(),
        };

        if let Some(scope) = self.turn_scope.read().await.clone() {
            wire_turn_child_job(
                &mut job,
                &scope,
                self.name(),
                "workflow.grapheme.run",
                ContinuationAwaitMode::Async,
            )
            .await;
        }

        match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
            RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
        }

        let _ = self
            .event_tx
            .send(TuiEvent::JobEnqueued {
                job_id: job_id.clone(),
                job_type: "workflow.grapheme.run".to_string(),
            })
            .await;

        let mut response = json!({
            "job_id": job_id,
            "job_type": "workflow.grapheme.run",
            "queue": queue,
            "status": "enqueued",
            "validation": validation
        });
        if let Some(scope) = self.turn_scope.read().await.clone() {
            if let Some(obj) = response.as_object_mut() {
                obj.insert(
                    "continuation".to_string(),
                    continuation_tool_metadata(
                        &scope,
                        &job_id,
                        ContinuationAwaitMode::Async,
                    ),
                );
            }
        }

        Ok(response)
    }
}

pub struct CognitionGraphemePromoteToRecurringTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionGraphemePromoteToRecurringTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionGraphemePromoteToRecurringTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_promote_to_recurring"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Promote Grapheme source to a durable recurring schedule (register_recurring).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Complete Grapheme source" },
                "cron_expr": { "type": "string", "description": "7-field cron: sec min hour day-of-month month day-of-week year (e.g. 0 0 */4 * * * *)" },
                "timezone": { "type": "string", "description": "IANA timezone", "default": "UTC" },
                "queue": { "type": "string", "description": "Runtime queue", "default": "default" },
                "id": { "type": "string", "description": "Optional recurring id" },
                "jitter_seconds": { "type": "integer", "description": "Jitter seconds", "default": 0 },
                "max_attempts": { "type": "integer", "description": "Max attempts per materialized job", "default": 1 },
                "enabled": { "type": "boolean", "description": "Enabled schedule", "default": true },
                "start_immediately": { "type": "boolean", "description": "Set next_run_at=now", "default": false },
                "delivery": crate::recurring_delivery::delivery_spec_schema_fragment()["delivery"].clone()
            },
            "required": ["source", "cron_expr"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_promote_to_recurring: source is required".to_string(),
                )
            })?;
        let cron_expr = input
            .get("cron_expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
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
            return Ok(json!({
                "status": "rejected",
                "reason": "invalid_grapheme_source",
                "job_type": "workflow.grapheme.run",
                "policy_message": "Refused recurring registration: Grapheme source failed runtime preflight.",
                "validation": validation
            }));
        }

        let recurring_id = input
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("recur-gph-{}", Uuid::new_v4().simple()));
        let queue = input
            .get("queue")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let timezone = input
            .get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("UTC");
        let jitter_seconds = input
            .get("jitter_seconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let max_attempts = input
            .get("max_attempts")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        let enabled = input
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let start_immediately = input
            .get("start_immediately")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let now = Utc::now();
        let payload_template_ref = format!("grapheme:inline:{source}");

        let mut definition = RecurringDefinition {
            id: recurring_id.clone(),
            queue: queue.to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            payload_template_ref,
            cron_expr: cron_expr.to_string(),
            timezone: timezone.to_string(),
            jitter_seconds,
            enabled,
            max_attempts,
            next_run_at: now,
            last_run_at: None,
            lease_owner: None,
            lease_expires_at: None,
        };

        if !start_immediately {
            definition.next_run_at = definition.compute_next_run_at(now)?;
        }

        let scope = self.turn_scope.read().await.clone();
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_for_registration(
            &recurring_id,
            cron_expr,
            timezone,
            &input,
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id,
            },
        )
        .await?;

        match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.register_recurring(definition).await?,
            RuntimeComposition::Surreal(rt) => rt.register_recurring(definition).await?,
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{} @ {}", recurring_id, cron_expr),
            })
            .await;

        Ok(json!({
            "recurring_id": recurring_id,
            "job_type": "workflow.grapheme.run",
            "queue": queue,
            "cron_expr": cron_expr,
            "timezone": timezone,
            "enabled": enabled,
            "start_immediately": start_immediately,
            "status": "registered",
            "delivery_bound": delivery_bound,
            "validation": validation
        }))
    }
}

pub struct CognitionGraphemePromoteLastRunToRecurringTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionGraphemePromoteLastRunToRecurringTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionGraphemePromoteLastRunToRecurringTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_promote_last_run_to_recurring"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Promote the last executed Grapheme source to recurring schedule. You can also provide source explicitly.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "cron_expr": { "type": "string", "description": "7-field cron: sec min hour day-of-month month day-of-week year (e.g. 0 0 */4 * * * *)" },
                "timezone": { "type": "string", "description": "IANA timezone", "default": "UTC" },
                "queue": { "type": "string", "description": "Runtime queue", "default": "default" },
                "id": { "type": "string", "description": "Optional recurring id" },
                "jitter_seconds": { "type": "integer", "description": "Jitter seconds", "default": 0 },
                "max_attempts": { "type": "integer", "description": "Max attempts per materialized job", "default": 1 },
                "enabled": { "type": "boolean", "description": "Enabled schedule", "default": true },
                "start_immediately": { "type": "boolean", "description": "Set next_run_at=now", "default": false },
                "source": { "type": "string", "description": "Optional source override; if omitted, uses last remembered source" },
                "delivery": crate::recurring_delivery::delivery_spec_schema_fragment()["delivery"].clone()
            },
            "required": ["cron_expr"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let cron_expr = input
            .get("cron_expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_promote_last_run_to_recurring: cron_expr is required"
                        .to_string(),
                )
            })?;

        let source = if let Some(src) = input.get("source").and_then(|v| v.as_str()) {
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
            return Ok(json!({
                "status": "rejected",
                "reason": "invalid_grapheme_source",
                "job_type": "workflow.grapheme.run",
                "policy_message": "Refused recurring registration from last run: Grapheme source failed runtime preflight.",
                "used_remembered_source": input.get("source").is_none(),
                "validation": validation
            }));
        }

        let recurring_id = input
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("recur-gph-{}", Uuid::new_v4().simple()));
        let queue = input
            .get("queue")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let timezone = input
            .get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("UTC");
        let jitter_seconds = input
            .get("jitter_seconds")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let max_attempts = input
            .get("max_attempts")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        let enabled = input
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let start_immediately = input
            .get("start_immediately")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let now = Utc::now();
        let payload_template_ref = format!("grapheme:inline:{source}");

        let mut definition = RecurringDefinition {
            id: recurring_id.clone(),
            queue: queue.to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            payload_template_ref,
            cron_expr: cron_expr.to_string(),
            timezone: timezone.to_string(),
            jitter_seconds,
            enabled,
            max_attempts,
            next_run_at: now,
            last_run_at: None,
            lease_owner: None,
            lease_expires_at: None,
        };

        if !start_immediately {
            definition.next_run_at = definition.compute_next_run_at(now)?;
        }

        let scope = self.turn_scope.read().await.clone();
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_for_registration(
            &recurring_id,
            cron_expr,
            timezone,
            &input,
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id,
            },
        )
        .await?;

        match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.register_recurring(definition).await?,
            RuntimeComposition::Surreal(rt) => rt.register_recurring(definition).await?,
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{} @ {}", recurring_id, cron_expr),
            })
            .await;

        Ok(json!({
            "recurring_id": recurring_id,
            "job_type": "workflow.grapheme.run",
            "queue": queue,
            "cron_expr": cron_expr,
            "timezone": timezone,
            "enabled": enabled,
            "start_immediately": start_immediately,
            "used_remembered_source": input.get("source").is_none(),
            "status": "registered",
            "delivery_bound": delivery_bound,
            "validation": validation
        }))
    }
}

pub struct CognitionUtilityTimeNowTool;

#[async_trait]
impl StasisTool for CognitionUtilityTimeNowTool {
    fn name(&self) -> &'static str {
        "cognition_utility_time_now"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Return current time in UTC and local timezone, including weekday and unix timestamp.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {}
        }))
    }

    async fn invoke(&self, _input: Value) -> stasis::prelude::Result<Value> {
        let now_utc = Utc::now();
        let now_local = Local::now();

        Ok(json!({
            "utc_rfc3339": now_utc.to_rfc3339(),
            "local_rfc3339": now_local.to_rfc3339(),
            "weekday": now_local.weekday().to_string(),
            "unix_seconds": now_utc.timestamp(),
            "unix_millis": now_utc.timestamp_millis(),
            "local_offset_seconds": now_local.offset().local_minus_utc()
        }))
    }
}

pub struct CognitionUtilityDayOfWeekTool;

#[async_trait]
impl StasisTool for CognitionUtilityDayOfWeekTool {
    fn name(&self) -> &'static str {
        "cognition_utility_day_of_week"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Return weekday for a YYYY-MM-DD date, or for today when date is omitted.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "date": {
                    "type": "string",
                    "description": "Optional date in YYYY-MM-DD"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let date_opt = input.get("date").and_then(|v| v.as_str());

        let date = if let Some(date_str) = date_opt {
            NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
                StasisError::PortFailure(format!(
                    "cognition_utility_day_of_week: invalid date '{}': {}",
                    date_str, e
                ))
            })?
        } else {
            Local::now().date_naive()
        };

        Ok(json!({
            "date": date.format("%Y-%m-%d").to_string(),
            "weekday": date.weekday().to_string(),
            "weekday_number_from_monday": date.weekday().number_from_monday(),
            "weekday_number_from_sunday": date.weekday().number_from_sunday()
        }))
    }
}

pub struct CognitionUtilityUuidTool;

#[async_trait]
impl StasisTool for CognitionUtilityUuidTool {
    fn name(&self) -> &'static str {
        "cognition_utility_uuid"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Generate UUID helper values for correlation, trace, and idempotency keys.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prefix": {
                    "type": "string",
                    "description": "Optional prefix for derived keys"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let id = Uuid::new_v4();
        let prefix = input
            .get("prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("cognition");

        Ok(json!({
            "uuid": id.to_string(),
            "uuid_simple": id.simple().to_string(),
            "correlation_id": format!("{}-{}", prefix, id.simple()),
            "trace_id": format!("{}-trace-{}", prefix, id.simple()),
            "idempotency_key": format!("idem-{}-{}", prefix, id.simple())
        }))
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

#[async_trait]
impl StasisTool for CognitionRuntimeRecurringPreviewTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_recurring_preview"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Validate cron/timezone configuration and preview upcoming recurring run times.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "cron_expr": {
                    "type": "string",
                    "description": "Cron expression to validate"
                },
                "timezone": {
                    "type": "string",
                    "description": "IANA timezone",
                    "default": "UTC"
                },
                "count": {
                    "type": "integer",
                    "description": "How many future runs to preview (1-20, default 5)",
                    "minimum": 1,
                    "maximum": 20
                },
                "start_at": {
                    "type": "string",
                    "description": "Optional RFC3339 UTC start timestamp"
                }
            },
            "required": ["cron_expr"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let cron_expr = input
            .get("cron_expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_recurring_preview: cron_expr is required".to_string(),
                )
            })?;
        let timezone = input
            .get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("UTC");
        let count = input
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(1, 20) as usize;

        let base_time = if let Some(start_at) = input.get("start_at").and_then(|v| v.as_str()) {
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

        let definition = RecurringDefinition {
            id: "preview-only".to_string(),
            queue: "default".to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            payload_template_ref: "grapheme:inline:preview".to_string(),
            cron_expr: cron_expr.to_string(),
            timezone: timezone.to_string(),
            jitter_seconds: 0,
            enabled: true,
            max_attempts: 1,
            next_run_at: base_time,
            last_run_at: None,
            lease_owner: None,
            lease_expires_at: None,
        };

        let mut cursor = base_time;
        let mut preview: Vec<Value> = Vec::with_capacity(count);

        for _ in 0..count {
            let next_run = definition.compute_next_run_at(cursor)?;
            preview.push(json!({
                "run_at_utc": next_run.to_rfc3339(),
                "unix_seconds": next_run.timestamp()
            }));
            cursor = next_run + Duration::seconds(1);
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{} @ {}", cron_expr, timezone),
            })
            .await;

        Ok(json!({
            "valid": true,
            "cron_expr": cron_expr,
            "timezone": timezone,
            "start_at_utc": base_time.to_rfc3339(),
            "count": count,
            "preview": preview
        }))
    }
}

impl CognitionRuntimeJobStatusTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeJobStatusTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_jobs_status"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Inspect job attempts and latest execution status for a given job_id.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "Runtime job identifier"
                }
            },
            "required": ["job_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let job_id = input
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_jobs_status: job_id is required".to_string(),
                )
            })?;

        let attempts = match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(job_id).await?,
            RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(job_id).await?,
        };

        let last = attempts.last();
        let latest_outcome = last
            .map(|a| format!("{:?}", a.outcome))
            .unwrap_or_else(|| "Unknown".to_string());
        let execution_id = last.and_then(|a| a.execution_id.clone());
        let diagnostics = last.and_then(|a| a.diagnostics.clone());

        let attempts_summary: Vec<Value> = attempts
            .iter()
            .map(|a| {
                json!({
                    "attempt": a.attempt_number,
                    "outcome": format!("{:?}", a.outcome),
                    "execution_id": a.execution_id,
                    "started_at": a.started_at,
                    "finished_at": a.finished_at,
                    "diagnostics": a.diagnostics,
                })
            })
            .collect();

        Ok(json!({
            "job_id": job_id,
            "attempt_count": attempts.len(),
            "latest_outcome": latest_outcome,
            "latest_execution_id": execution_id,
            "latest_diagnostics": diagnostics,
            "attempts": attempts_summary
        }))
    }
}

// ── Capability catalog tools (Phase A) ───────────────────────────────────────

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

#[async_trait]
impl StasisTool for CognitionCapabilityResolveTool {
    fn name(&self) -> &'static str {
        "cognition_capability_resolve"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Resolve a capability intent to Grapheme and MCP implementations.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "capability": { "type": "string", "description": "Capability id, e.g. document_search" },
                "query": { "type": "string", "description": "Optional fuzzy query when capability id is unknown" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let capability_id = input
            .get("capability")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        if capability_id.is_none() && query.is_none() {
            return Err(StasisError::PortFailure(
                "cognition.capability.resolve: capability or query is required".to_string(),
            ));
        }

        let summary = capability_id
            .clone()
            .or(query.clone())
            .unwrap_or_default();
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: summary,
            })
            .await;

        let registry = self.capability_registry.read().await;
        if let Some(capability_id) = capability_id {
            let response = registry
                .resolve(&capability_id)
                .ok_or_else(|| {
                    StasisError::PortFailure(format!(
                        "cognition.capability.resolve: unknown capability '{capability_id}'"
                    ))
                })?;
            return Ok(serde_json::to_value(response).map_err(|error| {
                StasisError::PortFailure(format!(
                    "cognition.capability.resolve: failed to encode response: {error}"
                ))
            })?);
        }

        let search = registry.search(query.as_deref().unwrap_or_default(), 1);
        let Some(first) = search.matches.first() else {
            return Ok(json!({
                "capability": null,
                "matches": search.matches,
                "message": "no capabilities matched query"
            }));
        };

        let response = registry.resolve(&first.capability).ok_or_else(|| {
            StasisError::PortFailure(format!(
                "cognition.capability.resolve: matched capability '{}' but resolve failed",
                first.capability
            ))
        })?;
        Ok(serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition.capability.resolve: failed to encode response: {error}"
            ))
        })?)
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

#[async_trait]
impl StasisTool for CognitionCapabilityListTool {
    fn name(&self) -> &'static str {
        "cognition_capability_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List registered capability intents in the Medousa capability catalog.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prefix": { "type": "string", "description": "Optional capability id prefix filter" },
                "limit": { "type": "integer", "description": "Max entries (default 50)" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let prefix = input
            .get("prefix")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = input
            .get("limit")
            .and_then(|value| value.as_u64())
            .unwrap_or(50)
            .clamp(1, 200) as usize;

        let registry = self.capability_registry.read().await;
        let mut response = registry.list();
        if let Some(prefix) = prefix {
            response.capabilities.retain(|entry| entry.id.starts_with(prefix));
        }
        response.capabilities.truncate(limit);
        Ok(serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition.capability.list: failed to encode response: {error}"
            ))
        })?)
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

#[async_trait]
impl StasisTool for CognitionCapabilitySearchTool {
    fn name(&self) -> &'static str {
        "cognition_capability_search"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Keyword search capability intents by query, alias, or keywords.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max matches (default 10)" }
            },
            "required": ["query"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input.get("query").and_then(|value| value.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition.capability.search: query is required".to_string())
        })?;
        let limit = input
            .get("limit")
            .and_then(|value| value.as_u64())
            .unwrap_or(10)
            .clamp(1, 50) as usize;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let registry = self.capability_registry.read().await;
        let response = registry.search(query, limit);
        Ok(serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition.capability.search: failed to encode response: {error}"
            ))
        })?)
    }
}

pub struct CognitionMcpDiscoverTool {
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMcpDiscoverTool {
    pub fn new(
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
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

#[async_trait]
impl StasisTool for CognitionMcpDiscoverTool {
    fn name(&self) -> &'static str {
        "cognition_mcp_discover"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Search external MCP tools via the MCP Client gateway.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "server_id": { "type": "string", "description": "Optional MCP server id filter" },
                "limit": { "type": "integer", "description": "Max matches (default 20)" }
            },
            "required": ["query"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input.get("query").and_then(|value| value.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition.mcp.discover: query is required".to_string())
        })?;
        let server_id = input
            .get("server_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let limit = input
            .get("limit")
            .and_then(|value| value.as_u64())
            .unwrap_or(20)
            .clamp(1, 100) as usize;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await;
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

        Ok(serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition.mcp.discover: failed to encode response: {error}"
            ))
        })?)
    }
}

pub struct CognitionMcpInvokeTool {
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMcpInvokeTool {
    pub fn new(
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
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

#[async_trait]
impl StasisTool for CognitionMcpInvokeTool {
    fn name(&self) -> &'static str {
        "cognition_mcp_invoke"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Invoke an external MCP tool via the MCP Client gateway.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "server_id": { "type": "string" },
                "tool_name": { "type": "string" },
                "input": { "type": "object" },
                "turn_token": { "type": "string", "description": "Optional pre-minted turn token" },
                "approval_granted": {
                    "type": "boolean",
                    "description": "Set true after the operator approves a prior approval_required response"
                }
            },
            "required": ["server_id", "tool_name"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let server_id = input.get("server_id").and_then(|value| value.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition.mcp.invoke: server_id is required".to_string())
        })?;
        let tool_name = input.get("tool_name").and_then(|value| value.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition.mcp.invoke: tool_name is required".to_string())
        })?;
        let tool_input = input
            .get("input")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{server_id}.{tool_name}"),
            })
            .await;

        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await;
        let turn_context = build_agent_mcp_turn_context(&session_id);
        let turn_token = if let Some(token) = input.get("turn_token").and_then(|value| value.as_str()) {
            Some(token.to_string())
        } else {
            mint_mcp_turn_token(&turn_context).map_err(|error| {
                StasisError::PortFailure(format!("cognition.mcp.invoke: {error}"))
            })?
        };
        let operator_approval_granted = input
            .get("approval_granted")
            .and_then(|value| value.as_bool());

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

        if !response.ok {
            if let Some(error) = response.error.as_ref() {
                if error.code == "approval_required" {
                    let _ = self
                        .event_tx
                        .send(TuiEvent::ApprovalRequired {
                            server_id: server_id.to_string(),
                            tool_name: tool_name.to_string(),
                            reason: error.message.clone(),
                        })
                        .await;
                }
            }
        }

        Ok(serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition.mcp.invoke: failed to encode response: {error}"
            ))
        })?)
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

#[async_trait]
impl StasisTool for CognitionMcpServersTool {
    fn name(&self) -> &'static str {
        "cognition_mcp_servers"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List MCP servers known to the MCP Client gateway.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn invoke(&self, _input: Value) -> stasis::prelude::Result<Value> {
        let response = self
            .gateway_client
            .list_servers()
            .await
            .map_err(|error| StasisError::PortFailure(format!("cognition.mcp.servers: {error}")))?;
        Ok(serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition.mcp.servers: failed to encode response: {error}"
            ))
        })?)
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

    fn enforce_lane_safety(
        &self,
        tool_name: &str,
        input: &Value,
    ) -> stasis::prelude::Result<()> {
        if let Some(action) = lane_safety_action_for_tool_call(tool_name, input) {
            if let Err(reason) = validate_lane_action(self.lane, action) {
                return Err(StasisError::PortFailure(format!(
                    "lane safety violation: {reason}"
                )));
            }
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
    _input: &Value,
) -> Option<LaneSafetyActionClass> {
    match tool_name {
        "cognition_job_enqueue" | "cognition_grapheme_promote_to_job" | "cognition_runtime_workflow_run" | "cognition_mcp_promote_to_job" => {
            Some(LaneSafetyActionClass::InteractiveIngress)
        }
        "cognition_grapheme_promote_to_recurring"
        | "cognition_grapheme_promote_last_run_to_recurring"
        | "cognition_runtime_recurring_register"
        | "cognition_runtime_workflow_schedule" => {
            Some(LaneSafetyActionClass::RecurringRegistration)
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
    pub capability_registry: Arc<RwLock<CapabilityRegistry>>,
    pub mcp_gateway_client: Arc<McpGatewayClient>,
    pub workflow_registry: Arc<crate::workflow::WorkflowRegistry>,
    pub locus_store: Arc<dyn NodeStore>,
    pub semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    pub medousa_identity_store: Arc<crate::identity_store_ext::MedousaIdentityMemoryStore>,
    pub identity_memory_store: Arc<dyn IdentityMemoryStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub memory_operations: Arc<dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations>,
    pub turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
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
    use stasis::application::orchestration::tool_registry::ToolRegistry;

    use super::{
        EngineExecutionLane, PolicyAwareToolRegistry, extract_module_ops_from_source,
        referenced_module_ops_for_tool_call,
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
        let registry = PolicyAwareToolRegistry::new(inner, Vec::new(), EngineExecutionLane::Interactive);

        let result = registry
            .invoke_tool(
                "cognition_grapheme_promote_to_recurring",
                json!({
                    "source": "query Run { websearch.search(query: \"rust\") { ok } }",
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
        let registry = PolicyAwareToolRegistry::new(inner, Vec::new(), EngineExecutionLane::Scheduled);

        let result = registry
            .invoke_tool(
                "cognition_grapheme_promote_to_recurring",
                json!({
                    "source": "query Run { websearch.search(query: \"rust\") { ok } }",
                    "cron_expr": "*/5 * * * *"
                }),
            )
            .await
            .expect("scheduled lane should allow recurring registration action");

        assert_eq!(result["status"], "ok");
    }
}
