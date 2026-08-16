//! Shared persistence vocabulary and capability-confined file transactions.
//!
//! Domain stores retain ownership of sequencing and mutation semantics. This
//! module only owns honest durability labels and the publication boundaries
//! that must behave identically across feeds, workspace state, task runs,
//! Forge item logs, slug reservations, catalogs, and Coder checkpoints.

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::store_root::{StoreRoot, StoreRootError, StoreRootPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurabilityLevel {
    Accepted,
    Written,
    Synced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    Feed,
    Workspace,
    ProjectTask,
    Forge,
    ForgeSlug,
    ForgeCatalog,
    CoderCheckpoint,
    Vault,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitReceipt {
    pub store: StoreKind,
    pub key: String,
    pub generation: u64,
    pub durability: DurabilityLevel,
    pub bytes: usize,
    pub committed_at: DateTime<Utc>,
}

impl CommitReceipt {
    pub fn new(
        store: StoreKind,
        key: impl Into<String>,
        generation: u64,
        durability: DurabilityLevel,
        bytes: usize,
    ) -> Self {
        Self {
            store,
            key: key.into(),
            generation,
            durability,
            bytes,
            committed_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceErrorKind {
    Conflict,
    Overloaded,
    RetryableIo,
    PermanentIo,
    Serialization,
    Corruption,
    Cancelled,
    ShuttingDown,
}

#[derive(Debug)]
pub struct PersistenceError {
    pub kind: PersistenceErrorKind,
    message: String,
}

impl PersistenceError {
    pub fn new(kind: PersistenceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PersistenceError {}

impl From<StoreRootError> for PersistenceError {
    fn from(error: StoreRootError) -> Self {
        let kind = match &error {
            StoreRootError::Io { source, .. }
                if source.kind() == std::io::ErrorKind::AlreadyExists =>
            {
                PersistenceErrorKind::Conflict
            }
            StoreRootError::Io { source, .. }
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                ) =>
            {
                PersistenceErrorKind::RetryableIo
            }
            StoreRootError::Io { .. } => PersistenceErrorKind::PermanentIo,
            StoreRootError::InvalidPath(_)
            | StoreRootError::Limit { .. }
            | StoreRootError::Confinement { .. } => PersistenceErrorKind::PermanentIo,
        };
        Self::new(kind, error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionFaultPoint {
    BeforeWrite,
    AfterWrite,
    BeforePublish,
    AfterPublish,
    BeforeAppend,
    AfterAppend,
    BeforeSnapshotPublish,
    AfterSnapshotPublish,
    BeforeSlugReservation,
    AfterSlugReservation,
    BeforeCatalogPublish,
    AfterCatalogPublish,
    BeforeGitCompletion,
    AfterGitCompletion,
    BeforeCheckpointDelta,
    AfterCheckpointDelta,
    BeforeObservation,
    AfterObservation,
    /// Durable mutation intent recorded before content becomes visible.
    BeforeIntent,
    AfterIntent,
    BeforeTempWrite,
    AfterTempWrite,
    BeforeFileSync,
    AfterFileSync,
    BeforeRenamePublish,
    AfterRenamePublish,
    BeforeParentSync,
    AfterParentSync,
    BeforeReceipt,
    AfterReceipt,
    BeforeCreateOnly,
    AfterCreateOnly,
    BeforeMove,
    AfterMove,
    Cancellation,
}

pub trait TransactionFaults: Send + Sync {
    fn check(&self, _point: TransactionFaultPoint) -> Result<(), PersistenceError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct NoTransactionFaults;

impl TransactionFaults for NoTransactionFaults {}

#[derive(Clone)]
pub struct FileTransaction {
    root: Arc<StoreRoot>,
    faults: Arc<dyn TransactionFaults>,
}

impl FileTransaction {
    pub fn new(root: Arc<StoreRoot>) -> Self {
        Self {
            root,
            faults: Arc::new(NoTransactionFaults),
        }
    }

    pub fn with_faults(root: Arc<StoreRoot>, faults: Arc<dyn TransactionFaults>) -> Self {
        Self { root, faults }
    }

    pub fn root(&self) -> &StoreRoot {
        &self.root
    }

    pub fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
        self.faults.check(point)
    }

    pub fn append_record(
        &self,
        path: &impl StoreRootPath,
        record: &[u8],
        durability: DurabilityLevel,
    ) -> Result<usize, PersistenceError> {
        self.faults.check(TransactionFaultPoint::BeforeWrite)?;
        self.faults.check(TransactionFaultPoint::BeforeAppend)?;
        let mut framed = Vec::with_capacity(record.len() + 1);
        framed.extend_from_slice(record);
        if !framed.ends_with(b"\n") {
            framed.push(b'\n');
        }
        self.root
            .append_durable(path, &framed, matches!(durability, DurabilityLevel::Synced))?;
        self.faults.check(TransactionFaultPoint::AfterWrite)?;
        self.faults.check(TransactionFaultPoint::AfterAppend)?;
        Ok(framed.len())
    }

    pub fn replace_snapshot(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
        durability: DurabilityLevel,
    ) -> Result<usize, PersistenceError> {
        if durability == DurabilityLevel::Accepted {
            return Err(PersistenceError::new(
                PersistenceErrorKind::PermanentIo,
                "accepted durability cannot publish a file transaction",
            ));
        }
        self.faults.check(TransactionFaultPoint::BeforeWrite)?;
        self.faults.check(TransactionFaultPoint::BeforePublish)?;
        self.faults
            .check(TransactionFaultPoint::BeforeSnapshotPublish)?;
        self.publish_bytes(path, bytes, false, durability)?;
        self.faults.check(TransactionFaultPoint::AfterPublish)?;
        self.faults
            .check(TransactionFaultPoint::AfterSnapshotPublish)?;
        Ok(bytes.len())
    }

    /// Create-only publication: fails if the destination already exists.
    pub fn create_only(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
        durability: DurabilityLevel,
    ) -> Result<usize, PersistenceError> {
        if durability == DurabilityLevel::Accepted {
            return Err(PersistenceError::new(
                PersistenceErrorKind::PermanentIo,
                "accepted durability cannot publish a file transaction",
            ));
        }
        self.faults.check(TransactionFaultPoint::BeforeWrite)?;
        self.faults.check(TransactionFaultPoint::BeforeCreateOnly)?;
        self.faults.check(TransactionFaultPoint::BeforePublish)?;
        if self.root.is_file(path).unwrap_or(false) {
            return Err(PersistenceError::new(
                PersistenceErrorKind::Conflict,
                "create_only destination already exists",
            ));
        }
        self.publish_bytes(path, bytes, true, durability)?;
        self.faults.check(TransactionFaultPoint::AfterPublish)?;
        self.faults.check(TransactionFaultPoint::AfterCreateOnly)?;
        Ok(bytes.len())
    }

    /// Same-filesystem rename with parent-directory sync fences.
    pub fn move_path(
        &self,
        from: &impl StoreRootPath,
        to: &impl StoreRootPath,
        durability: DurabilityLevel,
    ) -> Result<(), PersistenceError> {
        self.move_path_inner(from, to, durability, false)
    }

    /// Same-filesystem rename that fails if the destination already exists.
    pub fn move_path_create_only(
        &self,
        from: &impl StoreRootPath,
        to: &impl StoreRootPath,
        durability: DurabilityLevel,
    ) -> Result<(), PersistenceError> {
        self.move_path_inner(from, to, durability, true)
    }

    fn move_path_inner(
        &self,
        from: &impl StoreRootPath,
        to: &impl StoreRootPath,
        durability: DurabilityLevel,
        create_only: bool,
    ) -> Result<(), PersistenceError> {
        self.faults.check(TransactionFaultPoint::BeforeMove)?;
        self.faults
            .check(TransactionFaultPoint::BeforeRenamePublish)?;
        if create_only {
            self.root.rename_create_only(from, to)?;
        } else {
            self.root.rename(from, to)?;
        }
        if matches!(durability, DurabilityLevel::Synced) {
            self.faults.check(TransactionFaultPoint::BeforeParentSync)?;
            self.root.sync_parent_of(to)?;
            self.root.sync_parent_of(from).ok();
            self.faults.check(TransactionFaultPoint::AfterParentSync)?;
        }
        self.faults
            .check(TransactionFaultPoint::AfterRenamePublish)?;
        self.faults.check(TransactionFaultPoint::AfterMove)?;
        Ok(())
    }

    /// Restore helper: create-only rename from trash/recovery into destination.
    pub fn restore_path(
        &self,
        from: &impl StoreRootPath,
        to: &impl StoreRootPath,
        durability: DurabilityLevel,
    ) -> Result<(), PersistenceError> {
        self.move_path_create_only(from, to, durability)
    }

    /// Durable intent record used before content publication (H07 protocol).
    pub fn write_intent(
        &self,
        path: &impl StoreRootPath,
        record: &[u8],
        durability: DurabilityLevel,
    ) -> Result<usize, PersistenceError> {
        self.faults.check(TransactionFaultPoint::BeforeIntent)?;
        let written = self.replace_snapshot(path, record, durability)?;
        self.faults.check(TransactionFaultPoint::AfterIntent)?;
        Ok(written)
    }

    /// Committed receipt record after content publication succeeds.
    pub fn write_receipt(
        &self,
        path: &impl StoreRootPath,
        record: &[u8],
        durability: DurabilityLevel,
    ) -> Result<usize, PersistenceError> {
        self.faults.check(TransactionFaultPoint::BeforeReceipt)?;
        let written = self.replace_snapshot(path, record, durability)?;
        self.faults.check(TransactionFaultPoint::AfterReceipt)?;
        Ok(written)
    }

    fn publish_bytes(
        &self,
        path: &impl StoreRootPath,
        bytes: &[u8],
        create_only: bool,
        durability: DurabilityLevel,
    ) -> Result<(), PersistenceError> {
        self.faults.check(TransactionFaultPoint::BeforeTempWrite)?;
        self.faults.check(TransactionFaultPoint::BeforeFileSync)?;
        if create_only {
            self.root.atomic_create(path, bytes)?;
        } else {
            self.root.atomic_write(path, bytes)?;
        }
        self.faults.check(TransactionFaultPoint::AfterTempWrite)?;
        self.faults.check(TransactionFaultPoint::AfterFileSync)?;
        if matches!(durability, DurabilityLevel::Synced) {
            self.faults.check(TransactionFaultPoint::BeforeParentSync)?;
            self.root.sync_parent_of(path)?;
            self.faults.check(TransactionFaultPoint::AfterParentSync)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_root::StorePath;
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
                    format!("injected failure at {point:?}"),
                ));
            }
            Ok(())
        }
    }

    fn transaction() -> (tempfile::TempDir, FileTransaction) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        (directory, FileTransaction::new(root))
    }

    #[test]
    fn append_frames_records_and_crosses_requested_fence() {
        let (_directory, transaction) = transaction();
        let path = StorePath::parse("events.jsonl").unwrap();
        assert_eq!(
            transaction
                .append_record(&path, br#"{"generation":1}"#, DurabilityLevel::Synced)
                .unwrap(),
            17
        );
        let raw = transaction.root().read_limited(&path, 1024).unwrap();
        assert_eq!(raw, b"{\"generation\":1}\n");
    }

    #[test]
    fn failed_publication_never_replaces_the_authoritative_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        let path = StorePath::parse("snapshot.json").unwrap();
        root.atomic_write(&path, b"old").unwrap();
        let faults = Arc::new(FailAt {
            target: TransactionFaultPoint::BeforePublish,
            hits: AtomicUsize::new(0),
        });
        let transaction = FileTransaction::with_faults(Arc::clone(&root), faults);

        let error = transaction
            .replace_snapshot(&path, b"new", DurabilityLevel::Synced)
            .unwrap_err();
        assert_eq!(error.kind, PersistenceErrorKind::RetryableIo);
        assert_eq!(root.read_limited(&path, 1024).unwrap(), b"old");
    }

    #[test]
    fn accepted_is_not_a_file_publication_claim() {
        let (_directory, transaction) = transaction();
        let path = StorePath::parse("snapshot.json").unwrap();
        let error = transaction
            .replace_snapshot(&path, b"new", DurabilityLevel::Accepted)
            .unwrap_err();
        assert_eq!(error.kind, PersistenceErrorKind::PermanentIo);
    }

    #[test]
    fn create_only_refuses_existing_destination() {
        let (_directory, transaction) = transaction();
        let path = StorePath::parse("note.md").unwrap();
        transaction
            .create_only(&path, b"one", DurabilityLevel::Synced)
            .unwrap();
        let error = transaction
            .create_only(&path, b"two", DurabilityLevel::Synced)
            .unwrap_err();
        assert_eq!(error.kind, PersistenceErrorKind::Conflict);
        assert_eq!(transaction.root().read_limited(&path, 64).unwrap(), b"one");
    }

    #[test]
    fn move_path_publishes_destination_and_clears_source() {
        let (_directory, transaction) = transaction();
        let from = StorePath::parse("from.md").unwrap();
        let to = StorePath::parse("to.md").unwrap();
        transaction
            .create_only(&from, b"body", DurabilityLevel::Synced)
            .unwrap();
        transaction
            .move_path(&from, &to, DurabilityLevel::Synced)
            .unwrap();
        assert!(!transaction.root().is_file(&from).unwrap());
        assert_eq!(transaction.root().read_limited(&to, 64).unwrap(), b"body");
    }

    #[test]
    fn move_path_create_only_refuses_existing_destination() {
        let (_directory, transaction) = transaction();
        let from = StorePath::parse("from.md").unwrap();
        let to = StorePath::parse("to.md").unwrap();
        transaction
            .create_only(&from, b"source", DurabilityLevel::Synced)
            .unwrap();
        transaction
            .create_only(&to, b"winner", DurabilityLevel::Synced)
            .unwrap();
        let error = transaction
            .move_path_create_only(&from, &to, DurabilityLevel::Synced)
            .unwrap_err();
        assert_eq!(error.kind, PersistenceErrorKind::Conflict);
        assert_eq!(
            transaction.root().read_limited(&from, 64).unwrap(),
            b"source"
        );
        assert_eq!(transaction.root().read_limited(&to, 64).unwrap(), b"winner");
    }

    #[test]
    fn intent_fault_preserves_absence_of_destination() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&path).unwrap());
        let faults = Arc::new(FailAt {
            target: TransactionFaultPoint::BeforeIntent,
            hits: AtomicUsize::new(0),
        });
        let transaction = FileTransaction::with_faults(Arc::clone(&root), faults);
        let intent = StorePath::parse(".medousa/vault/intent.json").unwrap();
        let error = transaction
            .write_intent(&intent, br#"{"op":1}"#, DurabilityLevel::Synced)
            .unwrap_err();
        assert_eq!(error.kind, PersistenceErrorKind::RetryableIo);
        assert!(!root.is_file(&intent).unwrap());
    }

    #[test]
    fn vault_receipt_kind_is_available() {
        let receipt =
            CommitReceipt::new(StoreKind::Vault, "note:a.md", 1, DurabilityLevel::Synced, 4);
        assert_eq!(receipt.store, StoreKind::Vault);
    }
}
