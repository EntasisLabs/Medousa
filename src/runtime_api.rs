//! Public runtime primitives: query or mutate jobs, recurring, workflows, delivery.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{RuntimeComposition, StasisError};
use tokio::sync::mpsc;

use crate::bridge_tools::CognitionMcpPromoteToJobTool;
use crate::events::TuiEvent;
use crate::public_api::{COGNITION_RUNTIME_MUTATE, COGNITION_RUNTIME_QUERY};
use crate::runtime_tools::{
    CognitionRuntimeDeliveryStatusTool, CognitionRuntimeJobsCancelTool,
    CognitionRuntimeJobsListTool, CognitionRuntimeRecurringCancelTool,
    CognitionRuntimeRecurringDoctorTool, CognitionRuntimeRecurringListTool,
    CognitionRuntimeRecurringPauseTool, CognitionRuntimeRecurringRegisterTool,
    CognitionRuntimeWorkflowCancelTool, CognitionRuntimeWorkflowPlanTool,
    CognitionRuntimeWorkflowRunTool, CognitionRuntimeWorkflowScheduleTool,
    CognitionRuntimeWorkflowStatusTool,
};
use crate::tools::{
    CognitionGraphemePromoteLastRunToRecurringTool, CognitionGraphemePromoteToJobTool,
    CognitionGraphemePromoteToRecurringTool, CognitionJobEnqueueTool,
    CognitionRuntimeJobStatusTool, CognitionRuntimeRecurringPreviewTool,
};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};
use crate::workflow::WorkflowRegistry;

const QUERY_ID: ToolId = ToolId::new(COGNITION_RUNTIME_QUERY);
const MUTATE_ID: ToolId = ToolId::new(COGNITION_RUNTIME_MUTATE);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RuntimeResource {
    Job,
    Recurring,
    Workflow,
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RuntimeView {
    #[default]
    List,
    Status,
    Doctor,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RuntimeAction {
    Enqueue,
    Cancel,
    Register,
    Pause,
    Run,
    Schedule,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RuntimeFrom {
    #[default]
    Auto,
    LastRun,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeQueryInput {
    resource: RuntimeResource,
    #[serde(default)]
    view: RuntimeView,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    recurring_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    workflow_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    cron_expr: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
    #[serde(default)]
    #[schemars(with = "usize", skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
    #[serde(default)]
    #[schemars(with = "usize", skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
    #[serde(default)]
    #[schemars(with = "usize", skip_serializing_if = "Option::is_none")]
    pending_limit: Option<usize>,
    #[serde(default)]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    enabled_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeMutateInput {
    resource: RuntimeResource,
    action: RuntimeAction,
    #[serde(default)]
    from: RuntimeFrom,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    recurring_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    workflow_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    job_type: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    payload_ref: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    cron_expr: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    queue: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    goal: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    server_id: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    strategy: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    on_failure: Option<String>,
    #[serde(default)]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    jitter_seconds: Option<i64>,
    #[serde(default)]
    #[schemars(with = "u64", skip_serializing_if = "Option::is_none")]
    max_attempts: Option<u64>,
    #[serde(default)]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default)]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    start_immediately: Option<bool>,
    #[serde(default)]
    #[schemars(
        with = "serde_json::Map<String, Value>",
        skip_serializing_if = "Option::is_none"
    )]
    input: Option<Value>,
    #[serde(default)]
    #[schemars(
        with = "serde_json::Map<String, Value>",
        skip_serializing_if = "Option::is_none"
    )]
    context: Option<Value>,
    #[serde(default)]
    #[schemars(with = "Vec<Value>", skip_serializing_if = "Option::is_none")]
    steps: Option<Value>,
    #[serde(default)]
    #[schemars(
        with = "serde_json::Map<String, Value>",
        skip_serializing_if = "Option::is_none"
    )]
    delivery: Option<Value>,
    #[serde(default)]
    #[schemars(
        with = "serde_json::Map<String, Value>",
        skip_serializing_if = "Option::is_none"
    )]
    feeds: Option<Value>,
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
    /// Inspect jobs, recurring schedules, workflows, or delivery. resource=job|recurring|workflow|delivery. view=list|status|doctor|preview. job with job_id defaults to status.
    async fn invoke_typed(
        &self,
        input: RuntimeQueryInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, input).await?))
    }
}

#[medousa_tool(id = MUTATE_ID)]
impl CognitionRuntimeMutateTool {
    /// Enqueue, cancel, register, pause, run, schedule, or plan durable runtime work. resource=job|recurring|workflow. action=enqueue|cancel|register|pause|run|schedule|plan. script or server_id+tool_name select Grapheme/MCP promote.
    async fn invoke_typed(
        &self,
        input: RuntimeMutateInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, input).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionRuntimeQueryTool,
    input: RuntimeQueryInput,
) -> stasis::prelude::Result<Value> {
    match input.resource {
        RuntimeResource::Job => query_job(tool, input).await,
        RuntimeResource::Recurring => query_recurring(tool, input).await,
        RuntimeResource::Workflow => {
            require(
                input.workflow_id.as_deref(),
                "cognition_runtime_query: workflow status needs workflow_id",
            )?;
            CognitionRuntimeWorkflowStatusTool::new(
                tool.runtime.clone(),
                tool.workflow_registry.clone(),
            )
            .invoke(json_obj([("workflow_id", opt_str(input.workflow_id))]))
            .await
        }
        RuntimeResource::Delivery => {
            CognitionRuntimeDeliveryStatusTool::new(tool.runtime.clone())
                .invoke(json_obj([(
                    "pending_limit",
                    opt_usize(input.pending_limit),
                )]))
                .await
        }
    }
}

async fn query_job(
    tool: &CognitionRuntimeQueryTool,
    input: RuntimeQueryInput,
) -> stasis::prelude::Result<Value> {
    let has_id = input
        .job_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    if matches!(input.view, RuntimeView::Status)
        || (has_id && !matches!(input.view, RuntimeView::List))
    {
        require(
            input.job_id.as_deref(),
            "cognition_runtime_query: job status needs job_id",
        )?;
        CognitionRuntimeJobStatusTool::new(tool.runtime.clone())
            .invoke(json_obj([("job_id", opt_str(input.job_id))]))
            .await
    } else {
        CognitionRuntimeJobsListTool::new(tool.runtime.clone())
            .invoke(json_obj([
                ("state", opt_str(input.state)),
                ("correlation_id", opt_str(input.correlation_id)),
                ("limit", opt_usize(input.limit)),
            ]))
            .await
    }
}

async fn query_recurring(
    tool: &CognitionRuntimeQueryTool,
    input: RuntimeQueryInput,
) -> stasis::prelude::Result<Value> {
    match input.view {
        RuntimeView::Doctor => {
            CognitionRuntimeRecurringDoctorTool::new(tool.runtime.clone())
                .invoke(json_obj([("recurring_id", opt_str(input.recurring_id))]))
                .await
        }
        RuntimeView::Preview => {
            require(
                input.cron_expr.as_deref(),
                "cognition_runtime_query: recurring preview needs cron_expr",
            )?;
            CognitionRuntimeRecurringPreviewTool::new(tool.event_tx.clone())
                .invoke(json_obj([
                    ("cron_expr", opt_str(input.cron_expr)),
                    ("timezone", opt_str(input.timezone)),
                    ("count", opt_usize(input.count)),
                ]))
                .await
        }
        RuntimeView::Status | RuntimeView::List => {
            CognitionRuntimeRecurringListTool::new(tool.runtime.clone())
                .invoke(json_obj([("enabled_only", opt_bool(input.enabled_only))]))
                .await
        }
    }
}

async fn dispatch_mutate(
    tool: &CognitionRuntimeMutateTool,
    input: RuntimeMutateInput,
) -> stasis::prelude::Result<Value> {
    match (input.resource, input.action) {
        (RuntimeResource::Job, RuntimeAction::Enqueue) => enqueue_job(tool, input).await,
        (RuntimeResource::Job, RuntimeAction::Cancel) => {
            require(
                input.job_id.as_deref(),
                "cognition_runtime_mutate: job cancel needs job_id",
            )?;
            CognitionRuntimeJobsCancelTool::new(tool.runtime.clone(), tool.event_tx.clone())
                .invoke(json_obj([("job_id", opt_str(input.job_id))]))
                .await
        }
        (RuntimeResource::Recurring, RuntimeAction::Register) => {
            register_recurring(tool, input).await
        }
        (RuntimeResource::Recurring, RuntimeAction::Pause) => {
            require(
                input.recurring_id.as_deref(),
                "cognition_runtime_mutate: recurring pause needs recurring_id",
            )?;
            CognitionRuntimeRecurringPauseTool::new(tool.runtime.clone(), tool.event_tx.clone())
                .invoke(json_obj([("recurring_id", opt_str(input.recurring_id))]))
                .await
        }
        (RuntimeResource::Recurring, RuntimeAction::Cancel) => {
            require(
                input.recurring_id.as_deref(),
                "cognition_runtime_mutate: recurring cancel needs recurring_id",
            )?;
            CognitionRuntimeRecurringCancelTool::new(tool.runtime.clone(), tool.event_tx.clone())
                .invoke(json_obj([("recurring_id", opt_str(input.recurring_id))]))
                .await
        }
        (RuntimeResource::Workflow, RuntimeAction::Run) => {
            CognitionRuntimeWorkflowRunTool::new(
                tool.runtime.clone(),
                tool.workflow_registry.clone(),
                tool.event_tx.clone(),
                tool.turn_scope.clone(),
            )
            .invoke(workflow_payload(&input))
            .await
        }
        (RuntimeResource::Workflow, RuntimeAction::Schedule) => {
            CognitionRuntimeWorkflowScheduleTool::new(
                tool.runtime.clone(),
                tool.workflow_registry.clone(),
                tool.event_tx.clone(),
                tool.turn_scope.clone(),
            )
            .invoke(workflow_payload(&input))
            .await
        }
        (RuntimeResource::Workflow, RuntimeAction::Cancel) => {
            require(
                input.workflow_id.as_deref(),
                "cognition_runtime_mutate: workflow cancel needs workflow_id",
            )?;
            CognitionRuntimeWorkflowCancelTool::new(
                tool.runtime.clone(),
                tool.workflow_registry.clone(),
                tool.event_tx.clone(),
            )
            .invoke(json_obj([("workflow_id", opt_str(input.workflow_id))]))
            .await
        }
        (RuntimeResource::Workflow, RuntimeAction::Plan) => {
            require(
                input.goal.as_deref(),
                "cognition_runtime_mutate: workflow plan needs goal",
            )?;
            CognitionRuntimeWorkflowPlanTool::new(tool.event_tx.clone())
                .invoke(json_obj([
                    ("goal", opt_str(input.goal)),
                    ("context", input.context),
                ]))
                .await
        }
        (resource, action) => Err(StasisError::PortFailure(format!(
            "cognition_runtime_mutate: resource={resource:?} action={action:?} is not valid"
        ))),
    }
}

async fn enqueue_job(
    tool: &CognitionRuntimeMutateTool,
    input: RuntimeMutateInput,
) -> stasis::prelude::Result<Value> {
    if input
        .server_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        require(
            input.tool_name.as_deref(),
            "cognition_runtime_mutate: MCP job enqueue needs server_id and tool_name",
        )?;
        return CognitionMcpPromoteToJobTool::new(
            tool.runtime.clone(),
            tool.workflow_registry.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke(json_obj([
            ("server_id", opt_str(input.server_id)),
            ("tool_name", opt_str(input.tool_name)),
            ("input", input.input),
            ("note", opt_str(input.note)),
            ("queue", opt_str(input.queue)),
        ]))
        .await;
    }
    if input
        .script
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && input
            .payload_ref
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return CognitionGraphemePromoteToJobTool::new(
            tool.runtime.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke(json_obj([
            ("source", opt_str(input.script)),
            ("queue", opt_str(input.queue)),
            ("max_attempts", opt_u64(input.max_attempts)),
        ]))
        .await;
    }
    require(
        input.job_type.as_deref(),
        "cognition_runtime_mutate: job enqueue needs job_type and payload_ref, or script, or server_id+tool_name",
    )?;
    require(
        input.payload_ref.as_deref(),
        "cognition_runtime_mutate: job enqueue needs job_type and payload_ref, or script, or server_id+tool_name",
    )?;
    CognitionJobEnqueueTool::new(
        tool.runtime.clone(),
        tool.event_tx.clone(),
        tool.turn_scope.clone(),
    )
    .invoke(json_obj([
        ("job_type", opt_str(input.job_type)),
        ("payload_ref", opt_str(input.payload_ref)),
        ("note", opt_str(input.note)),
    ]))
    .await
}

async fn register_recurring(
    tool: &CognitionRuntimeMutateTool,
    input: RuntimeMutateInput,
) -> stasis::prelude::Result<Value> {
    if matches!(input.from, RuntimeFrom::LastRun) {
        require(
            input.cron_expr.as_deref(),
            "cognition_runtime_mutate: last-run recurring register needs cron_expr",
        )?;
        return CognitionGraphemePromoteLastRunToRecurringTool::new(
            tool.runtime.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke(json_obj([
            ("cron_expr", opt_str(input.cron_expr)),
            ("timezone", opt_str(input.timezone)),
            ("queue", opt_str(input.queue)),
            ("id", opt_str(input.recurring_id)),
            ("jitter_seconds", opt_i64(input.jitter_seconds)),
            ("max_attempts", opt_u64(input.max_attempts)),
            ("enabled", opt_bool(input.enabled)),
            ("start_immediately", opt_bool(input.start_immediately)),
        ]))
        .await;
    }
    if input
        .script
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && input
            .job_type
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return CognitionGraphemePromoteToRecurringTool::new(
            tool.runtime.clone(),
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
        )
        .invoke(json_obj([
            ("source", opt_str(input.script)),
            ("cron_expr", opt_str(input.cron_expr)),
            ("timezone", opt_str(input.timezone)),
            ("queue", opt_str(input.queue)),
            ("id", opt_str(input.recurring_id)),
            ("jitter_seconds", opt_i64(input.jitter_seconds)),
            ("max_attempts", opt_u64(input.max_attempts)),
            ("enabled", opt_bool(input.enabled)),
            ("start_immediately", opt_bool(input.start_immediately)),
        ]))
        .await;
    }
    CognitionRuntimeRecurringRegisterTool::new(
        tool.runtime.clone(),
        tool.event_tx.clone(),
        tool.turn_scope.clone(),
    )
    .invoke(json_obj([
        ("source", opt_str(input.script)),
        ("job_type", opt_str(input.job_type)),
        ("payload_template_ref", opt_str(input.payload_ref)),
        ("cron_expr", opt_str(input.cron_expr)),
        ("timezone", opt_str(input.timezone)),
        ("queue", opt_str(input.queue)),
        ("recurring_id", opt_str(input.recurring_id)),
        ("jitter_seconds", opt_i64(input.jitter_seconds)),
        ("max_attempts", opt_u64(input.max_attempts)),
        ("enabled", opt_bool(input.enabled)),
        ("start_immediately", opt_bool(input.start_immediately)),
        ("delivery", input.delivery),
        ("feeds", input.feeds),
    ]))
    .await
}

fn workflow_payload(input: &RuntimeMutateInput) -> Value {
    json_obj([
        ("name", opt_str(input.name.clone())),
        ("strategy", opt_str(input.strategy.clone())),
        ("mode", opt_str(input.mode.clone())),
        ("steps", input.steps.clone()),
        ("on_failure", opt_str(input.on_failure.clone())),
        ("note", opt_str(input.note.clone())),
        ("queue", opt_str(input.queue.clone())),
        ("cron_expr", opt_str(input.cron_expr.clone())),
        ("timezone", opt_str(input.timezone.clone())),
    ])
}

fn require(value: Option<&str>, message: &str) -> stasis::prelude::Result<()> {
    if value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        Ok(())
    } else {
        Err(StasisError::PortFailure(message.to_string()))
    }
}

fn json_obj(fields: impl IntoIterator<Item = (&'static str, Option<Value>)>) -> Value {
    let mut map = Map::new();
    for (key, value) in fields {
        if let Some(value) = value {
            map.insert(key.to_string(), value);
        }
    }
    Value::Object(map)
}

fn opt_str(value: Option<String>) -> Option<Value> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(Value::String)
}

fn opt_usize(value: Option<usize>) -> Option<Value> {
    value.map(|value| json!(value))
}

fn opt_u64(value: Option<u64>) -> Option<Value> {
    value.map(|value| json!(value))
}

fn opt_i64(value: Option<i64>) -> Option<Value> {
    value.map(|value| json!(value))
}

fn opt_bool(value: Option<bool>) -> Option<Value> {
    value.map(Value::Bool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_enums_are_snake_case() {
        let query: RuntimeQueryInput = serde_json::from_value(json!({
            "resource": "job",
            "view": "status",
            "job_id": "j1"
        }))
        .expect("job status");
        assert_eq!(query.resource, RuntimeResource::Job);
        assert_eq!(query.view, RuntimeView::Status);
        let mutate: RuntimeMutateInput = serde_json::from_value(json!({
            "resource": "workflow",
            "action": "plan",
            "goal": "digest csv"
        }))
        .expect("workflow plan");
        assert_eq!(mutate.action, RuntimeAction::Plan);
        assert_eq!(mutate.from, RuntimeFrom::Auto);
    }
}
