//! Shared daemon turn-pipeline output backed by the canonical turn journal.

use std::sync::{Arc, OnceLock};

use medousa_engine::{
    TurnEventLog, TurnPipelineEmission, TurnPipelineEnvelope, TurnPipelineError, TurnPipelineOutput,
};
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
        match emission.envelope {
            TurnPipelineEnvelope::V2(v2) => self.publish_v2(v2, emission.journal_override).await,
            TurnPipelineEnvelope::V3(v3) => self.publish_v3(v3, emission.journal_override).await,
        }
    }
}

impl TurnJournalOutput {
    async fn publish_v2(
        &self,
        v2: medousa_types::TurnStreamEnvelopeV2,
        journal_override: Option<medousa_engine::TurnEvent>,
    ) -> Result<(), TurnPipelineError> {
        let mut wire = crate::sse_turn_projection::v2_to_v1(&v2);
        let journal = journal_override
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

    async fn publish_v3(
        &self,
        v3: medousa_types::TurnStreamEnvelopeV3,
        journal_override: Option<medousa_engine::TurnEvent>,
    ) -> Result<(), TurnPipelineError> {
        let v2 = crate::sse_turn_projection::v3_to_v2(&v3).map_err(TurnPipelineError::Output)?;
        let journal = journal_override
            .unwrap_or_else(|| crate::sse_turn_projection::journal_turn_event_for_v3(&v3));
        let event_log = Arc::clone(&self.event_log);
        let seq = v3.seq;
        let terminal = v3.event.is_terminal();
        let emitted_at_utc = v3.emitted_at_utc;
        let stream_event_v3 = v3.event.clone();
        let receipt = tokio::task::spawn_blocking(move || {
            let receipt = event_log.append_sequenced_with_stream_v3(
                seq,
                journal,
                Some(emitted_at_utc),
                stream_event_v3,
                None,
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
        debug_assert_eq!(receipt.seq(), v3.seq);
        self.stream_tx.publish_v3(v3, v2);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use medousa_engine::{Principal, TurnEnvelope};
    use medousa_types::{TurnStreamEnvelopeV3, TurnStreamEventV3};

    use super::*;

    #[tokio::test]
    async fn native_v3_facts_are_journaled_and_published_once_with_optional_v2() {
        let root = std::env::temp_dir().join(format!(
            "medousa-v3-output-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let log = Arc::new(
            TurnEventLog::open_in(
                &root,
                TurnEnvelope::new("turn-native-v3", Principal::operator()),
            )
            .unwrap(),
        );
        let channel = TurnEventChannel::new(8);
        let mut subscriber = channel.try_subscribe().unwrap();
        let output = TurnJournalOutput::new(Arc::clone(&channel), Arc::clone(&log));
        let envelope = TurnStreamEnvelopeV3::new(
            "turn-native-v3",
            1,
            chrono::Utc::now(),
            TurnStreamEventV3::AssistantTextStarted {
                segment_id: "segment-1".into(),
                model_round: 1,
            },
        )
        .unwrap();

        output
            .publish(TurnPipelineEmission {
                envelope: TurnPipelineEnvelope::V3(envelope),
                journal_override: None,
            })
            .await
            .unwrap();

        let published = subscriber.recv().await.unwrap();
        assert_eq!(published.seq(), 1);
        assert!(published.v1.is_none());
        assert!(published.v2.is_none());
        assert!(matches!(
            published.v3.as_ref().map(|envelope| &envelope.event),
            Some(TurnStreamEventV3::AssistantTextStarted { segment_id, .. })
                if segment_id == "segment-1"
        ));

        let append = TurnStreamEnvelopeV3::new(
            "turn-native-v3",
            2,
            chrono::Utc::now(),
            TurnStreamEventV3::ContentAppend {
                segment_id: "segment-1".into(),
                text: "visible".into(),
            },
        )
        .unwrap();
        output
            .publish(TurnPipelineEmission {
                envelope: TurnPipelineEnvelope::V3(append),
                journal_override: None,
            })
            .await
            .unwrap();
        let published = subscriber.recv().await.unwrap();
        assert_eq!(published.seq(), 2);
        assert!(published.v1.is_some());
        assert!(published.v2.is_some());
        assert!(published.v3.is_some());

        let replay = log.snapshot_since(0);
        assert_eq!(replay.len(), 2);
        assert!(replay[0].stream_event_v2.is_none());
        assert!(replay[0].stream_event_v3.is_some());
        assert!(replay[1].stream_event_v2.is_none());
        assert!(replay[1].stream_event_v3.is_some());

        drop(output);
        drop(subscriber);
        drop(channel);
        drop(log);
        std::fs::remove_dir_all(root).ok();
    }
}
