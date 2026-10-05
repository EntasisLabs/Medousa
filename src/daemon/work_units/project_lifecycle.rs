//! Native undertaking lifecycle for an admitted owner Assistant, including
//! completion callbacks. No operator credential or ambient profile is borrowed.
use super::*;
use medousa_forge::{
    lifecycle,
    model::{ActorKind, ActorRef, ReviewDecisionId, WorkItem},
};
use medousa_types::forge::*;

pub enum ProjectLifecycleMutation {
    PrepareMerge(ProjectPrepareMergeInput),
    Approve(ProjectApproveInput),
    Apply(ProjectApplyInput),
    RequestChanges(ProjectRequestChangesInput),
    Discard(ProjectDiscardInput),
}

fn require_owner_lifecycle(principal: &RequestPrincipal) -> Result<()> {
    if !principal
        .capabilities()
        .contains(Capability::WorkspaceWrite)
        || !matches!(
            principal.kind(),
            PrincipalKind::LocalApp
                | PrincipalKind::Root
                | PrincipalKind::Portal
                | PrincipalKind::Continuation
        )
    {
        bail!(
            "undertaking lifecycle changes require the owning Assistant, not a delegated executor"
        );
    }
    Ok(())
}

impl WorkUnitHost {
    pub async fn project_review(
        &self,
        turn: &TurnExecutionContext,
        input: ProjectReviewQuery,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, false)?;
        let forge = self.forge.clone();
        self.execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok(bounded_response(lifecycle::review(
                    &forge,
                    &domain.user_id,
                    input,
                )?))
            })
            .await?
    }

    pub async fn project_review_file(
        &self,
        turn: &TurnExecutionContext,
        input: ProjectReviewFileQuery,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, false)?;
        let forge = self.forge.clone();
        self.execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok(bounded_response(lifecycle::review_file(
                    &forge,
                    &domain.user_id,
                    input,
                )?))
            })
            .await?
    }

    pub async fn mutate_project(
        &self,
        turn: &TurnExecutionContext,
        input: ProjectLifecycleMutation,
    ) -> Result<serde_json::Value> {
        let domain = admitted_domain(turn, true)?;
        require_owner_lifecycle(turn.principal())?;
        let actor = ActorRef {
            kind: ActorKind::Profile,
            id: format!(
                "{}:session:{}:turn:{}",
                domain.user_id,
                turn.session_id(),
                turn.turn_id()
            ),
        };
        let forge = self.forge.clone();
        let owner = domain.user_id.clone();
        let cancellation = turn.cancellation().clone();
        let (class, retained_bytes) = match &input {
            ProjectLifecycleMutation::PrepareMerge(_) => {
                (ExecutionClass::WorkEnvironment, 8 * 1024 * 1024)
            }
            ProjectLifecycleMutation::Apply(_) | ProjectLifecycleMutation::Discard(_) => {
                (ExecutionClass::LocalMutation, 256 * 1024)
            }
            ProjectLifecycleMutation::Approve(_) | ProjectLifecycleMutation::RequestChanges(_) => {
                (ExecutionClass::StoreIo, 256 * 1024)
            }
        };
        let (item, decision_id): (WorkItem, Option<ReviewDecisionId>) = self
            .execution
            .run(class, retained_bytes, move || {
                Ok((|| -> Result<_> {
                    if cancellation.is_cancelled() {
                        bail!("owner turn was cancelled before lifecycle admission");
                    }
                    Ok(match input {
                        ProjectLifecycleMutation::PrepareMerge(input) => (
                            lifecycle::prepare_merge(&forge, &owner, &actor, input)?,
                            None,
                        ),
                        ProjectLifecycleMutation::Approve(input) => {
                            let (item, id) = lifecycle::approve(&forge, &owner, &actor, input)?;
                            (item, Some(id))
                        }
                        ProjectLifecycleMutation::Apply(input) => {
                            let id = ReviewDecisionId::from(input.decision_id.clone());
                            (lifecycle::apply(&forge, &owner, &actor, input)?, Some(id))
                        }
                        ProjectLifecycleMutation::RequestChanges(input) => (
                            lifecycle::request_changes(&forge, &owner, &actor, input)?,
                            None,
                        ),
                        ProjectLifecycleMutation::Discard(input) => {
                            (lifecycle::discard(&forge, &owner, &actor, input)?, None)
                        }
                    })
                })())
            })
            .await??;
        let mut response = serde_json::json!({"work_id": item.id, "title": item.title,
            "state": item.state, "disposition": item.disposition, "decision_id": decision_id,
            "closed": item.state.is_terminal()});
        // Native state is authoritative even if secondary graph publication is
        // temporarily unavailable. A retry can resolve the exact Forge resource.
        let store = self.store.clone();
        let forge = self.forge.clone();
        let work_id = item.id.to_string();
        let observation = self
            .execution
            .run(ExecutionClass::Observation, 8 * 1024 * 1024, move || {
                Ok(native_project::resolve(
                    &store,
                    &domain,
                    &forge,
                    WorkProjectResolveInput {
                        work_id,
                        target: native_project::ProjectResolveTarget::ForgeWork {},
                    },
                ))
            })
            .await;
        response["graph_refreshed"] = matches!(observation, Ok(Ok(_))).into();
        if let Some(host) = crate::daemon::coordination::local_coordination_host() {
            response["memory_lineage"] = host
                .finalize_project_lifecycle(&item, decision_id.as_ref())
                .await;
        }
        bounded_response(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_uses_owner_workspace_authority_without_operator_escalation() {
        let callback = RequestPrincipal::continuation("user:alice");
        assert!(require_owner_lifecycle(&callback).is_ok());
        assert!(!callback.capabilities().contains(Capability::AdminExecute));
        assert!(require_owner_lifecycle(&RequestPrincipal::worker("user:alice")).is_err());
        assert!(
            require_owner_lifecycle(&RequestPrincipal::anonymous(
                crate::request_principal::TransportClass::Loopback
            ))
            .is_err()
        );
        assert!(
            require_owner_lifecycle(&RequestPrincipal::external_agent(
                Arc::from("executor"),
                "user:alice".into(),
                true,
                crate::request_principal::TransportClass::Loopback
            ))
            .is_err()
        );
    }
}
