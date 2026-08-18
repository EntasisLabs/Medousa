//! Public runtime primitives: query or mutate jobs, recurring, workflows, delivery.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::bridge_tools::{BridgeObject, CognitionMcpPromoteToJobTool, McpPromoteToJobInput};
use crate::events::TuiEvent;
use crate::public_api::{COGNITION_RUNTIME_MUTATE, COGNITION_RUNTIME_QUERY};
use crate::recurring_delivery::RecurringDeliverySpec;
use crate::recurring_feed::RecurringFeedSpec;
use crate::runtime_tools::{
    CognitionRuntimeDeliveryStatusTool, CognitionRuntimeJobsCancelTool,
    CognitionRuntimeJobsListTool, CognitionRuntimeRecurringCancelTool,
    CognitionRuntimeRecurringDoctorTool, CognitionRuntimeRecurringListTool,
    CognitionRuntimeRecurringPauseTool, CognitionRuntimeRecurringRegisterTool,
    CognitionRuntimeWorkflowCancelTool, CognitionRuntimeWorkflowPlanTool,
    CognitionRuntimeWorkflowRunTool, CognitionRuntimeWorkflowScheduleTool,
    CognitionRuntimeWorkflowStatusTool, CompatibleWorkflowSteps, RuntimeDeliveryStatusInput,
    RuntimeJobsCancelInput, RuntimeJobsListInput, RuntimeRecurringDoctorInput,
    RuntimeRecurringListInput, RuntimeRecurringRegisterInput, RuntimeRecurringToggleInput,
    RuntimeWorkflowCancelInput, RuntimeWorkflowPlanInput, RuntimeWorkflowRunInput,
    RuntimeWorkflowScheduleInput, RuntimeWorkflowStatusInput, WorkflowFailureInput,
    WorkflowPlanContext, WorkflowStrategyInput,
};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::tools::{
    CognitionGraphemePromoteLastRunToRecurringTool, CognitionGraphemePromoteToJobTool,
    CognitionGraphemePromoteToRecurringTool, CognitionJobEnqueueTool,
    CognitionRuntimeJobStatusTool, CognitionRuntimeRecurringPreviewTool, GraphemePromoteLastRunInput,
    GraphemePromoteToJobInput, GraphemePromoteToRecurringInput, JobEnqueueInput,
    RuntimeJobStatusInput, RuntimeRecurringPreviewInput,
};
use crate::typed_tools::{
    CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};
use crate::workflow::WorkflowRegistry;
use stasis::prelude::RuntimeComposition;

const QUERY_ID: ToolId = ToolId::new(COGNITION_RUNTIME_QUERY);
const MUTATE_ID: ToolId = ToolId::new(COGNITION_RUNTIME_MUTATE);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RuntimeFrom {
    #[default]
    Auto,
    LastRun,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum RuntimeQueryAction {
    #[serde(rename = "job.list")]
    JobList(JobList),
    #[serde(rename = "job.status")]
    JobStatus(JobStatus),
    #[serde(rename = "recurring.list")]
    RecurringList(RecurringList),
    #[serde(rename = "recurring.doctor")]
    RecurringDoctor(RecurringDoctor),
    #[serde(rename = "recurring.preview")]
    RecurringPreview(RecurringPreview),
    #[serde(rename = "workflow.status")]
    WorkflowStatus(WorkflowStatus),
    #[serde(rename = "delivery.status")]
    DeliveryStatus(DeliveryStatus),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum RuntimeMutateAction {
    #[serde(rename = "job.enqueue")]
    JobEnqueue(JobEnqueue),
    #[serde(rename = "job.cancel")]
    JobCancel(JobCancel),
    #[serde(rename = "recurring.register")]
    RecurringRegister(RecurringRegister),
    #[serde(rename = "recurring.pause")]
    RecurringPause(RecurringPause),
    #[serde(rename = "recurring.cancel")]
    RecurringCancel(RecurringCancel),
    #[serde(rename = "workflow.run")]
    WorkflowRun(WorkflowRun),
    #[serde(rename = "workflow.schedule")]
    WorkflowSchedule(WorkflowSchedule),
    #[serde(rename = "workflow.cancel")]
    WorkflowCancel(WorkflowCancel),
    #[serde(rename = "workflow.plan")]
    WorkflowPlan(WorkflowPlan),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct JobList {
    /// Filter: enqueued, leased, running, succeeded, failed, dead_letter, canceled
    #[serde(default)]
    state: Option<String>,
    /// Exact correlation id
    #[serde(default)]
    correlation_id: Option<String>,
    /// Max jobs (1-100, default 20)
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JobStatus {
    /// Runtime job id
    job_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct RecurringList {
    /// Only enabled schedules
    #[serde(default)]
    enabled_only: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct RecurringDoctor {
    /// Schedule id; omit for a summary
    #[serde(default)]
    recurring_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecurringPreview {
    /// 7-field cron
    cron_expr: String,
    /// IANA timezone (default UTC)
    #[serde(default)]
    timezone: Option<String>,
    /// How many future runs (1-20, default 5)
    #[serde(default)]
    count: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowStatus {
    /// Workflow id
    workflow_id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DeliveryStatus {
    /// Pending outbox rows to preview (1-50)
    #[serde(default)]
    pending_limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JobEnqueue {
    /// Grapheme source (promote to a one-off job)
    #[serde(default)]
    script: Option<String>,
    /// MCP server id
    #[serde(default)]
    server_id: Option<String>,
    /// MCP tool name (with server_id)
    #[serde(default)]
    tool_name: Option<String>,
    /// MCP tool arguments
    #[serde(default)]
    input: Option<Value>,
    /// Handler id, e.g. workflow.grapheme.run
    #[serde(default)]
    job_type: Option<String>,
    /// For grapheme: grapheme:inline:<source>
    #[serde(default)]
    payload_ref: Option<String>,
    /// Human-readable intent
    #[serde(default)]
    note: Option<String>,
    /// Runtime queue (default default)
    #[serde(default)]
    queue: Option<String>,
    /// Retry cap for Grapheme promote
    #[serde(default)]
    max_attempts: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct JobCancel {
    /// Runtime job id
    job_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecurringRegister {
    /// auto (default) or last_run to reuse the last Grapheme source
    #[serde(default)]
    from: RuntimeFrom,
    /// Grapheme source to schedule
    #[serde(default)]
    script: Option<String>,
    /// 7-field cron
    cron_expr: String,
    /// IANA timezone (default UTC)
    #[serde(default)]
    timezone: Option<String>,
    /// Handler; default workflow.grapheme.run
    #[serde(default)]
    job_type: Option<String>,
    /// payload_template_ref for non-grapheme jobs
    #[serde(default)]
    payload_ref: Option<String>,
    /// Runtime queue
    #[serde(default)]
    queue: Option<String>,
    /// Optional schedule id
    #[serde(default)]
    recurring_id: Option<String>,
    #[serde(default)]
    jitter_seconds: Option<i64>,
    #[serde(default)]
    max_attempts: Option<u64>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    start_immediately: Option<bool>,
    /// Where to push each successful run
    #[serde(default)]
    delivery: Option<RecurringDeliverySpec>,
    /// feed_ids to publish each tick
    #[serde(default)]
    feeds: Option<RecurringFeedSpec>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecurringPause {
    /// Schedule id
    recurring_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecurringCancel {
    /// Schedule id
    recurring_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowRun {
    /// Human-readable workflow name
    #[serde(default)]
    name: Option<String>,
    /// Ordered grapheme/prompt/mcp steps
    steps: CompatibleWorkflowSteps,
    /// sequential (default), concurrent, or handoff
    #[serde(default)]
    strategy: Option<WorkflowStrategyInput>,
    /// Workflow mode (default default)
    #[serde(default)]
    mode: Option<String>,
    /// stop (default) or continue
    #[serde(default)]
    on_failure: Option<WorkflowFailureInput>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    queue: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowSchedule {
    #[serde(default)]
    name: Option<String>,
    steps: CompatibleWorkflowSteps,
    /// 7-field cron
    cron_expr: String,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    strategy: Option<WorkflowStrategyInput>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    on_failure: Option<WorkflowFailureInput>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    queue: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowCancel {
    /// Workflow id
    workflow_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowPlan {
    /// What the workflow should accomplish
    goal: String,
    /// Optional urls, chat ids, extra constraints
    #[serde(default)]
    context: Option<Value>,
}

impl JsonSchema for RuntimeQueryAction {
    fn schema_name() -> String {
        "RuntimeQueryAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "job.list",
                "job.status",
                "recurring.list",
                "recurring.doctor",
                "recurring.preview",
                "workflow.status",
                "delivery.status",
            ]),
            true,
        )])
    }
}

impl JsonSchema for RuntimeMutateAction {
    fn schema_name() -> String {
        "RuntimeMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "job.enqueue",
                "job.cancel",
                "recurring.register",
                "recurring.pause",
                "recurring.cancel",
                "workflow.run",
                "workflow.schedule",
                "workflow.cancel",
                "workflow.plan",
            ]),
            true,
        )])
    }
}

pub fn runtime_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<JobList>(QUERY_ID, "job.list", "List durable jobs"),
        typed_action_schema::<JobStatus>(QUERY_ID, "job.status", "Status for one job"),
        typed_action_schema::<RecurringList>(QUERY_ID, "recurring.list", "List recurring schedules"),
        typed_action_schema::<RecurringDoctor>(
            QUERY_ID,
            "recurring.doctor",
            "Diagnose a recurring schedule",
        ),
        typed_action_schema::<RecurringPreview>(
            QUERY_ID,
            "recurring.preview",
            "Preview upcoming cron fire times",
        ),
        typed_action_schema::<WorkflowStatus>(QUERY_ID, "workflow.status", "Status for one workflow"),
        typed_action_schema::<DeliveryStatus>(
            QUERY_ID,
            "delivery.status",
            "Queue, outbox, and recurring delivery counts",
        ),
        typed_action_schema::<JobEnqueue>(
            MUTATE_ID,
            "job.enqueue",
            "Enqueue a job: Grapheme script, MCP server_id+tool_name, or job_type+payload_ref",
        ),
        typed_action_schema::<JobCancel>(MUTATE_ID, "job.cancel", "Cancel a durable job"),
        typed_action_schema::<RecurringRegister>(
            MUTATE_ID,
            "recurring.register",
            "Register a cron schedule (script, last run, or job_type+payload_ref)",
        ),
        typed_action_schema::<RecurringPause>(MUTATE_ID, "recurring.pause", "Pause a recurring schedule"),
        typed_action_schema::<RecurringCancel>(
            MUTATE_ID,
            "recurring.cancel",
            "Cancel a recurring schedule",
        ),
        typed_action_schema::<WorkflowRun>(
            MUTATE_ID,
            "workflow.run",
            "Run a multi-step durable workflow now",
        ),
        typed_action_schema::<WorkflowSchedule>(
            MUTATE_ID,
            "workflow.schedule",
            "Schedule a multi-step workflow on cron",
        ),
        typed_action_schema::<WorkflowCancel>(
            MUTATE_ID,
            "workflow.cancel",
            "Cancel a running or scheduled workflow",
        ),
        typed_action_schema::<WorkflowPlan>(
            MUTATE_ID,
            "workflow.plan",
            "Draft a durable workflow from a natural-language goal",
        ),
    ]
}

pub struct CognitionRuntimeQueryTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    workflow_registry: Arc<WorkflowRegistry>,
}

pub struct CognitionRuntimeMutateTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    workflow_registry: Arc<WorkflowRegistry>,
}

pub fn register_runtime_api_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    workflow_registry: Arc<WorkflowRegistry>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionRuntimeQueryTool {
        runtime: runtime.clone(),
        event_tx: event_tx.clone(),
        workflow_registry: workflow_registry.clone(),
    })?;
    registry.register_typed_tool(CognitionRuntimeMutateTool {
        runtime,
        event_tx,
        turn_scope,
        workflow_registry,
    })?;
    Ok(())
}

#[medousa_tool(id = QUERY_ID)]
impl CognitionRuntimeQueryTool {
    /// Inspect jobs, recurring, workflows, or delivery. action is a typed name (job.list, workflow.status, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: RuntimeQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = MUTATE_ID)]
impl CognitionRuntimeMutateTool {
    /// Mutate durable runtime work. action is a typed name (job.enqueue, workflow.run, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: RuntimeMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionRuntimeQueryTool,
    action: RuntimeQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        RuntimeQueryAction::JobList(params) => params.execute(tool).await,
        RuntimeQueryAction::JobStatus(params) => params.execute(tool).await,
        RuntimeQueryAction::RecurringList(params) => params.execute(tool).await,
        RuntimeQueryAction::RecurringDoctor(params) => params.execute(tool).await,
        RuntimeQueryAction::RecurringPreview(params) => params.execute(tool).await,
        RuntimeQueryAction::WorkflowStatus(params) => params.execute(tool).await,
        RuntimeQueryAction::DeliveryStatus(params) => params.execute(tool).await,
    }
}

async fn dispatch_mutate(
    tool: &CognitionRuntimeMutateTool,
    action: RuntimeMutateAction,
) -> stasis::prelude::Result<Value> {
    match action {
        RuntimeMutateAction::JobEnqueue(params) => params.execute(tool).await,
        RuntimeMutateAction::JobCancel(params) => params.execute(tool).await,
        RuntimeMutateAction::RecurringRegister(params) => params.execute(tool).await,
        RuntimeMutateAction::RecurringPause(params) => params.execute(tool).await,
        RuntimeMutateAction::RecurringCancel(params) => params.execute(tool).await,
        RuntimeMutateAction::WorkflowRun(params) => params.execute(tool).await,
        RuntimeMutateAction::WorkflowSchedule(params) => params.execute(tool).await,
        RuntimeMutateAction::WorkflowCancel(params) => params.execute(tool).await,
        RuntimeMutateAction::WorkflowPlan(params) => params.execute(tool).await,
    }
}

impl JobList {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeJobsListTool::new(tool.runtime.clone())
            .invoke_typed(RuntimeJobsListInput {
                state: CompatOption::from(self.state),
                correlation_id: CompatOption::from(self.correlation_id),
                limit: CompatOption::from(self.limit),
            })
            .await?;
        serialize_output(CognitionRuntimeJobsListTool::tool_id(), output)
    }
}

impl JobStatus {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeJobStatusTool::new(tool.runtime.clone())
            .invoke_typed(RuntimeJobStatusInput {
                job_id: Some(self.job_id),
            })
            .await?;
        serialize_output(CognitionRuntimeJobStatusTool::tool_id(), output)
    }
}

impl RecurringList {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeRecurringListTool::new(tool.runtime.clone())
            .invoke_typed(RuntimeRecurringListInput {
                enabled_only: self.enabled_only.unwrap_or(false),
            })
            .await?;
        serialize_output(CognitionRuntimeRecurringListTool::tool_id(), output)
    }
}

impl RecurringDoctor {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeRecurringDoctorTool::new(tool.runtime.clone())
            .invoke_typed(RuntimeRecurringDoctorInput {
                recurring_id: CompatOption::from(self.recurring_id),
            })
            .await?;
        serialize_output(CognitionRuntimeRecurringDoctorTool::tool_id(), output)
    }
}

impl RecurringPreview {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeRecurringPreviewTool::new(tool.event_tx.clone())
            .invoke_typed(RuntimeRecurringPreviewInput {
                cron_expr: Some(self.cron_expr),
                timezone: self.timezone.unwrap_or_else(|| "UTC".to_string()),
                count: self.count,
                start_at: None,
            })
            .await?;
        serialize_output(CognitionRuntimeRecurringPreviewTool::tool_id(), output)
    }
}

impl WorkflowStatus {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeWorkflowStatusTool::new(
            tool.runtime.clone(),
            tool.workflow_registry.clone(),
        )
        .invoke_typed(RuntimeWorkflowStatusInput {
            workflow_id: Some(self.workflow_id),
        })
        .await?;
        serialize_output(CognitionRuntimeWorkflowStatusTool::tool_id(), output)
    }
}

impl DeliveryStatus {
    async fn execute(self, tool: &CognitionRuntimeQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeDeliveryStatusTool::new(tool.runtime.clone())
            .invoke_typed(RuntimeDeliveryStatusInput {
                pending_limit: CompatOption::from(self.pending_limit),
            })
            .await?;
        serialize_output(CognitionRuntimeDeliveryStatusTool::tool_id(), output)
    }
}

impl JobEnqueue {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        if present(self.server_id.as_deref()) {
            let output = CognitionMcpPromoteToJobTool::new(
                tool.runtime.clone(),
                tool.workflow_registry.clone(),
                tool.event_tx.clone(),
                tool.turn_scope.clone(),
            )
            .invoke_typed(McpPromoteToJobInput {
                server_id: self.server_id,
                tool_name: self.tool_name,
                input: self.input.map(BridgeObject::from_value),
                note: self.note,
                queue: self.queue.unwrap_or_else(|| "default".to_string()),
                step_id: "mcp_step".to_string(),
            })
            .await?;
            return serialize_output(CognitionMcpPromoteToJobTool::tool_id(), output);
        }
        if present(self.script.as_deref()) && !present(self.payload_ref.as_deref()) {
            let output = CognitionGraphemePromoteToJobTool::new(
                tool.runtime.clone(),
                tool.event_tx.clone(),
                tool.turn_scope.clone(),
            )
            .invoke_typed(GraphemePromoteToJobInput {
                source: self.script,
                queue: self.queue.unwrap_or_else(|| "default".to_string()),
                priority: 100,
                max_attempts: self.max_attempts.unwrap_or(1),
            })
            .await?;
            return serialize_output(CognitionGraphemePromoteToJobTool::tool_id(), output);
        }
        let output = CognitionJobEnqueueTool::new(
            tool.runtime.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke_typed(JobEnqueueInput {
            job_type: self.job_type,
            payload_ref: self.payload_ref,
            note: self.note,
        })
        .await?;
        serialize_output(CognitionJobEnqueueTool::tool_id(), output)
    }
}

impl JobCancel {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeJobsCancelTool::new(tool.runtime.clone(), tool.event_tx.clone())
            .invoke_typed(RuntimeJobsCancelInput {
                job_id: Some(self.job_id),
            })
            .await?;
        serialize_output(CognitionRuntimeJobsCancelTool::tool_id(), output)
    }
}

impl RecurringRegister {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        if matches!(self.from, RuntimeFrom::LastRun) {
            let output = CognitionGraphemePromoteLastRunToRecurringTool::new(
                tool.runtime.clone(),
                tool.event_tx.clone(),
                tool.turn_scope.clone(),
            )
            .invoke_typed(GraphemePromoteLastRunInput {
                cron_expr: Some(self.cron_expr),
                timezone: self.timezone.unwrap_or_else(|| "UTC".to_string()),
                queue: self.queue.unwrap_or_else(|| "default".to_string()),
                id: self.recurring_id,
                jitter_seconds: self.jitter_seconds.unwrap_or(0),
                max_attempts: self.max_attempts.unwrap_or(1),
                enabled: self.enabled.unwrap_or(true),
                start_immediately: self.start_immediately.unwrap_or(false),
                source: None,
                delivery: self.delivery,
                feeds: self.feeds,
            })
            .await?;
            return serialize_output(
                CognitionGraphemePromoteLastRunToRecurringTool::tool_id(),
                output,
            );
        }
        if present(self.script.as_deref()) && !present(self.job_type.as_deref()) {
            let output = CognitionGraphemePromoteToRecurringTool::new(
                tool.runtime.clone(),
                tool.event_tx.clone(),
                tool.turn_scope.clone(),
            )
            .invoke_typed(GraphemePromoteToRecurringInput {
                source: self.script,
                cron_expr: Some(self.cron_expr),
                timezone: self.timezone.unwrap_or_else(|| "UTC".to_string()),
                queue: self.queue.unwrap_or_else(|| "default".to_string()),
                id: self.recurring_id,
                jitter_seconds: self.jitter_seconds.unwrap_or(0),
                max_attempts: self.max_attempts.unwrap_or(1),
                enabled: self.enabled.unwrap_or(true),
                start_immediately: self.start_immediately.unwrap_or(false),
                delivery: self.delivery,
                feeds: self.feeds,
            })
            .await?;
            return serialize_output(CognitionGraphemePromoteToRecurringTool::tool_id(), output);
        }
        let output = CognitionRuntimeRecurringRegisterTool::new(
            tool.runtime.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke_typed(RuntimeRecurringRegisterInput {
            source: self.script,
            job_type: self.job_type,
            payload_template_ref: self.payload_ref,
            cron_expr: Some(self.cron_expr),
            timezone: self.timezone,
            queue: self.queue,
            recurring_id: self.recurring_id,
            jitter_seconds: self.jitter_seconds,
            max_attempts: self.max_attempts,
            enabled: self.enabled,
            start_immediately: self.start_immediately,
            delivery: self.delivery,
            feeds: self.feeds,
        })
        .await?;
        serialize_output(CognitionRuntimeRecurringRegisterTool::tool_id(), output)
    }
}

impl RecurringPause {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output =
            CognitionRuntimeRecurringPauseTool::new(tool.runtime.clone(), tool.event_tx.clone())
                .invoke_typed(RuntimeRecurringToggleInput {
                    recurring_id: Some(self.recurring_id),
                })
                .await?;
        serialize_output(CognitionRuntimeRecurringPauseTool::tool_id(), output)
    }
}

impl RecurringCancel {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output =
            CognitionRuntimeRecurringCancelTool::new(tool.runtime.clone(), tool.event_tx.clone())
                .invoke_typed(RuntimeRecurringToggleInput {
                    recurring_id: Some(self.recurring_id),
                })
                .await?;
        serialize_output(CognitionRuntimeRecurringCancelTool::tool_id(), output)
    }
}

impl WorkflowRun {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeWorkflowRunTool::new(
            tool.runtime.clone(),
            tool.workflow_registry.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke_typed(RuntimeWorkflowRunInput {
            name: self.name,
            strategy: self.strategy.unwrap_or_default(),
            mode: self.mode.unwrap_or_else(|| "default".to_string()),
            steps: self.steps,
            on_failure: self.on_failure.unwrap_or_default(),
            note: self.note,
            queue: self.queue.unwrap_or_else(|| "default".to_string()),
        })
        .await?;
        serialize_output(CognitionRuntimeWorkflowRunTool::tool_id(), output)
    }
}

impl WorkflowSchedule {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeWorkflowScheduleTool::new(
            tool.runtime.clone(),
            tool.workflow_registry.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke_typed(RuntimeWorkflowScheduleInput {
            name: self.name,
            strategy: self.strategy.unwrap_or_default(),
            mode: self.mode.unwrap_or_else(|| "default".to_string()),
            steps: self.steps,
            on_failure: self.on_failure.unwrap_or_default(),
            note: self.note,
            queue: self.queue.unwrap_or_else(|| "default".to_string()),
            cron_expr: Some(self.cron_expr),
            timezone: self.timezone.unwrap_or_else(|| "UTC".to_string()),
            recurring_id: None,
            jitter_seconds: 0,
            max_attempts: 1,
            enabled: true,
            start_immediately: false,
            delivery: None,
            feeds: None,
        })
        .await?;
        serialize_output(CognitionRuntimeWorkflowScheduleTool::tool_id(), output)
    }
}

impl WorkflowCancel {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeWorkflowCancelTool::new(
            tool.runtime.clone(),
            tool.workflow_registry.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(RuntimeWorkflowCancelInput {
            workflow_id: Some(self.workflow_id),
        })
        .await?;
        serialize_output(CognitionRuntimeWorkflowCancelTool::tool_id(), output)
    }
}

impl WorkflowPlan {
    async fn execute(self, tool: &CognitionRuntimeMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionRuntimeWorkflowPlanTool::new(tool.event_tx.clone())
            .invoke_typed(RuntimeWorkflowPlanInput {
                goal: Some(self.goal),
                context: self.context.map(WorkflowPlanContext::from_value),
            })
            .await?;
        serialize_output(CognitionRuntimeWorkflowPlanTool::tool_id(), output)
    }
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn job_list_deserializes_from_action_only() {
        let query: RuntimeQueryAction = serde_json::from_value(json!({ "action": "job.list" }))
            .expect("job list");
        match query {
            RuntimeQueryAction::JobList(JobList {
                state,
                correlation_id,
                limit,
            }) => {
                assert!(state.is_none());
                assert!(correlation_id.is_none());
                assert!(limit.is_none());
            }
            other => panic!("expected job.list, got {other:?}"),
        }
    }

    #[test]
    fn runtime_actions_carry_their_params() {
        let query: RuntimeQueryAction = serde_json::from_value(json!({
            "action": "job.status",
            "job_id": "j1"
        }))
        .expect("job status");
        match query {
            RuntimeQueryAction::JobStatus(JobStatus { job_id }) => assert_eq!(job_id, "j1"),
            other => panic!("expected job.status, got {other:?}"),
        }
        let mutate: RuntimeMutateAction = serde_json::from_value(json!({
            "action": "workflow.plan",
            "goal": "digest csv"
        }))
        .expect("workflow plan");
        match mutate {
            RuntimeMutateAction::WorkflowPlan(WorkflowPlan { goal, context }) => {
                assert_eq!(goal, "digest csv");
                assert!(context.is_none());
            }
            other => panic!("expected workflow.plan, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let query =
            serde_json::to_value(schemars::schema_for!(RuntimeQueryAction)).expect("query");
        let mutate =
            serde_json::to_value(schemars::schema_for!(RuntimeMutateAction)).expect("mutate");
        for schema in [&query, &mutate] {
            let props = schema["properties"].as_object().expect("properties");
            assert_eq!(props.len(), 1);
            assert!(
                props["action"]["enum"]
                    .as_array()
                    .is_some_and(|values| !values.is_empty())
            );
            assert_eq!(schema["additionalProperties"], true);
        }
        assert!(
            query["properties"]["action"]["enum"]
                .as_array()
                .expect("query actions")
                .iter()
                .any(|value| value == "job.status")
        );
        assert!(
            mutate["properties"]["action"]["enum"]
                .as_array()
                .expect("mutate actions")
                .iter()
                .any(|value| value == "job.enqueue")
        );
    }

    #[test]
    fn schema_catalog_comes_from_variant_types() {
        let enqueue = runtime_type_schemas()
            .into_iter()
            .find(|entry| entry.name == "job.enqueue")
            .expect("job.enqueue");
        assert_eq!(enqueue.tool, MUTATE_ID);
        assert!(enqueue.parameters["properties"]["script"].is_object());
        assert_eq!(enqueue.parameters["properties"]["action"]["const"], "job.enqueue");
    }
}
