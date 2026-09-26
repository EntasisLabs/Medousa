//! Bound-remote execution adapter for the canonical workshop tools.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::delegated_task::{
    WORKER_SPAWN_SPEC_SCHEMA_VERSION, WorkerBotSpec, WorkerCodeProjectRef, WorkerManuscriptSpec,
    WorkerParentSpec, WorkerSpawnSpec, WorkerToolRequest,
};
use crate::delegation::DelegationService;
use crate::workshop_api::{
    WorkshopCancel, WorkshopExecutionRouter, WorkshopExecutionTarget, WorkshopIngressDefault,
    WorkshopPlacementRequest, WorkshopStatus, WorkshopSteer, register_workshop_execution_tools,
};
use crate::workshop_contract::{
    ExecutionPlacementResolution, ExecutionTargetCandidate, ExecutionTargetResolutionError,
    WorkerCodeProjectSetup, WorkshopSpawn,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingRemoteWorker {
    pub(crate) schema_version: u32,
    pub(crate) intent: String,
    pub(crate) task: String,
    pub(crate) user_ack: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) manuscript_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) manuscript: Option<WorkerManuscriptSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stage_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model_hint: Option<String>,
    pub(crate) parent: WorkerParentSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) code_project: Option<WorkerCodeProjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) code_project_setup: Option<WorkerCodeProjectSetup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) world_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expected_world_runtime_id: Option<String>,
    pub(crate) max_tool_rounds: usize,
    pub(crate) tools: WorkerToolRequest,
}

impl PendingRemoteWorker {
    pub(crate) fn resolve(
        &self,
        resolution: &ExecutionPlacementResolution,
    ) -> stasis::prelude::Result<WorkerSpawnSpec> {
        if let Some(expected_runtime_id) = &self.expected_world_runtime_id
            && resolution.resolved_runtime_id != *expected_runtime_id
        {
            return Err(worker_error(format!(
                "requested worlds belong to execution runtime '{expected_runtime_id}', not '{}'",
                resolution.resolved_runtime_id
            )));
        }
        Ok(WorkerSpawnSpec {
            schema_version: self.schema_version,
            intent: self.intent.clone(),
            task: self.task.clone(),
            user_ack: self.user_ack.clone(),
            manuscript_ids: self.manuscript_ids.clone(),
            manuscript: self.manuscript.clone(),
            stage_role: self.stage_role.clone(),
            model_hint: self.model_hint.clone(),
            parent: self.parent.clone(),
            code_project: self.code_project.clone(),
            code_project_setup: self.code_project_setup.clone(),
            execution_placement: resolution.clone(),
            world_ids: self.world_ids.clone(),
            max_tool_rounds: self.max_tool_rounds,
            tools: self.tools.clone(),
        })
    }
}

pub(crate) fn capture_remote_worker_spec(
    spawn: &WorkshopSpawn,
) -> stasis::prelude::Result<PendingRemoteWorker> {
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
    let expected_world_runtime_id = crate::turn_scope::execution_runtime_for_requested_worlds(
        &spawn.world_ids,
        &parent_scope.selected_worlds,
    )
    .map_err(worker_error)?;
    let world_ids = match expected_world_runtime_id.as_deref() {
        Some(runtime_id) => crate::turn_scope::resolve_requested_world_ids(
            &spawn.world_ids,
            &parent_scope.selected_worlds,
            runtime_id,
        )
        .map_err(worker_error)?,
        None => Vec::new(),
    };
    let code_binding =
        crate::agent_mode_state::get_session_code_binding(execution.session_id().as_str()).ok();
    let code_work_id = spawn
        .code_project_setup
        .is_none()
        .then(|| {
            code_binding
                .as_ref()
                .and_then(|binding| binding.work_id.clone())
        })
        .flatten();
    let code_project = if intent == crate::agent_runtime::turn_worker::TurnWorkerIntent::Coder {
        if spawn.code_project_setup.is_some() {
            None
        } else {
            let binding = code_binding.as_ref().ok_or_else(|| {
                worker_error(
                    "remote Coder requires a bound project or explicit destination project setup",
                )
            })?;
            let runtime_id = binding
                .execution_runtime_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| worker_error("remote Coder project has no execution authority"))?;
            let work_id = binding
                .work_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| worker_error("remote Coder project has no Forge undertaking"))?;
            let repo_id = binding
                .repo_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| worker_error("remote Coder project has no repository identity"))?;
            Some(WorkerCodeProjectRef {
                runtime_id: runtime_id.to_string(),
                work_id: work_id.to_string(),
                repo_id: repo_id.to_string(),
            })
        }
    } else {
        if spawn.code_project_setup.is_some() {
            return Err(worker_error(
                "only remote Coder work can create a destination project",
            ));
        }
        None
    };
    Ok(PendingRemoteWorker {
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
        code_project,
        code_project_setup: spawn.code_project_setup.clone(),
        world_ids,
        expected_world_runtime_id,
        max_tool_rounds,
        tools: WorkerToolRequest { names: tool_names },
    })
}

fn compile_remote_worker_spec(
    spawn: &WorkshopSpawn,
    resolution: &ExecutionPlacementResolution,
) -> stasis::prelude::Result<WorkerSpawnSpec> {
    capture_remote_worker_spec(spawn)?.resolve(resolution)
}

#[async_trait]
impl WorkshopExecutionTarget for RemoteWorkshopExecution {
    async fn candidates(&self) -> stasis::prelude::Result<Vec<ExecutionTargetCandidate>> {
        self.service
            .authorized_targets()
            .await
            .map(|targets| targets.into_iter().map(|target| target.candidate).collect())
            .map_err(|error| stasis::domain::errors::StasisError::PortFailure(error.to_string()))
    }

    async fn ingress_default_runtime_id(&self) -> stasis::prelude::Result<Option<String>> {
        self.service
            .binding()
            .await
            .map(|binding| binding.map(|binding| binding.target.peer_device_id))
            .map_err(|error| stasis::domain::errors::StasisError::PortFailure(error.to_string()))
    }

    async fn enqueue_spawn(
        &self,
        spawn: WorkshopSpawn,
        placement: WorkshopPlacementRequest,
    ) -> stasis::prelude::Result<Option<Value>> {
        let worker = capture_remote_worker_spec(&spawn)?;
        self.service
            .enqueue_spawn(worker, placement)
            .await
            .map(Some)
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
        let target = self
            .service
            .authorized_targets()
            .await
            .map_err(|error| stasis::domain::errors::StasisError::PortFailure(error.to_string()))?
            .into_iter()
            .find(|target| target.target.peer_device_id == resolution.resolved_runtime_id)
            .map(|target| target.target)
            .ok_or_else(|| {
                stasis::domain::errors::StasisError::PortFailure(
                    ExecutionTargetResolutionError::ExactUnavailable {
                        runtime_id: resolution.resolved_runtime_id.clone(),
                    }
                    .to_string(),
                )
            })?;
        let ticket = self
            .service
            .submit_to(target, worker, parent_runtime_id, resolution)
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
    use super::*;
    use crate::workshop_contract::ExecutionResolutionReason;

    #[test]
    fn every_canonical_worker_intent_is_available_to_remote_contracts() {
        for intent in [
            "research",
            "general",
            "coder",
            "memory.context",
            "memory.avec_calibrate",
        ] {
            assert!(crate::agent_runtime::turn_worker::TurnWorkerIntent::parse(intent).is_some());
        }
    }

    fn pending_worker() -> PendingRemoteWorker {
        PendingRemoteWorker {
            schema_version: WORKER_SPAWN_SPEC_SCHEMA_VERSION,
            intent: "research".into(),
            task: "Inspect the sources".into(),
            user_ack: "I will inspect the sources".into(),
            manuscript_ids: Vec::new(),
            manuscript: None,
            stage_role: None,
            model_hint: None,
            parent: WorkerParentSpec {
                stream_turn_id: 0,
                turn_correlation_id: "turn-a".into(),
                agent_mode: Some("general".into()),
                original_user_prompt: "Compare these claims".into(),
                provider: "provider-a".into(),
                model: "model-a".into(),
                response_depth_mode: "normal".into(),
                code_work_id: None,
                bot: None,
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
            },
            code_project: None,
            code_project_setup: None,
            world_ids: vec!["world:browser:alpha".into()],
            expected_world_runtime_id: Some("runtime-a".into()),
            max_tool_rounds: 6,
            tools: WorkerToolRequest { names: vec![] },
        }
    }

    #[test]
    fn pending_remote_worker_resolves_only_on_its_selected_world_runtime() {
        let pending = pending_worker();
        let matching = ExecutionPlacementResolution::resolved(
            crate::workshop_contract::ExecutionTargetSelection::Exact {
                runtime_id: "runtime-a".into(),
            },
            "runtime-a",
            ExecutionResolutionReason::ExactTarget,
        );
        let resolved = pending.resolve(&matching).expect("matching runtime");
        assert_eq!(resolved.execution_placement, matching);
        assert_eq!(resolved.world_ids, pending.world_ids);
        assert_eq!(resolved.parent, pending.parent);

        let mismatching = ExecutionPlacementResolution::resolved(
            crate::workshop_contract::ExecutionTargetSelection::Exact {
                runtime_id: "runtime-b".into(),
            },
            "runtime-b",
            ExecutionResolutionReason::ExactTarget,
        );
        assert!(pending.resolve(&mismatching).is_err());

        let encoded = serde_json::to_vec(&pending).expect("serialize pending worker");
        let restored: PendingRemoteWorker =
            serde_json::from_slice(&encoded).expect("restore pending worker");
        assert_eq!(
            restored.expected_world_runtime_id.as_deref(),
            Some("runtime-a")
        );
        assert_eq!(
            restored.resolve(&matching).expect("restored resolution"),
            resolved
        );
    }
}
