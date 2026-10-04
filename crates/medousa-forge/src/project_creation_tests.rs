use super::*;
use crate::model::{ActorKind, ActorRef, WorkPolicy, WorkTarget, WorkspaceMode};

fn empty_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    dir
}
fn actor() -> ActorRef {
    ActorRef {
        kind: ActorKind::User,
        id: "owner".into(),
    }
}
fn register(forge: &Forge, repo: &Path, mode: WorkspaceMode, key: &str) -> Result<WorkItem> {
    forge.register_project_with_request_key(
        "Scaffold".into(),
        "Build the app".into(),
        repo,
        "main".into(),
        "owner".into(),
        WorkPolicy::default(),
        mode,
        &actor(),
        Some(key),
    )
}

#[test]
fn git_init_only_repo_can_prepare_both_workspace_modes_without_creating_an_executor() {
    for mode in [WorkspaceMode::Isolated, WorkspaceMode::AttachedCheckout] {
        let repo = empty_repo();
        let root = tempfile::tempdir().unwrap();
        let forge = Forge::open(root.path()).unwrap();
        let item = register(&forge, repo.path(), mode, "empty").unwrap();
        let ready = forge.provision_project(&item.id, &actor()).unwrap();
        assert_eq!(ready.state, WorkState::Ready);
        assert!(ready.attempts.is_empty());
        assert!(ready.workspace_environment().unwrap().worktree.is_dir());
        assert_eq!(
            forge.git().run(repo.path(), &["ls-tree", "HEAD"]).unwrap(),
            ""
        );
        assert_eq!(
            forge
                .git()
                .run(repo.path(), &["rev-list", "--count", "HEAD"])
                .unwrap()
                .trim(),
            "1"
        );
        assert!(!repo.path().join("README.md").exists());
    }
}

#[test]
fn bootstrap_preserves_staged_unstaged_and_untracked_files_and_the_real_index() {
    let repo = empty_repo();
    let root = tempfile::tempdir().unwrap();
    let forge = Forge::open(root.path()).unwrap();
    let git = forge.git();
    std::fs::write(repo.path().join("staged.txt"), "staged content").unwrap();
    git.run(repo.path(), &["add", "staged.txt"]).unwrap();
    std::fs::write(repo.path().join("staged.txt"), "working content").unwrap();
    std::fs::write(repo.path().join("untracked.txt"), "my file").unwrap();
    let index = std::fs::read(repo.path().join(".git/index")).unwrap();
    let status = git.run(repo.path(), &["status", "--porcelain"]).unwrap();
    let item = register(
        &forge,
        repo.path(),
        WorkspaceMode::AttachedCheckout,
        "preserve",
    )
    .unwrap();
    forge.provision_project(&item.id, &actor()).unwrap();
    assert_eq!(
        std::fs::read(repo.path().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        git.run(repo.path(), &["status", "--porcelain"]).unwrap(),
        status
    );
    assert_eq!(
        git.run(repo.path(), &["show", ":staged.txt"]).unwrap(),
        "staged content"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("staged.txt")).unwrap(),
        "working content"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("untracked.txt")).unwrap(),
        "my file"
    );
    assert!(
        git.run(repo.path(), &["ls-tree", "HEAD"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn creation_retry_survives_restart_and_head_movement_and_rejects_changed_intent() {
    let repo = empty_repo();
    let root = tempfile::tempdir().unwrap();
    let forge = Forge::open(root.path()).unwrap();
    let item = register(&forge, repo.path(), WorkspaceMode::Isolated, "retry").unwrap();
    let ready = forge.provision_project(&item.id, &actor()).unwrap();
    drop(forge);
    let forge = Forge::open(root.path()).unwrap();
    forge
        .git()
        .run(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@local",
                "commit",
                "--allow-empty",
                "-m",
                "User commit",
            ],
        )
        .unwrap();
    let replay = register(&forge, repo.path(), WorkspaceMode::Isolated, "retry").unwrap();
    assert_eq!(replay.id, item.id);
    assert_eq!(
        serde_json::to_value(&replay.target).unwrap(),
        serde_json::to_value(&ready.target).unwrap()
    );
    assert_eq!(forge.list().unwrap().len(), 1);
    assert!(
        register(
            &forge,
            repo.path(),
            WorkspaceMode::AttachedCheckout,
            "retry"
        )
        .is_err()
    );
    assert!(
        forge
            .register_project_with_request_key(
                "Different".into(),
                "Build the app".into(),
                repo.path(),
                "main".into(),
                "owner".into(),
                WorkPolicy::default(),
                WorkspaceMode::Isolated,
                &actor(),
                Some("retry")
            )
            .is_err()
    );
}

#[test]
fn concurrent_exact_creation_retries_prepare_one_item_and_one_workspace() {
    let repo = empty_repo();
    let root = tempfile::tempdir().unwrap();
    let forge = Arc::new(Forge::open(root.path()).unwrap());
    let mut threads = Vec::new();
    for _ in 0..2 {
        let forge = forge.clone();
        let path = repo.path().to_path_buf();
        threads.push(std::thread::spawn(move || {
            let item = register(&forge, &path, WorkspaceMode::Isolated, "parallel").unwrap();
            forge.provision_project(&item.id, &actor()).unwrap()
        }));
    }
    let first = threads.remove(0).join().unwrap();
    let second = threads.remove(0).join().unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(
        first.workspace_environment(),
        second.workspace_environment()
    );
    assert_eq!(forge.list().unwrap().len(), 1);
}

#[test]
fn wrong_unborn_branch_and_missing_existing_branch_never_bootstrap() {
    let repo = empty_repo();
    let root = tempfile::tempdir().unwrap();
    let forge = Forge::open(root.path()).unwrap();
    let request = |base: &str| {
        forge.register_project_with_request_key(
            "Scaffold".into(),
            "Build the app".into(),
            repo.path(),
            base.into(),
            "owner".into(),
            WorkPolicy::default(),
            WorkspaceMode::Isolated,
            &actor(),
            None,
        )
    };
    assert!(matches!(
        request("missing"),
        Err(ForgeError::BaseRefMissing { .. })
    ));
    assert!(!forge.git().has_commits(repo.path()).unwrap());
    let item = request("main").unwrap();
    let WorkTarget::Git(target) = item.target;
    assert!(matches!(
        request("missing"),
        Err(ForgeError::BaseRefMissing { .. })
    ));
    assert_eq!(forge.git().head_oid(repo.path()).unwrap(), target.base_oid);
}

#[test]
fn a_closed_creation_key_cannot_silently_start_another_undertaking() {
    let repo = empty_repo();
    let root = tempfile::tempdir().unwrap();
    let forge = Forge::open(root.path()).unwrap();
    let item = register(&forge, repo.path(), WorkspaceMode::Isolated, "closed").unwrap();
    forge.discard(&item.id, &actor()).unwrap();
    assert!(register(&forge, repo.path(), WorkspaceMode::Isolated, "closed").is_err());
    assert_eq!(forge.list().unwrap().len(), 1);
}

#[test]
fn omitted_creation_base_remains_pinned_across_checkout_changes_and_owner_namespaces() {
    let repo = empty_repo();
    let root = tempfile::tempdir().unwrap();
    let forge = Forge::open(root.path()).unwrap();
    let item = register(&forge, repo.path(), WorkspaceMode::Isolated, "pin").unwrap();
    forge
        .git()
        .run(repo.path(), &["checkout", "-b", "other"])
        .unwrap();
    assert_eq!(
        forge
            .project_creation_base_ref("owner", "pin")
            .unwrap()
            .as_deref(),
        Some("main")
    );
    assert!(
        forge
            .project_creation_base_ref("different-owner", "pin")
            .unwrap()
            .is_none()
    );
    let replay = register(&forge, repo.path(), WorkspaceMode::Isolated, "pin").unwrap();
    assert_eq!(replay.id, item.id);
    assert!(register(&forge, repo.path(), WorkspaceMode::Isolated, "bad\nkey").is_err());
}
