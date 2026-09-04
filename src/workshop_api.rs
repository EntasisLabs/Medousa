//! Public workshop primitives: one query tool and one mutate tool.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;

#[cfg(feature = "full-daemon")]
use crate::agent_runtime::turn_worker::TurnWorkerScheduler;
#[cfg(feature = "full-daemon")]
use crate::agent_runtime::turn_worker_tools::{
    CognitionSpawnTurnWorkerTool, CognitionTurnWorkerCancelTool, CognitionTurnWorkerStatusTool,
    CognitionWorkshopSteerTool, SpawnTurnWorkerInput, TurnWorkerCancelInput, TurnWorkerStatusInput,
    WorkshopSteerInput,
};
use crate::public_api::{COGNITION_WORKSHOP_MUTATE, COGNITION_WORKSHOP_QUERY};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
#[cfg(feature = "full-daemon")]
use crate::typed_tools::{CompatOption, TypedTool, serialize_output};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};
use crate::workshop_contract::{WorkshopSpawn, workshop_spawn_type_schema};

const WORKSHOP_QUERY_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_QUERY);
const WORKSHOP_MUTATE_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_MUTATE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum WorkshopQueryAction {
    #[serde(rename = "workshop.status")]
    Status(WorkshopStatus),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum WorkshopMutateAction {
    #[serde(rename = "workshop.spawn")]
    Spawn(WorkshopSpawn),
    #[serde(rename = "workshop.cancel")]
    Cancel(WorkshopCancel),
    #[serde(rename = "workshop.steer")]
    Steer(WorkshopSteer),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct WorkshopStatus {
    #[serde(default)]
    pub(crate) work_id: Option<String>,
    #[serde(default)]
    pub(crate) session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkshopCancel {
    pub(crate) work_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkshopSteer {
    /// Exact bound-workshop generation returned by begin-work
    pub(crate) work_id: String,
    /// Steer text for the bound workshop
    pub(crate) message: String,
}

impl JsonSchema for WorkshopQueryAction {
    fn schema_name() -> String {
        "WorkshopQueryAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[("action", string_enum_schema(&["workshop.status"]), true)])
    }
}

impl JsonSchema for WorkshopMutateAction {
    fn schema_name() -> String {
        "WorkshopMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["workshop.spawn", "workshop.cancel", "workshop.steer"]),
            true,
        )])
    }
}

pub fn workshop_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<WorkshopStatus>(
            WORKSHOP_QUERY_ID,
            "workshop.status",
            "List or fetch status of background turn workers / bound workshop",
        ),
        workshop_spawn_type_schema(),
        typed_action_schema::<WorkshopCancel>(
            WORKSHOP_MUTATE_ID,
            "workshop.cancel",
            "Cancel a worker generation owned by the active host session",
        ),
        typed_action_schema::<WorkshopSteer>(
            WORKSHOP_MUTATE_ID,
            "workshop.steer",
            "Forward principal guidance into the active bound workshop",
        ),
    ]
}

#[async_trait]
pub trait WorkshopExecution: Send + Sync {
    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value>;
    async fn spawn(&self, input: WorkshopSpawn) -> stasis::prelude::Result<Value>;
    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value>;
    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value>;
}

pub struct CognitionWorkshopQueryTool {
    execution: Arc<dyn WorkshopExecution>,
}

pub struct CognitionWorkshopMutateTool {
    execution: Arc<dyn WorkshopExecution>,
}

pub fn register_workshop_execution_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    execution: Arc<dyn WorkshopExecution>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionWorkshopQueryTool {
        execution: execution.clone(),
    })?;
    registry.register_typed_tool(CognitionWorkshopMutateTool { execution })?;
    Ok(())
}

#[cfg(feature = "full-daemon")]
struct LocalWorkshopExecution {
    scheduler: Arc<TurnWorkerScheduler>,
}

#[cfg(feature = "full-daemon")]
#[async_trait]
impl WorkshopExecution for LocalWorkshopExecution {
    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnWorkerStatusTool::new(self.scheduler.clone())
            .invoke_typed(TurnWorkerStatusInput {
                work_id: CompatOption::from(input.work_id),
                session_id: CompatOption::from(input.session_id),
            })
            .await?;
        serialize_output(CognitionTurnWorkerStatusTool::tool_id(), output)
    }

    async fn spawn(&self, input: WorkshopSpawn) -> stasis::prelude::Result<Value> {
        let output = CognitionSpawnTurnWorkerTool::new(self.scheduler.clone())
            .invoke_typed(SpawnTurnWorkerInput {
                intent: CompatOption::from(input.intent),
                task: CompatOption::from(Some(input.task)),
                user_ack: CompatOption::from(Some(input.user_ack)),
                manuscript_id: CompatOption::from(input.manuscript_id),
                stage_role: CompatOption::from(input.stage_role),
                model_hint: CompatOption::from(input.model_hint),
                execution_target: CompatOption::from(input.execution_target),
            })
            .await?;
        serialize_output(CognitionSpawnTurnWorkerTool::tool_id(), output)
    }

    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value> {
        let output = CognitionTurnWorkerCancelTool::new(self.scheduler.clone())
            .invoke_typed(TurnWorkerCancelInput {
                work_id: input.work_id,
            })
            .await?;
        serialize_output(CognitionTurnWorkerCancelTool::tool_id(), output)
    }

    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value> {
        let output = CognitionWorkshopSteerTool::new(self.scheduler.clone())
            .invoke_typed(WorkshopSteerInput {
                work_id: input.work_id,
                message: input.message,
            })
            .await?;
        serialize_output(CognitionWorkshopSteerTool::tool_id(), output)
    }
}

#[cfg(feature = "full-daemon")]
pub fn register_workshop_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    scheduler: Arc<TurnWorkerScheduler>,
) -> stasis::prelude::Result<()> {
    register_workshop_execution_tools(registry, Arc::new(LocalWorkshopExecution { scheduler }))
}

#[medousa_tool(id = WORKSHOP_QUERY_ID)]
impl CognitionWorkshopQueryTool {
    /// Read workshop/worker status. action is a typed name (workshop.status). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: WorkshopQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = WORKSHOP_MUTATE_ID)]
impl CognitionWorkshopMutateTool {
    /// Write workshop/worker control: spawn, cancel, or steer. action is a typed name (workshop.spawn, workshop.cancel, workshop.steer). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: WorkshopMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionWorkshopQueryTool,
    action: WorkshopQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        WorkshopQueryAction::Status(params) => params.execute(tool).await,
    }
}

async fn dispatch_mutate(
    tool: &CognitionWorkshopMutateTool,
    action: WorkshopMutateAction,
) -> stasis::prelude::Result<Value> {
    match action {
        WorkshopMutateAction::Spawn(params) => params.execute(tool).await,
        WorkshopMutateAction::Cancel(params) => params.execute(tool).await,
        WorkshopMutateAction::Steer(params) => params.execute(tool).await,
    }
}

impl WorkshopStatus {
    async fn execute(self, tool: &CognitionWorkshopQueryTool) -> stasis::prelude::Result<Value> {
        tool.execution.status(self).await
    }
}

impl WorkshopSpawn {
    async fn execute(self, tool: &CognitionWorkshopMutateTool) -> stasis::prelude::Result<Value> {
        tool.execution.spawn(self).await
    }
}

impl WorkshopCancel {
    async fn execute(self, tool: &CognitionWorkshopMutateTool) -> stasis::prelude::Result<Value> {
        tool.execution.cancel(self).await
    }
}

impl WorkshopSteer {
    async fn execute(self, tool: &CognitionWorkshopMutateTool) -> stasis::prelude::Result<Value> {
        tool.execution.steer(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn workshop_actions_carry_their_params() {
        let query: WorkshopQueryAction = serde_json::from_value(json!({
            "action": "workshop.status",
            "work_id": "work-1"
        }))
        .expect("status");
        match query {
            WorkshopQueryAction::Status(WorkshopStatus { work_id, .. }) => {
                assert_eq!(work_id.as_deref(), Some("work-1"));
            }
        }
        let mutate: WorkshopMutateAction = serde_json::from_value(json!({
            "action": "workshop.spawn",
            "intent": "research",
            "task": "look this up",
            "user_ack": "On it."
        }))
        .expect("spawn");
        match mutate {
            WorkshopMutateAction::Spawn(WorkshopSpawn { intent, task, .. }) => {
                assert_eq!(intent.as_deref(), Some("research"));
                assert_eq!(task, "look this up");
            }
            other => panic!("expected workshop.spawn, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let query =
            serde_json::to_value(schemars::schema_for!(WorkshopQueryAction)).expect("query");
        let mutate =
            serde_json::to_value(schemars::schema_for!(WorkshopMutateAction)).expect("mutate");
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
    }
}
