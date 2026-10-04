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
#[cfg(feature = "full-daemon")]
use crate::daemon::coordination::assignments::{
    AssignmentEventsQuery, AssignmentGetQuery, AssignmentListQuery, OwnerEventsQuery,
};
#[cfg(feature = "full-daemon")]
use crate::daemon::work_units::{
    WorkContentResolveInput, WorkGraphMutateInput, WorkNativeReconcileInput,
    WorkNativeResolveInput, WorkProjectResolveInput, WorkUnitGetQuery,
};
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
    CognitionRuntimeJobStatusTool, CognitionRuntimeRecurringPreviewTool,
    GraphemePromoteLastRunInput, GraphemePromoteToJobInput, GraphemePromoteToRecurringInput,
    JobEnqueueInput, RuntimeJobStatusInput, RuntimeRecurringPreviewInput,
};
use crate::typed_tools::{
    CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};
use crate::workflow::WorkflowRegistry;
#[cfg(feature = "full-daemon")]
use medousa_types::work_coordination::{WorkCoordinationInput, WorkCoordinationQuery};
#[cfg(feature = "full-daemon")]
use medousa_types::work_unit::{WorkEventsQuery, WorkGraphQuery};
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
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.graph")]
    WorkGraph(WorkGraphQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.get")]
    WorkGet(WorkUnitGetQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.events")]
    WorkEvents(WorkEventsQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.coordination")]
    WorkCoordination(WorkCoordinationQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "assignment.list")]
    AssignmentList(AssignmentListQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "assignment.get")]
    AssignmentGet(AssignmentGetQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "assignment.events")]
    AssignmentEvents(AssignmentEventsQuery),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "owner.events")]
    OwnerEvents(OwnerEventsQuery),
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
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.record")]
    WorkRecord(WorkGraphMutateInput),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.resolve")]
    WorkResolve(WorkNativeResolveInput),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.reconcile")]
    WorkReconcile(WorkNativeReconcileInput),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.resolve_project")]
    WorkResolveProject(WorkProjectResolveInput),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.resolve_content")]
    WorkResolveContent(WorkContentResolveInput),
    #[cfg(feature = "full-daemon")]
    #[serde(rename = "work.coordinate")]
    WorkCoordinate(WorkCoordinationInput),
    #[serde(rename = "job.enqueue")]
    JobEnqueue(JobEnqueue),
    #[serde(rename = "job.cancel")]
    JobCancel(JobCancel),
    #[serde(rename = "recurring.register")]
    RecurringRegister(Box<RecurringRegister>),
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
        let mut actions = vec![
            "job.list",
            "job.status",
            "recurring.list",
            "recurring.doctor",
            "recurring.preview",
            "workflow.status",
            "delivery.status",
        ];
        #[cfg(feature = "full-daemon")]
        actions
            .splice(
                0..0,
                [
                    "work.graph",
                    "work.get",
                    "work.events",
                    "work.coordination",
                    "assignment.list",
                    "assignment.get",
                    "assignment.events",
                    "owner.events",
                ],
            )
            .for_each(drop);
        advertised_object_schema(&[("action", string_enum_schema(&actions), true)])
    }
}

impl JsonSchema for RuntimeMutateAction {
    fn schema_name() -> String {
        "RuntimeMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        let actions = [
            "job.enqueue",
            "job.cancel",
            "recurring.register",
            "recurring.pause",
            "recurring.cancel",
            "workflow.run",
            "workflow.schedule",
            "workflow.cancel",
            "workflow.plan",
        ];
        #[cfg(feature = "full-daemon")]
        let actions = [
            "work.record",
            "work.resolve",
            "work.reconcile",
            "work.resolve_project",
            "work.resolve_content",
            "work.coordinate",
        ]
        .into_iter()
        .chain(actions)
        .collect::<Vec<_>>();
        advertised_object_schema(&[("action", string_enum_schema(&actions), true)])
    }
}

pub fn runtime_type_schemas() -> Vec<TypedActionSchema> {
    let mut schemas = vec![
        typed_action_schema::<JobList>(QUERY_ID, "job.list", "List durable jobs"),
        typed_action_schema::<JobStatus>(QUERY_ID, "job.status", "Status for one job"),
        typed_action_schema::<RecurringList>(
            QUERY_ID,
            "recurring.list",
            "List recurring schedules",
        ),
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
        typed_action_schema::<WorkflowStatus>(
            QUERY_ID,
            "workflow.status",
            "Status for one workflow",
        ),
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
        typed_action_schema::<RecurringPause>(
            MUTATE_ID,
            "recurring.pause",
            "Pause a recurring schedule",
        ),
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
    ];
    #[cfg(feature = "full-daemon")]
    {
        let mut assignment_schemas = vec![
            typed_action_schema::<WorkGraphQuery>(
                QUERY_ID,
                "work.graph",
                "Inspect your saved resource relationships, work scopes, or intent events on this workshop",
            ),
            typed_action_schema::<WorkUnitGetQuery>(
                QUERY_ID,
                "work.get",
                "Inspect a session-independent work unit by exact identity",
            ),
            typed_action_schema::<WorkEventsQuery>(
                QUERY_ID,
                "work.events",
                "Read the next bounded events for your durable work subscription without consuming them; acknowledge the exact event with work.record",
            ),
            typed_action_schema::<WorkCoordinationQuery>(
                QUERY_ID,
                "work.coordination",
                "Inspect durable execute/review registration, native custody, pinned revision, and review verdict",
            ),
            typed_action_schema::<WorkCoordinationInput>(
                MUTATE_ID,
                "work.coordinate",
                "Register bounded execute/review observation for standalone finite work using exact native proposals. Optional fix_review_rounds (at most three) run only after exact changes_requested; each stage needs its own approved grant. Reviewers opt into medousa-work-review-v1 and follow-up executors into medousa-work-fix-v1. No grants or owner-chat continuations are issued",
            ),
            typed_action_schema::<WorkGraphMutateInput>(
                MUTATE_ID,
                "work.record",
                "Record intent, explicit scope, relationships, or contact preference; this does not launch, schedule, cancel, or contact native executors",
            ),
            typed_action_schema::<WorkNativeResolveInput>(
                MUTATE_ID,
                "work.resolve",
                "Resolve an exact user-vault note, folder, or stable reference on an explicit vault root; records bounded native metadata without reading note bodies into the response",
            ),
            typed_action_schema::<WorkProjectResolveInput>(
                MUTATE_ID,
                "work.resolve_project",
                "Observe the repository identity, lifecycle of an exact owned Forge work, or a note/folder in its pinned governed overlay; metadata only, no execution or file effect replay",
            ),
            typed_action_schema::<WorkContentResolveInput>(
                MUTATE_ID,
                "work.resolve_content",
                "Observe an exact visible artifact payload, your environment component, retained feed stream, or previously observed content reference; metadata only, without scheduling or publishing content",
            ),
            typed_action_schema::<WorkNativeReconcileInput>(
                MUTATE_ID,
                "work.reconcile",
                "Inspect native vault repair metadata, adopt a physically proven external move, or quarantine an exact uncertain journal without replaying file effects; requires explicit root and durable command identity",
            ),
            typed_action_schema::<AssignmentListQuery>(
                QUERY_ID,
                "assignment.list",
                "List your owned assignments across native executors on this workshop",
            ),
            typed_action_schema::<AssignmentGetQuery>(
                QUERY_ID,
                "assignment.get",
                "Inspect one owned assignment and exact execution provenance",
            ),
            typed_action_schema::<AssignmentEventsQuery>(
                QUERY_ID,
                "assignment.events",
                "Read an owned assignment's durable event history",
            ),
            typed_action_schema::<OwnerEventsQuery>(
                QUERY_ID,
                "owner.events",
                "Inspect your durable pending, consumed, and blocked owner events",
            ),
        ];
        assignment_schemas.append(&mut schemas);
        schemas = assignment_schemas;
    }
    schemas
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
    /// Inspect saved work scopes and relationships, owned assignments, jobs, recurring, workflows, or delivery. action is a typed name (work.graph, job.list, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: RuntimeQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = MUTATE_ID)]
impl CognitionRuntimeMutateTool {
    /// Mutate durable runtime work. work.coordinate registers a bounded native executor/reviewer handoff without issuing grants. work.record saves session-independent intent; work.resolve refreshes exact native vault metadata; work.reconcile repairs vault identity metadata; work.resolve_project observes owned Forge projects, work, and pinned overlays; work.resolve_content observes artifacts, components, and feeds without executing work. job.enqueue and workflow.run execute through their native admission. Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: RuntimeMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

#[cfg(feature = "full-daemon")]
fn runtime_error(error: impl std::fmt::Display) -> stasis::prelude::StasisError {
    stasis::prelude::StasisError::PortFailure(error.to_string())
}

#[cfg(feature = "full-daemon")]
fn admitted_assignment_query() -> stasis::prelude::Result<(
    Arc<crate::daemon::coordination::LocalPeerDispatcher>,
    Arc<crate::agent_runtime::execution_context::TurnExecutionContext>,
)> {
    let turn = crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| runtime_error("assignment queries require an admitted owner turn"))?;
    let host = crate::daemon::coordination::local_coordination_host()
        .ok_or_else(|| runtime_error("assignment ledger is not available on this workshop"))?;
    Ok((host, turn))
}

#[cfg(feature = "full-daemon")]
fn admitted_work_access() -> stasis::prelude::Result<(
    Arc<crate::daemon::work_units::WorkUnitHost>,
    Arc<crate::agent_runtime::execution_context::TurnExecutionContext>,
)> {
    let turn = crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| runtime_error("work domain access requires an admitted owner turn"))?;
    let host = crate::daemon::work_units::local_work_unit_host()
        .ok_or_else(|| runtime_error("work domain registry is not available on this workshop"))?;
    Ok((host, turn))
}

async fn dispatch_query(
    tool: &CognitionRuntimeQueryTool,
    action: RuntimeQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::WorkGraph(params) => {
            let (host, turn) = admitted_work_access()?;
            host.graph(&turn, params).await.map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::WorkGet(params) => {
            let (host, turn) = admitted_work_access()?;
            host.get(&turn, params).await.map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::WorkEvents(params) => {
            let (host, turn) = admitted_work_access()?;
            host.events(&turn, params).await.map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::WorkCoordination(params) => {
            let (host, turn) = admitted_work_access()?;
            host.coordination(&turn, params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::AssignmentList(params) => {
            let (host, turn) = admitted_assignment_query()?;
            host.list_assignments(turn.principal(), params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::AssignmentGet(params) => {
            let (host, turn) = admitted_assignment_query()?;
            host.get_assignment(turn.principal(), params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::AssignmentEvents(params) => {
            let (host, turn) = admitted_assignment_query()?;
            host.assignment_events(turn.principal(), params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeQueryAction::OwnerEvents(params) => {
            let (host, turn) = admitted_assignment_query()?;
            host.owner_events(turn.principal(), params)
                .await
                .map_err(runtime_error)
        }
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
        #[cfg(feature = "full-daemon")]
        RuntimeMutateAction::WorkRecord(params) => {
            let (host, turn) = admitted_work_access()?;
            host.record(&turn, params).await.map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeMutateAction::WorkResolve(params) => {
            let (host, turn) = admitted_work_access()?;
            host.resolve_native(&turn, params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeMutateAction::WorkReconcile(params) => {
            let (host, turn) = admitted_work_access()?;
            host.reconcile_native(&turn, params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeMutateAction::WorkResolveProject(params) => {
            let (host, turn) = admitted_work_access()?;
            host.resolve_project(&turn, params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeMutateAction::WorkResolveContent(params) => {
            let (host, turn) = admitted_work_access()?;
            host.resolve_content(&turn, params)
                .await
                .map_err(runtime_error)
        }
        #[cfg(feature = "full-daemon")]
        RuntimeMutateAction::WorkCoordinate(params) => {
            let (host, turn) = admitted_work_access()?;
            host.coordinate(&turn, params).await.map_err(runtime_error)
        }
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
        let output =
            CognitionRuntimeJobsCancelTool::new(tool.runtime.clone(), tool.event_tx.clone())
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

    #[cfg(feature = "full-daemon")]
    #[test]
    fn work_actions_are_typed_and_cannot_override_the_admitted_owner() {
        for action in ["work.graph", "work.get"] {
            assert!(
                serde_json::from_value::<RuntimeQueryAction>(json!({
                    "action": action, "user_id": "user:other", "work_unit_id": "work-1"
                }))
                .is_err()
            );
        }
        assert!(matches!(
            serde_json::from_value::<RuntimeQueryAction>(json!({
                "action": "work.graph", "collection": "work_units", "limit": 10
            }))
            .unwrap(),
            RuntimeQueryAction::WorkGraph(_)
        ));
        let accepted = json!({
            "action": "work.record", "command": {
                "command_id": "accept-work-1", "expected_revision": 0,
                "mutation": { "operation": "accept_work", "work_unit_id": "work-1",
                    "intent": "Keep documentation current", "kind": "maintenance",
                    "scope": {}, "completion_condition": "Matches accepted code changes" }
            }
        });
        assert!(matches!(
            serde_json::from_value::<RuntimeMutateAction>(accepted.clone()).unwrap(),
            RuntimeMutateAction::WorkRecord(_)
        ));
        let mut spoofed = accepted.clone();
        spoofed["command"]["user_id"] = json!("user:other");
        assert!(serde_json::from_value::<RuntimeMutateAction>(spoofed).is_err());
        let mut unknown = accepted;
        unknown["command"]["mutation"]["operation"] = json!("launch_agent");
        assert!(serde_json::from_value::<RuntimeMutateAction>(unknown).is_err());
        assert!(admitted_work_access().is_err());
        let resolve = json!({"action":"work.resolve","root_id":"personal","target":{"kind":"note","path":"docs.md"}});
        assert!(matches!(
            serde_json::from_value::<RuntimeMutateAction>(resolve.clone()).unwrap(),
            RuntimeMutateAction::WorkResolve(_)
        ));
        let mut spoofed_resolve = resolve;
        spoofed_resolve["user_id"] = json!("user:other");
        assert!(serde_json::from_value::<RuntimeMutateAction>(spoofed_resolve).is_err());
        let repair = json!({"action":"work.reconcile","root_id":"personal","command":{"operation":"inspect"}});
        assert!(matches!(
            serde_json::from_value::<RuntimeMutateAction>(repair.clone()).unwrap(),
            RuntimeMutateAction::WorkReconcile(_)
        ));
        let mut spoofed_repair = repair.clone();
        spoofed_repair["owner_id"] = json!("other");
        assert!(serde_json::from_value::<RuntimeMutateAction>(spoofed_repair).is_err());
        let mut spoofed_command = repair;
        spoofed_command["command"]["owner_id"] = json!("other");
        assert!(serde_json::from_value::<RuntimeMutateAction>(spoofed_command).is_err());
        let project = json!({"action":"work.resolve_project","work_id":"work-test","target":{"kind":"project"}});
        assert!(matches!(
            serde_json::from_value::<RuntimeMutateAction>(project.clone()).unwrap(),
            RuntimeMutateAction::WorkResolveProject(_)
        ));
        for field in ["owner_id", "repo_path", "native_revision"] {
            let mut spoofed = project.clone();
            spoofed["target"][field] = json!("spoofed");
            assert!(serde_json::from_value::<RuntimeMutateAction>(spoofed).is_err());
        }
        for target in [
            json!({"kind":"artifact","session_id":"session-test","artifact_id":"art:test"}),
            json!({"kind":"component","component_id":"dashboard"}),
            json!({"kind":"feed","feed_id":"digest"}),
        ] {
            let content = json!({"action":"work.resolve_content","target":target});
            assert!(matches!(
                serde_json::from_value::<RuntimeMutateAction>(content.clone()).unwrap(),
                RuntimeMutateAction::WorkResolveContent(_)
            ));
            for field in [
                "owner_id",
                "profile_id",
                "native_revision",
                "resolution",
                "payload_path",
            ] {
                let mut spoofed = content.clone();
                spoofed["target"][field] = json!("spoofed");
                assert!(serde_json::from_value::<RuntimeMutateAction>(spoofed).is_err());
                let mut outer = content.clone();
                outer[field] = json!("spoofed");
                assert!(serde_json::from_value::<RuntimeMutateAction>(outer).is_err());
            }
        }
        for name in [
            "work.graph",
            "work.get",
            "work.events",
            "work.record",
            "work.resolve",
            "work.reconcile",
            "work.resolve_project",
            "work.resolve_content",
            "work.coordinate",
        ] {
            assert!(
                runtime_type_schemas()
                    .iter()
                    .any(|entry| entry.name == name)
            );
        }
    }

    #[cfg(feature = "full-daemon")]
    #[test]
    fn work_coordination_actions_bind_owner_and_native_facts_outside_model_input() {
        let authority = format!("auth_{}", "a".repeat(64));
        let input = json!({"action":"work.coordinate", "coordination_id":"coord", "work_unit_id":"unit", "expected_scope_revision":1,
            "channel":{"authority_id":authority, "channel_id":"channel"}, "executor_proposal_id":"executor", "reviewer_proposal_id":"reviewer", "deadline":"2026-10-02T20:00:00Z"});
        assert!(matches!(
            serde_json::from_value::<RuntimeMutateAction>(input.clone()).unwrap(),
            RuntimeMutateAction::WorkCoordinate(_)
        ));
        for field in [
            "owner_id",
            "domain",
            "scope_digest",
            "head_oid",
            "execution_grant",
            "decision",
            "provenance",
        ] {
            let mut spoofed = input.clone();
            spoofed[field] = "spoofed".into();
            assert!(
                serde_json::from_value::<RuntimeMutateAction>(spoofed).is_err(),
                "{field}"
            );
        }
        let query = json!({"action":"work.coordination", "coordination_id":"coord", "channel":input["channel"]});
        assert!(matches!(
            serde_json::from_value::<RuntimeQueryAction>(query.clone()).unwrap(),
            RuntimeQueryAction::WorkCoordination(_)
        ));
        let mut spoofed = query;
        spoofed["owner_id"] = "other".into();
        assert!(serde_json::from_value::<RuntimeQueryAction>(spoofed).is_err());
        for name in ["work.coordinate", "work.coordination"] {
            assert!(
                runtime_type_schemas()
                    .iter()
                    .any(|entry| entry.name == name)
            );
        }
    }

    #[cfg(feature = "full-daemon")]
    #[test]
    fn assignment_queries_do_not_accept_model_supplied_owners() {
        assert!(
            serde_json::from_value::<RuntimeQueryAction>(json!({
                "action": "assignment.list", "principal_id": "someone-else"
            }))
            .is_err()
        );
        assert!(matches!(
            serde_json::from_value::<RuntimeQueryAction>(json!({
                "action": "assignment.list", "kind": "external_peer", "limit": 5
            }))
            .unwrap(),
            RuntimeQueryAction::AssignmentList(_)
        ));
        assert!(admitted_assignment_query().is_err());
        assert!(
            serde_json::from_value::<RuntimeQueryAction>(json!({
                "action": "owner.events", "owner_principal_id": "someone-else"
            }))
            .is_err()
        );
        for name in [
            "assignment.list",
            "assignment.get",
            "assignment.events",
            "owner.events",
        ] {
            assert!(
                runtime_type_schemas()
                    .iter()
                    .any(|entry| entry.name == name)
            );
        }
    }

    #[cfg(not(feature = "full-daemon"))]
    #[test]
    fn embedded_runtime_does_not_advertise_workshop_assignment_queries() {
        for action in [
            "work.graph",
            "work.get",
            "work.coordination",
            "assignment.list",
            "assignment.get",
            "assignment.events",
            "owner.events",
        ] {
            assert!(
                serde_json::from_value::<RuntimeQueryAction>(json!({ "action": action })).is_err()
            );
            assert!(
                !runtime_type_schemas()
                    .iter()
                    .any(|entry| entry.name == action)
            );
        }
        for action in [
            "work.record",
            "work.resolve",
            "work.reconcile",
            "work.resolve_project",
            "work.resolve_content",
            "work.coordinate",
        ] {
            assert!(
                serde_json::from_value::<RuntimeMutateAction>(json!({"action": action})).is_err()
            );
            assert!(
                !runtime_type_schemas()
                    .iter()
                    .any(|entry| entry.name == action)
            );
        }
    }

    #[test]
    fn job_list_deserializes_from_action_only() {
        let query: RuntimeQueryAction =
            serde_json::from_value(json!({ "action": "job.list" })).expect("job list");
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
        let query = serde_json::to_value(schemars::schema_for!(RuntimeQueryAction)).expect("query");
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
        assert_eq!(
            enqueue.parameters["properties"]["action"]["const"],
            "job.enqueue"
        );
    }
}
