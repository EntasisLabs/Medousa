//! Authenticated provider callbacks commit to the work journal before the
//! conversation mirror. Neither replay nor inspection sends another request.
use super::*;
use medousa_acp_client::coordination::store::CoordinationStore;
use medousa_forge::{forge::Forge, model::WorkId};
use medousa_types::{
    ExternalEventKind, ExternalProvider, ExternalProviderEventRequest,
    coordination::PeerAssignmentOutcome, work_coordination::*, work_provider::*,
};
use sha2::{Digest, Sha256};

fn identity(prefix: &str, conversation: &str, request: &str, event: Option<&str>) -> String {
    format!(
        "{prefix}:{:x}",
        Sha256::digest(
            serde_json::to_vec(&(conversation, request, event)).expect("string tuple serializes")
        )
    )
}

fn source_request(
    native: &CoordinationStore,
    domain: &UserDomainRef,
    source: &WorkProviderReviewSource,
) -> Result<medousa_types::coordination::ExternalPeerAssignmentRequest> {
    source_request_with_visibility(
        native,
        domain,
        source,
        crate::session_catalog::session_visible_to_profile,
    )
}

pub(super) fn source_request_with_visibility(
    native: &CoordinationStore,
    domain: &UserDomainRef,
    source: &WorkProviderReviewSource,
    visible: impl Fn(&str, &str) -> bool,
) -> Result<medousa_types::coordination::ExternalPeerAssignmentRequest> {
    if source.channel.authority_id != domain.authority_id {
        bail!("provider review source belongs to another workshop");
    }
    native.require_owner(&source.channel, &domain.user_id)?;
    let receipt = native.receipt(&source.channel, &source.executor_assignment_id)?;
    let proposal = native
        .proposal_for_assignment(&source.channel, &source.executor_assignment_id)?
        .ok_or_else(|| anyhow::anyhow!("provider review requires an exact native proposal"))?;
    if receipt.outcome != PeerAssignmentOutcome::Completed
        || proposal.request.owner_principal_id != domain.user_id
        || proposal.request.target.authority_id != domain.authority_id
        || proposal.request.owner_session.authority_id != domain.authority_id
        || proposal.request.execution_session.authority_id != domain.authority_id
        || receipt.binding.owner_principal_id != domain.user_id
        || receipt.binding.target != proposal.request.target
        || receipt.binding.execution_session != proposal.request.execution_session
        || !visible(
            proposal.request.owner_session.session_id.as_str(),
            &domain.user_id,
        )
        || proposal.request.context.sources.iter().any(|source| {
            source.selection.session.authority_id != domain.authority_id
                || !visible(
                    source.selection.session.session_id.as_str(),
                    &domain.user_id,
                )
        })
    {
        bail!("provider review requires a completed owned execution with visible source context");
    }
    Ok(proposal.request)
}

pub(super) fn review_revision(forge: &Forge, owner: &str, input: &WorkReviewInput) -> Result<bool> {
    let id = WorkId::parse_storage(&input.forge_work_id).map_err(anyhow::Error::msg)?;
    let _custody = forge
        .store()
        .try_lock_item(&id)?
        .ok_or_else(|| anyhow::anyhow!("provider review checkout custody busy"))?;
    review_revision_held(forge, owner, input)
}

/// Caller holds the Forge item fence through the graph transaction.
pub(super) fn review_revision_held(
    forge: &Forge,
    owner: &str,
    input: &WorkReviewInput,
) -> Result<bool> {
    let id = WorkId::parse_storage(&input.forge_work_id).map_err(anyhow::Error::msg)?;
    let item = forge.load(&id)?;
    if item.owner != owner {
        return Ok(false);
    }
    let Some(env) = item.workspace_environment() else {
        return Ok(false);
    };
    let _files = medousa_store::StoreRoot::open_nofollow(&env.worktree)?;
    Ok(env.generation == input.environment_generation
        && env.branch == input.branch
        && forge.git().repo_identity(&env.worktree)?.common_dir == env.repo.common_dir
        && forge.git().current_branch(&env.worktree)?.as_deref() == Some(env.branch.as_str())
        && forge.git().is_clean(&env.worktree)?
        && forge.git().head_oid(&env.worktree)?.to_string() == input.head_oid)
}

fn capture_review(
    forge: &Forge,
    native: &CoordinationStore,
    domain: &UserDomainRef,
    input: &WorkProviderRequestInput,
    source: &WorkProviderReviewSource,
    correlation: String,
) -> Result<WorkReviewInput> {
    let request = source_request(native, domain, source)?;
    let id = WorkId::parse_storage(&request.forge_work_id).map_err(anyhow::Error::msg)?;
    let _custody = forge
        .store()
        .try_lock_item(&id)?
        .ok_or_else(|| anyhow::anyhow!("provider review checkout custody busy"))?;
    let item = forge.load(&id)?;
    if item.owner != domain.user_id {
        bail!("provider review project is not owned by this domain");
    }
    let env = item
        .workspace_environment()
        .ok_or_else(|| anyhow::anyhow!("provider review requires a governed checkout"))?;
    let _files = medousa_store::StoreRoot::open_nofollow(&env.worktree)?;
    if forge.git().repo_identity(&env.worktree)?.common_dir != env.repo.common_dir
        || forge.git().current_branch(&env.worktree)?.as_deref() != Some(env.branch.as_str())
        || !forge.git().is_clean(&env.worktree)?
    {
        bail!("provider review requires a clean checkout on the governed branch");
    }
    Ok(WorkReviewInput {
        coordination_id: correlation,
        work_unit_id: input.work_unit_id.clone(),
        executor_assignment_id: source.executor_assignment_id.clone(),
        executor_receipt_id: native
            .receipt(&source.channel, &source.executor_assignment_id)?
            .receipt_id,
        forge_work_id: request.forge_work_id,
        environment_generation: env.generation,
        branch: env.branch.clone(),
        head_oid: forge.git().head_oid(&env.worktree)?.to_string(),
    })
}

fn native_provenance(actor: String) -> RecordProvenance {
    RecordProvenance {
        actor_id: actor,
        source: RecordSource::SystemEvent,
        evidence: vec![],
    }
}

impl WorkUnitHost {
    pub(crate) async fn prepare_provider_request(
        &self,
        domain: UserDomainRef,
        conversation: String,
        provider: ExternalProvider,
        request_id: String,
        input: WorkProviderRequestInput,
        text: String,
    ) -> Result<WorkProviderRequest> {
        let forge = self.forge.clone();
        let native =
            crate::daemon::coordination::local_coordination_host().map(|host| host.work_registry());
        self.with_store(move |store| {
            store.admit_peer_coordination(
                &domain,
                &input.work_unit_id,
                input.expected_scope_revision,
            )?;
            if let Some(native) = &native
                && native.work_is_controlled(&domain, &input.work_unit_id)?
            {
                bail!("provider work cannot replace an existing native execute/review controller");
            }
            if input.deadline <= chrono::Utc::now()
                || input.deadline - chrono::Utc::now() > chrono::Duration::days(1)
            {
                bail!("provider work deadline must be within 24 hours");
            }
            let instruction_digest = format!("{:x}", Sha256::digest(text.as_bytes()));
            if let Some(saved) = store.provider_dispatch(&domain, &conversation, &request_id)? {
                let dispatch = &saved.dispatch;
                if saved.closed_reason.is_some()
                    || dispatch.input != input
                    || dispatch.provider != provider
                    || dispatch.instructions != text
                    || store.peer_coordination_scope_digest(&domain, &input.work_unit_id)?
                        != dispatch.scope_digest
                {
                    bail!("provider request differs from saved handoff authority");
                }
            }
            if let Some(existing) = store.provider_request(&domain, &conversation, &request_id)? {
                if existing.request.input != input
                    || existing.request.provider != provider
                    || existing.request.instruction_digest != instruction_digest
                {
                    bail!("provider request identity describes different work");
                }
                if existing.dispatch_claimed {
                    bail!(
                        "provider dispatch already claimed; inspect its outcome without resending"
                    );
                }
                return Ok(existing.request);
            }
            let reviewed = input
                .review_of
                .as_ref()
                .map(|source| {
                    capture_review(
                        &forge,
                        native
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("native review source unavailable"))?,
                        &domain,
                        &input,
                        source,
                        identity("provider-review", &conversation, &request_id, None),
                    )
                })
                .transpose()?;
            let request = WorkProviderRequest {
                conversation_id: conversation.clone(),
                request_id: request_id.clone(),
                provider,
                scope_digest: store.peer_coordination_scope_digest(&domain, &input.work_unit_id)?,
                completion_condition: store
                    .work_unit(&domain, &input.work_unit_id)?
                    .completion_condition,
                input,
                instruction_digest,
                reviewed,
            };
            store.apply_native_command(
                &domain,
                identity("provider-request", &conversation, &request_id, None),
                WorkGraphMutation::RegisterProviderRequest {
                    request: Box::new(request.clone()),
                },
                native_provenance("adapter:provider-work".into()),
            )?;
            Ok(request)
        })
        .await
    }

    pub(crate) async fn claim_provider_request(
        &self,
        domain: UserDomainRef,
        request: WorkProviderRequest,
    ) -> Result<()> {
        let forge = self.forge.clone();
        let native =
            crate::daemon::coordination::local_coordination_host().map(|host| host.work_registry());
        self.with_store(move |store| {
            store.admit_peer_coordination(
                &domain,
                &request.input.work_unit_id,
                request.input.expected_scope_revision,
            )?;
            if store.peer_coordination_scope_digest(&domain, &request.input.work_unit_id)?
                != request.scope_digest
                || request.input.deadline <= chrono::Utc::now()
            {
                bail!("provider work scope or deadline changed before dispatch");
            }
            let _checkout = request
                .reviewed
                .as_ref()
                .map(|reviewed| -> Result<_> {
                    let id = WorkId::parse_storage(&reviewed.forge_work_id)
                        .map_err(anyhow::Error::msg)?;
                    let custody = forge
                        .store()
                        .try_lock_item(&id)?
                        .ok_or_else(|| anyhow::anyhow!("provider review checkout custody busy"))?;
                    if !review_revision_held(&forge, &domain.user_id, reviewed)? {
                        bail!("provider review revision changed before dispatch");
                    }
                    Ok(custody)
                })
                .transpose()?;
            let receipt = store.apply_native_command_checked(
                &domain,
                identity(
                    "provider-dispatch",
                    &request.conversation_id,
                    &request.request_id,
                    None,
                ),
                WorkGraphMutation::ClaimProviderRequest {
                    conversation_id: request.conversation_id.clone(),
                    request_id: request.request_id.clone(),
                },
                native_provenance("adapter:provider-work".into()),
                |_| {
                    (|| -> Result<()> {
                        if request.input.deadline <= chrono::Utc::now() {
                            bail!("provider dispatch deadline expired during admission");
                        }
                        if let Some(reviewed) = &request.reviewed
                            && !review_revision_held(&forge, &domain.user_id, reviewed)?
                        {
                            bail!("provider review revision changed during admission");
                        }
                        if let Some(native) = &native {
                            if native.work_is_controlled(&domain, &request.input.work_unit_id)? {
                                bail!("provider work cannot replace a native controller");
                            }
                            if let Some(source) = &request.input.review_of {
                                let source = source_request(native, &domain, source)?;
                                if let Some(saved) = store.provider_dispatch(
                                    &domain,
                                    &request.conversation_id,
                                    &request.request_id,
                                )? && format!(
                                    "{:x}",
                                    Sha256::digest(serde_json::to_vec(&source)?)
                                ) != saved.dispatch.source_request_digest
                                {
                                    bail!("saved provider handoff source changed before dispatch");
                                }
                            }
                        } else if request.reviewed.is_some() {
                            bail!("native review source unavailable");
                        }
                        Ok(())
                    })()
                    .map_err(|error| {
                        medousa_store::PersistenceError::new(
                            medousa_store::PersistenceErrorKind::PermanentIo,
                            error.to_string(),
                        )
                    })
                },
            )?;
            if receipt.replayed {
                bail!("provider dispatch custody is uncertain; never resend a claimed request");
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn provider_request(
        &self,
        domain: UserDomainRef,
        conversation: String,
        request: String,
    ) -> Result<Option<WorkProviderRecord>> {
        self.with_store(move |store| {
            Ok(store.provider_request(&domain, &conversation, &request)?)
        })
        .await
    }

    pub(crate) async fn record_provider_outcome(
        &self,
        domain: UserDomainRef,
        conversation: String,
        actor: String,
        input: ExternalProviderEventRequest,
    ) -> Result<()> {
        let forge = self.forge.clone();
        let native =
            crate::daemon::coordination::local_coordination_host().map(|host| host.work_registry());
        self.with_store(move |store| {
            let record = store
                .provider_request(&domain, &conversation, &input.request_id)?
                .ok_or_else(|| anyhow::anyhow!("unknown provider work request"))?;
            if input.reaction.is_some()
                || !matches!(
                    input.kind,
                    ExternalEventKind::Progress
                        | ExternalEventKind::Question
                        | ExternalEventKind::Completed
                        | ExternalEventKind::Failed
                )
            {
                bail!("provider work callback must be a progress, question or terminal result");
            }
            let event_id = format!("provider:{}", input.event_id);
            let event = if let Some(existing) = record
                .events
                .iter()
                .find(|event| event.event_id == event_id)
            {
                if existing.kind != input.kind || existing.text != input.text {
                    bail!("provider event identity describes different evidence");
                }
                existing.clone()
            } else {
                let request = &record.request;
                let unit = store.work_unit(&domain, &request.input.work_unit_id)?;
                let mut qualification = WorkProviderQualification::OutcomeOnly;
                let mut review_decision = None;
                if unit.scope_revision != request.input.expected_scope_revision
                    || store.peer_coordination_scope_digest(&domain, &unit.work_unit_id)?
                        != request.scope_digest
                {
                    qualification = WorkProviderQualification::ScopeChanged;
                } else if unit.state == WorkUnitState::Paused || unit.state.is_terminal() {
                    qualification = WorkProviderQualification::InactiveWork;
                } else if request.input.deadline <= chrono::Utc::now() {
                    qualification = WorkProviderQualification::Expired;
                } else if input.kind == ExternalEventKind::Completed
                    && let Some(reviewed) = &request.reviewed
                {
                    let visible = source_request(
                        native
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("native review source unavailable"))?,
                        &domain,
                        request.input.review_of.as_ref().unwrap(),
                    )
                    .is_ok();
                    if !visible {
                        qualification = WorkProviderQualification::InactiveWork;
                    } else if !review_revision(&forge, &domain.user_id, reviewed)? {
                        qualification = WorkProviderQualification::RevisionChanged;
                    } else if let Ok(decision) =
                        serde_json::from_str::<WorkReviewDecision>(&input.text)
                        && decision.reviewed == *reviewed
                        && !decision.summary.trim().is_empty()
                        && decision.summary.len() <= 4096
                        && input.text.len() <= 8192
                    {
                        qualification = match decision.verdict {
                            WorkReviewVerdict::Approved => {
                                WorkProviderQualification::ReviewApproved
                            }
                            WorkReviewVerdict::ChangesRequested => {
                                WorkProviderQualification::ChangesRequested
                            }
                        };
                        review_decision = Some(decision);
                    } else {
                        qualification = WorkProviderQualification::InvalidReview;
                    }
                }
                WorkProviderEvent {
                    conversation_id: conversation.clone(),
                    request_id: input.request_id.clone(),
                    event_id: event_id.clone(),
                    actor_id: actor,
                    request_sequence: record.events.len() as u64 + 1,
                    kind: input.kind,
                    text: input.text,
                    created_at: chrono::Utc::now(),
                    qualification,
                    review_decision,
                }
            };
            store.apply_native_command_checked(
                &domain,
                identity(
                    "provider-result",
                    &conversation,
                    &input.request_id,
                    Some(&event_id),
                ),
                WorkGraphMutation::RecordProviderEvent {
                    event: Box::new(event.clone()),
                },
                native_provenance(event.actor_id.clone()),
                |_| {
                    (|| -> Result<()> {
                        if matches!(event.qualification, WorkProviderQualification::ReviewApproved | WorkProviderQualification::ChangesRequested) {
                            let request = &record.request;
                            let unit = store.work_unit(&domain, &request.input.work_unit_id)?;
                            if unit.scope_revision != request.input.expected_scope_revision
                                || unit.state.is_terminal() || unit.state == WorkUnitState::Paused
                                || request.input.deadline <= chrono::Utc::now()
                                || store.peer_coordination_scope_digest(&domain, &unit.work_unit_id)? != request.scope_digest
                            {
                                bail!("provider review scope changed during publication; retain callback for reconciliation");
                            }
                            source_request(native.as_deref().ok_or_else(|| anyhow::anyhow!("native review source unavailable"))?, &domain, request.input.review_of.as_ref().unwrap())?;
                            if !review_revision(&forge, &domain.user_id, request.reviewed.as_ref().unwrap())? {
                                bail!("provider review revision changed during publication; retain callback for reconciliation");
                            }
                        }
                        Ok(())
                    })().map_err(|error| medousa_store::PersistenceError::new(medousa_store::PersistenceErrorKind::PermanentIo, error.to_string()))
                },
            )?;
            Ok(())
        })
        .await?;
        crate::daemon::coordination::wake_work_coordinator();
        Ok(())
    }
}
