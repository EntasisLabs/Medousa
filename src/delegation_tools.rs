//! Bound-remote execution adapter for the canonical workshop tools.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::delegation::DelegationService;
use crate::workshop_api::{
    WorkshopCancel, WorkshopExecutionRouter, WorkshopExecutionTarget, WorkshopIngressDefault,
    WorkshopStatus, WorkshopSteer, register_workshop_execution_tools,
};
use crate::workshop_contract::{
    ExecutionPlacementResolution, ExecutionTargetCandidate, ExecutionTargetResolutionError,
    WorkshopSpawn,
};

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
            ExecutionTargetResolutionError::UnsupportedTarget {
                detail: "the selected remote workshop does not yet accept manuscript, stage, or model overrides"
                    .to_string(),
            }
            .to_string(),
        ));
    }
    Ok(())
}

#[async_trait]
impl WorkshopExecutionTarget for RemoteWorkshopExecution {
    async fn candidate(&self) -> stasis::prelude::Result<Option<ExecutionTargetCandidate>> {
        let binding =
            self.service.binding().await.map_err(|error| {
                stasis::domain::errors::StasisError::PortFailure(error.to_string())
            })?;
        Ok(binding.map(|binding| {
            let runtime_id = binding.target.peer_device_id;
            ExecutionTargetCandidate {
                runtime_id: runtime_id.clone(),
                capabilities: stasis::domain::runtime::placement::WorkerCapabilities::any()
                    .node_id(runtime_id)
                    .with_capability("assistant.work"),
            }
        }))
    }

    async fn status(&self, input: WorkshopStatus) -> stasis::prelude::Result<Value> {
        self.service
            .status(input.work_id.as_deref(), input.session_id.as_deref())
            .await
    }

    async fn spawn_resolved(
        &self,
        spawn: WorkshopSpawn,
        parent_runtime_id: &str,
        resolution: ExecutionPlacementResolution,
    ) -> stasis::prelude::Result<Value> {
        let intent = remote_spawn_intent(&spawn)?.to_string();
        reject_unsupported_remote_hints(&spawn)?;
        let binding = self
            .service
            .binding()
            .await
            .map_err(|error| stasis::domain::errors::StasisError::PortFailure(error.to_string()))?
            .ok_or_else(|| {
                stasis::domain::errors::StasisError::PortFailure(
                    ExecutionTargetResolutionError::ExactUnavailable {
                        runtime_id: resolution.resolved_runtime_id.clone(),
                    }
                    .to_string(),
                )
            })?;
        if binding.target.peer_device_id != resolution.resolved_runtime_id {
            return Err(stasis::domain::errors::StasisError::PortFailure(
                ExecutionTargetResolutionError::ExactUnavailable {
                    runtime_id: resolution.resolved_runtime_id,
                }
                .to_string(),
            ));
        }
        let ticket = self
            .service
            .submit_to(
                binding.target,
                &spawn.task,
                &spawn.user_ack,
                &intent,
                parent_runtime_id,
                resolution,
            )
            .await?;
        Ok(json!({
            "ok": true,
            "worker_spawned": true,
            "execution_target": "bound_remote",
            "parent_runtime_id": ticket.parent_runtime_id,
            "execution_placement": ticket.execution_placement,
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
    let parent_runtime_id = crate::workshop_authority::current()
        .map(|authority| authority.as_str().to_string())
        .unwrap_or_else(|_| crate::workshop_contract::default_unknown_runtime_id());
    let target: Arc<dyn WorkshopExecutionTarget> = Arc::new(RemoteWorkshopExecution { service });
    register_workshop_execution_tools(
        registry,
        Arc::new(WorkshopExecutionRouter::new(
            parent_runtime_id,
            WorkshopIngressDefault::BoundRemote,
            vec![target],
        )),
    )
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
            execution_target: None,
        };
        assert!(remote_spawn_intent(&spawn).is_err());
    }
}
