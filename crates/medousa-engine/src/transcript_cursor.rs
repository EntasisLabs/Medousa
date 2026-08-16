//! H03 transcript cursor contract used by Coder logical checkpoints (H06.4 / H06.10).
//!
//! The digest covers only the durable journal prefix through the cursor fence.
//! Later appends must not invalidate a previously published cursor, and
//! reconstruction returns exactly that fenced prefix.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::turn_event::SequencedTurnEvent;
use crate::turn_event_log::TurnEventLog;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptCursor {
    pub turn_id: String,
    pub journal_seq: u64,
    pub fence: u64,
    pub digest: String,
}

impl TranscriptCursor {
    /// Capture a cursor over the durable journal prefix through the current fence.
    pub fn from_log(log: &TurnEventLog) -> Self {
        let fence = log.replay_fence();
        Self::from_log_through(log, fence)
    }

    /// Capture a cursor over the durable journal prefix through an explicit fence.
    pub fn from_log_through(log: &TurnEventLog, fence: u64) -> Self {
        let fence = fence.min(log.replay_fence());
        let events = log.snapshot_through(fence);
        Self {
            turn_id: log.envelope().turn_id.clone(),
            journal_seq: fence,
            fence,
            digest: digest_events(&events),
        }
    }

    /// Verify that the referenced H03 prefix is still available and unchanged.
    ///
    /// Later events beyond `self.fence` are ignored — verification remains valid
    /// after the journal advances.
    pub fn verify(&self, log: &TurnEventLog) -> Result<(), String> {
        if log.envelope().turn_id != self.turn_id {
            return Err("transcript cursor turn_id does not match journal".into());
        }
        let available = log.replay_fence();
        if available < self.fence {
            return Err("referenced H03 prefix is unavailable".into());
        }
        let events = log.snapshot_through(self.fence);
        let digest = digest_events(&events);
        if digest != self.digest {
            return Err("referenced H03 prefix digest mismatch".into());
        }
        Ok(())
    }
}

pub fn digest_events(events: &[SequencedTurnEvent]) -> String {
    let mut hasher = Sha256::new();
    for event in events {
        let encoded = serde_json::to_vec(event).unwrap_or_default();
        hasher.update((encoded.len() as u64).to_le_bytes());
        hasher.update(&encoded);
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub fn reconstruct_from_journal(
    log: &TurnEventLog,
    cursor: &TranscriptCursor,
) -> Result<Vec<SequencedTurnEvent>, String> {
    cursor.verify(log)?;
    Ok(log.snapshot_through(cursor.fence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_event::{Principal, TurnEnvelope, TurnEvent};

    fn open_log(tag: &str) -> (std::path::PathBuf, TurnEventLog) {
        let root = std::env::temp_dir().join(format!(
            "medousa-h06-cursor-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let envelope = TurnEnvelope::new(
            format!("turn-{}", uuid::Uuid::new_v4().simple()),
            Principal::operator(),
        );
        let log = TurnEventLog::open_in(&root, envelope).unwrap();
        (root, log)
    }

    #[test]
    fn cursor_rejects_unavailable_prefix() {
        let (root, log) = open_log("unavailable");
        log.append(TurnEvent::Notice {
            message: "one".into(),
        })
        .unwrap();
        let cursor = TranscriptCursor::from_log(&log);
        assert!(cursor.verify(&log).is_ok());
        assert_eq!(reconstruct_from_journal(&log, &cursor).unwrap().len(), 1);
        let mut dangling = cursor.clone();
        dangling.digest = "sha256:deadbeef".into();
        assert!(dangling.verify(&log).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn verification_remains_valid_after_later_events_are_appended() {
        let (root, log) = open_log("advanced");
        log.append(TurnEvent::Notice {
            message: "prefix".into(),
        })
        .unwrap();
        let cursor = TranscriptCursor::from_log(&log);
        log.append(TurnEvent::Notice {
            message: "later".into(),
        })
        .unwrap();
        log.append(TurnEvent::Notice {
            message: "even-later".into(),
        })
        .unwrap();
        assert!(cursor.verify(&log).is_ok());
        let reconstructed = reconstruct_from_journal(&log, &cursor).unwrap();
        assert_eq!(reconstructed.len(), 1);
        assert_eq!(cursor.fence, 1);
        assert_eq!(log.replay_fence(), 3);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reconstruction_returns_exactly_the_fenced_prefix() {
        let (root, log) = open_log("fence");
        log.append(TurnEvent::Notice {
            message: "a".into(),
        })
        .unwrap();
        log.append(TurnEvent::Notice {
            message: "b".into(),
        })
        .unwrap();
        let cursor = TranscriptCursor::from_log_through(&log, 1);
        log.append(TurnEvent::Notice {
            message: "c".into(),
        })
        .unwrap();
        let events = reconstruct_from_journal(&log, &cursor).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0].event {
            TurnEvent::Notice { message } => assert_eq!(message, "a"),
            other => panic!("unexpected event {other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn digest_covers_prefix_through_fence_not_full_transcript() {
        let (root, log) = open_log("digest");
        log.append(TurnEvent::Notice {
            message: "one".into(),
        })
        .unwrap();
        let early = TranscriptCursor::from_log(&log);
        log.append(TurnEvent::Notice {
            message: "two".into(),
        })
        .unwrap();
        let late = TranscriptCursor::from_log(&log);
        assert_ne!(early.digest, late.digest);
        assert_eq!(
            early.digest,
            digest_events(&log.snapshot_through(early.fence))
        );
        assert_eq!(
            late.digest,
            digest_events(&log.snapshot_through(late.fence))
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
