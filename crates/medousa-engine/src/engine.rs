//! Phase 1(f) — the engine `run_turn` API surface.
//!
//! The plan's target shape is:
//!
//! ```text
//! daemon: engine.run_turn(envelope) -> TurnHandle { events, outcome }
//! ```
//!
//! so the daemon stops owning turn orchestration (ticket + stream registry +
//! 30s grace inline in `daemon/interactive.rs`) and instead drives turns through
//! one engine entrypoint that takes a [`TurnEnvelope`] and returns a handle over
//! the durable event-log projection plus the turn outcome.
//!
//! This module defines that public shape — [`EngineTurnHandle`] and
//! [`TurnRunOutcome`] — on top of the [`TurnEventLog`] spine, so later phases
//! (and the daemon adapter) can adopt it. The full move of ticket/registry/grace
//! lifecycle out of `daemon/interactive.rs` into a concrete `run_turn` body is
//! the Phase 1 remainder (the orchestration code lives behind
//! `run_daemon_interactive_turn` today and is large to relocate behavior-preserving
//! in one pass); see the worker handoff notes.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use super::ports::{TurnStreamRegistryPort, TurnTicketPort};
use super::turn_event::{SequencedTurnEvent, TurnEvent, TurnEnvelope};
use super::turn_event_log::TurnEventLog;

/// Lifecycle ports the engine needs to orchestrate a daemon-hosted turn.
pub struct TurnLifecyclePorts {
    pub tickets: Arc<dyn TurnTicketPort>,
    pub streams: Arc<dyn TurnStreamRegistryPort>,
}

/// Run the daemon turn lifecycle around an executor closure:
/// brief subscribe grace, turn body, stream close, ticket clear, 30s replay grace.
pub async fn run_turn<F, Fut>(ports: TurnLifecyclePorts, envelope: TurnEnvelope, turn: F) -> EngineTurnHandle
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let turn_id = envelope.turn_id.clone();
    tokio::time::sleep(Duration::from_millis(25)).await;
    turn().await;

    ports.streams.mark_stream_closed(&turn_id).await;
    ports.tickets.clear_after_run(&turn_id).await;

    let log = ports
        .streams
        .event_log(&turn_id)
        .await
        .unwrap_or_else(|| {
            Arc::new(
                TurnEventLog::open(envelope.clone())
                    .unwrap_or_else(|_| panic!("turn log unavailable for {turn_id}")),
            )
        });
    let events = log.snapshot_since(0);
    let outcome = TurnRunOutcome::from_events(&events);
    if outcome.is_terminal() {
        log.mark_committed();
    }

    tokio::time::sleep(Duration::from_secs(30)).await;
    ports.streams.drop_stream(&turn_id).await;

    EngineTurnHandle::new(envelope, log, outcome)
}

/// Terminal classification of a completed (or handed-off) turn.
///
/// Mirrors the existing terminal sink methods + `turn_ticket` phases so the
/// daemon adapter can map it back to ticket state without re-deriving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnRunOutcome {
    /// Final answer committed (`agent_response`).
    Completed,
    /// Terminal clarifying question / pivot (`agent_needs_input`).
    NeedsInput,
    /// Mid-task handoff back to the principal (`agent_turn_checkpoint`).
    Checkpoint,
    /// Background worker spawned; host turn ended non-terminally (`agent_worker_ack`).
    WorkerHandoff,
    /// Turn failed (`agent_error`).
    Failed,
    /// Turn cancelled by the operator.
    Cancelled,
    /// No terminal event was observed (e.g. crash mid-turn before commit).
    Incomplete,
}

impl TurnRunOutcome {
    /// Derive the outcome from the final event the engine emitted, matching the
    /// terminality rules in [`TurnEvent::is_terminal`] and the worker-ack handoff.
    pub fn from_events(events: &[SequencedTurnEvent]) -> Self {
        // Prefer the last terminal/handoff signal observed.
        for sequenced in events.iter().rev() {
            match &sequenced.event {
                TurnEvent::FinalResponse { .. } => return TurnRunOutcome::Completed,
                TurnEvent::NeedsInput { .. } => return TurnRunOutcome::NeedsInput,
                TurnEvent::Checkpoint { .. } => return TurnRunOutcome::Checkpoint,
                TurnEvent::WorkerAck { .. } => return TurnRunOutcome::WorkerHandoff,
                TurnEvent::Error { .. } => return TurnRunOutcome::Failed,
                _ => continue,
            }
        }
        TurnRunOutcome::Incomplete
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TurnRunOutcome::Completed
                | TurnRunOutcome::NeedsInput
                | TurnRunOutcome::Failed
                | TurnRunOutcome::Cancelled
        )
    }
}

/// Handle returned by the engine for a run turn: the event projection source
/// (the durable spine) plus the resolved outcome.
///
/// `events` is exposed as the [`TurnEventLog`] so callers use its
/// `snapshot_since(seq)` projection for SSE replay and `fold_history()` for
/// history — exactly the two independent folds the plan specifies.
pub struct EngineTurnHandle {
    pub envelope: TurnEnvelope,
    pub log: Arc<TurnEventLog>,
    pub outcome: TurnRunOutcome,
}

impl EngineTurnHandle {
    pub fn new(envelope: TurnEnvelope, log: Arc<TurnEventLog>, outcome: TurnRunOutcome) -> Self {
        Self {
            envelope,
            log,
            outcome,
        }
    }

    /// SSE replay projection passthrough.
    pub fn events_since(&self, since: u64) -> Vec<SequencedTurnEvent> {
        self.log.snapshot_since(since)
    }
}

#[cfg(test)]
mod tests {
    use super::super::turn_event::Principal;
    use super::*;

    fn seq(event: TurnEvent, n: u64) -> SequencedTurnEvent {
        SequencedTurnEvent {
            envelope: TurnEnvelope::new("t", Principal::operator()).at_seq(n),
            event,
        }
    }

    #[test]
    fn outcome_from_events_prefers_terminal_signal() {
        let events = vec![
            seq(TurnEvent::ContentDelta { delta: "x".into() }, 1),
            seq(
                TurnEvent::FinalResponse {
                    text: "done".into(),
                    tool_names: vec![],
                    parts: vec![],
                    committed_at: chrono::Utc::now(),
                },
                2,
            ),
        ];
        assert_eq!(TurnRunOutcome::from_events(&events), TurnRunOutcome::Completed);
    }

    #[test]
    fn outcome_worker_ack_is_handoff_not_terminal() {
        let events = vec![seq(
            TurnEvent::WorkerAck {
                text: "on it".into(),
                tool_names: vec![],
                work_id: Some("w".into()),
                parts: vec![],
                committed_at: chrono::Utc::now(),
            },
            1,
        )];
        let outcome = TurnRunOutcome::from_events(&events);
        assert_eq!(outcome, TurnRunOutcome::WorkerHandoff);
        assert!(!outcome.is_terminal());
    }

    #[test]
    fn outcome_incomplete_when_no_terminal() {
        let events = vec![seq(TurnEvent::ContentDelta { delta: "x".into() }, 1)];
        assert_eq!(TurnRunOutcome::from_events(&events), TurnRunOutcome::Incomplete);
    }
}
