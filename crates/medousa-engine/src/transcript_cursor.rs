//! H03 transcript cursor contract used by Coder logical checkpoints (H06.4).

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
    pub fn from_log(log: &TurnEventLog) -> Self {
        let fence = log.replay_fence();
        let events = log.snapshot_since(0);
        Self {
            turn_id: log.envelope().turn_id.clone(),
            journal_seq: fence,
            fence,
            digest: digest_events(&events),
        }
    }

    pub fn verify(&self, log: &TurnEventLog) -> Result<(), String> {
        if log.envelope().turn_id != self.turn_id {
            return Err("transcript cursor turn_id does not match journal".into());
        }
        let fence = log.replay_fence();
        if fence < self.fence {
            return Err("referenced H03 prefix is unavailable".into());
        }
        let events = log.snapshot_since(0);
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
        if let Ok(encoded) = serde_json::to_vec(event) {
            hasher.update(&encoded);
        }
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub fn reconstruct_from_journal(
    log: &TurnEventLog,
    cursor: &TranscriptCursor,
) -> Result<Vec<SequencedTurnEvent>, String> {
    cursor.verify(log)?;
    Ok(log.snapshot_since(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_event::{Principal, TurnEnvelope, TurnEvent};

    #[test]
    fn cursor_rejects_unavailable_prefix() {
        let root = std::env::temp_dir().join(format!(
            "medousa-h06-cursor-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let envelope = TurnEnvelope::new(
            format!("turn-{}", uuid::Uuid::new_v4().simple()),
            Principal::operator(),
        );
        let log = TurnEventLog::open_in(&root, envelope).unwrap();
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
    }
}
