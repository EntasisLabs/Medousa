use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use locus_core_rs::NodeStore;
use serde_json::{Value, json};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use stasis::application::orchestration::prompt_pipeline::PromptExecutionPipeline;
use stasis::application::orchestration::tool_loop_pipeline::ToolLoopPipeline;
use stasis::application::orchestration::tool_registry::{
    InMemoryToolRegistry, StasisTool, ToolRegistry,
};
use stasis::domain::runtime::job_attempt::JobAttemptOutcome;
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::AiChatClient;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::prelude::{
    BackoffPolicy, MemoryRecallRequest, MemoryStoreRequest, NewJob, RecurringDefinition,
    RuntimeBackend, RuntimeComposition, StasisError, StasisRuntimeBuilder,
};
use stasis::prelude_ext::{
    LocusContextReader, LocusContextWriter, LocusNodeStoreFactory, MemoryContextReader,
    MemoryContextWriter,
};

use crate::events::TuiEvent;
use crate::engine_context::{
    EngineExecutionLane, LaneSafetyActionClass, validate_lane_action,
    validate_lane_policy_profile,
};
use crate::grapheme_sttp_compaction::{
    GraphemeCompactionModelTarget, maybe_compact_output_to_sttp,
};
use crate::process_once;

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

async fn run_grapheme_via_runtime(
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

async fn validate_grapheme_source_for_schedule(
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
}

impl CognitionJobEnqueueTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionJobEnqueueTool {
    fn name(&self) -> &'static str {
        "cognition.job.enqueue"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Persist a job into the Stasis runtime for durable background execution. \
             Use this to schedule work: grapheme scripts, orchestration patterns, \
             memory operations, or any registered workflow handler. \
             Valid job_type values: workflow.grapheme.run, workflow.grapheme.echo, \
             workflow.stasis.orchestration.sequential, workflow.stasis.orchestration.concurrent, \
             workflow.stasis.orchestration.handoff, workflow.stasis.agent_session, \
             workflow.stasis.prompt.",
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
                StasisError::PortFailure("cognition.job.enqueue: job_type is required".to_string())
            })?;
        let payload_ref = input
            .get("payload_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition.job.enqueue: payload_ref is required".to_string(),
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

        let job = NewJob {
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

        Ok(json!({
            "job_id": job_id,
            "status": "enqueued",
            "note": input.get("note").and_then(|v| v.as_str()).unwrap_or("")
        }))
    }
}

// ── CognitionGraphemeRunTool ─────────────────────────────────────────────────

pub struct CognitionGraphemeRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    model_target: GraphemeCompactionModelTarget,
}

impl CognitionGraphemeRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        session_id: String,
        model_target: GraphemeCompactionModelTarget,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            session_id,
            model_target,
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

        let job = NewJob {
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
        let raw_output = match process_once(&runtime_ref, "cognition_tui").await {
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
        let serialized_raw_output =
            serde_json::to_string(&raw_output).unwrap_or_else(|_| raw_output.to_string());

        let output = maybe_compact_output_to_sttp(
            self.name(),
            &self.session_id,
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

// ── CognitionMemoryStoreTool ─────────────────────────────────────────────────

pub struct CognitionMemoryStoreTool {
    writer: Arc<dyn MemoryContextWriter>,
    session_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryStoreTool {
    pub fn new(
        writer: Arc<dyn MemoryContextWriter>,
        session_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            writer,
            session_id,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryStoreTool {
    fn name(&self) -> &'static str {
        "cognition_memory_store"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Persist a memory node into the Locus memory store for future recall across turns. \
             Use this to remember important context, decisions, insights, or any information \
             that should survive beyond the current conversation window. \
             canonical example:

             ⊕⟨ ⏣0{ trigger: manual, response_format: temporal_node, origin_session: \"session-abc\", compression_depth: 1, parent_node: null, prime: { attractor_config: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85 }, context_summary: \"parser hardening session\", relevant_tier: raw, retrieval_budget: 8 } } ⟩
            ⦿⟨ ⏣0{ timestamp: \"2026-04-25T00:00:00Z\", tier: raw, session_id: \"session-abc\", schema_version: \"sttp-1.0\", user_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 }, model_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 } } ⟩
            ◈⟨ ⏣0{ focus(.99): \"grammar update\", decision(.96): { parser_mode(.95): \"strict_and_tolerant\" } } ⟩
            ⍉⟨ ⏣0{ rho: 0.95, kappa: 0.94, psi: 2.93, compression_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 } } ⟩            
             ",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The content to store. Should be a concise, self-contained statement."
                },
                "tier": {
                    "type": "string",
                    "description": "Memory tier: 'insight', 'context', or 'decision'. Defaults to 'context'."
                }
            },
            "required": ["content"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_memory_store: content is required".to_string())
            })?;
        let tier = input
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("context");

        let raw_node = json!({
            "content": content,
            "tier": tier
        })
        .to_string();

        let request = MemoryStoreRequest {
            session_id: self.session_id.clone(),
            raw_node,
        };

        let response = self.writer.store_context(&request).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: "cognition_memory_store".to_string(),
                input_summary: content.chars().take(50).collect(),
            })
            .await;

        Ok(json!({
            "node_id": response.node_id,
            "stored": response.valid,
            "validation_error": response.validation_error
        }))
    }
}

// ── CognitionMemoryRecallTool ────────────────────────────────────────────────

pub struct CognitionMemoryRecallTool {
    reader: Arc<dyn MemoryContextReader>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryRecallTool {
    pub fn new(reader: Arc<dyn MemoryContextReader>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { reader, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryRecallTool {
    fn name(&self) -> &'static str {
        "cognition_memory_recall"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Retrieve relevant memory nodes from the Locus store by semantic query. \
             Use this to surface previously stored context, decisions, or insights \
             relevant to the current moment of work.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language query using keywords describing what context to retrieve"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum nodes to retrieve (1–20, default 5)",
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition_memory_recall: query is required".to_string())
        })?;
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(20) as usize;

        let request = MemoryRecallRequest {
            query_text: Some(query.to_string()),
            limit,
            ..Default::default()
        };

        let response = self.reader.recall(&request).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: "cognition_memory_recall".to_string(),
                input_summary: query.chars().take(50).collect(),
            })
            .await;

        Ok(json!({
            "retrieved": response.retrieved,
            "node_sync_keys": response.node_sync_keys,
            "has_more": response.has_more,
            "retrieval_path": response.retrieval_path,
            "fallback_triggered": response.fallback_triggered
        }))
    }
}

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
}

impl CognitionGraphemeCliRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        session_id: String,
        model_target: GraphemeCompactionModelTarget,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            session_id,
            model_target,
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

        let output =
            maybe_compact_output_to_sttp(self.name(), &self.session_id, result, &self.model_target)
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
}

impl CognitionGraphemePromoteToJobTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
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

        let job = NewJob {
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

        Ok(json!({
            "job_id": job_id,
            "job_type": "workflow.grapheme.run",
            "queue": queue,
            "status": "enqueued",
            "validation": validation
        }))
    }
}

pub struct CognitionGraphemePromoteToRecurringTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemePromoteToRecurringTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
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
                "cron_expr": { "type": "string", "description": "Cron expression" },
                "timezone": { "type": "string", "description": "IANA timezone", "default": "UTC" },
                "queue": { "type": "string", "description": "Runtime queue", "default": "default" },
                "id": { "type": "string", "description": "Optional recurring id" },
                "jitter_seconds": { "type": "integer", "description": "Jitter seconds", "default": 0 },
                "max_attempts": { "type": "integer", "description": "Max attempts per materialized job", "default": 1 },
                "enabled": { "type": "boolean", "description": "Enabled schedule", "default": true },
                "start_immediately": { "type": "boolean", "description": "Set next_run_at=now", "default": false }
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
            "validation": validation
        }))
    }
}

pub struct CognitionGraphemePromoteLastRunToRecurringTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemePromoteLastRunToRecurringTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
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
                "cron_expr": { "type": "string", "description": "Cron expression" },
                "timezone": { "type": "string", "description": "IANA timezone", "default": "UTC" },
                "queue": { "type": "string", "description": "Runtime queue", "default": "default" },
                "id": { "type": "string", "description": "Optional recurring id" },
                "jitter_seconds": { "type": "integer", "description": "Jitter seconds", "default": 0 },
                "max_attempts": { "type": "integer", "description": "Max attempts per materialized job", "default": 1 },
                "enabled": { "type": "boolean", "description": "Enabled schedule", "default": true },
                "start_immediately": { "type": "boolean", "description": "Set next_run_at=now", "default": false },
                "source": { "type": "string", "description": "Optional source override; if omitted, uses last remembered source" }
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
            "validation": validation
        }))
    }
}

pub struct CognitionUtilityTimeNowTool;

#[async_trait]
impl StasisTool for CognitionUtilityTimeNowTool {
    fn name(&self) -> &'static str {
        "cognition_util_time_now"
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
        "cognition_util_time_day_of_week"
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
                    "cognition_util_time_day_of_week: invalid date '{}': {}",
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
        "cognition_util_id_uuid"
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
        "cognition_runtime_job_status"
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
                    "cognition_runtime_job_status job_id is required".to_string(),
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

#[derive(Clone)]
struct PolicyAwareToolRegistry {
    inner: Arc<dyn ToolRegistry>,
    allowed_module_ops: HashSet<String>,
    lane: EngineExecutionLane,
}

impl PolicyAwareToolRegistry {
    fn new(
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
        "cognition.job.enqueue" | "cognition_grapheme_promote_to_job" => {
            Some(LaneSafetyActionClass::InteractiveIngress)
        }
        "cognition_grapheme_promote_to_recurring"
        | "cognition_grapheme_promote_last_run_to_recurring" => {
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
        "cognition.job.enqueue" => {
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
    pub tool_loop_pipeline: ToolLoopPipeline,
    pub tool_registry: Arc<dyn ToolRegistry>,
    pub locus_store: Arc<dyn NodeStore>,
    pub identity_memory_store: Arc<dyn IdentityMemoryStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
}

impl TuiRuntime {
    pub fn tool_loop_pipeline_for_target(
        &self,
        provider: &str,
        model: &str,
        base_url: Option<&str>,
    ) -> ToolLoopPipeline {
        let resolved_provider = crate::resolve_llm_provider(Some(provider));
        let resolved_model = crate::resolve_llm_model(Some(model));
        let resolved_base_url = crate::resolve_llm_base_url(Some(&resolved_provider), base_url);
        let chat_client: Arc<dyn AiChatClient> = Arc::new(
            GenaiChatClient::from_provider_model_with_base_url(
                Some(&resolved_provider),
                &resolved_model,
                resolved_base_url.as_deref(),
            ),
        );
        let prompt_pipeline = PromptExecutionPipeline::new(chat_client);
        ToolLoopPipeline::new(prompt_pipeline, self.tool_registry.clone())
    }
}

pub async fn build_tui_runtime(
    backend: RuntimeBackend,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    allowed_grapheme_modules: Vec<String>,
    session_id: &str,
    event_tx: mpsc::Sender<TuiEvent>,
) -> anyhow::Result<TuiRuntime> {
    use std::sync::Arc;

    let backend_for_identity = backend.clone();

    let resolved_provider = crate::resolve_llm_provider(provider);
    let resolved_model = crate::resolve_llm_model(model);
    let resolved_base_url = crate::resolve_llm_base_url(Some(&resolved_provider), base_url);

    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&resolved_provider),
            &resolved_model,
            resolved_base_url.as_deref(),
        ),
    );

    let locus_store = LocusNodeStoreFactory::in_memory().await?;
    let memory_reader: Arc<dyn MemoryContextReader> =
        Arc::new(LocusContextReader::new(locus_store.clone()));
    let memory_writer: Arc<dyn MemoryContextWriter> =
        Arc::new(LocusContextWriter::new(locus_store.clone()));
    let identity_memory_store =
        crate::identity_memory::build_identity_memory_store_for_backend(&backend_for_identity)
            .await?;

    let runtime_composition = StasisRuntimeBuilder::new(backend)
        .with_chat_client(chat_client.clone())
        .with_memory_context_reader(memory_reader.clone())
        .with_memory_context_writer(memory_writer.clone())
        .with_identity_memory_store(identity_memory_store.clone())
        .build()
        .await?;

    let runtime = Arc::new(runtime_composition);

    let tool_registry = InMemoryToolRegistry::default();
    let compaction_target = GraphemeCompactionModelTarget {
        provider: resolved_provider.clone(),
        model: resolved_model.clone(),
        base_url: resolved_base_url.clone(),
    };
    tool_registry.register_tool(CognitionJobEnqueueTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemeRunTool::new(
        runtime.clone(),
        event_tx.clone(),
        session_id.to_string(),
        compaction_target.clone(),
    ))?;
    tool_registry.register_tool(CognitionMemoryStoreTool::new(
        memory_writer.clone(),
        session_id.to_string(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionMemoryRecallTool::new(
        memory_reader.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemeModulesSearchTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeModulesInfoTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeModulesOpsTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeExamplesTool::new(event_tx.clone()))?;
    tool_registry.register_tool(CognitionGraphemeCliRunTool::new(
        runtime.clone(),
        event_tx.clone(),
        session_id.to_string(),
        compaction_target,
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteToJobTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteToRecurringTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionGraphemePromoteLastRunToRecurringTool::new(
        runtime.clone(),
        event_tx.clone(),
    ))?;
    tool_registry.register_tool(CognitionUtilityTimeNowTool)?;
    tool_registry.register_tool(CognitionUtilityDayOfWeekTool)?;
    tool_registry.register_tool(CognitionUtilityUuidTool)?;
    tool_registry.register_tool(CognitionRuntimeJobStatusTool::new(runtime.clone()))?;
    tool_registry.register_tool(CognitionRuntimeRecurringPreviewTool::new(event_tx.clone()))?;

    let prompt_pipeline = PromptExecutionPipeline::new(chat_client);
    let base_registry: Arc<dyn ToolRegistry> = Arc::new(tool_registry);
    let guarded_registry: Arc<dyn ToolRegistry> = Arc::new(PolicyAwareToolRegistry::new(
        base_registry,
        allowed_grapheme_modules,
        EngineExecutionLane::Interactive,
    ));
    let tool_loop_pipeline = ToolLoopPipeline::new(prompt_pipeline, guarded_registry.clone());

    Ok(TuiRuntime {
        runtime,
        tool_loop_pipeline,
        tool_registry: guarded_registry,
        locus_store,
        identity_memory_store,
        memory_reader,
        memory_writer,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use genai::chat::Tool;
    use serde_json::json;
    use stasis::prelude::StasisError;
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
    async fn interactive_registry_blocks_recurring_registration_tools() {
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
            .await;

        let message = match result {
            Err(StasisError::PortFailure(message)) => message,
            Err(other) => panic!("unexpected error variant: {other}"),
            Ok(value) => panic!("expected lane safety violation, got success: {value}"),
        };

        assert!(message.contains("lane safety violation"));
        assert!(message.contains("action=recurring_registration"));
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
