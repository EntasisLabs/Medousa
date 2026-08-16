//! Generation-fenced Forge log compaction (H06.6).
//!
//! Compaction triggers on growth since the last successful run (1,000 events or
//! 8 MiB), publishes an anchored snapshot via H04 `FileTransaction`, optionally
//! seals the compacted prefix into a segment file, and refuses to publish when
//! item/log generation advances mid-run. Crash boundaries use unique staging
//! names and an in-progress marker; restart either resumes cleanup or leaves
//! the prior committed generation intact.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use medousa_store::{DurabilityLevel, FileTransaction, TransactionFaultPoint};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::events::TransitionEvent;
use crate::fold::fold;
use crate::log_v2::{self, LogAuthority, current_store_generation, item_relative};
use crate::model::{WorkId, WorkItem};
use crate::owner::ForgeItemRegistry;
use crate::store::{FsWorkStore, SnapshotEnvelope};

pub const COMPACTION_EVENT_THRESHOLD: u64 = 1_000;
pub const COMPACTION_BYTE_THRESHOLD: u64 = 8 * 1024 * 1024;
/// Events retained after the sealed prefix for snapshot+tail load.
pub const REPLAY_SUFFIX_EVENTS: u64 = 64;
pub const MAX_REPLAY_EVENTS: usize = 100_000;
pub const MAX_REPLAY_BYTES: usize = 64 * 1024 * 1024;

pub const COMPACTION_STATE_NAME: &str = "compaction.json";
pub const COMPACTION_INPROGRESS_PREFIX: &str = "compaction.inprogress-";
pub const SEGMENTS_DIR: &str = "segments";

static COMPACTION_TMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionThresholds {
    pub events: u64,
    pub bytes: u64,
}

impl Default for CompactionThresholds {
    fn default() -> Self {
        Self {
            events: COMPACTION_EVENT_THRESHOLD,
            bytes: COMPACTION_BYTE_THRESHOLD,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionFaultPoint {
    AfterFenceCapture,
    AfterSegmentStage,
    AfterSnapshotStage,
    AfterSnapshotPublish,
    AfterStatePublish,
    AfterCleanup,
}

pub trait CompactionFaults: Send + Sync {
    fn check(&self, _point: CompactionFaultPoint) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct NoCompactionFaults;

impl CompactionFaults for NoCompactionFaults {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionState {
    pub last_compacted_seq: u64,
    pub last_compacted_bytes: u64,
    pub item_generation: u64,
    pub log_generation: u32,
    pub snapshot_applied_seq: u64,
    pub next_log_offset: u64,
    pub anchor_hash: String,
}

impl Default for CompactionState {
    fn default() -> Self {
        Self {
            last_compacted_seq: 0,
            last_compacted_bytes: 0,
            item_generation: 0,
            log_generation: 1,
            snapshot_applied_seq: 0,
            next_log_offset: 0,
            anchor_hash: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionReceipt {
    pub applied_seq: u64,
    pub last_compacted_seq: u64,
    pub last_compacted_bytes: u64,
    pub sealed_segment: bool,
}

fn hex_hash(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn event_payload_hash(event: &TransitionEvent) -> [u8; 32] {
    let encoded = serde_json::to_vec(event).unwrap_or_default();
    let digest = Sha256::digest(&encoded);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub fn compaction_state_path(store: &FsWorkStore, work_id: &WorkId) -> std::path::PathBuf {
    store.item_dir(work_id).join(COMPACTION_STATE_NAME)
}

pub fn read_compaction_state(store: &FsWorkStore, work_id: &WorkId) -> Result<CompactionState> {
    let path = compaction_state_path(store, work_id);
    if !path.exists() {
        return Ok(CompactionState::default());
    }
    let raw = fs::read_to_string(&path)?;
    serde_json::from_str(&raw).map_err(|err| {
        ForgeError::Store(format!(
            "corrupt compaction state at {}: {err}",
            path.display()
        ))
    })
}

fn active_log_bytes(store: &FsWorkStore, work_id: &WorkId) -> Result<u64> {
    match log_v2::select_log_authority(store, work_id)? {
        LogAuthority::V2 => Ok(log_v2::events_v2_path(store, work_id)
            .metadata()
            .map(|meta| meta.len())
            .unwrap_or(0)),
        LogAuthority::V1 => Ok(store
            .events_path(work_id)
            .metadata()
            .map(|meta| meta.len())
            .unwrap_or(0)),
    }
}

pub fn needs_compaction(
    state: &CompactionState,
    current_seq: u64,
    current_bytes: u64,
    thresholds: CompactionThresholds,
) -> bool {
    let event_growth = current_seq.saturating_sub(state.last_compacted_seq);
    let byte_growth = current_bytes.saturating_sub(state.last_compacted_bytes);
    event_growth >= thresholds.events || byte_growth >= thresholds.bytes
}

/// Reject snapshot/log pairs stamped with a different store generation.
pub fn validate_snapshot_log_pair(
    store: &FsWorkStore,
    work_id: &WorkId,
    envelope: &SnapshotEnvelope,
) -> Result<()> {
    let Some(stamped) = envelope.log_generation else {
        return Ok(());
    };
    let current = current_store_generation(store, work_id)?;
    if stamped != current {
        return Err(ForgeError::Store(format!(
            "snapshot/log generation mismatch for {work_id}: snapshot={stamped} log={current}"
        )));
    }
    Ok(())
}

/// Bound decoded records and retained bytes while replaying the authoritative log.
pub fn replay_bounded(store: &FsWorkStore, work_id: &WorkId) -> Result<Vec<TransitionEvent>> {
    let events = store.replay(work_id)?;
    if events.len() > MAX_REPLAY_EVENTS {
        return Err(ForgeError::Overloaded(format!(
            "replay exceeds MAX_REPLAY_EVENTS ({MAX_REPLAY_EVENTS})"
        )));
    }
    let mut bytes = 0usize;
    for event in &events {
        bytes = bytes.saturating_add(serde_json::to_vec(event).map(|v| v.len()).unwrap_or(0));
        if bytes > MAX_REPLAY_BYTES {
            return Err(ForgeError::Overloaded(format!(
                "replay exceeds MAX_REPLAY_BYTES ({MAX_REPLAY_BYTES})"
            )));
        }
    }
    Ok(events)
}

/// Events with `seq > after_seq`, still under the replay budget.
pub fn replay_after(
    store: &FsWorkStore,
    work_id: &WorkId,
    after_seq: u64,
) -> Result<Vec<TransitionEvent>> {
    let events = replay_bounded(store, work_id)?;
    Ok(events
        .into_iter()
        .filter(|event| event.seq > after_seq)
        .collect())
}

fn cleanup_compaction_temps(store: &FsWorkStore, work_id: &WorkId) -> Result<()> {
    let dir = store.item_dir(work_id);
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(COMPACTION_INPROGRESS_PREFIX)
            || name.starts_with("manifest.json.compact-")
            || name.starts_with("compaction.json.tmp-")
        {
            let _ = fs::remove_file(entry.path());
        }
    }
    let segments = dir.join(SEGMENTS_DIR);
    if segments.is_dir() {
        for entry in fs::read_dir(&segments)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.contains(".staging-") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

fn write_inprogress_marker(store: &FsWorkStore, work_id: &WorkId) -> Result<String> {
    let leaf = format!(
        "{COMPACTION_INPROGRESS_PREFIX}{}-{}",
        std::process::id(),
        COMPACTION_TMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let path = item_relative(store, work_id, &leaf)?;
    store
        .store_root()
        .atomic_write(&path, b"in-progress\n")
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    Ok(leaf)
}

fn fold_through(
    events: &[TransitionEvent],
    through_seq: u64,
) -> Result<(WorkItem, u64, [u8; 32], Vec<TransitionEvent>)> {
    let mut prefix = Vec::new();
    let mut offset = 0u64;
    let mut anchor = [0u8; 32];
    for event in events {
        if event.seq > through_seq {
            break;
        }
        let encoded = serde_json::to_vec(event)?;
        // Approximate durable record size (JSONL line + newline, or v2 frame overhead ignored
        // for offset when authority is v1; for fencing we store the scanned JSON size sum).
        offset = offset.saturating_add(encoded.len() as u64 + 1);
        anchor = event_payload_hash(event);
        prefix.push(event.clone());
    }
    if prefix.is_empty() {
        return Err(ForgeError::Store(
            "compaction seal requires at least one event".into(),
        ));
    }
    let item = fold(&prefix)?;
    Ok((item, offset, anchor, prefix))
}

fn stage_segment(
    store: &FsWorkStore,
    work_id: &WorkId,
    seal_seq: u64,
    prefix: &[TransitionEvent],
) -> Result<String> {
    let staging_leaf = format!(
        "{SEGMENTS_DIR}/seg-{seal_seq}.staging-{}-{}",
        std::process::id(),
        COMPACTION_TMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut body = Vec::new();
    match log_v2::select_log_authority(store, work_id)? {
        LogAuthority::V2 => {
            let mut prev = [0u8; 32];
            for event in prefix {
                let frame = log_v2::encode_frame(event, &prev)?;
                let mut next = [0u8; 32];
                next.copy_from_slice(&frame[24..56]);
                prev = next;
                body.extend_from_slice(&frame);
            }
        }
        LogAuthority::V1 => {
            for event in prefix {
                let mut line = serde_json::to_vec(event)?;
                line.push(b'\n');
                body.extend_from_slice(&line);
            }
        }
    }
    let path = item_relative(store, work_id, &staging_leaf)?;
    store
        .store_root()
        .create_dir_all(&item_relative(store, work_id, SEGMENTS_DIR)?)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    store
        .store_root()
        .atomic_write(&path, &body)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    Ok(staging_leaf)
}

fn publish_segment(
    store: &FsWorkStore,
    work_id: &WorkId,
    seal_seq: u64,
    staging_leaf: &str,
) -> Result<()> {
    let staging = item_relative(store, work_id, staging_leaf)?;
    let final_leaf = format!("{SEGMENTS_DIR}/seg-{seal_seq}");
    let final_path = item_relative(store, work_id, &final_leaf)?;
    store
        .transaction()
        .check(TransactionFaultPoint::BeforePublish)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    store
        .store_root()
        .rename(&staging, &final_path)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    store
        .transaction()
        .check(TransactionFaultPoint::AfterPublish)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    Ok(())
}

fn publish_snapshot(
    store: &FsWorkStore,
    work_id: &WorkId,
    envelope: &SnapshotEnvelope,
) -> Result<()> {
    let relative = item_relative(store, work_id, "manifest.json")?;
    let bytes = serde_json::to_vec_pretty(envelope)?;
    store
        .transaction()
        .replace_snapshot(&relative, &bytes, DurabilityLevel::Synced)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    Ok(())
}

fn publish_compaction_state(
    store: &FsWorkStore,
    work_id: &WorkId,
    state: &CompactionState,
) -> Result<()> {
    let relative = item_relative(store, work_id, COMPACTION_STATE_NAME)?;
    let bytes = serde_json::to_vec_pretty(state)?;
    store
        .store_root()
        .atomic_write(&relative, &bytes)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    Ok(())
}

/// Run compaction when growth since the last success crosses the threshold.
pub fn compact_if_needed(
    store: &FsWorkStore,
    owners: &ForgeItemRegistry,
    work_id: &WorkId,
) -> Result<Option<CompactionReceipt>> {
    compact_if_needed_with(
        store,
        owners,
        work_id,
        CompactionThresholds::default(),
        &NoCompactionFaults,
    )
}

pub fn compact_if_needed_with(
    store: &FsWorkStore,
    owners: &ForgeItemRegistry,
    work_id: &WorkId,
    thresholds: CompactionThresholds,
    faults: &dyn CompactionFaults,
) -> Result<Option<CompactionReceipt>> {
    cleanup_compaction_temps(store, work_id)?;

    let prior = read_compaction_state(store, work_id)?;
    let tail = store.recover_tail(work_id)?;
    let current_bytes = active_log_bytes(store, work_id)?;
    if !needs_compaction(&prior, tail.last_seq, current_bytes, thresholds) {
        return Ok(None);
    }
    if tail.last_seq == 0 {
        return Ok(None);
    }

    let handle = owners.get_or_open(store, work_id)?;
    let fence_item_generation = {
        let owner = handle
            .lock()
            .map_err(|_| ForgeError::Store("item owner poisoned".into()))?;
        owner.item_generation
    };
    let fence_log_generation = current_store_generation(store, work_id)?;
    faults.check(CompactionFaultPoint::AfterFenceCapture)?;

    let _marker = write_inprogress_marker(store, work_id)?;

    let events = replay_bounded(store, work_id)?;
    let seal_seq = if tail.last_seq > REPLAY_SUFFIX_EVENTS {
        tail.last_seq - REPLAY_SUFFIX_EVENTS
    } else {
        tail.last_seq
    };
    let (item, next_log_offset, anchor, prefix) = fold_through(&events, seal_seq)?;
    let anchor_hash = hex_hash(&anchor);

    let mut sealed_segment = false;
    let staging_segment = if seal_seq > 0 && seal_seq < tail.last_seq {
        Some(stage_segment(store, work_id, seal_seq, &prefix)?)
    } else if seal_seq == tail.last_seq && !prefix.is_empty() {
        // Still seal a full-prefix archive when the whole log fits in the suffix window.
        Some(stage_segment(store, work_id, seal_seq, &prefix)?)
    } else {
        None
    };
    faults.check(CompactionFaultPoint::AfterSegmentStage)?;

    let envelope = SnapshotEnvelope {
        applied_seq: seal_seq,
        item: item.clone(),
        next_log_offset: Some(next_log_offset),
        anchor_hash: Some(anchor_hash.clone()),
        item_generation: Some(fence_item_generation),
        log_generation: Some(fence_log_generation),
    };
    faults.check(CompactionFaultPoint::AfterSnapshotStage)?;

    // Generation fence: refuse to publish if the item or log moved.
    {
        let owner = handle
            .lock()
            .map_err(|_| ForgeError::Store("item owner poisoned".into()))?;
        if owner.item_generation != fence_item_generation {
            cleanup_compaction_temps(store, work_id)?;
            return Err(ForgeError::Conflict(format!(
                "compaction refused: item generation advanced from {fence_item_generation} to {}",
                owner.item_generation
            )));
        }
    }
    if current_store_generation(store, work_id)? != fence_log_generation {
        cleanup_compaction_temps(store, work_id)?;
        return Err(ForgeError::Conflict(format!(
            "compaction refused: log generation advanced from {fence_log_generation}"
        )));
    }

    if let Some(staging) = staging_segment.as_deref() {
        publish_segment(store, work_id, seal_seq, staging)?;
        sealed_segment = true;
    }

    publish_snapshot(store, work_id, &envelope)?;
    faults.check(CompactionFaultPoint::AfterSnapshotPublish)?;

    let state = CompactionState {
        last_compacted_seq: tail.last_seq,
        last_compacted_bytes: current_bytes,
        item_generation: fence_item_generation,
        log_generation: fence_log_generation,
        snapshot_applied_seq: seal_seq,
        next_log_offset,
        anchor_hash,
    };
    publish_compaction_state(store, work_id, &state)?;
    faults.check(CompactionFaultPoint::AfterStatePublish)?;

    if let Ok(mut owner) = handle.lock() {
        crate::owner::mark_projection_clean(&mut owner, seal_seq);
        let _ = owner.sync_projection(item);
        owner.dirty = false;
    }

    cleanup_compaction_temps(store, work_id)?;
    faults.check(CompactionFaultPoint::AfterCleanup)?;

    Ok(Some(CompactionReceipt {
        applied_seq: seal_seq,
        last_compacted_seq: tail.last_seq,
        last_compacted_bytes: current_bytes,
        sealed_segment,
    }))
}

/// Test helper: install injectable transaction faults on the store.
#[allow(dead_code)]
pub fn with_transaction_faults(
    store: &FsWorkStore,
    faults: Arc<dyn medousa_store::TransactionFaults>,
) -> FileTransaction {
    FileTransaction::with_faults(Arc::clone(store.store_root_arc()), faults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventPayload;
    use crate::fold::apply_payload;
    use crate::model::{
        ActorKind, ActorRef, GitOid, GitWorkTarget, WorkItem, WorkState, WorkTarget,
    };
    use crate::owner::append_owned;
    use std::sync::atomic::AtomicUsize;
    use tempfile::TempDir;

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "compaction-test".into(),
        }
    }

    fn item(title: &str) -> WorkItem {
        WorkItem::new(
            title,
            "brief",
            WorkTarget::Git(GitWorkTarget {
                repo_path: std::path::PathBuf::from("/tmp/compaction-repo"),
                base_ref: "main".into(),
                base_oid: GitOid::new("a".repeat(40)),
            }),
            "user-1",
        )
    }

    fn registered(work: &WorkItem) -> EventPayload {
        EventPayload::ItemRegistered {
            item: Box::new(work.clone()),
        }
    }

    fn seed_events(store: &FsWorkStore, registry: &ForgeItemRegistry, work: &WorkItem, n: u64) {
        append_owned(store, registry, &work.id, &actor(), registered(work), None).unwrap();
        for i in 1..n {
            let from = if i % 2 == 1 {
                WorkState::Draft
            } else {
                WorkState::Ready
            };
            let to = if i % 2 == 1 {
                WorkState::Ready
            } else {
                WorkState::Draft
            };
            append_owned(
                store,
                registry,
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from,
                    to,
                    reason: Some(format!("e{i}")),
                },
                None,
            )
            .unwrap();
        }
    }

    struct FailCompaction {
        target: CompactionFaultPoint,
        hits: AtomicUsize,
    }

    impl CompactionFaults for FailCompaction {
        fn check(&self, point: CompactionFaultPoint) -> Result<()> {
            self.hits.fetch_add(1, Ordering::Relaxed);
            if point == self.target {
                return Err(ForgeError::Store(format!(
                    "injected compaction fault at {point:?}"
                )));
            }
            Ok(())
        }
    }

    #[test]
    fn threshold_tracks_growth_since_last_compaction() {
        let state = CompactionState {
            last_compacted_seq: 1_000,
            last_compacted_bytes: 8 * 1024 * 1024,
            ..CompactionState::default()
        };
        assert!(!needs_compaction(
            &state,
            1_500,
            8 * 1024 * 1024 + 100,
            CompactionThresholds::default()
        ));
        assert!(needs_compaction(
            &state,
            2_000,
            8 * 1024 * 1024,
            CompactionThresholds::default()
        ));
        assert!(needs_compaction(
            &state,
            1_001,
            16 * 1024 * 1024,
            CompactionThresholds::default()
        ));
    }

    #[test]
    fn compaction_publishes_once_until_threshold_recrossed() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("once");
        seed_events(&store, &registry, &work, 20);
        let thresholds = CompactionThresholds {
            events: 10,
            bytes: u64::MAX,
        };
        let first =
            compact_if_needed_with(&store, &registry, &work.id, thresholds, &NoCompactionFaults)
                .unwrap();
        assert!(first.is_some());
        let second =
            compact_if_needed_with(&store, &registry, &work.id, thresholds, &NoCompactionFaults)
                .unwrap();
        assert!(second.is_none());

        // Grow past threshold again.
        for i in 0..12 {
            append_owned(
                &store,
                &registry,
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: WorkState::Ready,
                    reason: Some(format!("more{i}")),
                },
                None,
            )
            .unwrap();
        }
        let third =
            compact_if_needed_with(&store, &registry, &work.id, thresholds, &NoCompactionFaults)
                .unwrap();
        assert!(third.is_some());
    }

    #[test]
    fn compaction_retains_replay_suffix_and_load_uses_snapshot_tail() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("suffix");
        let total = REPLAY_SUFFIX_EVENTS + 20;
        seed_events(&store, &registry, &work, total);
        let receipt = compact_if_needed_with(
            &store,
            &registry,
            &work.id,
            CompactionThresholds {
                events: 10,
                bytes: u64::MAX,
            },
            &NoCompactionFaults,
        )
        .unwrap()
        .expect("compacted");
        assert_eq!(receipt.applied_seq, total - REPLAY_SUFFIX_EVENTS);
        let envelope = store.read_snapshot(&work.id).unwrap().unwrap();
        assert_eq!(envelope.applied_seq, receipt.applied_seq);
        assert!(envelope.next_log_offset.is_some());
        assert!(envelope.anchor_hash.is_some());
        assert_eq!(envelope.log_generation, Some(1));

        let tail_events = replay_after(&store, &work.id, envelope.applied_seq).unwrap();
        assert_eq!(tail_events.len() as u64, REPLAY_SUFFIX_EVENTS);
        let mut item = envelope.item;
        for event in &tail_events {
            apply_payload(&mut item, event).unwrap();
        }
        let full = fold(&store.replay(&work.id).unwrap()).unwrap();
        assert_eq!(item.state, full.state);
        assert_eq!(item.id, full.id);
    }

    #[test]
    fn mismatched_snapshot_log_generation_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("mismatch");
        seed_events(&store, &registry, &work, 12);
        compact_if_needed_with(
            &store,
            &registry,
            &work.id,
            CompactionThresholds {
                events: 5,
                bytes: u64::MAX,
            },
            &NoCompactionFaults,
        )
        .unwrap();
        let mut envelope = store.read_snapshot(&work.id).unwrap().unwrap();
        envelope.log_generation = Some(99);
        let err = validate_snapshot_log_pair(&store, &work.id, &envelope).unwrap_err();
        assert!(err.to_string().contains("generation mismatch"));
    }

    #[test]
    fn generation_fence_refuses_when_owner_advances_mid_compaction() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = Arc::new(ForgeItemRegistry::new());
        let work = item("fence2");
        seed_events(&store, registry.as_ref(), &work, 15);

        struct AdvanceOnStage {
            store_root: std::path::PathBuf,
            registry: Arc<ForgeItemRegistry>,
            work_id: WorkId,
        }

        impl CompactionFaults for AdvanceOnStage {
            fn check(&self, point: CompactionFaultPoint) -> Result<()> {
                if point == CompactionFaultPoint::AfterSnapshotStage {
                    let store = FsWorkStore::open(&self.store_root).unwrap();
                    append_owned(
                        &store,
                        self.registry.as_ref(),
                        &self.work_id,
                        &actor(),
                        EventPayload::StateChanged {
                            from: WorkState::Draft,
                            to: WorkState::Ready,
                            reason: Some("race".into()),
                        },
                        None,
                    )?;
                }
                Ok(())
            }
        }

        let faults = AdvanceOnStage {
            store_root: tmp.path().to_path_buf(),
            registry: Arc::clone(&registry),
            work_id: work.id.clone(),
        };
        let err = compact_if_needed_with(
            &store,
            registry.as_ref(),
            &work.id,
            CompactionThresholds {
                events: 5,
                bytes: u64::MAX,
            },
            &faults,
        )
        .unwrap_err();
        assert!(matches!(err, ForgeError::Conflict(_)));
        // Prior committed compaction state should remain absent (never published).
        let state = read_compaction_state(&store, &work.id).unwrap();
        assert_eq!(state.last_compacted_seq, 0);
    }

    #[test]
    fn compaction_is_restartable_at_every_fault_boundary() {
        let points = [
            CompactionFaultPoint::AfterFenceCapture,
            CompactionFaultPoint::AfterSegmentStage,
            CompactionFaultPoint::AfterSnapshotStage,
            CompactionFaultPoint::AfterSnapshotPublish,
            CompactionFaultPoint::AfterStatePublish,
            CompactionFaultPoint::AfterCleanup,
        ];
        for point in points {
            let tmp = TempDir::new().unwrap();
            let store = FsWorkStore::open(tmp.path()).unwrap();
            let registry = ForgeItemRegistry::new();
            let work = item(&format!("fault-{point:?}"));
            seed_events(&store, &registry, &work, 18);
            let thresholds = CompactionThresholds {
                events: 5,
                bytes: u64::MAX,
            };
            let faults = FailCompaction {
                target: point,
                hits: AtomicUsize::new(0),
            };
            let err = compact_if_needed_with(&store, &registry, &work.id, thresholds, &faults)
                .unwrap_err();
            assert!(err.to_string().contains("injected compaction fault"));

            match point {
                CompactionFaultPoint::AfterStatePublish | CompactionFaultPoint::AfterCleanup => {
                    let state = read_compaction_state(&store, &work.id).unwrap();
                    assert!(state.last_compacted_seq > 0);
                }
                CompactionFaultPoint::AfterSnapshotPublish => {
                    // Snapshot may exist, but cursor unpublished — restart must finish.
                    assert_eq!(
                        read_compaction_state(&store, &work.id)
                            .unwrap()
                            .last_compacted_seq,
                        0
                    );
                }
                _ => {
                    assert_eq!(
                        read_compaction_state(&store, &work.id)
                            .unwrap()
                            .last_compacted_seq,
                        0
                    );
                }
            }

            let receipt = compact_if_needed_with(
                &store,
                &registry,
                &work.id,
                thresholds,
                &NoCompactionFaults,
            )
            .unwrap();
            match point {
                CompactionFaultPoint::AfterStatePublish | CompactionFaultPoint::AfterCleanup => {
                    // Already committed — no repeated work until growth.
                    assert!(receipt.is_none());
                }
                _ => {
                    assert!(receipt.is_some());
                }
            }
            let envelope = store.read_snapshot(&work.id).unwrap().unwrap();
            validate_snapshot_log_pair(&store, &work.id, &envelope).unwrap();
        }
    }

    #[test]
    fn snapshot_transaction_failpoint_leaves_prior_manifest() {
        use medousa_store::{PersistenceError, PersistenceErrorKind, TransactionFaults};

        let tmp = TempDir::new().unwrap();
        let mut store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("tx-fault");
        seed_events(&store, &registry, &work, 12);
        // Seed a prior manifest.
        store.write_snapshot(&work, 1).unwrap();

        struct FailPublish;
        impl TransactionFaults for FailPublish {
            fn check(
                &self,
                point: TransactionFaultPoint,
            ) -> std::result::Result<(), PersistenceError> {
                if point == TransactionFaultPoint::BeforeSnapshotPublish {
                    return Err(PersistenceError::new(
                        PersistenceErrorKind::RetryableIo,
                        "injected snapshot publish fault",
                    ));
                }
                Ok(())
            }
        }
        store.set_transaction(FileTransaction::with_faults(
            Arc::clone(store.store_root_arc()),
            Arc::new(FailPublish),
        ));

        let err = compact_if_needed_with(
            &store,
            &registry,
            &work.id,
            CompactionThresholds {
                events: 5,
                bytes: u64::MAX,
            },
            &NoCompactionFaults,
        )
        .unwrap_err();
        assert!(err.to_string().contains("injected snapshot publish fault"));
        let envelope = store.read_snapshot(&work.id).unwrap().unwrap();
        assert_eq!(envelope.applied_seq, 1);
    }

    #[test]
    fn restart_cleans_stranded_inprogress_markers() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let registry = ForgeItemRegistry::new();
        let work = item("restart");
        seed_events(&store, &registry, &work, 12);
        let marker = store
            .item_dir(&work.id)
            .join(format!("{COMPACTION_INPROGRESS_PREFIX}dead-1"));
        fs::create_dir_all(store.item_dir(&work.id)).unwrap();
        fs::write(&marker, b"stale").unwrap();
        compact_if_needed_with(
            &store,
            &registry,
            &work.id,
            CompactionThresholds {
                events: 5,
                bytes: u64::MAX,
            },
            &NoCompactionFaults,
        )
        .unwrap();
        assert!(!marker.exists());
    }
}
