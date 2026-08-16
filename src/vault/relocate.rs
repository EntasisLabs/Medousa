//! Journaled move / delete / restore (H07.1c).

use std::sync::Arc;

use medousa_store::{DurabilityLevel, StorePath};
use serde::{Deserialize, Serialize};

use crate::vault::contracts::{VaultCommitOutcome, VaultMutationError, vault_receipt};
use crate::vault::note::VaultNoteSource;
use crate::vault::owner::{VaultChangeRecord, VaultIndexOwner};
use crate::vault::path::VaultPath;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelocateKind {
    Move,
    Delete,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocateIntent {
    pub operation_id: String,
    pub kind: RelocateKind,
    pub root_id: String,
    pub source: String,
    pub destination: Option<String>,
    pub vault_generation: u64,
}

pub fn relocate_move(
    owner: &Arc<VaultIndexOwner>,
    source: &str,
    destination: &str,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let source_path =
        VaultPath::parse(source).map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let dest_path = VaultPath::parse(destination)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    commit_relocate(owner, RelocateKind::Move, source_path, Some(dest_path))
}

pub fn relocate_delete(
    owner: &Arc<VaultIndexOwner>,
    source: &str,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let source_path =
        VaultPath::parse(source).map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let trash = unique_trash_path(owner, &source_path)?;
    commit_relocate(owner, RelocateKind::Delete, source_path, Some(trash))
}

pub fn relocate_restore(
    owner: &Arc<VaultIndexOwner>,
    source: &str,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let source_path =
        VaultPath::parse(source).map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let trash = source_path.trash_path();
    commit_relocate(owner, RelocateKind::Restore, trash, Some(source_path))
}

fn commit_relocate(
    owner: &Arc<VaultIndexOwner>,
    kind: RelocateKind,
    source: VaultPath,
    destination: Option<VaultPath>,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let _admit = owner.admission.admit_mutation()?;
    let mut keys = vec![owner.lane_key(VaultNoteSource::User, source.as_str())];
    if let Some(dest) = &destination {
        keys.push(owner.lane_key(VaultNoteSource::User, dest.as_str()));
    }
    let _guard = owner.lanes.acquire(keys)?;

    let operation_id = uuid::Uuid::new_v4().simple().to_string();
    let intent = RelocateIntent {
        operation_id: operation_id.clone(),
        kind,
        root_id: owner.root_id.as_str().to_string(),
        source: source.to_string(),
        destination: destination.as_ref().map(|path| path.to_string()),
        vault_generation: owner.current_generation(),
    };
    let intent_path = StorePath::parse(&format!(".medousa/vault/intents/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let intent_bytes = serde_json::to_vec(&intent)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let tx = owner.transaction();
    tx.write_intent(&intent_path, &intent_bytes, DurabilityLevel::Synced)?;

    let dest = destination
        .ok_or_else(|| VaultMutationError::Invalid("relocate destination required".into()))?;
    if !owner.files.is_file(&source).unwrap_or(false) {
        let _ = tx.root().remove_file(&intent_path);
        return Err(VaultMutationError::Invalid(format!(
            "source not found: {source}"
        )));
    }

    // Destination publication is create-only (no-replace). An external create
    // that lands between the existence check and publish loses the race here
    // with Conflict; winner bytes are preserved.
    if let Err(error) = match kind {
        RelocateKind::Restore => tx.restore_path(&source, &dest, DurabilityLevel::Synced),
        RelocateKind::Move | RelocateKind::Delete => {
            tx.move_path_create_only(&source, &dest, DurabilityLevel::Synced)
        }
    } {
        let _ = tx.root().remove_file(&intent_path);
        return Err(error.into());
    }

    let vault_generation = match owner.bump_generation() {
        Ok(generation) => generation,
        Err(_) => {
            return Ok(VaultCommitOutcome {
                receipt: vault_receipt(
                    format!("relocate:{operation_id}"),
                    owner.current_generation(),
                    0,
                    DurabilityLevel::Synced,
                ),
                note_version: crate::vault::contracts::NoteVersion::from_digest(operation_id),
                vault_generation: owner.current_generation(),
                index_repair_required: true,
            });
        }
    };
    let receipt_path = StorePath::parse(&format!(".medousa/vault/receipts/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let receipt_bytes = serde_json::to_vec(&intent)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let receipt = vault_receipt(
        format!("relocate:{operation_id}"),
        vault_generation,
        0,
        DurabilityLevel::Synced,
    );
    let index_repair_required =
        match tx.write_receipt(&receipt_path, &receipt_bytes, DurabilityLevel::Synced) {
            Ok(_) => {
                let _ = tx.root().remove_file(&intent_path);
                false
            }
            Err(_) => true,
        };

    match kind {
        RelocateKind::Move => {
            owner.record_change(VaultChangeRecord {
                generation: vault_generation,
                path: source.to_string(),
                kind: "delete".into(),
                note_version: None,
            });
            owner.record_change(VaultChangeRecord {
                generation: vault_generation,
                path: dest.to_string(),
                kind: "upsert".into(),
                note_version: Some(
                    crate::vault::contracts::NoteVersion::from_digest(&operation_id)
                        .as_str()
                        .to_string(),
                ),
            });
        }
        RelocateKind::Delete => {
            owner.record_change(VaultChangeRecord {
                generation: vault_generation,
                path: source.to_string(),
                kind: "delete".into(),
                note_version: None,
            });
        }
        RelocateKind::Restore => {
            owner.record_change(VaultChangeRecord {
                generation: vault_generation,
                path: dest.to_string(),
                kind: "upsert".into(),
                note_version: None,
            });
        }
    }

    Ok(VaultCommitOutcome {
        receipt,
        note_version: crate::vault::contracts::NoteVersion::from_digest(operation_id),
        vault_generation,
        index_repair_required,
    })
}

/// Complete or abandon in-flight relocate intents after a crash.
pub fn recover_pending_relocate(
    owner: &Arc<VaultIndexOwner>,
    operation_id: &str,
) -> Result<Option<VaultCommitOutcome>, VaultMutationError> {
    let intent_path = StorePath::parse(&format!(".medousa/vault/intents/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let receipt_path = StorePath::parse(&format!(".medousa/vault/receipts/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let tx = owner.transaction();
    if tx.root().is_file(&receipt_path).unwrap_or(false) {
        let _ = tx.root().remove_file(&intent_path);
        return Ok(None);
    }
    if !tx.root().is_file(&intent_path).unwrap_or(false) {
        return Ok(None);
    }
    let bytes = tx
        .root()
        .read_limited(&intent_path, 64 * 1024)
        .map_err(VaultMutationError::from)?;
    let intent: RelocateIntent = serde_json::from_slice(&bytes)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let Some(dest_raw) = intent.destination.clone() else {
        let _ = tx.root().remove_file(&intent_path);
        return Ok(None);
    };
    let source = VaultPath::parse(&intent.source)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let dest = VaultPath::parse(&dest_raw)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let source_exists = tx.root().is_file(&source).unwrap_or(false);
    let dest_exists = tx.root().is_file(&dest).unwrap_or(false);
    if dest_exists && !source_exists {
        // Move/delete already published; write receipt. Keep intent until
        // the receipt is durable.
        let vault_generation = owner.bump_generation()?;
        let receipt_bytes = serde_json::to_vec(&intent)
            .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        tx.write_receipt(&receipt_path, &receipt_bytes, DurabilityLevel::Synced)?;
        let _ = tx.root().remove_file(&intent_path);
        return Ok(Some(VaultCommitOutcome {
            receipt: vault_receipt(
                format!("relocate:{operation_id}"),
                vault_generation,
                0,
                DurabilityLevel::Synced,
            ),
            note_version: crate::vault::contracts::NoteVersion::from_digest(operation_id),
            vault_generation,
            index_repair_required: true,
        }));
    }
    // Incomplete: drop intent; do not invent a new mutation.
    let _ = tx.root().remove_file(&intent_path);
    Ok(None)
}

fn unique_trash_path(
    owner: &VaultIndexOwner,
    source: &VaultPath,
) -> Result<VaultPath, VaultMutationError> {
    let base = source.trash_path();
    if !owner.files.is_file(&base).unwrap_or(false) {
        return Ok(base);
    }
    // Never clobber an existing trash entry.
    for attempt in 0..32 {
        let candidate = VaultPath::internal(&format!(
            "{}.{}",
            base.as_str(),
            uuid::Uuid::new_v4().simple()
        ))
        .or_else(|_| {
            // Fall back to a sibling unique name under .trash
            VaultPath::internal(&format!(
                ".trash/{}-{attempt}.md",
                source.as_str().replace('/', "_")
            ))
        })
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        if !owner.files.is_file(&candidate).unwrap_or(false) {
            return Ok(candidate);
        }
    }
    Err(VaultMutationError::Overloaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::contracts::{MutationPrecondition, VaultRootId};
    use crate::vault::mutation::{WriteMutation, commit_write};
    use medousa_store::StoreRoot;
    use tempfile::tempdir;

    fn owner() -> (tempfile::TempDir, Arc<VaultIndexOwner>) {
        let dir = tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        (dir, VaultIndexOwner::new(VaultRootId::new("test"), root))
    }

    #[test]
    fn delete_never_clobbers_existing_trash_entry() {
        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "keep.md".into(),
                content: "first\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        relocate_delete(&owner, "keep.md").unwrap();
        commit_write(
            &owner,
            WriteMutation {
                path: "keep.md".into(),
                content: "second\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        relocate_delete(&owner, "keep.md").unwrap();
        let trash_root = owner
            .files
            .list_directory_utf8(&VaultPath::internal(".trash").unwrap());
        let entries = trash_root.unwrap();
        assert!(entries.len() >= 2, "both trash versions retained");
    }

    #[test]
    fn move_is_one_operation() {
        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "from.md".into(),
                content: "body\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        relocate_move(&owner, "from.md", "to.md").unwrap();
        assert!(
            !owner
                .files
                .is_file(&VaultPath::parse("from.md").unwrap())
                .unwrap()
        );
        assert!(
            owner
                .files
                .is_file(&VaultPath::parse("to.md").unwrap())
                .unwrap()
        );
    }

    #[test]
    fn receipt_fault_after_move_is_repair_required_and_startup_recovers() {
        use medousa_store::{
            FileTransaction, NoTransactionFaults, PersistenceError, PersistenceErrorKind,
            TransactionFaultPoint, TransactionFaults,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FailAt {
            target: TransactionFaultPoint,
            hits: AtomicUsize,
        }
        impl TransactionFaults for FailAt {
            fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
                self.hits.fetch_add(1, Ordering::Relaxed);
                if point == self.target {
                    return Err(PersistenceError::new(
                        PersistenceErrorKind::RetryableIo,
                        format!("injected {point:?}"),
                    ));
                }
                Ok(())
            }
        }

        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "from.md".into(),
                content: "body\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        let root = Arc::clone(&owner.files);
        owner.set_transaction(FileTransaction::with_faults(
            Arc::clone(&root),
            Arc::new(FailAt {
                target: TransactionFaultPoint::BeforeReceipt,
                hits: AtomicUsize::new(0),
            }) as Arc<dyn TransactionFaults>,
        ));
        let outcome = relocate_move(&owner, "from.md", "to.md").expect("move published");
        assert!(outcome.index_repair_required);
        assert!(
            owner
                .files
                .is_file(&VaultPath::parse("to.md").unwrap())
                .unwrap()
        );
        assert!(
            !owner
                .files
                .is_file(&VaultPath::parse("from.md").unwrap())
                .unwrap()
        );
        let recover_err = crate::vault::mutation::recover_all_pending_writes(&owner).unwrap_err();
        assert!(matches!(
            recover_err,
            crate::vault::contracts::VaultMutationError::Persistence(_)
        ));
        owner.set_transaction(FileTransaction::with_faults(
            Arc::clone(&root),
            Arc::new(NoTransactionFaults),
        ));
        let recovered = crate::vault::mutation::recover_all_pending_writes(&owner).unwrap();
        assert_eq!(recovered.len(), 1);
        assert!(recovered[0].index_repair_required);
    }

    #[test]
    fn competing_destination_create_preserves_winner_bytes() {
        use medousa_store::{
            FileTransaction, PersistenceError, StorePath, TransactionFaultPoint, TransactionFaults,
        };

        struct PlantDestination {
            root: Arc<StoreRoot>,
            dest: StorePath,
        }
        impl TransactionFaults for PlantDestination {
            fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
                if point == TransactionFaultPoint::BeforeRenamePublish {
                    self.root
                        .atomic_create(&self.dest, b"external\n")
                        .expect("plant competing destination");
                }
                Ok(())
            }
        }

        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "from.md".into(),
                content: "source\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        let root = Arc::clone(&owner.files);
        owner.set_transaction(FileTransaction::with_faults(
            Arc::clone(&root),
            Arc::new(PlantDestination {
                root: Arc::clone(&root),
                dest: StorePath::parse("to.md").unwrap(),
            }) as Arc<dyn TransactionFaults>,
        ));
        let error = relocate_move(&owner, "from.md", "to.md").unwrap_err();
        assert!(
            matches!(error, VaultMutationError::Conflict(_)),
            "got {error:?}"
        );
        assert_eq!(
            owner
                .files
                .read_limited(&VaultPath::parse("to.md").unwrap(), 64)
                .unwrap(),
            b"external\n"
        );
        assert_eq!(
            owner
                .files
                .read_limited(&VaultPath::parse("from.md").unwrap(), 64)
                .unwrap(),
            b"source\n"
        );
    }
}
