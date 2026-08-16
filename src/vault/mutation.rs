//! Atomic create/update with durable intent → publication → receipt (H07.1b).

use std::sync::Arc;

use medousa_store::{DurabilityLevel, StorePath};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::vault::contracts::{
    MutationPrecondition, NoteVersion, VaultCommitOutcome, VaultMutationError, VaultMutationIntent,
    VaultMutationReceiptRecord, vault_receipt,
};
use crate::vault::note::{VaultNoteSource, content_hash};
use crate::vault::owner::VaultIndexOwner;
use crate::vault::path::VaultPath;

#[derive(Debug, Clone)]
pub struct WriteMutation {
    pub path: String,
    pub content: String,
    pub precondition: MutationPrecondition,
    pub expected_version: Option<NoteVersion>,
}

pub fn commit_write(
    owner: &Arc<VaultIndexOwner>,
    mutation: WriteMutation,
) -> Result<VaultCommitOutcome, VaultMutationError> {
    let _admit = owner.admission.admit_mutation()?;
    let path = VaultPath::parse(&mutation.path)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let normalized = path.to_string();
    let lane = owner.lane_key(VaultNoteSource::User, &normalized);
    let _guard = owner.lanes.acquire(vec![lane.clone()])?;
    owner.lanes.mark_pending_intent(&lane, true);

    let result = (|| {
        let existing = read_existing(owner, &path)?;
        check_precondition(&mutation, existing.as_ref())?;

        let operation_id = uuid::Uuid::new_v4().simple().to_string();
        let digest = content_digest(&mutation.content);
        let intent_generation = owner.current_generation();
        let intent = VaultMutationIntent {
            operation_id: operation_id.clone(),
            root_id: owner.root_id.as_str().to_string(),
            path: normalized.clone(),
            precondition: mutation.precondition,
            expected_version: mutation
                .expected_version
                .as_ref()
                .map(|v| v.as_str().to_string()),
            content_digest: digest.clone(),
            vault_generation: intent_generation,
        };
        let intent_path = intent_store_path(&operation_id)?;
        let intent_bytes = serde_json::to_vec(&intent)
            .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        let tx = owner.transaction();
        tx.write_intent(&intent_path, &intent_bytes, DurabilityLevel::Synced)?;

        match mutation.precondition {
            MutationPrecondition::CreateOnly => {
                tx.create_only(&path, mutation.content.as_bytes(), DurabilityLevel::Synced)?;
            }
            _ => {
                tx.replace_snapshot(&path, mutation.content.as_bytes(), DurabilityLevel::Synced)?;
            }
        }

        let vault_generation = owner.bump_generation();
        let note_version = NoteVersion::from_digest(content_hash(&mutation.content));
        let receipt_record = VaultMutationReceiptRecord {
            operation_id: operation_id.clone(),
            root_id: owner.root_id.as_str().to_string(),
            path: normalized.clone(),
            note_version: note_version.as_str().to_string(),
            vault_generation,
            bytes: mutation.content.len(),
        };
        let receipt_path = receipt_store_path(&operation_id)?;
        let receipt_bytes = serde_json::to_vec(&receipt_record)
            .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        tx.write_receipt(&receipt_path, &receipt_bytes, DurabilityLevel::Synced)?;
        let _ = tx.root().remove_file(&intent_path);

        Ok(VaultCommitOutcome {
            receipt: vault_receipt(
                format!("note:{normalized}"),
                vault_generation,
                mutation.content.len(),
                VaultIndexOwner::durability(),
            ),
            note_version,
            vault_generation,
            index_repair_required: false,
        })
    })();

    owner.lanes.mark_pending_intent(&lane, false);
    result
}

/// Recover an interrupted intent→publish→receipt sequence.
pub fn recover_pending_write(
    owner: &Arc<VaultIndexOwner>,
    operation_id: &str,
) -> Result<Option<VaultCommitOutcome>, VaultMutationError> {
    let intent_path = intent_store_path(operation_id)?;
    let receipt_path = receipt_store_path(operation_id)?;
    let tx = owner.transaction();
    let has_receipt = tx.root().is_file(&receipt_path).unwrap_or(false);
    let has_intent = tx.root().is_file(&intent_path).unwrap_or(false);

    if has_receipt {
        let bytes = tx
            .root()
            .read_limited(&receipt_path, 64 * 1024)
            .map_err(VaultMutationError::from)?;
        let record: VaultMutationReceiptRecord = serde_json::from_slice(&bytes)
            .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        let _ = tx.root().remove_file(&intent_path);
        return Ok(Some(VaultCommitOutcome {
            receipt: vault_receipt(
                format!("note:{}", record.path),
                record.vault_generation,
                record.bytes,
                DurabilityLevel::Synced,
            ),
            note_version: NoteVersion::from_digest(record.note_version),
            vault_generation: record.vault_generation,
            index_repair_required: false,
        }));
    }

    if !has_intent {
        return Ok(None);
    }

    let intent_bytes = tx
        .root()
        .read_limited(&intent_path, 64 * 1024)
        .map_err(VaultMutationError::from)?;
    let intent: VaultMutationIntent = serde_json::from_slice(&intent_bytes)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let note_path = VaultPath::parse(&intent.path)
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    let published = tx.root().is_file(&note_path).unwrap_or(false);
    if published {
        let content = String::from_utf8(tx.root().read_limited(&note_path, 8 * 1024 * 1024)?)
            .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
        if content_digest(&content) == intent.content_digest {
            // Content published; complete receipt without retrying the write.
            let vault_generation = owner.bump_generation();
            let note_version = NoteVersion::from_digest(content_hash(&content));
            let receipt_record = VaultMutationReceiptRecord {
                operation_id: intent.operation_id.clone(),
                root_id: intent.root_id,
                path: intent.path.clone(),
                note_version: note_version.as_str().to_string(),
                vault_generation,
                bytes: content.len(),
            };
            let receipt_bytes = serde_json::to_vec(&receipt_record)
                .map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
            tx.write_receipt(&receipt_path, &receipt_bytes, DurabilityLevel::Synced)?;
            let _ = tx.root().remove_file(&intent_path);
            return Ok(Some(VaultCommitOutcome {
                receipt: vault_receipt(
                    format!("note:{}", intent.path),
                    vault_generation,
                    content.len(),
                    DurabilityLevel::Synced,
                ),
                note_version,
                vault_generation,
                index_repair_required: true,
            }));
        }
    }

    // Incomplete publication: drop intent; do not invent a retry mutation.
    let _ = tx.root().remove_file(&intent_path);
    Ok(None)
}

fn check_precondition(
    mutation: &WriteMutation,
    existing: Option<&ExistingNote>,
) -> Result<(), VaultMutationError> {
    match mutation.precondition {
        MutationPrecondition::CreateOnly => {
            if existing.is_some() {
                return Err(VaultMutationError::Conflict(
                    "create_only destination already exists".into(),
                ));
            }
        }
        MutationPrecondition::Match => {
            let Some(existing) = existing else {
                return Err(VaultMutationError::StaleVersion { current: None });
            };
            let expected = mutation.expected_version.as_ref().ok_or_else(|| {
                VaultMutationError::Invalid("Match precondition requires expected version".into())
            })?;
            if &existing.version != expected {
                return Err(VaultMutationError::StaleVersion {
                    current: Some(existing.version.clone()),
                });
            }
        }
        MutationPrecondition::AbsentOrMatch => {
            if let Some(existing) = existing {
                let expected = mutation.expected_version.as_ref().ok_or_else(|| {
                    VaultMutationError::Invalid(
                        "AbsentOrMatch requires expected version when present".into(),
                    )
                })?;
                if &existing.version != expected {
                    return Err(VaultMutationError::StaleVersion {
                        current: Some(existing.version.clone()),
                    });
                }
            }
        }
        MutationPrecondition::Unconditional => {}
    }
    Ok(())
}

struct ExistingNote {
    version: NoteVersion,
}

fn read_existing(
    owner: &VaultIndexOwner,
    path: &VaultPath,
) -> Result<Option<ExistingNote>, VaultMutationError> {
    if !owner.files.is_file(path).unwrap_or(false) {
        return Ok(None);
    }
    let bytes = owner
        .files
        .read_limited(path, 8 * 1024 * 1024)
        .map_err(VaultMutationError::from)?;
    let body =
        String::from_utf8(bytes).map_err(|error| VaultMutationError::Invalid(error.to_string()))?;
    Ok(Some(ExistingNote {
        version: NoteVersion::from_digest(content_hash(&body)),
    }))
}

fn content_digest(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    format!("{digest:x}")
}

fn intent_store_path(operation_id: &str) -> Result<StorePath, VaultMutationError> {
    StorePath::parse(&format!(".medousa/vault/intents/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))
}

fn receipt_store_path(operation_id: &str) -> Result<StorePath, VaultMutationError> {
    StorePath::parse(&format!(".medousa/vault/receipts/{operation_id}.json"))
        .map_err(|error| VaultMutationError::Invalid(error.to_string()))
}

pub fn debug_intent_marker(path: &str) -> String {
    json!({ "path": path }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::contracts::VaultRootId;
    use crate::vault::owner::VaultIndexOwner;
    use medousa_store::{
        FileTransaction, NoTransactionFaults, PersistenceError, PersistenceErrorKind, StoreRoot,
        TransactionFaultPoint, TransactionFaults,
    };
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use tempfile::tempdir;

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

    fn owner() -> (tempfile::TempDir, Arc<VaultIndexOwner>) {
        let dir = tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        let owner = VaultIndexOwner::new(VaultRootId::new("test"), root);
        (dir, owner)
    }

    #[test]
    fn cm006_exactly_one_match_writer_commits() {
        let (_dir, owner) = owner();
        commit_write(
            &owner,
            WriteMutation {
                path: "note.md".into(),
                content: "v1\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap();
        let version = NoteVersion::from_digest(content_hash("v1\n"));
        let barrier = Arc::new(Barrier::new(2));
        let successes = Arc::new(AtomicUsize::new(0));
        let stale = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for idx in 0..2 {
            let owner = Arc::clone(&owner);
            let barrier = Arc::clone(&barrier);
            let successes = Arc::clone(&successes);
            let stale = Arc::clone(&stale);
            let version = version.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                let result = commit_write(
                    &owner,
                    WriteMutation {
                        path: "note.md".into(),
                        content: format!("writer-{idx}\n"),
                        precondition: MutationPrecondition::Match,
                        expected_version: Some(version),
                    },
                );
                match result {
                    Ok(_) => successes.fetch_add(1, Ordering::SeqCst),
                    Err(VaultMutationError::StaleVersion { .. }) => {
                        stale.fetch_add(1, Ordering::SeqCst)
                    }
                    Err(VaultMutationError::Persistence(_)) => 0,
                    Err(_) => 0,
                };
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(successes.load(Ordering::SeqCst), 1);
        assert_eq!(stale.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn crash_between_publish_and_receipt_recovers_without_retry_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        let owner = VaultIndexOwner::new(VaultRootId::new("test"), Arc::clone(&root));
        let faults = Arc::new(FailAt {
            target: TransactionFaultPoint::BeforeReceipt,
            hits: AtomicUsize::new(0),
        });
        owner.set_transaction(FileTransaction::with_faults(
            Arc::clone(&root),
            faults as Arc<dyn TransactionFaults>,
        ));
        let err = commit_write(
            &owner,
            WriteMutation {
                path: "recover.md".into(),
                content: "published\n".into(),
                precondition: MutationPrecondition::CreateOnly,
                expected_version: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, VaultMutationError::Persistence(_)));
        assert!(
            root.is_file(&StorePath::parse("recover.md").unwrap())
                .unwrap()
        );
        // Reset faults and recover from leftover intent.
        owner.set_transaction(FileTransaction::with_faults(
            Arc::clone(&root),
            Arc::new(NoTransactionFaults),
        ));
        let intent_dir = path.join(".medousa/vault/intents");
        let operation_id = std::fs::read_dir(&intent_dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name()
            .to_string_lossy()
            .trim_end_matches(".json")
            .to_string();
        let recovered = recover_pending_write(&owner, &operation_id)
            .unwrap()
            .expect("recovery");
        assert_eq!(recovered.note_version.as_str(), content_hash("published\n"));
        assert!(recovered.index_repair_required);
    }
}
