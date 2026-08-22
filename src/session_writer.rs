//! Bounded, acknowledged session-turn persistence.
//!
//! Producers await queue admission and an explicit store receipt. There is no
//! blocking overflow fallback and no successful acknowledgement before the
//! backing store has accepted the complete batch.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use once_cell::sync::Lazy;
use tokio::runtime::Handle;
use tokio::sync::{Semaphore, mpsc, oneshot};

use crate::agent_runtime::turn_context::TurnScratchpad;
use crate::session::ConversationTurn;
use crate::session_store::{CommitReceipt, StoreError};
use medousa_types::session::ExecutionRef;

const QUEUE_CAPACITY: usize = 1024;
const QUEUE_BYTE_CAPACITY: usize = 8 * 1024 * 1024;
const BATCH_MAX: usize = 64;

struct PersistJob {
    session_id: String,
    turn: ConversationTurn,
    scratch: Option<TurnScratchpad>,
    caused_by: Option<ExecutionRef>,
    _byte_permit: tokio::sync::OwnedSemaphorePermit,
    ack: oneshot::Sender<Result<CommitReceipt, StoreError>>,
}

enum WriterMessage {
    Persist(Box<PersistJob>),
    Drain(oneshot::Sender<()>),
}

#[derive(Debug, Default)]
pub struct WriterMetrics {
    pub committed_turns: AtomicU64,
    pub commit_batches: AtomicU64,
    pub write_failures: AtomicU64,
    pub queued_messages: AtomicUsize,
    pub queued_bytes: AtomicUsize,
    pub message_high_water: AtomicUsize,
    pub byte_high_water: AtomicUsize,
}

pub static WRITER_METRICS: Lazy<WriterMetrics> = Lazy::new(WriterMetrics::default);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterMetricsSnapshot {
    pub committed_turns: u64,
    pub commit_batches: u64,
    pub write_failures: u64,
    pub queued_messages: usize,
    pub queued_bytes: usize,
    pub message_high_water: usize,
    pub byte_high_water: usize,
}

pub fn writer_metrics_snapshot() -> WriterMetricsSnapshot {
    WriterMetricsSnapshot {
        committed_turns: WRITER_METRICS.committed_turns.load(Ordering::Relaxed),
        commit_batches: WRITER_METRICS.commit_batches.load(Ordering::Relaxed),
        write_failures: WRITER_METRICS.write_failures.load(Ordering::Relaxed),
        queued_messages: WRITER_METRICS.queued_messages.load(Ordering::Relaxed),
        queued_bytes: WRITER_METRICS.queued_bytes.load(Ordering::Relaxed),
        message_high_water: WRITER_METRICS.message_high_water.load(Ordering::Relaxed),
        byte_high_water: WRITER_METRICS.byte_high_water.load(Ordering::Relaxed),
    }
}

static BYTE_BUDGET: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(QUEUE_BYTE_CAPACITY)));
static SENDER: Lazy<Mutex<Option<mpsc::Sender<WriterMessage>>>> = Lazy::new(|| Mutex::new(None));

fn writer_sender() -> Result<mpsc::Sender<WriterMessage>, StoreError> {
    let mut slot = SENDER
        .lock()
        .map_err(|_| StoreError::Worker("session writer registry poisoned".to_string()))?;
    if let Some(sender) = slot.as_ref()
        && !sender.is_closed()
    {
        return Ok(sender.clone());
    }
    let handle = Handle::try_current()
        .map_err(|_| StoreError::Worker("session writer requires a Tokio runtime".to_string()))?;
    let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
    handle.spawn(writer_loop(rx));
    *slot = Some(tx.clone());
    Ok(tx)
}

async fn writer_loop(mut rx: mpsc::Receiver<WriterMessage>) {
    while let Some(message) = rx.recv().await {
        match message {
            WriterMessage::Drain(ack) => {
                let _ = ack.send(());
            }
            WriterMessage::Persist(first) => {
                let mut batch = Vec::with_capacity(BATCH_MAX);
                batch.push(*first);
                while batch.len() < BATCH_MAX {
                    match rx.try_recv() {
                        Ok(WriterMessage::Persist(job)) => batch.push(*job),
                        Ok(WriterMessage::Drain(ack)) => {
                            commit_jobs(batch).await;
                            let _ = ack.send(());
                            batch = Vec::new();
                            break;
                        }
                        Err(_) => break,
                    }
                }
                if !batch.is_empty() {
                    commit_jobs(batch).await;
                }
            }
        }
    }
}

async fn commit_jobs(mut jobs: Vec<PersistJob>) {
    while !jobs.is_empty() {
        let session_id = jobs[0].session_id.clone();
        let run_len = jobs
            .iter()
            .take_while(|job| job.session_id == session_id)
            .count();
        let run = jobs.drain(..run_len).collect::<Vec<_>>();
        let turns = run
            .iter()
            .map(|job| (job.turn.clone(), job.scratch.clone(), job.caused_by.clone()))
            .collect::<Vec<_>>();
        let result =
            crate::session::try_append_transcript_batch_with_scratch(&session_id, &turns).await;
        WRITER_METRICS
            .queued_messages
            .fetch_sub(run.len(), Ordering::Relaxed);
        let released_bytes = run
            .iter()
            .map(|job| job._byte_permit.num_permits())
            .sum::<usize>();
        WRITER_METRICS
            .queued_bytes
            .fetch_sub(released_bytes, Ordering::Relaxed);

        match &result {
            Ok(receipt) => {
                WRITER_METRICS
                    .committed_turns
                    .fetch_add(receipt.turns as u64, Ordering::Relaxed);
                WRITER_METRICS
                    .commit_batches
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                WRITER_METRICS
                    .write_failures
                    .fetch_add(1, Ordering::Relaxed);
                tracing::error!(session_id, %error, turns = run.len(), "session turn batch failed");
            }
        }
        for job in run {
            let _ = job.ack.send(result.clone());
        }
    }
}

fn estimated_job_bytes(turn: &ConversationTurn, scratch: Option<&TurnScratchpad>) -> usize {
    let turn_bytes = serde_json::to_vec(turn).map_or(1, |bytes| bytes.len());
    let scratch_bytes = scratch
        .and_then(|value| serde_json::to_vec(value).ok())
        .map_or(0, |bytes| bytes.len());
    turn_bytes.saturating_add(scratch_bytes).max(1)
}

fn record_queue_admission(bytes: usize) {
    let messages = WRITER_METRICS
        .queued_messages
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    let queued_bytes = WRITER_METRICS
        .queued_bytes
        .fetch_add(bytes, Ordering::Relaxed)
        + bytes;
    WRITER_METRICS
        .message_high_water
        .fetch_max(messages, Ordering::Relaxed);
    WRITER_METRICS
        .byte_high_water
        .fetch_max(queued_bytes, Ordering::Relaxed);
}

pub async fn persist_turn(
    session_id: &str,
    turn: ConversationTurn,
    scratch: Option<TurnScratchpad>,
) -> Result<CommitReceipt, StoreError> {
    persist_turn_with_execution(session_id, turn, scratch, None).await
}

pub async fn persist_turn_with_execution(
    session_id: &str,
    turn: ConversationTurn,
    scratch: Option<TurnScratchpad>,
    caused_by: Option<ExecutionRef>,
) -> Result<CommitReceipt, StoreError> {
    let sender = writer_sender()?;
    let byte_count = estimated_job_bytes(&turn, scratch.as_ref());
    if byte_count > QUEUE_BYTE_CAPACITY {
        return Err(StoreError::InvalidInput(format!(
            "session turn requires {byte_count} queue bytes; limit is {QUEUE_BYTE_CAPACITY}"
        )));
    }
    let byte_permit = Arc::clone(&BYTE_BUDGET)
        .acquire_many_owned(byte_count as u32)
        .await
        .map_err(|_| StoreError::Worker("session writer byte budget closed".to_string()))?;
    let (ack, receipt) = oneshot::channel();
    record_queue_admission(byte_count);
    if sender
        .send(WriterMessage::Persist(Box::new(PersistJob {
            session_id: session_id.to_string(),
            turn,
            scratch,
            caused_by,
            _byte_permit: byte_permit,
            ack,
        })))
        .await
        .is_err()
    {
        WRITER_METRICS
            .queued_messages
            .fetch_sub(1, Ordering::Relaxed);
        WRITER_METRICS
            .queued_bytes
            .fetch_sub(byte_count, Ordering::Relaxed);
        return Err(StoreError::Worker(
            "session writer stopped before queue admission".to_string(),
        ));
    }
    receipt.await.map_err(|_| {
        StoreError::Worker("session writer stopped before commit receipt".to_string())
    })?
}

pub async fn drain(deadline: Duration) -> Result<(), StoreError> {
    let sender = writer_sender()?;
    let (ack, drained) = oneshot::channel();
    sender
        .send(WriterMessage::Drain(ack))
        .await
        .map_err(|_| StoreError::Worker("session writer stopped before drain".to_string()))?;
    tokio::time::timeout(deadline, drained)
        .await
        .map_err(|_| StoreError::Worker("session writer drain deadline elapsed".to_string()))?
        .map_err(|_| StoreError::Worker("session writer stopped during drain".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn turn(body: &str) -> ConversationTurn {
        ConversationTurn {
            role: "assistant".to_string(),
            content: body.to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn every_accepted_turn_receives_a_commit_receipt() {
        let session = format!("session-writer-test-{}", uuid::Uuid::new_v4().simple());
        let mut receipts = Vec::new();
        for i in 0..8 {
            receipts.push(persist_turn(&session, turn(&format!("body {i}")), None).await);
        }
        assert!(receipts.iter().all(Result::is_ok));
        drain(Duration::from_secs(1)).await.unwrap();
        let session_id = crate::session_storage::SessionId::parse(&session).unwrap();
        let history = crate::session::load_history(&session);
        assert_eq!(history.len(), 8);
        crate::session_store::delete_session_transcript(&session_id).unwrap();
    }
}
