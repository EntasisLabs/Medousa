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
use stasis::prelude::StasisError;
use stasis::sdk::runtime_sdk::{RuntimeSdk, RuntimeStatsSnapshot};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::recurring_delivery::{
    DeliveryResolveContext, RecurringDeliverySpec, ambient_from_turn_scope,
    bind_recurring_delivery_spec_for_registration, delivery_binding_for_recurring,
};
use crate::recurring_feed::{RecurringFeedSpec, bind_recurring_feed_spec_for_registration};
use crate::recurring_schedule::RecurringScheduleSpec;
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::tools::validate_grapheme_source_for_schedule;
use crate::turn_continuation::{
    ContinuationAwaitMode, StoredDeliveryTarget, continuation_tool_metadata,
    find_active_job_by_correlation_id, materialize_recurring_now, patch_existing_job_correlation,
    register_turn_child_job,
};
use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};
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
    runtime.list_jobs_by_state(state).await
}

async fn get_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> stasis::prelude::Result<Option<Job>> {
    runtime.get_job(job_id).await
}

async fn save_job(runtime: &RuntimeComposition, job: Job) -> stasis::prelude::Result<()> {
    runtime.save_job(job).await
}

async fn list_recurring_definitions(
    runtime: &RuntimeComposition,
) -> stasis::prelude::Result<Vec<RecurringDefinition>> {
    runtime.list_recurring().await
}

async fn save_recurring_definition(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> stasis::prelude::Result<()> {
    runtime.save_recurring(definition).await
}

async fn register_recurring_definition(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> stasis::prelude::Result<()> {
    runtime.register_recurring(definition).await
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    state: CompatOption<String>,
    /// Optional correlation_id filter (exact match)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    correlation_id: CompatOption<String>,
    /// Max jobs to return (1-100, default 20)
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 100),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
}

#[derive(Debug)]
struct RuntimeJobsListCommand {
    state: Option<JobState>,
    correlation_id: Option<String>,
    limit: usize,
}

impl TryFrom<RuntimeJobsListInput> for RuntimeJobsListCommand {
    type Error = StasisError;

    fn try_from(input: RuntimeJobsListInput) -> Result<Self, Self::Error> {
        let state_value = input.state.into_option();
        let correlation_id_value = input.correlation_id.into_option();
        let limit = input.limit.into_option();
        let state = state_value
            .as_deref()
            .map(|raw| {
                parse_job_state_filter(raw).ok_or_else(|| {
                    StasisError::PortFailure(format!(
                        "cognition_runtime_jobs_list: unknown state '{raw}'"
                    ))
                })
            })
            .transpose()?;
        let correlation_id = correlation_id_value
            .and_then(|value| TrimmedText::new(value).ok().map(TrimmedText::into_string));

        Ok(Self {
            state,
            correlation_id,
            limit: limit.unwrap_or(20).clamp(1, 100),
        })
    }
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
        let command = RuntimeJobsListCommand::try_from(input)?;

        let states = if let Some(state) = command.state {
            vec![state]
        } else {
            vec![JobState::Enqueued, JobState::Leased, JobState::Running]
        };

        let mut jobs = Vec::new();
        for state in states {
            let mut batch = list_jobs_by_state(self.runtime.as_ref(), state).await?;
            jobs.append(&mut batch);
        }

        if let Some(correlation_id) = command.correlation_id.as_deref() {
            jobs.retain(|job| job.correlation_id == correlation_id);
        }

        jobs.sort_by_key(|b| std::cmp::Reverse(b.scheduled_at));
        jobs.truncate(command.limit);

        Ok(RuntimeJobsListOutput {
            count: jobs.len(),
            jobs: jobs.iter().map(job_summary).collect(),
        })
    }
}

fn required_runtime_identifier(
    value: Option<String>,
    tool_name: &str,
    field: &str,
) -> stasis::prelude::Result<String> {
    TrimmedText::new(value.unwrap_or_default())
        .map(TrimmedText::into_string)
        .map_err(|_| StasisError::PortFailure(format!("{tool_name}: {field} is required")))
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
            #[serde(default)]
            job_id: CompatOption<String>,
        }
        Ok(Self {
            job_id: WireInput::deserialize(deserializer)?.job_id.into_option(),
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
        let job_id = required_runtime_identifier(
            input.job_id,
            COGNITION_RUNTIME_JOBS_CANCEL_ID.as_str(),
            "job_id",
        )?;

        let Some(mut job) = get_job(self.runtime.as_ref(), &job_id).await? else {
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
            #[serde(default)]
            enabled_only: CompatOption<bool>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            enabled_only: input.enabled_only.into_option().unwrap_or(false),
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    recurring_id: CompatOption<String>,
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
        let recurring_id = input.recurring_id.into_option();
        let filter_id = recurring_id
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
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionRuntimeRecurringRegisterTool {
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
            #[serde(default)]
            source: CompatOption<String>,
            #[serde(default)]
            job_type: CompatOption<String>,
            #[serde(default)]
            payload_template_ref: CompatOption<String>,
            #[serde(default)]
            cron_expr: CompatOption<String>,
            #[serde(default)]
            timezone: CompatOption<String>,
            #[serde(default)]
            queue: CompatOption<String>,
            #[serde(default)]
            recurring_id: CompatOption<String>,
            #[serde(default)]
            id: CompatOption<String>,
            #[serde(default)]
            jitter_seconds: CompatOption<i64>,
            #[serde(default)]
            max_attempts: CompatOption<u64>,
            #[serde(default)]
            enabled: CompatOption<bool>,
            #[serde(default)]
            start_immediately: CompatOption<bool>,
            #[serde(default)]
            delivery: Option<RecurringDeliverySpec>,
            #[serde(default)]
            feeds: Option<RecurringFeedSpec>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            source: input.source.into_option(),
            job_type: input.job_type.into_option(),
            payload_template_ref: input.payload_template_ref.into_option(),
            cron_expr: input.cron_expr.into_option(),
            timezone: input.timezone.into_option(),
            queue: input.queue.into_option(),
            recurring_id: input
                .recurring_id
                .into_option()
                .or_else(|| input.id.into_option()),
            jitter_seconds: input.jitter_seconds.into_option(),
            max_attempts: input.max_attempts.into_option(),
            enabled: input.enabled.into_option(),
            start_immediately: input.start_immediately.into_option(),
            delivery: input.delivery,
            feeds: input.feeds,
        })
    }
}

impl RuntimeRecurringRegisterInput {
    pub(crate) fn grapheme(source: impl Into<String>, cron_expr: impl Into<String>) -> Self {
        Self {
            source: Some(source.into()),
            job_type: Some(default_grapheme_job_type()),
            payload_template_ref: None,
            cron_expr: Some(cron_expr.into()),
            timezone: Some(default_runtime_timezone()),
            queue: Some(default_runtime_queue()),
            recurring_id: None,
            jitter_seconds: Some(default_zero_i64()),
            max_attempts: Some(default_one_u64()),
            enabled: Some(default_true()),
            start_immediately: Some(default_false()),
            delivery: None,
            feeds: None,
        }
    }

    pub(crate) fn job(
        job_type: impl Into<String>,
        payload_template_ref: impl Into<String>,
        cron_expr: impl Into<String>,
    ) -> Self {
        Self {
            source: None,
            job_type: Some(job_type.into()),
            payload_template_ref: Some(payload_template_ref.into()),
            cron_expr: Some(cron_expr.into()),
            timezone: Some(default_runtime_timezone()),
            queue: Some(default_runtime_queue()),
            recurring_id: None,
            jitter_seconds: Some(default_zero_i64()),
            max_attempts: Some(default_one_u64()),
            enabled: Some(default_true()),
            start_immediately: Some(default_false()),
            delivery: None,
            feeds: None,
        }
    }
}

#[derive(Debug)]
struct RuntimeRecurringRegisterCommand {
    source: Option<RequiredContent>,
    job_type: TrimmedText,
    payload_template_ref: Option<TrimmedText>,
    cron_expr: TrimmedText,
    timezone: TrimmedText,
    queue: TrimmedText,
    recurring_id: Option<TrimmedText>,
    jitter_seconds: i64,
    max_attempts: u32,
    enabled: bool,
    start_immediately: bool,
    delivery: Option<RecurringDeliverySpec>,
    feeds: Option<RecurringFeedSpec>,
}

impl TryFrom<RuntimeRecurringRegisterInput> for RuntimeRecurringRegisterCommand {
    type Error = StasisError;

    fn try_from(input: RuntimeRecurringRegisterInput) -> Result<Self, Self::Error> {
        let source = input
            .source
            .and_then(|value| RequiredContent::new(value).ok());
        let job_type = input
            .job_type
            .and_then(|value| TrimmedText::new(value).ok())
            .unwrap_or_else(|| {
                TrimmedText::new(default_grapheme_job_type()).expect("literal is nonblank")
            });
        let payload_template_ref = input
            .payload_template_ref
            .and_then(|value| TrimmedText::new(value).ok());
        let cron_expr = TrimmedText::new(input.cron_expr.unwrap_or_default()).map_err(|_| {
            StasisError::PortFailure(
                "cognition_runtime_recurring_register: cron_expr is required".to_string(),
            )
        })?;
        let timezone = input
            .timezone
            .and_then(|value| TrimmedText::new(value).ok())
            .unwrap_or_else(|| {
                TrimmedText::new(default_runtime_timezone()).expect("literal is nonblank")
            });
        let queue = input
            .queue
            .and_then(|value| TrimmedText::new(value).ok())
            .unwrap_or_else(|| {
                TrimmedText::new(default_runtime_queue()).expect("literal is nonblank")
            });
        let recurring_id = input
            .recurring_id
            .and_then(|value| TrimmedText::new(value).ok());

        Ok(Self {
            source,
            job_type,
            payload_template_ref,
            cron_expr,
            timezone,
            queue,
            recurring_id,
            jitter_seconds: input.jitter_seconds.unwrap_or(0),
            max_attempts: input.max_attempts.unwrap_or(1) as u32,
            enabled: input.enabled.unwrap_or(true),
            start_immediately: input.start_immediately.unwrap_or(false),
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
        let command = RuntimeRecurringRegisterCommand::try_from(input)?;
        let RuntimeRecurringRegisterCommand {
            source,
            job_type,
            payload_template_ref,
            cron_expr,
            timezone,
            queue,
            recurring_id: requested_recurring_id,
            jitter_seconds,
            max_attempts,
            enabled,
            start_immediately,
            delivery,
            feeds,
        } = command;
        let payload_template_ref = if let Some(explicit) = payload_template_ref {
            explicit.into_string()
        } else if job_type.as_str() == "workflow.grapheme.run" {
            let source = source.as_ref().ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_runtime_recurring_register: source is required for workflow.grapheme.run"
                        .to_string(),
                )
            })?;
            let validation =
                validate_grapheme_source_for_schedule(&self.runtime, source.as_str()).await?;
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
            format!("grapheme:inline:{}", source.as_str())
        } else {
            return Err(StasisError::PortFailure(
                "cognition_runtime_recurring_register: payload_template_ref is required for non-grapheme job types"
                    .to_string(),
            ));
        };

        let recurring_id = requested_recurring_id
            .map(TrimmedText::into_string)
            .unwrap_or_else(|| format!("recur-{}", Uuid::new_v4().simple()));

        let definition = RecurringScheduleSpec::new(
            recurring_id.clone(),
            queue.as_str(),
            job_type.as_str(),
            payload_template_ref,
            cron_expr.as_str(),
            timezone.as_str(),
        )
        .jitter_seconds(jitter_seconds)
        .enabled(enabled)
        .max_attempts(max_attempts)
        .start_immediately(start_immediately)
        .build(Utc::now())?;

        let scope = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
        .await;
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_spec_for_registration(
            &recurring_id,
            cron_expr.as_str(),
            timezone.as_str(),
            delivery.as_ref(),
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id: fallback_session_id.clone(),
            },
        )
        .await?;
        let (feeds_bound, _) =
            bind_recurring_feed_spec_for_registration(&recurring_id, feeds.as_ref()).await?;

        register_recurring_definition(self.runtime.as_ref(), definition.clone()).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_RUNTIME_RECURRING_REGISTER_ID.as_str().to_string(),
                input_summary: format!("{recurring_id} @ {cron_expr}"),
            })
            .await;

        let feeds_bound_recurring = if feeds_bound {
            feeds.map(|feeds| feeds.feed_ids).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(RuntimeRecurringRegisterOutput::Registered {
            status: "registered".to_string(),
            recurring_id,
            job_type: job_type.into_string(),
            cron_expr: cron_expr.into_string(),
            timezone: timezone.into_string(),
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
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 50),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pending_limit: CompatOption<usize>,
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
        let pending_limit = input.pending_limit.into_option().unwrap_or(10).clamp(1, 50);

        let sdk = RuntimeSdk::new(self.runtime.as_ref().clone());
        let snapshot = sdk.stats_snapshot(pending_limit).await?;

        let pending = self
            .runtime
            .list_pending_outbox_events(pending_limit)
            .await?;

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
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionRuntimeWorkflowRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug)]
struct RuntimeWorkflowRunCommand {
    name: Option<TrimmedText>,
    strategy: WorkflowStrategyInput,
    mode: TrimmedText,
    steps: Vec<WorkflowStepSpec>,
    on_failure: WorkflowFailureInput,
    note: Option<String>,
    queue: TrimmedText,
}

impl TryFrom<RuntimeWorkflowRunInput> for RuntimeWorkflowRunCommand {
    type Error = StasisError;

    fn try_from(input: RuntimeWorkflowRunInput) -> Result<Self, Self::Error> {
        let required = |value: String, field: &str| {
            TrimmedText::new(value).map_err(|_| {
                StasisError::PortFailure(format!(
                    "cognition_runtime_workflow_run: {field} is required"
                ))
            })
        };
        let name = input.name.and_then(|value| TrimmedText::new(value).ok());
        let mode = required(input.mode, "mode")?;
        let queue = required(input.queue, "queue")?;

        Ok(Self {
            name,
            strategy: input.strategy,
            mode,
            steps: input.steps.0,
            on_failure: input.on_failure,
            note: input.note,
            queue,
        })
    }
}

impl RuntimeWorkflowRunCommand {
    fn request(&self) -> WorkflowRunRequest {
        WorkflowRunRequest {
            name: self.name.as_ref().map(ToString::to_string),
            strategy: self.strategy.as_str().to_string(),
            mode: self.mode.to_string(),
            steps: self.steps.clone(),
            on_failure: self.on_failure.as_str().to_string(),
            note: self.note.clone(),
            queue: Some(self.queue.to_string()),
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
        let command = RuntimeWorkflowRunCommand::try_from(input)?;
        let request = command.request();
        validate_workflow_request(&request)?;
        if let Some(rejection) =
            validate_grapheme_steps_for_workflow(self.runtime.as_ref(), &request).await?
        {
            return Ok(RuntimeWorkflowRunOutput::Rejected(rejection));
        }

        let workflow_id = new_workflow_id();
        let payload = build_workflow_payload(&workflow_id, &request, "interactive");
        let scope = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
        .await;
        let continuation = scope
            .as_ref()
            .map(|turn_scope| WorkflowEnqueueContinuation {
                turn_scope,
                tool_name: COGNITION_RUNTIME_WORKFLOW_RUN_ID.as_str(),
                await_mode: ContinuationAwaitMode::Async,
            });
        let job_id = enqueue_workflow_job(
            self.runtime.as_ref(),
            &payload,
            command.queue.as_str(),
            continuation,
        )
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
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionRuntimeWorkflowScheduleTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug)]
struct RuntimeWorkflowScheduleCommand {
    name: Option<TrimmedText>,
    strategy: WorkflowStrategyInput,
    mode: TrimmedText,
    steps: Vec<WorkflowStepSpec>,
    on_failure: WorkflowFailureInput,
    note: Option<String>,
    queue: TrimmedText,
    cron_expr: TrimmedText,
    timezone: TrimmedText,
    recurring_id: Option<TrimmedText>,
    jitter_seconds: i64,
    max_attempts: u32,
    enabled: bool,
    start_immediately: bool,
    delivery: Option<RecurringDeliverySpec>,
    feeds: Option<RecurringFeedSpec>,
}

impl TryFrom<RuntimeWorkflowScheduleInput> for RuntimeWorkflowScheduleCommand {
    type Error = StasisError;

    fn try_from(input: RuntimeWorkflowScheduleInput) -> Result<Self, Self::Error> {
        let required = |value: String, field: &str| {
            TrimmedText::new(value).map_err(|_| {
                StasisError::PortFailure(format!(
                    "cognition_runtime_workflow_schedule: {field} is required"
                ))
            })
        };
        let name = input.name.and_then(|value| TrimmedText::new(value).ok());
        let mode = required(input.mode, "mode")?;
        let queue = required(input.queue, "queue")?;
        let cron_expr = required(input.cron_expr.unwrap_or_default(), "cron_expr")?;
        let timezone = required(input.timezone, "timezone")?;
        let recurring_id = input
            .recurring_id
            .and_then(|value| TrimmedText::new(value).ok());

        Ok(Self {
            name,
            strategy: input.strategy,
            mode,
            steps: input.steps.0,
            on_failure: input.on_failure,
            note: input.note,
            queue,
            cron_expr,
            timezone,
            recurring_id,
            jitter_seconds: input.jitter_seconds,
            max_attempts: input.max_attempts as u32,
            enabled: input.enabled,
            start_immediately: input.start_immediately,
            delivery: input.delivery,
            feeds: input.feeds,
        })
    }
}

impl RuntimeWorkflowScheduleCommand {
    fn request(&self) -> WorkflowRunRequest {
        WorkflowRunRequest {
            name: self.name.as_ref().map(ToString::to_string),
            strategy: self.strategy.as_str().to_string(),
            mode: self.mode.to_string(),
            steps: self.steps.clone(),
            on_failure: self.on_failure.as_str().to_string(),
            note: self.note.clone(),
            queue: Some(self.queue.to_string()),
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
        let command = RuntimeWorkflowScheduleCommand::try_from(input)?;
        let request = command.request();
        validate_workflow_request(&request)?;
        if let Some(rejection) =
            validate_grapheme_steps_for_workflow(self.runtime.as_ref(), &request).await?
        {
            return Ok(RuntimeWorkflowScheduleOutput::Rejected(rejection));
        }

        let workflow_id = new_workflow_id();
        let payload = build_workflow_payload(&workflow_id, &request, "scheduled");
        let payload_template_ref = encode_workflow_payload(&payload)?;

        let recurring_id = command
            .recurring_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("wf-recur-{}", Uuid::new_v4().simple()));

        let now = Utc::now();
        let definition = RecurringScheduleSpec::new(
            recurring_id.clone(),
            command.queue.as_str(),
            WORKFLOW_SEQUENTIAL_JOB_TYPE,
            payload_template_ref,
            command.cron_expr.as_str(),
            command.timezone.as_str(),
        )
        .jitter_seconds(command.jitter_seconds)
        .enabled(command.enabled)
        .max_attempts(command.max_attempts)
        .start_immediately(command.start_immediately)
        .build(now)?;

        let scope = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
        .await;
        let ambient = ambient_from_turn_scope(scope.as_ref());
        let fallback_session_id = scope
            .as_ref()
            .map(|turn| turn.session_id.clone())
            .unwrap_or_else(|| format!("recurring-{recurring_id}"));
        let (delivery_bound, _) = bind_recurring_delivery_spec_for_registration(
            &recurring_id,
            command.cron_expr.as_str(),
            command.timezone.as_str(),
            command.delivery.as_ref(),
            DeliveryResolveContext {
                ambient: ambient.as_ref(),
                fallback_session_id: fallback_session_id.clone(),
            },
        )
        .await?;
        let (feeds_bound, _) =
            bind_recurring_feed_spec_for_registration(&recurring_id, command.feeds.as_ref())
                .await?;

        register_recurring_definition(self.runtime.as_ref(), definition.clone()).await?;

        let mut materialized_job_id = None;
        if command.start_immediately {
            let _ = materialize_recurring_now(self.runtime.as_ref(), "cognition_tui")
                .await
                .map_err(|err| {
                    StasisError::PortFailure(format!("materialize recurring failed: {err:#}"))
                })?;
            if let Some(job_id) =
                find_active_job_by_correlation_id(self.runtime.as_ref(), &workflow_id).await
            {
                if let Some(scope) =
                    crate::agent_runtime::execution_context::turn_continuation_scope(
                        &self.turn_scope,
                    )
                    .await
                {
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
                input_summary: format!("{workflow_id} @ {}", command.cron_expr.as_str()),
            })
            .await;

        let scope = crate::agent_runtime::execution_context::turn_continuation_scope(
            &self.turn_scope,
        )
        .await;
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
                cron_expr: command.cron_expr.into_string(),
                timezone: command.timezone.into_string(),
                next_run_at_utc: definition.next_run_at.to_rfc3339(),
                lane: "scheduled".to_string(),
                delivery_bound,
                feeds_bound,
                start_immediately: command.start_immediately,
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
            #[serde(default)]
            workflow_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            workflow_id: input.workflow_id.into_option(),
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
        let workflow_id = required_runtime_identifier(
            input.workflow_id,
            COGNITION_RUNTIME_WORKFLOW_STATUS_ID.as_str(),
            "workflow_id",
        )?;

        let Some(record) = self.registry.get(&workflow_id).await else {
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
            #[serde(default)]
            workflow_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            workflow_id: input.workflow_id.into_option(),
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
        let workflow_id = required_runtime_identifier(
            input.workflow_id,
            COGNITION_RUNTIME_WORKFLOW_CANCEL_ID.as_str(),
            "workflow_id",
        )?;

        let Some(record) = self.registry.get(&workflow_id).await else {
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

        self.registry.mark_canceled(&workflow_id).await;

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

    #[test]
    fn runtime_jobs_list_command_normalizes_filters_once() {
        let command = RuntimeJobsListCommand::try_from(RuntimeJobsListInput {
            state: Some("  running  ".to_string()).into(),
            correlation_id: Some("  workflow-1  ".to_string()).into(),
            limit: Some(0).into(),
        })
        .expect("job list command");

        assert_eq!(command.state, Some(JobState::Running));
        assert_eq!(command.correlation_id.as_deref(), Some("workflow-1"));
        assert_eq!(command.limit, 1);
    }

    #[test]
    fn required_runtime_identifier_keeps_tool_context() {
        let error = required_runtime_identifier(
            Some("  ".to_string()),
            "cognition_runtime_jobs_cancel",
            "job_id",
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cognition_runtime_jobs_cancel: job_id is required")
        );
    }

    #[test]
    fn recurring_input_constructors_keep_wire_defaults() {
        let grapheme = RuntimeRecurringRegisterInput::grapheme("source", "cron");
        assert_eq!(grapheme.source.as_deref(), Some("source"));
        assert_eq!(grapheme.job_type.as_deref(), Some("workflow.grapheme.run"));
        assert_eq!(grapheme.timezone.as_deref(), Some("UTC"));
        assert_eq!(grapheme.queue.as_deref(), Some("default"));
        assert_eq!(grapheme.enabled, Some(true));

        let job = RuntimeRecurringRegisterInput::job("job.type", "payload", "cron");
        assert!(job.source.is_none());
        assert_eq!(job.job_type.as_deref(), Some("job.type"));
        assert_eq!(job.payload_template_ref.as_deref(), Some("payload"));
        assert_eq!(job.start_immediately, Some(false));
    }

    #[test]
    fn recurring_register_command_normalizes_identifiers_and_preserves_source() {
        let mut input = RuntimeRecurringRegisterInput::job(" job.type ", " payload ", " cron ");
        input.source = Some("  source bytes\n".into());
        input.timezone = Some(" UTC ".into());
        input.queue = Some(" queue-a ".into());
        input.recurring_id = Some(" recurring-a ".into());

        let command =
            RuntimeRecurringRegisterCommand::try_from(input).expect("recurring register command");
        assert_eq!(command.job_type.as_str(), "job.type");
        assert_eq!(command.payload_template_ref.unwrap().as_str(), "payload");
        assert_eq!(command.cron_expr.as_str(), "cron");
        assert_eq!(command.timezone.as_str(), "UTC");
        assert_eq!(command.queue.as_str(), "queue-a");
        assert_eq!(command.recurring_id.unwrap().as_str(), "recurring-a");
        assert_eq!(command.source.unwrap().as_str(), "  source bytes\n");
    }

    #[test]
    fn recurring_register_command_owns_defaults_and_required_cron_error() {
        let mut input = RuntimeRecurringRegisterInput::grapheme("source", "cron");
        input.timezone = Some(" \n\t".into());
        input.queue = Some(" \n\t".into());
        input.recurring_id = Some(" \n\t".into());
        let command = RuntimeRecurringRegisterCommand::try_from(input)
            .expect("blank optional values use defaults");
        assert_eq!(command.job_type.as_str(), "workflow.grapheme.run");
        assert_eq!(command.timezone.as_str(), "UTC");
        assert_eq!(command.queue.as_str(), "default");
        assert!(command.recurring_id.is_none());
        assert!(command.enabled);
        assert!(!command.start_immediately);

        let mut missing_cron = RuntimeRecurringRegisterInput::grapheme("source", "cron");
        missing_cron.cron_expr = None;
        let error =
            RuntimeRecurringRegisterCommand::try_from(missing_cron).expect_err("cron is required");
        assert!(
            error
                .to_string()
                .contains("cognition_runtime_recurring_register: cron_expr is required")
        );
    }

    #[test]
    fn workflow_schedule_command_normalizes_schedule_fields_once() {
        let command = RuntimeWorkflowScheduleCommand::try_from(RuntimeWorkflowScheduleInput {
            name: Some(" Workflow name ".into()),
            strategy: WorkflowStrategyInput::Concurrent,
            mode: " default ".into(),
            steps: CompatibleWorkflowSteps(vec![WorkflowStepSpec::Prompt {
                id: "step-1".into(),
                user_prompt: "hello".into(),
                system_prompt: None,
            }]),
            on_failure: WorkflowFailureInput::Continue,
            note: Some("  preserve this note\n".into()),
            queue: " queue-a ".into(),
            cron_expr: Some(" cron ".into()),
            timezone: " UTC ".into(),
            recurring_id: Some(" workflow-recur ".into()),
            jitter_seconds: 4,
            max_attempts: 3,
            enabled: false,
            start_immediately: true,
            delivery: None,
            feeds: None,
        })
        .expect("workflow schedule command");

        let request = command.request();
        assert_eq!(command.name.as_ref().unwrap().as_str(), "Workflow name");
        assert_eq!(command.mode.as_str(), "default");
        assert_eq!(command.queue.as_str(), "queue-a");
        assert_eq!(command.cron_expr.as_str(), "cron");
        assert_eq!(command.timezone.as_str(), "UTC");
        assert_eq!(
            command.recurring_id.as_ref().unwrap().as_str(),
            "workflow-recur"
        );
        assert_eq!(request.strategy, "concurrent");
        assert_eq!(request.mode, "default");
        assert_eq!(request.queue.as_deref(), Some("queue-a"));
        assert_eq!(request.note.as_deref(), Some("  preserve this note\n"));
        assert_eq!(request.steps.len(), 1);
    }

    #[test]
    fn workflow_schedule_command_rejects_blank_schedule_fields() {
        let input = RuntimeWorkflowScheduleInput {
            name: None,
            strategy: WorkflowStrategyInput::Sequential,
            mode: "default".into(),
            steps: CompatibleWorkflowSteps(Vec::new()),
            on_failure: WorkflowFailureInput::Stop,
            note: None,
            queue: " \n\t".into(),
            cron_expr: Some("cron".into()),
            timezone: "UTC".into(),
            recurring_id: None,
            jitter_seconds: 0,
            max_attempts: 1,
            enabled: true,
            start_immediately: false,
            delivery: None,
            feeds: None,
        };
        let error = RuntimeWorkflowScheduleCommand::try_from(input).expect_err("queue is required");
        assert!(
            error
                .to_string()
                .contains("cognition_runtime_workflow_schedule: queue is required")
        );
    }

    #[test]
    fn workflow_run_command_normalizes_request_fields_once() {
        let command = RuntimeWorkflowRunCommand::try_from(RuntimeWorkflowRunInput {
            name: Some(" Run name ".into()),
            strategy: WorkflowStrategyInput::Handoff,
            mode: " default ".into(),
            steps: CompatibleWorkflowSteps(vec![WorkflowStepSpec::Prompt {
                id: "step-1".into(),
                user_prompt: "hello".into(),
                system_prompt: None,
            }]),
            on_failure: WorkflowFailureInput::Continue,
            note: Some("  keep note\n".into()),
            queue: " queue-a ".into(),
        })
        .expect("workflow run command");

        let request = command.request();
        assert_eq!(command.name.as_ref().unwrap().as_str(), "Run name");
        assert_eq!(command.mode.as_str(), "default");
        assert_eq!(command.queue.as_str(), "queue-a");
        assert_eq!(request.strategy, "handoff");
        assert_eq!(request.queue.as_deref(), Some("queue-a"));
        assert_eq!(request.note.as_deref(), Some("  keep note\n"));
        assert_eq!(request.steps.len(), 1);
    }

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
                state: None::<String>.into(),
                correlation_id: None::<String>.into(),
                limit: Some(10).into(),
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
