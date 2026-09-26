//! Generation-owned workspace persistence.
//!
//! Callers submit typed deltas. One actor assigns commit order, appends a
//! recoverable journal record, maintains the durable projection, and publishes
//! periodic generation-stamped snapshots. Callers never serialize whole maps
//! and queue saturation never falls back to synchronous filesystem I/O.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{DateTime, Utc};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};

use crate::agent_runtime::turn_worker::TurnWorkRecord;
use crate::daemon_api::{WorkBoardColumn, WorkCardAssociations, WorkspaceEvent};
use crate::persistence::{
    CommitReceipt, DurabilityLevel, FileTransaction, PersistenceError, PersistenceErrorKind,
    StoreKind,
};
use crate::store_root::{StorePath, StoreRoot};
use crate::workspace::ask_job_store::AskJobRecord;

const SNAPSHOT_FILE: &str = "state-v2.json";
const JOURNAL_FILE: &str = "journal-v2.jsonl";
const LEGACY_FEED_FILE: &str = "feed.jsonl";
const LEGACY_REVISION_FILE: &str = "revision";
const LEGACY_CARD_STATE_FILE: &str = "card_states.json";
const LEGACY_ASSOC_FILE: &str = "associations.json";
const LEGACY_ASK_JOBS_FILE: &str = "ask_jobs.json";
const LEGACY_TURN_WORKERS_FILE: &str = "turn_workers.json";
const COMMAND_CAPACITY: usize = 256;
const QUEUE_BYTES: usize = 16 * 1024 * 1024;
const CHECKPOINT_EVERY_GENERATIONS: u64 = 128;
const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORKSPACE_FEED_EVENTS: usize = 4_096;
const MAX_WORKSPACE_FEED_BYTES: usize = 8 * 1024 * 1024;
const MAX_JOURNAL_RECORD_BYTES: usize = 8 * 1024 * 1024;

static DISK_ACCESS: Mutex<()> = Mutex::new(());
static WRITER: OnceCell<WorkspacePersistenceHandle> = OnceCell::new();
static STARTUP_PROJECTION: OnceLock<WorkspaceProjection> = OnceLock::new();
static INITIALIZE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static STARTUP_FAILURE: Mutex<Option<PersistenceFailure>> = Mutex::new(None);

mod recovery;
use recovery::load_projection_at;

#[derive(Clone)]
struct PersistenceFailure {
    kind: PersistenceErrorKind,
    message: String,
}

impl PersistenceFailure {
    fn from_error(error: &PersistenceError) -> Self {
        Self {
            kind: error.kind,
            message: error.to_string(),
        }
    }

    fn error(&self) -> PersistenceError {
        PersistenceError::new(self.kind, &self.message)
    }
}

fn execution_service() -> &'static medousa_forge::execution::ForgeExecutionService {
    static SERVICE: OnceLock<medousa_forge::execution::ForgeExecutionService> = OnceLock::new();
    SERVICE.get_or_init(medousa_forge::execution::ForgeExecutionService::new)
}

async fn store_io<T: Send + 'static>(
    retained_bytes: usize,
    work: impl FnOnce() -> Result<T, PersistenceError> + Send + 'static,
) -> Result<T, PersistenceError> {
    execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::Compaction,
            retained_bytes,
            move || Ok(work()),
        )
        .await
        .map_err(|error| {
            PersistenceError::new(PersistenceErrorKind::PermanentIo, error.to_string())
        })?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceMutation {
    AppendEventAndRevision {
        event: WorkspaceEvent,
        revision: u64,
    },
    SetRevision {
        revision: u64,
    },
    PruneFeedBefore {
        cutoff: DateTime<Utc>,
    },
    SetCardColumn {
        card_id: String,
        column: Option<WorkBoardColumn>,
    },
    SetAssociation {
        card_id: String,
        association: WorkCardAssociations,
    },
    UpsertAskJob {
        record: Box<AskJobRecord>,
    },
    RetainAskJobs {
        job_ids: Vec<String>,
    },
    UpsertTurnWorker {
        record: Box<TurnWorkRecord>,
    },
    RetainTurnWorkers {
        work_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProjection {
    pub schema_version: u8,
    pub generation: u64,
    pub revision: u64,
    #[serde(default)]
    pub feed: VecDeque<WorkspaceEvent>,
    #[serde(default)]
    pub card_columns: HashMap<String, WorkBoardColumn>,
    #[serde(default)]
    pub associations: HashMap<String, WorkCardAssociations>,
    #[serde(default)]
    pub ask_jobs: HashMap<String, AskJobRecord>,
    #[serde(default)]
    pub turn_workers: HashMap<String, TurnWorkRecord>,
    #[serde(skip)]
    feed_bytes: usize,
}

impl Default for WorkspaceProjection {
    fn default() -> Self {
        Self {
            schema_version: 2,
            generation: 0,
            revision: 0,
            feed: VecDeque::new(),
            card_columns: HashMap::new(),
            associations: HashMap::new(),
            ask_jobs: HashMap::new(),
            turn_workers: HashMap::new(),
            feed_bytes: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspaceJournalRecord {
    schema_version: u8,
    generation: u64,
    mutations: Vec<WorkspaceMutation>,
}

#[derive(Debug, Deserialize)]
struct LegacyCardStateSnapshot {
    #[serde(default)]
    columns: HashMap<String, WorkBoardColumn>,
}

#[derive(Debug, Deserialize)]
struct LegacyAssociationRecord {
    card_id: String,
    #[serde(default)]
    vault_paths: Vec<String>,
    #[serde(default)]
    artifact_ids: Vec<String>,
    #[serde(default)]
    locus_node_ids: Vec<String>,
}

#[derive(Clone)]
struct WorkspacePersistenceHandle {
    tx: mpsc::Sender<PersistCommand>,
    queue_bytes: Arc<Semaphore>,
    failure: Arc<Mutex<Option<PersistenceFailure>>>,
}

enum PersistCommand {
    Mutations {
        mutations: Vec<WorkspaceMutation>,
        _permit: OwnedSemaphorePermit,
    },
    Flush(oneshot::Sender<Result<CommitReceipt, PersistenceError>>),
}

impl WorkspacePersistenceHandle {
    fn ensure_available(&self) -> Result<(), PersistenceError> {
        if let Some(failure) = self.failure.lock().expect("workspace failure").as_ref() {
            return Err(failure.error());
        }
        if self.tx.is_closed() {
            return Err(PersistenceError::new(
                PersistenceErrorKind::ShuttingDown,
                "workspace persistence writer is closed",
            ));
        }
        Ok(())
    }

    fn submit(&self, mutations: Vec<WorkspaceMutation>) -> Result<(), PersistenceError> {
        self.ensure_available()?;
        if mutations.is_empty() {
            return Ok(());
        }
        // Charge the serialized payload, including the largest generation header,
        // before accepting it. A single atomic command must fit one recovery record.
        let record = WorkspaceJournalRecord {
            schema_version: 2,
            generation: u64::MAX,
            mutations,
        };
        let mut counter = RecordSize { bytes: 0 };
        serde_json::to_writer(&mut counter, &record).map_err(|error| {
            PersistenceError::new(PersistenceErrorKind::Overloaded,
                format!("workspace mutation exceeds the {MAX_JOURNAL_RECORD_BYTES}-byte record budget: {error}"))
        })?;
        let permit = Arc::clone(&self.queue_bytes)
            .try_acquire_many_owned(counter.bytes.max(1) as u32)
            .map_err(|_| {
                PersistenceError::new(
                    PersistenceErrorKind::Overloaded,
                    "workspace persistence byte budget is exhausted",
                )
            })?;
        self.tx
            .try_send(PersistCommand::Mutations {
                mutations: record.mutations,
                _permit: permit,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => PersistenceError::new(
                    PersistenceErrorKind::Overloaded,
                    "workspace persistence command queue is full",
                ),
                mpsc::error::TrySendError::Closed(_) => PersistenceError::new(
                    PersistenceErrorKind::ShuttingDown,
                    "workspace persistence writer is closed",
                ),
            })
    }

    async fn flush(&self) -> Result<CommitReceipt, PersistenceError> {
        self.ensure_available()?;
        let (done, receive) = oneshot::channel();
        self.tx
            .send(PersistCommand::Flush(done))
            .await
            .map_err(|_| {
                PersistenceError::new(
                    PersistenceErrorKind::ShuttingDown,
                    "workspace persistence writer is closed",
                )
            })?;
        receive.await.map_err(|_| {
            PersistenceError::new(
                PersistenceErrorKind::ShuttingDown,
                "workspace persistence writer stopped before flush acknowledgement",
            )
        })?
    }
}

struct RecordSize {
    bytes: usize,
}
impl std::io::Write for RecordSize {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let size = self.bytes.saturating_add(buffer.len());
        if size > MAX_JOURNAL_RECORD_BYTES {
            return Err(std::io::Error::other("record byte budget exceeded"));
        }
        self.bytes = size;
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub async fn init_persist_writer() -> Result<(), PersistenceError> {
    let _initializing = INITIALIZE.lock().await;
    if let Some(writer) = WRITER.get() {
        return writer.ensure_available();
    }
    let recovered = store_io(0, || {
        let root_path = workspace_dir();
        let projection = load_projection_at(&root_path)?;
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path)?);
        let transaction = FileTransaction::new(root);
        // Commit all recovered generations before retiring the old journal. This
        // also removes an interrupted final append before any new append starts.
        publish_snapshot(&transaction, &projection)?;
        Ok((transaction, projection))
    })
    .await;
    let (transaction, projection) = match recovered {
        Ok(value) => value,
        Err(error) => {
            *STARTUP_FAILURE.lock().expect("workspace startup failure") =
                Some(PersistenceFailure::from_error(&error));
            return Err(error);
        }
    };
    let _ = STARTUP_PROJECTION.set(projection.clone());
    *STARTUP_FAILURE.lock().expect("workspace startup failure") = None;
    let (handle, rx) = writer_channel();
    tokio::spawn(run_persist_writer(
        rx,
        transaction,
        projection,
        Arc::clone(&handle.failure),
    ));
    let _ = WRITER.set(handle);
    Ok(())
}

fn writer_channel() -> (WorkspacePersistenceHandle, mpsc::Receiver<PersistCommand>) {
    let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
    (
        WorkspacePersistenceHandle {
            tx,
            queue_bytes: Arc::new(Semaphore::new(QUEUE_BYTES)),
            failure: Arc::new(Mutex::new(None)),
        },
        rx,
    )
}

fn writer() -> Result<&'static WorkspacePersistenceHandle, PersistenceError> {
    WRITER.get().ok_or_else(|| {
        STARTUP_FAILURE
            .lock()
            .expect("workspace startup failure")
            .as_ref()
            .map(PersistenceFailure::error)
            .unwrap_or_else(|| {
                PersistenceError::new(
                    PersistenceErrorKind::ShuttingDown,
                    "workspace persistence writer is not running",
                )
            })
    })
}

pub fn ensure_persistence_available() -> Result<(), PersistenceError> {
    writer()?.ensure_available()
}

pub async fn flush_persist_writer() -> Result<CommitReceipt, PersistenceError> {
    writer()?.flush().await
}

/// Accept one atomic mutation batch. Acceptance is not a durability claim;
/// callers requiring a durable acknowledgement must await the flush barrier.
pub fn queue_mutations(mutations: Vec<WorkspaceMutation>) -> Result<(), PersistenceError> {
    writer()?.submit(mutations)
}

pub fn queue_mutation(mutation: WorkspaceMutation) -> Result<(), PersistenceError> {
    queue_mutations(vec![mutation])
}

/// Stores hydrate only after the asynchronous bootstrap has recovered the disk.
/// Lazy store construction must never perform blocking recovery on a request.
pub fn startup_projection() -> Option<WorkspaceProjection> {
    STARTUP_PROJECTION.get().cloned()
}

pub(crate) fn persisted_projection() -> Result<WorkspaceProjection, PersistenceError> {
    load_projection_at(&workspace_dir())
}

async fn run_persist_writer(
    mut rx: mpsc::Receiver<PersistCommand>,
    transaction: FileTransaction,
    projection: WorkspaceProjection,
    failure: Arc<Mutex<Option<PersistenceFailure>>>,
) {
    let mut projection = Arc::new(projection);
    let mut journal_bytes = 0usize;
    while let Some(command) = rx.recv().await {
        // An append error can mean a partial write or an unacknowledged complete
        // write. Never append past it or claim a later flush succeeded. Recovery
        // on restart determines the last complete generation.
        if let Some(error) = failure
            .lock()
            .expect("workspace failure")
            .as_ref()
            .map(PersistenceFailure::error)
        {
            match command {
                PersistCommand::Flush(done) => {
                    let _ = done.send(Err(error));
                }
                PersistCommand::Mutations { .. } => {
                    tracing::error!(%error, "accepted workspace mutation could not be persisted");
                }
            }
            continue;
        }
        let result = match command {
            PersistCommand::Mutations { mutations, _permit } => {
                let generation = projection.generation.checked_add(1);
                let result = match generation {
                    Some(generation) => {
                        let tx = transaction.clone();
                        store_io(_permit.num_permits(), move || {
                            let record = WorkspaceJournalRecord {
                                schema_version: 2,
                                generation,
                                mutations,
                            };
                            let encoded = serde_json::to_vec(&record).map_err(|error| {
                                PersistenceError::new(
                                    PersistenceErrorKind::Serialization,
                                    error.to_string(),
                                )
                            })?;
                            let journal = StorePath::parse(JOURNAL_FILE)?;
                            let _disk = DISK_ACCESS.lock().expect("workspace disk access");
                            let bytes =
                                tx.append_record(&journal, &encoded, DurabilityLevel::Written)?;
                            Ok((record, bytes))
                        })
                        .await
                    }
                    None => Err(PersistenceError::new(
                        PersistenceErrorKind::Corruption,
                        "workspace generation exhausted",
                    )),
                };
                match result {
                    Ok((record, bytes)) => {
                        Arc::make_mut(&mut projection).generation = record.generation;
                        for mutation in record.mutations {
                            if let WorkspaceMutation::UpsertTurnWorker { record } = &mutation
                                && let Err(error) =
                                    crate::assistant_assignments::project_turn_worker(record).await
                            {
                                tracing::warn!(work_id = %record.work_id, %error, "assistant assignment projection failed");
                            }
                            Arc::make_mut(&mut projection).apply(mutation);
                        }
                        journal_bytes = journal_bytes.saturating_add(bytes);
                        if projection
                            .generation
                            .is_multiple_of(CHECKPOINT_EVERY_GENERATIONS)
                            || journal_bytes >= MAX_JOURNAL_BYTES
                        {
                            checkpoint(&transaction, &projection)
                                .await
                                .map(|()| journal_bytes = 0)
                        } else {
                            Ok(())
                        }
                    }
                    Err(error) => Err(error),
                }
                // The byte permit remains held through I/O and projection apply.
            }
            PersistCommand::Flush(done) => {
                let result = checkpoint(&transaction, &projection).await;
                let acknowledgement = match &result {
                    Ok(()) => {
                        journal_bytes = 0;
                        Ok(CommitReceipt::new(
                            StoreKind::Workspace,
                            "workspace",
                            projection.generation,
                            DurabilityLevel::Synced,
                            0,
                        ))
                    }
                    Err(error) => Err(PersistenceFailure::from_error(error).error()),
                };
                let _ = done.send(acknowledgement);
                result
            }
        };
        if let Err(error) = result {
            tracing::error!(%error, generation = projection.generation,
                "workspace persistence failed; new writes are disabled until recovery");
            *failure.lock().expect("workspace failure") =
                Some(PersistenceFailure::from_error(&error));
        }
    }
}

async fn checkpoint(
    transaction: &FileTransaction,
    projection: &Arc<WorkspaceProjection>,
) -> Result<(), PersistenceError> {
    let transaction = transaction.clone();
    let projection = Arc::clone(projection);
    store_io(0, move || publish_snapshot(&transaction, &projection)).await
}

fn publish_snapshot(
    transaction: &FileTransaction,
    projection: &WorkspaceProjection,
) -> Result<(), PersistenceError> {
    let _disk = DISK_ACCESS.lock().expect("workspace disk access");
    let snapshot = StorePath::parse(SNAPSHOT_FILE)?;
    let journal = StorePath::parse(JOURNAL_FILE)?;
    transaction.replace_snapshot_json(&snapshot, projection)?;
    transaction.replace_snapshot(&journal, b"", DurabilityLevel::Synced)?;
    Ok(())
}

impl WorkspaceProjection {
    fn apply(&mut self, mutation: WorkspaceMutation) {
        match mutation {
            WorkspaceMutation::AppendEventAndRevision { event, revision } => {
                self.revision = self.revision.max(revision);
                self.retain_feed_event(event);
            }
            WorkspaceMutation::SetRevision { revision } => {
                self.revision = self.revision.max(revision);
            }
            WorkspaceMutation::PruneFeedBefore { cutoff } => {
                self.feed.retain(|event| event.timestamp_utc >= cutoff);
                self.recalculate_bounds();
            }
            WorkspaceMutation::SetCardColumn { card_id, column } => {
                if let Some(column) = column {
                    self.card_columns.insert(card_id, column);
                } else {
                    self.card_columns.remove(&card_id);
                }
            }
            WorkspaceMutation::SetAssociation {
                card_id,
                association,
            } => {
                self.associations.insert(card_id, association);
            }
            WorkspaceMutation::UpsertAskJob { record } => {
                self.ask_jobs.insert(record.job_id.clone(), *record);
            }
            WorkspaceMutation::RetainAskJobs { job_ids } => {
                let retained = job_ids.into_iter().collect::<HashSet<_>>();
                self.ask_jobs.retain(|id, _| retained.contains(id));
            }
            WorkspaceMutation::UpsertTurnWorker { record } => {
                self.turn_workers.insert(record.work_id.clone(), *record);
            }
            WorkspaceMutation::RetainTurnWorkers { work_ids } => {
                let retained = work_ids.into_iter().collect::<HashSet<_>>();
                self.turn_workers.retain(|id, _| retained.contains(id));
            }
        }
        // Domain stores own retention and distinguish active work from completed
        // history. Persistence must not independently evict live records.
    }

    fn recalculate_bounds(&mut self) {
        self.feed_bytes = self.feed.iter().map(event_size).sum();
        while self.feed.len() > MAX_WORKSPACE_FEED_EVENTS
            || self.feed_bytes > MAX_WORKSPACE_FEED_BYTES
        {
            let Some(event) = self.feed.pop_front() else {
                break;
            };
            self.feed_bytes = self.feed_bytes.saturating_sub(event_size(&event));
        }
    }

    fn retain_feed_event(&mut self, event: WorkspaceEvent) {
        self.feed_bytes = self.feed_bytes.saturating_add(event_size(&event));
        self.feed.push_back(event);
        while self.feed.len() > MAX_WORKSPACE_FEED_EVENTS
            || self.feed_bytes > MAX_WORKSPACE_FEED_BYTES
        {
            let Some(event) = self.feed.pop_front() else {
                break;
            };
            self.feed_bytes = self.feed_bytes.saturating_sub(event_size(&event));
        }
    }
}

fn event_size(event: &WorkspaceEvent) -> usize {
    serde_json::to_vec(event).map_or(0, |bytes| bytes.len())
}

fn workspace_dir() -> PathBuf {
    crate::session::medousa_data_dir().join("workspace")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_retention_is_bounded_by_count() {
        let mut projection = WorkspaceProjection::default();
        for index in 0..MAX_WORKSPACE_FEED_EVENTS + 10 {
            projection.apply(WorkspaceMutation::AppendEventAndRevision {
                event: WorkspaceEvent {
                    id: format!("event-{index}"),
                    timestamp_utc: Utc::now(),
                    kind: crate::daemon_api::WorkspaceEventKind::TurnCompleted,
                    actor: crate::daemon_api::WorkspaceEventActor::System,
                    summary: "test".into(),
                    refs: Vec::new(),
                    detail_line: None,
                    context_line: None,
                    intent: None,
                    tool_names: Vec::new(),
                },
                revision: index as u64,
            });
        }
        assert_eq!(projection.feed.len(), MAX_WORKSPACE_FEED_EVENTS);
        assert_eq!(projection.revision, (MAX_WORKSPACE_FEED_EVENTS + 9) as u64);
    }

    #[test]
    fn legacy_feed_larger_than_journal_cap_imports_only_the_bounded_tail() {
        use std::io::Write as _;

        let directory = tempfile::tempdir().unwrap();
        let root_path = directory.path().canonicalize().unwrap().join("workspace");
        std::fs::create_dir_all(&root_path).unwrap();
        let feed_path = root_path.join(LEGACY_FEED_FILE);
        let mut feed = std::fs::File::create(&feed_path).unwrap();
        for index in 0..100 {
            let event = WorkspaceEvent {
                id: format!("legacy-event-{index:03}"),
                timestamp_utc: Utc::now(),
                kind: crate::daemon_api::WorkspaceEventKind::TurnCompleted,
                actor: crate::daemon_api::WorkspaceEventActor::System,
                summary: "x".repeat(100_000),
                refs: Vec::new(),
                detail_line: None,
                context_line: None,
                intent: None,
                tool_names: Vec::new(),
            };
            writeln!(feed, "{}", serde_json::to_string(&event).unwrap()).unwrap();
        }
        drop(feed);
        assert!(std::fs::metadata(&feed_path).unwrap().len() > MAX_JOURNAL_BYTES as u64);

        std::fs::write(root_path.join(LEGACY_REVISION_FILE), "42\n").unwrap();
        std::fs::write(
            root_path.join(LEGACY_CARD_STATE_FILE),
            r#"{"columns":{"card-1":"done"}}"#,
        )
        .unwrap();
        std::fs::write(
            root_path.join(LEGACY_ASSOC_FILE),
            r#"[{"card_id":"card-1","vault_paths":["notes/project.md"]}]"#,
        )
        .unwrap();

        let projection = load_projection_at(&root_path).unwrap();
        assert_eq!(projection.revision, 42);
        assert_eq!(
            projection.card_columns.get("card-1"),
            Some(&WorkBoardColumn::Done)
        );
        assert_eq!(
            projection.associations["card-1"].vault_paths,
            vec!["notes/project.md"]
        );
        assert!(projection.feed.len() < 100);
        assert!(projection.feed_bytes <= MAX_WORKSPACE_FEED_BYTES);
        assert_eq!(
            projection.feed.back().map(|event| event.id.as_str()),
            Some("legacy-event-099")
        );
        assert_ne!(
            projection.feed.front().map(|event| event.id.as_str()),
            Some("legacy-event-000")
        );
    }

    #[test]
    fn recovery_discards_a_partial_final_journal_record() {
        let directory = tempfile::tempdir().unwrap();
        let root_path = directory.path().canonicalize().unwrap().join("workspace");
        std::fs::create_dir_all(&root_path).unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path).unwrap());
        let transaction = FileTransaction::new(Arc::clone(&root));
        publish_snapshot(&transaction, &WorkspaceProjection::default()).unwrap();
        let journal = StorePath::parse(JOURNAL_FILE).unwrap();
        let record = WorkspaceJournalRecord {
            schema_version: 2,
            generation: 1,
            mutations: vec![WorkspaceMutation::SetRevision { revision: 7 }],
        };
        transaction
            .append_record(
                &journal,
                &serde_json::to_vec(&record).unwrap(),
                DurabilityLevel::Written,
            )
            .unwrap();
        root.append(&journal, b"{\"schema_version\":2").unwrap();

        let recovered = load_projection_at(&root_path).unwrap();
        assert_eq!(recovered.generation, 1);
        assert_eq!(recovered.revision, 7);
    }

    struct FailAt(crate::persistence::TransactionFaultPoint);
    impl crate::persistence::TransactionFaults for FailAt {
        fn check(
            &self,
            point: crate::persistence::TransactionFaultPoint,
        ) -> Result<(), PersistenceError> {
            if point == self.0 {
                Err(PersistenceError::new(
                    PersistenceErrorKind::RetryableIo,
                    "injected disk failure",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn test_transaction() -> (tempfile::TempDir, FileTransaction) {
        let directory = tempfile::tempdir().unwrap();
        let root = Arc::new(
            StoreRoot::open_or_create_nofollow(&directory.path().canonicalize().unwrap()).unwrap(),
        );
        let transaction = FileTransaction::new(root);
        publish_snapshot(&transaction, &WorkspaceProjection::default()).unwrap();
        (directory, transaction)
    }

    #[tokio::test]
    async fn writer_flush_is_a_durable_ordered_barrier() {
        let (directory, transaction) = test_transaction();
        let (handle, rx) = writer_channel();
        let task = tokio::spawn(run_persist_writer(
            rx,
            transaction,
            WorkspaceProjection::default(),
            Arc::clone(&handle.failure),
        ));
        handle
            .submit(vec![
                WorkspaceMutation::SetRevision { revision: 7 },
                WorkspaceMutation::SetCardColumn {
                    card_id: "card".into(),
                    column: Some(WorkBoardColumn::Done),
                },
            ])
            .unwrap();
        let receipt = handle.flush().await.unwrap();
        assert_eq!(receipt.durability, DurabilityLevel::Synced);
        assert_eq!(receipt.generation, 1);
        let recovered = load_projection_at(&directory.path().canonicalize().unwrap()).unwrap();
        assert_eq!(recovered.revision, 7);
        assert_eq!(
            recovered.card_columns.get("card"),
            Some(&WorkBoardColumn::Done)
        );
        assert_eq!(
            std::fs::metadata(directory.path().join(JOURNAL_FILE))
                .unwrap()
                .len(),
            0
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn append_failure_is_sticky_across_flushes_and_rejects_new_work() {
        use crate::persistence::TransactionFaultPoint;
        let (directory, transaction) = test_transaction();
        let root = Arc::new(
            StoreRoot::open_or_create_nofollow(&directory.path().canonicalize().unwrap()).unwrap(),
        );
        let broken = FileTransaction::with_faults(
            root,
            Arc::new(FailAt(TransactionFaultPoint::BeforeAppend)),
        );
        let (handle, rx) = writer_channel();
        let task = tokio::spawn(run_persist_writer(
            rx,
            broken,
            WorkspaceProjection::default(),
            Arc::clone(&handle.failure),
        ));
        handle
            .submit(vec![WorkspaceMutation::SetRevision { revision: 9 }])
            .unwrap();
        assert!(handle.flush().await.is_err());
        assert!(handle.flush().await.is_err());
        assert!(
            handle
                .submit(vec![WorkspaceMutation::SetRevision { revision: 10 }])
                .is_err()
        );
        assert_eq!(
            load_projection_at(&directory.path().canonicalize().unwrap())
                .unwrap()
                .revision,
            0
        );
        assert_eq!(
            transaction
                .root()
                .metadata(&StorePath::parse(JOURNAL_FILE).unwrap())
                .unwrap()
                .size,
            0
        );
        drop(handle);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn uncertain_append_is_recovered_once_without_replay() {
        use crate::persistence::TransactionFaultPoint;
        let (directory, _) = test_transaction();
        let root = Arc::new(
            StoreRoot::open_or_create_nofollow(&directory.path().canonicalize().unwrap()).unwrap(),
        );
        let broken = FileTransaction::with_faults(
            root,
            Arc::new(FailAt(TransactionFaultPoint::AfterAppend)),
        );
        let (handle, rx) = writer_channel();
        let task = tokio::spawn(run_persist_writer(
            rx,
            broken,
            WorkspaceProjection::default(),
            Arc::clone(&handle.failure),
        ));
        handle
            .submit(vec![WorkspaceMutation::SetRevision { revision: 9 }])
            .unwrap();
        assert!(handle.flush().await.is_err());
        drop(handle);
        task.await.unwrap();
        let recovered = load_projection_at(&directory.path().canonicalize().unwrap()).unwrap();
        assert_eq!(recovered.generation, 1);
        assert_eq!(recovered.revision, 9);
    }

    #[test]
    fn crash_after_snapshot_publish_preserves_recovery_and_next_append() {
        use crate::persistence::TransactionFaultPoint;
        let (directory, transaction) = test_transaction();
        let root_path = directory.path().canonicalize().unwrap();
        let journal = StorePath::parse(JOURNAL_FILE).unwrap();
        transaction
            .append_record(
                &journal,
                &serde_json::to_vec(&WorkspaceJournalRecord {
                    schema_version: 2,
                    generation: 1,
                    mutations: vec![WorkspaceMutation::SetRevision { revision: 7 }],
                })
                .unwrap(),
                DurabilityLevel::Synced,
            )
            .unwrap();
        transaction
            .root()
            .append(&journal, b"{interrupted")
            .unwrap();
        let recovered = load_projection_at(&root_path).unwrap();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path).unwrap());
        let broken = FileTransaction::with_faults(
            root,
            Arc::new(FailAt(TransactionFaultPoint::AfterSnapshotPublish)),
        );
        assert!(publish_snapshot(&broken, &recovered).is_err());
        let recovered = load_projection_at(&root_path).unwrap();
        assert_eq!(recovered.generation, 1);
        publish_snapshot(&transaction, &recovered).unwrap();
        transaction
            .append_record(
                &journal,
                &serde_json::to_vec(&WorkspaceJournalRecord {
                    schema_version: 2,
                    generation: 2,
                    mutations: vec![WorkspaceMutation::SetRevision { revision: 8 }],
                })
                .unwrap(),
                DurabilityLevel::Synced,
            )
            .unwrap();
        assert_eq!(load_projection_at(&root_path).unwrap().revision, 8);
    }

    #[test]
    fn admission_accounts_for_payload_bytes_and_rejects_unrecoverable_records() {
        let (handle, mut rx) = writer_channel();
        handle
            .submit(vec![WorkspaceMutation::SetCardColumn {
                card_id: "x".repeat(128 * 1024),
                column: Some(WorkBoardColumn::Done),
            }])
            .unwrap();
        assert!(handle.queue_bytes.available_permits() < QUEUE_BYTES - 128 * 1024);
        let command = rx.try_recv().unwrap();
        assert!(handle.queue_bytes.available_permits() < QUEUE_BYTES - 128 * 1024);
        drop(command);
        assert_eq!(handle.queue_bytes.available_permits(), QUEUE_BYTES);
        let error = handle
            .submit(vec![WorkspaceMutation::SetCardColumn {
                card_id: "x".repeat(MAX_JOURNAL_RECORD_BYTES),
                column: None,
            }])
            .unwrap_err();
        assert_eq!(error.kind, PersistenceErrorKind::Overloaded);
        assert!(rx.try_recv().is_err());
        assert_eq!(handle.queue_bytes.available_permits(), QUEUE_BYTES);
    }

    #[test]
    fn snapshot_aggregate_size_is_not_a_history_read_limit() {
        let (directory, transaction) = test_transaction();
        let mut projection = WorkspaceProjection::default();
        for index in 0..65 {
            projection.associations.insert(
                format!("card-{index}"),
                WorkCardAssociations {
                    vault_paths: vec!["x".repeat(1024 * 1024)],
                    artifact_ids: vec![],
                    locus_node_ids: vec![],
                },
            );
        }
        publish_snapshot(&transaction, &projection).unwrap();
        assert!(
            std::fs::metadata(directory.path().join(SNAPSHOT_FILE))
                .unwrap()
                .len()
                > 64 * 1024 * 1024
        );
        drop(projection);
        let recovered = load_projection_at(&directory.path().canonicalize().unwrap()).unwrap();
        assert_eq!(recovered.associations.len(), 65);
        assert_eq!(
            recovered.associations["card-64"].vault_paths[0].len(),
            1024 * 1024
        );
    }

    #[tokio::test]
    async fn checkpoint_resets_journal_accounting_instead_of_counting_snapshot_bytes() {
        use crate::persistence::{TransactionFaultPoint, TransactionFaults};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CountSnapshots(AtomicUsize);
        impl TransactionFaults for CountSnapshots {
            fn check(&self, point: TransactionFaultPoint) -> Result<(), PersistenceError> {
                if point == TransactionFaultPoint::BeforeSnapshotPublish {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
                Ok(())
            }
        }
        let (directory, _) = test_transaction();
        let root = Arc::new(
            StoreRoot::open_or_create_nofollow(&directory.path().canonicalize().unwrap()).unwrap(),
        );
        let counts = Arc::new(CountSnapshots(AtomicUsize::new(0)));
        let transaction = FileTransaction::with_faults(root, counts.clone());
        let (handle, rx) = writer_channel();
        let task = tokio::spawn(run_persist_writer(
            rx,
            transaction,
            WorkspaceProjection::default(),
            Arc::clone(&handle.failure),
        ));
        for index in 0..9 {
            handle
                .submit(vec![WorkspaceMutation::SetAssociation {
                    card_id: format!("card-{index}"),
                    association: WorkCardAssociations {
                        vault_paths: vec!["x".repeat(1024 * 1024)],
                        artifact_ids: vec![],
                        locus_node_ids: vec![],
                    },
                }])
                .unwrap();
        }
        handle.flush().await.unwrap();
        // One size-triggered checkpoint, one flush. Each publishes snapshot + journal.
        assert_eq!(counts.0.load(Ordering::SeqCst), 4);
        drop(handle);
        task.await.unwrap();
    }

    #[test]
    fn persistence_never_evicts_active_jobs_to_enforce_a_history_count() {
        let now = Utc::now();
        let mut projection = WorkspaceProjection::default();
        for index in 0..2_001 {
            let record: AskJobRecord = serde_json::from_value(serde_json::json!({
                "job_id": format!("active-{index}"), "prompt": "long build", "status": "running",
                "session_id": "session", "created_at_utc": now, "updated_at_utc": now,
            }))
            .unwrap();
            projection.apply(WorkspaceMutation::UpsertAskJob {
                record: Box::new(record),
            });
        }
        assert_eq!(projection.ask_jobs.len(), 2_001);
        assert!(projection.ask_jobs.contains_key("active-0"));
    }

    #[test]
    fn feed_retention_survives_journal_recovery() {
        let (directory, transaction) = test_transaction();
        let cutoff = Utc::now() - chrono::Duration::days(7);
        let mut old = crate::workspace::event::event_for_vault_link("old", "notes/old.md").unwrap();
        old.timestamp_utc = cutoff - chrono::Duration::seconds(1);
        let recent =
            crate::workspace::event::event_for_vault_link("recent", "notes/recent.md").unwrap();
        let retained_id = recent.id.clone();
        let record = WorkspaceJournalRecord {
            schema_version: 2,
            generation: 1,
            mutations: vec![
                WorkspaceMutation::AppendEventAndRevision {
                    event: old,
                    revision: 1,
                },
                WorkspaceMutation::AppendEventAndRevision {
                    event: recent,
                    revision: 2,
                },
                WorkspaceMutation::PruneFeedBefore { cutoff },
            ],
        };
        transaction
            .append_record(
                &StorePath::parse(JOURNAL_FILE).unwrap(),
                &serde_json::to_vec(&record).unwrap(),
                DurabilityLevel::Synced,
            )
            .unwrap();
        let recovered = load_projection_at(&directory.path().canonicalize().unwrap()).unwrap();
        assert_eq!(recovered.feed.len(), 1);
        assert_eq!(recovered.feed[0].id, retained_id);
        assert_eq!(recovered.revision, 2);
    }
}
