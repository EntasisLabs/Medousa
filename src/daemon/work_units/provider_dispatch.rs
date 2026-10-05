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
        dispatch.scope_digest = self
            .store
            .peer_coordination_scope_digest(domain, &dispatch.input.work_unit_id)?;
        if let Some(source) = &dispatch.after_provider_completion {
            let request = self
                .store
                .provider_request(
                    domain,
                    &source.request.conversation_id,
                    &source.request.request_id,
                )?
                .ok_or_else(|| {
                    anyhow::anyhow!("provider predecessor is not in this owner domain")
                })?;
            dispatch.source_request_digest = digest(&request.request)?;
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
                            || native.work_is_controlled(domain, &dispatch.input.work_unit_id)?
                        {
                            bail!(
                                "provider chain scope or native custody changed during admission"
                            );
                        }
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
            return Ok(());
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

    /// Transfer saved future custody to one exact successor. No network effect.
    pub(crate) async fn resume_provider_plan(
        &self,
        domain: UserDomainRef,
        dispatch: WorkProviderDispatch,
    ) -> Result<bool> {
        let native = crate::daemon::coordination::local_coordination_host()
            .ok_or_else(|| anyhow::anyhow!("native coordination unavailable"))?
            .work_registry();
        let host = WorkUnitHost {
            store: self.store.clone(),
            execution: self.execution.clone(),
            forge: self.forge.clone(),
        };
        self.with_store(move |_| {
            host.resume_provider_plan_with(
                &native,
                &domain,
                &dispatch,
                crate::session_catalog::session_visible_to_profile,
            )
        })
        .await
    }

    fn resume_provider_plan_with(
        &self,
        native: &CoordinationStore,
        domain: &UserDomainRef,
        dispatch: &WorkProviderDispatch,
        visible: impl Fn(&str, &str) -> bool,
    ) -> Result<bool> {
        let Some(mut next) = self.store.next_provider_dispatch(domain, dispatch)? else {
            return Ok(false);
        };
        let unit = self.store.work_unit(domain, &dispatch.input.work_unit_id)?;
        let stale = dispatch.input.deadline <= chrono::Utc::now()
            || unit.state.is_terminal()
            || unit.scope_revision != dispatch.input.expected_scope_revision
            || self
                .store
                .peer_coordination_scope_digest(domain, &unit.work_unit_id)?
                != dispatch.scope_digest;
        if stale {
            self.close_provider_dispatch_with(
                domain,
                dispatch,
                "provider plan expired, cancelled or rescoped",
            )?;
            return Ok(true);
        }
        if unit.state == WorkUnitState::Paused {
            return Ok(true);
        }
        if self.store.provider_chain_terminal(domain, &next).is_err() {
            self.close_provider_dispatch_with(
                domain,
                dispatch,
                "provider plan predecessor failed or became stale",
            )?;
            return Ok(true);
        }
        self.admit_provider_dispatch_with(native, domain, &mut next, visible)?;
        Ok(true)
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
        if unit.state == WorkUnitState::Paused {
            return Ok(false);
        }
        if dispatch.after_provider_completion.is_some() {
            let terminal = match self.store.provider_chain_terminal(domain, dispatch) {
                Ok(terminal) => terminal,
                Err(error) => {
                    self.close_provider_dispatch_with(
                        domain,
                        dispatch,
                        &format!("provider predecessor is not admitted: {error}"),
                    )?;
                    return Ok(false);
                }
            };
            if terminal.is_none()
                || !matches!(
                    unit.state,
                    WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
                )
            {
                return Ok(false);
            }
            self.store.admit_peer_coordination(
                domain,
                &unit.work_unit_id,
                dispatch.input.expected_scope_revision,
            )?;
            if native.work_is_controlled(domain, &unit.work_unit_id)? {
                bail!("provider chain cannot replace native controller custody");
            }
            return Ok(true);
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

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::{AuthorityId, ExternalEventKind};

    fn domain() -> UserDomainRef {
        UserDomainRef {
            authority_id: AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap(),
            user_id: "owner".into(),
        }
    }
    fn event(
        kind: ExternalEventKind,
        qualification: WorkProviderQualification,
        sequence: u64,
    ) -> WorkProviderEvent {
        WorkProviderEvent {
            conversation_id: "upstream".into(),
            request_id: "first".into(),
            event_id: format!("event-{sequence}"),
            actor_id: PROVIDER_DISPATCH_ACTOR.into(),
            request_sequence: sequence,
            kind,
            text: "Immutable predecessor result".into(),
            created_at: chrono::Utc::now(),
            qualification,
            review_decision: None,
        }
    }
    async fn fixture() -> (
        Arc<tempfile::TempDir>,
        Arc<WorkUnitHost>,
        Arc<CoordinationStore>,
        WorkProviderDispatch,
    ) {
        fixture_with_plan(false).await
    }

    async fn fixture_with_plan(
        with_plan: bool,
    ) -> (
        Arc<tempfile::TempDir>,
        Arc<WorkUnitHost>,
        Arc<CoordinationStore>,
        WorkProviderDispatch,
    ) {
        let execution = Arc::new(ForgeExecutionService::new());
        let exec = execution.clone();
        execution
            .run(ExecutionClass::StoreIo, MAX_SNAPSHOT_BYTES, move || {
                Ok((|| -> Result<_> {
                    let temp =
                        Arc::new(tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?);
                    let root = temp.path().canonicalize()?;
                    let host = Arc::new(WorkUnitHost {
                        store: Arc::new(WorkGraphStore::open(&root.join("graph"))?),
                        execution: exec,
                        forge: Arc::new(medousa_forge::forge::Forge::open(root.join("forge"))?),
                    });
                    let native = Arc::new(CoordinationStore::open(&root.join("native"))?);
                    host.store.apply_native_command(
                        &domain(),
                        "work".into(),
                        WorkGraphMutation::AcceptWork {
                            work_unit_id: "unit".into(),
                            intent: "Execute and assess the result".into(),
                            kind: WorkUnitKind::Finite,
                            scope: WorkScope::default(),
                            completion_condition: "Native completion evidence is required".into(),
                            contact: WorkContactPreference::Silent,
                            origin: None,
                            budget: None,
                        },
                        provenance(),
                    )?;
                    let input = WorkProviderRequestInput {
                        work_unit_id: "unit".into(),
                        expected_scope_revision: 1,
                        deadline: chrono::Utc::now() + chrono::Duration::hours(1),
                        review_of: None,
                    };
                    let request = WorkProviderRequest {
                        conversation_id: "upstream".into(),
                        request_id: "first".into(),
                        provider: medousa_types::ExternalProvider::GrokBot,
                        input: input.clone(),
                        instruction_digest: "c".repeat(64),
                        scope_digest: host
                            .store
                            .peer_coordination_scope_digest(&domain(), "unit")?,
                        completion_condition: "Native completion evidence is required".into(),
                        reviewed: None,
                        predecessor: None,
                    };
                    host.store.apply_native_command(
                        &domain(),
                        "request".into(),
                        WorkGraphMutation::RegisterProviderRequest {
                            request: Box::new(request),
                        },
                        provenance(),
                    )?;
                    host.store.apply_native_command(
                        &domain(),
                        "send".into(),
                        WorkGraphMutation::ClaimProviderRequest {
                            conversation_id: "upstream".into(),
                            request_id: "first".into(),
                        },
                        provenance(),
                    )?;
                    let mut dispatch = WorkProviderDispatch {
                        conversation_id: "downstream".into(),
                        request_id: "second".into(),
                        provider: medousa_types::ExternalProvider::Muse,
                        input,
                        instructions:
                            "Assess the predecessor outcome; do not claim native approval".into(),
                        target_digest: "d".repeat(64),
                        scope_digest: String::new(),
                        source_request_digest: String::new(),
                        after_provider_completion: Some(WorkProviderDispatchSource {
                            request: WorkProviderRequestRef {
                                conversation_id: "upstream".into(),
                                request_id: "first".into(),
                            },
                            target_digest: "e".repeat(64),
                        }),
                        coordinator_wake: false,
                        remaining_stages: vec![],
                    };
                    if with_plan {
                        dispatch.remaining_stages = vec![WorkProviderPlannedStage {
                            input: WorkProviderStageInput {
                                conversation_id: "third".into(),
                                request_id: "third-stage".into(),
                                text: "Assess stage two".into(),
                                coordinator_wake: false,
                            },
                            provider: medousa_types::ExternalProvider::Dots,
                            target_digest: "f".repeat(64),
                        }];
                    }
                    host.admit_provider_dispatch_with(
                        &native,
                        &domain(),
                        &mut dispatch,
                        |_, _| false,
                    )?;
                    Ok((temp, host, native, dispatch))
                })())
            })
            .await
            .unwrap()
            .unwrap()
    }

    fn inbox(
        host: &WorkUnitHost,
        conversation: &str,
        request: &str,
    ) -> Result<medousa_work::CoordinatorInbox> {
        host.store
            .coordinator_inboxes(&domain().authority_id, 4, None)?
            .inboxes
            .into_iter()
            .find(|inbox| inbox.conversation_id == conversation && inbox.request_id == request)
            .ok_or_else(|| anyhow::anyhow!("exact runtime inbox missing"))
    }

    #[tokio::test]
    async fn provider_chain_driver_waits_reopens_and_sends_one_correlated_stage_without_a_chat() {
        let (temp, host, native, dispatch) = fixture().await;
        assert!(
            host.prepare_provider_request(
                domain(),
                dispatch.conversation_id.clone(),
                dispatch.provider,
                dispatch.request_id.clone(),
                dispatch.input.clone(),
                dispatch.instructions.clone()
            )
            .await
            .is_err()
        );
        let saved = dispatch.clone();
        let ledger = native.clone();
        let driver = host.clone();
        host.with_store(move |_| {
            assert!(
                !driver.provider_dispatch_ready_with(&ledger, &domain(), &saved, |_, _| false)?
            );
            driver.store.apply_native_command(
                &domain(),
                "progress".into(),
                WorkGraphMutation::RecordProviderEvent {
                    event: Box::new(event(
                        ExternalEventKind::Progress,
                        WorkProviderQualification::OutcomeOnly,
                        1,
                    )),
                },
                provenance(),
            )?;
            assert!(
                !driver.provider_dispatch_ready_with(&ledger, &domain(), &saved, |_, _| false)?
            );
            driver.store.apply_native_command(
                &domain(),
                "terminal".into(),
                WorkGraphMutation::RecordProviderEvent {
                    event: Box::new(event(
                        ExternalEventKind::Completed,
                        WorkProviderQualification::OutcomeOnly,
                        2,
                    )),
                },
                provenance(),
            )?;
            driver.consume_provider_inbox_with(
                &inbox(&driver, "upstream", "first")?,
                Some(&ledger),
                |_, _| false,
            )?;
            driver.consume_provider_inbox_with(
                &inbox(&driver, "upstream", "first")?,
                Some(&ledger),
                |_, _| false,
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let root = temp.path().to_path_buf();
        let driver = host.clone();
        let saved = dispatch.clone();
        let ledger = native.clone();
        let reopened = host
            .with_store(move |_| {
                let reopened = Arc::new(WorkUnitHost {
                    store: Arc::new(WorkGraphStore::open(&root.join("graph"))?),
                    execution: driver.execution.clone(),
                    forge: driver.forge.clone(),
                });
                assert!(reopened.provider_dispatch_ready_with(
                    &ledger,
                    &domain(),
                    &saved,
                    |_, _| false
                )?);
                Ok(reopened)
            })
            .await
            .unwrap();
        let request = reopened
            .prepare_provider_request(
                domain(),
                dispatch.conversation_id.clone(),
                dispatch.provider,
                dispatch.request_id.clone(),
                dispatch.input.clone(),
                dispatch.instructions.clone(),
            )
            .await
            .unwrap();
        assert_eq!(request.predecessor.as_ref().unwrap().event_id, "event-2");
        let context = reopened
            .provider_predecessor_context(domain(), request.predecessor.clone().unwrap(), 2048)
            .await
            .unwrap();
        assert!(context.contains("Immutable predecessor result"));
        // Only after the real durable claim would the shared transport run.
        reopened
            .claim_provider_request(domain(), request.clone())
            .await
            .unwrap();
        assert!(
            reopened
                .claim_provider_request(domain(), request.clone())
                .await
                .is_err()
        );
        let driver = reopened.clone();
        let saved = dispatch.clone();
        let ledger = native.clone();
        reopened
            .with_store(move |_| {
                assert!(!driver.provider_dispatch_ready_with(
                    &ledger,
                    &domain(),
                    &saved,
                    |_, _| false
                )?);
                let mut terminal = event(
                    ExternalEventKind::Completed,
                    WorkProviderQualification::OutcomeOnly,
                    1,
                );
                terminal.conversation_id = saved.conversation_id.clone();
                terminal.request_id = saved.request_id.clone();
                terminal.event_id = "second-result".into();
                driver.store.apply_native_command(
                    &domain(),
                    "second-result".into(),
                    WorkGraphMutation::RecordProviderEvent {
                        event: Box::new(terminal),
                    },
                    provenance(),
                )?;
                driver.consume_provider_inbox_with(
                    &inbox(&driver, &saved.conversation_id, &saved.request_id)?,
                    Some(&ledger),
                    |_, _| false,
                )?;
                let unit = driver.store.work_unit(&domain(), "unit")?;
                assert_eq!(unit.state, WorkUnitState::Waiting);
                assert_eq!(unit.contact, WorkContactPreference::Silent);
                assert!(unit.origin.is_none());
                assert!(unit.conversations.is_empty());
                Ok(())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn provider_chain_driver_fences_failed_paused_cancelled_rescoped_and_stale_results() {
        for scenario in [
            "failed",
            "paused",
            "cancelled",
            "scope",
            "inactive",
            "budget",
        ] {
            let (_temp, host, native, dispatch) = fixture().await;
            let driver = host.clone();
            let saved = dispatch.clone();
            let ledger = native.clone();
            host.with_store(move |_| {
                driver.store.apply_native_command(
                    &domain(),
                    "terminal".into(),
                    WorkGraphMutation::RecordProviderEvent {
                        event: Box::new(event(
                            if scenario == "failed" {
                                ExternalEventKind::Failed
                            } else {
                                ExternalEventKind::Completed
                            },
                            if scenario == "inactive" {
                                WorkProviderQualification::InactiveWork
                            } else {
                                WorkProviderQualification::OutcomeOnly
                            },
                            1,
                        )),
                    },
                    provenance(),
                )?;
                if matches!(scenario, "paused" | "cancelled" | "failed") {
                    driver.store.apply_native_command(
                        &domain(),
                        "state".into(),
                        WorkGraphMutation::SetState {
                            work_unit_id: "unit".into(),
                            state: match scenario {
                                "paused" => WorkUnitState::Paused,
                                "cancelled" => WorkUnitState::Cancelled,
                                _ => WorkUnitState::NeedsAttention,
                            },
                            reason: "changed state".into(),
                            evidence: vec![],
                        },
                        provenance(),
                    )?;
                } else if scenario == "scope" {
                    driver.store.apply_native_command(
                        &domain(),
                        "scope".into(),
                        WorkGraphMutation::SetScope {
                            work_unit_id: "unit".into(),
                            scope: WorkScope::default(),
                        },
                        provenance(),
                    )?;
                } else if scenario == "budget" {
                    driver.store.apply_native_command(
                        &domain(),
                        "budget".into(),
                        WorkGraphMutation::SetBudget {
                            work_unit_id: "unit".into(),
                            limits: WorkBudgetLimits {
                                cost_microusd: 100,
                                execution_count: 2,
                                concurrent_executions: 1,
                                deadline: saved.input.deadline,
                            },
                        },
                        provenance(),
                    )?;
                }
                let result =
                    driver.provider_dispatch_ready_with(&ledger, &domain(), &saved, |_, _| false);
                if scenario == "budget" {
                    assert!(result.is_err());
                } else {
                    assert!(!result?);
                }
                let retained = driver
                    .store
                    .provider_dispatch(&domain(), "downstream", "second")?
                    .unwrap();
                assert_eq!(
                    retained.closed_reason.is_some(),
                    !matches!(scenario, "paused" | "budget")
                );
                assert!(
                    driver
                        .store
                        .provider_request(&domain(), "downstream", "second")?
                        .is_none()
                );
                if scenario == "paused" {
                    driver.store.apply_native_command(
                        &domain(),
                        "resume".into(),
                        WorkGraphMutation::SetState {
                            work_unit_id: "unit".into(),
                            state: WorkUnitState::Active,
                            reason: "resume".into(),
                            evidence: vec![],
                        },
                        provenance(),
                    )?;
                    assert!(driver.provider_dispatch_ready_with(
                        &ledger,
                        &domain(),
                        &saved,
                        |_, _| false
                    )?);
                }
                Ok(())
            })
            .await
            .unwrap();
        }
    }
    #[tokio::test]
    async fn provider_whole_plan_materializes_one_frozen_successor_or_closes_failed_tail() {
        for failed in [false, true] {
            let (_temp, host, native, dispatch) = fixture_with_plan(true).await;
            let driver = host.clone();
            let saved = dispatch.clone();
            host.with_store(move |_| {
                driver.store.apply_native_command(
                    &domain(),
                    "terminal".into(),
                    WorkGraphMutation::RecordProviderEvent {
                        event: Box::new(event(
                            ExternalEventKind::Completed,
                            WorkProviderQualification::OutcomeOnly,
                            1,
                        )),
                    },
                    provenance(),
                )?;
                assert!(
                    driver
                        .store
                        .next_provider_dispatch(&domain(), &saved)?
                        .is_none()
                );
                Ok(())
            })
            .await
            .unwrap();
            let request = host
                .prepare_provider_request(
                    domain(),
                    dispatch.conversation_id.clone(),
                    dispatch.provider,
                    dispatch.request_id.clone(),
                    dispatch.input.clone(),
                    dispatch.instructions.clone(),
                )
                .await
                .unwrap();
            host.claim_provider_request(domain(), request)
                .await
                .unwrap();
            let driver = host.clone();
            let saved = dispatch.clone();
            let ledger = native.clone();
            host.with_store(move |_| {
                if failed {
                    let mut terminal = event(
                        ExternalEventKind::Failed,
                        WorkProviderQualification::OutcomeOnly,
                        1,
                    );
                    terminal.conversation_id = saved.conversation_id.clone();
                    terminal.request_id = saved.request_id.clone();
                    driver.store.apply_native_command(
                        &domain(),
                        "second-failed".into(),
                        WorkGraphMutation::RecordProviderEvent {
                            event: Box::new(terminal),
                        },
                        provenance(),
                    )?;
                }
                assert!(
                    driver.resume_provider_plan_with(&ledger, &domain(), &saved, |_, _| false)?
                );
                assert!(!driver.resume_provider_plan_with(
                    &ledger,
                    &domain(),
                    &saved,
                    |_, _| false
                )?);
                let next = driver
                    .store
                    .provider_dispatch(&domain(), "third", "third-stage")?;
                if failed {
                    assert!(next.is_none());
                    assert!(
                        driver
                            .store
                            .provider_dispatch(
                                &domain(),
                                &saved.conversation_id,
                                &saved.request_id
                            )?
                            .unwrap()
                            .closed_reason
                            .is_some()
                    );
                } else {
                    let next = next.unwrap().dispatch;
                    assert_eq!(next.target_digest, "f".repeat(64));
                    assert!(next.remaining_stages.is_empty());
                    assert!(!driver.provider_dispatch_ready_with(
                        &ledger,
                        &domain(),
                        &next,
                        |_, _| false
                    )?);
                }
                Ok(())
            })
            .await
            .unwrap();
        }
    }
}
