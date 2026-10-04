//! Admitted deterministic work intake. It never creates a model turn, changes
//! credentials, sends a provider request, or delivers a user notification.
use super::*;
use medousa_acp_client::coordination::store::CoordinationStore;
use medousa_types::{ExternalEventKind, work_provider::*};
use medousa_work::{COORDINATOR_ACTOR, CoordinatorInbox, CoordinatorInboxPage};
use sha2::{Digest, Sha256};

impl WorkUnitHost {
    pub(crate) async fn coordinator_inboxes(
        &self,
        after: Option<String>,
        limit: usize,
    ) -> Result<CoordinatorInboxPage> {
        let authority = crate::workshop_authority::current()
            .map_err(anyhow::Error::msg)?
            .clone();
        self.with_store(move |store| {
            Ok(store.coordinator_inboxes(&authority, limit, after.as_deref())?)
        })
        .await
    }

    pub(crate) fn consume_provider_inbox(&self, inbox: &CoordinatorInbox) -> Result<()> {
        let native =
            crate::daemon::coordination::local_coordination_host().map(|host| host.work_registry());
        self.consume_provider_inbox_with(
            inbox,
            native.as_deref(),
            crate::session_catalog::session_visible_to_profile,
        )
    }

    pub(super) fn consume_provider_inbox_with(
        &self,
        inbox: &CoordinatorInbox,
        native: Option<&CoordinationStore>,
        visible: impl Fn(&str, &str) -> bool,
    ) -> Result<()> {
        let page = self.store.events(
            &inbox.domain,
            COORDINATOR_ACTOR,
            WorkEventsQuery {
                subscription_id: inbox.subscription_id.clone(),
                limit: Some(1),
            },
        )?;
        let Some(event) = page.events.first() else {
            return Ok(());
        };
        if page.status == WorkSubscriptionStatus::Paused {
            return Ok(());
        }
        let WorkGraphMutation::RecordProviderEvent { event: provider } = &event.command.mutation
        else {
            bail!("runtime work inbox contains an unsupported event");
        };
        let record = self
            .store
            .provider_request(&inbox.domain, &inbox.conversation_id, &inbox.request_id)?
            .ok_or_else(|| anyhow::anyhow!("runtime provider request missing"))?;
        let request = &record.request;
        let current = self
            .store
            .work_unit(&inbox.domain, &request.input.work_unit_id)?;
        let belongs = provider.conversation_id == inbox.conversation_id
            && provider.request_id == inbox.request_id;
        let active = matches!(
            current.state,
            WorkUnitState::Accepted | WorkUnitState::Active | WorkUnitState::Waiting
        );
        let admitted = belongs
            && active
            && page.status != WorkSubscriptionStatus::Stopped
            && current.scope_revision == request.input.expected_scope_revision
            && self
                .store
                .peer_coordination_scope_digest(&inbox.domain, &current.work_unit_id)?
                == request.scope_digest
            && self.store.provider_stage_is_current(
                &inbox.domain,
                &inbox.conversation_id,
                &inbox.request_id,
            )?;
        let mut state = None;
        let mut reason = "Runtime retained provider evidence without advancing an unrelated, inactive or superseded stage".to_string();
        // Hold checkout custody until state and acknowledgment are committed.
        let _checkout = if admitted
            && provider.kind == ExternalEventKind::Completed
            && provider.qualification == WorkProviderQualification::ReviewApproved
            && request.input.deadline > chrono::Utc::now()
            && let Some(reviewed) = &request.reviewed
        {
            let id = medousa_forge::model::WorkId::parse_storage(&reviewed.forge_work_id)
                .map_err(anyhow::Error::msg)?;
            Some(
                self.forge
                    .store()
                    .try_lock_item(&id)?
                    .ok_or_else(|| anyhow::anyhow!("provider stage checkout custody busy"))?,
            )
        } else {
            None
        };
        if admitted {
            match provider.kind {
                ExternalEventKind::Progress => {
                    reason = "Runtime observed provider progress; stage remains pending".into()
                }
                ExternalEventKind::Question => {
                    state = Some(WorkUnitState::NeedsAttention);
                    reason =
                        "Provider stage requires an answer; no automatic reply or retry admitted"
                            .into();
                }
                ExternalEventKind::Failed => {
                    state = Some(WorkUnitState::NeedsAttention);
                    reason = "Provider stage failed; no replacement execution admitted".into();
                }
                ExternalEventKind::Completed => {
                    if request.input.deadline <= chrono::Utc::now() {
                        state = Some(WorkUnitState::NeedsAttention);
                        reason = "Provider completion expired before stage publication".into();
                    } else if request.reviewed.is_none()
                        && provider.qualification == WorkProviderQualification::OutcomeOnly
                    {
                        state = Some(WorkUnitState::Waiting);
                        reason = "Provider execution completed; an admitted exact revision review is still required".into();
                    } else {
                        state = Some(WorkUnitState::NeedsAttention);
                        reason = format!(
                            "Provider review requires attention: {:?}",
                            provider.qualification
                        );
                        if provider.qualification == WorkProviderQualification::ReviewApproved {
                            let native = native.ok_or_else(|| {
                                anyhow::anyhow!("native provider review source unavailable")
                            })?;
                            let source =
                                request.input.review_of.as_ref().ok_or_else(|| {
                                    anyhow::anyhow!("provider review source missing")
                                })?;
                            let pin = request
                                .reviewed
                                .as_ref()
                                .ok_or_else(|| anyhow::anyhow!("provider review pin missing"))?;
                            if provider_events::source_request_with_visibility(
                                native,
                                &inbox.domain,
                                source,
                                &visible,
                            )
                            .is_ok()
                                && provider_events::review_revision_held(
                                    &self.forge,
                                    &inbox.domain.user_id,
                                    pin,
                                )?
                            {
                                state = Some(WorkUnitState::Satisfied);
                                reason = "Provider review approved the exact native executor receipt and current clean checkout".into();
                            } else {
                                reason = "Provider review source visibility or checkout revision changed before publication".into();
                            }
                        }
                    }
                }
                _ => bail!("unsupported provider stage event"),
            }
        }
        self.store.apply_native_command_checked(&inbox.domain,
            format!("work-stage:{:x}", Sha256::digest(serde_json::to_vec(&(&inbox.subscription_id, event.receipt.revision))?)),
            WorkGraphMutation::AdvanceProviderStage { subscription_id: inbox.subscription_id.clone(), event_revision: event.receipt.revision, state, reason },
            RecordProvenance { actor_id: COORDINATOR_ACTOR.into(), source: RecordSource::SystemEvent, evidence: vec![] },
            |_| {
                (|| -> Result<()> {
                    if state.is_some() {
                        self.store.admit_peer_coordination(&inbox.domain, &request.input.work_unit_id, request.input.expected_scope_revision)?;
                        if self.store.peer_coordination_scope_digest(&inbox.domain, &current.work_unit_id)? != request.scope_digest
                            || !self.store.provider_stage_is_current(&inbox.domain, &inbox.conversation_id, &inbox.request_id)? { bail!("provider stage scope or custody changed during publication"); }
                        if native.is_some_and(|native| native.work_is_controlled(&inbox.domain, &current.work_unit_id).unwrap_or(true)) { bail!("provider stage cannot replace native controller custody"); }
                    }
                    if state == Some(WorkUnitState::Satisfied) {
                        let native = native.ok_or_else(|| anyhow::anyhow!("native provider review source unavailable"))?;
                        provider_events::source_request_with_visibility(native, &inbox.domain, request.input.review_of.as_ref().unwrap(), &visible)?;
                        if request.input.deadline <= chrono::Utc::now() || !provider_events::review_revision_held(&self.forge, &inbox.domain.user_id, request.reviewed.as_ref().unwrap())? { bail!("provider review expired or checkout revision changed during publication"); }
                    }
                    Ok(())
                })().map_err(|error| medousa_store::PersistenceError::new(medousa_store::PersistenceErrorKind::Conflict, error.to_string()))
            })?;
        Ok(())
    }
}
