//! Generation-owned workspace persistence.
//!
//! Callers submit typed deltas. One actor assigns commit order, appends a
//! recoverable journal record, maintains the durable projection, and publishes
//! periodic generation-stamped snapshots. Callers never serialize whole maps
//! and queue saturation never falls back to synchronous filesystem I/O.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use chrono::Utc;
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
const COMMAND_CHARGE_BYTES: u32 = 64 * 1024;
const QUEUE_BYTES: usize = 16 * 1024 * 1024;
const CHECKPOINT_EVERY_GENERATIONS: u64 = 128;
const MAX_JOURNAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORKSPACE_FEED_EVENTS: usize = 4_096;
const MAX_WORKSPACE_FEED_BYTES: usize = 8 * 1024 * 1024;
const MAX_WORKSPACE_RECORDS: usize = 2_000;
const MAX_WORKSPACE_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

static WRITER: OnceCell<WorkspacePersistenceHandle> = OnceCell::new();
static STARTUP_PROJECTION: OnceLock<Option<WorkspaceProjection>> = OnceLock::new();

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
}

enum PersistCommand {
    Mutation {
        mutation: Box<WorkspaceMutation>,
        _permit: OwnedSemaphorePermit,
    },
    Flush(oneshot::Sender<Result<CommitReceipt, PersistenceError>>),
}

pub fn init_persist_writer() -> Result<(), PersistenceError> {
    if WRITER.get().is_some() {
        return Ok(());
    }
    let root_path = workspace_dir();
    let mut projection = load_projection_at(&root_path)?;
    projection.recalculate_bounds();

    let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path)?);
    let snapshot_path = StorePath::parse(SNAPSHOT_FILE).expect("static workspace snapshot path");
    let snapshot_exists = root.metadata(&snapshot_path).is_ok();
    let transaction = FileTransaction::new(root);
    if !snapshot_exists {
        publish_snapshot(&transaction, &projection)?;
    }

    let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
    let handle = WorkspacePersistenceHandle {
        tx,
        queue_bytes: Arc::new(Semaphore::new(QUEUE_BYTES)),
    };
    tokio::spawn(run_persist_writer(rx, transaction, projection));
    let _ = WRITER.set(handle);
    Ok(())
}

pub async fn flush_persist_writer() -> Result<CommitReceipt, PersistenceError> {
    let handle = WRITER.get().ok_or_else(|| {
        PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer is not running",
        )
    })?;
    let (done, receive) = oneshot::channel();
    handle
        .tx
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

pub fn queue_mutation(mutation: WorkspaceMutation) -> Result<(), PersistenceError> {
    let handle = WRITER.get().ok_or_else(|| {
        PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer is not running",
        )
    })?;
    let permit = Arc::clone(&handle.queue_bytes)
        .try_acquire_many_owned(COMMAND_CHARGE_BYTES)
        .map_err(|_| {
            PersistenceError::new(
                PersistenceErrorKind::Overloaded,
                "workspace persistence byte budget is exhausted",
            )
        })?;
    handle
        .tx
        .try_send(PersistCommand::Mutation {
            mutation: Box::new(mutation),
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

pub fn startup_projection() -> Option<WorkspaceProjection> {
    STARTUP_PROJECTION
        .get_or_init(|| load_projection_at(&workspace_dir()).ok())
        .clone()
}

pub(crate) fn persisted_projection() -> Result<WorkspaceProjection, PersistenceError> {
    load_projection_at(&workspace_dir())
}

async fn run_persist_writer(
    mut rx: mpsc::Receiver<PersistCommand>,
    transaction: FileTransaction,
    mut projection: WorkspaceProjection,
) {
    let journal = StorePath::parse(JOURNAL_FILE).expect("static workspace journal path");
    let mut journal_bytes = 0usize;
    let mut last_error: Option<String> = None;

    while let Some(command) = rx.recv().await {
        let mut mutations = Vec::new();
        let mut flushes = Vec::new();
        match command {
            PersistCommand::Mutation { mutation, .. } => mutations.push(*mutation),
            PersistCommand::Flush(done) => flushes.push(done),
        }
        while mutations.len() < COMMAND_CAPACITY {
            match rx.try_recv() {
                Ok(PersistCommand::Mutation { mutation, .. }) => mutations.push(*mutation),
                Ok(PersistCommand::Flush(done)) => flushes.push(done),
                Err(_) => break,
            }
        }

        if !mutations.is_empty() {
            let mutations = coalesce_mutations(mutations);
            let generation = projection.generation.saturating_add(1);
            let record = WorkspaceJournalRecord {
                schema_version: 2,
                generation,
                mutations: mutations.clone(),
            };
            match serde_json::to_vec(&record) {
                Ok(encoded) => {
                    let tx = transaction.clone();
                    let path = journal.clone();
                    let size = encoded.len().saturating_add(1);
                    match tokio::task::spawn_blocking(move || {
                        tx.append_record(&path, &encoded, DurabilityLevel::Written)
                    })
                    .await
                    {
                        Ok(Ok(_)) => {
                            projection.generation = generation;
                            for mutation in mutations {
                                projection.apply(mutation);
                            }
                            journal_bytes = journal_bytes.saturating_add(size);
                        }
                        Ok(Err(error)) => last_error = Some(error.to_string()),
                        Err(error) => last_error = Some(error.to_string()),
                    }
                }
                Err(error) => last_error = Some(error.to_string()),
            }
        }

        let checkpoint_due = projection.generation > 0
            && (projection
                .generation
                .is_multiple_of(CHECKPOINT_EVERY_GENERATIONS)
                || journal_bytes >= MAX_JOURNAL_BYTES
                || !flushes.is_empty());
        if checkpoint_due {
            match checkpoint(&transaction, &projection).await {
                Ok(bytes) => journal_bytes = bytes,
                Err(error) => last_error = Some(error.to_string()),
            }
        }

        for done in flushes {
            let result = if let Some(message) = last_error.take() {
                Err(PersistenceError::new(
                    PersistenceErrorKind::PermanentIo,
                    message,
                ))
            } else {
                Ok(CommitReceipt::new(
                    StoreKind::Workspace,
                    "workspace",
                    projection.generation,
                    DurabilityLevel::Synced,
                    0,
                ))
            };
            let _ = done.send(result);
        }
    }
}

async fn checkpoint(
    transaction: &FileTransaction,
    projection: &WorkspaceProjection,
) -> Result<usize, PersistenceError> {
    let encoded = serde_json::to_vec(projection).map_err(|error| {
        PersistenceError::new(PersistenceErrorKind::Serialization, error.to_string())
    })?;
    if encoded.len() > MAX_WORKSPACE_SNAPSHOT_BYTES {
        return Err(PersistenceError::new(
            PersistenceErrorKind::Overloaded,
            "workspace projection exceeds the snapshot byte budget",
        ));
    }
    let tx = transaction.clone();
    let snapshot = StorePath::parse(SNAPSHOT_FILE).expect("static workspace snapshot path");
    let journal = StorePath::parse(JOURNAL_FILE).expect("static workspace journal path");
    let bytes = encoded.len();
    tokio::task::spawn_blocking(move || {
        tx.replace_snapshot(&snapshot, &encoded, DurabilityLevel::Synced)?;
        tx.replace_snapshot(&journal, b"", DurabilityLevel::Synced)?;
        Ok::<_, PersistenceError>(())
    })
    .await
    .map_err(|error| {
        PersistenceError::new(PersistenceErrorKind::PermanentIo, error.to_string())
    })??;
    Ok(bytes)
}

fn publish_snapshot(
    transaction: &FileTransaction,
    projection: &WorkspaceProjection,
) -> Result<(), PersistenceError> {
    let encoded = serde_json::to_vec(projection).map_err(|error| {
        PersistenceError::new(PersistenceErrorKind::Serialization, error.to_string())
    })?;
    let snapshot = StorePath::parse(SNAPSHOT_FILE).expect("static workspace snapshot path");
    let journal = StorePath::parse(JOURNAL_FILE).expect("static workspace journal path");
    transaction.replace_snapshot(&snapshot, &encoded, DurabilityLevel::Synced)?;
    transaction.replace_snapshot(&journal, b"", DurabilityLevel::Synced)?;
    Ok(())
}

impl WorkspaceProjection {
    fn apply(&mut self, mutation: WorkspaceMutation) {
        match mutation {
            WorkspaceMutation::AppendEventAndRevision { event, revision } => {
                self.revision = self.revision.max(revision);
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
            WorkspaceMutation::SetRevision { revision } => {
                self.revision = self.revision.max(revision);
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
        trim_record_map(&mut self.ask_jobs, |record| record.updated_at_utc);
        trim_record_map(&mut self.turn_workers, |record| record.updated_at);
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
}

fn trim_record_map<T>(map: &mut HashMap<String, T>, updated: impl Fn(&T) -> chrono::DateTime<Utc>) {
    if map.len() <= MAX_WORKSPACE_RECORDS {
        return;
    }
    let mut ordered = map
        .iter()
        .map(|(id, record)| (updated(record), id.clone()))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(at, _)| *at);
    for (_, id) in ordered
        .into_iter()
        .take(map.len().saturating_sub(MAX_WORKSPACE_RECORDS))
    {
        map.remove(&id);
    }
}

fn event_size(event: &WorkspaceEvent) -> usize {
    serde_json::to_vec(event).map_or(0, |bytes| bytes.len())
}

fn coalesce_mutations(mutations: Vec<WorkspaceMutation>) -> Vec<WorkspaceMutation> {
    let mut ordered = Vec::new();
    let mut revision = None;
    let mut columns = HashMap::new();
    let mut associations = HashMap::new();
    let mut asks = HashMap::new();
    let mut ask_retain = None;
    let mut workers = HashMap::new();
    let mut worker_retain = None;
    for mutation in mutations {
        match mutation {
            WorkspaceMutation::AppendEventAndRevision { .. } => ordered.push(mutation),
            WorkspaceMutation::SetRevision { revision: value } => revision = Some(value),
            WorkspaceMutation::SetCardColumn { card_id, column } => {
                columns.insert(card_id, column);
            }
            WorkspaceMutation::SetAssociation {
                card_id,
                association,
            } => {
                associations.insert(card_id, association);
            }
            WorkspaceMutation::UpsertAskJob { record } => {
                asks.insert(record.job_id.clone(), record);
            }
            WorkspaceMutation::RetainAskJobs { job_ids } => ask_retain = Some(job_ids),
            WorkspaceMutation::UpsertTurnWorker { record } => {
                workers.insert(record.work_id.clone(), record);
            }
            WorkspaceMutation::RetainTurnWorkers { work_ids } => worker_retain = Some(work_ids),
        }
    }
    if let Some(revision) = revision {
        ordered.push(WorkspaceMutation::SetRevision { revision });
    }
    ordered.extend(
        columns
            .into_iter()
            .map(|(card_id, column)| WorkspaceMutation::SetCardColumn { card_id, column }),
    );
    ordered.extend(associations.into_iter().map(|(card_id, association)| {
        WorkspaceMutation::SetAssociation {
            card_id,
            association,
        }
    }));
    ordered.extend(
        asks.into_values()
            .map(|record| WorkspaceMutation::UpsertAskJob { record }),
    );
    if let Some(job_ids) = ask_retain {
        ordered.push(WorkspaceMutation::RetainAskJobs { job_ids });
    }
    ordered.extend(
        workers
            .into_values()
            .map(|record| WorkspaceMutation::UpsertTurnWorker { record }),
    );
    if let Some(work_ids) = worker_retain {
        ordered.push(WorkspaceMutation::RetainTurnWorkers { work_ids });
    }
    ordered
}

fn workspace_dir() -> PathBuf {
    crate::session::medousa_data_dir().join("workspace")
}

fn load_projection_at(root_path: &Path) -> Result<WorkspaceProjection, PersistenceError> {
    let root = StoreRoot::open_or_create_nofollow(root_path)?;
    let snapshot = StorePath::parse(SNAPSHOT_FILE)?;
    let journal = StorePath::parse(JOURNAL_FILE)?;
    let raw = match root.read_limited(&snapshot, MAX_WORKSPACE_SNAPSHOT_BYTES as u64) {
        Ok(raw) => raw,
        Err(error) if error.is_not_found() => return load_legacy_projection(root_path),
        Err(error) => return Err(error.into()),
    };
    let mut projection: WorkspaceProjection = serde_json::from_slice(&raw).map_err(|error| {
        PersistenceError::new(PersistenceErrorKind::Corruption, error.to_string())
    })?;
    if projection.schema_version != 2 {
        return Err(PersistenceError::new(
            PersistenceErrorKind::Corruption,
            "unsupported workspace projection version",
        ));
    }
    if let Ok(raw) = root.read_limited(&journal, MAX_JOURNAL_BYTES as u64) {
        for line in complete_lines(&raw) {
            let record: WorkspaceJournalRecord = serde_json::from_slice(line).map_err(|error| {
                PersistenceError::new(PersistenceErrorKind::Corruption, error.to_string())
            })?;
            if record.schema_version != 2 {
                return Err(PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    "unsupported workspace journal version",
                ));
            }
            if record.generation <= projection.generation {
                continue;
            }
            if record.generation != projection.generation.saturating_add(1) {
                return Err(PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    "workspace journal generation gap",
                ));
            }
            projection.generation = record.generation;
            for mutation in record.mutations {
                projection.apply(mutation);
            }
        }
    }
    projection.recalculate_bounds();
    Ok(projection)
}

fn load_legacy_projection(root_path: &Path) -> Result<WorkspaceProjection, PersistenceError> {
    let root = StoreRoot::open_or_create_nofollow(root_path)?;
    let mut projection = WorkspaceProjection::default();
    if let Some(raw) = read_optional(&root, LEGACY_REVISION_FILE, 64)? {
        projection.revision = std::str::from_utf8(&raw)
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
    }
    if let Some(raw) = read_optional(&root, LEGACY_FEED_FILE, MAX_JOURNAL_BYTES as u64)? {
        for line in complete_lines(&raw) {
            if let Ok(event) = serde_json::from_slice(line) {
                projection.feed.push_back(event);
            }
        }
    }
    if let Some(raw) = read_optional(&root, LEGACY_CARD_STATE_FILE, MAX_JOURNAL_BYTES as u64)?
        && let Ok(snapshot) = serde_json::from_slice::<LegacyCardStateSnapshot>(&raw)
    {
        projection.card_columns = snapshot.columns;
    }
    if let Some(raw) = read_optional(&root, LEGACY_ASSOC_FILE, MAX_JOURNAL_BYTES as u64)?
        && let Ok(rows) = serde_json::from_slice::<Vec<LegacyAssociationRecord>>(&raw)
    {
        projection.associations = rows
            .into_iter()
            .map(|row| {
                (
                    row.card_id,
                    WorkCardAssociations {
                        vault_paths: row.vault_paths,
                        artifact_ids: row.artifact_ids,
                        locus_node_ids: row.locus_node_ids,
                    },
                )
            })
            .collect();
    }
    if let Some(raw) = read_optional(
        &root,
        LEGACY_ASK_JOBS_FILE,
        MAX_WORKSPACE_SNAPSHOT_BYTES as u64,
    )? && let Ok(records) = serde_json::from_slice(&raw)
    {
        projection.ask_jobs = records;
    }
    if let Some(raw) = read_optional(
        &root,
        LEGACY_TURN_WORKERS_FILE,
        MAX_WORKSPACE_SNAPSHOT_BYTES as u64,
    )? && let Ok(records) = serde_json::from_slice(&raw)
    {
        projection.turn_workers = records;
    }
    if projection.turn_workers.is_empty()
        && let Some(data_dir) = root_path.parent()
    {
        let data_root = StoreRoot::open_or_create_nofollow(data_dir)?;
        if let Some(raw) = read_optional(
            &data_root,
            "turn_workers.json",
            MAX_WORKSPACE_SNAPSHOT_BYTES as u64,
        )? && let Ok(records) = serde_json::from_slice(&raw)
        {
            projection.turn_workers = records;
        }
    }
    projection.recalculate_bounds();
    Ok(projection)
}

fn read_optional(
    root: &StoreRoot,
    relative: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, PersistenceError> {
    let path = StorePath::parse(relative)?;
    match root.read_limited(&path, limit) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn complete_lines(raw: &[u8]) -> impl Iterator<Item = &[u8]> {
    raw.split(|byte| *byte == b'\n')
        .take(if raw.ends_with(b"\n") {
            usize::MAX
        } else {
            raw.split(|byte| *byte == b'\n').count().saturating_sub(1)
        })
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalescing_keeps_appends_and_only_latest_replaceable_values() {
        let mutations = vec![
            WorkspaceMutation::SetRevision { revision: 1 },
            WorkspaceMutation::SetRevision { revision: 2 },
            WorkspaceMutation::SetCardColumn {
                card_id: "card".into(),
                column: Some(WorkBoardColumn::Backlog),
            },
            WorkspaceMutation::SetCardColumn {
                card_id: "card".into(),
                column: Some(WorkBoardColumn::Done),
            },
        ];
        let compacted = coalesce_mutations(mutations);
        assert_eq!(compacted.len(), 2);
        assert!(matches!(
            compacted[0],
            WorkspaceMutation::SetRevision { revision: 2 }
        ));
        assert!(matches!(
            compacted[1],
            WorkspaceMutation::SetCardColumn {
                column: Some(WorkBoardColumn::Done),
                ..
            }
        ));
    }

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
}
