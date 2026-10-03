//! Bounded work-scoped recovery. No source-chat turn or coordinator model is
//! required; exact native grants and claims continue to own provider effects.
use super::*;
use crate::daemon::work_units::{WorkUnitHost, local_work_unit_host};
use medousa_types::{work_coordination::*, work_unit::UserDomainRef};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkCoordinationProgress {
    DeferredBusy,
    AwaitingApproval,
    ExecutorRunning,
    ReviewerRunning,
    Closed,
}

pub(crate) fn review_result(
    plan: &WorkCoordinationPlan,
    input: &WorkReviewInput,
    receipt: &ExternalPeerAssignmentReceipt,
) -> WorkCoordinationResult {
    let mut result = WorkCoordinationResult {
        coordination_id: plan.input.coordination_id.clone(),
        outcome: WorkCoordinationOutcome::ReviewerFailed,
        receipt_id: Some(receipt.receipt_id.clone()),
        decision: None,
    };
    if receipt.binding.assignment_id != plan.reviewer_assignment_id
        || receipt.binding.channel != plan.input.channel
        || receipt.binding.owner_principal_id != plan.domain.user_id
    {
        result.outcome = WorkCoordinationOutcome::InvalidReview;
        return result;
    }
    if receipt.outcome != PeerAssignmentOutcome::Completed {
        return result;
    }
    result.outcome = WorkCoordinationOutcome::InvalidReview;
    if receipt.result.len() > 8192 {
        return result;
    }
    let Ok(decision) = serde_json::from_str::<WorkReviewDecision>(&receipt.result) else {
        return result;
    };
    if decision.reviewed != *input
        || decision.summary.trim().is_empty()
        || decision.summary.len() > 4096
        || decision.summary.contains('\0')
    {
        return result;
    }
    result.outcome = match decision.verdict {
        WorkReviewVerdict::Approved => WorkCoordinationOutcome::Approved,
        WorkReviewVerdict::ChangesRequested => WorkCoordinationOutcome::ChangesRequested,
    };
    result.decision = Some(decision);
    result
}

impl LocalPeerDispatcher {
    pub(crate) fn work_registry(&self) -> Arc<CoordinationStore> {
        self.store.clone()
    }
    async fn work_io<T: Send + 'static>(
        &self,
        f: impl FnOnce(&CoordinationStore, &WorkUnitHost, &medousa_forge::forge::Forge) -> Result<T>
        + Send
        + 'static,
    ) -> Result<T> {
        let native = self.store.clone();
        let work = local_work_unit_host()
            .ok_or_else(|| anyhow::anyhow!("work coordination host unavailable"))?;
        let forge = self.state.forge.clone();
        self.state
            .forge_execution
            .run(
                ExecutionClass::StoreIo,
                medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
                move || Ok(f(&native, &work, &forge)),
            )
            .await?
    }

    pub(crate) async fn register_work_coordination(
        &self,
        domain: UserDomainRef,
        input: WorkCoordinationInput,
    ) -> Result<WorkCoordinationPlan> {
        let owner = domain.user_id.clone();
        let snapshot = input.clone();
        let saved = self
            .work_io(move |native, _, _| {
                native.require_owner(&snapshot.channel, &owner)?;
                // Replaying saved intent remains readable after pause/expiry. It
                // cannot reissue native authority or dispatch a closed registration.
                match native.work_plan(&snapshot.channel, &snapshot.coordination_id) {
                    Ok(plan) => Ok(Some(plan)),
                    Err(error)
                        if error
                            .downcast_ref::<medousa_store::StoreRootError>()
                            .is_some_and(|e| e.is_not_found()) =>
                    {
                        Ok(None)
                    }
                    Err(error) => Err(error),
                }
            })
            .await?;
        if let Some(plan) = saved {
            if plan.domain != domain || plan.input != input {
                bail!("work coordination identity describes different intent");
            }
            self.wake.notify_one();
            return Ok(plan);
        }
        let owner = domain.user_id.clone();
        let snapshot = input.clone();
        let (executor, reviewer) = self
            .work_io(move |native, _, _| {
                native.require_owner(&snapshot.channel, &owner)?;
                Ok((
                    native.proposal(&snapshot.channel, &snapshot.executor_proposal_id)?,
                    native.proposal(&snapshot.channel, &snapshot.reviewer_proposal_id)?,
                ))
            })
            .await?;
        let principal = RequestPrincipal::worker(domain.user_id.clone());
        // Registration reuses native context visibility and exact local-runtime
        // validation without trying to dispatch a partially registered stage.
        self.hydrate(&principal, &executor.request, false).await?;
        self.hydrate(&principal, &reviewer.request, false).await?;
        let pinned_domain = domain.clone();
        let pinned_input = input.clone();
        let scope_digest = self
            .work_io(move |_, work, _| work.peer_scope_digest(&pinned_domain, &pinned_input))
            .await?;
        let plan = WorkCoordinationPlan {
            scope_digest,
            domain,
            input,
            executor_assignment_id: executor.request.assignment_id,
            reviewer_assignment_id: reviewer.request.assignment_id,
            forge_work_id: executor.request.forge_work_id,
        };
        let saved = plan.clone();
        self.work_io(move |native, work, forge| {
            let _custody = forge
                .store()
                .try_lock_item(
                    &medousa_forge::model::WorkId::parse_storage(&saved.forge_work_id)
                        .map_err(anyhow::Error::msg)?,
                )?
                .ok_or_else(|| anyhow::anyhow!("Forge work custody busy"))?;
            work.register_peer_plan(native, forge, &saved)
        })
        .await?;
        self.wake.notify_one();
        Ok(plan)
    }

    pub(crate) async fn get_work_coordination(
        &self,
        domain: UserDomainRef,
        query: WorkCoordinationQuery,
    ) -> Result<serde_json::Value> {
        self.work_io(move |native, work, forge| {
            native.require_owner(&query.channel, &domain.user_id)?;
            let plan = native.work_plan(&query.channel, &query.coordination_id)?;
            if plan.domain != domain { bail!("work coordination belongs to another domain"); }
            work.peer_read_access(forge, &plan)?;
            for id in [&plan.input.executor_proposal_id, &plan.input.reviewer_proposal_id] {
                let request = native.proposal(&plan.input.channel, id)?.request;
                if !crate::session_catalog::session_visible_to_profile(request.owner_session.session_id.as_str(), &domain.user_id)
                    || request.context.sources.iter().any(|source| !crate::session_catalog::session_visible_to_profile(source.selection.session.session_id.as_str(), &domain.user_id)) {
                    bail!("native work result source context is not visible to this owner");
                }
            }
            Ok(serde_json::json!({"plan":plan,
                "work": work.inspect_peer_plan(&plan)?,
                "executor_approval": native.proposal_decision(&native.proposal(&plan.input.channel, &plan.input.executor_proposal_id)?)?,
                "reviewer_approval": native.proposal_decision(&native.proposal(&plan.input.channel, &plan.input.reviewer_proposal_id)?)?,
                "executor_claimed":native.work_stage_claimed(&plan, true)?,
                "reviewer_claimed":native.work_stage_claimed(&plan, false)?,
                "review_input":native.work_review_input(&plan)?,
                "result":native.work_coordination_result(&plan)?,
                "executor_binding":native.peer_if_recorded(&plan.input.channel, &plan.executor_assignment_id)?,
                "reviewer_binding":native.peer_if_recorded(&plan.input.channel, &plan.reviewer_assignment_id)?,
                "executor_receipt":native.receipt_if_recorded(&plan.input.channel, &plan.executor_assignment_id)?,
                "reviewer_receipt":native.receipt_if_recorded(&plan.input.channel, &plan.reviewer_assignment_id)?}))
        }).await
    }

    async fn close_work_coordination(
        &self,
        plan: &WorkCoordinationPlan,
        result: WorkCoordinationResult,
    ) -> Result<WorkCoordinationProgress> {
        let plan = plan.clone();
        self.work_io(move |native, work, forge| {
            native.record_work_coordination_result(&plan, &result)?;
            work.project_peer_result(native, forge, &plan, &result)?;
            Ok(WorkCoordinationProgress::Closed)
        })
        .await
    }

    pub(crate) async fn resume_work_coordination(
        &self,
        plan: WorkCoordinationPlan,
    ) -> Result<WorkCoordinationProgress> {
        let snapshot = plan.clone();
        let lease = self
            .work_io(move |native, _, _| native.try_work_coordination_lease(&snapshot))
            .await?;
        let Some(_lease) = lease else {
            return Ok(WorkCoordinationProgress::DeferredBusy);
        };
        advance_work_coordination(self, &plan).await
    }

    async fn dispatch_work_stage(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<()> {
        let snapshot = plan.clone();
        let request = self
            .work_io(move |native, _, _| {
                let id = if executor {
                    &snapshot.executor_assignment_id
                } else {
                    &snapshot.reviewer_assignment_id
                };
                // A persisted binding is execution custody, even after grant expiry.
                // Never start another session to replace it after a daemon restart.
                if native
                    .peer_if_recorded(&snapshot.input.channel, id)?
                    .is_some()
                {
                    return Ok(None);
                }
                let proposal_id = if executor {
                    &snapshot.input.executor_proposal_id
                } else {
                    &snapshot.input.reviewer_proposal_id
                };
                Ok(Some(
                    native
                        .require_approved_proposal(
                            &snapshot.input.channel,
                            proposal_id,
                            &snapshot.domain.user_id,
                        )?
                        .request,
                ))
            })
            .await?;
        if let Some(request) = request {
            let binding = self
                .dispatch(
                    &RequestPrincipal::worker(plan.domain.user_id.clone()),
                    &request,
                )
                .await?;
            let proposal_id = if executor {
                &plan.input.executor_proposal_id
            } else {
                &plan.input.reviewer_proposal_id
            };
            crate::peer_coordination_mesh::record_remote_peer_completion_destination_binding_admitted(proposal_id, &binding).await?;
        }
        Ok(())
    }
}

/// Ports preserve native custody; the controller itself contains no process,
/// filesystem, or user-delivery effects. Tests use the same driver with a fake
/// provider and real native stores/Forge revision observations.
#[async_trait]
pub(crate) trait WorkCoordinationPort: Send + Sync {
    async fn result(&self, plan: &WorkCoordinationPlan) -> Result<Option<WorkCoordinationResult>>;
    async fn admit(&self, plan: &WorkCoordinationPlan) -> Result<()>;
    async fn receipt(
        &self,
        plan: &WorkCoordinationPlan,
        executor: bool,
    ) -> Result<Option<ExternalPeerAssignmentReceipt>>;
    async fn pin(&self, plan: &WorkCoordinationPlan) -> Result<WorkReviewInput>;
    async fn revision_current(
        &self,
        plan: &WorkCoordinationPlan,
        input: &WorkReviewInput,
    ) -> Result<bool>;
    async fn dispatch_stage(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<()>;
    async fn ready(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<bool>;
    async fn close(
        &self,
        plan: &WorkCoordinationPlan,
        result: WorkCoordinationResult,
    ) -> Result<WorkCoordinationProgress>;
}

#[async_trait]
impl WorkCoordinationPort for LocalPeerDispatcher {
    async fn ready(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<bool> {
        let plan = plan.clone();
        self.work_io(move |native, _, _| {
            let id = if executor {
                &plan.input.executor_proposal_id
            } else {
                &plan.input.reviewer_proposal_id
            };
            let proposal = native.proposal(&plan.input.channel, id)?;
            Ok(native
                .proposal_decision(&proposal)?
                .is_some_and(|decision| decision.approved))
        })
        .await
    }
    async fn result(&self, plan: &WorkCoordinationPlan) -> Result<Option<WorkCoordinationResult>> {
        let plan = plan.clone();
        self.work_io(move |native, _, _| native.work_coordination_result(&plan))
            .await
    }
    async fn admit(&self, plan: &WorkCoordinationPlan) -> Result<()> {
        let plan = plan.clone();
        self.work_io(move |_, work, forge| work.admit_peer_plan(forge, &plan).map(|_| ()))
            .await
    }
    async fn receipt(
        &self,
        plan: &WorkCoordinationPlan,
        executor: bool,
    ) -> Result<Option<ExternalPeerAssignmentReceipt>> {
        let proposal_id = if executor {
            plan.input.executor_proposal_id.clone()
        } else {
            plan.input.reviewer_proposal_id.clone()
        };
        let plan = plan.clone();
        let receipt = self
            .work_io(move |native, _, _| {
                native.receipt_if_recorded(
                    &plan.input.channel,
                    if executor {
                        &plan.executor_assignment_id
                    } else {
                        &plan.reviewer_assignment_id
                    },
                )
            })
            .await?;
        if let Some(receipt) = &receipt {
            crate::peer_coordination_mesh::record_remote_peer_completion_destination_binding_admitted(&proposal_id, &receipt.binding).await?;
        }
        Ok(receipt)
    }
    async fn pin(&self, plan: &WorkCoordinationPlan) -> Result<WorkReviewInput> {
        let plan = plan.clone();
        self.work_io(move |native, work, forge| {
            if let Some(input) = native.work_review_input(&plan)? {
                Ok(input)
            } else {
                work.pin_peer_output(native, forge, &plan)
            }
        })
        .await
    }
    async fn revision_current(
        &self,
        plan: &WorkCoordinationPlan,
        input: &WorkReviewInput,
    ) -> Result<bool> {
        let plan = plan.clone();
        let input = input.clone();
        self.work_io(move |_, work, forge| work.peer_revision_current(forge, &plan, &input))
            .await
    }
    async fn dispatch_stage(&self, plan: &WorkCoordinationPlan, executor: bool) -> Result<()> {
        self.dispatch_work_stage(plan, executor).await
    }
    async fn close(
        &self,
        plan: &WorkCoordinationPlan,
        result: WorkCoordinationResult,
    ) -> Result<WorkCoordinationProgress> {
        self.close_work_coordination(plan, result).await
    }
}

pub(crate) async fn advance_work_coordination(
    port: &dyn WorkCoordinationPort,
    plan: &WorkCoordinationPlan,
) -> Result<WorkCoordinationProgress> {
    if let Some(result) = port.result(plan).await? {
        return port.close(plan, result).await;
    }
    port.admit(plan).await?;
    let failure = |outcome, receipt_id| WorkCoordinationResult {
        coordination_id: plan.input.coordination_id.clone(),
        outcome,
        receipt_id,
        decision: None,
    };
    if chrono::Utc::now() >= plan.input.deadline {
        return port
            .close(
                plan,
                failure(WorkCoordinationOutcome::DeadlineExceeded, None),
            )
            .await;
    }
    let Some(receipt) = port.receipt(plan, true).await? else {
        if !port.ready(plan, true).await? {
            return Ok(WorkCoordinationProgress::AwaitingApproval);
        }
        port.dispatch_stage(plan, true).await?;
        return Ok(WorkCoordinationProgress::ExecutorRunning);
    };
    if receipt.outcome != PeerAssignmentOutcome::Completed {
        return port
            .close(
                plan,
                failure(
                    WorkCoordinationOutcome::ExecutorFailed,
                    Some(receipt.receipt_id),
                ),
            )
            .await;
    }
    let input = port.pin(plan).await?;
    if !port.revision_current(plan, &input).await? {
        return port
            .close(
                plan,
                failure(WorkCoordinationOutcome::RevisionChanged, None),
            )
            .await;
    }
    let Some(receipt) = port.receipt(plan, false).await? else {
        if !port.ready(plan, false).await? {
            return Ok(WorkCoordinationProgress::AwaitingApproval);
        }
        port.dispatch_stage(plan, false).await?;
        return Ok(WorkCoordinationProgress::ReviewerRunning);
    };
    port.close(plan, review_result(plan, &input, &receipt))
        .await
}

#[cfg(test)]
mod tests;
