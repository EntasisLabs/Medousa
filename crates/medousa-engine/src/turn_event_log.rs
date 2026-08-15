//! Phase 1 (b/c/e) — the durable per-turn event-log spine.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
const LIVE_RING_MAX_EVENTS: usize = 512;
const LIVE_RING_MAX_BYTES: usize = 4 * 1024 * 1024;
const REPLAY_PAGE_MAX_EVENTS: usize = 256;
const REPLAY_PAGE_MAX_BYTES: usize = 1024 * 1024;
const JOURNAL_RECORD_MAX_BYTES: usize = 2 * 1024 * 1024;
const SPARSE_INDEX_STRIDE: u64 = 64;
const ACTIVE_SYNC_INTERVAL: Duration = Duration::from_millis(250);

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
    events: VecDeque<RetainedEvent>,
    retained_bytes: usize,
    evicted_events: u64,
    evicted_bytes: u64,
    sparse_offsets: Vec<SparseOffset>,
    journal: Option<BufWriter<File>>,
    journal_writes: u64,
    journal_flushes: u64,
    journal_bytes: u64,
    journal_syncs: u64,
    last_synced_seq: u64,
    last_sync: Instant,
    committed: bool,
}

struct RetainedEvent {
    event: SequencedTurnEvent,
    encoded_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
struct SparseOffset {
    seq: u64,
    offset: u64,
}

struct JournalScan {
    next_seq: u64,
    events: VecDeque<RetainedEvent>,
    retained_bytes: usize,
    evicted_events: u64,
    evicted_bytes: u64,
    sparse_offsets: Vec<SparseOffset>,
    valid_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnEventLogMetrics {
    pub retained_events: usize,
    pub retained_bytes: usize,
    pub evicted_events: u64,
    pub evicted_bytes: u64,
    pub journal_writes: u64,
    pub journal_flushes: u64,
    pub journal_syncs: u64,
    pub journal_bytes: u64,
    pub last_synced_seq: u64,
    pub sparse_checkpoints: usize,
}

/// One bounded replay read through a stable high-water fence.
#[derive(Debug, Clone)]
pub struct TurnReplayPage {
    pub events: Vec<SequencedTurnEvent>,
    pub fence_seq: u64,
    pub has_more: bool,
}

/// Durability reached by a successful journal append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalDurability {
    /// The complete record has been flushed to the filesystem.
    Written,
    /// The journal has crossed an explicit `sync_data` fence.
    Synced,
}

/// Result of one acknowledged journal append.
#[derive(Debug, Clone)]
pub struct JournalAppendReceipt {
    pub sequenced: SequencedTurnEvent,
    pub durability: JournalDurability,
    pub through_offset: u64,
}

impl JournalAppendReceipt {
    pub fn seq(&self) -> u64 {
        self.sequenced.seq()
    }
}

/// Result of atomically recording that a terminal turn was committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalCommitReceipt {
    pub through_seq: u64,
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
        let scan = scan_journal(&root, &journal_path, &envelope.turn_id)?;
        if journal.metadata()?.len() != scan.valid_bytes {
            journal.set_len(scan.valid_bytes)?;
        }
        Ok(Self {
            envelope,
            root,
            inner: Mutex::new(LogInner {
                next_seq: scan.next_seq,
                events: scan.events,
                retained_bytes: scan.retained_bytes,
                evicted_events: scan.evicted_events,
                evicted_bytes: scan.evicted_bytes,
                sparse_offsets: scan.sparse_offsets,
                journal: Some(BufWriter::new(journal)),
                journal_writes: 0,
                journal_flushes: 0,
                journal_bytes: scan.valid_bytes,
                journal_syncs: 0,
                last_synced_seq: 0,
                last_sync: Instant::now(),
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

    /// Append using the journal's next sequence and wait for an honest write receipt.
    pub fn append(&self, event: TurnEvent) -> std::io::Result<JournalAppendReceipt> {
        let seq = self.lock().next_seq;
        self.append_sequenced(seq, event)
    }

    /// Append an actor-sequenced event, rejecting divergence from journal order.
    pub fn append_sequenced(
        &self,
        seq: u64,
        event: TurnEvent,
    ) -> std::io::Result<JournalAppendReceipt> {
        let terminal = event.is_terminal();
        let sequenced = SequencedTurnEvent {
            envelope: self.envelope.at_seq(seq),
            event,
        };
        let mut line = serde_json::to_vec(&sequenced).map_err(std::io::Error::other)?;
        line.push(b'\n');
        if line.len() > JOURNAL_RECORD_MAX_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "turn journal record exceeds the configured byte limit",
            ));
        }

        let mut inner = self.lock();
        if seq != inner.next_seq {
            return Err(std::io::Error::other(format!(
                "journal sequence {seq} diverged from expected {}",
                inner.next_seq
            )));
        }
        let should_sync = terminal || inner.last_sync.elapsed() >= ACTIVE_SYNC_INTERVAL;
        {
            let journal = inner
                .journal
                .as_mut()
                .ok_or_else(|| std::io::Error::other("turn journal is closed"))?;
            journal.write_all(&line)?;
            journal.flush()?;
            if should_sync {
                journal.get_ref().sync_data()?;
            }
        }
        let durability = if should_sync {
            JournalDurability::Synced
        } else {
            JournalDurability::Written
        };

        let record_offset = inner.journal_bytes;
        inner.next_seq = inner.next_seq.saturating_add(1);
        inner.journal_writes = inner.journal_writes.saturating_add(1);
        inner.journal_flushes = inner.journal_flushes.saturating_add(1);
        inner.journal_bytes = inner
            .journal_bytes
            .saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
        if (seq - 1).is_multiple_of(SPARSE_INDEX_STRIDE) {
            inner.sparse_offsets.push(SparseOffset {
                seq,
                offset: record_offset,
            });
        }
        if should_sync {
            inner.journal_syncs = inner.journal_syncs.saturating_add(1);
            inner.last_synced_seq = seq;
            inner.last_sync = Instant::now();
        }
        inner.retained_bytes = inner.retained_bytes.saturating_add(line.len());
        inner.events.push_back(RetainedEvent {
            event: sequenced.clone(),
            encoded_bytes: line.len(),
        });
        while inner.events.len() > LIVE_RING_MAX_EVENTS
            || inner.retained_bytes > LIVE_RING_MAX_BYTES
        {
            let Some(evicted) = inner.events.pop_front() else { break };
            inner.retained_bytes = inner.retained_bytes.saturating_sub(evicted.encoded_bytes);
            inner.evicted_events = inner.evicted_events.saturating_add(1);
            inner.evicted_bytes = inner
                .evicted_bytes
                .saturating_add(u64::try_from(evicted.encoded_bytes).unwrap_or(u64::MAX));
        }
        Ok(JournalAppendReceipt {
            sequenced,
            durability,
            through_offset: inner.journal_bytes,
        })
    }

    pub fn snapshot_since(&self, since: u64) -> Vec<SequencedTurnEvent> {
        let fence = self.replay_fence();
        let mut cursor = since;
        let mut events = Vec::new();
        while cursor < fence {
            let Ok(page) = self.replay_page(cursor, fence) else { break };
            let Some(last) = page.events.last() else { break };
            cursor = last.seq();
            events.extend(page.events);
            if !page.has_more {
                break;
            }
        }
        events
    }

    /// Capture the highest sequence accepted by the journal owner.
    pub fn replay_fence(&self) -> u64 {
        self.lock().next_seq.saturating_sub(1)
    }

    /// Read one event/byte-bounded page after `since`, never beyond `fence`.
    pub fn replay_page(&self, since: u64, fence: u64) -> std::io::Result<TurnReplayPage> {
        let (fence, checkpoint, ring_page) = {
            let inner = self.lock();
            let fence = fence.min(inner.next_seq.saturating_sub(1));
            if since >= fence {
                return Ok(TurnReplayPage {
                    events: Vec::new(),
                    fence_seq: fence,
                    has_more: false,
                });
            }
            let ring_start = inner.events.front().map(|event| event.event.seq());
            let ring_covers_cursor = ring_start
                .map(|start| since.saturating_add(1) >= start)
                .unwrap_or(false);
            if ring_covers_cursor {
                let mut bytes = 0usize;
                let events = inner
                    .events
                    .iter()
                    .filter(|retained| retained.event.seq() > since && retained.event.seq() <= fence)
                    .take_while(|retained| {
                        let fits = bytes == 0
                            || bytes.saturating_add(retained.encoded_bytes)
                                <= REPLAY_PAGE_MAX_BYTES;
                        if fits {
                            bytes = bytes.saturating_add(retained.encoded_bytes);
                        }
                        fits
                    })
                    .take(REPLAY_PAGE_MAX_EVENTS)
                    .map(|retained| retained.event.clone())
                    .collect::<Vec<_>>();
                (fence, None, Some(events))
            } else {
                let target = since.saturating_add(1);
                let checkpoint = inner
                    .sparse_offsets
                    .iter()
                    .rev()
                    .find(|checkpoint| checkpoint.seq <= target)
                    .copied()
                    .unwrap_or(SparseOffset { seq: 1, offset: 0 });
                (fence, Some(checkpoint), None)
            }
        };

        let events = match ring_page {
            Some(events) => events,
            None => self.read_replay_page(
                since,
                fence,
                checkpoint.expect("disk replay has a checkpoint"),
            )?,
        };
        let last_seq = events.last().map(SequencedTurnEvent::seq).unwrap_or(since);
        if last_seq == since && since < fence {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "turn journal ended before the replay fence",
            ));
        }
        Ok(TurnReplayPage {
            events,
            fence_seq: fence,
            has_more: last_seq < fence,
        })
    }

    fn read_replay_page(
        &self,
        since: u64,
        fence: u64,
        checkpoint: SparseOffset,
    ) -> std::io::Result<Vec<SequencedTurnEvent>> {
        let turn_id = TurnEventId::parse(&self.envelope.turn_id).map_err(std::io::Error::other)?;
        let name = journal_name(&turn_id);
        reject_link(&self.root, &name)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = self.root.open_with(&name, &options)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(checkpoint.offset))?;

        let mut expected_seq = checkpoint.seq;
        let mut bytes = 0usize;
        let mut line = Vec::new();
        let mut events = Vec::new();
        while events.len() < REPLAY_PAGE_MAX_EVENTS {
            let Some(_terminated) =
                read_bounded_line(&mut reader, &mut line, JOURNAL_RECORD_MAX_BYTES)?
            else {
                break;
            };
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let event: SequencedTurnEvent =
                serde_json::from_slice(&line).map_err(std::io::Error::other)?;
            if event.envelope.turn_id != self.envelope.turn_id || event.seq() != expected_seq {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "turn journal sequence or identity mismatch",
                ));
            }
            expected_seq = expected_seq.saturating_add(1);
            if event.seq() <= since {
                continue;
            }
            if event.seq() > fence {
                break;
            }
            if !events.is_empty() && bytes.saturating_add(line.len()) > REPLAY_PAGE_MAX_BYTES {
                break;
            }
            bytes = bytes.saturating_add(line.len());
            events.push(event);
        }
        Ok(events)
    }

    pub fn fold_history(&self) -> Vec<ConversationTurn> {
        let Ok(turn_id) = TurnEventId::parse(&self.envelope.turn_id) else { return Vec::new() };
        read_journal(&self.root, &journal_name(&turn_id))
            .map(|events| fold_history_from_events(&events))
            .unwrap_or_default()
    }

    pub fn mark_committed(&self) -> std::io::Result<JournalCommitReceipt> {
        let through_seq;
        {
            let mut inner = self.lock();
            through_seq = inner.next_seq.saturating_sub(1);
            if let Some(journal) = inner.journal.as_mut() {
                journal.flush()?;
                journal.get_ref().sync_data()?;
                inner.journal_syncs = inner.journal_syncs.saturating_add(1);
                inner.last_synced_seq = through_seq;
                inner.last_sync = Instant::now();
            }
        }
        let turn_id = TurnEventId::parse(&self.envelope.turn_id).map_err(std::io::Error::other)?;
        let marker = commit_marker_name(&turn_id);
        atomic_write(&self.root, &marker, Utc::now().to_rfc3339().as_bytes())?;
        self.lock().committed = true;
        Ok(JournalCommitReceipt { through_seq })
    }

    pub fn is_committed(&self) -> bool {
        self.lock().committed
    }

    pub fn metrics(&self) -> TurnEventLogMetrics {
        let inner = self.lock();
        TurnEventLogMetrics {
            retained_events: inner.events.len(),
            retained_bytes: inner.retained_bytes,
            evicted_events: inner.evicted_events,
            evicted_bytes: inner.evicted_bytes,
            journal_writes: inner.journal_writes,
            journal_flushes: inner.journal_flushes,
            journal_syncs: inner.journal_syncs,
            journal_bytes: inner.journal_bytes,
            last_synced_seq: inner.last_synced_seq,
            sparse_checkpoints: inner.sparse_offsets.len(),
        }
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

fn scan_journal(root: &Dir, name: &str, turn_id: &str) -> std::io::Result<JournalScan> {
    reject_link(root, name)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = root.open_with(name, &options)?;
    let mut reader = BufReader::new(file);
    let mut scan = JournalScan {
        next_seq: 1,
        events: VecDeque::new(),
        retained_bytes: 0,
        evicted_events: 0,
        evicted_bytes: 0,
        sparse_offsets: Vec::new(),
        valid_bytes: 0,
    };
    let mut line = Vec::new();
    loop {
        let Some(terminated) =
            read_bounded_line(&mut reader, &mut line, JOURNAL_RECORD_MAX_BYTES)?
        else {
            return Ok(scan);
        };
        if !terminated {
            return Ok(scan);
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            scan.valid_bytes = scan
                .valid_bytes
                .saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
            continue;
        }
        let event: SequencedTurnEvent =
            serde_json::from_slice(&line).map_err(std::io::Error::other)?;
        if event.envelope.turn_id != turn_id || event.seq() != scan.next_seq {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "turn journal sequence or identity mismatch",
            ));
        }
        if (event.seq() - 1).is_multiple_of(SPARSE_INDEX_STRIDE) {
            scan.sparse_offsets.push(SparseOffset {
                seq: event.seq(),
                offset: scan.valid_bytes,
            });
        }
        scan.next_seq = scan.next_seq.saturating_add(1);
        scan.valid_bytes = scan
            .valid_bytes
            .saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
        scan.retained_bytes = scan.retained_bytes.saturating_add(line.len());
        scan.events.push_back(RetainedEvent {
            event,
            encoded_bytes: line.len(),
        });
        while scan.events.len() > LIVE_RING_MAX_EVENTS
            || scan.retained_bytes > LIVE_RING_MAX_BYTES
        {
            let Some(evicted) = scan.events.pop_front() else { break };
            scan.retained_bytes = scan.retained_bytes.saturating_sub(evicted.encoded_bytes);
            scan.evicted_events = scan.evicted_events.saturating_add(1);
            scan.evicted_bytes = scan
                .evicted_bytes
                .saturating_add(u64::try_from(evicted.encoded_bytes).unwrap_or(u64::MAX));
        }
    }
}

fn read_journal(root: &Dir, name: &str) -> Option<Vec<SequencedTurnEvent>> {
    reject_link(root, name).ok()?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = root.open_with(name, &options).ok()?;
    let mut reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut line = Vec::new();
    loop {
        let Some(terminated) =
            read_bounded_line(&mut reader, &mut line, JOURNAL_RECORD_MAX_BYTES).ok()?
        else {
            return Some(events);
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<SequencedTurnEvent>(&line) {
            Ok(ev) => events.push(ev),
            Err(_) if !terminated => return Some(events),
            Err(_) => return None,
        }
    }
}

/// Read at most `limit` bytes without allowing `BufRead::read_line` to grow an
/// attacker-controlled allocation before it finds a delimiter.
fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    output: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<Option<bool>> {
    output.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok((!output.is_empty()).then_some(false));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index.saturating_add(1));
        if output.len().saturating_add(take) > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "turn journal record exceeds the configured byte limit",
            ));
        }
        output.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            return Ok(Some(true));
        }
    }
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
        log.append(TurnEvent::ContentDelta { delta: "a".into() }).unwrap();
        log.append(TurnEvent::ContentDelta { delta: "b".into() }).unwrap();
        log.append(final_ev("done", vec![])).unwrap();
        let seqs: Vec<u64> = log.snapshot_since(0).iter().map(|e| e.seq()).collect();
        assert_eq!(seqs, vec![1, 2, 3]);
        let tail: Vec<u64> = log.snapshot_since(2).iter().map(|e| e.seq()).collect();
        assert_eq!(tail, vec![3]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn append_receipts_enforce_actor_order_and_terminal_sync() {
        let root = tmp_root("receipts");
        let log = TurnEventLog::open_in(&root, env("turn-receipts")).unwrap();

        let written = log
            .append_sequenced(1, TurnEvent::ContentDelta { delta: "a".into() })
            .unwrap();
        assert_eq!(written.durability, JournalDurability::Written);
        assert!(log
            .append_sequenced(3, TurnEvent::ContentDelta { delta: "gap".into() })
            .is_err());

        let synced = log.append_sequenced(2, final_ev("done", vec![])).unwrap();
        assert_eq!(synced.durability, JournalDurability::Synced);
        assert!(synced.through_offset > written.through_offset);

        log.close_journal();
        assert!(log.append(TurnEvent::Notice { message: "late".into() }).is_err());
        assert_eq!(log.snapshot_since(0).len(), 2);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn bounded_ring_replays_evicted_prefix_in_pages() {
        let root = tmp_root("paged-replay");
        let log = TurnEventLog::open_in(&root, env("turn-paged-replay")).unwrap();
        for index in 0..700 {
            log.append(TurnEvent::Notice {
                message: format!("event-{index}"),
            })
            .unwrap();
        }
        log.append(final_ev("done", vec![])).unwrap();

        let metrics = log.metrics();
        assert_eq!(metrics.retained_events, LIVE_RING_MAX_EVENTS);
        assert_eq!(metrics.evicted_events, 701 - LIVE_RING_MAX_EVENTS as u64);
        assert!(metrics.retained_bytes <= LIVE_RING_MAX_BYTES);
        assert!(metrics.sparse_checkpoints > 1);

        let fence = log.replay_fence();
        let first = log.replay_page(0, fence).unwrap();
        assert_eq!(first.events.len(), REPLAY_PAGE_MAX_EVENTS);
        assert_eq!(first.events.first().unwrap().seq(), 1);
        assert!(first.has_more);

        let mut cursor = 0;
        let mut replayed = Vec::new();
        while cursor < fence {
            let page = log.replay_page(cursor, fence).unwrap();
            cursor = page.events.last().unwrap().seq();
            replayed.extend(page.events);
        }
        assert_eq!(replayed.len(), 701);
        assert_eq!(replayed.last().unwrap().seq(), fence);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn bounded_line_reader_rejects_missing_delimiter_without_growing_past_limit() {
        let bytes = vec![b'x'; 65];
        let mut reader = std::io::Cursor::new(bytes);
        let mut output = Vec::new();
        let error = read_bounded_line(&mut reader, &mut output, 64).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(output.len() <= 64);
    }

    #[test]
    fn reopen_rejects_malformed_record_before_a_valid_suffix() {
        let root = tmp_root("malformed-middle");
        let envelope = env("turn-malformed-middle");
        {
            let log = TurnEventLog::open_in(&root, envelope.clone()).unwrap();
            log.append(TurnEvent::Notice { message: "first".into() }).unwrap();
        }
        let path = root.join(journal_name(
            &TurnEventId::parse("turn-malformed-middle").unwrap(),
        ));
        let valid_suffix = SequencedTurnEvent {
            envelope: envelope.at_seq(2),
            event: TurnEvent::Notice { message: "suffix".into() },
        };
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(file, "{{malformed}}").unwrap();
        serde_json::to_writer(&mut file, &valid_suffix).unwrap();
        writeln!(file).unwrap();
        drop(file);

        assert!(TurnEventLog::open_in(&root, envelope).is_err());
        assert!(recover_uncommitted(&root).is_empty());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fold_history_projects_terminal_and_handoff_bodies() {
        let root = tmp_root("fold");
        let log = TurnEventLog::open_in(&root, env("turn-fold")).unwrap();
        log.append(TurnEvent::ContentDelta {
            delta: "streamed".into(),
        }).unwrap();
        log.append(TurnEvent::WorkerAck {
            text: "on it".into(),
            tool_names: vec!["spawn".into()],
            work_id: Some("w1".into()),
            parts: vec![],
            committed_at: Utc::now(),
        }).unwrap();
        log.append(final_ev("final body", vec!["data_probe".into()])).unwrap();
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
            log.append(TurnEvent::ContentDelta { delta: "hi".into() }).unwrap();
            log.append(final_ev("answer A", vec![])).unwrap();
        }
        {
            let log = TurnEventLog::open_in(&root, env("turn-B")).unwrap();
            log.append(final_ev("answer B", vec![])).unwrap();
            log.mark_committed().unwrap();
        }
        {
            let log = TurnEventLog::open_in(&root, env("turn-C")).unwrap();
            log.append(TurnEvent::ContentDelta {
                delta: "partial".into(),
            }).unwrap();
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
            }).unwrap();
            log.append(final_ev("committed body", vec![])).unwrap();
        }
        {
            let path = root.join(journal_name(&TurnEventId::parse("turn-reopen").unwrap()));
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            write!(f, "{{\"envelope\":{{\"turn_id\":\"turn-reopen\"").unwrap();
        }
        {
            let reopened = TurnEventLog::open_in(&root, env("turn-reopen")).unwrap();
            let receipt = reopened
                .append(TurnEvent::Notice { message: "after restart".into() })
                .unwrap();
            assert_eq!(receipt.seq(), 3);
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
        colon.append(final_ev("colon", vec![])).unwrap();
        underscore.append(final_ev("underscore", vec![])).unwrap();

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
        log.append(final_ev("held authority", vec![])).unwrap();
        log.mark_committed().unwrap();
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
