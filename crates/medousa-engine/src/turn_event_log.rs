//! Phase 1 (b/c/e) — the durable per-turn event-log spine.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, OpenOptions};
use chrono::Utc;
use medousa_types::authority_id::TurnEventId;
use medousa_types::session::ConversationTurn;
use medousa_types::turn::TurnPart;

use crate::turn_event::{Principal, SequencedTurnEvent, TurnEnvelope, TurnEvent};

/// Subdirectory under the data dir holding per-turn journals.
pub const TURN_LOG_DIR: &str = "turn_log";
const JOURNAL_EXT: &str = "jsonl";
const COMMIT_EXT: &str = "committed";

static LOG_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Override the journal root (call from the daemon adapter after resolving data dir).
pub fn configure_log_root(root: PathBuf) {
    let _ = LOG_ROOT.set(root);
}

/// Default journal root: configured root, else `MEDOUSA_DATA_DIR/turn_log`, else `.medousa/turn_log`.
pub fn default_log_root() -> PathBuf {
    LOG_ROOT.get().cloned().unwrap_or_else(|| {
        std::env::var("MEDOUSA_DATA_DIR")
            .map(|d| PathBuf::from(d).join(TURN_LOG_DIR))
            .unwrap_or_else(|_| PathBuf::from(".medousa").join(TURN_LOG_DIR))
    })
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
    root: Dir,
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
        std::fs::create_dir_all(root.as_ref())?;
        let root = Dir::open_ambient_dir(root.as_ref(), ambient_authority())?;
        let turn_id = TurnEventId::parse(&envelope.turn_id).map_err(std::io::Error::other)?;
        let journal_path = journal_name(&turn_id);
        let mut options = OpenOptions::new();
        options.create(true).append(true).follow(FollowSymlinks::No);
        let journal = root.open_with(&journal_path, &options)?;
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

    pub fn append(&self, event: TurnEvent) -> SequencedTurnEvent {
        let mut inner = self.lock();
        let seq = inner.next_seq;
        inner.next_seq = inner.next_seq.saturating_add(1);
        let sequenced = SequencedTurnEvent {
            envelope: self.envelope.at_seq(seq),
            event,
        };
        if let Some(journal) = inner.journal.as_mut()
            && let Ok(line) = serde_json::to_string(&sequenced)
            && writeln!(journal, "{line}")
                .and_then(|_| journal.flush())
                .is_err()
        {
            tracing::warn!(
                turn_id = %self.envelope.turn_id,
                correlation_id = %self.envelope.correlation_id,
                seq,
                "turn_event_log journal append failed"
            );
        }
        inner.events.push(sequenced.clone());
        sequenced
    }

    pub fn snapshot_since(&self, since: u64) -> Vec<SequencedTurnEvent> {
        self.lock()
            .events
            .iter()
            .filter(|ev| ev.seq() > since)
            .cloned()
            .collect()
    }

    pub fn fold_history(&self) -> Vec<ConversationTurn> {
        fold_history_from_events(&self.lock().events)
    }

    pub fn mark_committed(&self) {
        {
            let mut inner = self.lock();
            inner.committed = true;
            if let Some(journal) = inner.journal.as_mut() {
                let _ = journal.flush();
            }
        }
        let Ok(turn_id) = TurnEventId::parse(&self.envelope.turn_id) else {
            tracing::error!(turn_id = %self.envelope.turn_id, "invalid turn id reached commit");
            return;
        };
        let marker = commit_marker_name(&turn_id);
        if let Err(err) = atomic_write(&self.root, &marker, Utc::now().to_rfc3339().as_bytes()) {
            tracing::warn!(
                turn_id = %self.envelope.turn_id,
                correlation_id = %self.envelope.correlation_id,
                error = %err,
                "turn_event_log failed to write commit marker"
            );
        }
    }

    pub fn is_committed(&self) -> bool {
        self.lock().committed
    }

    /// Close the durable journal handle before a capability-owning adapter
    /// unlinks the exact journal during session deletion.
    pub fn close_journal(&self) {
        self.lock().journal.take();
    }
}

fn journal_name(turn_id: &TurnEventId) -> String {
    format!("{}.{JOURNAL_EXT}", turn_id.storage_key().as_str())
}

fn commit_marker_name(turn_id: &TurnEventId) -> String {
    marker_name_for_storage_key(turn_id.storage_key().as_str())
}

fn marker_name_for_storage_key(storage_key: &str) -> String {
    format!("{storage_key}.{COMMIT_EXT}")
}

fn atomic_write(root: &Dir, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    reject_link(root, name)?;
    let temporary = format!(".medousa-tmp-{}", uuid::Uuid::new_v4().simple());
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = root.open_with(&temporary, &options)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_data()) {
        let _ = root.remove_file(&temporary);
        return Err(error);
    }
    drop(file);
    if let Err(error) = root.rename(&temporary, root, name) {
        let _ = root.remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn reject_link(root: &Dir, name: &str) -> std::io::Result<()> {
    match root.symlink_metadata(name) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::other(
            "turn journal path is a symbolic link",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn fold_history_from_events(events: &[SequencedTurnEvent]) -> Vec<ConversationTurn> {
    events
        .iter()
        .filter_map(|sequenced| project_turn_to_history(&sequenced.event))
        .collect()
}

pub fn project_turn_to_history(event: &TurnEvent) -> Option<ConversationTurn> {
    match event {
        TurnEvent::FinalResponse {
            text,
            tool_names,
            parts,
            committed_at,
        } => Some(history_turn(text, tool_names, None, parts, *committed_at)),
        TurnEvent::WorkerAck {
            text,
            tool_names,
            parts,
            committed_at,
            ..
        } => Some(history_turn(text, tool_names, None, parts, *committed_at)),
        TurnEvent::Checkpoint {
            text,
            tool_names,
            parts,
            committed_at,
        } => Some(history_turn(
            text,
            tool_names,
            Some("checkpoint".to_string()),
            parts,
            *committed_at,
        )),
        TurnEvent::NeedsInput {
            text,
            tool_names,
            parts,
            committed_at,
        } => Some(history_turn(
            text,
            tool_names,
            Some("needs_input".to_string()),
            parts,
            *committed_at,
        )),
        _ => None,
    }
}

fn history_turn(
    text: &str,
    tool_names: &[String],
    answer_state: Option<String>,
    parts: &[TurnPart],
    committed_at: chrono::DateTime<Utc>,
) -> ConversationTurn {
    let parts = if parts.is_empty() {
        vec![TurnPart::Text {
            markdown: text.to_string(),
        }]
    } else {
        parts.to_vec()
    };
    ConversationTurn {
        role: "assistant".to_string(),
        content: text.to_string(),
        timestamp: committed_at,
        tool_names: tool_names.to_vec(),
        answer_state,
        parts: Some(parts),
        slice_summary: None,
        speaker_profile_id: None,
    }
}

#[derive(Debug, Clone)]
pub struct RecoveredTurn {
    pub turn_id: String,
    pub session_id: Option<String>,
    pub envelope: TurnEnvelope,
    pub history: Vec<ConversationTurn>,
}

pub fn recover_uncommitted(root: impl AsRef<Path>) -> Vec<RecoveredTurn> {
    let Ok(root) = Dir::open_ambient_dir(root.as_ref(), ambient_authority()) else {
        return Vec::new();
    };
    let Ok(entries) = root.entries() else {
        return Vec::new();
    };

    let mut recovered = Vec::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(&format!(".{JOURNAL_EXT}")) else {
            continue;
        };
        if !matches!(entry.file_type(), Ok(file_type) if file_type.is_file()) {
            continue;
        }
        let marker = marker_name_for_storage_key(stem);
        if root
            .symlink_metadata(&marker)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        {
            continue;
        }
        let Some(events) = read_journal(&root, &name) else {
            continue;
        };
        if events.is_empty() {
            continue;
        }
        let Some(turn_id) = events
            .first()
            .and_then(|event| TurnEventId::parse(&event.envelope.turn_id).ok())
        else {
            continue;
        };
        if turn_id.storage_key().as_str() != stem
            || events
                .iter()
                .any(|event| event.envelope.turn_id != turn_id.as_str())
        {
            continue;
        }
        let history = fold_history_from_events(&events);
        if history.is_empty() {
            continue;
        }
        let envelope = events
            .last()
            .map(|ev| ev.envelope.clone())
            .unwrap_or_else(|| TurnEnvelope::new(stem, Principal::system()));
        let session_id = envelope.surface.as_ref().and_then(|s| s.channel_id.clone());
        recovered.push(RecoveredTurn {
            turn_id: envelope.turn_id.clone(),
            session_id,
            envelope,
            history,
        });
    }
    recovered
}

fn read_journal(root: &Dir, name: &str) -> Option<Vec<SequencedTurnEvent>> {
    reject_link(root, name).ok()?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = root.open_with(name, &options).ok()?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<SequencedTurnEvent>(&line) {
            Ok(ev) => events.push(ev),
            Err(_) => break,
        }
    }
    Some(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-turnlog-{tag}-{}",
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn env(turn: &str) -> TurnEnvelope {
        TurnEnvelope::new(turn, Principal::operator())
    }

    fn final_ev(text: &str, tool_names: Vec<String>) -> TurnEvent {
        TurnEvent::FinalResponse {
            text: text.into(),
            tool_names,
            parts: vec![],
            committed_at: Utc::now(),
        }
    }

    #[test]
    fn append_stamps_monotonic_seq_and_snapshot_since_filters() {
        let root = tmp_root("seq");
        let log = TurnEventLog::open_in(&root, env("turn-seq")).unwrap();
        log.append(TurnEvent::ContentDelta { delta: "a".into() });
        log.append(TurnEvent::ContentDelta { delta: "b".into() });
        log.append(final_ev("done", vec![]));
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
        log.append(TurnEvent::ContentDelta {
            delta: "streamed".into(),
        });
        log.append(TurnEvent::WorkerAck {
            text: "on it".into(),
            tool_names: vec!["spawn".into()],
            work_id: Some("w1".into()),
            parts: vec![],
            committed_at: Utc::now(),
        });
        log.append(final_ev("final body", vec!["data_probe".into()]));
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
        {
            let log = TurnEventLog::open_in(&root, env("turn-A")).unwrap();
            log.append(TurnEvent::ContentDelta { delta: "hi".into() });
            log.append(final_ev("answer A", vec![]));
        }
        {
            let log = TurnEventLog::open_in(&root, env("turn-B")).unwrap();
            log.append(final_ev("answer B", vec![]));
            log.mark_committed();
        }
        {
            let log = TurnEventLog::open_in(&root, env("turn-C")).unwrap();
            log.append(TurnEvent::ContentDelta {
                delta: "partial".into(),
            });
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
            log.append(TurnEvent::ContentDelta {
                delta: "one".into(),
            });
            log.append(final_ev("committed body", vec![]));
        }
        {
            let path = root.join(journal_name(&TurnEventId::parse("turn-reopen").unwrap()));
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "{{\"envelope\":{{\"turn_id\":\"turn-reopen\"").unwrap();
        }
        let recovered = recover_uncommitted(&root);
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].history[0].content, "committed body");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn hostile_turn_ids_cannot_select_or_alias_paths() {
        let root = tmp_root("authority");
        let colon = TurnEventLog::open_in(&root, env("turn:a")).unwrap();
        let underscore = TurnEventLog::open_in(&root, env("turn_a")).unwrap();
        colon.append(final_ev("colon", vec![]));
        underscore.append(final_ev("underscore", vec![]));

        let journals = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some(JOURNAL_EXT)
            })
            .count();
        assert_eq!(journals, 2);
        assert!(!root.parent().unwrap().join("escape.jsonl").exists());
        assert!(TurnEventLog::open_in(&root, env("../escape")).is_ok());
        assert!(!root.parent().unwrap().join("escape.jsonl").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn held_root_survives_ambient_replacement() {
        let root = tmp_root("replacement");
        let held = root.with_extension("held");
        let log = TurnEventLog::open_in(&root, env("turn-replacement")).unwrap();

        fs::rename(&root, &held).unwrap();
        fs::create_dir_all(&root).unwrap();
        log.append(final_ev("held authority", vec![]));
        log.mark_committed();
        log.close_journal();

        let turn_id = TurnEventId::parse("turn-replacement").unwrap();
        assert!(held.join(journal_name(&turn_id)).is_file());
        assert!(held.join(commit_marker_name(&turn_id)).is_file());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);

        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&held).ok();
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_link_backed_journal() {
        use std::os::unix::fs::symlink;

        let root = tmp_root("link");
        let outside = root.with_extension("outside");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, b"outside canary").unwrap();
        let turn_id = TurnEventId::parse("turn-link").unwrap();
        symlink(&outside, root.join(journal_name(&turn_id))).unwrap();

        assert!(recover_uncommitted(&root).is_empty());
        assert_eq!(fs::read(&outside).unwrap(), b"outside canary");

        fs::remove_dir_all(&root).ok();
        fs::remove_file(&outside).ok();
    }
}
