//! Agent-facing Stasis runtime control tools (Phase D1).
//!
//! Design: docs/internal/runtime-tools-roadmap.md

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::ports::outbound::runtime::outbox_store::OutboxStore;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{BackoffPolicy, NewJob, StasisError};
use stasis::sdk::runtime_sdk::{RuntimeSdk, RuntimeStatsSnapshot};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::tools::validate_grapheme_source_for_schedule;
use crate::workflow::{
    MedousaSequentialWorkflowPayload, WORKFLOW_SEQUENTIAL_JOB_TYPE, WorkflowRecord,
    WorkflowRegistry, WorkflowRunRequest, WorkflowStatus, encode_workflow_payload,
    new_workflow_id, preflight_grapheme_steps, validate_workflow_request,
};

fn job_state_label(state: &JobState) -> &'static str {
    match state {
        JobState::Enqueued => "enqueued",
        JobState::Leased => "leased",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::DeadLetter => "dead_letter",
        JobState::Canceled => "canceled",
    }
}

fn parse_job_state_filter(value: &str) -> Option<JobState> {
    match value.trim().to_ascii_lowercase().as_str() {
        "enqueued" => Some(JobState::Enqueued),
        "leased" => Some(JobState::Leased),
        "running" => Some(JobState::Running),
        "succeeded" => Some(JobState::Succeeded),
        "failed" => Some(JobState::Failed),
        "dead_letter" | "deadletter" => Some(JobState::DeadLetter),
        "canceled" | "cancelled" => Some(JobState::Canceled),
        _ => None,
    }
}

fn job_to_json(job: &Job) -> Value {
    json!({
        "job_id": job.id,
        "queue": job.queue,
        "job_type": job.job_type,
        "payload_ref": job.payload_ref,
        "state": job_state_label(&job.state),
        "priority": job.priority,
        "attempts": job.attempts,
        "max_attempts": job.max_attempts,
        "correlation_id": job.correlation_id,
        "trace_id": job.trace_id,
        "scheduled_at_utc": job.scheduled_at.to_rfc3339(),
        "started_at_utc": job.started_at.map(|t| t.to_rfc3339()),
        "finished_at_utc": job.finished_at.map(|t| t.to_rfc3339()),
        "last_error": job.last_error,
    })
}

fn recurring_to_json(definition: &RecurringDefinition) -> Value {
    json!({
        "recurring_id": definition.id,
        "queue": definition.queue,
        "job_type": definition.job_type,
        "payload_template_ref": definition.payload_template_ref,
        "cron_expr": definition.cron_expr,
        "timezone": definition.timezone,
        "jitter_seconds": definition.jitter_seconds,
        "enabled": definition.enabled,
        "max_attempts": definition.max_attempts,
        "next_run_at_utc": definition.next_run_at.to_rfc3339(),
        "last_run_at_utc": definition.last_run_at.map(|t| t.to_rfc3339()),
    })
}

async fn list_jobs_by_state(
    runtime: &RuntimeComposition,
    state: JobState,
) -> stasis::prelude::Result<Vec<Job>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.list_by_state(state).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.list_by_state(state).await,
    }
}

async fn get_job(runtime: &RuntimeComposition, job_id: &str) -> stasis::prelude::Result<Option<Job>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await,
    }
}

async fn save_job(runtime: &RuntimeComposition, job: Job) -> stasis::prelude::Result<()> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.save(job).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.save(job).await,
    }
}

async fn list_recurring_definitions(
    runtime: &RuntimeComposition,
) -> stasis::prelude::Result<Vec<RecurringDefinition>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.recurring_store.list().await,
        RuntimeComposition::Surreal(rt) => rt.recurring_store.list().await,
    }
}

async fn save_recurring_definition(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> stasis::prelude::Result<()> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.recurring_store.save(definition).await,
        RuntimeComposition::Surreal(rt) => rt.recurring_store.save(definition).await,
    }
}

async fn register_recurring_definition(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> stasis::prelude::Result<()> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.register_recurring(definition).await,
        RuntimeComposition::Surreal(rt) => rt.register_recurring(definition).await,
    }
}

fn stats_to_json(snapshot: RuntimeStatsSnapshot) -> Value {
    json!({
        "enqueued_jobs": snapshot.enqueued_jobs,
        "running_jobs": snapshot.running_jobs,
        "succeeded_jobs": snapshot.succeeded_jobs,
        "failed_jobs": snapshot.failed_jobs,
        "dead_letter_jobs": snapshot.dead_letter_jobs,
        "pending_outbox_events": snapshot.pending_outbox_events,
        "recurring_definitions": snapshot.recurring_definitions,
    })
}

// ── cognition_runtime_jobs_list ───────────────────────────────────────────────

pub struct CognitionRuntimeJobsListTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionRuntimeJobsListTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeJobsListTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_jobs_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "List runtime jobs with optional state and correlation_id filters. \
             Defaults to enqueued and running jobs.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "state": {
                    "type": "string",
                    "description": "Optional filter: enqueued, leased, running, succeeded, failed, dead_letter, canceled"
                },
                "correlation_id": {
                    "type": "string",
                    "description": "Optional correlation_id filter (exact match)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max jobs to return (1-100, default 20)",
                    "minimum": 1,
                    "maximum": 100
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .clamp(1, 100) as usize;
        let correlation_id = input
            .get("correlation_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let states = if let Some(state_raw) = input.get("state").and_then(|v| v.as_str()) {
            let state = parse_job_state_filter(state_raw).ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "cognition_runtime_jobs_list: unknown state '{state_raw}'"
                ))
            })?;
            vec![state]
        } else {
            vec![JobState::Enqueued, JobState::Leased, JobState::Running]
        };

        let mut jobs = Vec::new();
        for state in states {
            let mut batch = list_jobs_by_state(self.runtime.as_ref(), state).await?;
            jobs.append(&mut batch);
        }

        if let Some(correlation_id) = correlation_id {
            jobs.retain(|job| job.correlation_id == correlation_id);
        }

        jobs.sort_by(|a, b| b.scheduled_at.cmp(&a.scheduled_at));
        jobs.truncate(limit);

        Ok(json!({
            "count": jobs.len(),
            "jobs": jobs.iter().map(job_to_json).collect::<Vec<_>>()
        }))
    }
}

// ── cognition_runtime_jobs_cancel ─────────────────────────────────────────────

pub struct CognitionRuntimeJobsCancelTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeJobsCancelTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeJobsCancelTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_jobs_cancel"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Cancel a pending runtime job (enqueued or leased).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "job_id": { "type": "string", "description": "Runtime job identifier" }
            },
            "required": ["job_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let job_id = input
            .get("job_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_jobs_cancel: job_id is required".to_string(),
                )
            })?;

        let Some(mut job) = get_job(self.runtime.as_ref(), job_id).await? else {
            return Ok(json!({
                "job_id": job_id,
                "status": "not_found"
            }));
        };

        let previous_state = job_state_label(&job.state).to_string();
        let cancelable = matches!(job.state, JobState::Enqueued | JobState::Leased);
        if !cancelable {
            return Ok(json!({
                "job_id": job_id,
                "status": "not_cancelable",
                "state": previous_state,
                "reason": "only enqueued or leased jobs can be canceled"
            }));
        }

        job.state = JobState::Canceled;
        job.finished_at = Some(Utc::now());
        save_job(self.runtime.as_ref(), job).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: job_id.to_string(),
            })
            .await;

        Ok(json!({
            "job_id": job_id,
            "status": "canceled",
            "previous_state": previous_state
        }))
    }
}

// ── cognition_runtime_recurring_list ──────────────────────────────────────────

pub struct CognitionRuntimeRecurringListTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionRuntimeRecurringListTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeRecurringListTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_recurring_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List registered recurring job definitions.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "enabled_only": {
                    "type": "boolean",
                    "description": "When true, return only enabled schedules",
                    "default": false
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let enabled_only = input
            .get("enabled_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut definitions = list_recurring_definitions(self.runtime.as_ref()).await?;
        if enabled_only {
            definitions.retain(|definition| definition.enabled);
        }
        definitions.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(json!({
            "count": definitions.len(),
            "recurring": definitions.iter().map(recurring_to_json).collect::<Vec<_>>()
        }))
    }
}

// ── cognition_runtime_recurring_register ──────────────────────────────────────

pub struct CognitionRuntimeRecurringRegisterTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeRecurringRegisterTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeRecurringRegisterTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_recurring_register"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Register a durable recurring schedule for Grapheme or other runtime job types. \
             Grapheme sources are preflight-validated before registration.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Grapheme source (required when job_type is workflow.grapheme.run)"
                },
                "job_type": {
                    "type": "string",
                    "description": "Runtime job handler",
                    "default": "workflow.grapheme.run"
                },
                "payload_template_ref": {
                    "type": "string",
                    "description": "Optional explicit payload template (overrides source)"
                },
                "cron_expr": { "type": "string", "description": "Cron expression" },
                "timezone": { "type": "string", "description": "IANA timezone", "default": "UTC" },
                "queue": { "type": "string", "description": "Runtime queue", "default": "default" },
                "recurring_id": { "type": "string", "description": "Optional recurring id" },
                "jitter_seconds": { "type": "integer", "default": 0 },
                "max_attempts": { "type": "integer", "default": 1 },
                "enabled": { "type": "boolean", "default": true },
                "start_immediately": { "type": "boolean", "default": false }
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
                    "cognition_runtime_recurring_register: cron_expr is required".to_string(),
                )
            })?;
        let job_type = input
            .get("job_type")
            .and_then(|v| v.as_str())
            .unwrap_or("workflow.grapheme.run");
        let payload_template_ref = if let Some(explicit) = input
            .get("payload_template_ref")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            explicit.to_string()
        } else if job_type == "workflow.grapheme.run" {
            let source = input.get("source").and_then(|v| v.as_str()).ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_recurring_register: source is required for workflow.grapheme.run"
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
                    "job_type": job_type,
                    "policy_message": "Refused recurring registration: Grapheme source failed runtime preflight.",
                    "validation": validation
                }));
            }
            format!("grapheme:inline:{source}")
        } else {
            return Err(StasisError::PortFailure(
                "cognition_runtime_recurring_register: payload_template_ref is required for non-grapheme job types"
                    .to_string(),
            ));
        };

        let recurring_id = input
            .get("recurring_id")
            .or_else(|| input.get("id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("recur-{}", Uuid::new_v4().simple()));
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
        let mut definition = RecurringDefinition {
            id: recurring_id.clone(),
            queue: queue.to_string(),
            job_type: job_type.to_string(),
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

        if start_immediately {
            definition.next_run_at = now;
        } else {
            definition.next_run_at = definition.compute_next_run_at(now)?;
        }

        register_recurring_definition(self.runtime.as_ref(), definition.clone()).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{recurring_id} @ {cron_expr}"),
            })
            .await;

        Ok(json!({
            "status": "registered",
            "recurring_id": recurring_id,
            "job_type": job_type,
            "cron_expr": cron_expr,
            "timezone": timezone,
            "next_run_at_utc": definition.next_run_at.to_rfc3339(),
            "enabled": enabled
        }))
    }
}

// ── cognition_runtime_recurring_pause / cancel ────────────────────────────────

pub struct CognitionRuntimeRecurringPauseTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeRecurringPauseTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeRecurringPauseTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_recurring_pause"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Pause a recurring schedule by setting enabled=false.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "recurring_id": { "type": "string", "description": "Recurring definition id" }
            },
            "required": ["recurring_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        set_recurring_enabled_for_runtime(
            self.runtime.as_ref(),
            &self.event_tx,
            self.name(),
            input,
            false,
            "paused",
        )
        .await
    }
}

pub struct CognitionRuntimeRecurringCancelTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeRecurringCancelTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeRecurringCancelTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_recurring_cancel"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Disable a recurring schedule (soft cancel). \
             The definition remains in the registry with enabled=false.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "recurring_id": { "type": "string", "description": "Recurring definition id" }
            },
            "required": ["recurring_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        set_recurring_enabled_for_runtime(
            self.runtime.as_ref(),
            &self.event_tx,
            self.name(),
            input,
            false,
            "canceled",
        )
        .await
    }
}

async fn set_recurring_enabled_for_runtime(
    runtime: &RuntimeComposition,
    event_tx: &mpsc::Sender<TuiEvent>,
    tool_name: &str,
    input: Value,
    enabled: bool,
    status_label: &str,
) -> stasis::prelude::Result<Value> {
    let recurring_id = input
        .get("recurring_id")
        .or_else(|| input.get("id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            StasisError::PortFailure(format!(
                "{tool_name}: recurring_id is required"
            ))
        })?;

    let definitions = list_recurring_definitions(runtime).await?;
    let Some(mut definition) = definitions
        .into_iter()
        .find(|definition| definition.id == recurring_id)
    else {
        return Ok(json!({
            "recurring_id": recurring_id,
            "status": "not_found"
        }));
    };

    if definition.enabled == enabled {
        return Ok(json!({
            "recurring_id": recurring_id,
            "status": if enabled { "already_enabled" } else { "already_disabled" },
            "enabled": definition.enabled
        }));
    }

    definition.enabled = enabled;
    save_recurring_definition(runtime, definition).await?;

    let _ = event_tx
        .send(TuiEvent::ToolInvoked {
            tool_name: tool_name.to_string(),
            input_summary: recurring_id.to_string(),
        })
        .await;

    Ok(json!({
        "recurring_id": recurring_id,
        "status": status_label,
        "enabled": enabled
    }))
}

// ── cognition_runtime_delivery_status ───────────────────────────────────────────

pub struct CognitionRuntimeDeliveryStatusTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionRuntimeDeliveryStatusTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeDeliveryStatusTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_delivery_status"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Summarize runtime queue, outbox, and recurring workload counts. \
             Includes pending outbox event previews when available.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "pending_limit": {
                    "type": "integer",
                    "description": "Max pending outbox rows to preview (1-50, default 10)",
                    "minimum": 1,
                    "maximum": 50
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let pending_limit = input
            .get("pending_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .clamp(1, 50) as usize;

        let sdk = RuntimeSdk::new(self.runtime.as_ref().clone());
        let snapshot = sdk.stats_snapshot(pending_limit).await?;

        let pending = match self.runtime.as_ref() {
            RuntimeComposition::InMemory(rt) => rt.outbox_store.list_pending(pending_limit).await?,
            RuntimeComposition::Surreal(rt) => rt.outbox_store.list_pending(pending_limit).await?,
        };

        let pending_preview = pending
            .iter()
            .map(|event| {
                json!({
                    "event_id": event.event_id,
                    "status": format!("{:?}", event.status),
                    "event_type": format!("{:?}", event.event.event_type),
                    "job_id": event.event.job_id,
                    "correlation_id": event.event.correlation_id,
                    "occurred_at_utc": event.event.occurred_at.to_rfc3339(),
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "stats": stats_to_json(snapshot),
            "pending_outbox_preview": pending_preview,
            "now_utc": Utc::now().to_rfc3339()
        }))
    }
}

// ── Phase D2 workflow composition ─────────────────────────────────────────────

fn parse_workflow_run_request(input: &Value) -> stasis::prelude::Result<WorkflowRunRequest> {
    serde_json::from_value(input.clone()).map_err(|error| {
        StasisError::PortFailure(format!(
            "invalid workflow request json: {error}"
        ))
    })
}

async fn validate_grapheme_steps_for_workflow(
    runtime: &RuntimeComposition,
    request: &WorkflowRunRequest,
) -> stasis::prelude::Result<Option<Value>> {
    let preflight = preflight_grapheme_steps(runtime, &request.steps).await?;
    for entry in &preflight {
        let validated = entry
            .get("validation")
            .and_then(|v| v.get("validated"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !validated {
            return Ok(Some(json!({
                "status": "rejected",
                "reason": "invalid_grapheme_source",
                "policy_message": "Refused workflow: one or more grapheme steps failed runtime preflight.",
                "grapheme_preflight": preflight
            })));
        }
    }
    Ok(None)
}

fn build_workflow_payload(
    workflow_id: &str,
    request: &WorkflowRunRequest,
    lane: &str,
) -> MedousaSequentialWorkflowPayload {
    MedousaSequentialWorkflowPayload {
        workflow_id: workflow_id.to_string(),
        name: request.name.clone(),
        strategy: request.strategy.clone(),
        mode: request.mode.clone(),
        on_failure: request.on_failure.clone(),
        note: request.note.clone(),
        lane: lane.to_string(),
        steps: request.steps.clone(),
    }
}

async fn enqueue_workflow_job(
    runtime: &RuntimeComposition,
    payload: &MedousaSequentialWorkflowPayload,
    queue: &str,
) -> stasis::prelude::Result<String> {
    let job_id = format!("wf-job-{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let job = NewJob {
        id: job_id.clone(),
        queue: queue.to_string(),
        job_type: WORKFLOW_SEQUENTIAL_JOB_TYPE.to_string(),
        payload_ref: encode_workflow_payload(payload)?,
        priority: 100,
        max_attempts: 1,
        idempotency_key: format!("idem-{job_id}"),
        correlation_id: payload.workflow_id.clone(),
        causation_id: "cognition_runtime_workflow".to_string(),
        trace_id: payload.workflow_id.clone(),
        sttp_input_node_id: format!("sttp:in:workflow:{}", payload.workflow_id),
        scheduled_at: now,
        backoff_policy: BackoffPolicy::default(),
    };

    match runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
    }
    Ok(job_id)
}

fn workflow_record_to_json(record: &WorkflowRecord, root_job: Option<&Job>) -> Value {
    json!({
        "workflow_id": record.workflow_id,
        "name": record.name,
        "status": record.status.as_str(),
        "strategy": record.strategy,
        "mode": record.mode,
        "on_failure": record.on_failure,
        "note": record.note,
        "root_job_id": record.root_job_id,
        "root_job_state": root_job.map(|job| job_state_label(&job.state)),
        "scheduled_recurring_id": record.scheduled_recurring_id,
        "created_at_utc": record.created_at.to_rfc3339(),
        "step_results": record.step_results,
    })
}

// ── cognition_runtime_workflow_run ────────────────────────────────────────────

pub struct CognitionRuntimeWorkflowRunTool {
    runtime: Arc<RuntimeComposition>,
    registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeWorkflowRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            runtime,
            registry,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeWorkflowRunTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_workflow_run"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Execute a declarative multi-step workflow now. \
             v1 supports sequential strategy with grapheme, prompt, and MCP steps.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Optional human-readable workflow name" },
                "strategy": { "type": "string", "default": "sequential" },
                "mode": { "type": "string", "default": "default" },
                "steps": {
                    "type": "array",
                    "description": "Ordered workflow steps (grapheme, prompt, or mcp)",
                    "items": { "type": "object" }
                },
                "on_failure": { "type": "string", "enum": ["stop", "continue"], "default": "stop" },
                "note": { "type": "string" },
                "queue": { "type": "string", "default": "default" }
            },
            "required": ["steps"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let request = parse_workflow_run_request(&input)?;
        validate_workflow_request(&request)?;
        if let Some(rejection) = validate_grapheme_steps_for_workflow(self.runtime.as_ref(), &request).await? {
            return Ok(rejection);
        }

        let workflow_id = new_workflow_id();
        let queue = input
            .get("queue")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let payload = build_workflow_payload(&workflow_id, &request, "interactive");
        let job_id = enqueue_workflow_job(self.runtime.as_ref(), &payload, queue).await?;

        let record = WorkflowRecord {
            workflow_id: workflow_id.clone(),
            name: request.name.clone(),
            strategy: request.strategy.clone(),
            mode: request.mode.clone(),
            on_failure: request.on_failure.clone(),
            note: request.note.clone(),
            root_job_id: job_id.clone(),
            status: WorkflowStatus::Enqueued,
            created_at: Utc::now(),
            scheduled_recurring_id: None,
            step_results: Vec::new(),
        };
        self.registry.insert(record).await;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: workflow_id.clone(),
            })
            .await;

        Ok(json!({
            "workflow_id": workflow_id,
            "status": "enqueued",
            "strategy": request.strategy,
            "job_ids": [job_id],
            "lane": "interactive"
        }))
    }
}

// ── cognition_runtime_workflow_schedule ───────────────────────────────────────

pub struct CognitionRuntimeWorkflowScheduleTool {
    runtime: Arc<RuntimeComposition>,
    registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeWorkflowScheduleTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            runtime,
            registry,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeWorkflowScheduleTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_workflow_schedule"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Register a recurring schedule for a declarative workflow. \
             Requires scheduled lane; grapheme steps are preflight-validated.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "strategy": { "type": "string", "default": "sequential" },
                "mode": { "type": "string", "default": "default" },
                "steps": { "type": "array", "items": { "type": "object" } },
                "on_failure": { "type": "string", "enum": ["stop", "continue"], "default": "stop" },
                "note": { "type": "string" },
                "queue": { "type": "string", "default": "default" },
                "cron_expr": { "type": "string", "description": "Cron expression" },
                "timezone": { "type": "string", "default": "UTC" },
                "recurring_id": { "type": "string" },
                "jitter_seconds": { "type": "integer", "default": 0 },
                "max_attempts": { "type": "integer", "default": 1 },
                "enabled": { "type": "boolean", "default": true },
                "start_immediately": { "type": "boolean", "default": false }
            },
            "required": ["steps", "cron_expr"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let request = parse_workflow_run_request(&input)?;
        validate_workflow_request(&request)?;
        if let Some(rejection) = validate_grapheme_steps_for_workflow(self.runtime.as_ref(), &request).await? {
            return Ok(rejection);
        }

        let cron_expr = input
            .get("cron_expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_workflow_schedule: cron_expr is required".to_string(),
                )
            })?;

        let workflow_id = new_workflow_id();
        let queue = input
            .get("queue")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let payload = build_workflow_payload(&workflow_id, &request, "scheduled");
        let payload_template_ref = encode_workflow_payload(&payload)?;

        let recurring_id = input
            .get("recurring_id")
            .or_else(|| input.get("id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("wf-recur-{}", Uuid::new_v4().simple()));
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
        let mut definition = RecurringDefinition {
            id: recurring_id.clone(),
            queue: queue.to_string(),
            job_type: WORKFLOW_SEQUENTIAL_JOB_TYPE.to_string(),
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

        if start_immediately {
            definition.next_run_at = now;
        } else {
            definition.next_run_at = definition.compute_next_run_at(now)?;
        }

        register_recurring_definition(self.runtime.as_ref(), definition.clone()).await?;

        let record = WorkflowRecord {
            workflow_id: workflow_id.clone(),
            name: request.name.clone(),
            strategy: request.strategy.clone(),
            mode: request.mode.clone(),
            on_failure: request.on_failure.clone(),
            note: request.note.clone(),
            root_job_id: String::new(),
            status: WorkflowStatus::Enqueued,
            created_at: now,
            scheduled_recurring_id: Some(recurring_id.clone()),
            step_results: Vec::new(),
        };
        self.registry.insert(record).await;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: format!("{workflow_id} @ {cron_expr}"),
            })
            .await;

        Ok(json!({
            "workflow_id": workflow_id,
            "status": "scheduled",
            "strategy": request.strategy,
            "recurring_id": recurring_id,
            "cron_expr": cron_expr,
            "timezone": timezone,
            "next_run_at_utc": definition.next_run_at.to_rfc3339(),
            "lane": "scheduled"
        }))
    }
}

// ── cognition_runtime_workflow_status ─────────────────────────────────────────

pub struct CognitionRuntimeWorkflowStatusTool {
    runtime: Arc<RuntimeComposition>,
    registry: Arc<WorkflowRegistry>,
}

impl CognitionRuntimeWorkflowStatusTool {
    pub fn new(runtime: Arc<RuntimeComposition>, registry: Arc<WorkflowRegistry>) -> Self {
        Self { runtime, registry }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeWorkflowStatusTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_workflow_status"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Aggregate status for a workflow by workflow_id.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "workflow_id": { "type": "string", "description": "Workflow identifier" }
            },
            "required": ["workflow_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let workflow_id = input
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_workflow_status: workflow_id is required".to_string(),
                )
            })?;

        let Some(record) = self.registry.get(workflow_id).await else {
            return Ok(json!({
                "workflow_id": workflow_id,
                "status": "not_found"
            }));
        };

        let root_job = if record.root_job_id.is_empty() {
            None
        } else {
            get_job(self.runtime.as_ref(), &record.root_job_id).await?
        };

        Ok(workflow_record_to_json(&record, root_job.as_ref()))
    }
}

// ── cognition_runtime_workflow_cancel ─────────────────────────────────────────

pub struct CognitionRuntimeWorkflowCancelTool {
    runtime: Arc<RuntimeComposition>,
    registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeWorkflowCancelTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            runtime,
            registry,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionRuntimeWorkflowCancelTool {
    fn name(&self) -> &'static str {
        "cognition_runtime_workflow_cancel"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Cancel a workflow: disable scheduled recurring (if any) and cancel pending root job.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "workflow_id": { "type": "string", "description": "Workflow identifier" }
            },
            "required": ["workflow_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let workflow_id = input
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_workflow_cancel: workflow_id is required".to_string(),
                )
            })?;

        let Some(record) = self.registry.get(workflow_id).await else {
            return Ok(json!({
                "workflow_id": workflow_id,
                "status": "not_found"
            }));
        };

        let mut recurring_disabled = false;
        if let Some(recurring_id) = record.scheduled_recurring_id.as_deref() {
            let definitions = list_recurring_definitions(self.runtime.as_ref()).await?;
            if let Some(mut definition) = definitions
                .into_iter()
                .find(|definition| definition.id == recurring_id)
            {
                if definition.enabled {
                    definition.enabled = false;
                    save_recurring_definition(self.runtime.as_ref(), definition).await?;
                    recurring_disabled = true;
                }
            }
        }

        let mut job_status = json!(null);
        if !record.root_job_id.is_empty() {
            if let Some(mut job) = get_job(self.runtime.as_ref(), &record.root_job_id).await? {
                let previous_state = job_state_label(&job.state).to_string();
                if matches!(job.state, JobState::Enqueued | JobState::Leased) {
                    job.state = JobState::Canceled;
                    job.finished_at = Some(Utc::now());
                    save_job(self.runtime.as_ref(), job).await?;
                    job_status = json!({
                        "job_id": record.root_job_id,
                        "status": "canceled",
                        "previous_state": previous_state
                    });
                } else {
                    job_status = json!({
                        "job_id": record.root_job_id,
                        "status": "not_cancelable",
                        "state": previous_state
                    });
                }
            }
        }

        self.registry.mark_canceled(workflow_id).await;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: workflow_id.to_string(),
            })
            .await;

        Ok(json!({
            "workflow_id": workflow_id,
            "status": "canceled",
            "recurring_disabled": recurring_disabled,
            "job": job_status
        }))
    }
}

#[cfg(test)]
mod tests {
    use stasis::prelude::{BackoffPolicy, NewJob, RuntimeBackend, StasisRuntimeBuilder};
    use stasis::sdk::runtime_sdk::RuntimeSdk;

    use super::*;

    #[tokio::test]
    async fn jobs_list_returns_enqueued_job() {
        let runtime = StasisRuntimeBuilder::new(RuntimeBackend::InMemory)
            .build()
            .await
            .expect("runtime");
        let sdk = RuntimeSdk::new(runtime.clone());
        let job_id = "test-job-list-1".to_string();
        sdk.enqueue(NewJob {
            id: job_id.clone(),
            queue: "default".to_string(),
            job_type: "workflow.grapheme.echo".to_string(),
            payload_ref: "echo:test".to_string(),
            priority: 100,
            max_attempts: 1,
            idempotency_key: "idem-test".to_string(),
            correlation_id: job_id.clone(),
            causation_id: "test".to_string(),
            trace_id: job_id.clone(),
            sttp_input_node_id: "sttp:test".to_string(),
            scheduled_at: Utc::now(),
            backoff_policy: BackoffPolicy::default(),
        })
        .await
        .expect("enqueue");

        let tool = CognitionRuntimeJobsListTool::new(Arc::new(runtime));
        let response = tool
            .invoke(json!({ "limit": 10 }))
            .await
            .expect("list jobs");
        let jobs = response
            .get("jobs")
            .and_then(|value| value.as_array())
            .expect("jobs array");
        assert!(jobs.iter().any(|job| job.get("job_id") == Some(&json!(job_id))));
    }

    #[tokio::test]
    async fn jobs_cancel_marks_job_canceled() {
        let runtime = StasisRuntimeBuilder::new(RuntimeBackend::InMemory)
            .build()
            .await
            .expect("runtime");
        let sdk = RuntimeSdk::new(runtime.clone());
        let job_id = "test-job-cancel-1".to_string();
        sdk.enqueue(NewJob {
            id: job_id.clone(),
            queue: "default".to_string(),
            job_type: "workflow.grapheme.echo".to_string(),
            payload_ref: "echo:test".to_string(),
            priority: 100,
            max_attempts: 1,
            idempotency_key: "idem-cancel".to_string(),
            correlation_id: job_id.clone(),
            causation_id: "test".to_string(),
            trace_id: job_id.clone(),
            sttp_input_node_id: "sttp:test".to_string(),
            scheduled_at: Utc::now(),
            backoff_policy: BackoffPolicy::default(),
        })
        .await
        .expect("enqueue");

        let (event_tx, _event_rx) = mpsc::channel(4);
        let cancel_tool =
            CognitionRuntimeJobsCancelTool::new(Arc::new(runtime.clone()), event_tx);
        let cancel_response = cancel_tool
            .invoke(json!({ "job_id": job_id }))
            .await
            .expect("cancel");
        assert_eq!(
            cancel_response.get("status").and_then(|v| v.as_str()),
            Some("canceled")
        );

        let job = get_job(&runtime, &job_id)
            .await
            .expect("get job")
            .expect("job exists");
        assert!(matches!(job.state, JobState::Canceled));
    }

    #[test]
    fn parse_job_state_filter_accepts_aliases() {
        assert!(parse_job_state_filter("dead_letter").is_some());
        assert!(parse_job_state_filter("cancelled").is_some());
        assert!(parse_job_state_filter("unknown").is_none());
    }
}
