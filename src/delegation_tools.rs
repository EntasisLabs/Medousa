//! Remote execution adapter for the canonical workshop worker command.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::delegation::DelegationService;
use crate::public_api::COGNITION_WORKSHOP_MUTATE;
use crate::schema_api::{advertised_object_schema, string_enum_schema};
use crate::typed_tools::{ExternalJson, ToolId, ToolRegistration, medousa_tool};
use crate::workshop_contract::WorkshopSpawn;

const WORKSHOP_MUTATE_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_MUTATE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum RemoteWorkshopMutateAction {
    #[serde(rename = "workshop.spawn")]
    Spawn(WorkshopSpawn),
}

impl JsonSchema for RemoteWorkshopMutateAction {
    fn schema_name() -> String {
        "RemoteWorkshopMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        advertised_object_schema(&[("action", string_enum_schema(&["workshop.spawn"]), true)])
    }
}

pub struct RemoteWorkshopMutateTool {
    service: Arc<DelegationService>,
}

impl RemoteWorkshopMutateTool {
    pub fn new(service: Arc<DelegationService>) -> Self {
        Self { service }
    }
}

fn remote_spawn_intent(spawn: &WorkshopSpawn) -> stasis::prelude::Result<&str> {
    let intent = spawn.intent.as_deref().unwrap_or("research").trim();
    match intent {
        "research" | "general" => Ok(intent),
        other => Err(stasis::domain::errors::StasisError::PortFailure(format!(
            "bound remote workshop does not admit worker intent '{other}'"
        ))),
    }
}

fn reject_unsupported_remote_hints(spawn: &WorkshopSpawn) -> stasis::prelude::Result<()> {
    if spawn
        .manuscript_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || spawn
            .stage_role
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || spawn
            .model_hint
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(stasis::domain::errors::StasisError::PortFailure(
            "bound remote workshop does not yet accept manuscript, stage, or model overrides"
                .to_string(),
        ));
    }
    Ok(())
}

fn remote_work_id(result: &Value) -> Option<&str> {
    result
        .get("execution")
        .and_then(|execution| execution.get("executionId"))
        .and_then(Value::as_str)
}

#[medousa_tool(id = WORKSHOP_MUTATE_ID)]
impl RemoteWorkshopMutateTool {
    /// Start worker execution on the explicitly bound remote workshop. Use action=workshop.spawn. The bound peer is selected by daemon policy, never by model input.
    async fn invoke_typed(
        &self,
        action: RemoteWorkshopMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        let RemoteWorkshopMutateAction::Spawn(spawn) = action;
        let intent = remote_spawn_intent(&spawn)?;
        reject_unsupported_remote_hints(&spawn)?;
        let result = self.service.delegate(&spawn.task).await?;
        Ok(ExternalJson::new(json!({
            "ok": true,
            "worker_spawned": true,
            "execution_target": "bound_remote",
            "work_id": remote_work_id(&result),
            "intent": intent,
            "status": "completed",
            "user_ack": spawn.user_ack,
            "result": result,
        })))
    }
}

pub fn register_remote_workshop_tools(
    registry: &mut impl ToolRegistration,
    service: Arc<DelegationService>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(RemoteWorkshopMutateTool::new(service))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_workshop_schema_only_advertises_spawn() {
        let schema = serde_json::to_value(schemars::schema_for!(RemoteWorkshopMutateAction))
            .expect("schema");
        assert_eq!(
            schema["properties"]["action"]["enum"],
            json!(["workshop.spawn"])
        );
    }

    #[test]
    fn remote_spawn_rejects_unavailable_execution_profiles() {
        let spawn = WorkshopSpawn {
            intent: Some("memory.context".to_string()),
            task: "remember this".to_string(),
            user_ack: "On it.".to_string(),
            manuscript_id: None,
            stage_role: None,
            model_hint: None,
        };
        assert!(remote_spawn_intent(&spawn).is_err());
    }
}
