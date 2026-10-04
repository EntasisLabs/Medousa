use super::*;
use medousa_types::{ContextManifest, ConversationRangeSelection, ResolvedConversationRange};

fn fixture(
    path: &std::path::Path,
    policy: PeerHandoffPolicy,
) -> (
    CoordinationStore,
    PeerHandoffRecord,
    ExternalPeerAssignmentBinding,
) {
    let store = CoordinationStore::open(path).unwrap();
    let authority = format!("auth_{}", "a".repeat(64)).parse().unwrap();
    let owner_session = SessionRef {
        authority_id: authority,
        session_id: "ses_sender".parse().unwrap(),
    };
    let channel = CoordinationChannelRef {
        authority_id: owner_session.authority_id.clone(),
        channel_id: "handoff".into(),
    };
    store
        .create_channel(&CoordinationChannelRecord {
            channel: channel.clone(),
            owner_principal_id: "owner".into(),
            member_principal_ids: vec!["owner".into()],
            attached_sessions: vec![owner_session.clone()],
        })
        .unwrap();
    let request = ExternalPeerAssignmentRequest {
        assignment_id: "assignment".into(),
        idempotency_key: "command".into(),
        owner_principal_id: "owner".into(),
        owner_session: owner_session.clone(),
        channel: channel.clone(),
        target: ExternalPeerTarget {
            authority_id: owner_session.authority_id.clone(),
            execution_runtime_id: "local".into(),
            runtime: ExternalPeerRuntime::Medousa,
        },
        context: ContextManifest {
            manifest_id: format!("ctx_{}", "b".repeat(32)).parse().unwrap(),
            sources: vec![ResolvedConversationRange {
                selection: ConversationRangeSelection {
                    session: owner_session.clone(),
                    after_entry_seq: None,
                    through_entry_seq: 1,
                },
                selection_digest: "sha256:source".into(),
            }],
            created_by: "owner".into(),
            created_at: chrono::Utc::now(),
        },
        execution_session: SessionRef {
            authority_id: owner_session.authority_id.clone(),
            session_id: "ses_worker".parse().unwrap(),
        },
        instructions: "Implement".into(),
        execution_grant_id: "grant".into(),
        forge_work_id: "work-1".into(),
        existing_agent_session_id: None,
    };
    let mut proposal = PeerAssignmentProposal {
        proposal_id: String::new(),
        request: request.clone(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        continue_owner: false,
    };
    proposal.proposal_id = super::super::proposals::proposal_identity(&proposal).unwrap();
    store.record_proposal(&proposal).unwrap();
    let record = PeerHandoffRecord {
        request: request.clone(),
        policy,
        admission: PeerHandoffAdmission::Delegate,
        source: medousa_types::TranscriptEntryRef {
            session: owner_session,
            entry_id: format!("ent_{}", "c".repeat(32)).parse().unwrap(),
            entry_seq: 1,
        },
        source_digest: "sha256:user".into(),
        sender_bot_id: None,
        expires_at: proposal.expires_at,
    };
    store.record_handoff(&record).unwrap();
    let binding = ExternalPeerAssignmentBinding {
        assignment_id: request.assignment_id.clone(),
        owner_principal_id: "owner".into(),
        channel: channel.clone(),
        target: request.target.clone(),
        execution_session: request.execution_session.clone(),
        agent_session_id: "native-worker".into(),
    };
    store
        .decide_proposal(
            &channel,
            &PeerProposalDecision {
                proposal_id: proposal.proposal_id,
                owner_principal_id: "owner".into(),
                approved: true,
            },
        )
        .unwrap();
    store
        .approve_assignment(&ExternalPeerAssignmentGrant {
            request: request.clone(),
            expires_at: proposal.expires_at,
        })
        .unwrap();
    store.claim_assignment(&request).unwrap();
    (store, record, binding)
}
fn terminal(store: &CoordinationStore, binding: &ExternalPeerAssignmentBinding) -> OwnerEvent {
    let receipt = ExternalPeerAssignmentReceipt {
        receipt_id: super::super::intake::terminal_receipt_id(binding),
        binding: binding.clone(),
        outcome: PeerAssignmentOutcome::Completed,
        result: "Completed; no review needed!".into(),
    };
    store.observe_receipt_once(&receipt).unwrap();
    store.peer_owner_event(&receipt).unwrap()
}
#[test]
fn worker_completion_cannot_waive_sender_review_and_only_sender_can_accept() {
    let root = tempfile::tempdir().unwrap();
    let (store, record, binding) = fixture(root.path(), PeerHandoffPolicy::default());
    store.record_peer(&binding).unwrap();
    store.record_handoff_acceptance(&binding).unwrap();
    let event = terminal(&store, &binding);
    let view = store
        .handoff_view(&binding.channel, &binding.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(view.state, PeerHandoffState::AwaitingSenderReview);
    assert_eq!(view.responsible_session, record.request.owner_session);
    let OwnerEventPayload::AssignmentTerminal { receipt, .. } = event.payload else {
        unreachable!()
    };
    let decision = PeerHandoffReview {
        receipt_id: receipt.receipt_id,
        verdict: PeerHandoffVerdict::Accept,
        reason: "Checked the result".into(),
        sender_session: record.request.owner_session.clone(),
        turn_id: "sender-review".into(),
    };
    assert!(
        store
            .review_handoff(
                &binding.channel,
                &binding.assignment_id,
                &binding.execution_session,
                "owner",
                &decision
            )
            .is_err()
    );
    assert!(
        store
            .review_handoff(
                &binding.channel,
                &binding.assignment_id,
                &record.request.owner_session,
                "other-owner",
                &decision
            )
            .is_err()
    );
    store
        .review_handoff(
            &binding.channel,
            &binding.assignment_id,
            &record.request.owner_session,
            "owner",
            &decision,
        )
        .unwrap();
    assert_eq!(
        store
            .handoff_view(&binding.channel, &binding.assignment_id)
            .unwrap()
            .unwrap()
            .state,
        PeerHandoffState::Accepted
    );
}
#[test]
fn ownership_transfers_only_after_native_acceptance_and_callbacks_remain_independent() {
    let root = tempfile::tempdir().unwrap();
    let policy = PeerHandoffPolicy {
        responsibility: HandoffResponsibility::Transfer,
        completion: HandoffCompletion::WorkerResult,
        wake_on_accepted: false,
        wake_on_terminal: false,
        ..Default::default()
    };
    let (store, record, binding) = fixture(root.path(), policy);
    store.record_peer(&binding).unwrap();
    assert_eq!(
        store
            .handoff_view(&binding.channel, &binding.assignment_id)
            .unwrap()
            .unwrap()
            .responsible_session,
        record.request.owner_session
    );
    store.record_handoff_acceptance(&binding).unwrap();
    let event = store.handoff_acceptance(&record).unwrap().unwrap();
    assert!(store.require_handoff_event(&event).is_err());
    assert_eq!(
        store
            .handoff_view(&binding.channel, &binding.assignment_id)
            .unwrap()
            .unwrap()
            .responsible_session,
        binding.execution_session
    );
    terminal(&store, &binding);
    assert_eq!(
        store
            .handoff_view(&binding.channel, &binding.assignment_id)
            .unwrap()
            .unwrap()
            .state,
        PeerHandoffState::Accepted
    );
}
#[test]
fn silent_callbacks_are_internal_and_exact_attempts_reconcile_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let (store, record, binding) = fixture(
        root.path(),
        PeerHandoffPolicy {
            contact: WorkContactPreference::Silent,
            ..Default::default()
        },
    );
    store.record_peer(&binding).unwrap();
    store.record_handoff_acceptance(&binding).unwrap();
    let event = terminal(&store, &binding);
    let session = CoordinationStore::handoff_wake_session(&record).unwrap();
    assert_ne!(session, record.request.owner_session);
    let lease = store.try_handoff_event_lease(&event).unwrap().unwrap();
    assert!(store.try_handoff_event_lease(&event).unwrap().is_none());
    let OwnerEventIntakeClaim::Started(attempt) =
        store.begin_handoff_event(&event, &lease).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        store.handoff_for_wake(&session, &attempt.turn_id).unwrap(),
        record
    );
    assert!(
        store
            .handoff_for_wake(&record.request.execution_session, &attempt.turn_id)
            .is_err()
    );
    drop(lease);
    drop(store);
    let store = CoordinationStore::open(root.path()).unwrap();
    let lease = store.try_handoff_event_lease(&event).unwrap().unwrap();
    assert_eq!(
        store.begin_handoff_event(&event, &lease).unwrap(),
        OwnerEventIntakeClaim::Unresolved(attempt.clone())
    );
    let ack = OwnerEventIntakeAcknowledgment {
        intake: attempt,
        decision: medousa_types::TranscriptEntryRef {
            session,
            entry_id: format!("ent_{}", "d".repeat(32)).parse().unwrap(),
            entry_seq: 1,
        },
        decision_digest: "sha256:decision".into(),
        command_refs: vec![],
        terminal_delivery_ref: None,
    };
    store.acknowledge_handoff_event(&ack, &lease).unwrap();
    assert!(matches!(
        store.begin_handoff_event(&event, &lease).unwrap(),
        OwnerEventIntakeClaim::Consumed(_)
    ));
    assert!(
        !store
            .pending_local_owner_events(&binding.channel.authority_id, "local", 10, None)
            .unwrap()
            .iter()
            .any(|e| e.event_id == event.event_id)
    );
    let view = store
        .list_owner_events(&binding.channel.authority_id, "owner", 10, None)
        .unwrap()
        .into_iter()
        .find(|v| v.event.event_id == event.event_id)
        .unwrap();
    assert_eq!(view.status, OwnerEventStatus::Consumed);
}
#[test]
fn handoff_retry_cannot_change_review_policy_or_sender_and_revocation_blocks_wakes() {
    let root = tempfile::tempdir().unwrap();
    let (store, record, binding) = fixture(root.path(), PeerHandoffPolicy::default());
    assert!(!store.record_handoff(&record).unwrap());
    let mut changed = record.clone();
    changed.policy.completion = HandoffCompletion::WorkerResult;
    assert!(store.record_handoff(&changed).is_err());
    changed = record.clone();
    changed.source.session = binding.execution_session.clone();
    assert!(store.record_handoff(&changed).is_err());
    store.record_peer(&binding).unwrap();
    store.record_handoff_acceptance(&binding).unwrap();
    let event = store.handoff_acceptance(&record).unwrap().unwrap();
    store
        .revoke_assignment_grant(&binding.channel, &record.request.execution_grant_id)
        .unwrap();
    assert!(store.try_handoff_event_lease(&event).is_err());
}

#[test]
fn only_definitively_rejected_attempts_can_retry_and_stale_turns_cannot_reenter() {
    let root = tempfile::tempdir().unwrap();
    let (store, record, binding) = fixture(root.path(), PeerHandoffPolicy::default());
    store.record_peer(&binding).unwrap();
    store.record_handoff_acceptance(&binding).unwrap();
    let event = store.handoff_acceptance(&record).unwrap().unwrap();
    let lease = store.try_handoff_event_lease(&event).unwrap().unwrap();
    let OwnerEventIntakeClaim::Started(first) = store.begin_handoff_event(&event, &lease).unwrap()
    else {
        panic!()
    };
    store.reject_handoff_event(&first, &lease).unwrap();
    let session = CoordinationStore::handoff_wake_session(&record).unwrap();
    assert!(store.handoff_for_wake(&session, &first.turn_id).is_err());
    let OwnerEventIntakeClaim::Started(second) = store.begin_handoff_event(&event, &lease).unwrap()
    else {
        panic!()
    };
    assert_eq!(second.attempt, 1);
    assert_ne!(second.turn_id, first.turn_id);
    assert!(store.handoff_for_wake(&session, &second.turn_id).is_ok());
    // Losing the ticket is unresolved custody, never a reason to issue a third turn.
    assert!(matches!(
        store.begin_handoff_event(&event, &lease).unwrap(),
        OwnerEventIntakeClaim::Unresolved(_)
    ));
}

#[test]
fn concurrent_acceptance_keeps_one_event_and_reviews_keep_the_first_attribution() {
    let root = tempfile::tempdir().unwrap();
    let (store, record, binding) = fixture(root.path(), PeerHandoffPolicy::default());
    store.record_peer(&binding).unwrap();
    let store = std::sync::Arc::new(store);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let store = store.clone();
            let binding = binding.clone();
            std::thread::spawn(move || store.record_handoff_acceptance(&binding).unwrap())
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|created| *created)
            .count(),
        1
    );
    let event = terminal(&store, &binding);
    let OwnerEventPayload::AssignmentTerminal { receipt, .. } = event.payload else {
        panic!()
    };
    let mut review = PeerHandoffReview {
        receipt_id: receipt.receipt_id,
        verdict: PeerHandoffVerdict::ChangesRequested,
        reason: "Missing checks".into(),
        sender_session: record.request.owner_session.clone(),
        turn_id: "first".into(),
    };
    store
        .review_handoff(
            &binding.channel,
            &binding.assignment_id,
            &record.request.owner_session,
            "owner",
            &review,
        )
        .unwrap();
    review.turn_id = "retry".into();
    assert!(
        !store
            .review_handoff(
                &binding.channel,
                &binding.assignment_id,
                &record.request.owner_session,
                "owner",
                &review
            )
            .unwrap()
    );
    review.verdict = PeerHandoffVerdict::Accept;
    assert!(
        store
            .review_handoff(
                &binding.channel,
                &binding.assignment_id,
                &record.request.owner_session,
                "owner",
                &review
            )
            .is_err()
    );
    let view = store
        .handoff_view(&binding.channel, &binding.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(view.state, PeerHandoffState::ChangesRequested);
    assert_eq!(view.review.unwrap().turn_id, "first");
}

#[test]
fn inbox_metadata_stays_compact_even_when_a_worker_returns_a_large_result() {
    let root = tempfile::tempdir().unwrap();
    let (store, record, binding) = fixture(root.path(), PeerHandoffPolicy::default());
    store.record_peer(&binding).unwrap();
    let receipt = ExternalPeerAssignmentReceipt {
        receipt_id: super::super::intake::terminal_receipt_id(&binding),
        binding: binding.clone(),
        outcome: PeerAssignmentOutcome::Completed,
        result: "z".repeat(32 * 1024),
    };
    store.observe_receipt_once(&receipt).unwrap();
    let summary = store
        .handoff_summary(&binding.channel, &binding.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(summary.policy, record.policy);
    assert_eq!(summary.state, PeerHandoffState::AwaitingSenderReview);
    assert!(serde_json::to_vec(&summary).unwrap().len() < 1024);
    drop(store);
    let reopened = CoordinationStore::open(root.path()).unwrap();
    assert_eq!(
        reopened
            .handoff_summary(&binding.channel, &binding.assignment_id)
            .unwrap(),
        Some(summary)
    );
}
