use super::*;
use medousa_types::{coordination::*, work_unit::UserDomainRef, *};

fn fixture() -> (
    tempfile::TempDir,
    CoordinationStore,
    WorkCoordinationPlan,
    Vec<ExternalPeerAssignmentRequest>,
) {
    fixture_for_runtime(ExternalPeerRuntime::Codex)
}

fn fixture_for_runtime(
    runtime: ExternalPeerRuntime,
) -> (
    tempfile::TempDir,
    CoordinationStore,
    WorkCoordinationPlan,
    Vec<ExternalPeerAssignmentRequest>,
) {
    let temp = tempfile::tempdir().unwrap();
    let store = CoordinationStore::open(temp.path()).unwrap();
    let authority = AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
    let channel = CoordinationChannelRef {
        authority_id: authority.clone(),
        channel_id: "work-channel".into(),
    };
    let owner_session = SessionRef {
        authority_id: authority.clone(),
        session_id: SessionId::parse("ses_owner").unwrap(),
    };
    store
        .create_channel(&CoordinationChannelRecord {
            channel: channel.clone(),
            owner_principal_id: "owner".into(),
            member_principal_ids: vec!["owner".into()],
            attached_sessions: vec![owner_session.clone()],
        })
        .unwrap();
    let expiry = chrono::Utc::now() + chrono::Duration::hours(2);
    let mut requests = vec![];
    let mut proposals = vec![];
    for id in ["executor", "reviewer"] {
        let request = ExternalPeerAssignmentRequest {
            assignment_id: id.into(),
            idempotency_key: format!("command-{id}"),
            owner_principal_id: "owner".into(),
            owner_session: owner_session.clone(),
            channel: channel.clone(),
            target: ExternalPeerTarget {
                authority_id: authority.clone(),
                execution_runtime_id: "runtime".into(),
                runtime,
            },
            context: ContextManifest {
                manifest_id: ContextManifestId::parse(format!(
                    "ctx_{}",
                    if id == "executor" {
                        "a".repeat(32)
                    } else {
                        "b".repeat(32)
                    }
                ))
                .unwrap(),
                sources: vec![ResolvedConversationRange {
                    selection: ConversationRangeSelection {
                        session: owner_session.clone(),
                        after_entry_seq: None,
                        through_entry_seq: 1,
                    },
                    selection_digest: "digest".into(),
                }],
                created_by: "owner".into(),
                created_at: chrono::Utc::now(),
            },
            execution_session: SessionRef {
                authority_id: authority.clone(),
                session_id: SessionId::parse(format!("ses_{id}")).unwrap(),
            },
            instructions: format!("Do {id} using {WORK_REVIEW_CONTRACT}"),
            execution_grant_id: format!("grant-{id}"),
            forge_work_id: "forge-work".into(),
            existing_agent_session_id: None,
        };
        let mut proposal = PeerAssignmentProposal {
            proposal_id: String::new(),
            request: request.clone(),
            expires_at: expiry,
            continue_owner: false,
        };
        proposal.proposal_id = super::super::proposals::proposal_identity(&proposal).unwrap();
        store.record_proposal(&proposal).unwrap();
        store
            .decide_proposal(
                &channel,
                &PeerProposalDecision {
                    proposal_id: proposal.proposal_id.clone(),
                    owner_principal_id: "owner".into(),
                    approved: true,
                },
            )
            .unwrap();
        store
            .approve_assignment(&ExternalPeerAssignmentGrant {
                request: request.clone(),
                expires_at: expiry,
            })
            .unwrap();
        proposals.push(proposal.proposal_id);
        requests.push(request);
    }
    let plan = WorkCoordinationPlan {
        fix_review_assignments: vec![],
        round_index: 0,
        domain: UserDomainRef {
            authority_id: authority,
            user_id: "owner".into(),
        },
        input: WorkCoordinationInput {
            coordination_id: "coordination".into(),
            work_unit_id: "unit".into(),
            expected_scope_revision: 1,
            channel,
            executor_proposal_id: proposals[0].clone(),
            reviewer_proposal_id: proposals[1].clone(),
            deadline: chrono::Utc::now() + chrono::Duration::hours(1),
            fix_review_rounds: vec![],
        },
        scope_digest: "a".repeat(64),
        executor_assignment_id: "executor".into(),
        reviewer_assignment_id: "reviewer".into(),
        forge_work_id: "forge-work".into(),
    };
    (temp, store, plan, requests)
}

fn complete(
    store: &CoordinationStore,
    request: &ExternalPeerAssignmentRequest,
    result: String,
) -> ExternalPeerAssignmentReceipt {
    store.claim_assignment(request).unwrap();
    let binding = ExternalPeerAssignmentBinding {
        assignment_id: request.assignment_id.clone(),
        owner_principal_id: request.owner_principal_id.clone(),
        channel: request.channel.clone(),
        target: request.target.clone(),
        execution_session: request.execution_session.clone(),
        agent_session_id: format!("agent-{}", request.assignment_id),
    };
    store.record_peer(&binding).unwrap();
    let receipt = ExternalPeerAssignmentReceipt {
        receipt_id: peer_terminal_receipt_id(&binding),
        binding,
        outcome: PeerAssignmentOutcome::Completed,
        result,
    };
    store.observe_receipt_once(&receipt).unwrap();
    receipt
}

#[test]
fn registration_reopens_without_reissuing_grants_and_fences_reuse() {
    let (temp, store, plan, requests) = fixture();
    assert!(store.register_work_plan(&plan).unwrap());
    assert_eq!(
        store.work_plan_for_assignment(&requests[0]).unwrap(),
        Some(plan.clone())
    );
    assert!(!store.register_work_plan(&plan).unwrap());
    let mut changed = plan.clone();
    changed.input.coordination_id = "another".into();
    assert!(store.register_work_plan(&changed).is_err());
    drop(store);
    let reopened = CoordinationStore::open(temp.path()).unwrap();
    assert_eq!(
        reopened
            .local_work_plans(&plan.domain.authority_id, "runtime", 4, None)
            .unwrap(),
        vec![plan.clone()]
    );
    assert!(
        reopened
            .local_work_plans(&plan.domain.authority_id, "other-runtime", 4, None)
            .unwrap()
            .is_empty()
    );
    assert!(
        reopened
            .local_work_plans(
                &AuthorityId::parse(format!("auth_{}", "b".repeat(64))).unwrap(),
                "runtime",
                4,
                None
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        reopened
            .require_owner(&plan.input.channel, "other")
            .is_err()
    );
}

#[test]
fn native_dispatch_claim_must_follow_registration_and_checks_share_its_lock() {
    let (_temp, store, plan, requests) = fixture();
    let lock = store
        .work_assignment_lock(&plan.input.channel, &plan.executor_assignment_id)
        .unwrap();
    assert!(store.register_work_plan(&plan).is_err());
    assert!(store.claim_assignment(&requests[0]).is_err());
    drop(lock);
    store.register_work_plan(&plan).unwrap();
    assert!(
        store
            .claim_assignment_checked(&requests[0], || bail!("paused work"))
            .is_err()
    );
    assert_eq!(
        store.claim_assignment(&requests[0]).unwrap(),
        super::super::AssignmentClaim::Claimed
    );
    assert_eq!(
        store.claim_assignment(&requests[0]).unwrap(),
        super::super::AssignmentClaim::Existing
    );
}

#[test]
fn late_registration_and_expired_or_revoked_native_grants_fail_closed() {
    let (_temp, store, plan, requests) = fixture();
    store.claim_assignment(&requests[0]).unwrap();
    assert!(store.register_work_plan(&plan).is_err());
    let (_temp, store, mut plan, requests) = fixture();
    store
        .revoke_assignment_grant(&requests[1].channel, &requests[1].execution_grant_id)
        .unwrap();
    assert!(store.register_work_plan(&plan).is_err());
    plan.input.deadline = chrono::Utc::now() - chrono::Duration::seconds(1);
    assert!(store.register_work_plan(&plan).is_err());
}

#[test]
fn partial_registration_blocks_dispatch_and_exact_replay_repairs_it() {
    let (_temp, store, plan, requests) = fixture();
    store
        .create(
            &object_path(
                &plan.input.channel,
                "work-stage",
                &plan.executor_assignment_id,
            )
            .unwrap(),
            &plan,
        )
        .unwrap();
    assert!(store.claim_assignment(&requests[0]).is_err());
    store.register_work_plan(&plan).unwrap();
    assert_eq!(
        store.claim_assignment(&requests[0]).unwrap(),
        super::super::AssignmentClaim::Claimed
    );
}

#[test]
fn review_input_requires_completion_and_is_immutable_across_restart() {
    let (temp, store, plan, requests) = fixture();
    store.register_work_plan(&plan).unwrap();
    let mut input = WorkReviewInput {
        coordination_id: plan.input.coordination_id.clone(),
        work_unit_id: plan.input.work_unit_id.clone(),
        executor_assignment_id: plan.executor_assignment_id.clone(),
        executor_receipt_id: "unknown".into(),
        forge_work_id: plan.forge_work_id.clone(),
        environment_generation: 1,
        branch: "main".into(),
        head_oid: "a".repeat(40),
    };
    assert!(store.record_work_review_input(&plan, &input).is_err());
    let receipt = complete(&store, &requests[0], "implemented".into());
    input.executor_receipt_id = receipt.receipt_id;
    store.record_work_review_input(&plan, &input).unwrap();
    let reopened = CoordinationStore::open(temp.path()).unwrap();
    assert_eq!(
        reopened.work_review_input(&plan).unwrap(),
        Some(input.clone())
    );
    input.head_oid = "b".repeat(40);
    assert!(reopened.record_work_review_input(&plan, &input).is_err());
}

#[test]
fn coordinator_lease_is_cross_process_and_terminal_receipts_cannot_be_replaced() {
    let (temp, store, plan, requests) = fixture();
    store.register_work_plan(&plan).unwrap();
    let lease = store.try_work_coordination_lease(&plan).unwrap().unwrap();
    let reopened = CoordinationStore::open(temp.path()).unwrap();
    assert!(
        reopened
            .try_work_coordination_lease(&plan)
            .unwrap()
            .is_none()
    );
    drop(lease);
    assert!(
        reopened
            .try_work_coordination_lease(&plan)
            .unwrap()
            .is_some()
    );
    let first = complete(&store, &requests[0], "first".into());
    let mut duplicate = first.clone();
    duplicate.result = "different".into();
    assert!(!store.observe_receipt_once(&duplicate).unwrap());
    assert_eq!(
        store
            .receipt(&plan.input.channel, &plan.executor_assignment_id)
            .unwrap(),
        first
    );
}

#[test]
fn fabricated_native_review_approval_cannot_be_persisted() {
    let (_temp, store, plan, _) = fixture();
    store.register_work_plan(&plan).unwrap();
    let result = WorkCoordinationResult {
        coordination_id: plan.input.coordination_id.clone(),
        outcome: WorkCoordinationOutcome::Approved,
        receipt_id: Some("fabricated".into()),
        decision: None,
    };
    assert!(
        store
            .record_work_coordination_result(&plan, &result)
            .is_err()
    );
}

#[test]
fn work_owned_terminals_do_not_spend_owner_chat_continuation_authority() {
    let (_temp, store, plan, requests) = fixture();
    store.register_work_plan(&plan).unwrap();
    let receipt = complete(&store, &requests[0], "implemented".into());
    assert!(store.retain_work_terminal_for_controller(&receipt).unwrap());
    assert!(store.retain_work_terminal_for_controller(&receipt).unwrap());
    assert!(
        store
            .pending_local_owner_events(&plan.domain.authority_id, "runtime", 8, None)
            .unwrap()
            .is_empty()
    );
    assert!(store.work_coordination_result(&plan).unwrap().is_none());
}

#[test]
fn native_coder_turn_identity_replays_after_restart_and_revocation_prevents_readmission() {
    let (temp, store, plan, requests) = fixture_for_runtime(ExternalPeerRuntime::Medousa);
    store.register_work_plan(&plan).unwrap();
    let request = &requests[0];
    let turn_id = format!("medousa_coder_{}", request.assignment_id);
    assert!(
        store
            .record_native_coder("unrelated-turn", request)
            .is_err()
    );
    store.claim_assignment(request).unwrap();
    store.record_native_coder(&turn_id, request).unwrap();
    store.record_native_coder(&turn_id, request).unwrap();
    let reopened = CoordinationStore::open(temp.path()).unwrap();
    assert_eq!(
        reopened
            .native_coder_request(&plan.domain.authority_id, &turn_id)
            .unwrap(),
        *request
    );
    assert_eq!(
        reopened.claim_assignment(request).unwrap(),
        super::super::AssignmentClaim::Existing
    );
    let mut changed = request.clone();
    changed.instructions = "Different unapproved work".into();
    assert!(reopened.record_native_coder(&turn_id, &changed).is_err());
    reopened
        .revoke_assignment_grant(&request.channel, &request.execution_grant_id)
        .unwrap();
    assert!(reopened.record_native_coder(&turn_id, request).is_err());
    assert!(
        reopened
            .require_assignment_grant(request, chrono::Utc::now())
            .is_err()
    );
}

#[test]
fn fix_round_registration_rejects_reused_native_assignments_and_unbounded_intent() {
    let (_temp, store, mut plan, _) = fixture();
    let round = WorkCoordinationRoundInput {
        executor_proposal_id: plan.input.executor_proposal_id.clone(),
        reviewer_proposal_id: plan.input.reviewer_proposal_id.clone(),
    };
    let assignments = WorkCoordinationRoundAssignments {
        executor_assignment_id: plan.executor_assignment_id.clone(),
        reviewer_assignment_id: plan.reviewer_assignment_id.clone(),
    };
    plan.input.fix_review_rounds = vec![round.clone()];
    plan.fix_review_assignments = vec![assignments.clone()];
    assert!(store.register_work_plan(&plan).is_err());
    plan.input.fix_review_rounds = vec![round; MAX_FIX_REVIEW_ROUNDS + 1];
    plan.fix_review_assignments = vec![assignments; MAX_FIX_REVIEW_ROUNDS + 1];
    assert!(store.register_work_plan(&plan).is_err());
    assert!(
        !store
            .work_is_controlled(&plan.domain, &plan.input.work_unit_id)
            .unwrap()
    );
}
