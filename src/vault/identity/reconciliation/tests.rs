use super::super::tests::{by_id, fixture, note, write};
use super::*;
use crate::vault::{
    contracts::{MutationPrecondition, VaultRootId},
    mutation::{WriteMutation, commit_write},
    relocate::{relocate_delete, relocate_move},
};
use medousa_store::{
    PersistenceError, PersistenceErrorKind, TransactionFaultPoint, TransactionFaults,
};
use std::sync::{Arc, atomic::AtomicBool};

fn key(n: usize) -> VaultReconciliationKey {
    VaultReconciliationKey {
        id: format!("{n:064x}"),
        digest: format!("{:064x}", n + 1000),
    }
}
fn token(owner: &VaultIndexOwner, id: &str) -> String {
    let tx = VaultIdentityTransaction::begin(owner, true).unwrap();
    format!(
        "vault-observation-v1:{}:{}",
        tx.vault_id(),
        tx.retained_resource(id).unwrap().revision
    )
}
pub(crate) struct FailPublication {
    pub path: std::path::PathBuf,
    pub create_only: bool,
    pub fired: AtomicBool,
}
impl TransactionFaults for FailPublication {
    fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
        let point_matches = if self.create_only {
            point == TransactionFaultPoint::AfterCreateOnly
        } else {
            point == TransactionFaultPoint::AfterSnapshotPublish
        };
        if point_matches
            && std::fs::read(&self.path).ok().as_deref() == Some(b"new")
            && !self.fired.swap(true, Ordering::SeqCst)
        {
            return Err(PersistenceError::new(
                PersistenceErrorKind::RetryableIo,
                "native publication lost its witness fence",
            ));
        }
        Ok(())
    }
}
fn uncertain(
    create_only: bool,
) -> (
    tempfile::TempDir,
    Arc<VaultIndexOwner>,
    Option<VaultResourceIdentity>,
    PendingVaultJournal,
) {
    let (dir, owner) = fixture();
    let prior = if create_only {
        None
    } else {
        write(&owner, "note.md", "old");
        Some(note(&owner, "note.md"))
    };
    owner.set_transaction(FileTransaction::with_faults(
        owner.files.clone(),
        Arc::new(FailPublication {
            path: dir.path().join("note.md"),
            create_only,
            fired: AtomicBool::new(false),
        }),
    ));
    assert!(
        commit_write(
            &owner,
            WriteMutation {
                path: "note.md".into(),
                content: "new".into(),
                precondition: if create_only {
                    MutationPrecondition::CreateOnly
                } else {
                    MutationPrecondition::Unconditional
                },
                expected_version: None
            }
        )
        .is_err()
    );
    owner.set_transaction(FileTransaction::new(owner.files.clone()));
    assert!(crate::vault::mutation::recover_all_pending_writes(&owner).is_err());
    let pending = VaultIdentityTransaction::begin(&owner, true)
        .unwrap()
        .inspect_pending()
        .unwrap()
        .remove(0);
    assert!(!pending.has_publication_witness);
    (dir, owner, prior, pending)
}

#[test]
fn adoption_keeps_identity_and_replays_without_rolling_back_later_moves() {
    let (dir, owner) = fixture();
    write(&owner, "a.md", "body");
    let original = note(&owner, "a.md");
    std::fs::rename(dir.path().join("a.md"), dir.path().join("b.md")).unwrap();
    let expected = token(&owner, &original.resource_id);
    let receipt = VaultIdentityTransaction::begin(&owner, false)
        .unwrap()
        .reconcile_move(
            key(1),
            &original.resource_id,
            &expected,
            &VaultPath::parse("b.md").unwrap(),
        )
        .unwrap();
    assert!(!receipt.replayed);
    assert_eq!(by_id(&owner, &original.resource_id).path, "b.md");
    relocate_move(&owner, "b.md", "c.md").unwrap();
    let restarted = VaultIndexOwner::new(VaultRootId::new("restarted"), owner.files.clone());
    assert!(
        VaultIdentityTransaction::begin(&restarted, false)
            .unwrap()
            .reconcile_move(
                key(1),
                &original.resource_id,
                &expected,
                &VaultPath::parse("b.md").unwrap()
            )
            .unwrap()
            .replayed
    );
    assert_eq!(by_id(&restarted, &original.resource_id).path, "c.md");
    let mut changed = key(1);
    changed.digest = key(2).digest;
    assert!(matches!(
        VaultIdentityTransaction::begin(&restarted, false)
            .unwrap()
            .reconcile_move(
                changed,
                &original.resource_id,
                &expected,
                &VaultPath::parse("b.md").unwrap()
            ),
        Err(VaultMutationError::Conflict(_))
    ));
}

#[test]
fn equal_content_stale_revision_tombstone_and_symlink_never_authorize_adoption() {
    let (dir, owner) = fixture();
    write(&owner, "a.md", "same");
    let original = note(&owner, "a.md");
    let stale = token(&owner, &original.resource_id);
    std::fs::rename(dir.path().join("a.md"), dir.path().join("b.md")).unwrap();
    std::fs::write(dir.path().join("copy.md"), b"same").unwrap();
    by_id(&owner, &original.resource_id);
    let expected = token(&owner, &original.resource_id);
    assert!(matches!(
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .reconcile_move(
                key(1),
                &original.resource_id,
                &stale,
                &VaultPath::parse("b.md").unwrap()
            ),
        Err(VaultMutationError::Conflict(_))
    ));
    assert!(matches!(
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .reconcile_move(
                key(2),
                &original.resource_id,
                &expected,
                &VaultPath::parse("copy.md").unwrap()
            ),
        Err(VaultMutationError::ExternallyAmbiguous(_))
    ));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("b.md"), dir.path().join("linked.md")).unwrap();
        assert!(
            VaultIdentityTransaction::begin(&owner, false)
                .unwrap()
                .reconcile_move(
                    key(3),
                    &original.resource_id,
                    &expected,
                    &VaultPath::parse("linked.md").unwrap()
                )
                .is_err()
        );
    }
    VaultIdentityTransaction::begin(&owner, false)
        .unwrap()
        .reconcile_move(
            key(4),
            &original.resource_id,
            &expected,
            &VaultPath::parse("b.md").unwrap(),
        )
        .unwrap();
    relocate_delete(&owner, "b.md").unwrap();
    std::fs::rename(
        dir.path().join(".trash/b.md"),
        dir.path().join("resurrected.md"),
    )
    .unwrap();
    let tombstoned = token(&owner, &original.resource_id);
    assert!(matches!(
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .reconcile_move(
                key(5),
                &original.resource_id,
                &tombstoned,
                &VaultPath::parse("resurrected.md").unwrap()
            ),
        Err(VaultMutationError::Conflict(_))
    ));
}

#[test]
fn folder_adoption_is_exact_and_does_not_rebase_unselected_descendants() {
    let (dir, owner) = fixture();
    write(&owner, "folder/note.md", "body");
    let note = note(&owner, "folder/note.md");
    let folder = VaultIdentityTransaction::begin(&owner, false)
        .unwrap()
        .observe_path(
            &VaultPath::parse("folder").unwrap(),
            VaultResourceKind::Folder,
        )
        .unwrap();
    std::fs::rename(dir.path().join("folder"), dir.path().join("moved")).unwrap();
    let expected = token(&owner, &folder.resource_id);
    VaultIdentityTransaction::begin(&owner, false)
        .unwrap()
        .reconcile_move(
            key(1),
            &folder.resource_id,
            &expected,
            &VaultPath::parse("moved").unwrap(),
        )
        .unwrap();
    assert_eq!(by_id(&owner, &folder.resource_id).path, "moved");
    let child = by_id(&owner, &note.resource_id);
    assert_eq!(child.path, "folder/note.md");
    assert_eq!(child.resolution, ResourceResolution::Unavailable);
}

#[test]
fn quarantine_retains_evidence_and_never_assigns_an_unproven_id_to_current_bytes() {
    for create_only in [false, true] {
        let (dir, owner, prior, pending) = uncertain(create_only);
        let intent_path = operation_path("intents", &pending.operation_id).unwrap();
        let original = owner.files.read(&intent_path).unwrap();
        let receipt = VaultIdentityTransaction::begin(&owner, true)
            .unwrap()
            .quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
            .unwrap();
        assert!(!receipt.replayed);
        let archive: QuarantinedJournal = serde_json::from_slice(
            &owner
                .files
                .read(&operation_path("reconciliations", &pending.operation_id).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(archive.original_intent.as_bytes(), original);
        assert!(archive.original_receipt.is_none());
        assert_eq!(std::fs::read(dir.path().join("note.md")).unwrap(), b"new");
        let current = note(&owner, "note.md");
        assert_ne!(Some(current.resource_id.clone()), pending.resource_id);
        if let Some(prior) = prior {
            assert_eq!(
                by_id(&owner, &prior.resource_id).resolution,
                ResourceResolution::Unavailable
            );
            assert_ne!(current.resource_id, prior.resource_id);
        }
        write(&owner, "unblocked.md", "next native mutation");
        let restarted = VaultIndexOwner::new(VaultRootId::new("restarted"), owner.files.clone());
        assert!(
            VaultIdentityTransaction::begin(&restarted, false)
                .unwrap()
                .quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
                .unwrap()
                .replayed
        );
    }
}

#[test]
fn archive_before_snapshot_failure_retains_intent_and_failed_transaction_cannot_clear_it() {
    let (_dir, owner, prior, pending) = uncertain(false);
    let mut tx = VaultIdentityTransaction::begin(&owner, true).unwrap();
    owner.identity_persist_fault.store(true, Ordering::SeqCst);
    assert!(
        tx.quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
            .is_err()
    );
    assert!(tx.finish_recorded_quarantines().is_err());
    assert!(
        tx.quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
            .is_err()
    );
    drop(tx);
    assert!(
        owner
            .files
            .is_file(&operation_path("intents", &pending.operation_id).unwrap())
            .unwrap()
    );
    assert!(
        owner
            .files
            .is_file(&operation_path("reconciliations", &pending.operation_id).unwrap())
            .unwrap()
    );
    owner.identity_persist_fault.store(false, Ordering::SeqCst);
    let restarted = VaultIndexOwner::new(VaultRootId::new("restarted"), owner.files.clone());
    VaultIdentityTransaction::begin(&restarted, true)
        .unwrap()
        .quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
        .unwrap();
    assert_eq!(
        by_id(&restarted, &prior.unwrap().resource_id).resolution,
        ResourceResolution::Unavailable
    );
}

#[test]
fn restart_finishes_snapshot_committed_cleanup_without_repeating_file_mutation() {
    let (dir, owner, _, pending) = uncertain(false);
    let path = operation_path("intents", &pending.operation_id).unwrap();
    let original = owner.files.read(&path).unwrap();
    VaultIdentityTransaction::begin(&owner, true)
        .unwrap()
        .quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
        .unwrap();
    // Simulate a crash after the receipt fence, before removing the intent.
    owner.files.atomic_write(&path, &original).unwrap();
    let restarted = VaultIndexOwner::new(VaultRootId::new("restarted"), owner.files.clone());
    let tx = VaultIdentityTransaction::begin(&restarted, false).unwrap();
    assert!(tx.inspect_pending().unwrap().is_empty());
    drop(tx);
    assert_eq!(std::fs::read(dir.path().join("note.md")).unwrap(), b"new");
    write(&restarted, "after-restart.md", "unblocked");
}

#[test]
fn changed_intent_or_quarantine_archive_is_retained_without_reset() {
    let (_dir, owner, _, pending) = uncertain(false);
    assert!(matches!(
        VaultIdentityTransaction::begin(&owner, true)
            .unwrap()
            .quarantine_journal(key(1), &pending.operation_id, &"0".repeat(64)),
        Err(VaultMutationError::Conflict(_))
    ));
    let path = operation_path("intents", &pending.operation_id).unwrap();
    let original = owner.files.read(&path).unwrap();
    VaultIdentityTransaction::begin(&owner, true)
        .unwrap()
        .quarantine_journal(key(1), &pending.operation_id, &pending.intent_digest)
        .unwrap();
    let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
    changed["content_digest"] = serde_json::json!("different");
    owner
        .files
        .atomic_write(&path, &serde_json::to_vec(&changed).unwrap())
        .unwrap();
    assert!(matches!(
        VaultIdentityTransaction::begin(&owner, true),
        Err(VaultMutationError::Conflict(_))
    ));
    assert_eq!(
        owner.files.read(&path).unwrap(),
        serde_json::to_vec(&changed).unwrap()
    );
}

#[test]
fn ambiguous_startup_keeps_native_reads_available_but_blocks_new_writes() {
    let (dir, _owner, _, _) = uncertain(false);
    let restarted =
        crate::vault::owner::ensure_owner_for_root(dir.path().canonicalize().unwrap()).unwrap();
    assert_eq!(
        restarted
            .files
            .read(&VaultPath::parse("note.md").unwrap())
            .unwrap(),
        b"new"
    );
    assert!(matches!(
        commit_write(
            &restarted,
            WriteMutation {
                path: "other.md".into(),
                content: "body".into(),
                precondition: MutationPrecondition::Unconditional,
                expected_version: None
            }
        ),
        Err(VaultMutationError::ExternallyAmbiguous(_))
    ));
}

#[test]
fn full_receipt_history_denies_new_adoption_but_preserves_replay_and_native_writes() {
    let (dir, owner) = fixture();
    write(&owner, "a.md", "body");
    let original = note(&owner, "a.md");
    let mut current = "a.md";
    let mut last_expected = String::new();
    for index in 1..=MAX_RECONCILIATIONS {
        let next = if current == "a.md" { "b.md" } else { "a.md" };
        let expected = token(&owner, &original.resource_id);
        std::fs::rename(dir.path().join(current), dir.path().join(next)).unwrap();
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .reconcile_move(
                key(index),
                &original.resource_id,
                &expected,
                &VaultPath::parse(next).unwrap(),
            )
            .unwrap();
        last_expected = expected;
        current = next;
    }
    let next = if current == "a.md" { "b.md" } else { "a.md" };
    let expected = token(&owner, &original.resource_id);
    std::fs::rename(dir.path().join(current), dir.path().join(next)).unwrap();
    assert!(matches!(
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .reconcile_move(
                key(MAX_RECONCILIATIONS + 1),
                &original.resource_id,
                &expected,
                &VaultPath::parse(next).unwrap()
            ),
        Err(VaultMutationError::Overloaded)
    ));
    assert!(
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .reconcile_move(
                key(MAX_RECONCILIATIONS),
                &original.resource_id,
                &last_expected,
                &VaultPath::parse(current).unwrap()
            )
            .unwrap()
            .replayed
    );
    write(&owner, "new.md", "native writes still admitted");
    assert_eq!(
        VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .snapshot
            .reconciliations
            .len(),
        MAX_RECONCILIATIONS
    );
}
