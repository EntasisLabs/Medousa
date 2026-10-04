//! Explicit native admission for one later provider review send. This is not a
//! model grant, executor launch, owner-chat continuation or delivery retry.
use super::*;
use medousa_acp_client::coordination::store::CoordinationStore;
use medousa_types::{coordination::*, work_provider::*};
use medousa_work::PROVIDER_DISPATCH_ACTOR;
use sha2::{Digest, Sha256};

fn digest(value: &impl serde::Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn provenance() -> RecordProvenance {
    RecordProvenance {
        actor_id: PROVIDER_DISPATCH_ACTOR.into(),
        source: RecordSource::SystemEvent,
        evidence: vec![],
    }
}

fn source_proposal(
    native: &CoordinationStore,
    domain: &UserDomainRef,
    source: &WorkProviderReviewSource,
    visible: impl Fn(&str, &str) -> bool,
) -> Result<PeerAssignmentProposal> {
    if source.channel.authority_id != domain.authority_id {
        bail!("provider handoff requires a local native source");
    }
    native.require_owner(&source.channel, &domain.user_id)?;
    let proposal = native
        .proposal_for_assignment(&source.channel, &source.executor_assignment_id)?
        .ok_or_else(|| anyhow::anyhow!("provider handoff requires an exact native proposal"))?;
    let request = &proposal.request;
    if request.owner_principal_id != domain.user_id
        || request.target.authority_id != domain.authority_id
        || request.owner_session.authority_id != domain.authority_id
        || request.execution_session.authority_id != domain.authority_id
        || !visible(request.owner_session.session_id.as_str(), &domain.user_id)
        || request.context.sources.iter().any(|source| {
            source.selection.session.authority_id != domain.authority_id
                || !visible(
                    source.selection.session.session_id.as_str(),
                    &domain.user_id,
                )
        })
    {
        bail!("provider handoff source context is no longer visible to its owner");
    }
    Ok(proposal)
}

impl WorkUnitHost {
    pub(crate) async fn admit_provider_dispatch(
        &self,
        domain: UserDomainRef,
        mut dispatch: WorkProviderDispatch,
    ) -> Result<()> {
        let native = crate::daemon::coordination::local_coordination_host()
            .ok_or_else(|| anyhow::anyhow!("native coordination unavailable"))?
            .work_registry();
        let host = self.clone_for_dispatch();
        self.with_store(move |_| {
            host.admit_provider_dispatch_with(
                &native,
                &domain,
                &mut dispatch,
                crate::session_catalog::session_visible_to_profile,
            )
        })
        .await?;
        crate::daemon::coordination::wake_work_coordinator();
        Ok(())
    }

    // WorkUnitHost's stores are shared; synchronous helpers are only invoked
    // inside the existing StoreIo admission, including their test fixtures.
    fn clone_for_dispatch(&self) -> Self {
        Self {
            store: self.store.clone(),
            execution: self.execution.clone(),
            forge: self.forge.clone(),
        }
    }

    pub(super) fn admit_provider_dispatch_with(
        &self,
        native: &CoordinationStore,
        domain: &UserDomainRef,
        dispatch: &mut WorkProviderDispatch,
        visible: impl Fn(&str, &str) -> bool,
    ) -> Result<()> {
        if let Some(saved) =
            self.store
                .provider_dispatch(domain, &dispatch.conversation_id, &dispatch.request_id)?
        {
            // Derive admission fields from the saved record, never rebase intent.
            dispatch.scope_digest = saved.dispatch.scope_digest.clone();
            dispatch.source_request_digest = saved.dispatch.source_request_digest.clone();
            if saved.dispatch != *dispatch {
                bail!("provider handoff identity describes different intent");
            }
            return Ok(());
        }
        self.store.admit_peer_coordination(
            domain,
            &dispatch.input.work_unit_id,
            dispatch.input.expected_scope_revision,
        )?;
        if native.work_is_controlled(domain, &dispatch.input.work_unit_id)? {
            bail!("provider handoff cannot replace a native execute/review controller");
        }
        let source = dispatch
            .input
            .review_of
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("provider handoff requires work.review_of"))?;
        let proposal = source_proposal(native, domain, source, &visible)?;
        native.require_approved_proposal(
            &source.channel,
            &proposal.proposal_id,
            &domain.user_id,
        )?;
        let item = self.forge.load(
            &medousa_forge::model::WorkId::parse_storage(&proposal.request.forge_work_id)
                .map_err(anyhow::Error::msg)?,
        )?;
        if item.owner != domain.user_id {
            bail!("provider handoff project is not owned by this domain");
        }
        dispatch.source_request_digest = digest(&proposal.request)?;
        dispatch.scope_digest = self
            .store
            .peer_coordination_scope_digest(domain, &dispatch.input.work_unit_id)?;
        self.store.apply_native_command_checked(
            domain,
            format!(
                "provider-handoff:{}",
                digest(&(&dispatch.conversation_id, &dispatch.request_id))?
            ),
            WorkGraphMutation::RegisterProviderDispatch {
                dispatch: Box::new(dispatch.clone()),
            },
            provenance(),
            |_| {
                (|| -> Result<()> {
                    self.store.admit_peer_coordination(
                        domain,
                        &dispatch.input.work_unit_id,
                        dispatch.input.expected_scope_revision,
                    )?;
                    if self
                        .store
                        .peer_coordination_scope_digest(domain, &dispatch.input.work_unit_id)?
                        != dispatch.scope_digest
                    {
                        bail!("provider handoff scope changed during admission");
                    }
                    if native.work_is_controlled(domain, &dispatch.input.work_unit_id)? {
                        bail!("native work custody changed during provider handoff admission");
                    }
                    native.require_approved_proposal(
                        &source.channel,
                        &proposal.proposal_id,
                        &domain.user_id,
                    )?;
                    source_proposal(native, domain, source, &visible)?;
                    Ok(())
                })()
                .map_err(|error| {
                    medousa_store::PersistenceError::new(
                        medousa_store::PersistenceErrorKind::Conflict,
                        error.to_string(),
                    )
                })
            },
        )?;
        Ok(())
    }

    pub(crate) async fn pending_provider_dispatch(
        &self,
        domain: UserDomainRef,
        conversation: String,
        request: String,
    ) -> Result<Option<WorkProviderDispatchRecord>> {
        self.with_store(move |store| {
            Ok(store.provider_dispatch(&domain, &conversation, &request)?)
        })
        .await
    }

    pub(crate) async fn close_provider_dispatch(
        &self,
        domain: UserDomainRef,
        dispatch: WorkProviderDispatch,
        reason: String,
    ) -> Result<()> {
        let host = self.clone_for_dispatch();
        self.with_store(move |_| host.close_provider_dispatch_with(&domain, &dispatch, &reason))
            .await
    }

    pub(crate) async fn provider_dispatch_ready(
        &self,
        domain: UserDomainRef,
        dispatch: WorkProviderDispatch,
    ) -> Result<bool> {
        let native = crate::daemon::coordination::local_coordination_host()
            .ok_or_else(|| anyhow::anyhow!("native coordination unavailable"))?
            .work_registry();
        let host = self.clone_for_dispatch();
        self.with_store(move |_| {
            host.provider_dispatch_ready_with(
                &native,
                &domain,
                &dispatch,
                crate::session_catalog::session_visible_to_profile,
            )
        })
        .await
    }

    pub(super) fn provider_dispatch_ready_with(
        &self,
        native: &CoordinationStore,
        domain: &UserDomainRef,
        dispatch: &WorkProviderDispatch,
        visible: impl Fn(&str, &str) -> bool,
    ) -> Result<bool> {
        let saved = self
            .store
            .provider_dispatch(domain, &dispatch.conversation_id, &dispatch.request_id)?
            .ok_or_else(|| anyhow::anyhow!("provider handoff admission missing"))?;
        if saved.dispatch != *dispatch {
            bail!("provider handoff differs from saved admission");
        }
        if saved.closed_reason.is_some()
            || self
                .store
                .provider_request(domain, &dispatch.conversation_id, &dispatch.request_id)?
                .is_some_and(|request| request.dispatch_claimed)
        {
            return Ok(false);
        }
        let unit = self.store.work_unit(domain, &dispatch.input.work_unit_id)?;
        let reason = if dispatch.input.deadline <= chrono::Utc::now() {
            Some("provider handoff deadline expired")
        } else if unit.scope_revision != dispatch.input.expected_scope_revision
            || self
                .store
                .peer_coordination_scope_digest(domain, &unit.work_unit_id)?
                != dispatch.scope_digest
        {
            Some("provider handoff scope changed")
        } else if unit.state.is_terminal() {
            Some("provider handoff work is terminal")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.close_provider_dispatch_with(domain, dispatch, reason)?;
            return Ok(false);
        }
        if !matches!(
            unit.state,
            WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
        ) {
            return Ok(false);
        }
        self.store.admit_peer_coordination(
            domain,
            &unit.work_unit_id,
            dispatch.input.expected_scope_revision,
        )?;
        if native.work_is_controlled(domain, &unit.work_unit_id)? {
            bail!("provider handoff cannot replace native controller custody");
        }
        let source = dispatch
            .input
            .review_of
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("provider handoff source missing"))?;
        let proposal = source_proposal(native, domain, source, &visible)?;
        if digest(&proposal.request)? != dispatch.source_request_digest {
            bail!("provider handoff source request changed");
        }
        let Some(receipt) =
            native.receipt_if_recorded(&source.channel, &source.executor_assignment_id)?
        else {
            return Ok(false);
        };
        if receipt.outcome != PeerAssignmentOutcome::Completed {
            self.close_provider_dispatch_with(
                domain,
                dispatch,
                "native executor did not complete; provider review not sent",
            )?;
            return Ok(false);
        }
        provider_events::source_request_with_visibility(native, domain, source, visible)?;
        Ok(true)
    }

    fn close_provider_dispatch_with(
        &self,
        domain: &UserDomainRef,
        dispatch: &WorkProviderDispatch,
        reason: &str,
    ) -> Result<()> {
        self.store.apply_native_command(
            domain,
            format!(
                "provider-handoff-close:{}",
                digest(&(&dispatch.conversation_id, &dispatch.request_id))?
            ),
            WorkGraphMutation::CloseProviderDispatch {
                conversation_id: dispatch.conversation_id.clone(),
                request_id: dispatch.request_id.clone(),
                reason: reason.into(),
            },
            provenance(),
        )?;
        Ok(())
    }
}
