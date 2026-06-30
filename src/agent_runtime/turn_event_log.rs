//! Phase 1 (b/c/e) — the durable per-turn event-log spine.
//!
//! This is the single source of truth the plan calls for: the engine appends
//! typed [`TurnEvent`]s here; SSE replay and history become **projections**
//! (folds) off the log rather than the throwaway `TurnEventChannel` ring and the
//! error-swallowing direct `append_turn`.
//!
//! Responsibilities:
//! * **append-only ordering + seq stamping** — [`TurnEventLog::append`] stamps
//!   the monotonic `seq` that SSE `?since=N` replay dedupes on,
//! * **durable journal** — each turn is journaled to `<root>/turn_log/<turn_id>.jsonl`
//!   as one JSON line per event, so an uncommitted turn survives a `kill -9`,
//! * **SSE replay projection** — [`TurnEventLog::snapshot_since`],
//! * **history projection** — [`TurnEventLog::fold_history`] folds terminal /
//!   handoff bodies into `ConversationTurn`s,
//! * **commit marker + recovery** — [`TurnEventLog::mark_committed`] records that
//!   the terminal body reached `session_turn`; [`recover_uncommitted`] replays
//!   journals whose terminal body never committed, idempotent by `turn_id`.
//!
//! ## Adoption status
//!
//! This module is built and unit-tested as the spine **data structure +
//! projection + recovery API** that Phase 2/3/5 adopt. The live daemon path
//! (replacing `TurnEventChannel` for SSE and swapping the direct persist) is the
//! documented Phase 1 remainder — see the worker handoff notes. Keeping it
//! additive here means the projections/recovery are real and verifiable without
//! destabilizing the proven SSE/persist folds in a single pass.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;

use crate::session::ConversationTurn;

use super::turn_event::{SequencedTurnEvent, TurnEnvelope, TurnEvent};

/// Subdirectory under the data dir holding per-turn journals.
const TURN_LOG_DIR: &str = "turn_log";
const JOURNAL_EXT: &str = "jsonl";
const COMMIT_EXT: &str = "committed";

/// Default journal root: `<medousa_data_dir>/turn_log`.
pub fn default_log_root() -> PathBuf {
    crate::paths::medousa_data_dir().join(TURN_LOG_DIR)
}

struct LogInner {
    next_seq: u64,
    events: Vec<SequencedTurnEvent>,
    journal: Option<File>,
    committed: bool,
}

/// Append-only event log for a single turn (the spine).
pub struct TurnEventLog {
    envelope: TurnEnvelope,
    root: PathBuf,
    inner: Mutex<LogInner>,
}

impl TurnEventLog {
    /// Open (create) the durable log for a turn under the default data dir.
    pub fn open(envelope: TurnEnvelope) -> std::io::Result<Self> {
        Self::open_in(default_log_root(), envelope)
    }

    /// Open (create) the durable log for a turn under an explicit root
    /// (used by tests / alternate data dirs).
    pub fn open_in(root: impl AsRef<Path>, envelope: TurnEnvelope) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let journal_path = journal_path(&root, &envelope.turn_id);
        let journal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)?;
        Ok(Self {
            envelope,
            root,
            inner: Mutex::new(LogInner {
                next_seq: 1,
                events: Vec::new(),
                journal: Some(journal),
                committed: false,
            }),
        })
    }

    pub fn turn_id(&self) -> &str {
        &self.envelope.turn_id
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LogInner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Append an event: stamp the next `seq`, retain it in order, and durably
    /// journal it (append JSON line). Journal write happens-before the returned
    /// value so a reader can never observe a seq missing from the journal.
    pub fn append(&self, event: TurnEvent) -> SequencedTurnEvent {
        let mut inner = self.lock();
        let seq = inner.next_seq;
        inner.next_seq = inner.next_seq.saturating_add(1);
        let sequenced = SequencedTurnEvent {
            envelope: self.envelope.at_seq(seq),
            event,
        };
        if let Some(journal) = inner.journal.as_mut() {
            if let Ok(line) = serde_json::to_string(&sequenced) {
                // Best-effort durable append; a write failure is surfaced, not
                // swallowed silently (the in-memory log still retains the event
                // so live SSE is unaffected).
                if writeln!(journal, "{line}").and_then(|_| journal.flush()).is_err() {
                    eprintln!(
                        "turn_event_log: journal append failed (turn={} seq={seq})",
                        self.envelope.turn_id
                    );
                }
            }
        }
        inner.events.push(sequenced.clone());
        sequenced
    }

    /// SSE replay projection: all events with `seq > since`, in order.
    pub fn snapshot_since(&self, since: u64) -> Vec<SequencedTurnEvent> {
        self.lock()
            .events
            .iter()
            .filter(|ev| ev.seq() > since)
            .cloned()
            .collect()
    }

    /// History projection: fold terminal / handoff bodies into `ConversationTurn`s.
    ///
    /// This is intentionally a coarse projection over the durable event bodies;
    /// the live persist path additionally folds rich `TurnPart`s
    /// (`TurnPartsAccumulator`). Bringing those parts into `TurnEvent` so this
    /// fold is byte-identical to the live persist is part of the Phase 1
    /// remainder (tracked in the handoff notes).
    pub fn fold_history(&self) -> Vec<ConversationTurn> {
        fold_history(&self.lock().events)
    }

    /// Record that this turn's terminal body has been committed to durable
    /// history (`session_turn`). Writes a sidecar marker so recovery can skip it.
    pub fn mark_committed(&self) {
        {
            let mut inner = self.lock();
            inner.committed = true;
            if let Some(journal) = inner.journal.as_mut() {
                let _ = journal.flush();
            }
        }
        let marker = commit_marker_path(&self.root, &self.envelope.turn_id);
        if let Err(err) = fs::write(&marker, Utc::now().to_rfc3339()) {
            eprintln!(
                "turn_event_log: failed to write commit marker for turn {}: {err}",
                self.envelope.turn_id
            );
        }
    }

    pub fn is_committed(&self) -> bool {
        self.lock().committed
    }
}

fn journal_path(root: &Path, turn_id: &str) -> PathBuf {
    root.join(format!("{}.{JOURNAL_EXT}", sanitize_turn_id(turn_id)))
}

fn commit_marker_path(root: &Path, turn_id: &str) -> PathBuf {
    root.join(format!("{}.{COMMIT_EXT}", sanitize_turn_id(turn_id)))
}

/// Keep journal filenames filesystem-safe (turn ids are uuid-ish but guard anyway).
fn sanitize_turn_id(turn_id: &str) -> String {
    turn_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Coarse fold of a sequenced event stream into persisted history turns.
fn fold_history(events: &[SequencedTurnEvent]) -> Vec<ConversationTurn> {
    events
        .iter()
        .filter_map(|sequenced| match &sequenced.event {
            TurnEvent::FinalResponse { text, tool_names }
            | TurnEvent::Checkpoint { text, tool_names }
            | TurnEvent::NeedsInput { text, tool_names }
            | TurnEvent::WorkerAck {
                text, tool_names, ..
            } => {
                let answer_state = match &sequenced.event {
                    TurnEvent::Checkpoint { .. } => Some("checkpoint".to_string()),
                    TurnEvent::NeedsInput { .. } => Some("needs_input".to_string()),
                    _ => None,
                };
                Some(ConversationTurn {
                    role: "assistant".to_string(),
                    content: text.clone(),
                    timestamp: Utc::now(),
                    tool_names: tool_names.clone(),
                    answer_state,
                    parts: None,
                    slice_summary: None,
                })
            }
            _ => None,
        })
        .collect()
}

/// A turn whose journal contains a terminal/handoff body but was never marked
/// committed — a candidate for startup replay-recovery. Idempotent by `turn_id`:
/// the caller re-commits the body to `session_turn` only if absent.
#[derive(Debug, Clone)]
pub struct RecoveredTurn {
    pub turn_id: String,
    pub session_id: Option<String>,
    pub envelope: TurnEnvelope,
    /// The history turns this journal folds to (usually one terminal body).
    pub history: Vec<ConversationTurn>,
}

/// Scan the journal root and return turns whose terminal body never committed.
///
/// Recovery deletes the "reply shown but missing from history" class: on daemon
/// start, replay these and persist their bodies (idempotent by turn id).
pub fn recover_uncommitted(root: impl AsRef<Path>) -> Vec<RecoveredTurn> {
    let root = root.as_ref();
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut recovered = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(JOURNAL_EXT) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // Already committed → nothing to recover.
        if commit_marker_path(root, stem).exists() {
            continue;
        }
        let Some(events) = read_journal(&path) else {
            continue;
        };
        if events.is_empty() {
            continue;
        }
        let history = fold_history(&events);
        if history.is_empty() {
            // No terminal body was reached before the crash — nothing to commit.
            continue;
        }
        let envelope = events
            .last()
            .map(|ev| ev.envelope.clone())
            .unwrap_or_else(|| TurnEnvelope::new(stem, super::turn_event::Principal::system()));
        let session_id = envelope
            .surface
            .as_ref()
            .and_then(|s| s.channel_id.clone());
        recovered.push(RecoveredTurn {
            turn_id: envelope.turn_id.clone(),
            session_id,
            envelope,
            history,
        });
    }
    recovered
}

fn read_journal(path: &Path) -> Option<Vec<SequencedTurnEvent>> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        // Tolerate a torn final line from a crash mid-write: stop at first parse failure.
        match serde_json::from_str::<SequencedTurnEvent>(&line) {
            Ok(ev) => events.push(ev),
            Err(_) => break,
        }
    }
    Some(events)
}

#[cfg(test)]
mod tests {
    use super::super::turn_event::Principal;
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-turnlog-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ))
    }

    fn env(turn: &str) -> TurnEnvelope {
        TurnEnvelope::new(turn, Principal::operator())
    }

    #[test]
    fn append_stamps_monotonic_seq_and_snapshot_since_filters() {
        let root = tmp_root("seq");
        let log = TurnEventLog::open_in(&root, env("turn-seq")).unwrap();
        log.append(TurnEvent::ContentDelta { delta: "a".into() });
        log.append(TurnEvent::ContentDelta { delta: "b".into() });
        log.append(TurnEvent::FinalResponse {
            text: "done".into(),
            tool_names: vec![],
        });
        let seqs: Vec<u64> = log.snapshot_since(0).iter().map(|e| e.seq()).collect();
        assert_eq!(seqs, vec![1, 2, 3]);
        let tail: Vec<u64> = log.snapshot_since(2).iter().map(|e| e.seq()).collect();
        assert_eq!(tail, vec![3]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fold_history_projects_terminal_and_handoff_bodies() {
        let root = tmp_root("fold");
        let log = TurnEventLog::open_in(&root, env("turn-fold")).unwrap();
        log.append(TurnEvent::ContentDelta { delta: "streamed".into() });
        log.append(TurnEvent::WorkerAck {
            text: "on it".into(),
            tool_names: vec!["spawn".into()],
            work_id: Some("w1".into()),
        });
        log.append(TurnEvent::FinalResponse {
            text: "final body".into(),
            tool_names: vec!["data_probe".into()],
        });
        let history = log.fold_history();
        assert_eq!(history.len(), 2, "worker ack + final fold to history");
        assert_eq!(history[0].content, "on it");
        assert_eq!(history[1].content, "final body");
        assert_eq!(history[1].tool_names, vec!["data_probe".to_string()]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recovery_returns_uncommitted_terminal_turns_and_skips_committed() {
        let root = tmp_root("recover");

        // Turn A: reached a terminal body but was never marked committed → recover.
        {
            let log = TurnEventLog::open_in(&root, env("turn-A")).unwrap();
            log.append(TurnEvent::ContentDelta { delta: "hi".into() });
            log.append(TurnEvent::FinalResponse {
                text: "answer A".into(),
                tool_names: vec![],
            });
            // no mark_committed → simulates kill -9 before history commit.
        }
        // Turn B: terminal body committed → must be skipped.
        {
            let log = TurnEventLog::open_in(&root, env("turn-B")).unwrap();
            log.append(TurnEvent::FinalResponse {
                text: "answer B".into(),
                tool_names: vec![],
            });
            log.mark_committed();
        }
        // Turn C: no terminal body (crashed mid-stream) → nothing to recover.
        {
            let log = TurnEventLog::open_in(&root, env("turn-C")).unwrap();
            log.append(TurnEvent::ContentDelta { delta: "partial".into() });
        }

        let mut recovered = recover_uncommitted(&root);
        recovered.sort_by(|a, b| a.turn_id.cmp(&b.turn_id));
        assert_eq!(recovered.len(), 1, "only turn-A is recoverable");
        assert_eq!(recovered[0].turn_id, "turn-A");
        assert_eq!(recovered[0].history.len(), 1);
        assert_eq!(recovered[0].history[0].content, "answer A");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn journal_survives_reopen_and_tolerates_torn_tail() {
        let root = tmp_root("reopen");
        {
            let log = TurnEventLog::open_in(&root, env("turn-reopen")).unwrap();
            log.append(TurnEvent::ContentDelta { delta: "one".into() });
            log.append(TurnEvent::FinalResponse {
                text: "committed body".into(),
                tool_names: vec![],
            });
        }
        // Append a torn/partial line simulating a crash mid-write.
        {
            let path = journal_path(&root, "turn-reopen");
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(f, "{{\"envelope\":{{\"turn_id\":\"turn-reopen\"").unwrap();
        }
        let recovered = recover_uncommitted(&root);
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].history[0].content, "committed body");
        fs::remove_dir_all(&root).ok();
    }
}
