//! Async workspace persistence — single writer task, debounced snapshot files.

use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use once_cell::sync::OnceCell;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Sleep};

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
    Flush(oneshot::Sender<()>),
}

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

impl Default for PendingSnapshots {
    fn default() -> Self {
        Self {
            card_states: None,
            associations: None,
            ask_jobs: None,
            turn_workers: None,
        }
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

pub async fn flush_persist_writer() {
    let Some(tx) = WRITER_TX.get() else {
        return;
    };
    let (done, rx) = oneshot::channel();
    if tx.send(PersistOp::Flush(done)).await.is_err() {
        return;
    }
    let _ = rx.await;
}

fn try_enqueue(op: PersistOp) {
    let Some(tx) = WRITER_TX.get() else {
        apply_sync_fallback(op);
        return;
    };
    match tx.try_send(op) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(op) | mpsc::error::TrySendError::Closed(op)) => {
            apply_sync_fallback(op);
        }
    }
}

pub fn queue_append_feed_line(line: String) {
    try_enqueue(PersistOp::AppendFeedLine(line));
}

pub fn queue_write_revision(revision: u64) {
    try_enqueue(PersistOp::WriteRevision(revision));
}

pub fn queue_snapshot_card_states(body: String) {
    try_enqueue(PersistOp::SnapshotCardStates(body));
}

pub fn queue_snapshot_associations(body: String) {
    try_enqueue(PersistOp::SnapshotAssociations(body));
}

pub fn queue_snapshot_ask_jobs(body: String) {
    try_enqueue(PersistOp::SnapshotAskJobs(body));
}

pub fn queue_snapshot_turn_workers(body: String) {
    try_enqueue(PersistOp::SnapshotTurnWorkers(body));
}

async fn run_persist_writer(mut rx: mpsc::Receiver<PersistOp>) {
    let mut pending = PendingSnapshots::default();
    let debounce = Duration::from_millis(DEBOUNCE_MS);
    let mut debounce_sleep: Pin<Box<Sleep>> = Box::pin(time::sleep(debounce));
    debounce_sleep.as_mut().reset(time::Instant::now() + debounce);

    loop {
        tokio::select! {
            message = rx.recv() => {
                let Some(op) = message else {
                    flush_pending_snapshots(&mut pending).await;
                    break;
                };
                match op {
                    PersistOp::AppendFeedLine(line) => {
                        if let Err(err) = append_feed_line(&line).await {
                            eprintln!("workspace persist: feed append failed: {err}");
                        }
                    }
                    PersistOp::WriteRevision(revision) => {
                        if let Err(err) = write_revision(revision).await {
                            eprintln!("workspace persist: revision write failed: {err}");
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
                        flush_pending_snapshots(&mut pending).await;
                        let _ = done.send(());
                    }
                }
            }
            _ = &mut debounce_sleep, if pending.any() => {
                flush_pending_snapshots(&mut pending).await;
            }
        }
    }
}

async fn flush_pending_snapshots(pending: &mut PendingSnapshots) {
    let batch = pending.take_all();
    if let Some(body) = batch.card_states {
        if let Err(err) = write_file(CARD_STATE_FILE, &body).await {
            eprintln!("workspace persist: card_states write failed: {err}");
        }
    }
    if let Some(body) = batch.associations {
        if let Err(err) = write_file(ASSOC_FILE, &body).await {
            eprintln!("workspace persist: associations write failed: {err}");
        }
    }
    if let Some(body) = batch.ask_jobs {
        if let Err(err) = write_file(ASK_JOBS_FILE, &body).await {
            eprintln!("workspace persist: ask_jobs write failed: {err}");
        }
    }
    if let Some(body) = batch.turn_workers {
        if let Err(err) = write_file(TURN_WORKERS_FILE, &body).await {
            eprintln!("workspace persist: turn_workers write failed: {err}");
        }
    }
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

fn apply_sync_fallback(op: PersistOp) {
    match op {
        PersistOp::AppendFeedLine(line) => {
            let _ = std::fs::create_dir_all(workspace_dir());
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(workspace_path(FEED_FILE))
            {
                use std::io::Write;
                let _ = writeln!(file, "{line}");
            }
        }
        PersistOp::WriteRevision(revision) => {
            let _ = std::fs::create_dir_all(workspace_dir());
            let _ = std::fs::write(workspace_path(REVISION_FILE), revision.to_string());
        }
        PersistOp::SnapshotCardStates(body) => {
            let _ = std::fs::create_dir_all(workspace_dir());
            let _ = std::fs::write(workspace_path(CARD_STATE_FILE), body);
        }
        PersistOp::SnapshotAssociations(body) => {
            let _ = std::fs::create_dir_all(workspace_dir());
            let _ = std::fs::write(workspace_path(ASSOC_FILE), body);
        }
        PersistOp::SnapshotAskJobs(body) => {
            let _ = std::fs::create_dir_all(workspace_dir());
            let _ = std::fs::write(workspace_path(ASK_JOBS_FILE), body);
        }
        PersistOp::SnapshotTurnWorkers(body) => {
            let _ = std::fs::create_dir_all(workspace_dir());
            let _ = std::fs::write(workspace_path(TURN_WORKERS_FILE), body);
        }
        PersistOp::Flush(done) => {
            let _ = done.send(());
        }
    }
}
