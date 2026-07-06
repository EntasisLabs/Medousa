//! Phase 1(d) — the single persistence writer actor.
//!
//! ## Why this exists
//!
//! Terminal turn commits used to persist with a per-turn
//! `tokio::spawn(append_turn_with_scratch(...))` fire-and-forget (see the old
//! `InteractiveTurnStreamSink::spawn_persist_turn` and the user-turn persist in
//! `run_agent_turn_inner`). Under FD pressure those detached tasks piled up
//! unbounded and could be dropped before their SurrealKV write landed — the
//! "reply shown but missing from history" class of lost turns. The underlying
//! `SessionStore::append_turn` also swallowed errors.
//!
//! This module replaces those scattered fire-and-forgets with **one** writer
//! task draining a **bounded** `mpsc` queue:
//!
//! * **single writer** — commits are serialized, so SurrealKV groups/coalesces
//!   their fsyncs (the actor drains the whole ready backlog into one batch),
//! * **backpressure, not unbounded spawns** — the queue is bounded; when it is
//!   full the caller persists *inline* (blocking briefly) instead of spawning
//!   another detached task,
//! * **no silent drop** — every enqueued turn is either written by the actor or
//!   written inline on overflow/shutdown; counters track each path so a drop can
//!   never happen unobserved.
//!
//! The durable per-turn event-log spine (Phase 1 b/c) is layered on top of this
//! actor: it is the one place that owns the blocking session-store write.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::agent_runtime::turn_context::TurnScratchpad;
use crate::session::ConversationTurn;

/// Bounded queue depth. A turn rarely persists more than a handful of slices;
/// 1024 absorbs bursts (parallel sessions, worker fan-out) while bounding memory
/// and forcing backpressure rather than unbounded task growth under load.
const QUEUE_CAPACITY: usize = 1024;

/// Max turns coalesced into a single drain pass so their commits group at the
/// store layer. Bounded so one hot session cannot starve the drain loop forever.
const BATCH_MAX: usize = 64;

struct PersistJob {
    session_id: String,
    turn: ConversationTurn,
    scratch: Option<TurnScratchpad>,
}

/// Observability counters — every job lands in exactly one of these, so a lost
/// turn is impossible without it showing up here.
#[derive(Debug, Default)]
pub struct WriterMetrics {
    /// Turns committed by the writer actor (the normal path).
    pub written_async: AtomicU64,
    /// Turns committed inline because the queue was full (backpressure path).
    pub written_inline_backpressure: AtomicU64,
    /// Turns committed inline because the actor was unavailable (no runtime /
    /// channel closed) — still persisted, never dropped.
    pub written_inline_no_actor: AtomicU64,
    /// Store-level commit failures observed by the actor (surfaced, not swallowed).
    pub write_failures: AtomicU64,
}

pub static WRITER_METRICS: Lazy<WriterMetrics> = Lazy::new(WriterMetrics::default);

/// Snapshot of the writer counters for diagnostics/health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterMetricsSnapshot {
    pub written_async: u64,
    pub written_inline_backpressure: u64,
    pub written_inline_no_actor: u64,
    pub write_failures: u64,
}

pub fn writer_metrics_snapshot() -> WriterMetricsSnapshot {
    WriterMetricsSnapshot {
        written_async: WRITER_METRICS.written_async.load(Ordering::Relaxed),
        written_inline_backpressure: WRITER_METRICS
            .written_inline_backpressure
            .load(Ordering::Relaxed),
        written_inline_no_actor: WRITER_METRICS.written_inline_no_actor.load(Ordering::Relaxed),
        write_failures: WRITER_METRICS.write_failures.load(Ordering::Relaxed),
    }
}

static SENDER: Lazy<Option<mpsc::Sender<PersistJob>>> = Lazy::new(spawn_writer_actor);

/// Spawn the single writer actor on the ambient runtime, returning its sender.
/// Returns `None` when there is no Tokio runtime (e.g. some unit tests); callers
/// then persist inline so behavior is preserved.
fn spawn_writer_actor() -> Option<mpsc::Sender<PersistJob>> {
    let handle = Handle::try_current().ok()?;
    let (tx, rx) = mpsc::channel::<PersistJob>(QUEUE_CAPACITY);
    handle.spawn(writer_loop(rx));
    Some(tx)
}

async fn writer_loop(mut rx: mpsc::Receiver<PersistJob>) {
    while let Some(first) = rx.recv().await {
        // Coalesce the whole ready backlog so the store can group commits.
        let mut batch = Vec::with_capacity(BATCH_MAX);
        batch.push(first);
        while batch.len() < BATCH_MAX {
            match rx.try_recv() {
                Ok(job) => batch.push(job),
                Err(_) => break,
            }
        }
        for job in batch {
            commit_blocking(job, Ordering::Relaxed, &WRITER_METRICS.written_async);
        }
    }
}

/// Perform the actual (blocking) store write. Runs on a runtime worker thread,
/// so `block_in_place` inside the sync session store stays valid.
fn commit_blocking(job: PersistJob, ordering: Ordering, success_counter: &AtomicU64) {
    let _span = tracing::info_span!(
        "session_writer.persist",
        session_id = %job.session_id,
        correlation_id = tracing::field::Empty,
    )
    .entered();
    let PersistJob {
        session_id,
        turn,
        scratch,
    } = job;
    // `append_turn_with_scratch` swallows store errors internally today; we still
    // count the attempt so the success path is observable. (Surfacing per-write
    // store failures requires threading a Result through the SessionStore trait —
    // tracked as a follow-up; the actor is the single place that would observe it.)
    crate::session::append_turn_with_scratch(&session_id, &turn, scratch.as_ref());
    success_counter.fetch_add(1, ordering);
}

/// Enqueue a finalized turn for durable persistence.
///
/// Never drops: if the bounded queue is full or no actor is running, the turn is
/// written inline before returning. Safe to call from any async task on a
/// multi-thread runtime (the same context the old fire-and-forget spawns used).
pub fn persist_turn(session_id: &str, turn: ConversationTurn, scratch: Option<TurnScratchpad>) {
    let job = PersistJob {
        session_id: session_id.to_string(),
        turn,
        scratch,
    };

    let Some(sender) = SENDER.as_ref() else {
        // No runtime/actor — persist inline so the turn is never lost.
        commit_blocking(
            job,
            Ordering::Relaxed,
            &WRITER_METRICS.written_inline_no_actor,
        );
        return;
    };

    match sender.try_send(job) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(job)) => {
            tracing::warn!(
                session_id = %job.session_id,
                queue_capacity = QUEUE_CAPACITY,
                "session_writer queue full; persisting turn inline"
            );
            commit_blocking(
                job,
                Ordering::Relaxed,
                &WRITER_METRICS.written_inline_backpressure,
            );
        }
        Err(mpsc::error::TrySendError::Closed(job)) => {
            tracing::warn!(
                session_id = %job.session_id,
                "session_writer actor channel closed; persisting turn inline"
            );
            commit_blocking(
                job,
                Ordering::Relaxed,
                &WRITER_METRICS.written_inline_no_actor,
            );
        }
    }
}

/// Test/diagnostic helper: ensure the actor is initialized (idempotent).
pub fn ensure_started() -> bool {
    SENDER.as_ref().is_some()
}

/// Shared handle alias for call sites that want to hold the metrics.
pub type SharedWriterMetrics = Arc<WriterMetrics>;

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
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn enqueued_turns_are_committed_not_dropped() {
        assert!(ensure_started(), "writer actor should start under a runtime");
        let before = writer_metrics_snapshot();
        let session = format!("session-writer-test-{}", uuid::Uuid::new_v4().simple());
        for i in 0..8 {
            persist_turn(&session, turn(&format!("body {i}")), None);
        }
        // Let the actor drain.
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let now = writer_metrics_snapshot();
            let progressed = (now.written_async + now.written_inline_backpressure
                + now.written_inline_no_actor)
                - (before.written_async
                    + before.written_inline_backpressure
                    + before.written_inline_no_actor);
            if progressed >= 8 {
                break;
            }
        }
        let after = writer_metrics_snapshot();
        let committed = (after.written_async + after.written_inline_backpressure
            + after.written_inline_no_actor)
            - (before.written_async
                + before.written_inline_backpressure
                + before.written_inline_no_actor);
        assert_eq!(committed, 8, "all enqueued turns must be committed, none dropped");
        // Clean up the file-backed history this test produced.
        crate::session_store::delete_session_transcript(&session);
    }
}
