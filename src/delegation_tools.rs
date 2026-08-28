//! Bound-remote execution adapter for the canonical workshop tools.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::delegation::DelegationService;
use crate::workshop_api::{
    WorkshopCancel, WorkshopExecution, WorkshopStatus, WorkshopSteer,
    register_workshop_execution_tools,
};
use crate::workshop_contract::WorkshopSpawn;

struct RemoteWorkshopExecution {
    service: Arc<DelegationService>,
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

#[async_trait]
impl WorkshopExecution for RemoteWorkshopExecution {
    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value> {
        self.service
            .status(input.work_id.as_deref(), input.session_id.as_deref())
            .await
    }

    async fn spawn(&self, spawn: WorkshopSpawn) -> stasis::prelude::Result<Value> {
        let intent = remote_spawn_intent(&spawn)?.to_string();
        reject_unsupported_remote_hints(&spawn)?;
        let ticket = self
            .service
            .submit(&spawn.task, &spawn.user_ack, &intent)
            .await?;
        Ok(json!({
            "ok": true,
            "worker_spawned": true,
            "execution_target": "bound_remote",
            "work_id": ticket.work_id,
            "stasis_job_id": ticket.job_id,
            "intent": intent,
            "status": ticket.status,
            "user_ack": spawn.user_ack,
            "message": "Remote worker admitted on the durable workshop bus.",
        }))
    }

    async fn cancel(&self, input: WorkshopCancel) -> stasis::prelude::Result<Value> {
        self.service.cancel(&input.work_id).await
    }

    async fn steer(&self, input: WorkshopSteer) -> stasis::prelude::Result<Value> {
        let _requested_generation = input.work_id;
        let _guidance = input.message;
        Err(stasis::domain::errors::StasisError::PortFailure(
            "the bound remote workshop does not support steering yet".to_string(),
        ))
    }
}

pub fn register_remote_workshop_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    service: Arc<DelegationService>,
) -> stasis::prelude::Result<()> {
    register_workshop_execution_tools(registry, Arc::new(RemoteWorkshopExecution { service }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
