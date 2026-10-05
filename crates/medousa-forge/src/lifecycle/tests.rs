use super::*;
use crate::git::{CheckpointAuthor, GitEngine};

struct Fixture {
    _dir: tempfile::TempDir,
    repo: std::path::PathBuf,
    forge: Forge,
    item: WorkItem,
}
fn actor() -> ActorRef {
    ActorRef {
        kind: ActorKind::Profile,
        id: "owner:session:taco:turn:callback".into(),
    }
}
fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}
fn fixture(mode: WorkspaceMode) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().canonicalize().unwrap().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    std::fs::write(repo.join("hello.txt"), "before\n").unwrap();
    git(&repo, &["add", "."]);
    GitEngine::detect()
        .unwrap()
        .commit_checkpoint(&repo, "initial", &CheckpointAuthor::default())
        .unwrap();
    if mode == WorkspaceMode::AttachedCheckout {
        std::fs::write(repo.join("hello.txt"), "staged\n").unwrap();
        git(&repo, &["add", "hello.txt"]);
    }
    let forge = Forge::open(dir.path().canonicalize().unwrap().join("forge")).unwrap();
    let item = forge
        .register_with_workspace_mode(
            "lifecycle",
            "finish this work",
            &repo,
            "main",
            "owner",
            mode,
            &actor(),
        )
        .unwrap();
    let item = forge.provision(&item.id, &actor()).unwrap();
    Fixture {
        _dir: dir,
        repo,
        forge,
        item,
    }
}
fn prepare(fx: &Fixture) -> ProjectReviewPin {
    let workspace = fx.item.workspace_environment().unwrap().worktree.clone();
    std::fs::write(workspace.join("hello.txt"), "after\n").unwrap();
    prepare_merge(
        &fx.forge,
        "owner",
        &actor(),
        ProjectPrepareMergeInput {
            work_id: fx.item.id.to_string(),
            ack_risks: false,
        },
    )
    .unwrap();
    pin(fx)
}
fn pin(fx: &Fixture) -> ProjectReviewPin {
    let value = review(
        &fx.forge,
        "owner",
        ProjectReviewQuery {
            work_id: fx.item.id.to_string(),
            attempt_id: None,
        },
    )
    .unwrap();
    serde_json::from_value(value["reviewed"].clone()).unwrap()
}
fn approval(
    fx: &Fixture,
    pin: ProjectReviewPin,
    strategy: ProjectIntegrationStrategy,
) -> ProjectApproveInput {
    ProjectApproveInput {
        work_id: fx.item.id.to_string(),
        reviewed: pin,
        strategy,
        rationale: "Reviewed sealed hello.txt and verification evidence".into(),
        acknowledged_violations: vec![],
    }
}
fn approve_ff(fx: &Fixture, pin: ProjectReviewPin) -> ReviewDecisionId {
    approve(
        &fx.forge,
        "owner",
        &actor(),
        approval(fx, pin, ProjectIntegrationStrategy::FastForwardOnly),
    )
    .unwrap()
    .1
}
fn advance_base(fx: &Fixture) {
    std::fs::write(fx.repo.join("new.txt"), "other work\n").unwrap();
    git(&fx.repo, &["add", "new.txt"]);
    fx.forge
        .git()
        .commit_checkpoint(&fx.repo, "another commit", &CheckpointAuthor::default())
        .unwrap();
}

#[test]
fn isolated_native_lifecycle_reviews_approves_merges_and_closes() {
    let fx = fixture(WorkspaceMode::Isolated);
    let baseline = fx.forge.git().head_oid(&fx.repo).unwrap();
    let reviewed = prepare(&fx);
    assert_eq!(fx.forge.git().head_oid(&fx.repo).unwrap(), baseline);
    let file = review_file(
        &fx.forge,
        "owner",
        ProjectReviewFileQuery {
            work_id: fx.item.id.to_string(),
            reviewed: reviewed.clone(),
            path: "hello.txt".into(),
            max_bytes: None,
        },
    )
    .unwrap();
    assert!(file["diff"].as_str().unwrap().contains("+after"));
    assert_eq!(file["truncated"], false);
    let decision = approve_ff(&fx, reviewed.clone());
    // Approval is not integration, and exact retries preserve the original record.
    assert_eq!(fx.forge.git().head_oid(&fx.repo).unwrap(), baseline);
    assert_eq!(approve_ff(&fx, reviewed.clone()), decision);
    assert_eq!(
        fx.forge.load(&fx.item.id).unwrap().review_decisions.len(),
        1
    );
    let saved = Forge::open(fx.forge.store().root()).unwrap();
    let result = apply(
        &saved,
        "owner",
        &actor(),
        ProjectApplyInput {
            work_id: fx.item.id.to_string(),
            decision_id: decision.to_string(),
        },
    )
    .unwrap();
    assert_eq!(result.state, WorkState::Accepted);
    assert_eq!(
        result.disposition,
        Some(AcceptedDisposition::BaseFastForwarded)
    );
    assert_eq!(
        saved.git().head_oid(&fx.repo).unwrap().as_str(),
        reviewed.reviewed_head_oid
    );
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("hello.txt")).unwrap(),
        "after\n"
    );
    assert!(result.review_decisions[0].actor.id.contains("taco"));
    // Repeated application cannot integrate twice or reopen accepted work.
    assert!(
        apply(
            &saved,
            "owner",
            &actor(),
            ProjectApplyInput {
                work_id: fx.item.id.to_string(),
                decision_id: decision.to_string()
            }
        )
        .is_err()
    );
    assert_eq!(saved.load(&fx.item.id).unwrap().state, WorkState::Accepted);
}

#[test]
fn attached_acceptance_preserves_head_index_branch_and_user_files() {
    let fx = fixture(WorkspaceMode::AttachedCheckout);
    let head = fx.forge.git().head_oid(&fx.repo).unwrap();
    let index = git(&fx.repo, &["diff", "--cached"]);
    let reviewed = prepare(&fx);
    assert!(
        approve(
            &fx.forge,
            "owner",
            &actor(),
            approval(
                &fx,
                reviewed.clone(),
                ProjectIntegrationStrategy::FastForwardOnly
            )
        )
        .is_err()
    );
    let (_, decision) = approve(
        &fx.forge,
        "owner",
        &actor(),
        approval(&fx, reviewed, ProjectIntegrationStrategy::KeepCheckout),
    )
    .unwrap();
    let accepted = apply(
        &fx.forge,
        "owner",
        &actor(),
        ProjectApplyInput {
            work_id: fx.item.id.to_string(),
            decision_id: decision.to_string(),
        },
    )
    .unwrap();
    assert_eq!(
        accepted.disposition,
        Some(AcceptedDisposition::CheckoutRetained)
    );
    assert_eq!(fx.forge.git().head_oid(&fx.repo).unwrap(), head);
    assert_eq!(git(&fx.repo, &["diff", "--cached"]), index);
    assert_eq!(git(&fx.repo, &["branch", "--show-current"]).trim(), "main");
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("hello.txt")).unwrap(),
        "after\n"
    );
}

#[test]
fn changed_base_rejects_old_review_before_approval() {
    let fx = fixture(WorkspaceMode::Isolated);
    let reviewed = prepare(&fx);
    advance_base(&fx);
    assert!(
        approve(
            &fx.forge,
            "owner",
            &actor(),
            approval(&fx, reviewed, ProjectIntegrationStrategy::FastForwardOnly)
        )
        .is_err()
    );
    assert!(
        fx.forge
            .load(&fx.item.id)
            .unwrap()
            .review_decisions
            .is_empty()
    );
}

#[test]
fn changed_base_after_approval_cannot_be_merged() {
    let fx = fixture(WorkspaceMode::Isolated);
    let decision = approve_ff(&fx, prepare(&fx));
    advance_base(&fx);
    let head = fx.forge.git().head_oid(&fx.repo).unwrap();
    assert!(
        apply(
            &fx.forge,
            "owner",
            &actor(),
            ProjectApplyInput {
                work_id: fx.item.id.to_string(),
                decision_id: decision.to_string()
            }
        )
        .is_err()
    );
    assert_eq!(fx.forge.git().head_oid(&fx.repo).unwrap(), head);
    assert_eq!(
        fx.forge.load(&fx.item.id).unwrap().state,
        WorkState::AwaitingReview
    );
}

#[test]
fn changed_workspace_after_approval_cannot_be_accepted() {
    for mode in [WorkspaceMode::Isolated, WorkspaceMode::AttachedCheckout] {
        let fx = fixture(mode);
        let reviewed = prepare(&fx);
        let strategy = if mode == WorkspaceMode::Isolated {
            ProjectIntegrationStrategy::FastForwardOnly
        } else {
            ProjectIntegrationStrategy::KeepCheckout
        };
        let (_, decision) = approve(
            &fx.forge,
            "owner",
            &actor(),
            approval(&fx, reviewed, strategy),
        )
        .unwrap();
        std::fs::write(
            fx.item
                .workspace_environment()
                .unwrap()
                .worktree
                .join("hello.txt"),
            "unreviewed\n",
        )
        .unwrap();
        assert!(
            apply(
                &fx.forge,
                "owner",
                &actor(),
                ProjectApplyInput {
                    work_id: fx.item.id.to_string(),
                    decision_id: decision.to_string()
                }
            )
            .is_err()
        );
        assert_eq!(
            fx.forge.load(&fx.item.id).unwrap().state,
            WorkState::AwaitingReview
        );
    }
}

#[test]
fn request_changes_invalidates_approval_preserves_workspace_and_requires_new_review() {
    let fx = fixture(WorkspaceMode::Isolated);
    let reviewed = prepare(&fx);
    let decision = approve_ff(&fx, reviewed.clone());
    let ready = request_changes(
        &fx.forge,
        "owner",
        &actor(),
        ProjectRequestChangesInput {
            work_id: fx.item.id.to_string(),
            reviewed: reviewed.clone(),
            reason: "Add another check".into(),
        },
    )
    .unwrap();
    assert_eq!(ready.state, WorkState::Ready);
    assert!(ready.review_decisions.is_empty());
    assert_eq!(ready.changes_requested.len(), 1);
    assert_eq!(
        ready.workspace_environment().unwrap().worktree,
        fx.item.workspace_environment().unwrap().worktree
    );
    assert!(
        apply(
            &fx.forge,
            "owner",
            &actor(),
            ProjectApplyInput {
                work_id: fx.item.id.to_string(),
                decision_id: decision.to_string()
            }
        )
        .is_err()
    );
    std::fs::write(
        ready
            .workspace_environment()
            .unwrap()
            .worktree
            .join("hello.txt"),
        "second pass\n",
    )
    .unwrap();
    prepare_merge(
        &fx.forge,
        "owner",
        &actor(),
        ProjectPrepareMergeInput {
            work_id: fx.item.id.to_string(),
            ack_risks: false,
        },
    )
    .unwrap();
    assert_ne!(pin(&fx).evidence_id, reviewed.evidence_id);
    // Older candidate evidence cannot authorize the newer shared workspace.
    assert!(
        approve(
            &fx.forge,
            "owner",
            &actor(),
            approval(&fx, reviewed, ProjectIntegrationStrategy::FastForwardOnly)
        )
        .is_err()
    );
}

#[test]
fn wrong_owner_or_forged_review_coordinates_cannot_mutate_the_undertaking() {
    let fx = fixture(WorkspaceMode::Isolated);
    let reviewed = prepare(&fx);
    assert!(owned_item(&fx.forge, "stranger", fx.item.id.as_str()).is_err());
    assert!(
        prepare_merge(
            &fx.forge,
            "stranger",
            &actor(),
            ProjectPrepareMergeInput {
                work_id: fx.item.id.to_string(),
                ack_risks: false
            }
        )
        .is_err()
    );
    assert!(
        approve(
            &fx.forge,
            "stranger",
            &actor(),
            approval(
                &fx,
                reviewed.clone(),
                ProjectIntegrationStrategy::FastForwardOnly
            )
        )
        .is_err()
    );
    assert!(
        discard(
            &fx.forge,
            "stranger",
            &actor(),
            ProjectDiscardInput {
                work_id: fx.item.id.to_string()
            }
        )
        .is_err()
    );
    for field in [
        "attempt_id",
        "evidence_id",
        "evidence_digest",
        "baseline_oid",
        "reviewed_head_oid",
        "expected_base_oid",
        "environment_generation",
    ] {
        let mut forged = serde_json::to_value(&reviewed).unwrap();
        forged[field] = if field == "environment_generation" {
            json!(999)
        } else {
            json!("forged")
        };
        let forged = serde_json::from_value(forged).unwrap();
        assert!(
            approve(
                &fx.forge,
                "owner",
                &actor(),
                approval(&fx, forged, ProjectIntegrationStrategy::FastForwardOnly)
            )
            .is_err(),
            "accepted forged {field}"
        );
    }
    assert!(
        fx.forge
            .load(&fx.item.id)
            .unwrap()
            .review_decisions
            .is_empty()
    );
}

#[test]
fn preparation_never_takes_a_running_coder_lease_and_can_complete_the_ui_lease() {
    for kind in ["medousa-coder", "codex", "human"] {
        let fx = fixture(WorkspaceMode::Isolated);
        let (_, lease) = fx
            .forge
            .begin_workspace_attempt(
                &fx.item.id,
                ExecutorDescriptor {
                    kind: kind.into(),
                    detail: json!({}),
                },
                None,
                &actor(),
            )
            .unwrap();
        std::fs::write(
            fx.item
                .workspace_environment()
                .unwrap()
                .worktree
                .join("hello.txt"),
            "working\n",
        )
        .unwrap();
        let prepared = prepare_merge(
            &fx.forge,
            "owner",
            &actor(),
            ProjectPrepareMergeInput {
                work_id: fx.item.id.to_string(),
                ack_risks: false,
            },
        );
        if kind == "human" {
            assert_eq!(prepared.unwrap().state, WorkState::AwaitingReview);
            assert!(!fx.forge.load(&fx.item.id).unwrap().has_active_attempts());
        } else {
            assert!(matches!(prepared, Err(ForgeError::WorkspaceBusy(_))));
            assert!(
                fx.forge
                    .find_lease(&lease.lease_id, lease.generation)
                    .is_ok()
            );
            assert!(fx.forge.discard_if_idle(&fx.item.id, &actor()).is_err());
            assert!(
                fx.forge
                    .find_lease(&lease.lease_id, lease.generation)
                    .is_ok()
            );
            assert!(
                discard(
                    &fx.forge,
                    "owner",
                    &actor(),
                    ProjectDiscardInput {
                        work_id: fx.item.id.to_string()
                    }
                )
                .is_err()
            );
        }
    }
}

#[test]
fn bounded_diff_does_not_read_unsealed_files_or_invent_review_coverage() {
    let fx = fixture(WorkspaceMode::Isolated);
    let reviewed = prepare(&fx);
    let mut input = ProjectReviewFileQuery {
        work_id: fx.item.id.to_string(),
        reviewed,
        path: "hello.txt".into(),
        max_bytes: Some(8),
    };
    assert_eq!(
        review_file(&fx.forge, "owner", input.clone()).unwrap()["truncated"],
        true
    );
    input.path = "../private.txt".into();
    assert!(review_file(&fx.forge, "owner", input.clone()).is_err());
    input.path = "hello.txt".into();
    input.max_bytes = Some(MAX_REVIEW_DIFF_BYTES + 1);
    assert!(review_file(&fx.forge, "owner", input).is_err());
}

#[test]
fn failed_capture_releases_the_assistants_lease_and_keeps_work_available() {
    let fx = fixture(WorkspaceMode::Isolated);
    std::fs::write(fx.item.workspace_environment().unwrap().worktree.join("secret.txt"), "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA00000000000000000000000000000000000000000000\n-----END RSA PRIVATE KEY-----\n").unwrap();
    assert!(
        prepare_merge(
            &fx.forge,
            "owner",
            &actor(),
            ProjectPrepareMergeInput {
                work_id: fx.item.id.to_string(),
                ack_risks: false
            }
        )
        .is_err()
    );
    let item = fx.forge.load(&fx.item.id).unwrap();
    assert!(!item.has_active_attempts());
    assert_eq!(item.state, WorkState::Ready);
    assert!(item.workspace_environment().unwrap().worktree.exists());
}

#[test]
fn explicit_discard_closes_both_modes_without_claiming_a_merge() {
    for mode in [WorkspaceMode::Isolated, WorkspaceMode::AttachedCheckout] {
        let fx = fixture(mode);
        let workspace = fx.item.workspace_environment().unwrap().worktree.clone();
        std::fs::write(workspace.join("hello.txt"), "discarded undertaking\n").unwrap();
        let result = discard(
            &fx.forge,
            "owner",
            &actor(),
            ProjectDiscardInput {
                work_id: fx.item.id.to_string(),
            },
        )
        .unwrap();
        assert_eq!(result.state, WorkState::Discarded);
        assert!(result.disposition.is_none());
        if mode == WorkspaceMode::AttachedCheckout {
            assert_eq!(
                std::fs::read_to_string(fx.repo.join("hello.txt")).unwrap(),
                "discarded undertaking\n"
            );
        }
    }
}

#[test]
fn changed_policy_evidence_cannot_be_approved_or_applied() {
    let fx = fixture(WorkspaceMode::Isolated);
    let reviewed = prepare(&fx);
    let decision = approve_ff(&fx, reviewed.clone());
    let item = fx.forge.load(&fx.item.id).unwrap();
    let attempt = item
        .attempts
        .iter()
        .find(|attempt| attempt.id.as_str() == reviewed.attempt_id)
        .unwrap();
    let path = fx
        .forge
        .store()
        .item_dir(&item.id)
        .join("attempts")
        .join(attempt.seq.to_string())
        .join("evidence/policy.json");
    // Even an equivalent JSON report is no longer the exact captured bytes.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(b'\n');
    std::fs::write(path, bytes).unwrap();
    assert!(
        review(
            &fx.forge,
            "owner",
            ProjectReviewQuery {
                work_id: item.id.to_string(),
                attempt_id: None
            }
        )
        .is_err()
    );
    assert!(
        approve(
            &fx.forge,
            "owner",
            &actor(),
            approval(&fx, reviewed, ProjectIntegrationStrategy::FastForwardOnly)
        )
        .is_err()
    );
    assert!(
        apply(
            &fx.forge,
            "owner",
            &actor(),
            ProjectApplyInput {
                work_id: item.id.to_string(),
                decision_id: decision.to_string()
            }
        )
        .is_err()
    );
    assert_eq!(
        fx.forge.load(&item.id).unwrap().state,
        WorkState::AwaitingReview
    );
}
