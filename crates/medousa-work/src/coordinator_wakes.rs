//! Durable admission and attempt identity for one result-only model wake.
use super::*;
use medousa_types::{ExternalEventKind, SessionId, SessionRef, work_coordinator::*};

pub(super) fn wake_key(conversation: &str, request: &str) -> Result<String> {
    identifier(conversation)?;
    identifier(request)?;
    digest(&("work-coordinator-wake-v1", conversation, request))
}

pub fn coordinator_session(
    domain: &UserDomainRef,
    conversation: &str,
    request: &str,
) -> Result<SessionRef> {
    let id = digest(&(domain, conversation, request))?;
    Ok(SessionRef {
        authority_id: domain.authority_id.clone(),
        session_id: SessionId::parse(format!("{WORK_COORDINATOR_SESSION_PREFIX}{id}"))
            .map_err(|e| invalid(e.to_string()))?,
    })
}

pub fn coordinator_turn_id(
    domain: &UserDomainRef,
    conversation: &str,
    request: &str,
) -> Result<String> {
    Ok(format!(
        "{WORK_COORDINATOR_TURN_PREFIX}{}",
        digest(&(domain, conversation, request))?
    ))
}

fn native(provenance: &RecordProvenance) -> Result<()> {
    if provenance.source != RecordSource::SystemEvent || provenance.actor_id != COORDINATOR_ACTOR {
        return Err(invalid(
            "model wake admission and attempts require native coordinator custody",
        ));
    }
    Ok(())
}

impl WorkGraphStore {
    pub fn coordinator_wake(
        &self,
        domain: &UserDomainRef,
        conversation: &str,
        request: &str,
    ) -> Result<Option<WorkCoordinatorWakeRecord>> {
        Ok(self
            .load(domain)?
            .coordinator_wakes
            .get(&wake_key(conversation, request)?)
            .cloned())
    }

    /// Recheck the frozen model admission immediately before ticket creation.
    /// Parent budgets can change without changing this unit's scope generation.
    pub fn require_coordinator_wake_admission(
        &self,
        domain: &UserDomainRef,
        wake: &WorkCoordinatorWake,
    ) -> Result<()> {
        self.load(domain)?.admit_wake(wake, true)
    }

    pub fn try_coordinator_wake_lease(
        &self,
        domain: &UserDomainRef,
        conversation: &str,
        request: &str,
    ) -> Result<Option<std::fs::File>> {
        let path = StorePath::parse(&format!(
            "{}.wake-lock",
            digest(&(domain, wake_key(conversation, request)?))?
        ))?;
        let lease = self.transaction.root().open_lock_file(&path)?;
        match lease.try_lock_exclusive() {
            Ok(()) => Ok(Some(lease)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(invalid(format!("model wake lease unavailable: {error}"))),
        }
    }
}

impl Snapshot {
    pub(super) fn wake_pending(&self, record: &WorkCoordinatorWakeRecord) -> bool {
        record.decision.is_none()
            && record.blocked_reason.is_none()
            && (record.attempt.is_some()
                || record.wake.input.deadline <= Utc::now()
                || self
                    .work_units
                    .get(&record.wake.input.work_unit_id)
                    .is_some_and(|unit| {
                        unit.state == WorkUnitState::Cancelled
                            || unit.scope_revision != record.wake.input.expected_scope_revision
                    })
                || self.provider_requests.values().any(|request| {
                    request.request.conversation_id == record.wake.conversation_id
                        && request.request.request_id == record.wake.request_id
                        && request.events.iter().any(|event| {
                            matches!(
                                event.kind,
                                ExternalEventKind::Completed | ExternalEventKind::Failed
                            )
                        })
                }))
    }

    fn validate_wake(&self, wake: &WorkCoordinatorWake) -> Result<()> {
        wake_key(&wake.conversation_id, &wake.request_id)?;
        let request = self
            .provider_requests
            .values()
            .find(|record| {
                record.request.conversation_id == wake.conversation_id
                    && record.request.request_id == wake.request_id
            })
            .ok_or_else(|| invalid("model wake requires an exact provider request"))?;
        if request.request.input != wake.input
            || request.request.scope_digest != wake.scope_digest
            || wake.session
                != coordinator_session(&self.domain, &wake.conversation_id, &wake.request_id)?
        {
            return Err(invalid("model wake differs from exact work/provider scope"));
        }
        for value in [&wake.provider, &wake.model, &wake.response_depth_mode] {
            text(value, 256)?;
        }
        if wake.reasoning_effort.len() > 64 || wake.reasoning_effort.chars().any(char::is_control) {
            return Err(invalid("invalid wake reasoning configuration"));
        }
        Ok(())
    }

    fn admit_wake(&self, wake: &WorkCoordinatorWake, allow_satisfied: bool) -> Result<()> {
        self.validate_wake(wake)?;
        let unit = &self.work_units[&wake.input.work_unit_id];
        let state_admitted = matches!(
            unit.state,
            WorkUnitState::Accepted
                | WorkUnitState::Active
                | WorkUnitState::Waiting
                | WorkUnitState::NeedsAttention
        ) || (allow_satisfied && unit.state == WorkUnitState::Satisfied);
        if unit.scope_revision != wake.input.expected_scope_revision
            || unit.kind != WorkUnitKind::Finite
            || !unit.scope.children.is_empty()
            || !unit.scope.depends_on.is_empty()
            || !state_admitted
            || wake.input.deadline <= Utc::now()
        {
            return Err(invalid(
                "model wake scope or deadline is no longer admitted",
            ));
        }
        let resources = unit
            .scope
            .resources
            .iter()
            .map(|reference| {
                let record = self
                    .resources
                    .get(&reference_key(reference)?)
                    .ok_or_else(|| invalid("wake scope resource missing"))?;
                Ok((
                    reference,
                    record.revision,
                    record.resolution,
                    &record.native_revision,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        if digest(&(unit.scope_revision, &unit.scope, resources))? != wake.scope_digest {
            return Err(invalid("model wake resource versions changed"));
        }
        let mut pending = vec![unit.work_unit_id.clone()];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id.clone()) {
                continue;
            }
            if self.work_units[&id].budget.is_some() {
                return Err(invalid(
                    "unmetered model wakes cannot consume budgeted work",
                ));
            }
            pending.extend(
                self.work_units
                    .iter()
                    .filter(|(_, unit)| unit.scope.children.contains(&id))
                    .map(|(id, _)| id.clone()),
            );
        }
        Ok(())
    }

    pub(super) fn register_coordinator_wake(
        &mut self,
        wake: WorkCoordinatorWake,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        native(provenance)?;
        self.admit_wake(&wake, false)?;
        let key = wake_key(&wake.conversation_id, &wake.request_id)?;
        if self.coordinator_wakes.contains_key(&key) {
            return Err(invalid(
                "model wake already admitted; replay original native command",
            ));
        }
        self.coordinator_wakes.insert(
            key,
            WorkCoordinatorWakeRecord {
                wake,
                attempt: None,
                decision: None,
                blocked_reason: None,
                revision: self.revision,
            },
        );
        Ok(())
    }

    pub(super) fn claim_coordinator_wake(
        &mut self,
        conversation: &str,
        request: &str,
        attempt: WorkCoordinatorAttempt,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        native(provenance)?;
        let key = wake_key(conversation, request)?;
        let record = self
            .coordinator_wakes
            .get(&key)
            .ok_or_else(|| invalid("model wake has no native admission"))?;
        self.admit_wake(&record.wake, true)?;
        if record.attempt.is_some() || record.decision.is_some() || record.blocked_reason.is_some()
        {
            return Err(invalid(
                "model wake attempt already owns custody; reconcile without relaunch",
            ));
        }
        if attempt.turn_id != coordinator_turn_id(&self.domain, conversation, request)?
            || attempt.prompt_digest.len() != 64
            || !attempt.prompt_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("invalid model wake attempt identity"));
        }
        if !self.provider_stage_is_current(conversation, request)?
            || !self.provider_requests.values().any(|record| {
                record.request.conversation_id == conversation
                    && record.request.request_id == request
                    && record.dispatch_claimed
                    && record.events.iter().any(|event| {
                        event.event_id == attempt.event_id
                            && matches!(
                                event.kind,
                                ExternalEventKind::Completed | ExternalEventKind::Failed
                            )
                    })
            })
        {
            return Err(invalid(
                "model wake lacks the exact current provider terminal",
            ));
        }
        let record = self.coordinator_wakes.get_mut(&key).unwrap();
        record.attempt = Some(attempt);
        record.revision = self.revision;
        Ok(())
    }

    pub(super) fn complete_coordinator_wake(
        &mut self,
        conversation: &str,
        request: &str,
        decision: WorkCoordinatorDecision,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        native(provenance)?;
        let key = wake_key(conversation, request)?;
        let record = self
            .coordinator_wakes
            .get_mut(&key)
            .ok_or_else(|| invalid("model wake admission missing"))?;
        if record.attempt.is_none()
            || record.decision.is_some()
            || record.blocked_reason.is_some()
            || decision.entry.session != record.wake.session
            || decision.entry.entry_seq == 0
        {
            return Err(invalid(
                "model wake decision lacks exact claimed execution custody",
            ));
        }
        text(&decision.content_digest, 256)?;
        record.decision = Some(decision);
        record.revision = self.revision;
        Ok(())
    }

    pub(super) fn block_coordinator_wake(
        &mut self,
        conversation: &str,
        request: &str,
        reason: &str,
        provenance: &RecordProvenance,
    ) -> Result<()> {
        native(provenance)?;
        text(reason, 4096)?;
        let record = self
            .coordinator_wakes
            .get_mut(&wake_key(conversation, request)?)
            .ok_or_else(|| invalid("model wake admission missing"))?;
        if record.decision.is_some() || record.blocked_reason.is_some() {
            return Err(invalid("model wake is already reconciled or blocked"));
        }
        record.blocked_reason = Some(reason.into());
        record.revision = self.revision;
        Ok(())
    }

    pub(super) fn validate_coordinator_wakes(&self) -> Result<()> {
        for (key, record) in &self.coordinator_wakes {
            self.validate_wake(&record.wake)?;
            if *key != wake_key(&record.wake.conversation_id, &record.wake.request_id)?
                || record.revision == 0
                || record.revision > self.revision
            {
                return Err(invalid("invalid retained model wake"));
            }
            if let Some(attempt) = &record.attempt {
                identifier(&attempt.event_id)?;
                if attempt.turn_id
                    != coordinator_turn_id(
                        &self.domain,
                        &record.wake.conversation_id,
                        &record.wake.request_id,
                    )?
                    || attempt.prompt_digest.len() != 64
                    || !attempt.prompt_digest.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(invalid("invalid retained model wake attempt"));
                }
                if !self.provider_requests.values().any(|request| {
                    request.request.conversation_id == record.wake.conversation_id
                        && request.request.request_id == record.wake.request_id
                        && request.dispatch_claimed
                        && request.events.iter().any(|event| {
                            event.event_id == attempt.event_id
                                && matches!(
                                    event.kind,
                                    ExternalEventKind::Completed | ExternalEventKind::Failed
                                )
                        })
                }) {
                    return Err(invalid(
                        "retained model wake attempt lacks exact provider terminal custody",
                    ));
                }
            }
            if let Some(decision) = &record.decision {
                if record.attempt.is_none()
                    || record.blocked_reason.is_some()
                    || decision.entry.session != record.wake.session
                    || decision.entry.entry_seq == 0
                {
                    return Err(invalid("invalid retained model wake decision"));
                }
                text(&decision.content_digest, 256)?;
            }
            if let Some(reason) = &record.blocked_reason {
                text(reason, 4096)?;
            }
        }
        Ok(())
    }
}
