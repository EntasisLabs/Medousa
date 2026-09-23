//! Source-neutral durable owner events. Producers must authenticate and verify
//! the native evidence before recording; the store provides replay fencing and
//! serialized, crash-aware continuation state.

use super::{CoordinationStore, object_path};
use crate::coordination::store::intake::{OwnerEventIntakeClaim, OwnerIntakeLease};
use anyhow::{Result, bail};
use medousa_types::AuthorityId;
use medousa_types::coordination::{
    OwnerEvent, OwnerEventBlocked, OwnerEventIntakeAcknowledgment, OwnerEventIntakeAttempt,
    OwnerEventPayload, OwnerEventSource, OwnerEventStatus, OwnerEventView,
};
use sha2::{Digest, Sha256};

const MAX_OWNER_EVENTS: usize = 10_000;
const MAX_EVENT_BYTES: usize = 256 * 1024;
const MAX_ATTEMPTS: u32 = 8;

/// Stable global ordering key because event ids are only unique within a channel.
pub fn owner_event_cursor_key(event: &OwnerEvent) -> String {
    let mut hash = Sha256::new();
    for part in [
        event.owner_session.authority_id.as_str(),
        event.channel.authority_id.as_str(),
        event.channel.channel_id.as_str(),
        event.event_id.as_str(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    format!("owner_event_{:x}", hash.finalize())
}

impl CoordinationStore {
    /// Persist an already-authorized native event before notifying its owner.
    /// The event id is immutable within a coordination channel.
    pub fn record_owner_event(&self, event: &OwnerEvent) -> Result<bool> {
        validate_event_shape(event)?;
        self.require_owner(&event.channel, &event.owner_principal_id)?;
        if serde_json::to_vec(event)?.len() > MAX_EVENT_BYTES {
            bail!("owner event exceeds size limit");
        }
        self.create(
            &object_path(&event.channel, "owner-event", &event.event_id)?,
            event,
        )
    }

    pub fn owner_event(
        &self,
        channel: &medousa_types::coordination::CoordinationChannelRef,
        event_id: &str,
    ) -> Result<OwnerEvent> {
        let event: OwnerEvent = self.read(&object_path(channel, "owner-event", event_id)?)?;
        validate_event_shape(&event)?;
        if event.channel != *channel || event.event_id != event_id {
            bail!("owner event identity mismatch");
        }
        Ok(event)
    }

    /// Bounded local recovery page. Sources whose native authority is not yet
    /// wired to continuation can be marked blocked and inspected separately.
    pub fn pending_local_owner_event_records(
        &self,
        authority: &AuthorityId,
        limit: usize,
        after_event_id: Option<&str>,
    ) -> Result<Vec<OwnerEvent>> {
        if !(1..=256).contains(&limit) {
            bail!("invalid owner event recovery limit");
        }
        let entries = self.root.list_root_utf8()?;
        if entries.len() > MAX_OWNER_EVENTS {
            bail!("owner event recovery scan budget exhausted");
        }
        let mut pending = std::collections::BTreeMap::new();
        for entry in entries {
            if !entry.name.starts_with("oe1-") {
                continue;
            }
            let event: OwnerEvent = match self.read(&medousa_store::StorePath::parse(&entry.name)?)
            {
                Ok(event) => event,
                Err(error) => {
                    tracing::warn!(record = %entry.name, %error, "skipping poisoned owner event record during bounded recovery scan");
                    continue;
                }
            };
            if let Err(error) = validate_event_shape(&event) {
                tracing::warn!(record = %entry.name, %error, "skipping invalid owner event record during bounded recovery scan");
                continue;
            }
            if event.owner_session.authority_id != *authority
                || after_event_id
                    .is_some_and(|after| owner_event_cursor_key(&event).as_str() <= after)
            {
                continue;
            }
            let is_pending = (|| -> Result<bool> {
                if self.owner_event(&event.channel, &event.event_id)? != event {
                    bail!("owner event recovery identity mismatch");
                }
                Ok(self.owner_event_ack(&event)?.is_none()
                    && self.owner_event_blocked(&event)?.is_none())
            })();
            match is_pending {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(record = %entry.name, %error, "skipping poisoned owner event state during bounded recovery scan");
                    continue;
                }
            }
            {
                pending.insert(owner_event_cursor_key(&event), event);
                if pending.len() > limit {
                    pending.pop_last();
                }
            }
        }
        let page: Vec<_> = pending.into_values().collect();
        if serde_json::to_vec(&page)?.len() > 1024 * 1024 {
            bail!("owner event recovery page byte budget exhausted");
        }
        Ok(page)
    }

    pub fn list_owner_events(
        &self,
        authority: &AuthorityId,
        owner_principal_id: &str,
        limit: usize,
        after_cursor: Option<&str>,
    ) -> Result<Vec<OwnerEventView>> {
        if !(1..=256).contains(&limit) {
            bail!("invalid owner event query limit");
        }
        let entries = self.root.list_root_utf8()?;
        if entries.len() > MAX_OWNER_EVENTS {
            bail!("owner event query scan budget exhausted");
        }
        let mut page = std::collections::BTreeMap::new();
        for entry in entries {
            let (event, legacy_receipt) = if entry.name.starts_with("oe1-") {
                let event: OwnerEvent = match self
                    .read(&medousa_store::StorePath::parse(&entry.name)?)
                {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::warn!(record = %entry.name, %error, "skipping poisoned owner event record in bounded owner query");
                        continue;
                    }
                };
                (event, None)
            } else if entry.name.starts_with("r1-") {
                let receipt: medousa_types::coordination::ExternalPeerAssignmentReceipt = match self
                    .read(&medousa_store::StorePath::parse(&entry.name)?)
                {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        tracing::warn!(record = %entry.name, %error, "skipping poisoned peer receipt in bounded owner query");
                        continue;
                    }
                };
                match self.peer_owner_event(&receipt) {
                    Ok(event) => (event, Some(receipt)),
                    Err(error) => {
                        tracing::warn!(record = %entry.name, %error, "skipping invalid peer receipt in bounded owner query");
                        continue;
                    }
                }
            } else {
                continue;
            };
            if event.owner_principal_id != owner_principal_id
                || event.owner_session.authority_id != *authority
            {
                continue;
            }
            let cursor = owner_event_cursor_key(&event);
            if after_cursor.is_some_and(|after| cursor.as_str() <= after) {
                continue;
            }
            let view = match (|| -> Result<OwnerEventView> {
                if let Some(receipt) = legacy_receipt {
                    if self.receipt(&receipt.binding.channel, &receipt.binding.assignment_id)?
                        != receipt
                    {
                        bail!("peer receipt query identity mismatch");
                    }
                    if let Some(ack) = self.owner_ack(&receipt)? {
                        let intake = OwnerEventIntakeAttempt {
                            event: event.clone(),
                            attempt: ack.intake.attempt,
                            turn_id: ack.intake.turn_id,
                        };
                        return Ok(OwnerEventView {
                            event,
                            status: OwnerEventStatus::Consumed,
                            latest_attempt: Some(intake.clone()),
                            acknowledgment: Some(OwnerEventIntakeAcknowledgment {
                                intake,
                                decision: ack.decision,
                                decision_digest: ack.decision_digest,
                                command_refs: Vec::new(),
                                terminal_delivery_ref: None,
                            }),
                            blocked: None,
                        });
                    }
                    for attempt in (0..MAX_ATTEMPTS).rev() {
                        let key = format!("{}:{attempt}", receipt.receipt_id);
                        let path = object_path(&receipt.binding.channel, "owner-attempt", &key)?;
                        let peer_attempt: medousa_types::coordination::PeerOwnerIntakeAttempt =
                            match self.read(&path) {
                                Ok(intake) => intake,
                                Err(error)
                                    if error
                                        .downcast_ref::<medousa_store::StoreRootError>()
                                        .is_some_and(|error| error.is_not_found()) =>
                                {
                                    continue;
                                }
                                Err(error) => return Err(error),
                            };
                        if peer_attempt.receipt != receipt || peer_attempt.attempt != attempt {
                            bail!("peer owner attempt query identity mismatch");
                        }
                        match self.root.metadata(&object_path(
                            &receipt.binding.channel,
                            "owner-rejected",
                            &key,
                        )?) {
                            Ok(_) => {
                                if !self.read::<bool>(&object_path(
                                    &receipt.binding.channel,
                                    "owner-rejected",
                                    &key,
                                )?)? {
                                    bail!("invalid rejected peer owner attempt marker");
                                }
                            }
                            Err(error) if error.is_not_found() => {
                                let intake = OwnerEventIntakeAttempt {
                                    event: event.clone(),
                                    attempt,
                                    turn_id: peer_attempt.turn_id,
                                };
                                return Ok(OwnerEventView {
                                    event,
                                    status: OwnerEventStatus::Started,
                                    latest_attempt: Some(intake),
                                    acknowledgment: None,
                                    blocked: None,
                                });
                            }
                            Err(error) => return Err(error.into()),
                        }
                    }
                    return Ok(OwnerEventView {
                        event,
                        status: OwnerEventStatus::Pending,
                        latest_attempt: None,
                        acknowledgment: None,
                        blocked: None,
                    });
                }
                if self.owner_event(&event.channel, &event.event_id)? != event {
                    bail!("owner event query identity mismatch");
                }
                if let Some(acknowledgment) = self.owner_event_ack(&event)? {
                    return Ok(OwnerEventView {
                        event,
                        status: OwnerEventStatus::Consumed,
                        latest_attempt: Some(acknowledgment.intake.clone()),
                        acknowledgment: Some(acknowledgment),
                        blocked: None,
                    });
                }
                if let Some(blocked) = self.owner_event_blocked(&event)? {
                    return Ok(OwnerEventView {
                        event,
                        status: OwnerEventStatus::Blocked,
                        latest_attempt: None,
                        acknowledgment: None,
                        blocked: Some(blocked),
                    });
                }
                let attempt_count = event
                    .limits
                    .retry_count
                    .min(event.limits.wake_count)
                    .min(MAX_ATTEMPTS as u16);
                for attempt in (0..attempt_count).rev() {
                    let key = format!("{}:{attempt}", event.event_id);
                    let path = object_path(&event.channel, "owner-event-attempt", &key)?;
                    let intake: OwnerEventIntakeAttempt = match self.read(&path) {
                        Ok(intake) => intake,
                        Err(error)
                            if error
                                .downcast_ref::<medousa_store::StoreRootError>()
                                .is_some_and(|error| error.is_not_found()) =>
                        {
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    if intake.event != event || intake.attempt != u32::from(attempt) {
                        bail!("owner event attempt query identity mismatch");
                    }
                    match self.root.metadata(&object_path(
                        &event.channel,
                        "owner-event-rejected",
                        &key,
                    )?) {
                        Ok(_) => continue,
                        Err(error) if error.is_not_found() => {
                            return Ok(OwnerEventView {
                                event,
                                status: OwnerEventStatus::Started,
                                latest_attempt: Some(intake),
                                acknowledgment: None,
                                blocked: None,
                            });
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Ok(OwnerEventView {
                    event,
                    status: OwnerEventStatus::Pending,
                    latest_attempt: None,
                    acknowledgment: None,
                    blocked: None,
                })
            })() {
                Ok(view) => view,
                Err(error) => {
                    tracing::warn!(record = %entry.name, %error, "skipping poisoned owner event state in bounded owner query");
                    continue;
                }
            };
            page.insert(cursor, view);
            if page.len() > limit {
                page.pop_last();
            }
        }
        let views: Vec<_> = page.into_values().collect();
        if serde_json::to_vec(&views)?.len() > 1024 * 1024 {
            bail!("owner event query page byte budget exhausted");
        }
        Ok(views)
    }

    pub fn begin_durable_owner_event_intake(
        &self,
        event: &OwnerEvent,
        lease: &OwnerIntakeLease,
    ) -> Result<OwnerEventIntakeClaim> {
        let OwnerEventPayload::AssignmentTerminal { .. } = &event.payload else {
            bail!("owner event source lacks a separate event-scoped continuation grant");
        };
        self.begin_owner_event_intake(event, lease)
    }

    /// Retry only after a known rejection before canonical turn execution.
    pub fn reject_durable_owner_event_admission(
        &self,
        intake: &OwnerEventIntakeAttempt,
        lease: &OwnerIntakeLease,
    ) -> Result<()> {
        let OwnerEventPayload::AssignmentTerminal { receipt, .. } = &intake.event.payload else {
            bail!("owner event source lacks a separate event-scoped continuation grant");
        };
        if self.peer_owner_event(receipt)? != intake.event {
            bail!("owner event identity mismatch");
        }
        let peer_intake = medousa_types::coordination::PeerOwnerIntakeAttempt {
            receipt: (**receipt).clone(),
            attempt: intake.attempt,
            turn_id: intake.turn_id.clone(),
        };
        self.reject_owner_admission(&peer_intake, lease)
    }

    /// Persist a committed, execution-correlated owner decision after the host
    /// has checked transcript content and any resulting command/delivery refs.
    pub fn acknowledge_durable_owner_event(
        &self,
        ack: &OwnerEventIntakeAcknowledgment,
        lease: &OwnerIntakeLease,
    ) -> Result<bool> {
        let OwnerEventPayload::AssignmentTerminal { receipt, .. } = &ack.intake.event.payload
        else {
            bail!("owner event source lacks a separate event-scoped continuation grant");
        };
        if !ack.command_refs.is_empty()
            || ack.terminal_delivery_ref.is_some()
            || self.peer_owner_event(receipt)? != ack.intake.event
        {
            bail!("owner event acknowledgment is not an exact peer-terminal decision");
        }
        self.acknowledge_owner_intake(
            &medousa_types::coordination::PeerOwnerIntakeAcknowledgment {
                intake: medousa_types::coordination::PeerOwnerIntakeAttempt {
                    receipt: (**receipt).clone(),
                    attempt: ack.intake.attempt,
                    turn_id: ack.intake.turn_id.clone(),
                },
                decision: ack.decision.clone(),
                decision_digest: ack.decision_digest.clone(),
            },
            lease,
        )
    }

    /// Unsupported or exhausted events remain durable and inspectable, and no
    /// longer occupy every recovery page.
    pub fn block_owner_event(
        &self,
        channel: &medousa_types::coordination::CoordinationChannelRef,
        blocked: &OwnerEventBlocked,
    ) -> Result<bool> {
        let event = self.owner_event(channel, &blocked.event_id)?;
        if blocked.reason.trim().is_empty() || blocked.reason.len() > 2048 {
            bail!("invalid owner event block reason");
        }
        match self.create(
            &object_path(&event.channel, "owner-event-blocked", &blocked.event_id)?,
            blocked,
        ) {
            Ok(created) => Ok(created),
            Err(error) => match self.owner_event_blocked(&event)? {
                Some(existing)
                    if existing.event_id == blocked.event_id
                        && existing.reason == blocked.reason
                        && existing.requires_user_decision == blocked.requires_user_decision =>
                {
                    Ok(false)
                }
                _ => Err(error),
            },
        }
    }

    pub fn owner_event_blocked_record(
        &self,
        channel: &medousa_types::coordination::CoordinationChannelRef,
        event_id: &str,
    ) -> Result<Option<OwnerEventBlocked>> {
        let event = self.owner_event(channel, event_id)?;
        self.owner_event_blocked(&event)
    }

    fn owner_event_ack(
        &self,
        event: &OwnerEvent,
    ) -> Result<Option<OwnerEventIntakeAcknowledgment>> {
        let path = object_path(&event.channel, "owner-event-ack", &event.event_id)?;
        match self.root.metadata(&path) {
            Ok(_) => {}
            Err(error) if error.is_not_found() => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let ack: OwnerEventIntakeAcknowledgment = self.read(&path)?;
        if ack.intake.event != *event
            || ack.decision.session != event.owner_session
            || ack.decision.entry_seq == 0
            || ack.decision_digest.trim().is_empty()
        {
            bail!("stored owner event acknowledgment identity mismatch");
        }
        self.validate_event_attempt(&ack.intake)?;
        Ok(Some(ack))
    }

    fn owner_event_blocked(&self, event: &OwnerEvent) -> Result<Option<OwnerEventBlocked>> {
        let path = object_path(&event.channel, "owner-event-blocked", &event.event_id)?;
        match self.root.metadata(&path) {
            Ok(_) => {
                let blocked: OwnerEventBlocked = self.read(&path)?;
                if blocked.event_id != event.event_id || blocked.reason.trim().is_empty() {
                    bail!("stored owner event block identity mismatch");
                }
                Ok(Some(blocked))
            }
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn validate_event_attempt(&self, intake: &OwnerEventIntakeAttempt) -> Result<()> {
        let key = format!("{}:{}", intake.event.event_id, intake.attempt);
        let existing: OwnerEventIntakeAttempt = self.read(&object_path(
            &intake.event.channel,
            "owner-event-attempt",
            &key,
        )?)?;
        if existing != *intake
            || self.owner_event(&intake.event.channel, &intake.event.event_id)? != intake.event
        {
            bail!("owner event intake identity mismatch");
        }
        Ok(())
    }
}

fn validate_event_shape(event: &OwnerEvent) -> Result<()> {
    if event.schema_version != 1
        || event.event_id.trim().is_empty()
        || event.event_id.len() > 256
        || event.owner_principal_id.trim().is_empty()
        || event.channel.channel_id.trim().is_empty()
        || event.owner_session.session_id.as_str().trim().is_empty()
    {
        bail!("invalid owner event identity");
    }
    let references_match = match (&event.source, &event.payload) {
        (
            OwnerEventSource::ExternalPeer { receipt_id },
            OwnerEventPayload::AssignmentTerminal {
                assignment_id,
                receipt,
            },
        ) => receipt_id == &receipt.receipt_id && assignment_id == &receipt.binding.assignment_id,
        (
            OwnerEventSource::Assignment {
                assignment_id: source_id,
            },
            OwnerEventPayload::AssignmentTerminal { assignment_id, .. }
            | OwnerEventPayload::Stall { assignment_id, .. },
        ) => source_id == assignment_id,
        (
            OwnerEventSource::Approval {
                approval_ref: source_ref,
            },
            OwnerEventPayload::Approval { approval_ref, .. },
        ) => source_ref == approval_ref,
        (
            OwnerEventSource::HumanMessage {
                message_ref: source_ref,
            },
            OwnerEventPayload::AddressedMessage { message_ref },
        ) => source_ref == message_ref,
        (
            OwnerEventSource::Schedule {
                schedule_id: source_schedule,
                occurrence_id: source_occurrence,
            },
            OwnerEventPayload::ScheduleOccurrence {
                schedule_id,
                occurrence_id,
            },
        ) => source_schedule == schedule_id && source_occurrence == occurrence_id,
        (
            OwnerEventSource::Delivery {
                delivery_id: source_id,
            },
            OwnerEventPayload::DeliveryFailure { delivery_id, .. },
        ) => source_id == delivery_id,
        _ => false,
    };
    if !references_match {
        bail!("owner event source and payload references disagree");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::coordination::{
        CoordinationChannelRecord, CoordinationChannelRef, ExternalPeerAssignmentBinding,
        ExternalPeerAssignmentGrant, ExternalPeerAssignmentReceipt, ExternalPeerAssignmentRequest,
        ExternalPeerRuntime, ExternalPeerTarget, OwnerContinuationLimits, OwnerEventPayload,
        OwnerEventSource, PeerAssignmentProposal, PeerOwnerContinuationGrant, PeerProposalDecision,
    };
    use medousa_types::{
        ContextManifest, ConversationRangeSelection, ResolvedConversationRange, SessionRef,
    };

    fn fixture(path: &std::path::Path) -> (CoordinationStore, OwnerEvent) {
        let store = CoordinationStore::open(path).unwrap();
        let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let channel = CoordinationChannelRef {
            authority_id: authority.clone(),
            channel_id: "channel-inbox-test".into(),
        };
        let owner_session = medousa_types::SessionRef {
            authority_id: authority,
            session_id: "ses_owner_inbox_test".parse().unwrap(),
        };
        let owner = "owner-1".to_string();
        store
            .create_channel(&CoordinationChannelRecord {
                channel: channel.clone(),
                owner_principal_id: owner.clone(),
                member_principal_ids: vec![owner.clone()],
                attached_sessions: vec![owner_session.clone()],
            })
            .unwrap();
        let event = OwnerEvent {
            schema_version: 1,
            event_id: "human-message-1".into(),
            owner_principal_id: owner,
            owner_session,
            channel,
            source: OwnerEventSource::HumanMessage {
                message_ref: "msg-1".into(),
            },
            occurred_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            payload: OwnerEventPayload::AddressedMessage {
                message_ref: "msg-1".into(),
            },
            limits: OwnerContinuationLimits::default(),
        };
        (store, event)
    }

    fn terminal_fixture(path: &std::path::Path) -> (CoordinationStore, OwnerEvent) {
        let store = CoordinationStore::open(path).unwrap();
        let owner_authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
        let target_authority = AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap();
        let owner_session = SessionRef {
            authority_id: owner_authority.clone(),
            session_id: "ses_owner_approval_test".parse().unwrap(),
        };
        let channel = CoordinationChannelRef {
            authority_id: owner_authority,
            channel_id: "channel-approval-test".into(),
        };
        let owner = "owner-approval".to_string();
        store
            .create_channel(&CoordinationChannelRecord {
                channel: channel.clone(),
                owner_principal_id: owner.clone(),
                member_principal_ids: vec![owner.clone()],
                attached_sessions: vec![owner_session.clone()],
            })
            .unwrap();
        let request = ExternalPeerAssignmentRequest {
            assignment_id: "assignment-approval".into(),
            idempotency_key: "command-approval".into(),
            owner_principal_id: owner.clone(),
            owner_session: owner_session.clone(),
            channel: channel.clone(),
            target: ExternalPeerTarget {
                authority_id: target_authority.clone(),
                execution_runtime_id: "worker-host".into(),
                runtime: ExternalPeerRuntime::Codex,
            },
            context: ContextManifest {
                manifest_id: format!("ctx_{}", "c".repeat(32)).parse().unwrap(),
                sources: vec![ResolvedConversationRange {
                    selection: ConversationRangeSelection {
                        session: owner_session.clone(),
                        after_entry_seq: None,
                        through_entry_seq: 1,
                    },
                    selection_digest: "sha256:approval-test".into(),
                }],
                created_by: owner.clone(),
                created_at: chrono::DateTime::parse_from_rfc3339("2026-09-23T00:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            },
            execution_session: SessionRef {
                authority_id: target_authority,
                session_id: "ses_executor_approval_test".parse().unwrap(),
            },
            instructions: "Review the designated work".into(),
            execution_grant_id: "grant-approval".into(),
            forge_work_id: "work-approval".into(),
            existing_agent_session_id: None,
        };
        let mut proposal = PeerAssignmentProposal {
            proposal_id: String::new(),
            request: request.clone(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            continue_owner: true,
        };
        proposal.proposal_id = super::super::proposals::proposal_identity(&proposal).unwrap();
        store.record_proposal(&proposal).unwrap();
        store
            .decide_proposal(
                &channel,
                &PeerProposalDecision {
                    proposal_id: proposal.proposal_id.clone(),
                    owner_principal_id: owner.clone(),
                    approved: true,
                },
            )
            .unwrap();
        store
            .approve_owner_continuation(&PeerOwnerContinuationGrant {
                request: request.clone(),
                expires_at: proposal.expires_at,
            })
            .unwrap();
        store
            .approve_assignment(&ExternalPeerAssignmentGrant {
                request: request.clone(),
                expires_at: proposal.expires_at,
            })
            .unwrap();
        store.claim_assignment(&request).unwrap();
        let binding = ExternalPeerAssignmentBinding {
            assignment_id: request.assignment_id.clone(),
            owner_principal_id: owner,
            channel: channel.clone(),
            target: request.target.clone(),
            execution_session: request.execution_session.clone(),
            agent_session_id: "agent-approval-test".into(),
        };
        store.record_peer(&binding).unwrap();
        let receipt = ExternalPeerAssignmentReceipt {
            receipt_id: crate::coordination::store::intake::terminal_receipt_id(&binding),
            binding,
            outcome: medousa_types::coordination::PeerAssignmentOutcome::Completed,
            result: "peer prompt ended".into(),
        };
        store.record_receipt(&receipt).unwrap();
        let event = store.peer_owner_event(&receipt).unwrap();
        (store, event)
    }

    #[test]
    fn events_replay_exactly_and_survive_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let (store, event) = fixture(temp.path());
        assert!(store.record_owner_event(&event).unwrap());
        assert!(!store.record_owner_event(&event).unwrap());
        drop(store);
        let reopened = CoordinationStore::open(temp.path()).unwrap();
        assert_eq!(
            reopened
                .owner_event(&event.channel, &event.event_id)
                .unwrap(),
            event
        );
        assert_eq!(
            reopened
                .pending_local_owner_event_records(&event.owner_session.authority_id, 10, None,)
                .unwrap(),
            vec![event]
        );
    }

    #[test]
    fn conflict_and_poisoned_event_records_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let (store, event) = fixture(temp.path());
        store.record_owner_event(&event).unwrap();
        let mut valid = event.clone();
        valid.event_id = "human-message-2".into();
        valid.source = OwnerEventSource::HumanMessage {
            message_ref: "msg-2".into(),
        };
        valid.payload = OwnerEventPayload::AddressedMessage {
            message_ref: "msg-2".into(),
        };
        store.record_owner_event(&valid).unwrap();
        let mut conflict = event.clone();
        conflict.payload = OwnerEventPayload::AddressedMessage {
            message_ref: "different-message".into(),
        };
        assert!(store.record_owner_event(&conflict).is_err());
        let path = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("oe1-")
            })
            .unwrap();
        std::fs::write(path, b"poisoned").unwrap();
        assert!(store.owner_event(&event.channel, &event.event_id).is_err());
        assert_eq!(
            store
                .pending_local_owner_event_records(&event.owner_session.authority_id, 10, None)
                .unwrap(),
            vec![valid]
        );
    }

    #[test]
    fn busy_fence_and_unresolved_claim_survive_reopen_without_turn_relaunch() {
        let temp = tempfile::tempdir().unwrap();
        let (store, event) = terminal_fixture(temp.path());
        let lease = store.try_owner_event_intake_lease(&event).unwrap().unwrap();
        let reopened = CoordinationStore::open(temp.path()).unwrap();
        assert!(
            reopened
                .try_owner_event_intake_lease(&event)
                .unwrap()
                .is_none()
        );
        let OwnerEventIntakeClaim::Started(intake) = store
            .begin_durable_owner_event_intake(&event, &lease)
            .unwrap()
        else {
            panic!("expected first durable event claim");
        };
        drop(lease);
        drop(store);
        let lease = reopened
            .try_owner_event_intake_lease(&event)
            .unwrap()
            .unwrap();
        assert_eq!(
            reopened
                .begin_durable_owner_event_intake(&event, &lease)
                .unwrap(),
            OwnerEventIntakeClaim::Unresolved(intake)
        );
    }

    #[test]
    fn owner_event_query_projects_legacy_terminal_attempt_and_ack_state() {
        let temp = tempfile::tempdir().unwrap();
        let (store, event) = terminal_fixture(temp.path());
        let owner = event.owner_principal_id.clone();
        let authority = event.owner_session.authority_id.clone();
        let lease = store.try_owner_event_intake_lease(&event).unwrap().unwrap();
        let OwnerEventIntakeClaim::Started(intake) =
            store.begin_owner_event_intake(&event, &lease).unwrap()
        else {
            panic!("expected a new peer receipt owner attempt");
        };
        let view = store
            .list_owner_events(&authority, &owner, 16, None)
            .unwrap()
            .into_iter()
            .find(|view| view.event.event_id == event.event_id)
            .unwrap();
        assert_eq!(view.status, OwnerEventStatus::Started);
        assert_eq!(
            view.latest_attempt.as_ref().unwrap().turn_id,
            intake.turn_id
        );

        let OwnerEventPayload::AssignmentTerminal { receipt, .. } = &event.payload else {
            unreachable!();
        };
        let ack = medousa_types::coordination::PeerOwnerIntakeAcknowledgment {
            intake: medousa_types::coordination::PeerOwnerIntakeAttempt {
                receipt: (**receipt).clone(),
                attempt: intake.attempt,
                turn_id: intake.turn_id.clone(),
            },
            decision: medousa_types::TranscriptEntryRef {
                session: event.owner_session.clone(),
                entry_id: format!("ent_{}", "d".repeat(32)).parse().unwrap(),
                entry_seq: 1,
            },
            decision_digest: "sha256:owner-decision".into(),
        };
        assert!(store.acknowledge_owner_intake(&ack, &lease).unwrap());
        let view = store
            .list_owner_events(&authority, &owner, 16, None)
            .unwrap()
            .into_iter()
            .find(|view| view.event.event_id == event.event_id)
            .unwrap();
        assert_eq!(view.status, OwnerEventStatus::Consumed);
        assert!(view.acknowledgment.is_some());
    }

    #[test]
    fn poisoned_legacy_receipt_does_not_starve_a_valid_generic_owner_event() {
        let temp = tempfile::tempdir().unwrap();
        let (store, event) = terminal_fixture(temp.path());
        let path = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("r1-")
            })
            .unwrap();
        std::fs::write(path, b"poisoned receipt").unwrap();

        let events = store
            .pending_local_owner_events(&event.owner_session.authority_id, "worker-host", 8, None)
            .unwrap();
        assert!(
            events.iter().any(|candidate| {
                matches!(candidate.payload, OwnerEventPayload::Approval { .. })
            })
        );
    }
}
