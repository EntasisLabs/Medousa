//! Shared daemon turn-pipeline output backed by the canonical turn journal.

use std::sync::{Arc, OnceLock};

use medousa_engine::{TurnEventLog, TurnPipelineEmission, TurnPipelineError, TurnPipelineOutput};
use tokio::sync::Semaphore;

use crate::turn_event_channel::TurnEventChannel;

const GLOBAL_TURN_PIPELINE_BYTES: usize = 64 * 1024 * 1024;

/// Process-wide backpressure shared by every foreground daemon deployment.
pub fn daemon_turn_pipeline_budget() -> Arc<Semaphore> {
    static BUDGET: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(BUDGET.get_or_init(|| Arc::new(Semaphore::new(GLOBAL_TURN_PIPELINE_BYTES))))
}

/// Publishes one sequenced turn pipeline to both durable replay and live clients.
///
/// Every daemon deployment uses this adapter so journal sequence, terminal
/// commit fences, and wire projections have one implementation.
pub struct TurnJournalOutput {
    stream_tx: Arc<TurnEventChannel>,
    event_log: Arc<TurnEventLog>,
}

impl TurnJournalOutput {
    pub fn new(stream_tx: Arc<TurnEventChannel>, event_log: Arc<TurnEventLog>) -> Self {
        Self {
            stream_tx,
            event_log,
        }
    }
}

impl TurnPipelineOutput for TurnJournalOutput {
    async fn publish(&self, emission: TurnPipelineEmission) -> Result<(), TurnPipelineError> {
        let mut wire = crate::sse_turn_projection::v2_to_v1(&emission.envelope);
        let v2 = emission.envelope;
        let journal = emission
            .journal_override
            .unwrap_or_else(|| crate::sse_turn_projection::journal_turn_event_for_v2(&v2));
        let event_log = Arc::clone(&self.event_log);
        let seq = v2.seq;
        let terminal = v2.event.is_terminal();
        let emitted_at_utc = v2.emitted_at_utc;
        let stream_event_v2 = crate::sse_turn_projection::frozen_v2_replay_event(&v2.event);
        let receipt = tokio::task::spawn_blocking(move || {
            let receipt = event_log.append_sequenced_with_stream_v2(
                seq,
                journal,
                Some(emitted_at_utc),
                stream_event_v2,
            )?;
            if terminal {
                let commit = event_log.mark_committed()?;
                if commit.through_seq != receipt.seq() {
                    return Err(std::io::Error::other(format!(
                        "journal commit fence {} diverged from terminal sequence {}",
                        commit.through_seq,
                        receipt.seq()
                    )));
                }
            }
            Ok::<_, std::io::Error>(receipt)
        })
        .await
        .map_err(|error| TurnPipelineError::Output(format!("journal writer stopped: {error}")))?
        .map_err(|error| TurnPipelineError::Output(format!("journal append failed: {error}")))?;
        wire.seq = receipt.seq();
        self.stream_tx.publish_pair(wire, v2);
        Ok(())
    }
}
