//! Admit or observe an authenticated Stasis grant through the existing worker.

use std::sync::Arc;

use async_trait::async_trait;
use medousa_types::session::{ExecutionId, ExecutionRef};
use serde_json::json;
use stasis::domain::agent::envelope::{
    AGENT_ENVELOPE_SCHEMA_VERSION_V1, AgentEnvelope, AgentEnvelopeKind,
};
use stasis::domain::runtime::job::JobState;
use stasis::prelude::RuntimeComposition;

use crate::agent_runtime::turn_context::{TurnScratchpad, WorkerHandoffCapsule};
use crate::agent_runtime::turn_worker::{
    DelegatedWorkAdmissionError, TurnWorkRecord, TurnWorkStatus, turn_worker_store,
};
use crate::delegated_task::{
    DELEGATED_TASK_SCHEMA_VERSION, DelegatedTaskAdmission, DelegatedTaskError,
    DelegatedTaskObservation, DelegatedTaskRequest, DelegatedTaskResult, DelegatedTaskStatus,
    WorkerManuscriptSpec, WorkerRouteProvenance, delegated_context_prompt, delegated_work_id,
    materialize_delegated_context, validate_task_request, worker_spec_digest,
};
use crate::pairing::PairedDeviceRecord;
use crate::peer_execution_policy::TaskExecutionGrant;
use crate::runtime_composition_ext::RuntimeCompositionExt;

#[async_trait]
pub trait DelegatedTaskExecutor: Send + Sync {
    async fn submit_or_observe(
        &self,
        sender: &PairedDeviceRecord,
        request: &DelegatedTaskRequest,
        task_execution_grant: &TaskExecutionGrant,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError>;
}

/// Production adapter: context enters the canonical session store, execution
/// enters `workflow.medousa.turn_worker`, and terminal identity stays Stasis.
pub struct DaemonDelegatedTaskExecutor {
    runtime: Arc<RuntimeComposition>,
    local_device_id: String,
    provider: String,
    model: String,
    response_depth_mode: String,
    max_tool_rounds: usize,
}

impl DaemonDelegatedTaskExecutor {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        local_device_id: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
        response_depth_mode: impl Into<String>,
        max_tool_rounds: usize,
    ) -> Self {
        Self {
            runtime,
            local_device_id: local_device_id.into(),
            provider: provider.into(),
            model: model.into(),
            response_depth_mode: response_depth_mode.into(),
            max_tool_rounds: max_tool_rounds.max(1),
        }
    }

    fn task_prompt(request: &DelegatedTaskRequest) -> Result<String, DelegatedTaskError> {
        request
            .grant
            .payload
            .get("user_prompt")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| DelegatedTaskError::invalid("delegated task prompt is required"))
    }

    fn manuscript_handoff(
        request: &DelegatedTaskRequest,
    ) -> Result<Option<crate::identity_manuscript::WorkerManuscriptHandoff>, DelegatedTaskError>
    {
        let Some(worker) = request.worker.as_ref() else {
            return Ok(None);
        };
        if let Some(manuscript) = worker.manuscript.as_ref() {
            return Ok(Some(worker_manuscript_handoff(manuscript)));
        }
        let Some(manuscript_id) = worker.manuscript_ids.first() else {
            return Ok(None);
        };
        let context = crate::identity_manuscript::build_manuscript_context(manuscript_id)
            .map_err(|error| DelegatedTaskError::conflict(error.to_string()))?;
        Ok(Some((&context).into()))
    }

    fn route_provenance(record: &TurnWorkRecord) -> WorkerRouteProvenance {
        WorkerRouteProvenance {
            provider: record.provider.clone(),
            model: record.model.clone(),
            response_depth_mode: record.response_depth_mode.clone(),
            stage_role: record.stage_role.clone(),
            model_hint: record.model_hint.clone(),
        }
    }

    async fn ensure_worker_job(
        &self,
        work_id: &str,
        created: bool,
    ) -> Result<(), DelegatedTaskError> {
        let existing = self
            .runtime
            .get_job(work_id)
            .await
            .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
        let needs_enqueue = created
            || existing.as_ref().is_none_or(|job| {
                matches!(
                    job.state,
                    JobState::Succeeded
                        | JobState::Failed
                        | JobState::DeadLetter
                        | JobState::Canceled
                )
            });
        let worker = turn_worker_store().get(work_id);
        let terminal = worker.as_ref().is_some_and(|record| {
            matches!(
                record.status,
                TurnWorkStatus::Completed | TurnWorkStatus::Failed | TurnWorkStatus::Cancelled
            )
        });
        if needs_enqueue && !terminal {
            crate::agent_runtime::turn_worker_job::enqueue_turn_worker_job(
                self.runtime.as_ref(),
                work_id,
                0,
            )
            .await
            .map_err(|error| DelegatedTaskError::internal(error.to_string()))?;
        }
        Ok(())
    }

    fn terminal_result(
        &self,
        request: &DelegatedTaskRequest,
        terminal_record: &TurnWorkRecord,
        execution: &ExecutionRef,
        derivation: &medousa_types::session::SessionDerivation,
    ) -> DelegatedTaskResult {
        let (kind, payload) = match terminal_record.status {
            TurnWorkStatus::Completed => (
                AgentEnvelopeKind::TurnCompleted,
                json!({
                    "text": terminal_record.result_text.clone(),
                    "tool_names": terminal_record.tool_names.clone(),
                    "termination_reason": terminal_record.termination_reason.clone(),
                    "execution": execution,
                    "derivation": derivation,
                    "parent_runtime_id": terminal_record.parent_runtime_id,
                    "execution_placement": terminal_record.execution_placement,
                }),
            ),
            TurnWorkStatus::Cancelled => (
                AgentEnvelopeKind::Cancelled,
                json!({
                    "error": terminal_record.error.clone().unwrap_or_else(|| "delegated worker cancelled".to_string()),
                    "execution": execution,
                    "derivation": derivation,
                    "parent_runtime_id": terminal_record.parent_runtime_id,
                    "execution_placement": terminal_record.execution_placement,
                }),
            ),
            TurnWorkStatus::Failed => (
                AgentEnvelopeKind::Failed,
                json!({
                    "error": terminal_record.error.clone().unwrap_or_else(|| "delegated worker failed".to_string()),
                    "execution": execution,
                    "derivation": derivation,
                    "parent_runtime_id": terminal_record.parent_runtime_id,
                    "execution_placement": terminal_record.execution_placement,
                }),
            ),
            TurnWorkStatus::Pending | TurnWorkStatus::Running => unreachable!("terminal wait"),
        };
        DelegatedTaskResult {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            terminal: AgentEnvelope {
                schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
                kind,
                envelope_id: format!("result-{}", terminal_record.work_id),
                session_id: request.grant.session_id.clone(),
                thread_id: request.grant.thread_id.clone(),
                turn_id: request.grant.turn_id.clone(),
                job_id: request.grant.job_id.clone(),
                correlation_id: request.grant.correlation_id.clone(),
                causation_id: request.grant.envelope_id.clone(),
                participant_id: Some(self.local_device_id.clone()),
                occurred_at: terminal_record.updated_at,
                payload,
            },
            execution: execution.clone(),
            parent_runtime_id: terminal_record.parent_runtime_id.clone(),
            execution_placement: terminal_record.execution_placement.clone(),
            task_execution_grant: terminal_record.task_execution_grant.clone(),
            worker_spec_digest: request
                .worker
                .as_ref()
                .and_then(|worker| worker_spec_digest(worker).ok()),
            worker_route: request
                .worker
                .as_ref()
                .map(|_| Self::route_provenance(terminal_record)),
            derivation: derivation.clone(),
        }
    }
}

#[async_trait]
impl DelegatedTaskExecutor for DaemonDelegatedTaskExecutor {
    async fn submit_or_observe(
        &self,
        sender: &PairedDeviceRecord,
        request: &DelegatedTaskRequest,
        task_execution_grant: &TaskExecutionGrant,
    ) -> Result<DelegatedTaskObservation, DelegatedTaskError> {
        validate_task_request(request)?;
        if request.execution_placement.resolution_reason
            != crate::workshop_contract::ExecutionResolutionReason::LegacyUnknown
            && request.execution_placement.resolved_runtime_id != self.local_device_id
        {
            return Err(DelegatedTaskError::conflict(format!(
                "delegated work resolved for runtime '{}' but reached '{}'",
                request.execution_placement.resolved_runtime_id, self.local_device_id
            )));
        }
        let target_authority = crate::workshop_authority::current()
            .map_err(DelegatedTaskError::internal)?
            .clone();
        let store = crate::session_store::get_session_store();
        let materialized = materialize_delegated_context(
            store.as_ref(),
            &target_authority,
            &sender.phone_id,
            request,
        )
        .await?;
        let target_session = materialized.derivation.target_session.session_id.clone();
        // The derived session remains internal to this worker execution. It is
        // not projected into the receiving workshop's visible session catalog.
        let profile_id = format!("peer:{}", sender.phone_id.trim());
        let turn_id = request
            .grant
            .turn_id
            .as_deref()
            .expect("validated delegated turn id");
        let work_id = delegated_work_id(&sender.phone_id, turn_id);
        let task_prompt = Self::task_prompt(request)?;
        let manuscript = Self::manuscript_handoff(request)?;
        let worker_intent = request
            .worker
            .as_ref()
            .map(|worker| worker.intent.as_str())
            .unwrap_or("research");
        let parsed_intent = crate::agent_runtime::turn_worker::TurnWorkerIntent::parse(
            worker_intent,
        )
        .ok_or_else(|| DelegatedTaskError::invalid("delegated worker intent is unsupported"))?;
        let stage_role = request
            .worker
            .as_ref()
            .and_then(|worker| worker.stage_role.clone());
        let model_hint = request
            .worker
            .as_ref()
            .and_then(|worker| worker.model_hint.clone());
        let (provider, model) = crate::agent_runtime::turn_worker::resolve_worker_llm_target(
            &self.provider,
            &self.model,
            parsed_intent,
            stage_role.as_deref(),
            model_hint.as_deref(),
        );
        let response_depth_mode = request
            .worker
            .as_ref()
            .map(|worker| worker.parent.response_depth_mode.clone())
            .unwrap_or_else(|| self.response_depth_mode.clone());
        let max_tool_rounds = request
            .worker
            .as_ref()
            .map(|worker| worker.max_tool_rounds)
            .unwrap_or(self.max_tool_rounds)
            .max(1);
        let context_prompt = delegated_context_prompt(&request.context);
        let mut handoff = WorkerHandoffCapsule::from_host_context(
            target_session.as_str(),
            request
                .worker
                .as_ref()
                .map(|worker| worker.parent.stream_turn_id)
                .unwrap_or(0),
            Some(request.grant.correlation_id.clone()),
            request
                .worker
                .as_ref()
                .map(|worker| worker.parent.original_user_prompt.as_str())
                .filter(|prompt| !prompt.is_empty())
                .unwrap_or(&context_prompt),
            &TurnScratchpad::default(),
            None,
            None,
            None,
        );
        // `from_host_context` protects ordinary host prompts with a smaller
        // cap. This context was already bounded and digest-checked as a grant.
        handoff.parent_user_prompt = context_prompt;
        handoff.manuscript = manuscript;
        handoff.bot_profile_appendix = request
            .worker
            .as_ref()
            .and_then(|worker| worker.parent.bot.as_ref())
            .map(|bot| bot.prompt_appendix.clone());
        handoff.apply_spawn(worker_intent, &task_prompt, &work_id);
        let mut record = TurnWorkRecord::delegated(
            work_id.clone(),
            target_session.to_string(),
            profile_id.clone(),
            request.grant.correlation_id.clone(),
            task_prompt,
            provider,
            model,
            response_depth_mode,
            max_tool_rounds,
            handoff,
            request.parent_runtime_id.clone(),
            request.execution_placement.clone(),
            task_execution_grant.clone(),
        );
        record.worker_spawn_spec = request.worker.clone();
        record.intent = worker_intent.to_string();
        record.user_ack = request
            .worker
            .as_ref()
            .map(|worker| worker.user_ack.clone())
            .unwrap_or_default();
        record.parent_stream_turn_id = request
            .worker
            .as_ref()
            .map(|worker| worker.parent.stream_turn_id)
            .unwrap_or(0);
        record.parent_user_prompt = request
            .worker
            .as_ref()
            .map(|worker| worker.parent.original_user_prompt.clone());
        record.parent_agent_mode = request
            .worker
            .as_ref()
            .and_then(|worker| worker.parent.agent_mode.clone());
        record.parent_code_work_id = request.worker.as_ref().and_then(|worker| {
            worker
                .code_project
                .as_ref()
                .map(|project| project.work_id.clone())
                .or_else(|| worker.parent.code_work_id.clone())
        });
        record.stage_role = stage_role;
        record.model_hint = model_hint;
        record.manuscript_id = request
            .worker
            .as_ref()
            .and_then(|worker| worker.manuscript_ids.first().cloned());
        record.supports_ui_artifacts = request
            .worker
            .as_ref()
            .is_some_and(|worker| worker.parent.supports_ui_artifacts);
        record.supports_liquid_markdown = request
            .worker
            .as_ref()
            .is_some_and(|worker| worker.parent.supports_liquid_markdown);
        record.supports_browser_host = request
            .worker
            .as_ref()
            .is_some_and(|worker| worker.parent.supports_browser_host);
        let created =
            turn_worker_store()
                .try_insert_delegated(record)
                .map_err(|error| match error {
                    DelegatedWorkAdmissionError::SessionDeleting => {
                        DelegatedTaskError::conflict("delegated worker session is being deleted")
                    }
                    DelegatedWorkAdmissionError::ConflictingIdentity => {
                        DelegatedTaskError::conflict(
                            "delegated Stasis turn identity was already used for different work",
                        )
                    }
                })?;
        self.ensure_worker_job(&work_id, created).await?;
        let current = turn_worker_store()
            .get(&work_id)
            .ok_or_else(|| DelegatedTaskError::internal("delegated worker record disappeared"))?;
        if current.identity_user_id.as_deref() != Some(profile_id.as_str()) {
            return Err(DelegatedTaskError::conflict(
                "delegated worker does not belong to the authenticated peer",
            ));
        }
        let execution = ExecutionRef {
            authority_id: target_authority,
            session_id: target_session,
            execution_id: ExecutionId::parse(&work_id)
                .map_err(|error| DelegatedTaskError::internal(error.to_string()))?,
        };
        let status = match current.status {
            TurnWorkStatus::Pending => DelegatedTaskStatus::Pending,
            TurnWorkStatus::Running => DelegatedTaskStatus::Running,
            TurnWorkStatus::Completed => DelegatedTaskStatus::Completed,
            TurnWorkStatus::Failed => DelegatedTaskStatus::Failed,
            TurnWorkStatus::Cancelled => DelegatedTaskStatus::Cancelled,
        };
        let result = status
            .is_terminal()
            .then(|| self.terminal_result(request, &current, &execution, &materialized.derivation));
        if result.is_some() {
            turn_worker_store().update(&work_id, |record| {
                record.synthesis_delivered = true;
            });
        }
        Ok(DelegatedTaskObservation {
            schema_version: DELEGATED_TASK_SCHEMA_VERSION,
            work_id,
            admission: if created {
                DelegatedTaskAdmission::Accepted
            } else {
                DelegatedTaskAdmission::Existing
            },
            status,
            execution,
            parent_runtime_id: current.parent_runtime_id.clone(),
            execution_placement: current.execution_placement.clone(),
            task_execution_grant: current.task_execution_grant.clone(),
            worker_spec_digest: request
                .worker
                .as_ref()
                .and_then(|worker| worker_spec_digest(worker).ok()),
            worker_route: request
                .worker
                .as_ref()
                .map(|_| Self::route_provenance(&current)),
            derivation: materialized.derivation,
            result,
        })
    }
}

fn worker_manuscript_handoff(
    manuscript: &WorkerManuscriptSpec,
) -> crate::identity_manuscript::WorkerManuscriptHandoff {
    crate::identity_manuscript::WorkerManuscriptHandoff {
        id: manuscript.id.clone(),
        name: manuscript.name.clone(),
        worker_intent: manuscript.worker_intent.clone(),
        stage_role: manuscript.stage_role.clone(),
        model_hint: manuscript.model_hint.clone(),
        voice_appendix: manuscript.voice_appendix.clone(),
        system_appendix: manuscript.system_appendix.clone(),
        tools_allow: manuscript.tools_allow.clone(),
        openshell_enabled: manuscript.openshell_enabled,
        openshell_policy_template: manuscript.openshell_policy_template.clone(),
        openshell_sandbox_from: manuscript.openshell_sandbox_from.clone(),
    }
}
