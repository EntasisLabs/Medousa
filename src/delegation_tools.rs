//! Bound-remote execution adapter for the canonical workshop tools.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::delegated_task::{
    WORKER_SPAWN_SPEC_SCHEMA_VERSION, WorkerBotSpec, WorkerManuscriptSpec, WorkerParentSpec,
    WorkerSpawnSpec, WorkerToolRequest,
};
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

fn worker_error(message: impl Into<String>) -> stasis::domain::errors::StasisError {
    stasis::domain::errors::StasisError::PortFailure(message.into())
}

#[cfg(feature = "full-daemon")]
fn resolve_remote_manuscript(
    manuscript_id: Option<&str>,
) -> stasis::prelude::Result<Option<WorkerManuscriptSpec>> {
    let Some(id) = manuscript_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let manuscript = crate::identity_manuscript::build_manuscript_context(id)
        .map_err(|error| worker_error(error.to_string()))?;
    Ok(Some(WorkerManuscriptSpec {
        id: manuscript.id,
        name: manuscript.name,
        worker_intent: manuscript.worker_intent,
        stage_role: manuscript.worker_stage_role,
        model_hint: manuscript.worker_model_hint,
        voice_appendix: manuscript.voice_appendix,
        system_appendix: manuscript.system_appendix,
        max_tool_rounds: manuscript.max_tool_rounds,
        tools_allow: manuscript.tools_allow,
        openshell_enabled: manuscript.openshell_enabled,
        openshell_policy_template: manuscript.openshell_policy_template,
        openshell_sandbox_from: manuscript.openshell_sandbox_from,
    }))
}

#[cfg(not(feature = "full-daemon"))]
fn resolve_remote_manuscript(
    _manuscript_id: Option<&str>,
) -> stasis::prelude::Result<Option<WorkerManuscriptSpec>> {
    // Personal/mobile does not own the remote workshop's manuscript catalog.
    // The immutable id remains in the spec and the destination resolves it.
    Ok(None)
}

fn compile_remote_worker_spec(
    spawn: &WorkshopSpawn,
    resolution: &ExecutionPlacementResolution,
) -> stasis::prelude::Result<WorkerSpawnSpec> {
    let execution = crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| worker_error("remote worker spawn requires an admitted daemon turn"))?;
    let manuscript_id = spawn
        .manuscript_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let manuscript = resolve_remote_manuscript(manuscript_id.as_deref())?;
    let intent_text = spawn
        .intent
        .as_deref()
        .map(str::trim)
        .filter(|intent| !intent.is_empty())
        .or_else(|| {
            manuscript
                .as_ref()
                .and_then(|manuscript| manuscript.worker_intent.as_deref())
        })
        // Explicit compatibility for the original bound-remote ingress.
        .unwrap_or("research");
    let intent = crate::agent_runtime::turn_worker::TurnWorkerIntent::parse(intent_text)
        .ok_or_else(|| worker_error(format!("unknown worker intent '{intent_text}'")))?;
    let stage_role = spawn
        .stage_role
        .as_deref()
        .or_else(|| {
            manuscript
                .as_ref()
                .and_then(|value| value.stage_role.as_deref())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let model_hint = spawn
        .model_hint
        .as_deref()
        .or_else(|| {
            manuscript
                .as_ref()
                .and_then(|value| value.model_hint.as_deref())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let manuscript_tools = manuscript
        .as_ref()
        .map(|value| value.tools_allow.as_slice())
        .unwrap_or(&[]);
    let mut tool_names = crate::agent_runtime::turn_worker::worker_allowlist_for_intent_and_tools(
        intent,
        manuscript_tools,
    )
    .into_iter()
    .collect::<Vec<_>>();
    tool_names.sort();
    let bot = execution.bot_identity().map(|bot| WorkerBotSpec {
        bot_id: bot.bot_id().to_string(),
        profile_revision: bot.profile_revision(),
        memory_scope_id: bot.memory_scope_id().to_string(),
        prompt_appendix: bot.prompt_appendix(),
    });
    let parent_mode = crate::agent_mode_state::resolve_for_turn_with_fallback(
        execution.session_id().as_str(),
        None,
        execution.bot_identity().and_then(|bot| bot.default_mode()),
    );
    let parent_scope = execution.legacy_scope();
    let parent_surface = execution.surface();
    let max_tool_rounds = manuscript
        .as_ref()
        .and_then(|value| value.max_tool_rounds)
        .unwrap_or_else(|| crate::agent_runtime::turn_worker::max_worker_tool_rounds(intent))
        .max(1);
    let code_work_id =
        crate::agent_mode_state::get_session_code_binding(execution.session_id().as_str())
            .ok()
            .and_then(|binding| binding.work_id);
    Ok(WorkerSpawnSpec {
        schema_version: WORKER_SPAWN_SPEC_SCHEMA_VERSION,
        intent: intent.as_str().to_string(),
        task: spawn.task.trim().to_string(),
        user_ack: spawn.user_ack.trim().to_string(),
        manuscript_ids: manuscript_id.into_iter().collect(),
        manuscript,
        stage_role,
        model_hint,
        parent: WorkerParentSpec {
            stream_turn_id: 0,
            turn_correlation_id: execution.correlation_id().to_string(),
            agent_mode: Some(parent_mode.mode.as_str().to_string()),
            original_user_prompt: parent_scope.original_prompt.trim().to_string(),
            provider: execution.route().provider().to_string(),
            model: execution.route().model().to_string(),
            response_depth_mode: parent_scope.response_depth_mode.clone(),
            code_work_id,
            bot,
            supports_ui_artifacts: parent_surface.ui_artifacts,
            supports_liquid_markdown: parent_surface.liquid_markdown,
            supports_browser_host: parent_surface.browser_host,
        },
        execution_placement: resolution.clone(),
        max_tool_rounds,
        tools: WorkerToolRequest { names: tool_names },
    })
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
        let worker = compile_remote_worker_spec(&spawn, &resolution)?;
        let intent = worker.intent.clone();
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
            .submit_to(binding.target, worker, parent_runtime_id, resolution)
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
        self.service.steer(&input.work_id, &input.message).await
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
    #[test]
    fn every_canonical_worker_intent_is_available_to_remote_contracts() {
        for intent in [
            "research",
            "general",
            "memory.context",
            "memory.avec_calibrate",
        ] {
            assert!(crate::agent_runtime::turn_worker::TurnWorkerIntent::parse(intent).is_some());
        }
    }
}
