//! Shared persistence vocabulary and capability-confined file transactions.
//!
//! Domain stores retain ownership of sequencing and mutation semantics. This
//! module only owns honest durability labels and the publication boundaries
//! that must behave identically across feeds, workspace state, task runs,
//! Forge item logs, slug reservations, catalogs, and Coder checkpoints.

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::store_root::{StorePath, StoreRoot, StoreRootError};

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
        path: &StorePath,
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
        path: &StorePath,
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
        self.root.atomic_write(path, bytes)?;
        self.faults.check(TransactionFaultPoint::AfterPublish)?;
        self.faults
            .check(TransactionFaultPoint::AfterSnapshotPublish)?;
        Ok(bytes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
