//! Work admission and revision custody for the native peer controller.
use super::*;
use medousa_acp_client::coordination::store::{CoordinationStore, work::WORK_REVIEW_CONTRACT};
use medousa_forge::{
    forge::Forge,
    model::{WorkId, WorkItem},
};
use medousa_types::{coordination::*, work_coordination::*};
use sha2::Digest;

fn item(forge: &Forge, plan: &WorkCoordinationPlan) -> Result<WorkItem> {
    let item =
        forge.load(&WorkId::parse_storage(&plan.forge_work_id).map_err(anyhow::Error::msg)?)?;
    if item.owner != plan.domain.user_id || item.id.to_string() != plan.forge_work_id {
        bail!("work coordination does not own the native Forge work");
    }
    Ok(item)
}

fn checkout(
    forge: &Forge,
    plan: &WorkCoordinationPlan,
    receipt_id: String,
) -> Result<WorkReviewInput> {
    let item = item(forge, plan)?;
    let env = item
        .workspace_environment()
        .ok_or_else(|| anyhow::anyhow!("review requires an exact governed Forge checkout"))?;
    let _files = medousa_store::StoreRoot::open_nofollow(&env.worktree)?;
    if forge.git().repo_identity(&env.worktree)?.common_dir != env.repo.common_dir
        || forge.git().current_branch(&env.worktree)?.as_deref() != Some(env.branch.as_str())
        || !forge.git().is_clean(&env.worktree)?
    {
        bail!("review requires a clean checkout on its governed branch");
    }
    Ok(WorkReviewInput {
        coordination_id: plan.input.coordination_id.clone(),
        work_unit_id: plan.input.work_unit_id.clone(),
        executor_assignment_id: plan.executor_assignment_id.clone(),
        executor_receipt_id: receipt_id,
        forge_work_id: plan.forge_work_id.clone(),
        environment_generation: env.generation,
        branch: env.branch.clone(),
        head_oid: forge.git().head_oid(&env.worktree)?.to_string(),
    })
}

fn revision_matches(
    forge: &Forge,
    plan: &WorkCoordinationPlan,
    input: &WorkReviewInput,
) -> Result<bool> {
    let item = item(forge, plan)?;
    let Some(env) = item.workspace_environment() else {
        return Ok(false);
    };
    let _files = medousa_store::StoreRoot::open_nofollow(&env.worktree)?;
    Ok(env.generation == input.environment_generation
        && env.branch == input.branch
        && forge.git().repo_identity(&env.worktree)?.common_dir == env.repo.common_dir
        && forge.git().current_branch(&env.worktree)?.as_deref() == Some(env.branch.as_str())
        && forge.git().is_clean(&env.worktree)?
        && forge.git().head_oid(&env.worktree)?.as_str() == input.head_oid)
}

impl WorkUnitHost {
    pub(crate) fn peer_read_access(
        &self,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
    ) -> Result<()> {
        item(forge, plan)?;
        Ok(())
    }
    pub(crate) fn register_peer_plan(
        &self,
        native: &CoordinationStore,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
    ) -> Result<()> {
        let _scope = self.store.peer_coordination_custody(
            &plan.domain,
            &plan.input.work_unit_id,
            plan.input.expected_scope_revision,
            &plan.scope_digest,
        )?;
        self.admit_peer_plan(forge, plan)?;
        native.register_work_plan(plan)?;
        Ok(())
    }
    pub(crate) fn inspect_peer_plan(
        &self,
        plan: &WorkCoordinationPlan,
    ) -> Result<serde_json::Value> {
        let unit = self
            .store
            .work_unit(&plan.domain, &plan.input.work_unit_id)?;
        Ok(
            serde_json::json!({"state":unit.state,"state_reason":unit.state_reason,
            "scope_current":unit.scope_revision == plan.input.expected_scope_revision && self.store.peer_coordination_scope_digest(&plan.domain, &plan.input.work_unit_id)? == plan.scope_digest}),
        )
    }
    pub(crate) fn peer_scope_digest(
        &self,
        domain: &UserDomainRef,
        input: &WorkCoordinationInput,
    ) -> Result<String> {
        self.store.admit_peer_coordination(
            domain,
            &input.work_unit_id,
            input.expected_scope_revision,
        )?;
        Ok(self
            .store
            .peer_coordination_scope_digest(domain, &input.work_unit_id)?)
    }
    pub async fn coordinate(
        &self,
        turn: &TurnExecutionContext,
        input: WorkCoordinationInput,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        let host = crate::daemon::coordination::local_coordination_host()
            .ok_or_else(|| anyhow::anyhow!("native coordination unavailable"))?;
        bounded_response(serde_json::to_value(
            host.register_work_coordination(domain, input).await?,
        )?)
    }

    pub async fn coordination(
        &self,
        turn: &TurnExecutionContext,
        input: WorkCoordinationQuery,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, false)?;
        let host = crate::daemon::coordination::local_coordination_host()
            .ok_or_else(|| anyhow::anyhow!("native coordination unavailable"))?;
        bounded_response(host.get_work_coordination(domain, input).await?)
    }

    pub(crate) fn admit_peer_plan(
        &self,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
    ) -> Result<WorkUnit> {
        if plan.domain.authority_id
            != *crate::workshop_authority::current().map_err(anyhow::Error::msg)?
        {
            bail!("work coordination belongs to another workshop");
        }
        item(forge, plan)?;
        if self.peer_scope_digest(&plan.domain, &plan.input)? != plan.scope_digest {
            bail!("work scope resource revisions changed");
        }
        Ok(self.store.admit_peer_coordination(
            &plan.domain,
            &plan.input.work_unit_id,
            plan.input.expected_scope_revision,
        )?)
    }

    /// Rechecked by native hydrate before startup and after provider handshake,
    /// including direct dispatch calls outside the recovery controller.
    pub(crate) fn peer_stage_context(
        &self,
        native: &CoordinationStore,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
        request: &ExternalPeerAssignmentRequest,
    ) -> Result<Option<String>> {
        let _custody = forge
            .store()
            .try_lock_item(
                &WorkId::parse_storage(&plan.forge_work_id).map_err(anyhow::Error::msg)?,
            )?
            .ok_or_else(|| anyhow::anyhow!("Forge work custody busy"))?;
        let unit = self.admit_peer_plan(forge, plan)?;
        if chrono::Utc::now() >= plan.input.deadline
            || native.work_coordination_result(plan)?.is_some()
        {
            bail!("work coordination is closed or expired");
        }
        let executor = request.assignment_id == plan.executor_assignment_id;
        let proposal_id = if executor {
            &plan.input.executor_proposal_id
        } else {
            &plan.input.reviewer_proposal_id
        };
        let proposal = native.require_approved_proposal(
            &request.channel,
            proposal_id,
            &plan.domain.user_id,
        )?;
        if proposal.request != *request {
            bail!("work stage differs from its approved proposal");
        }
        if executor {
            return Ok(None);
        }
        let input = native
            .work_review_input(plan)?
            .ok_or_else(|| anyhow::anyhow!("review dependency has not been durably pinned"))?;
        if !revision_matches(forge, plan, &input)? {
            bail!("review checkout changed after executor completion");
        }
        let receipt = native.receipt(&request.channel, &plan.executor_assignment_id)?;
        if receipt.outcome != PeerAssignmentOutcome::Completed
            || receipt.receipt_id != input.executor_receipt_id
        {
            bail!("review requires the exact completed executor receipt");
        }
        // The preapproved protocol opts into these derived data fields. No
        // request instructions, context digests, or execution grants change.
        Ok(Some(serde_json::to_string(&serde_json::json!({
            "protocol": WORK_REVIEW_CONTRACT,
            "intent": unit.intent,
            "completion_condition": unit.completion_condition,
            "scope": unit.scope,
            "reviewed": input,
            "executor_result": receipt.result,
            "response_contract": "Return only JSON: {reviewed: the exact supplied reviewed object, verdict: approved or changes_requested, summary: nonempty text up to 4096 bytes}. Review the pinned committed revision; do not edit the checkout."
        }))?))
    }

    pub(crate) fn pin_peer_output(
        &self,
        native: &CoordinationStore,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
    ) -> Result<WorkReviewInput> {
        let _custody = forge
            .store()
            .try_lock_item(
                &WorkId::parse_storage(&plan.forge_work_id).map_err(anyhow::Error::msg)?,
            )?
            .ok_or_else(|| anyhow::anyhow!("Forge work custody busy"))?;
        self.admit_peer_plan(forge, plan)?;
        let receipt = native.receipt(&plan.input.channel, &plan.executor_assignment_id)?;
        let input = checkout(forge, plan, receipt.receipt_id)?;
        native.record_work_review_input(plan, &input)?;
        Ok(input)
    }

    pub(crate) fn peer_revision_current(
        &self,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
        input: &WorkReviewInput,
    ) -> Result<bool> {
        let _custody = forge
            .store()
            .try_lock_item(
                &WorkId::parse_storage(&plan.forge_work_id).map_err(anyhow::Error::msg)?,
            )?
            .ok_or_else(|| anyhow::anyhow!("Forge work custody busy"))?;
        self.admit_peer_plan(forge, plan)?;
        revision_matches(forge, plan, input)
    }

    pub(crate) fn project_peer_result(
        &self,
        native: &CoordinationStore,
        forge: &Forge,
        plan: &WorkCoordinationPlan,
        result: &WorkCoordinationResult,
    ) -> Result<()> {
        let _custody = forge
            .store()
            .try_lock_item(
                &WorkId::parse_storage(&plan.forge_work_id).map_err(anyhow::Error::msg)?,
            )?
            .ok_or_else(|| anyhow::anyhow!("Forge work custody busy"))?;
        let current = self
            .store
            .work_unit(&plan.domain, &plan.input.work_unit_id)?;
        if current.state.is_terminal()
            || current.scope_revision != plan.input.expected_scope_revision
        {
            // A completed registration never revives cancelled or superseded work.
            native.record_work_projection(plan)?;
            return Ok(());
        }
        let mut approved = result.outcome == WorkCoordinationOutcome::Approved;
        if approved {
            let input = native
                .work_review_input(plan)?
                .ok_or_else(|| anyhow::anyhow!("approved review input missing"))?;
            approved = revision_matches(forge, plan, &input)?;
        }
        let evidence: Vec<_> = [&plan.executor_assignment_id, &plan.reviewer_assignment_id]
            .into_iter()
            .map(|id| ResourceRef {
                authority_id: plan.domain.authority_id.clone(),
                kind: ResourceKind::Assignment,
                id: id.clone(),
            })
            .collect();
        if current.state == WorkUnitState::NeedsAttention
            && current.state_evidence == evidence
            && current.provenance.source == RecordSource::SystemEvent
        {
            native.record_work_projection(plan)?;
            return Ok(());
        }
        self.admit_peer_plan(forge, plan)?;
        for (index, reference) in evidence.iter().enumerate() {
            let id = if index == 0 {
                &plan.input.executor_proposal_id
            } else {
                &plan.input.reviewer_proposal_id
            };
            let proposal = native.proposal(&plan.input.channel, id)?;
            let receipt = native.receipt_if_recorded(&plan.input.channel, &reference.id)?;
            native_graph::publish(&self.store, &plan.domain, reference.clone(), serde_json::json!({
                "source":"coordination_assignment", "assignment_id":reference.id, "channel":plan.input.channel,
                "forge_work_id":plan.forge_work_id, "execution_session":proposal.request.execution_session,
                "coordination_id":plan.input.coordination_id, "work_unit_id":plan.input.work_unit_id,
                "receipt_id":receipt.as_ref().map(|receipt| &receipt.receipt_id),
                "execution_outcome":receipt.as_ref().map(|receipt| receipt.outcome),
                "coverage":"native_request_and_receipt_metadata"
            }).to_string(), format!("peer-assignment-v1:{:x}", sha2::Sha256::digest(serde_json::to_vec(&(proposal, receipt, result))?)),
                ResourceResolution::Available, "adapter:peer-work")?;
        }
        for _ in 0..3 {
            self.admit_peer_plan(forge, plan)?;
            let revision = self
                .store
                .query(&plan.domain, WorkGraphQuery::default())?
                .revision;
            let state = if approved {
                WorkUnitState::Satisfied
            } else {
                WorkUnitState::NeedsAttention
            };
            let mutation = WorkGraphMutation::SetState {
                work_unit_id: plan.input.work_unit_id.clone(),
                state,
                reason: format!(
                    "Native execute/review: {:?}{}",
                    result.outcome,
                    if result.outcome == WorkCoordinationOutcome::Approved && !approved {
                        "; revision changed before publication"
                    } else {
                        ""
                    }
                ),
                evidence: evidence.clone(),
            };
            let command = WorkGraphCommand {
                command_id: format!(
                    "peer-work:{:x}",
                    sha2::Sha256::digest(serde_json::to_vec(&(&plan, &mutation, revision))?)
                ),
                expected_revision: revision,
                mutation,
            };
            let provenance = RecordProvenance {
                actor_id: "adapter:peer-work".into(),
                source: RecordSource::SystemEvent,
                evidence: evidence.clone(),
            };
            match self
                .store
                .apply_checked(&plan.domain, command, provenance, |_| {
                    self.store
                        .admit_peer_coordination(
                            &plan.domain,
                            &plan.input.work_unit_id,
                            plan.input.expected_scope_revision,
                        )
                        .and_then(|_| {
                            if self.store.peer_coordination_scope_digest(
                                &plan.domain,
                                &plan.input.work_unit_id,
                            )? != plan.scope_digest
                            {
                                return Err(medousa_store::PersistenceError::new(
                                    medousa_store::PersistenceErrorKind::Conflict,
                                    "work resource revisions changed before result publication",
                                ));
                            }
                            Ok(())
                        })
                }) {
                Ok(_) => {
                    native.record_work_projection(plan)?;
                    return Ok(());
                }
                Err(error) if error.kind == medousa_store::PersistenceErrorKind::Conflict => {
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
        }
        bail!("work result publication conflicted; durable native result retained")
    }
}

#[cfg(test)]
mod tests;
