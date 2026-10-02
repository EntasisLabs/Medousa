//! Stable user-vault identities. Paths and filesystem identities are evidence,
//! not semantic IDs. Native journals retain the bindings until projection sync.

use std::{cell::Cell, collections::BTreeMap, fs::File, sync::atomic::Ordering};

use fs2::FileExt;
use medousa_store::{FileTransaction, StoreEntryKind, StoreMetadata, StorePath};
use medousa_types::work_unit::ResourceResolution;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    contracts::{VaultFileIdentity, VaultIdentityBinding, VaultMutationError},
    owner::VaultIndexOwner,
    path::VaultPath,
};

mod reconciliation;
pub(crate) use reconciliation::{
    ReconciliationDisposition, VaultReconciliationKey, VaultReconciliationReceipt,
};

pub const MAX_IDENTITY_BYTES: usize = 1024 * 1024;
pub const MAX_OBSERVATION_NOTE_BYTES: u64 = 1024 * 1024;
const MAX_IDENTITIES: usize = 4096;
// Covers an escaped 1 KiB locator plus witnesses and observation metadata.
const NATIVE_PROJECTION_HEADROOM: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VaultResourceKind {
    Note,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultResourceIdentity {
    pub resource_id: String,
    pub kind: VaultResourceKind,
    pub path: String,
    pub file: VaultFileIdentity,
    pub resolution: ResourceResolution,
    pub observation: Option<String>,
    pub revision: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentitySnapshot {
    schema_version: u16,
    vault_id: String,
    root_file: VaultFileIdentity,
    revision: u64,
    resources: BTreeMap<String, VaultResourceIdentity>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    reconciliations: BTreeMap<String, VaultReconciliationReceipt>,
}

/// Held across native publication and metadata repair. Cross-process admission
/// is nonblocking, and every access stays beneath the already-held root.
pub(crate) struct VaultIdentityTransaction<'a> {
    owner: &'a VaultIndexOwner,
    _lock: File,
    snapshot: IdentitySnapshot,
    publication_failed: Cell<bool>,
}

fn store_path(name: &str) -> Result<StorePath, VaultMutationError> {
    StorePath::parse(&format!(".medousa/vault/resource-identities.{name}"))
        .map_err(|e| VaultMutationError::Invalid(e.to_string()))
}

fn nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
fn valid_nonce(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn fingerprint(
    owner: &VaultIndexOwner,
    path: &VaultPath,
    kind: VaultResourceKind,
) -> Result<StoreMetadata, VaultMutationError> {
    let metadata = owner.files.metadata(path)?;
    if !matches!(
        (kind, metadata.kind),
        (VaultResourceKind::Note, StoreEntryKind::File)
            | (VaultResourceKind::Folder, StoreEntryKind::Directory)
    ) {
        return Err(VaultMutationError::Invalid(
            "vault resource kind does not match native metadata".into(),
        ));
    }
    Ok(metadata)
}
fn observation(metadata: StoreMetadata, hash: Option<&str>) -> Result<String, VaultMutationError> {
    let bytes = serde_json::to_vec(&(
        VaultFileIdentity::from(metadata),
        metadata.size,
        metadata.modified,
        hash,
    ))
    .map_err(|e| VaultMutationError::Invalid(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

impl<'a> VaultIdentityTransaction<'a> {
    pub(crate) fn begin(
        owner: &'a VaultIndexOwner,
        recovery: bool,
    ) -> Result<Self, VaultMutationError> {
        if !owner.active.load(Ordering::Acquire) {
            return Err(VaultMutationError::Invalid("vault owner is retired".into()));
        }
        let lock = owner.files.open_lock_file(&store_path("lock")?)?;
        lock.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                VaultMutationError::Overloaded
            } else {
                VaultMutationError::Persistence(e.to_string())
            }
        })?;
        let root_file = VaultFileIdentity::from(owner.files.root_metadata()?);
        let (snapshot, fresh) = match owner
            .files
            .read_limited(&store_path("json")?, MAX_IDENTITY_BYTES as u64)
        {
            Ok(bytes) => (
                serde_json::from_slice::<IdentitySnapshot>(&bytes).map_err(|e| {
                    VaultMutationError::Persistence(format!("invalid vault identity registry: {e}"))
                })?,
                false,
            ),
            Err(e) if e.is_not_found() => (
                IdentitySnapshot {
                    schema_version: 1,
                    vault_id: nonce(),
                    root_file: root_file.clone(),
                    revision: 0,
                    resources: BTreeMap::new(),
                    reconciliations: BTreeMap::new(),
                },
                true,
            ),
            Err(e) => return Err(e.into()),
        };
        if snapshot.schema_version != 1
            || !valid_nonce(&snapshot.vault_id)
            || snapshot.root_file != root_file
        {
            return Err(VaultMutationError::ExternallyAmbiguous(
                "vault identity registry belongs to an unsupported or different physical root"
                    .into(),
            ));
        }
        if snapshot.resources.len() > MAX_IDENTITIES {
            return Err(VaultMutationError::Overloaded);
        }
        for (id, record) in &snapshot.resources {
            if id != &record.resource_id
                || !valid_nonce(id)
                || record.revision == 0
                || record.revision > snapshot.revision
                || record.resolution == ResourceResolution::Unresolved
            {
                return Err(VaultMutationError::Persistence(
                    "invalid vault identity record".into(),
                ));
            }
            VaultPath::parse(&record.path)?;
        }
        reconciliation::validate_receipts(&snapshot)?;
        let transaction = Self {
            owner,
            _lock: lock,
            snapshot,
            publication_failed: Cell::new(false),
        };
        if fresh {
            transaction.persist()?;
        }
        transaction.finish_recorded_quarantines()?;
        if !recovery && !reconciliation::pending_ids(owner)?.is_empty() {
            return Err(VaultMutationError::ExternallyAmbiguous(
                "native journal repair is pending; identity cannot be resolved or reassigned"
                    .into(),
            ));
        }
        Ok(transaction)
    }

    pub(crate) fn pending_journal_ids(
        owner: &VaultIndexOwner,
    ) -> Result<Vec<String>, VaultMutationError> {
        reconciliation::pending_ids(owner)
    }

    pub(crate) fn vault_id(&self) -> &str {
        &self.snapshot.vault_id
    }

    pub(crate) fn current_revision(&self) -> u64 {
        self.snapshot.revision
    }

    pub(crate) fn retained_resource(
        &self,
        id: &str,
    ) -> Result<VaultResourceIdentity, VaultMutationError> {
        self.ensure_usable()?;
        self.snapshot
            .resources
            .get(id)
            .cloned()
            .ok_or_else(|| VaultMutationError::Invalid("unknown vault resource identity".into()))
    }

    pub(crate) fn resource_kind(&self, id: &str) -> Result<VaultResourceKind, VaultMutationError> {
        Ok(self.retained_resource(id)?.kind)
    }

    fn ensure_usable(&self) -> Result<(), VaultMutationError> {
        if self.publication_failed.get() {
            return Err(VaultMutationError::Persistence(
                "identity transaction must be reopened after a failed metadata publication".into(),
            ));
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), VaultMutationError> {
        self.ensure_usable()?;
        let result = self.persist_snapshot();
        if result.is_err() {
            self.publication_failed.set(true);
        }
        result
    }

    fn persist_snapshot(&self) -> Result<(), VaultMutationError> {
        if self.owner.identity_persist_fault.load(Ordering::Acquire) {
            return Err(VaultMutationError::Persistence(
                "injected vault identity projection failure".into(),
            ));
        }
        if serde_json::to_vec(&self.snapshot)
            .map_err(|e| VaultMutationError::Persistence(e.to_string()))?
            .len()
            > MAX_IDENTITY_BYTES
        {
            return Err(VaultMutationError::Overloaded);
        }
        FileTransaction::new(self.owner.files.clone())
            .replace_snapshot_json(&store_path("json")?, &self.snapshot)?;
        Ok(())
    }

    fn admit_native_projection(&self, new_identity: bool) -> Result<(), VaultMutationError> {
        self.ensure_usable()?;
        if (new_identity && self.snapshot.resources.len() >= MAX_IDENTITIES)
            || serde_json::to_vec(&self.snapshot)
                .map_err(|e| VaultMutationError::Persistence(e.to_string()))?
                .len()
                > MAX_IDENTITY_BYTES - NATIVE_PROJECTION_HEADROOM
        {
            return Err(VaultMutationError::Overloaded);
        }
        Ok(())
    }

    fn put(
        &mut self,
        record: VaultResourceIdentity,
    ) -> Result<VaultResourceIdentity, VaultMutationError> {
        let (record, changed) = self.update_record(record)?;
        if changed {
            self.persist()?;
        } else {
            // Finish a prior publication's interrupted parent fence on replay.
            self.owner.files.sync_parent_of(&store_path("json")?)?;
        }
        Ok(record)
    }

    fn update_record(
        &mut self,
        mut record: VaultResourceIdentity,
    ) -> Result<(VaultResourceIdentity, bool), VaultMutationError> {
        self.ensure_usable()?;
        if let Some(previous) = self.snapshot.resources.get(&record.resource_id) {
            if previous.kind != record.kind
                || (previous.resolution == ResourceResolution::Tombstoned
                    && record.resolution != ResourceResolution::Tombstoned)
            {
                return Err(VaultMutationError::Conflict(
                    "vault identity cannot change kind or reuse a tombstone".into(),
                ));
            }
            if previous.path == record.path
                && previous.file == record.file
                && previous.resolution == record.resolution
                && previous.observation == record.observation
            {
                return Ok((previous.clone(), false));
            }
        } else if self.snapshot.resources.len() >= MAX_IDENTITIES {
            return Err(VaultMutationError::Overloaded);
        }
        self.snapshot.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .ok_or(VaultMutationError::Overloaded)?;
        record.revision = self.snapshot.revision;
        self.snapshot
            .resources
            .insert(record.resource_id.clone(), record.clone());
        Ok((record, true))
    }

    fn bind_existing(
        &mut self,
        path: &VaultPath,
        kind: VaultResourceKind,
        metadata: StoreMetadata,
        hash: Option<&str>,
        resolution: ResourceResolution,
    ) -> Result<VaultResourceIdentity, VaultMutationError> {
        let file = VaultFileIdentity::from(metadata);
        // A fingerprint at another path is evidence of an external move, not
        // authority to merge identities or silently move semantic links.
        if self.snapshot.resources.values().any(|r| {
            r.kind == kind
                && r.file == file
                && r.path != path.as_str()
                && r.resolution != ResourceResolution::Tombstoned
        }) {
            return Err(VaultMutationError::ExternallyAmbiguous("physical resource was observed at another locator; managed reconciliation is required".into()));
        }
        let previous = self
            .snapshot
            .resources
            .values()
            .find(|r| {
                r.kind == kind
                    && r.path == path.as_str()
                    && r.file == file
                    && r.resolution != ResourceResolution::Tombstoned
            })
            .cloned();
        // Retire availability of prior objects at this locator before issuing a
        // new ID. External replacement is never an identity-preserving write.
        let replaced: Vec<_> = self
            .snapshot
            .resources
            .values()
            .filter(|r| {
                r.path == path.as_str()
                    && r.resolution == ResourceResolution::Available
                    && (r.file != file || r.kind != kind)
            })
            .cloned()
            .collect();
        for mut record in replaced {
            record.resolution = ResourceResolution::Unavailable;
            self.put(record)?;
        }
        self.put(VaultResourceIdentity {
            resource_id: previous.map_or_else(nonce, |r| r.resource_id),
            kind,
            path: path.to_string(),
            file,
            resolution,
            observation: Some(observation(metadata, hash)?),
            revision: 0,
        })
    }

    pub(crate) fn observe_path(
        &mut self,
        path: &VaultPath,
        kind: VaultResourceKind,
    ) -> Result<VaultResourceIdentity, VaultMutationError> {
        let before = fingerprint(self.owner, path, kind)?;
        if kind == VaultResourceKind::Note && before.size > MAX_OBSERVATION_NOTE_BYTES {
            return self.bind_existing(path, kind, before, None, ResourceResolution::Unavailable);
        }
        let hash = if kind == VaultResourceKind::Note {
            Some(format!(
                "sha256:{:x}",
                Sha256::digest(
                    self.owner
                        .files
                        .read_limited(path, MAX_OBSERVATION_NOTE_BYTES)?
                )
            ))
        } else {
            None
        };
        let after = fingerprint(self.owner, path, kind)?;
        if VaultFileIdentity::from(before) != VaultFileIdentity::from(after)
            || before.size != after.size
            || before.modified != after.modified
        {
            return Err(VaultMutationError::ExternallyAmbiguous(
                "native resource changed during observation".into(),
            ));
        }
        self.bind_existing(
            path,
            kind,
            after,
            hash.as_deref(),
            ResourceResolution::Available,
        )
    }

    pub(crate) fn observe_id(
        &mut self,
        id: &str,
    ) -> Result<VaultResourceIdentity, VaultMutationError> {
        self.ensure_usable()?;
        let mut record =
            self.snapshot.resources.get(id).cloned().ok_or_else(|| {
                VaultMutationError::Invalid("unknown vault resource identity".into())
            })?;
        if record.resolution == ResourceResolution::Tombstoned {
            return Ok(record);
        }
        let path = VaultPath::parse(&record.path)?;
        match self.owner.files.metadata(&path) {
            Ok(metadata) if VaultFileIdentity::from(metadata) == record.file => {
                self.observe_path(&path, record.kind)
            }
            Ok(_) => {
                record.resolution = ResourceResolution::Unavailable;
                self.put(record)
            }
            Err(e) if e.is_not_found() => {
                record.resolution = ResourceResolution::Unavailable;
                self.put(record)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) fn prepare_write(
        &mut self,
        path: &VaultPath,
    ) -> Result<VaultIdentityBinding, VaultMutationError> {
        self.admit_native_projection(false)?;
        let (resource_id, prior_file) = match self.owner.files.metadata(path) {
            Ok(_) => {
                let metadata = fingerprint(self.owner, path, VaultResourceKind::Note)?;
                let file = VaultFileIdentity::from(metadata);
                let existing = self
                    .snapshot
                    .resources
                    .values()
                    .find(|r| {
                        r.path == path.as_str()
                            && r.kind == VaultResourceKind::Note
                            && r.file == file
                            && r.resolution != ResourceResolution::Tombstoned
                    })
                    .cloned();
                let record = match existing {
                    Some(record) => record,
                    None => self.bind_existing(
                        path,
                        VaultResourceKind::Note,
                        metadata,
                        None,
                        ResourceResolution::Available,
                    )?,
                };
                (record.resource_id, Some(record.file))
            }
            Err(e) if e.is_not_found() => {
                self.admit_native_projection(true)?;
                (nonce(), None)
            }
            Err(e) => return Err(e.into()),
        };
        Ok(VaultIdentityBinding {
            vault_id: self.snapshot.vault_id.clone(),
            resource_id,
            prior_file,
            published_file: None,
        })
    }

    pub(crate) fn prepare_relocate(
        &mut self,
        source: &VaultPath,
        restore: bool,
    ) -> Result<VaultIdentityBinding, VaultMutationError> {
        if restore {
            self.admit_native_projection(true)?;
            return Ok(VaultIdentityBinding {
                vault_id: self.snapshot.vault_id.clone(),
                resource_id: nonce(),
                prior_file: Some(fingerprint(self.owner, source, VaultResourceKind::Note)?.into()),
                published_file: None,
            });
        }
        self.prepare_write(source)
    }

    /// A native publication witness, frozen in the receipt/intent, permits
    /// repair after deletion or external replacement without rebinding the ID.
    pub(crate) fn complete(
        &mut self,
        binding: &VaultIdentityBinding,
        path: &VaultPath,
        deleted: bool,
        hash: Option<&str>,
    ) -> Result<VaultResourceIdentity, VaultMutationError> {
        if binding.vault_id != self.snapshot.vault_id || !valid_nonce(&binding.resource_id) {
            return Err(VaultMutationError::Conflict(
                "journal identity binding does not belong to this root".into(),
            ));
        }
        let file = binding.published_file.as_ref().ok_or_else(|| VaultMutationError::ExternallyAmbiguous(
            "native publication has no durable file witness; identity repair requires reconciliation".into()))?;
        let mut resolution = if deleted {
            ResourceResolution::Tombstoned
        } else {
            ResourceResolution::Unavailable
        };
        let mut observed = None;
        if !deleted {
            match self.owner.files.metadata(path) {
                Ok(metadata) if &VaultFileIdentity::from(metadata) == file => {
                    if metadata.kind != StoreEntryKind::File {
                        return Err(VaultMutationError::Invalid(
                            "published note is not a file".into(),
                        ));
                    }
                    resolution = ResourceResolution::Available;
                    observed = Some(observation(metadata, hash)?);
                }
                Ok(_) => {}
                Err(e) if e.is_not_found() => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.put(VaultResourceIdentity {
            resource_id: binding.resource_id.clone(),
            kind: VaultResourceKind::Note,
            path: path.to_string(),
            file: file.clone(),
            resolution,
            observation: observed,
            revision: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{
        contracts::{MutationPrecondition, VaultRootId},
        mutation::{WriteMutation, commit_write, recover_all_pending_writes},
        relocate::{relocate_delete, relocate_move, relocate_restore},
    };
    use medousa_store::{
        PersistenceError, PersistenceErrorKind, StoreRoot, TransactionFaultPoint, TransactionFaults,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    pub(super) fn fixture() -> (tempfile::TempDir, Arc<VaultIndexOwner>) {
        let dir = tempfile::tempdir().unwrap();
        let files = Arc::new(
            StoreRoot::open_or_create_nofollow(&dir.path().canonicalize().unwrap()).unwrap(),
        );
        let owner = VaultIndexOwner::new(VaultRootId::new("test"), files);
        (dir, owner)
    }
    pub(super) fn write(owner: &Arc<VaultIndexOwner>, path: &str, body: &str) {
        commit_write(
            owner,
            WriteMutation {
                path: path.into(),
                content: body.into(),
                precondition: MutationPrecondition::Unconditional,
                expected_version: None,
            },
        )
        .unwrap();
    }
    pub(super) fn note(owner: &VaultIndexOwner, path: &str) -> VaultResourceIdentity {
        VaultIdentityTransaction::begin(owner, false)
            .unwrap()
            .observe_path(&VaultPath::parse(path).unwrap(), VaultResourceKind::Note)
            .unwrap()
    }
    pub(super) fn by_id(owner: &VaultIndexOwner, id: &str) -> VaultResourceIdentity {
        VaultIdentityTransaction::begin(owner, false)
            .unwrap()
            .observe_id(id)
            .unwrap()
    }
    struct FailOnce {
        point: TransactionFaultPoint,
        fired: AtomicBool,
    }
    impl TransactionFaults for FailOnce {
        fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
            if point == self.point && !self.fired.swap(true, Ordering::SeqCst) {
                return Err(PersistenceError::new(
                    PersistenceErrorKind::RetryableIo,
                    "identity fixture native publication failure",
                ));
            }
            Ok(())
        }
    }

    #[test]
    fn managed_write_move_delete_restore_and_path_reuse_survive_restart() {
        let (_dir, owner) = fixture();
        write(&owner, "original.md", "# First\n");
        let first = note(&owner, "original.md");
        write(&owner, "original.md", "# Updated\n");
        let updated = note(&owner, "original.md");
        assert_eq!(first.resource_id, updated.resource_id);
        assert_ne!(first.file, updated.file); // Atomic replacement is journal-backed.
        relocate_move(&owner, "original.md", "renamed.md").unwrap();
        let restarted =
            VaultIndexOwner::new(VaultRootId::new("different-locator"), owner.files.clone());
        let moved = by_id(&restarted, &first.resource_id);
        assert_eq!(moved.path, "renamed.md");
        assert_eq!(moved.resolution, ResourceResolution::Available);
        assert_eq!(
            note(&restarted, "renamed.md").resource_id,
            first.resource_id
        );
        relocate_delete(&restarted, "renamed.md").unwrap();
        let deleted = by_id(&restarted, &first.resource_id);
        assert_eq!(deleted.resolution, ResourceResolution::Tombstoned);
        relocate_restore(&restarted, "renamed.md").unwrap();
        let restored = note(&restarted, "renamed.md");
        assert_ne!(restored.resource_id, first.resource_id);
        assert_eq!(
            by_id(&restarted, &first.resource_id).resolution,
            ResourceResolution::Tombstoned
        );
        relocate_delete(&restarted, "renamed.md").unwrap();
        write(&restarted, "renamed.md", "# Updated\n");
        assert_ne!(
            note(&restarted, "renamed.md").resource_id,
            restored.resource_id
        );
    }

    #[test]
    fn replacement_after_publication_cannot_inherit_the_publishers_identity() {
        struct ReplacePublished {
            root: std::path::PathBuf,
            point: TransactionFaultPoint,
            fired: AtomicBool,
        }
        impl TransactionFaults for ReplacePublished {
            fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
                let path = self.root.join("note.md");
                if point == self.point
                    && std::fs::read(&path).ok().as_deref() == Some(b"new")
                    && !self.fired.swap(true, Ordering::SeqCst)
                {
                    std::fs::rename(&path, self.root.join("externally-moved.md")).unwrap();
                    std::fs::write(&path, b"new").unwrap();
                }
                Ok(())
            }
        }
        for create_only in [false, true] {
            let (dir, owner) = fixture();
            let prior = if create_only {
                None
            } else {
                write(&owner, "note.md", "old");
                Some(note(&owner, "note.md").resource_id)
            };
            let faults = Arc::new(ReplacePublished {
                root: dir.path().to_path_buf(),
                point: if create_only {
                    TransactionFaultPoint::AfterCreateOnly
                } else {
                    TransactionFaultPoint::AfterSnapshotPublish
                },
                fired: AtomicBool::new(false),
            });
            owner.set_transaction(FileTransaction::with_faults(
                owner.files.clone(),
                faults.clone(),
            ));
            let outcome = commit_write(
                &owner,
                WriteMutation {
                    path: "note.md".into(),
                    content: "new".into(),
                    precondition: if create_only {
                        MutationPrecondition::CreateOnly
                    } else {
                        MutationPrecondition::Unconditional
                    },
                    expected_version: None,
                },
            )
            .unwrap();
            assert!(!outcome.index_repair_required);
            assert!(faults.fired.load(Ordering::SeqCst));
            let published = VaultIdentityTransaction::begin(&owner, false)
                .unwrap()
                .snapshot
                .resources
                .values()
                .find(|r| r.path == "note.md")
                .unwrap()
                .clone();
            if let Some(prior) = prior {
                assert_eq!(published.resource_id, prior);
            }
            assert_eq!(published.resolution, ResourceResolution::Unavailable);
            let replacement = note(&owner, "note.md");
            assert_ne!(replacement.resource_id, published.resource_id);
            assert_eq!(replacement.resolution, ResourceResolution::Available);
            assert_eq!(
                by_id(&owner, &published.resource_id).resolution,
                ResourceResolution::Unavailable
            );
        }
    }

    #[test]
    fn completed_receipt_replay_cannot_roll_back_a_later_locator() {
        let (_dir, owner) = fixture();
        write(&owner, "first.md", "body");
        let first = note(&owner, "first.md");
        let receipts = StorePath::parse(".medousa/vault/receipts").unwrap();
        let write_id = owner.files.list_directory_utf8(&receipts).unwrap()[0]
            .name
            .trim_end_matches(".json")
            .to_string();
        relocate_move(&owner, "first.md", "second.md").unwrap();
        crate::vault::mutation::recover_pending_write(&owner, &write_id).unwrap();
        assert_eq!(by_id(&owner, &first.resource_id).path, "second.md");
        let move_id = owner
            .files
            .list_directory_utf8(&receipts)
            .unwrap()
            .into_iter()
            .find(|entry| entry.name != format!("{write_id}.json"))
            .unwrap()
            .name
            .trim_end_matches(".json")
            .to_string();
        relocate_move(&owner, "second.md", "third.md").unwrap();
        crate::vault::relocate::recover_pending_relocate(&owner, &move_id).unwrap();
        assert_eq!(by_id(&owner, &first.resource_id).path, "third.md");
    }

    #[test]
    fn external_move_stays_ambiguous_and_equal_content_replacement_gets_new_identity() {
        let (dir, owner) = fixture();
        write(&owner, "note.md", "same bytes");
        let first = note(&owner, "note.md");
        std::fs::rename(dir.path().join("note.md"), dir.path().join("external.md")).unwrap();
        assert_eq!(
            by_id(&owner, &first.resource_id).resolution,
            ResourceResolution::Unavailable
        );
        assert!(matches!(
            VaultIdentityTransaction::begin(&owner, false)
                .unwrap()
                .observe_path(
                    &VaultPath::parse("external.md").unwrap(),
                    VaultResourceKind::Note
                ),
            Err(VaultMutationError::ExternallyAmbiguous(_))
        ));
        // A separate physical file with equal content cannot inherit links.
        std::fs::write(dir.path().join("note.md"), b"same bytes").unwrap();
        let replacement = note(&owner, "note.md");
        assert_ne!(replacement.resource_id, first.resource_id);
        assert_eq!(
            by_id(&owner, &first.resource_id).resolution,
            ResourceResolution::Unavailable
        );
    }

    #[test]
    fn folder_identity_survives_restart_and_external_move_requires_reconciliation() {
        let (dir, owner) = fixture();
        std::fs::create_dir(dir.path().join("notes")).unwrap();
        let first = VaultIdentityTransaction::begin(&owner, false)
            .unwrap()
            .observe_path(
                &VaultPath::parse("notes").unwrap(),
                VaultResourceKind::Folder,
            )
            .unwrap();
        let restarted = VaultIndexOwner::new(VaultRootId::new("test"), owner.files.clone());
        assert_eq!(
            by_id(&restarted, &first.resource_id).resolution,
            ResourceResolution::Available
        );
        std::fs::rename(dir.path().join("notes"), dir.path().join("elsewhere")).unwrap();
        assert_eq!(
            by_id(&restarted, &first.resource_id).resolution,
            ResourceResolution::Unavailable
        );
        assert!(
            VaultIdentityTransaction::begin(&restarted, false)
                .unwrap()
                .observe_path(
                    &VaultPath::parse("elsewhere").unwrap(),
                    VaultResourceKind::Folder
                )
                .is_err()
        );
    }

    #[test]
    fn projection_failure_after_native_commit_keeps_binding_and_receipt_for_recovery() {
        for operation in ["write", "move", "delete", "restore"] {
            let (_dir, owner) = fixture();
            write(&owner, "from.md", "old");
            let first = note(&owner, "from.md");
            if operation == "restore" {
                relocate_delete(&owner, "from.md").unwrap();
            }
            owner.identity_persist_fault.store(true, Ordering::Release);
            let outcome = match operation {
                "write" => commit_write(
                    &owner,
                    WriteMutation {
                        path: "from.md".into(),
                        content: "new".into(),
                        precondition: MutationPrecondition::Unconditional,
                        expected_version: None,
                    },
                ),
                "move" => relocate_move(&owner, "from.md", "to.md"),
                "delete" => relocate_delete(&owner, "from.md"),
                _ => relocate_restore(&owner, "from.md"),
            }
            .expect("native effect remains committed");
            assert!(outcome.index_repair_required);
            assert!(VaultIdentityTransaction::begin(&owner, false).is_err());
            owner.identity_persist_fault.store(false, Ordering::Release);
            let restarted = VaultIndexOwner::new(VaultRootId::new("test"), owner.files.clone());
            recover_all_pending_writes(&restarted).unwrap();
            assert!(recover_all_pending_writes(&restarted).unwrap().is_empty());
            let recovered = by_id(&restarted, &first.resource_id);
            match operation {
                "write" => {
                    assert_eq!(recovered.resolution, ResourceResolution::Available);
                    assert_eq!(note(&restarted, "from.md").resource_id, first.resource_id);
                }
                "move" => {
                    assert_eq!(recovered.path, "to.md");
                    assert_eq!(note(&restarted, "to.md").resource_id, first.resource_id);
                }
                "delete" => assert_eq!(recovered.resolution, ResourceResolution::Tombstoned),
                _ => {
                    assert_eq!(recovered.resolution, ResourceResolution::Tombstoned);
                    assert_ne!(note(&restarted, "from.md").resource_id, first.resource_id);
                }
            }
        }
    }

    #[test]
    fn post_move_fence_failure_preserves_journal_and_exact_file_witness() {
        let (_dir, owner) = fixture();
        write(&owner, "from.md", "body");
        let first = note(&owner, "from.md");
        owner.set_transaction(FileTransaction::with_faults(
            owner.files.clone(),
            Arc::new(FailOnce {
                point: TransactionFaultPoint::AfterRenamePublish,
                fired: AtomicBool::new(false),
            }),
        ));
        assert!(
            relocate_move(&owner, "from.md", "to.md")
                .unwrap()
                .index_repair_required
        );
        assert!(
            !owner
                .files
                .is_file(&VaultPath::parse("from.md").unwrap())
                .unwrap()
        );
        assert!(VaultIdentityTransaction::begin(&owner, false).is_err());
        owner.set_transaction(FileTransaction::new(owner.files.clone()));
        recover_all_pending_writes(&owner).unwrap();
        assert_eq!(note(&owner, "to.md").resource_id, first.resource_id);
    }

    #[test]
    fn missing_write_publication_witness_never_guesses_from_equal_bytes() {
        let (_dir, owner) = fixture();
        owner.set_transaction(FileTransaction::with_faults(
            owner.files.clone(),
            Arc::new(FailOnce {
                point: TransactionFaultPoint::AfterCreateOnly,
                fired: AtomicBool::new(false),
            }),
        ));
        assert!(
            commit_write(
                &owner,
                WriteMutation {
                    path: "note.md".into(),
                    content: "published".into(),
                    precondition: MutationPrecondition::CreateOnly,
                    expected_version: None
                }
            )
            .is_err()
        );
        assert!(
            owner
                .files
                .is_file(&VaultPath::parse("note.md").unwrap())
                .unwrap()
        );
        owner.set_transaction(FileTransaction::new(owner.files.clone()));
        assert!(matches!(
            recover_all_pending_writes(&owner),
            Err(VaultMutationError::ExternallyAmbiguous(_))
        ));
        assert!(VaultIdentityTransaction::begin(&owner, false).is_err());
        assert!(
            !owner
                .files
                .list_directory_utf8(&StorePath::parse(".medousa/vault/intents").unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn copied_registry_and_corruption_fail_without_reissuing_ids_or_touching_content() {
        let (dir, owner) = fixture();
        write(&owner, "note.md", "body");
        let (_other_dir, other) = fixture();
        other
            .files
            .create_dir_all(&StorePath::parse(".medousa/vault").unwrap())
            .unwrap();
        let bytes = owner.files.read(&store_path("json").unwrap()).unwrap();
        other
            .files
            .atomic_write(&store_path("json").unwrap(), &bytes)
            .unwrap();
        assert!(VaultIdentityTransaction::begin(&other, false).is_err());
        std::fs::write(
            dir.path().join(".medousa/vault/resource-identities.json"),
            b"broken",
        )
        .unwrap();
        assert!(VaultIdentityTransaction::begin(&owner, false).is_err());
        assert_eq!(
            owner
                .files
                .read(&VaultPath::parse("note.md").unwrap())
                .unwrap(),
            b"body"
        );
    }
}
