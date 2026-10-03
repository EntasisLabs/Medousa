use super::*;

fn fixture() -> (
    WorkCoordinationPlan,
    WorkReviewInput,
    ExternalPeerAssignmentReceipt,
) {
    let authority = medousa_types::AuthorityId::parse(format!("auth_{}", "a".repeat(64))).unwrap();
    let channel = CoordinationChannelRef {
        authority_id: authority.clone(),
        channel_id: "channel".into(),
    };
    let plan = WorkCoordinationPlan {
        scope_digest: "a".repeat(64),
        domain: UserDomainRef {
            authority_id: authority.clone(),
            user_id: "owner".into(),
        },
        input: WorkCoordinationInput {
            coordination_id: "coord".into(),
            work_unit_id: "unit".into(),
            expected_scope_revision: 1,
            channel: channel.clone(),
            executor_proposal_id: "exec-proposal".into(),
            reviewer_proposal_id: "review-proposal".into(),
            deadline: chrono::Utc::now() + chrono::Duration::hours(1),
        },
        executor_assignment_id: "exec".into(),
        reviewer_assignment_id: "review".into(),
        forge_work_id: "work".into(),
    };
    let input = WorkReviewInput {
        coordination_id: "coord".into(),
        work_unit_id: "unit".into(),
        executor_assignment_id: "exec".into(),
        executor_receipt_id: "exec-receipt".into(),
        forge_work_id: "work".into(),
        environment_generation: 1,
        branch: "main".into(),
        head_oid: "a".repeat(40),
    };
    let binding = ExternalPeerAssignmentBinding {
        assignment_id: "review".into(),
        owner_principal_id: "owner".into(),
        channel,
        target: ExternalPeerTarget {
            authority_id: authority.clone(),
            execution_runtime_id: "runtime".into(),
            runtime: ExternalPeerRuntime::Codex,
        },
        execution_session: SessionRef {
            authority_id: authority,
            session_id: medousa_types::SessionId::parse("ses_review").unwrap(),
        },
        agent_session_id: "native-review".into(),
    };
    let receipt = ExternalPeerAssignmentReceipt {
        receipt_id: peer_terminal_receipt_id(&binding),
        binding,
        outcome: PeerAssignmentOutcome::Completed,
        result: serde_json::to_string(&WorkReviewDecision {
            reviewed: input.clone(),
            verdict: WorkReviewVerdict::Approved,
            summary: "Tests pass and change meets the requested scope".into(),
        })
        .unwrap(),
    };
    (plan, input, receipt)
}

#[test]
fn native_completion_requires_an_explicit_exact_revision_verdict() {
    let (plan, input, mut receipt) = fixture();
    assert_eq!(
        review_result(&plan, &input, &receipt).outcome,
        WorkCoordinationOutcome::Approved
    );
    for text in ["done", "approved", "{}", "```json\n{}\n```"] {
        receipt.result = text.into();
        assert_eq!(
            review_result(&plan, &input, &receipt).outcome,
            WorkCoordinationOutcome::InvalidReview
        );
    }
}

#[test]
fn stale_receipts_generations_and_checkout_revisions_cannot_approve() {
    let (plan, input, receipt) = fixture();
    for field in [
        "executor_receipt_id",
        "forge_work_id",
        "head_oid",
        "branch",
        "work_unit_id",
        "coordination_id",
        "executor_assignment_id",
    ] {
        let mut decision: serde_json::Value = serde_json::from_str(&receipt.result).unwrap();
        decision["reviewed"][field] = "different".into();
        let mut stale = receipt.clone();
        stale.result = decision.to_string();
        assert_eq!(
            review_result(&plan, &input, &stale).outcome,
            WorkCoordinationOutcome::InvalidReview,
            "{field}"
        );
    }
    let mut stale = input.clone();
    stale.environment_generation += 1;
    assert_eq!(
        review_result(&plan, &stale, &receipt).outcome,
        WorkCoordinationOutcome::InvalidReview
    );
    let mut foreign = receipt.clone();
    foreign.binding.owner_principal_id = "other".into();
    assert_eq!(
        review_result(&plan, &input, &foreign).outcome,
        WorkCoordinationOutcome::InvalidReview
    );
}

#[test]
fn changes_requested_and_interrupted_review_never_satisfy_work() {
    let (plan, input, mut receipt) = fixture();
    let mut decision: WorkReviewDecision = serde_json::from_str(&receipt.result).unwrap();
    decision.verdict = WorkReviewVerdict::ChangesRequested;
    receipt.result = serde_json::to_string(&decision).unwrap();
    assert_eq!(
        review_result(&plan, &input, &receipt).outcome,
        WorkCoordinationOutcome::ChangesRequested
    );
    receipt.outcome = PeerAssignmentOutcome::Interrupted;
    assert_eq!(
        review_result(&plan, &input, &receipt).outcome,
        WorkCoordinationOutcome::ReviewerFailed
    );
}

#[test]
fn oversized_unknown_and_empty_review_fields_fail_closed() {
    let (plan, input, mut receipt) = fixture();
    let good = receipt.result.clone();
    for summary in ["".to_string(), " ".into(), "x".repeat(4097), "\0".into()] {
        let mut decision: serde_json::Value = serde_json::from_str(&good).unwrap();
        decision["summary"] = summary.into();
        receipt.result = decision.to_string();
        assert_eq!(
            review_result(&plan, &input, &receipt).outcome,
            WorkCoordinationOutcome::InvalidReview
        );
    }
    let mut decision: serde_json::Value = serde_json::from_str(&good).unwrap();
    decision["grant"] = "new authority".into();
    receipt.result = decision.to_string();
    assert_eq!(
        review_result(&plan, &input, &receipt).outcome,
        WorkCoordinationOutcome::InvalidReview
    );
    receipt.result = "x".repeat(8193);
    assert_eq!(
        review_result(&plan, &input, &receipt).outcome,
        WorkCoordinationOutcome::InvalidReview
    );
}
