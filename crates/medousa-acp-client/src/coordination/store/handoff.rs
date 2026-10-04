//! Native sender contracts. Worker outcomes never rewrite responsibility or waive review.
use super::{
    CoordinationStore,
    intake::{OwnerEventIntakeClaim, OwnerIntakeLease},
    object_path,
};
use anyhow::{Result, bail};
use medousa_types::{
    SessionId, SessionRef, coordination::*, work_handoff::*, work_unit::WorkContactPreference,
};
use sha2::{Digest, Sha256};

impl CoordinationStore {
    pub fn handoff(
        &self,
        channel: &CoordinationChannelRef,
        id: &str,
    ) -> Result<Option<PeerHandoffRecord>> {
        let path = object_path(channel, "handoff", id)?;
        match self.root.metadata(&path) {
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e.into()),
            Ok(_) => {
                let record: PeerHandoffRecord = self.read(&path)?;
                if record.request.channel != *channel || record.request.assignment_id != id {
                    bail!("handoff identity mismatch");
                }
                self.require_owner(channel, &record.request.owner_principal_id)?;
                Ok(Some(record))
            }
        }
    }
    pub fn record_handoff(&self, record: &PeerHandoffRecord) -> Result<bool> {
        let r = &record.request;
        self.require_owner(&r.channel, &r.owner_principal_id)?;
        let proposal = self
            .proposal_for_assignment(&r.channel, &r.assignment_id)?
            .ok_or_else(|| anyhow::anyhow!("handoff requires its immutable assignment snapshot"))?;
        if proposal.request != *r
            || record.expires_at != proposal.expires_at
            || record.source.session != r.owner_session
            || record.source.entry_seq == 0
            || record.source_digest.trim().is_empty()
            || !r.context.sources.iter().any(|s| {
                s.selection.session == record.source.session
                    && record.source.entry_seq > s.selection.after_entry_seq.unwrap_or(0)
                    && record.source.entry_seq <= s.selection.through_entry_seq
            })
        {
            bail!("handoff source or assignment differs from the native snapshot");
        }
        self.create(
            &object_path(&r.channel, "handoff", &r.assignment_id)?,
            record,
        )
    }
    pub fn handoff_view(
        &self,
        channel: &CoordinationChannelRef,
        id: &str,
    ) -> Result<Option<PeerHandoffView>> {
        let Some(handoff) = self.handoff(channel, id)? else {
            return Ok(None);
        };
        let binding = self.peer_if_recorded(channel, id)?;
        let receipt = self.receipt_if_recorded(channel, id)?;
        let accepted = self.handoff_acceptance(&handoff)?.is_some()
            || receipt
                .as_ref()
                .is_some_and(|r| r.outcome == PeerAssignmentOutcome::Completed);
        let path = object_path(channel, "handoff-review", id)?;
        let review: Option<PeerHandoffReview> = match self.root.metadata(&path) {
            Ok(_) => Some(self.read(&path)?),
            Err(e) if e.is_not_found() => None,
            Err(e) => return Err(e.into()),
        };
        let state = match (&receipt, &review) {
            (Some(r), _) if r.outcome != PeerAssignmentOutcome::Completed => {
                PeerHandoffState::Failed
            }
            (_, Some(r)) => match r.verdict {
                PeerHandoffVerdict::Accept => PeerHandoffState::Accepted,
                PeerHandoffVerdict::ChangesRequested => PeerHandoffState::ChangesRequested,
            },
            (Some(_), None) if handoff.policy.completion == HandoffCompletion::SenderReview => {
                PeerHandoffState::AwaitingSenderReview
            }
            (Some(_), None) => PeerHandoffState::Accepted,
            (None, _) if accepted => PeerHandoffState::Working,
            _ => PeerHandoffState::AwaitingAcceptance,
        };
        // A dispatch attempt alone is never an ownership transfer.
        let responsible_session =
            if accepted && handoff.policy.responsibility == HandoffResponsibility::Transfer {
                handoff.request.execution_session.clone()
            } else {
                handoff.request.owner_session.clone()
            };
        Ok(Some(PeerHandoffView {
            handoff,
            state,
            responsible_session,
            binding,
            receipt,
            review,
        }))
    }
    pub fn handoff_summary(
        &self,
        channel: &CoordinationChannelRef,
        id: &str,
    ) -> Result<Option<PeerHandoffSummary>> {
        Ok(self.handoff_view(channel, id)?.map(Into::into))
    }
    pub fn review_handoff(
        &self,
        channel: &CoordinationChannelRef,
        id: &str,
        sender: &SessionRef,
        owner: &str,
        decision: &PeerHandoffReview,
    ) -> Result<bool> {
        let record = self
            .handoff(channel, id)?
            .ok_or_else(|| anyhow::anyhow!("handoff unavailable"))?;
        let receipt = self.receipt(channel, id)?;
        if record.request.owner_principal_id != owner
            || record.request.owner_session != *sender
            || decision.sender_session != *sender
            || decision.reason.trim().is_empty()
            || decision.reason.len() > 4096
            || decision.turn_id.trim().is_empty()
            || record.policy.completion != HandoffCompletion::SenderReview
            || receipt.receipt_id != decision.receipt_id
            || receipt.outcome != PeerAssignmentOutcome::Completed
        {
            bail!("only the sender may review the exact completed handoff requiring review");
        }
        let path = object_path(channel, "handoff-review", id)?;
        match self.create(&path, decision) {
            Ok(created) => Ok(created),
            Err(error) => {
                let saved: PeerHandoffReview = self.read(&path)?;
                if saved.receipt_id == decision.receipt_id
                    && saved.verdict == decision.verdict
                    && saved.reason == decision.reason
                    && saved.sender_session == decision.sender_session
                {
                    Ok(false) // Preserve the first decision's attributed turn on an exact semantic retry.
                } else {
                    Err(error)
                }
            }
        }
    }
    fn handoff_acceptance(&self, record: &PeerHandoffRecord) -> Result<Option<OwnerEvent>> {
        let id = format!("handoff-accepted:{}", record.request.assignment_id);
        let path = object_path(&record.request.channel, "owner-event", &id)?;
        match self.root.metadata(&path) {
            Ok(_) => {
                let event = self.owner_event(&record.request.channel, &id)?;
                let OwnerEventPayload::AssignmentAccepted { binding } = &event.payload else {
                    bail!("invalid handoff acceptance payload");
                };
                if event.owner_session != record.request.owner_session
                    || event.owner_principal_id != record.request.owner_principal_id
                    || binding.assignment_id != record.request.assignment_id
                    || self
                        .peer_if_recorded(&record.request.channel, &record.request.assignment_id)?
                        .as_ref()
                        != Some(binding.as_ref())
                {
                    bail!("handoff acceptance identity mismatch");
                }
                Ok(Some(event))
            }
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn record_handoff_acceptance(
        &self,
        binding: &ExternalPeerAssignmentBinding,
    ) -> Result<bool> {
        let Some(record) = self.handoff(&binding.channel, &binding.assignment_id)? else {
            return Ok(false);
        };
        if self
            .peer_if_recorded(&binding.channel, &binding.assignment_id)?
            .as_ref()
            != Some(binding)
        {
            bail!("handoff acceptance requires committed native custody");
        }
        if self.handoff_acceptance(&record)?.is_some() {
            return Ok(false);
        }
        let event = OwnerEvent {
            schema_version: 1,
            event_id: format!("handoff-accepted:{}", binding.assignment_id),
            owner_principal_id: record.request.owner_principal_id.clone(),
            owner_session: record.request.owner_session.clone(),
            channel: binding.channel.clone(),
            source: OwnerEventSource::Assignment {
                assignment_id: binding.assignment_id.clone(),
            },
            occurred_at: chrono::Utc::now(),
            payload: OwnerEventPayload::AssignmentAccepted {
                binding: Box::new(binding.clone()),
            },
            limits: OwnerContinuationLimits::default(),
        };
        match self.record_owner_event(&event) {
            Ok(created) => Ok(created),
            Err(error) => {
                let saved = self.handoff_acceptance(&record)?;
                let mut retry = event;
                if let Some(saved) = saved {
                    retry.occurred_at = saved.occurred_at;
                    if retry == saved {
                        return Ok(false);
                    }
                }
                Err(error)
            }
        }
    }
    pub fn handoff_wake_session(record: &PeerHandoffRecord) -> Result<SessionRef> {
        if record.policy.contact == WorkContactPreference::ReturnToOrigin {
            return Ok(record.request.owner_session.clone());
        }
        // Silent/alternate reporting routes keep internal callbacks out of the user's chat.
        // Contact delivery is separate native authority and is not performed by this wake.
        let hash = Sha256::digest(serde_json::to_vec(&(
            "handoff-wake-v1",
            &record.request.channel,
            &record.request.assignment_id,
        ))?);
        Ok(SessionRef {
            authority_id: record.request.owner_session.authority_id.clone(),
            session_id: SessionId::parse(format!("ses_handoff_{hash:x}"))?,
        })
    }
    pub fn require_handoff_event(&self, event: &OwnerEvent) -> Result<PeerHandoffRecord> {
        let (id, terminal) = match &event.payload {
            OwnerEventPayload::AssignmentAccepted { binding } => (&binding.assignment_id, false),
            OwnerEventPayload::AssignmentTerminal { assignment_id, .. } => (assignment_id, true),
            _ => bail!("not a native handoff lifecycle event"),
        };
        let record = self
            .handoff(&event.channel, id)?
            .ok_or_else(|| anyhow::anyhow!("handoff unavailable"))?;
        if record.request.owner_session != event.owner_session
            || record.request.owner_principal_id != event.owner_principal_id
        {
            bail!("handoff callback owner mismatch");
        }
        self.require_assignment_grant(&record.request, chrono::Utc::now())?;
        match &event.payload {
            OwnerEventPayload::AssignmentAccepted { binding } => {
                if self.handoff_acceptance(&record)?.as_ref() != Some(event)
                    || self.peer_if_recorded(&event.channel, id)?.as_ref() != Some(binding.as_ref())
                {
                    bail!("handoff acceptance evidence mismatch");
                }
            }
            OwnerEventPayload::AssignmentTerminal { receipt, .. } => {
                if self.receipt(&event.channel, id)? != **receipt {
                    bail!("handoff terminal evidence mismatch")
                }
            }
            _ => unreachable!(),
        }
        if record.expires_at <= chrono::Utc::now()
            || if terminal {
                !record.policy.wake_on_terminal
            } else {
                !record.policy.wake_on_accepted
            }
        {
            bail!("handoff callback disabled or expired");
        }
        Ok(record)
    }

    pub fn handoff_for_wake(
        &self,
        session: &SessionRef,
        turn_id: &str,
    ) -> Result<PeerHandoffRecord> {
        let entries = self.root.list_root_utf8()?;
        if entries.len() > 10_000 {
            bail!("handoff wake lookup budget exhausted")
        }
        for entry in entries {
            if !entry.name.starts_with("hf1-") {
                continue;
            }
            let record: PeerHandoffRecord =
                self.read(&medousa_store::StorePath::parse(&entry.name)?)?;
            if Self::handoff_wake_session(&record)? != *session {
                continue;
            }
            let mut events = Vec::new();
            if let Some(e) = self.handoff_acceptance(&record)? {
                events.push(e)
            }
            if let Some(r) =
                self.receipt_if_recorded(&record.request.channel, &record.request.assignment_id)?
            {
                events.push(self.peer_owner_event(&r)?)
            }
            for event in events {
                for attempt in 0..8 {
                    let path = object_path(
                        &event.channel,
                        "owner-event-attempt",
                        &format!("{}:{attempt}", event.event_id),
                    )?;
                    match self.root.metadata(&path) {
                        Ok(_) => {
                            let intake: OwnerEventIntakeAttempt = self.read(&path)?;
                            if intake.turn_id == turn_id && intake.event == event {
                                self.validate_event_attempt(&intake)?;
                                let rejected = object_path(
                                    &event.channel,
                                    "owner-event-rejected",
                                    &format!("{}:{attempt}", event.event_id),
                                )?;
                                match self.root.metadata(&rejected) {
                                    Ok(_) => bail!("handoff callback admission was rejected"),
                                    Err(e) if e.is_not_found() => {}
                                    Err(e) => return Err(e.into()),
                                }
                                if self.owner_event_ack(&event)?.is_some()
                                    || self.owner_event_blocked(&event)?.is_some()
                                {
                                    bail!("handoff callback admission already settled");
                                }
                                self.require_handoff_event(&event)?;
                                return Ok(record);
                            }
                        }
                        Err(e) if e.is_not_found() => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
        }
        bail!("no native handoff admission matches this callback turn")
    }

    pub fn try_handoff_event_lease(&self, event: &OwnerEvent) -> Result<Option<OwnerIntakeLease>> {
        let record = self.require_handoff_event(event)?;
        self.try_owner_session_intake_lease(
            &Self::handoff_wake_session(&record)?,
            &event.owner_principal_id,
        )
    }
    fn validate_handoff_lease(&self, event: &OwnerEvent, lease: &OwnerIntakeLease) -> Result<()> {
        let record = self.require_handoff_event(event)?;
        if !std::sync::Arc::ptr_eq(&self.intake_identity, &lease.store_identity)
            || lease.session != Self::handoff_wake_session(&record)?
            || lease.owner != event.owner_principal_id
        {
            bail!("handoff callback lease mismatch");
        }
        Ok(())
    }
    pub fn begin_handoff_event(
        &self,
        event: &OwnerEvent,
        lease: &OwnerIntakeLease,
    ) -> Result<OwnerEventIntakeClaim> {
        self.validate_handoff_lease(event, lease)?;
        // Persist legacy receipt projection before generic attempts/acknowledgments.
        self.record_owner_event(event)?;
        if let Some(ack) = self.owner_event_ack(event)? {
            return Ok(OwnerEventIntakeClaim::Consumed(ack));
        }
        for attempt in 0..8 {
            let key = format!("{}:{attempt}", event.event_id);
            let path = object_path(&event.channel, "owner-event-attempt", &key)?;
            let intake = OwnerEventIntakeAttempt {
                event: event.clone(),
                attempt,
                turn_id: format!(
                    "handoff_wake_{:x}",
                    Sha256::digest(serde_json::to_vec(&(&event.channel, &key))?)
                ),
            };
            match self.root.metadata(&path) {
                Ok(_) => {
                    let saved: OwnerEventIntakeAttempt = self.read(&path)?;
                    if saved != intake {
                        bail!("handoff wake attempt mismatch")
                    }
                    match self.root.metadata(&object_path(
                        &event.channel,
                        "owner-event-rejected",
                        &key,
                    )?) {
                        Ok(_) => {
                            let rejected: bool = self.read(&object_path(
                                &event.channel,
                                "owner-event-rejected",
                                &key,
                            )?)?;
                            if !rejected {
                                bail!("invalid handoff rejection marker");
                            }
                            continue;
                        }
                        Err(e) if e.is_not_found() => {
                            return Ok(OwnerEventIntakeClaim::Unresolved(saved));
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(e) if e.is_not_found() => {
                    return Ok(if self.create(&path, &intake)? {
                        OwnerEventIntakeClaim::Started(intake)
                    } else {
                        OwnerEventIntakeClaim::Unresolved(intake)
                    });
                }
                Err(e) => return Err(e.into()),
            }
        }
        bail!("handoff admission retry budget exhausted")
    }
    pub fn reject_handoff_event(
        &self,
        intake: &OwnerEventIntakeAttempt,
        lease: &OwnerIntakeLease,
    ) -> Result<bool> {
        self.validate_handoff_lease(&intake.event, lease)?;
        self.validate_event_attempt(intake)?;
        self.create(
            &object_path(
                &intake.event.channel,
                "owner-event-rejected",
                &format!("{}:{}", intake.event.event_id, intake.attempt),
            )?,
            &true,
        )
    }
    pub fn acknowledge_handoff_event(
        &self,
        ack: &OwnerEventIntakeAcknowledgment,
        lease: &OwnerIntakeLease,
    ) -> Result<bool> {
        self.validate_handoff_lease(&ack.intake.event, lease)?;
        self.validate_event_attempt(&ack.intake)?;
        if ack.decision.session != lease.session
            || ack.decision.entry_seq == 0
            || ack.decision_digest.trim().is_empty()
            || !ack.command_refs.is_empty()
            || ack.terminal_delivery_ref.is_some()
        {
            bail!("handoff acknowledgment requires the exact callback transcript");
        }
        self.create(
            &object_path(
                &ack.intake.event.channel,
                "owner-event-ack",
                &ack.intake.event.event_id,
            )?,
            ack,
        )
    }
}

#[cfg(test)]
mod tests;
