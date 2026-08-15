//! Async workspace persistence — single writer task, debounced snapshot files.

use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use once_cell::sync::OnceCell;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Sleep};

use crate::persistence::{PersistenceError, PersistenceErrorKind};
use crate::session;

const DEBOUNCE_MS: u64 = 1500;

const FEED_FILE: &str = "feed.jsonl";
const REVISION_FILE: &str = "revision";
const CARD_STATE_FILE: &str = "card_states.json";
const ASSOC_FILE: &str = "associations.json";
const ASK_JOBS_FILE: &str = "ask_jobs.json";
const TURN_WORKERS_FILE: &str = "turn_workers.json";

static WRITER_TX: OnceCell<mpsc::Sender<PersistOp>> = OnceCell::new();

enum PersistOp {
    AppendFeedLine(String),
    WriteRevision(u64),
    SnapshotCardStates(String),
    SnapshotAssociations(String),
    SnapshotAskJobs(String),
    SnapshotTurnWorkers(String),
    Flush(oneshot::Sender<Result<(), PersistenceError>>),
}

#[derive(Default)]
struct PendingSnapshots {
    card_states: Option<String>,
    associations: Option<String>,
    ask_jobs: Option<String>,
    turn_workers: Option<String>,
}

impl PendingSnapshots {
    fn any(&self) -> bool {
        self.card_states.is_some()
            || self.associations.is_some()
            || self.ask_jobs.is_some()
            || self.turn_workers.is_some()
    }

    fn take_all(&mut self) -> Self {
        std::mem::take(self)
    }
}

fn workspace_dir() -> PathBuf {
    session::medousa_data_dir().join("workspace")
}

fn workspace_path(relative: &str) -> PathBuf {
    workspace_dir().join(relative)
}

async fn ensure_workspace_dir() -> std::io::Result<()> {
    fs::create_dir_all(workspace_dir()).await
}

/// Start the global persist writer (daemon bootstrap). Idempotent.
pub fn init_persist_writer() {
    if WRITER_TX.get().is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel(512);
    tokio::spawn(run_persist_writer(rx));
    let _ = WRITER_TX.set(tx);
}

pub async fn flush_persist_writer() -> Result<(), PersistenceError> {
    let Some(tx) = WRITER_TX.get() else {
        return Err(PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer is not running",
        ));
    };
    let (done, rx) = oneshot::channel();
    if tx.send(PersistOp::Flush(done)).await.is_err() {
        return Err(PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer is closed",
        ));
    }
    rx.await.map_err(|_| {
        PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer stopped before flush acknowledgement",
        )
    })?
}

fn try_enqueue(op: PersistOp) -> Result<(), PersistenceError> {
    let Some(tx) = WRITER_TX.get() else {
        return Err(PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer is not running",
        ));
    };
    match tx.try_send(op) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(_)) => Err(PersistenceError::new(
            PersistenceErrorKind::Overloaded,
            "workspace persistence queue is full",
        )),
        Err(mpsc::error::TrySendError::Closed(_)) => Err(PersistenceError::new(
            PersistenceErrorKind::ShuttingDown,
            "workspace persistence writer is closed",
        )),
    }
}

pub fn queue_append_feed_line(line: String) -> Result<(), PersistenceError> {
    try_enqueue(PersistOp::AppendFeedLine(line))
}

pub fn queue_write_revision(revision: u64) -> Result<(), PersistenceError> {
    try_enqueue(PersistOp::WriteRevision(revision))
}

pub fn queue_snapshot_card_states(body: String) -> Result<(), PersistenceError> {
    try_enqueue(PersistOp::SnapshotCardStates(body))
}

pub fn queue_snapshot_associations(body: String) -> Result<(), PersistenceError> {
    try_enqueue(PersistOp::SnapshotAssociations(body))
}

pub fn queue_snapshot_ask_jobs(body: String) -> Result<(), PersistenceError> {
    try_enqueue(PersistOp::SnapshotAskJobs(body))
}

pub fn queue_snapshot_turn_workers(body: String) -> Result<(), PersistenceError> {
    try_enqueue(PersistOp::SnapshotTurnWorkers(body))
}

async fn run_persist_writer(mut rx: mpsc::Receiver<PersistOp>) {
    let mut pending = PendingSnapshots::default();
    let mut last_failure: Option<String> = None;
    let debounce = Duration::from_millis(DEBOUNCE_MS);
    let mut debounce_sleep: Pin<Box<Sleep>> = Box::pin(time::sleep(debounce));
    debounce_sleep
        .as_mut()
        .reset(time::Instant::now() + debounce);

    loop {
        tokio::select! {
            message = rx.recv() => {
                let Some(op) = message else {
                    let _ = flush_pending_snapshots(&mut pending).await;
                    break;
                };
                match op {
                    PersistOp::AppendFeedLine(line) => {
                        if let Err(err) = append_feed_line(&line).await {
                            eprintln!("workspace persist: feed append failed: {err}");
                            last_failure = Some(err.to_string());
                        }
                    }
                    PersistOp::WriteRevision(revision) => {
                        if let Err(err) = write_revision(revision).await {
                            eprintln!("workspace persist: revision write failed: {err}");
                            last_failure = Some(err.to_string());
                        }
                    }
                    PersistOp::SnapshotCardStates(body) => {
                        pending.card_states = Some(body);
                        debounce_sleep.as_mut().reset(time::Instant::now() + debounce);
                    }
                    PersistOp::SnapshotAssociations(body) => {
                        pending.associations = Some(body);
                        debounce_sleep.as_mut().reset(time::Instant::now() + debounce);
                    }
                    PersistOp::SnapshotAskJobs(body) => {
                        pending.ask_jobs = Some(body);
                        debounce_sleep.as_mut().reset(time::Instant::now() + debounce);
                    }
                    PersistOp::SnapshotTurnWorkers(body) => {
                        pending.turn_workers = Some(body);
                        debounce_sleep.as_mut().reset(time::Instant::now() + debounce);
                    }
                    PersistOp::Flush(done) => {
                        let result = flush_pending_snapshots(&mut pending).await.and_then(|()| {
                            if let Some(message) = last_failure.take() {
                                Err(PersistenceError::new(PersistenceErrorKind::PermanentIo, message))
                            } else {
                                Ok(())
                            }
                        });
                        let _ = done.send(result);
                    }
                }
            }
            _ = &mut debounce_sleep, if pending.any() => {
                if let Err(error) = flush_pending_snapshots(&mut pending).await {
                    eprintln!("workspace persist: snapshot flush failed: {error}");
                    last_failure = Some(error.to_string());
                }
            }
        }
    }
}

async fn flush_pending_snapshots(pending: &mut PendingSnapshots) -> Result<(), PersistenceError> {
    let batch = pending.take_all();
    if let Some(body) = batch.card_states {
        write_file(CARD_STATE_FILE, &body).await.map_err(|error| {
            PersistenceError::new(PersistenceErrorKind::PermanentIo, error.to_string())
        })?;
    }
    if let Some(body) = batch.associations {
        write_file(ASSOC_FILE, &body).await.map_err(|error| {
            PersistenceError::new(PersistenceErrorKind::PermanentIo, error.to_string())
        })?;
    }
    if let Some(body) = batch.ask_jobs {
        write_file(ASK_JOBS_FILE, &body).await.map_err(|error| {
            PersistenceError::new(PersistenceErrorKind::PermanentIo, error.to_string())
        })?;
    }
    if let Some(body) = batch.turn_workers {
        write_file(TURN_WORKERS_FILE, &body)
            .await
            .map_err(|error| {
                PersistenceError::new(PersistenceErrorKind::PermanentIo, error.to_string())
            })?;
    }
    Ok(())
}

async fn append_feed_line(line: &str) -> std::io::Result<()> {
    ensure_workspace_dir().await?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(workspace_path(FEED_FILE))
        .await?;
    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    file.flush().await
}

async fn write_revision(revision: u64) -> std::io::Result<()> {
    ensure_workspace_dir().await?;
    fs::write(workspace_path(REVISION_FILE), revision.to_string()).await
}

async fn write_file(relative: &str, body: &str) -> std::io::Result<()> {
    ensure_workspace_dir().await?;
    fs::write(workspace_path(relative), body).await
}
