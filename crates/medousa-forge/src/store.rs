//! Filesystem-backed store. Layout (authoritative Forge metadata lives under
//! the configured forge root — never inside the user's worktree):
//!
//! ```text
//! {forge_root}/
//!   schema_version                 # single line, u32 (store layout)
//!   items/{opaque_work_key}/manifest.json      # snapshot — strictly a replay cache
//!   items/{opaque_work_key}/events.jsonl       # v1 append-only log
//!   items/{opaque_work_key}/events.v2          # v2 framed log (when migrated)
//!   items/{opaque_work_key}/store_generation   # 1=v1 authority, 2=v2 authority
//! ```
//!
//! Snapshots are written atomically (tmp + sync + rename + dir sync). Replay
//! tolerates a truncated trailing record, which is the expected aftermath of a
//! crash mid-append. Per-item `store_generation` selects v1 vs v2 authority.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use fs2::FileExt;
use medousa_store::{DurabilityLevel, FileTransaction, StoreRoot};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::events::{EVENT_SCHEMA_VERSION, EventPayload, TransitionEvent};
use crate::log_v2::{
    self, LogAuthority, append_v2_frame_at, events_v2_path, recover_tail_v2, replay_v2,
    select_log_authority,
};
use crate::model::{ActorRef, WorkId, WorkItem};

pub const STORE_SCHEMA_VERSION: u32 = 1;
static SNAPSHOT_TMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// An exclusive cross-process file lock (fs2). Unlocking happens on drop.
/// Lock ordering is always repo → item, never the reverse.
pub struct FileLock {
    file: File,
}

impl FileLock {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(Self { file })
    }

    fn try_acquire(path: &Path) -> Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self { file })),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Snapshot cache envelope: the folded item plus how far the log had been
/// applied when cached. Never authoritative — replay is.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEnvelope {
    pub applied_seq: u64,
    pub item: WorkItem,
}

/// In-memory tail recovered once per item, then advanced on append.
#[derive(Debug, Clone)]
pub struct TailMeta {
    pub last_seq: u64,
    pub last_offset: u64,
    pub last_hash: [u8; 32],
    pub lease_acquisitions: u64,
    pub operations_started: u64,
}

pub struct FsWorkStore {
    root: PathBuf,
    store_root: Arc<StoreRoot>,
    transaction: FileTransaction,
    tails: Mutex<HashMap<String, TailMeta>>,
    append_gates: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl FsWorkStore {
    /// Open (creating if needed) a forge store at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("items"))?;
        let version_path = root.join("schema_version");
        if version_path.exists() {
            let raw = fs::read_to_string(&version_path)?;
            let version: u32 = raw.trim().parse().map_err(|_| {
                ForgeError::Store(format!("unreadable schema_version in {}", root.display()))
            })?;
            if version > STORE_SCHEMA_VERSION {
                return Err(ForgeError::Store(format!(
                    "store schema version {version} is newer than this build ({STORE_SCHEMA_VERSION})"
                )));
            }
        } else {
            fs::write(&version_path, format!("{STORE_SCHEMA_VERSION}\n"))?;
        }
        // StoreRoot needs a stable no-follow handle; keep `self.root` as the
        // caller-facing spelling so worktree/orphan paths stay comparable.
        let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
        let store_root = Arc::new(
            StoreRoot::open_or_create_nofollow(&canonical)
                .map_err(|err| ForgeError::Store(err.to_string()))?,
        );
        let transaction = FileTransaction::new(Arc::clone(&store_root));
        Ok(Self {
            root,
            store_root,
            transaction,
            tails: Mutex::new(HashMap::new()),
            append_gates: Mutex::new(HashMap::new()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn store_root(&self) -> &StoreRoot {
        &self.store_root
    }

    pub fn store_root_arc(&self) -> &Arc<StoreRoot> {
        &self.store_root
    }

    pub fn transaction(&self) -> &FileTransaction {
        &self.transaction
    }

    /// Replace the store's transaction (used by failpoint tests).
    pub fn set_transaction(&mut self, transaction: FileTransaction) {
        self.transaction = transaction;
    }

    pub fn item_dir(&self, work_id: &WorkId) -> PathBuf {
        let items = self.root.join("items");
        let opaque = items.join(work_id.storage_key());
        if opaque.exists() {
            return opaque;
        }
        if is_legacy_forge_id(work_id.as_str()) {
            let legacy = items.join(work_id.as_str());
            if legacy
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
            {
                return legacy;
            }
        }
        opaque
    }

    pub fn events_path(&self, work_id: &WorkId) -> PathBuf {
        self.item_dir(work_id).join("events.jsonl")
    }

    pub fn snapshot_path(&self, work_id: &WorkId) -> PathBuf {
        self.item_dir(work_id).join("manifest.json")
    }

    pub fn item_exists(&self, work_id: &WorkId) -> bool {
        self.events_path(work_id).exists() || events_v2_path(self, work_id).exists()
    }

    /// Append an event, assigning the next monotonic seq for this work item.
    /// The write is flushed and synced before returning.
    ///
    /// Production Forge/reconcile mutations must use
    /// [`crate::forge::Forge::commit_event`] / [`crate::owner::append_owned`]
    /// so the per-item owner remains authoritative. Prefer those paths outside
    /// store-unit tests and fixtures.
    pub fn append(
        &self,
        work_id: &WorkId,
        actor: &ActorRef,
        payload: EventPayload,
    ) -> Result<TransitionEvent> {
        let tail = self.cached_tail(work_id)?;
        self.append_at(work_id, actor, payload, tail.last_seq + 1)
    }

    pub fn append_at(
        &self,
        work_id: &WorkId,
        actor: &ActorRef,
        payload: EventPayload,
        seq: u64,
    ) -> Result<TransitionEvent> {
        let gate = {
            let mut gates = self
                .append_gates
                .lock()
                .map_err(|_| ForgeError::Store("append gate poisoned".into()))?;
            Arc::clone(
                gates
                    .entry(work_id.as_str().to_owned())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _append_guard = gate
            .lock()
            .map_err(|_| ForgeError::Store("append gate poisoned".into()))?;

        let dir = self.item_dir(work_id);
        fs::create_dir_all(&dir)?;
        // Always re-scan the durable log before writing so a stale in-memory
        // tail cannot reuse sequences past a torn or repaired prefix.
        let authority = select_log_authority(self, work_id)?;
        let tail = self.recover_tail(work_id)?;
        if seq != tail.last_seq.saturating_add(1) {
            return Err(ForgeError::Conflict(format!(
                "append sequence fence: expected {}, got {seq}",
                tail.last_seq.saturating_add(1)
            )));
        }
        let event = TransitionEvent::new(work_id.clone(), seq, actor.clone(), payload);
        if event.schema_version != EVENT_SCHEMA_VERSION {
            return Err(ForgeError::Store("event schema drift".into()));
        }

        match authority {
            LogAuthority::V2 => {
                let path = events_v2_path(self, work_id);
                if path.exists() {
                    let len = fs::metadata(&path)?.len();
                    if len > tail.last_offset {
                        let file = OpenOptions::new().write(true).open(&path)?;
                        file.set_len(tail.last_offset)?;
                        file.sync_all()?;
                    } else if len < tail.last_offset {
                        return Err(ForgeError::Store(format!(
                            "events.v2 shorter than recovered tail at {}",
                            path.display()
                        )));
                    }
                }
                let written = append_v2_frame_at(
                    self,
                    work_id,
                    &event,
                    &tail.last_hash,
                    DurabilityLevel::Synced,
                )?;
                let offset = tail.last_offset.saturating_add(written as u64);
                let payload = serde_json::to_vec(&event)?;
                let digest = Sha256::digest(&payload);
                let mut frame_checksum = [0u8; 32];
                frame_checksum.copy_from_slice(&digest);
                self.remember_tail_hash(work_id, &event, offset, frame_checksum);
                Ok(event)
            }
            LogAuthority::V1 => {
                let path = self.events_path(work_id);
                if path.exists() {
                    let len = fs::metadata(&path)?.len();
                    if len > tail.last_offset {
                        // Incomplete final record only — truncate before appending so
                        // the next frame cannot glue onto torn bytes.
                        let file = OpenOptions::new().write(true).open(&path)?;
                        file.set_len(tail.last_offset)?;
                        file.sync_all()?;
                    } else if len < tail.last_offset {
                        return Err(ForgeError::Store(format!(
                            "events log shorter than recovered tail at {}",
                            path.display()
                        )));
                    }
                }
                let mut line = serde_json::to_vec(&event)?;
                line.push(b'\n');
                let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
                file.write_all(&line)?;
                file.sync_all()?;
                let offset = file.metadata().map(|meta| meta.len()).unwrap_or(0);
                self.remember_tail(work_id, &event, offset);
                Ok(event)
            }
        }
    }

    pub fn cached_last_seq(&self, work_id: &WorkId) -> Result<u64> {
        Ok(self.cached_tail(work_id)?.last_seq)
    }

    pub fn cached_tail(&self, work_id: &WorkId) -> Result<TailMeta> {
        if let Some(tail) = self
            .tails
            .lock()
            .map_err(|_| ForgeError::Store("tail cache poisoned".into()))?
            .get(work_id.as_str())
            .cloned()
        {
            let path = match select_log_authority(self, work_id)? {
                LogAuthority::V2 => events_v2_path(self, work_id),
                LogAuthority::V1 => self.events_path(work_id),
            };
            let len = if path.exists() {
                fs::metadata(&path)?.len()
            } else {
                0
            };
            // Accept the cache only when it still matches the durable file
            // length. A longer file means a torn final record; shorter means
            // external truncation — both require a fresh disk recovery.
            if len == tail.last_offset {
                return Ok(tail);
            }
        }
        let tail = self.recover_tail(work_id)?;
        self.tails
            .lock()
            .map_err(|_| ForgeError::Store("tail cache poisoned".into()))?
            .insert(work_id.as_str().to_owned(), tail.clone());
        Ok(tail)
    }

    /// Stream the log once, keeping only tail metadata. Does not build a
    /// `Vec<String>` of the whole file.
    ///
    /// Tolerates only a demonstrably incomplete final record (unparseable
    /// bytes at EOF). Corruption anywhere before EOF fails closed.
    pub fn recover_tail(&self, work_id: &WorkId) -> Result<TailMeta> {
        match select_log_authority(self, work_id)? {
            LogAuthority::V2 => {
                let _ = log_v2::cleanup_migration_staging(self, work_id);
                recover_tail_v2(&events_v2_path(self, work_id))
            }
            LogAuthority::V1 => {
                let path = self.events_path(work_id);
                if !path.exists() {
                    return Ok(TailMeta {
                        last_seq: 0,
                        last_offset: 0,
                        last_hash: [0; 32],
                        lease_acquisitions: 0,
                        operations_started: 0,
                    });
                }
                let data = fs::read(&path)?;
                scan_jsonl_tail(&path, &data)
            }
        }
    }

    fn remember_tail(&self, work_id: &WorkId, event: &TransitionEvent, offset: u64) {
        self.remember_tail_hash(
            work_id,
            event,
            offset,
            hash_line(&serde_json::to_string(event).unwrap_or_default()),
        );
    }

    fn remember_tail_hash(
        &self,
        work_id: &WorkId,
        event: &TransitionEvent,
        offset: u64,
        last_hash: [u8; 32],
    ) {
        if let Ok(mut tails) = self.tails.lock() {
            let previous = tails.get(work_id.as_str()).cloned();
            tails.insert(
                work_id.as_str().to_owned(),
                TailMeta {
                    last_seq: event.seq,
                    last_offset: offset,
                    last_hash,
                    lease_acquisitions: previous
                        .as_ref()
                        .map(|tail| {
                            tail.lease_acquisitions
                                + u64::from(matches!(
                                    event.payload,
                                    EventPayload::LeaseAcquired { .. }
                                ))
                        })
                        .unwrap_or(u64::from(matches!(
                            event.payload,
                            EventPayload::LeaseAcquired { .. }
                        ))),
                    operations_started: previous
                        .as_ref()
                        .map(|tail| {
                            tail.operations_started
                                + u64::from(matches!(
                                    event.payload,
                                    EventPayload::OperationStarted { .. }
                                ))
                        })
                        .unwrap_or(u64::from(matches!(
                            event.payload,
                            EventPayload::OperationStarted { .. }
                        ))),
                },
            );
        }
    }

    /// Replay the authoritative event log for this item (v1 or v2).
    pub fn replay(&self, work_id: &WorkId) -> Result<Vec<TransitionEvent>> {
        match select_log_authority(self, work_id)? {
            LogAuthority::V2 => {
                let _ = log_v2::cleanup_migration_staging(self, work_id);
                replay_v2(&events_v2_path(self, work_id))
            }
            LogAuthority::V1 => self.replay_v1(work_id),
        }
    }

    /// Replay v1 JSONL regardless of generation marker (migration validation).
    pub fn replay_v1(&self, work_id: &WorkId) -> Result<Vec<TransitionEvent>> {
        let path = self.events_path(work_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut pending: Option<(usize, String)> = None;
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some((prev_idx, prev)) = pending.take() {
                match serde_json::from_str::<TransitionEvent>(&prev) {
                    Ok(event) => events.push(event),
                    Err(err) => {
                        return Err(ForgeError::Store(format!(
                            "corrupt event at {} line {}: {err}",
                            path.display(),
                            prev_idx + 1
                        )));
                    }
                }
            }
            pending = Some((idx, line));
        }
        if let Some((idx, line)) = pending {
            match serde_json::from_str::<TransitionEvent>(&line) {
                Ok(event) => events.push(event),
                Err(_) => {
                    let _ = idx;
                }
            }
        }
        for window in events.windows(2) {
            if window[1].seq <= window[0].seq {
                return Err(ForgeError::Store(format!(
                    "non-monotonic seq at {} ({} then {})",
                    path.display(),
                    window[0].seq,
                    window[1].seq
                )));
            }
        }
        Ok(events)
    }

    /// Drop a cached tail after authority changes (e.g. v1→v2 migration).
    pub fn invalidate_tail(&self, work_id: &WorkId) {
        if let Ok(mut tails) = self.tails.lock() {
            tails.remove(work_id.as_str());
        }
    }

    /// Write the snapshot cache atomically: tmp file + sync + rename + dir sync.
    /// The snapshot is strictly a cache and may be deleted at any time.
    /// `applied_seq` records how far the fold had gone when cached.
    pub fn write_snapshot(&self, item: &WorkItem, applied_seq: u64) -> Result<()> {
        let dir = self.item_dir(&item.id);
        fs::create_dir_all(&dir)
            .map_err(|err| snapshot_io_error("create snapshot directory", &dir, err))?;
        let sequence = SNAPSHOT_TMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(
            "manifest.json.tmp-{}-{sequence}",
            std::process::id()
        ));
        let final_path = self.snapshot_path(&item.id);
        let envelope = SnapshotEnvelope {
            applied_seq,
            item: item.clone(),
        };
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp)
                .map_err(|err| snapshot_io_error("create snapshot temporary file", &tmp, err))?;
            file.write_all(serde_json::to_string_pretty(&envelope)?.as_bytes())
                .map_err(|err| snapshot_io_error("write snapshot temporary file", &tmp, err))?;
            file.sync_all()
                .map_err(|err| snapshot_io_error("sync snapshot temporary file", &tmp, err))?;
        }
        if let Err(err) = replace_snapshot(&tmp, &final_path) {
            let _ = fs::remove_file(&tmp);
            return Err(snapshot_io_error("replace snapshot", &final_path, err));
        }
        sync_dir(&dir).map_err(|err| snapshot_io_error("sync snapshot directory", &dir, err))?;
        Ok(())
    }

    /// Read the snapshot cache, if present and parseable.
    pub fn read_snapshot(&self, work_id: &WorkId) -> Result<Option<SnapshotEnvelope>> {
        let path = self.snapshot_path(work_id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)?;
        match serde_json::from_str::<SnapshotEnvelope>(&raw) {
            Ok(envelope) => Ok(Some(envelope)),
            // Cache is disposable — treat an unreadable snapshot as absent.
            Err(_) => Ok(None),
        }
    }

    /// All work item ids known to the store (directory scan).
    pub fn list_item_ids(&self) -> Result<Vec<WorkId>> {
        let items_dir = self.root.join("items");
        let mut ids = Vec::new();
        if !items_dir.exists() {
            return Ok(ids);
        }
        for entry in fs::read_dir(&items_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let snapshot = entry.path().join("manifest.json");
            if let Ok(raw) = fs::read_to_string(snapshot)
                && let Ok(envelope) = serde_json::from_str::<SnapshotEnvelope>(&raw)
                && envelope.item.id.storage_key() == name
            {
                ids.push(envelope.item.id);
                continue;
            }
            let events = entry.path().join("events.jsonl");
            if let Ok(raw) = fs::read_to_string(events)
                && let Some(line) = raw.lines().find(|line| !line.trim().is_empty())
                && let Ok(event) = serde_json::from_str::<TransitionEvent>(line)
                && event.work_id.storage_key() == name
            {
                ids.push(event.work_id);
                continue;
            }
            let events_v2 = entry.path().join("events.v2");
            if events_v2.exists()
                && let Ok(events) = crate::log_v2::replay_v2(&events_v2)
                && let Some(event) = events.first()
                && event.work_id.storage_key() == name
            {
                ids.push(event.work_id.clone());
                continue;
            }
            if is_legacy_forge_id(&name) && !name.starts_with("work1-") {
                ids.push(WorkId::from(name));
            }
        }
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(ids)
    }

    /// Exclusive cross-process lock on a work item's mutation stream.
    pub fn lock_item(&self, work_id: &WorkId) -> Result<FileLock> {
        FileLock::acquire(&self.item_dir(work_id).join(".lock"))
    }

    /// Non-blocking variant: `Ok(None)` when another process holds the lock.
    pub fn try_lock_item(&self, work_id: &WorkId) -> Result<Option<FileLock>> {
        FileLock::try_acquire(&self.item_dir(work_id).join(".lock"))
    }

    /// Exclusive cross-process lock on a repository's mutation stream
    /// (integration, worktree add/remove). Always acquired *before* any item
    /// lock — lock ordering is repo → item.
    pub fn lock_repo(&self, repo_key: &str) -> Result<FileLock> {
        let key = crate::model::Digest::sha256_hex(repo_key.as_bytes());
        FileLock::acquire(
            &self
                .root
                .join("repos")
                .join(format!("repo1-{}", key.as_str()))
                .join(".lock"),
        )
    }
}

fn scan_jsonl_tail(path: &Path, data: &[u8]) -> Result<TailMeta> {
    let mut last_seq = 0u64;
    let mut last_hash = [0u8; 32];
    let mut lease_acquisitions = 0u64;
    let mut operations_started = 0u64;
    let mut last_good_offset = 0u64;
    let mut previous_seq = 0u64;
    let mut start = 0usize;

    while start < data.len() {
        let relative_newline = data[start..].iter().position(|&byte| byte == b'\n');
        let (line_end, record_end, at_eof) = match relative_newline {
            Some(index) => {
                let line_end = start + index;
                (line_end, line_end + 1, line_end + 1 >= data.len())
            }
            None => (data.len(), data.len(), true),
        };
        let line_bytes = &data[start..line_end];
        if line_bytes
            .iter()
            .all(|&byte| byte == b' ' || byte == b'\t' || byte == b'\r')
        {
            // Blank lines are ignored but do not extend the committed prefix.
            start = record_end;
            continue;
        }
        let line = std::str::from_utf8(line_bytes).map_err(|_| {
            ForgeError::Store(format!(
                "corrupt event encoding at {} offset {start}",
                path.display()
            ))
        })?;
        match serde_json::from_str::<TransitionEvent>(line) {
            Ok(event) => {
                if event.seq <= previous_seq && previous_seq != 0 {
                    return Err(ForgeError::Store(format!(
                        "non-monotonic seq at {} ({} then {})",
                        path.display(),
                        previous_seq,
                        event.seq
                    )));
                }
                previous_seq = event.seq;
                last_seq = event.seq;
                last_hash = hash_line(line);
                if matches!(event.payload, EventPayload::LeaseAcquired { .. }) {
                    lease_acquisitions += 1;
                }
                if matches!(event.payload, EventPayload::OperationStarted { .. }) {
                    operations_started += 1;
                }
                last_good_offset = record_end as u64;
                start = record_end;
            }
            Err(err) => {
                if at_eof {
                    // Demonstrably incomplete final record only.
                    break;
                }
                return Err(ForgeError::Store(format!(
                    "corrupt event at {} offset {start}: {err}",
                    path.display()
                )));
            }
        }
    }

    Ok(TailMeta {
        last_seq,
        last_offset: last_good_offset,
        last_hash,
        lease_acquisitions,
        operations_started,
    })
}

fn hash_line(line: &str) -> [u8; 32] {
    let digest = Sha256::digest(line.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn is_legacy_forge_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn snapshot_io_error(action: &str, path: &Path, error: std::io::Error) -> ForgeError {
    ForgeError::Store(format!("{action} {}: {error}", path.display()))
}

#[cfg(not(windows))]
fn replace_snapshot(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace_snapshot(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows::core::PCWSTR;

    let from = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .is_err()
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    // Windows replacement uses MOVEFILE_WRITE_THROUGH. Opening a directory
    // with std::fs::File is not supported there and returns access denied.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SideEffect;
    use crate::model::{ActorKind, GitOid, OperationId, WorkTarget};
    use tempfile::TempDir;

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "forge".into(),
        }
    }

    fn target() -> WorkTarget {
        WorkTarget::Git(crate::model::GitWorkTarget {
            repo_path: PathBuf::from("/tmp/repo"),
            base_ref: "main".into(),
            base_oid: GitOid::new("a".repeat(40)),
        })
    }

    fn registered(item: &WorkItem) -> EventPayload {
        EventPayload::ItemRegistered {
            item: Box::new(item.clone()),
        }
    }

    #[test]
    fn open_creates_layout_and_schema_version() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        assert!(store.root().join("items").is_dir());
        let version = fs::read_to_string(store.root().join("schema_version")).unwrap();
        assert_eq!(version.trim(), STORE_SCHEMA_VERSION.to_string());
        // Re-open is fine.
        FsWorkStore::open(tmp.path()).unwrap();
    }

    #[test]
    fn append_assigns_monotonic_seq_and_replays() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        let e1 = store.append(&item.id, &actor(), registered(&item)).unwrap();
        let e2 = store
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Draft,
                    to: crate::model::WorkState::Provisioning,
                    reason: None,
                },
            )
            .unwrap();
        assert_eq!(e1.seq, 1);
        assert_eq!(e2.seq, 2);
        let events = store.replay(&item.id).unwrap();
        assert_eq!(events, vec![e1, e2]);
    }

    #[test]
    fn replay_tolerates_truncated_tail() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        // Simulate crash mid-append: garbage at the end of the file.
        let path = store.events_path(&item.id);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"schema_version\":1,\"work_id\":\"wor")
            .unwrap();
        drop(file);
        let events = store.replay(&item.id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, registered(&item));
        // Appending after the truncated tail resumes at seq 2 and truncates
        // the torn bytes so the new record is not glued onto garbage.
        let next = store.append(&item.id, &actor(), registered(&item)).unwrap();
        assert_eq!(next.seq, 2);
        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 2);
        assert!(store.replay(&item.id).unwrap().len() == 2);
    }

    #[test]
    fn recover_tail_rejects_corrupt_middle_record() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        let path = store.events_path(&item.id);
        let original = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{original}not-json\n{original}")).unwrap();
        let err = store.recover_tail(&item.id).unwrap_err();
        assert!(matches!(err, ForgeError::Store(_)));
        // Sequence must not advance past the corruption.
        let err = store
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Draft,
                    to: crate::model::WorkState::Ready,
                    reason: None,
                },
            )
            .unwrap_err();
        assert!(matches!(
            err,
            ForgeError::Store(_) | ForgeError::Conflict(_)
        ));
    }

    #[test]
    fn recover_tail_accepts_complete_final_record_without_newline() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        let event = store.append(&item.id, &actor(), registered(&item)).unwrap();
        let path = store.events_path(&item.id);
        let mut raw = fs::read_to_string(&path).unwrap();
        assert!(raw.ends_with('\n'));
        raw.pop();
        fs::write(&path, &raw).unwrap();
        // Fresh process/open — empty in-memory cache.
        let reopened = FsWorkStore::open(tmp.path()).unwrap();
        let tail = reopened.recover_tail(&item.id).unwrap();
        assert_eq!(tail.last_seq, event.seq);
        assert_eq!(tail.last_offset, raw.len() as u64);
        let next = reopened
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Draft,
                    to: crate::model::WorkState::Ready,
                    reason: None,
                },
            )
            .unwrap();
        assert_eq!(next.seq, 2);
    }

    #[test]
    fn stale_cached_tail_is_refreshed_from_disk_before_append() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        let path = store.events_path(&item.id);
        // Poison the in-memory cache as if a concurrent writer advanced it.
        {
            let mut tails = store.tails.lock().unwrap();
            tails.insert(
                item.id.as_str().to_owned(),
                TailMeta {
                    last_seq: 99,
                    last_offset: fs::metadata(&path).unwrap().len(),
                    last_hash: [9; 32],
                    lease_acquisitions: 0,
                    operations_started: 0,
                },
            );
        }
        // File length still matches the poisoned cache offset, but append_at
        // re-scans disk and fences the bogus sequence.
        let err = store
            .append_at(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Draft,
                    to: crate::model::WorkState::Ready,
                    reason: None,
                },
                100,
            )
            .unwrap_err();
        assert!(matches!(err, ForgeError::Conflict(_)));
        // Torn bytes beyond the cache offset force a disk refresh.
        {
            let mut tails = store.tails.lock().unwrap();
            let len = fs::metadata(&path).unwrap().len();
            tails.insert(
                item.id.as_str().to_owned(),
                TailMeta {
                    last_seq: 1,
                    last_offset: len,
                    last_hash: [1; 32],
                    lease_acquisitions: 0,
                    operations_started: 0,
                },
            );
        }
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"partial\":").unwrap();
        drop(file);
        let tail = store.cached_tail(&item.id).unwrap();
        assert_eq!(tail.last_seq, 1);
        assert!(tail.last_offset < fs::metadata(&path).unwrap().len());
        let next = store
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Draft,
                    to: crate::model::WorkState::Ready,
                    reason: None,
                },
            )
            .unwrap();
        assert_eq!(next.seq, 2);
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            store.recover_tail(&item.id).unwrap().last_offset
        );
    }

    #[test]
    fn concurrent_append_fencing_rejects_duplicate_sequences() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let tmp = TempDir::new().unwrap();
        let store = Arc::new(FsWorkStore::open(tmp.path()).unwrap());
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let store = Arc::clone(&store);
            let work_id = item.id.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                store.append_at(
                    &work_id,
                    &actor(),
                    EventPayload::StateChanged {
                        from: crate::model::WorkState::Draft,
                        to: crate::model::WorkState::Ready,
                        reason: None,
                    },
                    2,
                )
            }));
        }
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        let wins = results.iter().filter(|result| result.is_ok()).count();
        let losses = results.iter().filter(|result| result.is_err()).count();
        assert_eq!(wins, 1);
        assert_eq!(losses, 1);
        assert_eq!(store.replay(&item.id).unwrap().len(), 2);
    }

    #[test]
    fn restart_recovery_reads_disk_not_previous_process_cache() {
        let tmp = TempDir::new().unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        {
            let store = FsWorkStore::open(tmp.path()).unwrap();
            store.append(&item.id, &actor(), registered(&item)).unwrap();
            store
                .append(
                    &item.id,
                    &actor(),
                    EventPayload::StateChanged {
                        from: crate::model::WorkState::Draft,
                        to: crate::model::WorkState::Ready,
                        reason: None,
                    },
                )
                .unwrap();
            let path = store.events_path(&item.id);
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(b"{\"schema_version\":1,").unwrap();
        }
        let reopened = FsWorkStore::open(tmp.path()).unwrap();
        let tail = reopened.recover_tail(&item.id).unwrap();
        assert_eq!(tail.last_seq, 2);
        let next = reopened
            .append(
                &item.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Ready,
                    to: crate::model::WorkState::Draft,
                    reason: Some("repair".into()),
                },
            )
            .unwrap();
        assert_eq!(next.seq, 3);
        assert_eq!(reopened.replay(&item.id).unwrap().len(), 3);
    }

    #[test]
    fn replay_rejects_corrupt_mid_log_line() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        // Corrupt the middle by rewriting the file with a bad first line.
        let path = store.events_path(&item.id);
        let original = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("not-json\n{original}")).unwrap();
        let err = store.replay(&item.id).unwrap_err();
        assert!(matches!(err, ForgeError::Store(_)));
    }

    #[test]
    fn snapshot_round_trip_and_cache_semantics() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        store.write_snapshot(&item, 1).unwrap();
        let back = store.read_snapshot(&item.id).unwrap().unwrap();
        assert_eq!(back.applied_seq, 1);
        assert_eq!(back.item, item);
        // Cache is disposable: delete it, replay still works.
        fs::remove_file(store.snapshot_path(&item.id)).unwrap();
        assert!(store.read_snapshot(&item.id).unwrap().is_none());
        assert_eq!(store.replay(&item.id).unwrap().len(), 1);
    }

    #[test]
    fn snapshot_replacement_preserves_the_latest_complete_cache() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let mut item = WorkItem::new("first", "b", target(), "user-1");

        store.write_snapshot(&item, 1).unwrap();
        item.title = "second".into();
        store.write_snapshot(&item, 2).unwrap();

        let back = store.read_snapshot(&item.id).unwrap().unwrap();
        assert_eq!(back.applied_seq, 2);
        assert_eq!(back.item.title, "second");
        assert!(
            fs::read_dir(store.item_dir(&item.id))
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains(".tmp-"))
        );
    }

    #[test]
    fn operation_side_effect_events_round_trip_through_store() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        let op = OperationId::new();
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationStarted {
                    operation_id: op.clone(),
                    kind: crate::events::OperationKind::Provision,
                    attempt_id: None,
                },
            )
            .unwrap();
        store
            .append(
                &item.id,
                &actor(),
                EventPayload::OperationSideEffect {
                    operation_id: op.clone(),
                    effect: SideEffect::WorktreeAdded {
                        path: PathBuf::from("/tmp/forge/worktrees/r/w"),
                        branch: "worktree/w".into(),
                        baseline_oid: GitOid::new("b".repeat(40)),
                    },
                },
            )
            .unwrap();
        let events = store.replay(&item.id).unwrap();
        assert_eq!(events.len(), 3);
    }

    // ---- corruption matrix + locking ----

    #[test]
    fn replay_rejects_non_monotonic_seq() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        // Rewrite the log with a backwards seq.
        let path = store.events_path(&item.id);
        let line = fs::read_to_string(&path).unwrap();
        let mut event: TransitionEvent = serde_json::from_str(line.trim()).unwrap();
        let first = serde_json::to_string(&event).unwrap();
        event.seq = 1; // duplicate seq — not monotonic
        let second = serde_json::to_string(&event).unwrap();
        fs::write(&path, format!("{first}\n{second}\n")).unwrap();
        let err = store.replay(&item.id).unwrap_err();
        assert!(matches!(err, ForgeError::Store(_)));
    }

    #[test]
    fn open_rejects_garbage_and_newer_schema_versions() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("items")).unwrap();
        fs::write(tmp.path().join("schema_version"), "banana\n").unwrap();
        assert!(FsWorkStore::open(tmp.path()).is_err());

        fs::write(tmp.path().join("schema_version"), "999\n").unwrap();
        assert!(FsWorkStore::open(tmp.path()).is_err());

        fs::write(tmp.path().join("schema_version"), "1\n").unwrap();
        assert!(FsWorkStore::open(tmp.path()).is_ok());
    }

    #[test]
    fn corrupt_snapshot_is_treated_as_absent_cache() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        store.append(&item.id, &actor(), registered(&item)).unwrap();
        fs::create_dir_all(store.item_dir(&item.id)).unwrap();
        fs::write(store.snapshot_path(&item.id), b"{not json").unwrap();
        assert!(store.read_snapshot(&item.id).unwrap().is_none());
        // Replay is unaffected — snapshot was only a cache.
        assert_eq!(store.replay(&item.id).unwrap().len(), 1);
    }

    #[test]
    fn item_lock_excludes_second_holder() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let item = WorkItem::new("t", "b", target(), "user-1");
        let held = store.lock_item(&item.id).unwrap();
        assert!(store.try_lock_item(&item.id).unwrap().is_none());
        drop(held);
        assert!(store.try_lock_item(&item.id).unwrap().is_some());
    }

    #[test]
    fn repo_lock_excludes_second_holder() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let held = store.lock_repo("repo-key").unwrap();
        let digest = crate::model::Digest::sha256_hex(b"repo-key");
        let second = FileLock::try_acquire(
            &store
                .root()
                .join("repos")
                .join(format!("repo1-{}", digest.as_str()))
                .join(".lock"),
        )
        .unwrap();
        assert!(second.is_none());
        drop(held);
    }

    #[test]
    fn list_item_ids_scans_directory() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let a = WorkItem::new("a", "b", target(), "user-1");
        let b = WorkItem::new("b", "b", target(), "user-1");
        store.append(&a.id, &actor(), registered(&a)).unwrap();
        store.append(&b.id, &actor(), registered(&b)).unwrap();
        let ids = store.list_item_ids().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&a.id));
        assert!(ids.contains(&b.id));
    }

    #[test]
    fn hostile_work_ids_cannot_select_store_paths() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path().join("forge")).unwrap();
        let hostile = WorkId::from("../../outside".to_string());
        let path = store.item_dir(&hostile);
        assert_eq!(path.parent(), Some(store.root().join("items").as_path()));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("work1-")
        );
        assert!(!path.to_string_lossy().contains("outside"));
    }
}
