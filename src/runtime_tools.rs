//! Agent-facing Stasis runtime control tools (Phase D1).
//!
//! Design: docs/internal/runtime-tools-roadmap.md

use std::sync::Arc;

use chrono::Utc;
use schemars::JsonSchema;
use schemars::schema::{ArrayValidation, InstanceType, Schema, SchemaObject, SingleOrVec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::ports::outbound::runtime::outbox_store::OutboxStore;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;
use stasis::prelude::StasisError;
use stasis::sdk::runtime_sdk::{RuntimeSdk, RuntimeStatsSnapshot};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::recurring_delivery::{
    DeliveryResolveContext, RecurringDeliverySpec, ambient_from_turn_scope,
    bind_recurring_delivery_spec_for_registration, delivery_binding_for_recurring,
};
use crate::recurring_feed::{RecurringFeedSpec, bind_recurring_feed_spec_for_registration};
use crate::tools::validate_grapheme_source_for_schedule;
use crate::turn_continuation::{
    ContinuationAwaitMode, StoredDeliveryTarget, TurnContinuationScope, continuation_tool_metadata,
    find_active_job_by_correlation_id, materialize_recurring_now, patch_existing_job_correlation,
    register_turn_child_job,
};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};
use crate::workflow::{
    MedousaWorkflowPayload, WORKFLOW_SEQUENTIAL_JOB_TYPE, WorkflowEnqueueContinuation,
    WorkflowRecord, WorkflowRegistry, WorkflowRunRequest, WorkflowStatus, WorkflowStepResult,
    WorkflowStepSpec, encode_workflow_payload, enqueue_workflow_job, new_workflow_id,
    preflight_grapheme_steps, validate_workflow_request, workflow_job_type_for_strategy,
};
use crate::workflow_plan::{WorkflowPlanRequest, plan_workflow_from_goal};

const COGNITION_RUNTIME_JOBS_LIST_ID: ToolId = ToolId::new("cognition_runtime_jobs_list");
const COGNITION_RUNTIME_JOBS_CANCEL_ID: ToolId = ToolId::new("cognition_runtime_jobs_cancel");
const COGNITION_RUNTIME_RECURRING_LIST_ID: ToolId = ToolId::new("cognition_runtime_recurring_list");
const COGNITION_RUNTIME_RECURRING_DOCTOR_ID: ToolId =
    ToolId::new("cognition_runtime_recurring_doctor");
const COGNITION_RUNTIME_DELIVERY_STATUS_ID: ToolId =
    ToolId::new("cognition_runtime_delivery_status");
const COGNITION_RUNTIME_WORKFLOW_STATUS_ID: ToolId =
    ToolId::new("cognition_runtime_workflow_status");
const COGNITION_RUNTIME_RECURRING_PAUSE_ID: ToolId =
    ToolId::new("cognition_runtime_recurring_pause");
const COGNITION_RUNTIME_RECURRING_CANCEL_ID: ToolId =
    ToolId::new("cognition_runtime_recurring_cancel");
const COGNITION_RUNTIME_RECURRING_REGISTER_ID: ToolId =
    ToolId::new("cognition_runtime_recurring_register");
const COGNITION_RUNTIME_WORKFLOW_RUN_ID: ToolId = ToolId::new("cognition_runtime_workflow_run");
const COGNITION_RUNTIME_WORKFLOW_SCHEDULE_ID: ToolId =
    ToolId::new("cognition_runtime_workflow_schedule");
const COGNITION_RUNTIME_WORKFLOW_CANCEL_ID: ToolId =
    ToolId::new("cognition_runtime_workflow_cancel");
const COGNITION_RUNTIME_WORKFLOW_PLAN_ID: ToolId = ToolId::new("cognition_runtime_workflow_plan");

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

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeJobSummary {
    job_id: String,
    queue: String,
    job_type: String,
    payload_ref: String,
    state: String,
    priority: i32,
    attempts: u32,
    max_attempts: u32,
    correlation_id: String,
    trace_id: String,
    scheduled_at_utc: String,
    started_at_utc: Option<String>,
    finished_at_utc: Option<String>,
    last_error: Option<String>,
}

fn job_summary(job: &Job) -> RuntimeJobSummary {
    RuntimeJobSummary {
        job_id: job.id.clone(),
        queue: job.queue.clone(),
        job_type: job.job_type.clone(),
        payload_ref: job.payload_ref.clone(),
        state: job_state_label(&job.state).to_string(),
        priority: job.priority,
        attempts: job.attempts,
        max_attempts: job.max_attempts,
        correlation_id: job.correlation_id.clone(),
        trace_id: job.trace_id.clone(),
        scheduled_at_utc: job.scheduled_at.to_rfc3339(),
        started_at_utc: job.started_at.map(|time| time.to_rfc3339()),
        finished_at_utc: job.finished_at.map(|time| time.to_rfc3339()),
        last_error: job.last_error.clone(),
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeDeliveryBinding {
    channel: String,
    channel_id: String,
    user_id: String,
    session_id: String,
    stream_id: Option<String>,
}

impl From<&StoredDeliveryTarget> for RuntimeDeliveryBinding {
    fn from(target: &StoredDeliveryTarget) -> Self {
        Self {
            channel: target.channel.clone(),
            channel_id: target.channel_id.clone(),
            user_id: target.user_id.clone(),
            session_id: target.session_id.clone(),
            stream_id: target.stream_id.clone(),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringSummary {
    recurring_id: String,
    queue: String,
    job_type: String,
    payload_template_ref: String,
    cron_expr: String,
    timezone: String,
    jitter_seconds: i64,
    enabled: bool,
    max_attempts: u32,
    next_run_at_utc: String,
    last_run_at_utc: Option<String>,
    manuscript_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery: Option<RuntimeDeliveryBinding>,
}

fn recurring_summary(
    definition: &RecurringDefinition,
    delivery: Option<&StoredDeliveryTarget>,
) -> RuntimeRecurringSummary {
    RuntimeRecurringSummary {
        recurring_id: definition.id.clone(),
        queue: definition.queue.clone(),
        job_type: definition.job_type.clone(),
        payload_template_ref: definition.payload_template_ref.clone(),
        cron_expr: definition.cron_expr.clone(),
        timezone: definition.timezone.clone(),
        jitter_seconds: definition.jitter_seconds,
        enabled: definition.enabled,
        max_attempts: definition.max_attempts,
        next_run_at_utc: definition.next_run_at.to_rfc3339(),
        last_run_at_utc: definition.last_run_at.map(|time| time.to_rfc3339()),
        manuscript_id: crate::recurring_agent_turn::manuscript_id_from_recurring_payload(
            &definition.job_type,
            &definition.payload_template_ref,
        ),
        delivery: delivery.map(RuntimeDeliveryBinding::from),
    }
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

async fn get_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> stasis::prelude::Result<Option<Job>> {
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

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeStatsOutput {
    enqueued_jobs: usize,
    running_jobs: usize,
    succeeded_jobs: usize,
    failed_jobs: usize,
    dead_letter_jobs: usize,
    pending_outbox_events: usize,
    recurring_definitions: usize,
}

impl From<RuntimeStatsSnapshot> for RuntimeStatsOutput {
    fn from(snapshot: RuntimeStatsSnapshot) -> Self {
        Self {
            enqueued_jobs: snapshot.enqueued_jobs,
            running_jobs: snapshot.running_jobs,
            succeeded_jobs: snapshot.succeeded_jobs,
            failed_jobs: snapshot.failed_jobs,
            dead_letter_jobs: snapshot.dead_letter_jobs,
            pending_outbox_events: snapshot.pending_outbox_events,
            recurring_definitions: snapshot.recurring_definitions,
        }
    }
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeJobsListInput {
    /// Optional filter: enqueued, leased, running, succeeded, failed, dead_letter, canceled
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    /// Optional correlation_id filter (exact match)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
    /// Max jobs to return (1-100, default 20)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 100),
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeJobsListOutput {
    count: usize,
    jobs: Vec<RuntimeJobSummary>,
}

#[medousa_tool(id = COGNITION_RUNTIME_JOBS_LIST_ID)]
impl CognitionRuntimeJobsListTool {
    /// List runtime jobs with optional state and correlation_id filters. Defaults to enqueued and running jobs.
    async fn invoke_typed(
        &self,
        input: RuntimeJobsListInput,
    ) -> stasis::prelude::Result<RuntimeJobsListOutput> {
        let limit = input.limit.unwrap_or(20).clamp(1, 100);
        let correlation_id = input
            .correlation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let states = if let Some(state_raw) = input.state.as_deref() {
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

        jobs.sort_by_key(|b| std::cmp::Reverse(b.scheduled_at));
        jobs.truncate(limit);

        Ok(RuntimeJobsListOutput {
            count: jobs.len(),
            jobs: jobs.iter().map(job_summary).collect(),
        })
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

#[derive(Debug, JsonSchema)]
pub struct RuntimeJobsCancelInput {
    /// Runtime job identifier
    #[schemars(required, with = "String")]
    job_id: Option<String>,
}

impl<'de> Deserialize<'de> for RuntimeJobsCancelInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            job_id: Option<String>,
        }
        Ok(Self {
            job_id: WireInput::deserialize(deserializer)?.job_id,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeJobsCancelOutput {
    NotFound {
        job_id: String,
        status: String,
    },
    NotCancelable {
        job_id: String,
        status: String,
        state: String,
        reason: String,
    },
    Canceled {
        job_id: String,
        status: String,
        previous_state: String,
    },
}

#[medousa_tool(id = COGNITION_RUNTIME_JOBS_CANCEL_ID)]
impl CognitionRuntimeJobsCancelTool {
    /// Cancel a pending runtime job (enqueued or leased).
    async fn invoke_typed(
        &self,
        input: RuntimeJobsCancelInput,
    ) -> stasis::prelude::Result<RuntimeJobsCancelOutput> {
        let job_id = input
            .job_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_jobs_cancel: job_id is required".to_string(),
                )
            })?;

        let Some(mut job) = get_job(self.runtime.as_ref(), job_id).await? else {
            return Ok(RuntimeJobsCancelOutput::NotFound {
                job_id: job_id.to_string(),
                status: "not_found".to_string(),
            });
        };

        let previous_state = job_state_label(&job.state).to_string();
        let cancelable = matches!(job.state, JobState::Enqueued | JobState::Leased);
        if !cancelable {
            return Ok(RuntimeJobsCancelOutput::NotCancelable {
                job_id: job_id.to_string(),
                status: "not_cancelable".to_string(),
                state: previous_state,
                reason: "only enqueued or leased jobs can be canceled".to_string(),
            });
        }

        job.state = JobState::Canceled;
        job.finished_at = Some(Utc::now());
        save_job(self.runtime.as_ref(), job).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_RUNTIME_JOBS_CANCEL_ID.as_str().to_string(),
                input_summary: job_id.to_string(),
            })
            .await;

        Ok(RuntimeJobsCancelOutput::Canceled {
            job_id: job_id.to_string(),
            status: "canceled".to_string(),
            previous_state,
        })
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

#[derive(Debug, JsonSchema)]
pub struct RuntimeRecurringListInput {
    /// When true, return only enabled schedules
    #[schemars(default)]
    enabled_only: bool,
}

impl<'de> Deserialize<'de> for RuntimeRecurringListInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            enabled_only: Option<bool>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            enabled_only: input.enabled_only.unwrap_or(false),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringListOutput {
    count: usize,
    recurring: Vec<RuntimeRecurringSummary>,
}

#[medousa_tool(id = COGNITION_RUNTIME_RECURRING_LIST_ID)]
impl CognitionRuntimeRecurringListTool {
    /// List registered recurring job definitions with optional channel delivery bindings.
    async fn invoke_typed(
        &self,
        input: RuntimeRecurringListInput,
    ) -> stasis::prelude::Result<RuntimeRecurringListOutput> {
        let mut definitions = list_recurring_definitions(self.runtime.as_ref()).await?;
        if input.enabled_only {
            definitions.retain(|definition| definition.enabled);
        }
        definitions.sort_by(|a, b| a.id.cmp(&b.id));

        let mut rows = Vec::with_capacity(definitions.len());
        for definition in &definitions {
            let delivery = delivery_binding_for_recurring(&definition.id).await;
            rows.push(recurring_summary(definition, delivery.as_ref()));
        }

        Ok(RuntimeRecurringListOutput {
            count: definitions.len(),
            recurring: rows,
        })
    }
}

// ── cognition_runtime_recurring_doctor ────────────────────────────────────────

pub struct CognitionRuntimeRecurringDoctorTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionRuntimeRecurringDoctorTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeRecurringDoctorInput {
    /// Optional single recurring id; omit to inspect all
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    recurring_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeFeedBinding {
    feed_ids: Vec<String>,
    payload_mode: String,
}

impl From<&crate::recurring_feed::RecurringFeedBinding> for RuntimeFeedBinding {
    fn from(binding: &crate::recurring_feed::RecurringFeedBinding) -> Self {
        Self {
            feed_ids: binding.feed_ids.clone(),
            payload_mode: binding.payload_mode.as_str().to_string(),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringDoctorEntry {
    recurring_id: String,
    queue: String,
    job_type: String,
    payload_template_ref: String,
    cron_expr: String,
    timezone: String,
    jitter_seconds: i64,
    enabled: bool,
    max_attempts: u32,
    next_run_at_utc: String,
    last_run_at_utc: Option<String>,
    manuscript_id: Option<String>,
    delivery: Option<RuntimeDeliveryBinding>,
    cron_valid: bool,
    delivery_bound: bool,
    push_ready: bool,
    feeds_bound: bool,
    feeds_binding: Option<RuntimeFeedBinding>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringDoctorOutput {
    count: usize,
    missing_delivery_bindings: usize,
    cron_warnings: usize,
    recurring: Vec<RuntimeRecurringDoctorEntry>,
    hints: Vec<String>,
}

#[medousa_tool(id = COGNITION_RUNTIME_RECURRING_DOCTOR_ID)]
impl CognitionRuntimeRecurringDoctorTool {
    /// Diagnose recurring schedules: cron, next run, enabled state, and channel delivery bindings.
    async fn invoke_typed(
        &self,
        input: RuntimeRecurringDoctorInput,
    ) -> stasis::prelude::Result<RuntimeRecurringDoctorOutput> {
        let filter_id = input
            .recurring_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let mut definitions = list_recurring_definitions(self.runtime.as_ref()).await?;
        if let Some(id) = filter_id.as_deref() {
            definitions.retain(|definition| definition.id == id);
        }
        definitions.sort_by(|a, b| a.id.cmp(&b.id));

        let mut entries = Vec::with_capacity(definitions.len());
        let mut missing_delivery = 0usize;
        let mut cron_warnings = 0usize;

        for definition in &definitions {
            let delivery = delivery_binding_for_recurring(&definition.id).await;
            if delivery.is_none() {
                missing_delivery += 1;
            }

            let cron_ok = crate::recurring_delivery::validate_recurring_cron(
                &definition.cron_expr,
                &definition.timezone,
            )
            .is_ok();
            if !cron_ok {
                cron_warnings += 1;
            }

            let feed_binding =
                crate::recurring_feed::feed_binding_for_recurring(&definition.id).await;
            entries.push(RuntimeRecurringDoctorEntry {
                recurring_id: definition.id.clone(),
                queue: definition.queue.clone(),
                job_type: definition.job_type.clone(),
                payload_template_ref: definition.payload_template_ref.clone(),
                cron_expr: definition.cron_expr.clone(),
                timezone: definition.timezone.clone(),
                jitter_seconds: definition.jitter_seconds,
                enabled: definition.enabled,
                max_attempts: definition.max_attempts,
                next_run_at_utc: definition.next_run_at.to_rfc3339(),
                last_run_at_utc: definition.last_run_at.map(|time| time.to_rfc3339()),
                manuscript_id: crate::recurring_agent_turn::manuscript_id_from_recurring_payload(
                    &definition.job_type,
                    &definition.payload_template_ref,
                ),
                delivery: delivery.as_ref().map(RuntimeDeliveryBinding::from),
                cron_valid: cron_ok,
                delivery_bound: delivery.is_some(),
                push_ready: delivery.is_some() && definition.enabled,
                feeds_bound: feed_binding.is_some(),
                feeds_binding: feed_binding.as_ref().map(RuntimeFeedBinding::from),
            });
        }

        Ok(RuntimeRecurringDoctorOutput {
            count: entries.len(),
            missing_delivery_bindings: missing_delivery,
            cron_warnings,
            recurring: entries,
            hints: vec![
                "Set delivery.telegram_chat_id or delivery.mode=linked_channel when registering from TUI after Telegram ingest on the same session.".to_string(),
                "Use delivery.mode=current_channel during an active ingest agent turn.".to_string(),
                "Cron uses 7 fields: sec min hour dom month dow year (e.g. 0 0 */4 * * * *).".to_string(),
            ],
        })
    }
}

// ── cognition_runtime_recurring_register ──────────────────────────────────────

pub struct CognitionRuntimeRecurringRegisterTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionRuntimeRecurringRegisterTool {
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

fn default_grapheme_job_type() -> String {
    "workflow.grapheme.run".to_string()
}

fn default_runtime_timezone() -> String {
    "UTC".to_string()
}

fn default_runtime_queue() -> String {
    "default".to_string()
}

fn default_zero_i64() -> i64 {
    0
}

fn default_one_u64() -> u64 {
    1
}

fn default_one_i64() -> i64 {
    1
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[derive(Debug, JsonSchema)]
pub struct RuntimeRecurringRegisterInput {
    /// Grapheme source (required when job_type is workflow.grapheme.run)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    /// Runtime job handler
    #[schemars(with = "String", default = "default_grapheme_job_type")]
    pub(crate) job_type: Option<String>,
    /// Optional explicit payload template (overrides source)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) payload_template_ref: Option<String>,
    /// 7-field cron: sec min hour day-of-month month day-of-week year (e.g. every 4h: 0 0 */4 * * * *)
    #[schemars(required, with = "String")]
    pub(crate) cron_expr: Option<String>,
    /// IANA timezone
    #[schemars(with = "String", default = "default_runtime_timezone")]
    pub(crate) timezone: Option<String>,
    /// Runtime queue
    #[schemars(with = "String", default = "default_runtime_queue")]
    pub(crate) queue: Option<String>,
    /// Optional recurring id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) recurring_id: Option<String>,
    #[schemars(with = "i64", default = "default_zero_i64")]
    pub(crate) jitter_seconds: Option<i64>,
    #[schemars(with = "i64", default = "default_one_i64")]
    pub(crate) max_attempts: Option<u64>,
    #[schemars(with = "bool", default = "default_true")]
    pub(crate) enabled: Option<bool>,
    #[schemars(with = "bool", default = "default_false")]
    pub(crate) start_immediately: Option<bool>,
    /// Where to push each successful run (independent of current UI channel). 7-field cron required separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "RecurringDeliverySpec",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) delivery: Option<RecurringDeliverySpec>,
    /// Environment feed ids to publish each materialized run terminal event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "RecurringFeedSpec", skip_serializing_if = "Option::is_none")]
    pub(crate) feeds: Option<RecurringFeedSpec>,
}

impl<'de> Deserialize<'de> for RuntimeRecurringRegisterInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            source: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            job_type: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            payload_template_ref: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            cron_expr: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            timezone: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            queue: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            recurring_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_i64"
            )]
            jitter_seconds: Option<i64>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
            )]
            max_attempts: Option<u64>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            enabled: Option<bool>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
            )]
            start_immediately: Option<bool>,
            #[serde(default)]
            delivery: Option<RecurringDeliverySpec>,
            #[serde(default)]
            feeds: Option<RecurringFeedSpec>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            source: input.source,
            job_type: input.job_type,
            payload_template_ref: input.payload_template_ref,
            cron_expr: input.cron_expr,
            timezone: input.timezone,
            queue: input.queue,
            recurring_id: input.recurring_id.or(input.id),
            jitter_seconds: input.jitter_seconds,
            max_attempts: input.max_attempts,
            enabled: input.enabled,
            start_immediately: input.start_immediately,
            delivery: input.delivery,
            feeds: input.feeds,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeRecurringRegisterOutput {
    Rejected {
        status: String,
        reason: String,
        job_type: String,
        policy_message: String,
        validation: ExternalJson,
    },
    Registered {
        status: String,
        recurring_id: String,
        job_type: String,
        cron_expr: String,
        timezone: String,
        next_run_at_utc: String,
        enabled: bool,
        delivery_bound: bool,
        feeds_bound: bool,
        live: bool,
        feeds_bound_recurring: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_RUNTIME_RECURRING_REGISTER_ID)]
impl CognitionRuntimeRecurringRegisterTool {
    /// Register a durable recurring schedule for Grapheme or other runtime job types. Grapheme sources are preflight-validated before registration.
    pub(crate) async fn invoke_typed(
        &self,
        input: RuntimeRecurringRegisterInput,
    ) -> stasis::prelude::Result<RuntimeRecurringRegisterOutput> {
        let cron_expr = input.cron_expr.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_runtime_recurring_register: cron_expr is required".to_string(),
            )
        })?;
        let job_type = input.job_type.as_deref().unwrap_or("workflow.grapheme.run");
        let payload_template_ref = if let Some(explicit) = input
            .payload_template_ref
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            explicit.to_string()
        } else if job_type == "workflow.grapheme.run" {
            let source = input.source.as_deref().ok_or_else(|| {
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
                return Ok(RuntimeRecurringRegisterOutput::Rejected {
                    status: "rejected".to_string(),
                    reason: "invalid_grapheme_source".to_string(),
                    job_type: job_type.to_string(),
                    policy_message:
                        "Refused recurring registration: Grapheme source failed runtime preflight."
                            .to_string(),
                    validation: ExternalJson::new(validation),
                });
            }
            format!("grapheme:inline:{source}")
        } else {
            return Err(StasisError::PortFailure(
                "cognition_runtime_recurring_register: payload_template_ref is required for non-grapheme job types"
                    .to_string(),
            ));
        };

        let recurring_id = input
            .recurring_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("recur-{}", Uuid::new_v4().simple()));
        let queue = input.queue.as_deref().unwrap_or("default");
        let timezone = input.timezone.as_deref().unwrap_or("UTC");
        let jitter_seconds = input.jitter_seconds.unwrap_or(0);
        let max_attempts = input.max_attempts.unwrap_or(1) as u32;
        let enabled = input.enabled.unwrap_or(true);
        let start_immediately = input.start_immediately.unwrap_or(false);

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

        let scope = self.turn_scope.read().await.clone();
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_spec_for_registration(
            &recurring_id,
            cron_expr,
            timezone,
            input.delivery.as_ref(),
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id: fallback_session_id.clone(),
            },
        )
        .await?;
        let (feeds_bound, _) =
            bind_recurring_feed_spec_for_registration(&recurring_id, input.feeds.as_ref()).await?;

        register_recurring_definition(self.runtime.as_ref(), definition.clone()).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_RUNTIME_RECURRING_REGISTER_ID.as_str().to_string(),
                input_summary: format!("{recurring_id} @ {cron_expr}"),
            })
            .await;

        let feeds_bound_recurring = if feeds_bound {
            input.feeds.map(|feeds| feeds.feed_ids).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(RuntimeRecurringRegisterOutput::Registered {
            status: "registered".to_string(),
            recurring_id,
            job_type: job_type.to_string(),
            cron_expr: cron_expr.to_string(),
            timezone: timezone.to_string(),
            next_run_at_utc: definition.next_run_at.to_rfc3339(),
            enabled,
            delivery_bound,
            feeds_bound,
            live: true,
            feeds_bound_recurring,
        })
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

#[derive(Debug, JsonSchema)]
pub struct RuntimeRecurringToggleInput {
    /// Recurring definition id
    #[schemars(required, with = "String")]
    recurring_id: Option<String>,
}

impl<'de> Deserialize<'de> for RuntimeRecurringToggleInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self {
            recurring_id: value
                .get("recurring_id")
                .or_else(|| value.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeRecurringToggleOutput {
    recurring_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}

#[medousa_tool(id = COGNITION_RUNTIME_RECURRING_PAUSE_ID)]
impl CognitionRuntimeRecurringPauseTool {
    /// Pause a recurring schedule by setting enabled=false.
    async fn invoke_typed(
        &self,
        input: RuntimeRecurringToggleInput,
    ) -> stasis::prelude::Result<RuntimeRecurringToggleOutput> {
        let recurring_id = required_recurring_toggle_id(
            input.recurring_id.as_deref(),
            COGNITION_RUNTIME_RECURRING_PAUSE_ID.as_str(),
        )?;
        set_recurring_enabled_for_runtime(
            self.runtime.as_ref(),
            &self.event_tx,
            COGNITION_RUNTIME_RECURRING_PAUSE_ID.as_str(),
            recurring_id,
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

#[medousa_tool(id = COGNITION_RUNTIME_RECURRING_CANCEL_ID)]
impl CognitionRuntimeRecurringCancelTool {
    /// Disable a recurring schedule (soft cancel). The definition remains in the registry with enabled=false.
    async fn invoke_typed(
        &self,
        input: RuntimeRecurringToggleInput,
    ) -> stasis::prelude::Result<RuntimeRecurringToggleOutput> {
        let recurring_id = required_recurring_toggle_id(
            input.recurring_id.as_deref(),
            COGNITION_RUNTIME_RECURRING_CANCEL_ID.as_str(),
        )?;
        set_recurring_enabled_for_runtime(
            self.runtime.as_ref(),
            &self.event_tx,
            COGNITION_RUNTIME_RECURRING_CANCEL_ID.as_str(),
            recurring_id,
            false,
            "canceled",
        )
        .await
    }
}

fn required_recurring_toggle_id<'a>(
    recurring_id: Option<&'a str>,
    tool_name: &str,
) -> stasis::prelude::Result<&'a str> {
    recurring_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| StasisError::PortFailure(format!("{tool_name}: recurring_id is required")))
}

async fn set_recurring_enabled_for_runtime(
    runtime: &RuntimeComposition,
    event_tx: &mpsc::Sender<TuiEvent>,
    tool_name: &str,
    recurring_id: &str,
    enabled: bool,
    status_label: &str,
) -> stasis::prelude::Result<RuntimeRecurringToggleOutput> {
    let definitions = list_recurring_definitions(runtime).await?;
    let Some(mut definition) = definitions
        .into_iter()
        .find(|definition| definition.id == recurring_id)
    else {
        return Ok(RuntimeRecurringToggleOutput {
            recurring_id: recurring_id.to_string(),
            status: "not_found".to_string(),
            enabled: None,
        });
    };

    if definition.enabled == enabled {
        return Ok(RuntimeRecurringToggleOutput {
            recurring_id: recurring_id.to_string(),
            status: if enabled {
                "already_enabled".to_string()
            } else {
                "already_disabled".to_string()
            },
            enabled: Some(definition.enabled),
        });
    }

    definition.enabled = enabled;
    save_recurring_definition(runtime, definition).await?;

    let _ = event_tx
        .send(TuiEvent::ToolInvoked {
            tool_name: tool_name.to_string(),
            input_summary: recurring_id.to_string(),
        })
        .await;

    Ok(RuntimeRecurringToggleOutput {
        recurring_id: recurring_id.to_string(),
        status: status_label.to_string(),
        enabled: Some(enabled),
    })
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeDeliveryStatusInput {
    /// Max pending outbox rows to preview (1-50, default 10)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 50),
        skip_serializing_if = "Option::is_none"
    )]
    pending_limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PendingOutboxPreview {
    event_id: String,
    status: String,
    event_type: String,
    job_id: String,
    correlation_id: String,
    occurred_at_utc: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeDeliveryStatusOutput {
    stats: RuntimeStatsOutput,
    pending_outbox_preview: Vec<PendingOutboxPreview>,
    now_utc: String,
}

#[medousa_tool(id = COGNITION_RUNTIME_DELIVERY_STATUS_ID)]
impl CognitionRuntimeDeliveryStatusTool {
    /// Summarize runtime queue, outbox, and recurring workload counts. Includes pending outbox event previews when available.
    async fn invoke_typed(
        &self,
        input: RuntimeDeliveryStatusInput,
    ) -> stasis::prelude::Result<RuntimeDeliveryStatusOutput> {
        let pending_limit = input.pending_limit.unwrap_or(10).clamp(1, 50);

        let sdk = RuntimeSdk::new(self.runtime.as_ref().clone());
        let snapshot = sdk.stats_snapshot(pending_limit).await?;

        let pending = match self.runtime.as_ref() {
            RuntimeComposition::InMemory(rt) => rt.outbox_store.list_pending(pending_limit).await?,
            RuntimeComposition::Surreal(rt) => rt.outbox_store.list_pending(pending_limit).await?,
        };

        let pending_preview = pending
            .iter()
            .map(|event| PendingOutboxPreview {
                event_id: event.event_id.clone(),
                status: format!("{:?}", event.status),
                event_type: format!("{:?}", event.event.event_type),
                job_id: event.event.job_id.clone(),
                correlation_id: event.event.correlation_id.clone(),
                occurred_at_utc: event.event.occurred_at.to_rfc3339(),
            })
            .collect::<Vec<_>>();

        Ok(RuntimeDeliveryStatusOutput {
            stats: snapshot.into(),
            pending_outbox_preview: pending_preview,
            now_utc: Utc::now().to_rfc3339(),
        })
    }
}

// ── Phase D2 workflow composition ─────────────────────────────────────────────

async fn validate_grapheme_steps_for_workflow(
    runtime: &RuntimeComposition,
    request: &WorkflowRunRequest,
) -> stasis::prelude::Result<Option<RuntimeWorkflowPreflightRejection>> {
    let preflight = preflight_grapheme_steps(runtime, &request.steps).await?;
    for entry in &preflight {
        let validated = entry
            .get("validation")
            .and_then(|v| v.get("validated"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !validated {
            return Ok(Some(RuntimeWorkflowPreflightRejection {
                status: "rejected".to_string(),
                reason: "invalid_grapheme_source".to_string(),
                policy_message:
                    "Refused workflow: one or more grapheme steps failed runtime preflight."
                        .to_string(),
                grapheme_preflight: ExternalJson::new(Value::Array(preflight)),
            }));
        }
    }
    Ok(None)
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct CompatibleWorkflowSteps(Vec<WorkflowStepSpec>);

impl JsonSchema for CompatibleWorkflowSteps {
    fn schema_name() -> String {
        "CompatibleWorkflowSteps".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::Array.into()),
            array: Some(Box::new(ArrayValidation {
                items: Some(SingleOrVec::Single(Box::new(Schema::Object(
                    SchemaObject {
                        instance_type: Some(InstanceType::Object.into()),
                        ..SchemaObject::default()
                    },
                )))),
                ..ArrayValidation::default()
            })),
            ..SchemaObject::default()
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStrategyInput {
    #[default]
    Sequential,
    Concurrent,
    Handoff,
}

impl WorkflowStrategyInput {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::Concurrent => "concurrent",
            Self::Handoff => "handoff",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFailureInput {
    #[default]
    Stop,
    Continue,
}

impl WorkflowFailureInput {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Continue => "continue",
        }
    }
}

fn default_workflow_mode() -> String {
    "default".to_string()
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeWorkflowPreflightRejection {
    status: String,
    reason: String,
    policy_message: String,
    grapheme_preflight: ExternalJson,
}

fn build_workflow_payload(
    workflow_id: &str,
    request: &WorkflowRunRequest,
    lane: &str,
) -> MedousaWorkflowPayload {
    MedousaWorkflowPayload {
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

// ── cognition_runtime_workflow_run ────────────────────────────────────────────

pub struct CognitionRuntimeWorkflowRunTool {
    runtime: Arc<RuntimeComposition>,
    registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionRuntimeWorkflowRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            registry,
            event_tx,
            turn_scope,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeWorkflowRunInput {
    /// Optional human-readable workflow name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    #[schemars(default)]
    strategy: WorkflowStrategyInput,
    #[serde(default = "default_workflow_mode")]
    #[schemars(default = "default_workflow_mode")]
    mode: String,
    /// Ordered workflow steps (grapheme, prompt, or mcp)
    steps: CompatibleWorkflowSteps,
    #[serde(default)]
    #[schemars(default)]
    on_failure: WorkflowFailureInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default = "default_runtime_queue")]
    #[schemars(default = "default_runtime_queue")]
    queue: String,
}

impl RuntimeWorkflowRunInput {
    fn request(&self) -> WorkflowRunRequest {
        WorkflowRunRequest {
            name: self.name.clone(),
            strategy: self.strategy.as_str().to_string(),
            mode: self.mode.clone(),
            steps: self.steps.0.clone(),
            on_failure: self.on_failure.as_str().to_string(),
            note: self.note.clone(),
            queue: Some(self.queue.clone()),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeWorkflowRunEnqueued {
    workflow_id: String,
    status: String,
    strategy: String,
    job_ids: Vec<String>,
    root_job_id: String,
    job_type: String,
    lane: String,
    continuation: Option<ExternalJson>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeWorkflowRunOutput {
    Rejected(RuntimeWorkflowPreflightRejection),
    Enqueued(RuntimeWorkflowRunEnqueued),
}

#[medousa_tool(id = COGNITION_RUNTIME_WORKFLOW_RUN_ID)]
impl CognitionRuntimeWorkflowRunTool {
    /// Execute a declarative multi-step workflow now. Strategies: sequential (default), concurrent (parallel read-only steps), handoff (sequential with $handoff.context).
    async fn invoke_typed(
        &self,
        input: RuntimeWorkflowRunInput,
    ) -> stasis::prelude::Result<RuntimeWorkflowRunOutput> {
        let request = input.request();
        validate_workflow_request(&request)?;
        if let Some(rejection) =
            validate_grapheme_steps_for_workflow(self.runtime.as_ref(), &request).await?
        {
            return Ok(RuntimeWorkflowRunOutput::Rejected(rejection));
        }

        let workflow_id = new_workflow_id();
        let payload = build_workflow_payload(&workflow_id, &request, "interactive");
        let scope = self.turn_scope.read().await.clone();
        let continuation = scope
            .as_ref()
            .map(|turn_scope| WorkflowEnqueueContinuation {
                turn_scope,
                tool_name: COGNITION_RUNTIME_WORKFLOW_RUN_ID.as_str(),
                await_mode: ContinuationAwaitMode::Async,
            });
        let job_id =
            enqueue_workflow_job(self.runtime.as_ref(), &payload, &input.queue, continuation)
                .await?;
        let job_type = workflow_job_type_for_strategy(&request.strategy)
            .unwrap_or(WORKFLOW_SEQUENTIAL_JOB_TYPE);

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
                tool_name: COGNITION_RUNTIME_WORKFLOW_RUN_ID.as_str().to_string(),
                input_summary: workflow_id.clone(),
            })
            .await;

        let continuation = scope.as_ref().map(|turn_scope| {
            ExternalJson::new(continuation_tool_metadata(
                turn_scope,
                &job_id,
                ContinuationAwaitMode::Async,
            ))
        });
        Ok(RuntimeWorkflowRunOutput::Enqueued(
            RuntimeWorkflowRunEnqueued {
                workflow_id,
                status: "enqueued".to_string(),
                strategy: request.strategy,
                job_ids: vec![job_id.clone()],
                root_job_id: job_id,
                job_type: job_type.to_string(),
                lane: "interactive".to_string(),
                continuation,
            },
        ))
    }
}

// ── cognition_runtime_workflow_schedule ───────────────────────────────────────

pub struct CognitionRuntimeWorkflowScheduleTool {
    runtime: Arc<RuntimeComposition>,
    registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionRuntimeWorkflowScheduleTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            registry,
            event_tx,
            turn_scope,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeWorkflowScheduleInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    #[schemars(default)]
    strategy: WorkflowStrategyInput,
    #[serde(default = "default_workflow_mode")]
    #[schemars(default = "default_workflow_mode")]
    mode: String,
    steps: CompatibleWorkflowSteps,
    #[serde(default)]
    #[schemars(default)]
    on_failure: WorkflowFailureInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default = "default_runtime_queue")]
    #[schemars(default = "default_runtime_queue")]
    queue: String,
    /// 7-field cron: sec min hour day-of-month month day-of-week year (e.g. every 4h: 0 0 */4 * * * *)
    #[schemars(required, with = "String")]
    cron_expr: Option<String>,
    #[serde(default = "default_runtime_timezone")]
    #[schemars(default = "default_runtime_timezone")]
    timezone: String,
    #[serde(default, alias = "id", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    recurring_id: Option<String>,
    #[serde(default)]
    #[schemars(default = "default_zero_i64")]
    jitter_seconds: i64,
    #[serde(default = "default_one_u64")]
    #[schemars(with = "i64", default = "default_one_i64")]
    max_attempts: u64,
    #[serde(default = "default_true")]
    #[schemars(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    #[schemars(default = "default_false")]
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

impl RuntimeWorkflowScheduleInput {
    fn request(&self) -> WorkflowRunRequest {
        WorkflowRunRequest {
            name: self.name.clone(),
            strategy: self.strategy.as_str().to_string(),
            mode: self.mode.clone(),
            steps: self.steps.0.clone(),
            on_failure: self.on_failure.as_str().to_string(),
            note: self.note.clone(),
            queue: Some(self.queue.clone()),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeWorkflowScheduled {
    workflow_id: String,
    status: String,
    strategy: String,
    recurring_id: String,
    cron_expr: String,
    timezone: String,
    next_run_at_utc: String,
    lane: String,
    delivery_bound: bool,
    feeds_bound: bool,
    start_immediately: bool,
    materialized_job_id: Option<String>,
    continuation: Option<ExternalJson>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeWorkflowScheduleOutput {
    Rejected(RuntimeWorkflowPreflightRejection),
    Scheduled(RuntimeWorkflowScheduled),
}

#[medousa_tool(id = COGNITION_RUNTIME_WORKFLOW_SCHEDULE_ID)]
impl CognitionRuntimeWorkflowScheduleTool {
    /// Register a recurring schedule for a declarative workflow. Requires scheduled lane; grapheme steps are preflight-validated.
    async fn invoke_typed(
        &self,
        input: RuntimeWorkflowScheduleInput,
    ) -> stasis::prelude::Result<RuntimeWorkflowScheduleOutput> {
        let request = input.request();
        validate_workflow_request(&request)?;
        if let Some(rejection) =
            validate_grapheme_steps_for_workflow(self.runtime.as_ref(), &request).await?
        {
            return Ok(RuntimeWorkflowScheduleOutput::Rejected(rejection));
        }

        let cron_expr = input.cron_expr.as_deref().ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_runtime_workflow_schedule: cron_expr is required".to_string(),
            )
        })?;

        let workflow_id = new_workflow_id();
        let payload = build_workflow_payload(&workflow_id, &request, "scheduled");
        let payload_template_ref = encode_workflow_payload(&payload)?;

        let recurring_id = input
            .recurring_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("wf-recur-{}", Uuid::new_v4().simple()));

        let now = Utc::now();
        let mut definition = RecurringDefinition {
            id: recurring_id.clone(),
            queue: input.queue.clone(),
            job_type: WORKFLOW_SEQUENTIAL_JOB_TYPE.to_string(),
            payload_template_ref,
            cron_expr: cron_expr.to_string(),
            timezone: input.timezone.clone(),
            jitter_seconds: input.jitter_seconds,
            enabled: input.enabled,
            max_attempts: input.max_attempts as u32,
            next_run_at: now,
            last_run_at: None,
            lease_owner: None,
            lease_expires_at: None,
        };

        if input.start_immediately {
            definition.next_run_at = now;
        } else {
            definition.next_run_at = definition.compute_next_run_at(now)?;
        }

        let scope = self.turn_scope.read().await.clone();
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

        register_recurring_definition(self.runtime.as_ref(), definition.clone()).await?;

        let mut materialized_job_id = None;
        if input.start_immediately {
            let _ = materialize_recurring_now(self.runtime.as_ref(), "cognition_tui")
                .await
                .map_err(|err| {
                    StasisError::PortFailure(format!("materialize recurring failed: {err:#}"))
                })?;
            if let Some(job_id) =
                find_active_job_by_correlation_id(self.runtime.as_ref(), &workflow_id).await
            {
                if let Some(scope) = self.turn_scope.read().await.clone() {
                    let job_type = workflow_job_type_for_strategy(&request.strategy)
                        .unwrap_or(WORKFLOW_SEQUENTIAL_JOB_TYPE);
                    let _ = patch_existing_job_correlation(
                        self.runtime.as_ref(),
                        &job_id,
                        &scope,
                        COGNITION_RUNTIME_WORKFLOW_SCHEDULE_ID.as_str(),
                    )
                    .await;
                    register_turn_child_job(
                        &scope,
                        &job_id,
                        COGNITION_RUNTIME_WORKFLOW_SCHEDULE_ID.as_str(),
                        job_type,
                        ContinuationAwaitMode::Async,
                    )
                    .await;
                }
                materialized_job_id = Some(job_id);
            }
        }

        let record = WorkflowRecord {
            workflow_id: workflow_id.clone(),
            name: request.name.clone(),
            strategy: request.strategy.clone(),
            mode: request.mode.clone(),
            on_failure: request.on_failure.clone(),
            note: request.note.clone(),
            root_job_id: materialized_job_id.clone().unwrap_or_default(),
            status: WorkflowStatus::Enqueued,
            created_at: now,
            scheduled_recurring_id: Some(recurring_id.clone()),
            step_results: Vec::new(),
        };
        self.registry.insert(record).await;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_RUNTIME_WORKFLOW_SCHEDULE_ID.as_str().to_string(),
                input_summary: format!("{workflow_id} @ {cron_expr}"),
            })
            .await;

        let scope = self.turn_scope.read().await.clone();
        let continuation = materialized_job_id.as_ref().and_then(|job_id| {
            scope.as_ref().map(|turn_scope| {
                ExternalJson::new(continuation_tool_metadata(
                    turn_scope,
                    job_id,
                    ContinuationAwaitMode::Async,
                ))
            })
        });
        Ok(RuntimeWorkflowScheduleOutput::Scheduled(
            RuntimeWorkflowScheduled {
                workflow_id,
                status: "scheduled".to_string(),
                strategy: request.strategy,
                recurring_id,
                cron_expr: cron_expr.to_string(),
                timezone: input.timezone,
                next_run_at_utc: definition.next_run_at.to_rfc3339(),
                lane: "scheduled".to_string(),
                delivery_bound,
                feeds_bound,
                start_immediately: input.start_immediately,
                materialized_job_id,
                continuation,
            },
        ))
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

#[derive(Debug, JsonSchema)]
pub struct RuntimeWorkflowStatusInput {
    /// Workflow identifier
    #[schemars(required, with = "String")]
    workflow_id: Option<String>,
}

impl<'de> Deserialize<'de> for RuntimeWorkflowStatusInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            workflow_id: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            workflow_id: input.workflow_id,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeWorkflowStepResult {
    id: String,
    kind: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<ExternalJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl From<&WorkflowStepResult> for RuntimeWorkflowStepResult {
    fn from(result: &WorkflowStepResult) -> Self {
        Self {
            id: result.id.clone(),
            kind: result.kind.clone(),
            status: result.status.clone(),
            output: result.output.clone().map(ExternalJson::new),
            error: result.error.clone(),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RuntimeWorkflowStatusFound {
    workflow_id: String,
    name: Option<String>,
    status: String,
    strategy: String,
    mode: String,
    on_failure: String,
    note: Option<String>,
    root_job_id: String,
    root_job_state: Option<String>,
    scheduled_recurring_id: Option<String>,
    created_at_utc: String,
    step_results: Vec<RuntimeWorkflowStepResult>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeWorkflowStatusOutput {
    Found(Box<RuntimeWorkflowStatusFound>),
    NotFound { workflow_id: String, status: String },
}

#[medousa_tool(id = COGNITION_RUNTIME_WORKFLOW_STATUS_ID)]
impl CognitionRuntimeWorkflowStatusTool {
    /// Aggregate status for a workflow by workflow_id.
    async fn invoke_typed(
        &self,
        input: RuntimeWorkflowStatusInput,
    ) -> stasis::prelude::Result<RuntimeWorkflowStatusOutput> {
        let workflow_id = input
            .workflow_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_workflow_status: workflow_id is required".to_string(),
                )
            })?;

        let Some(record) = self.registry.get(workflow_id).await else {
            return Ok(RuntimeWorkflowStatusOutput::NotFound {
                workflow_id: workflow_id.to_string(),
                status: "not_found".to_string(),
            });
        };

        let root_job = if record.root_job_id.is_empty() {
            None
        } else {
            get_job(self.runtime.as_ref(), &record.root_job_id).await?
        };

        Ok(RuntimeWorkflowStatusOutput::Found(Box::new(
            RuntimeWorkflowStatusFound {
                workflow_id: record.workflow_id,
                name: record.name,
                status: record.status.as_str().to_string(),
                strategy: record.strategy,
                mode: record.mode,
                on_failure: record.on_failure,
                note: record.note,
                root_job_id: record.root_job_id,
                root_job_state: root_job
                    .as_ref()
                    .map(|job| job_state_label(&job.state).to_string()),
                scheduled_recurring_id: record.scheduled_recurring_id,
                created_at_utc: record.created_at.to_rfc3339(),
                step_results: record
                    .step_results
                    .iter()
                    .map(RuntimeWorkflowStepResult::from)
                    .collect(),
            },
        )))
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

#[derive(Debug, JsonSchema)]
pub struct RuntimeWorkflowCancelInput {
    /// Workflow identifier
    #[schemars(required, with = "String")]
    workflow_id: Option<String>,
}

impl<'de> Deserialize<'de> for RuntimeWorkflowCancelInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            workflow_id: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            workflow_id: input.workflow_id,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeWorkflowCancelJobOutput {
    Canceled {
        job_id: String,
        status: String,
        previous_state: String,
    },
    NotCancelable {
        job_id: String,
        status: String,
        state: String,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum RuntimeWorkflowCancelOutput {
    NotFound {
        workflow_id: String,
        status: String,
    },
    Canceled {
        workflow_id: String,
        status: String,
        recurring_disabled: bool,
        job: Option<RuntimeWorkflowCancelJobOutput>,
    },
}

#[medousa_tool(id = COGNITION_RUNTIME_WORKFLOW_CANCEL_ID)]
impl CognitionRuntimeWorkflowCancelTool {
    /// Cancel a workflow: disable scheduled recurring (if any) and cancel pending root job.
    async fn invoke_typed(
        &self,
        input: RuntimeWorkflowCancelInput,
    ) -> stasis::prelude::Result<RuntimeWorkflowCancelOutput> {
        let workflow_id = input
            .workflow_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_workflow_cancel: workflow_id is required".to_string(),
                )
            })?;

        let Some(record) = self.registry.get(workflow_id).await else {
            return Ok(RuntimeWorkflowCancelOutput::NotFound {
                workflow_id: workflow_id.to_string(),
                status: "not_found".to_string(),
            });
        };

        let mut recurring_disabled = false;
        if let Some(recurring_id) = record.scheduled_recurring_id.as_deref() {
            let definitions = list_recurring_definitions(self.runtime.as_ref()).await?;
            if let Some(mut definition) = definitions
                .into_iter()
                .find(|definition| definition.id == recurring_id)
                && definition.enabled
            {
                definition.enabled = false;
                save_recurring_definition(self.runtime.as_ref(), definition).await?;
                recurring_disabled = true;
            }
        }

        let mut job_status = None;
        if !record.root_job_id.is_empty()
            && let Some(mut job) = get_job(self.runtime.as_ref(), &record.root_job_id).await?
        {
            let previous_state = job_state_label(&job.state).to_string();
            if matches!(job.state, JobState::Enqueued | JobState::Leased) {
                job.state = JobState::Canceled;
                job.finished_at = Some(Utc::now());
                save_job(self.runtime.as_ref(), job).await?;
                job_status = Some(RuntimeWorkflowCancelJobOutput::Canceled {
                    job_id: record.root_job_id.clone(),
                    status: "canceled".to_string(),
                    previous_state,
                });
            } else {
                job_status = Some(RuntimeWorkflowCancelJobOutput::NotCancelable {
                    job_id: record.root_job_id.clone(),
                    status: "not_cancelable".to_string(),
                    state: previous_state,
                });
            }
        }

        self.registry.mark_canceled(workflow_id).await;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_RUNTIME_WORKFLOW_CANCEL_ID.as_str().to_string(),
                input_summary: workflow_id.to_string(),
            })
            .await;

        Ok(RuntimeWorkflowCancelOutput::Canceled {
            workflow_id: workflow_id.to_string(),
            status: "canceled".to_string(),
            recurring_disabled,
            job: job_status,
        })
    }
}

// ── cognition_runtime_workflow_plan (D4) ────────────────────────────────────────

pub struct CognitionRuntimeWorkflowPlanTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionRuntimeWorkflowPlanTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct WorkflowPlanContext(Value);

impl JsonSchema for WorkflowPlanContext {
    fn schema_name() -> String {
        "WorkflowPlanContext".to_string()
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

#[derive(Debug, JsonSchema)]
pub struct RuntimeWorkflowPlanInput {
    /// Natural-language description of desired durable work
    #[schemars(required, with = "String")]
    goal: Option<String>,
    /// Optional hints: url, csv_url, topic, query, telegram_chat_id, timezone
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "WorkflowPlanContext", skip_serializing_if = "Option::is_none")]
    context: Option<WorkflowPlanContext>,
}

impl<'de> Deserialize<'de> for RuntimeWorkflowPlanInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self {
            goal: value
                .get("goal")
                .and_then(Value::as_str)
                .map(str::to_string),
            context: value.get("context").cloned().map(WorkflowPlanContext),
        })
    }
}

#[medousa_tool(id = COGNITION_RUNTIME_WORKFLOW_PLAN_ID)]
impl CognitionRuntimeWorkflowPlanTool {
    /// Suggest a workflow JSON plan from a natural-language goal without executing it. Returns execute_with guidance (workflow_run, workflow_schedule, capability_invoke, etc.).
    async fn invoke_typed(
        &self,
        input: RuntimeWorkflowPlanInput,
    ) -> stasis::prelude::Result<crate::workflow_plan::WorkflowPlanResponse> {
        let goal = input
            .goal
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_workflow_plan: goal is required".to_string(),
                )
            })?;

        let request = WorkflowPlanRequest {
            goal: goal.to_string(),
            context: input.context.map(|context| context.0),
        };

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_RUNTIME_WORKFLOW_PLAN_ID.as_str().to_string(),
                input_summary: goal.chars().take(80).collect(),
            })
            .await;

        Ok(plan_workflow_from_goal(&request))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use stasis::application::orchestration::tool_registry::StasisTool;
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
            .invoke_typed(RuntimeJobsListInput {
                state: None,
                correlation_id: None,
                limit: Some(10),
            })
            .await
            .expect("list jobs");
        assert!(response.jobs.iter().any(|job| job.job_id == job_id));
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
        let cancel_tool = CognitionRuntimeJobsCancelTool::new(Arc::new(runtime.clone()), event_tx);
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
