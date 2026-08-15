//! Framed Forge log v2 with snapshot anchors and crash-safe v1 migration (H06.5).
//!
//! Authority is selected by the per-item `store_generation` marker:
//! - absent / `1` → v1 JSONL is authoritative (migration staging is ignored)
//! - `2` → framed `events.v2` is authoritative; v1 is retained read-only
//!
//! Partial final frames at EOF are tolerated. Middle-frame corruption fails closed.
//! Consecutive frames optionally chain via `prev_hash` (previous payload checksum).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use medousa_store::{
    DurabilityLevel, FileTransaction, PersistenceError, StorePath, TransactionFaultPoint,
    TransactionFaults,
};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::events::TransitionEvent;
use crate::model::WorkId;
use crate::store::{FsWorkStore, STORE_SCHEMA_VERSION, SnapshotEnvelope, TailMeta};

pub const LOG_V2_MAGIC: &[u8; 8] = b"FRGLOG02";
pub const LOG_V2_SCHEMA: u32 = 2;
/// magic(8) + schema(4) + seq(8) + payload_len(4) + checksum(32) + prev_hash(32)
pub const FRAME_HEADER_LEN: usize = 88;
pub const GENERATION_MARKER_NAME: &str = "store_generation";
pub const EVENTS_V2_NAME: &str = "events.v2";

static MIGRATION_TMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogAuthority {
    /// Serve and append v1 JSONL. Any `events.v2` staging is non-authoritative.
    V1,
    /// Serve and append framed v2. v1 remains read-only rollback data.
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationFaultPoint {
    AfterV1Validate,
    AfterV2StagingWrite,
    AfterEquivalenceCheck,
    AfterV2Publish,
    AfterMarkerPublish,
    AfterCleanup,
}

pub trait MigrationFaults: Send + Sync {
    fn check(&self, _point: MigrationFaultPoint) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct NoMigrationFaults;

impl MigrationFaults for NoMigrationFaults {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFrameHeader {
    pub schema: u32,
    pub seq: u64,
    pub payload_len: u32,
    pub checksum: [u8; 32],
    pub prev_hash: [u8; 32],
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEnvelopeV2 {
    pub schema_version: u32,
    pub applied_seq: u64,
    pub next_log_offset: u64,
    pub anchor_hash: String,
    pub item_generation: u64,
    pub item: crate::model::WorkItem,
}

#[derive(Debug)]
enum FrameDecodeError {
    /// Not enough bytes for a complete frame — valid only at EOF.
    Partial,
    Corrupt(String),
}

pub fn events_v2_path(store: &FsWorkStore, work_id: &WorkId) -> PathBuf {
    store.item_dir(work_id).join(EVENTS_V2_NAME)
}

pub fn generation_marker_path(store: &FsWorkStore, work_id: &WorkId) -> PathBuf {
    store.item_dir(work_id).join(GENERATION_MARKER_NAME)
}

pub fn item_relative(store: &FsWorkStore, work_id: &WorkId, leaf: &str) -> Result<StorePath> {
    let abs = store.item_dir(work_id).join(leaf);
    let rel = abs.strip_prefix(store.root()).map_err(|_| {
        ForgeError::Store(format!(
            "item path {} escapes forge root {}",
            abs.display(),
            store.root().display()
        ))
    })?;
    let text = rel
        .to_str()
        .ok_or_else(|| ForgeError::Store(format!("non-utf8 store path {}", rel.display())))?;
    // StorePath requires `/` separators.
    let normalized = text.replace('\\', "/");
    StorePath::parse(&normalized).map_err(|err| ForgeError::Store(err.to_string()))
}

pub fn current_store_generation(store: &FsWorkStore, work_id: &WorkId) -> u32 {
    read_generation_marker(store, work_id).unwrap_or(STORE_SCHEMA_VERSION)
}

fn read_generation_marker(store: &FsWorkStore, work_id: &WorkId) -> Option<u32> {
    fs::read_to_string(generation_marker_path(store, work_id))
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
}

/// Authoritative reader/writer selection from the generation marker and on-disk
/// leftovers. Migration-in-progress and interrupted staging remain on v1 until
/// the marker publishes `2`.
pub fn select_log_authority(store: &FsWorkStore, work_id: &WorkId) -> Result<LogAuthority> {
    match read_generation_marker(store, work_id) {
        None | Some(1) => Ok(LogAuthority::V1),
        Some(2) => Ok(LogAuthority::V2),
        Some(other) => Err(ForgeError::Store(format!(
            "unsupported store_generation {other} for {}",
            work_id
        ))),
    }
}

/// Remove stranded `events.v2.mig-*` staging files. Safe under any authority.
pub fn cleanup_migration_staging(store: &FsWorkStore, work_id: &WorkId) -> Result<()> {
    cleanup_migration_staging_inner(store, work_id)
}

fn cleanup_migration_staging_inner(store: &FsWorkStore, work_id: &WorkId) -> Result<()> {
    let dir = store.item_dir(work_id);
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("events.v2.mig-") {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

pub fn encode_frame(event: &TransitionEvent, prev_hash: &[u8; 32]) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(event)?;
    let checksum = Sha256::digest(&payload);
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(LOG_V2_MAGIC);
    frame.extend_from_slice(&LOG_V2_SCHEMA.to_le_bytes());
    frame.extend_from_slice(&event.seq.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&checksum);
    frame.extend_from_slice(prev_hash);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn decode_frame_strict(
    bytes: &[u8],
) -> std::result::Result<(LogFrameHeader, TransitionEvent, usize), FrameDecodeError> {
    if bytes.len() < FRAME_HEADER_LEN {
        return Err(FrameDecodeError::Partial);
    }
    if &bytes[..8] != LOG_V2_MAGIC {
        return Err(FrameDecodeError::Corrupt("invalid v2 magic".into()));
    }
    let schema = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if schema != LOG_V2_SCHEMA {
        return Err(FrameDecodeError::Corrupt(format!(
            "unsupported v2 frame schema {schema}"
        )));
    }
    let seq = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
    let payload_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&bytes[24..56]);
    let mut prev_hash = [0u8; 32];
    prev_hash.copy_from_slice(&bytes[56..88]);
    let end = FRAME_HEADER_LEN + payload_len;
    if bytes.len() < end {
        return Err(FrameDecodeError::Partial);
    }
    let payload = &bytes[FRAME_HEADER_LEN..end];
    if Sha256::digest(payload).as_slice() != checksum {
        return Err(FrameDecodeError::Corrupt(
            "v2 frame checksum mismatch".into(),
        ));
    }
    let event: TransitionEvent = serde_json::from_slice(payload)
        .map_err(|err| FrameDecodeError::Corrupt(format!("v2 frame payload decode: {err}")))?;
    if event.seq != seq {
        return Err(FrameDecodeError::Corrupt(
            "v2 frame sequence mismatch".into(),
        ));
    }
    Ok((
        LogFrameHeader {
            schema,
            seq,
            payload_len: payload_len as u32,
            checksum,
            prev_hash,
        },
        event,
        end,
    ))
}

pub fn decode_frame(bytes: &[u8]) -> Result<(LogFrameHeader, TransitionEvent, usize)> {
    match decode_frame_strict(bytes) {
        Ok(ok) => Ok(ok),
        Err(FrameDecodeError::Partial) => Err(ForgeError::Store("partial v2 frame".into())),
        Err(FrameDecodeError::Corrupt(msg)) => Err(ForgeError::Store(msg)),
    }
}

fn scan_v2_bytes(path: &Path, bytes: &[u8]) -> Result<(Vec<TransitionEvent>, TailMeta)> {
    let mut events = Vec::new();
    let mut offset = 0usize;
    let mut previous_seq = 0u64;
    let mut expected_prev = [0u8; 32];
    let mut last_seq = 0u64;
    let mut last_hash = [0u8; 32];
    let mut last_offset = 0u64;
    let mut lease_acquisitions = 0u64;
    let mut operations_started = 0u64;

    while offset < bytes.len() {
        match decode_frame_strict(&bytes[offset..]) {
            Ok((header, event, consumed)) => {
                if header.prev_hash != expected_prev {
                    return Err(ForgeError::Store(format!(
                        "v2 frame chain break at {} offset {offset}",
                        path.display()
                    )));
                }
                if event.seq <= previous_seq && previous_seq != 0 {
                    return Err(ForgeError::Store(format!(
                        "non-monotonic v2 seq at {} ({} then {})",
                        path.display(),
                        previous_seq,
                        event.seq
                    )));
                }
                previous_seq = event.seq;
                last_seq = event.seq;
                last_hash = header.checksum;
                expected_prev = header.checksum;
                last_offset = (offset + consumed) as u64;
                if matches!(
                    event.payload,
                    crate::events::EventPayload::LeaseAcquired { .. }
                ) {
                    lease_acquisitions += 1;
                }
                if matches!(
                    event.payload,
                    crate::events::EventPayload::OperationStarted { .. }
                ) {
                    operations_started += 1;
                }
                events.push(event);
                offset += consumed;
            }
            Err(FrameDecodeError::Partial) => {
                // Incomplete final frame at EOF only.
                break;
            }
            Err(FrameDecodeError::Corrupt(msg)) => {
                return Err(ForgeError::Store(format!(
                    "corrupt v2 frame at {} offset {offset}: {msg}",
                    path.display()
                )));
            }
        }
    }

    Ok((
        events,
        TailMeta {
            last_seq,
            last_offset,
            last_hash,
            lease_acquisitions,
            operations_started,
        },
    ))
}

pub fn replay_v2(path: &Path) -> Result<Vec<TransitionEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(path)?;
    Ok(scan_v2_bytes(path, &bytes)?.0)
}

pub fn recover_tail_v2(path: &Path) -> Result<TailMeta> {
    if !path.exists() {
        return Ok(TailMeta {
            last_seq: 0,
            last_offset: 0,
            last_hash: [0; 32],
            lease_acquisitions: 0,
            operations_started: 0,
        });
    }
    let bytes = fs::read(path)?;
    Ok(scan_v2_bytes(path, &bytes)?.1)
}

pub fn append_v2_frame_at(
    store: &FsWorkStore,
    work_id: &WorkId,
    event: &TransitionEvent,
    prev_hash: &[u8; 32],
    durability: DurabilityLevel,
) -> Result<usize> {
    let path = item_relative(store, work_id, EVENTS_V2_NAME)?;
    let frame = encode_frame(event, prev_hash)?;
    let item_dir = store.item_dir(work_id);
    let item_key = item_dir.strip_prefix(store.root()).map_err(|_| {
        ForgeError::Store(format!(
            "item path {} escapes forge root {}",
            item_dir.display(),
            store.root().display()
        ))
    })?;
    let item_rel = item_key
        .to_str()
        .ok_or_else(|| ForgeError::Store(format!("non-utf8 item path {}", item_key.display())))?;
    let item_path = StorePath::parse(&item_rel.replace('\\', "/"))
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    store
        .store_root()
        .create_dir_all(&item_path)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    let sync = matches!(durability, DurabilityLevel::Synced);
    store
        .transaction()
        .check(TransactionFaultPoint::BeforeAppend)
        .map_err(persistence_err)?;
    store
        .store_root()
        .append_durable(&path, &frame, sync)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    store
        .transaction()
        .check(TransactionFaultPoint::AfterAppend)
        .map_err(persistence_err)?;
    Ok(frame.len())
}

/// Dual-read / single-write v1 → v2 migration. v1 remains read-only after commit.
/// Restartable after every fault boundary; staging uses unique temporary names.
pub fn migrate_item_to_v2(store: &FsWorkStore, work_id: &WorkId) -> Result<u64> {
    migrate_item_to_v2_with_faults(store, work_id, &NoMigrationFaults)
}

pub fn migrate_item_to_v2_with_faults(
    store: &FsWorkStore,
    work_id: &WorkId,
    faults: &dyn MigrationFaults,
) -> Result<u64> {
    // Interrupted cleanup / already committed.
    if select_log_authority(store, work_id)? == LogAuthority::V2 {
        cleanup_migration_staging_inner(store, work_id)?;
        faults.check(MigrationFaultPoint::AfterCleanup)?;
        let tail = recover_tail_v2(&events_v2_path(store, work_id))?;
        return Ok(tail.last_seq);
    }

    cleanup_migration_staging_inner(store, work_id)?;

    // Validate v1 under exclusive ownership of this caller's item lock (caller).
    let v1_events = store.replay_v1(work_id)?;
    faults.check(MigrationFaultPoint::AfterV1Validate)?;

    let mut frames = Vec::new();
    let mut prev = [0u8; 32];
    for event in &v1_events {
        let frame = encode_frame(event, &prev)?;
        let checksum = {
            let mut out = [0u8; 32];
            out.copy_from_slice(&frame[24..56]);
            out
        };
        prev = checksum;
        frames.extend_from_slice(&frame);
    }

    fs::create_dir_all(store.item_dir(work_id))?;
    let staging_leaf = format!(
        "events.v2.mig-{}-{}",
        std::process::id(),
        MIGRATION_TMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let staging_path = item_relative(store, work_id, &staging_leaf)?;
    let staging_abs = store.item_dir(work_id).join(&staging_leaf);

    // Write staging via StoreRoot create + sync, then publish with rename.
    store
        .store_root()
        .atomic_write(&staging_path, &frames)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    faults.check(MigrationFaultPoint::AfterV2StagingWrite)?;

    let staged = fs::read(&staging_abs)?;
    let (v2_events, _) = scan_v2_bytes(&staging_abs, &staged)?;
    if !events_semantically_equal(&v1_events, &v2_events) {
        let _ = fs::remove_file(&staging_abs);
        return Err(ForgeError::Store(format!(
            "v1/v2 semantic mismatch during migration of {work_id}"
        )));
    }
    faults.check(MigrationFaultPoint::AfterEquivalenceCheck)?;

    let final_path = item_relative(store, work_id, EVENTS_V2_NAME)?;
    store
        .transaction()
        .check(TransactionFaultPoint::BeforePublish)
        .map_err(persistence_err)?;
    store
        .store_root()
        .rename(&staging_path, &final_path)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    sync_item_dir(store, work_id)?;
    faults.check(MigrationFaultPoint::AfterV2Publish)?;
    store
        .transaction()
        .check(TransactionFaultPoint::AfterPublish)
        .map_err(persistence_err)?;

    // Atomically publish generation marker — authority switch.
    let marker = item_relative(store, work_id, GENERATION_MARKER_NAME)?;
    let marker_bytes = format!("{LOG_V2_SCHEMA}\n").into_bytes();
    store
        .transaction()
        .check(TransactionFaultPoint::BeforeSnapshotPublish)
        .map_err(persistence_err)?;
    store
        .store_root()
        .atomic_write(&marker, &marker_bytes)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
    sync_item_dir(store, work_id)?;
    faults.check(MigrationFaultPoint::AfterMarkerPublish)?;
    store
        .transaction()
        .check(TransactionFaultPoint::AfterSnapshotPublish)
        .map_err(persistence_err)?;

    cleanup_migration_staging_inner(store, work_id)?;
    faults.check(MigrationFaultPoint::AfterCleanup)?;

    store.invalidate_tail(work_id);
    Ok(v1_events.last().map(|event| event.seq).unwrap_or(0))
}

fn events_semantically_equal(v1: &[TransitionEvent], v2: &[TransitionEvent]) -> bool {
    if v1.len() != v2.len() {
        return false;
    }
    v1.iter().zip(v2.iter()).all(|(a, b)| {
        a.seq == b.seq
            && a.work_id == b.work_id
            && a.schema_version == b.schema_version
            && a.payload == b.payload
            && a.actor == b.actor
    })
}

fn sync_item_dir(store: &FsWorkStore, work_id: &WorkId) -> Result<()> {
    #[cfg(unix)]
    {
        use std::fs::File;
        File::open(store.item_dir(work_id))?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = (store, work_id);
    }
    Ok(())
}

fn persistence_err(err: PersistenceError) -> ForgeError {
    ForgeError::Store(err.to_string())
}

pub fn write_snapshot_v2(
    transaction: &FileTransaction,
    relative: &str,
    envelope: &SnapshotEnvelopeV2,
    durability: DurabilityLevel,
) -> Result<usize> {
    let path = StorePath::parse(relative).map_err(|err| ForgeError::Store(err.to_string()))?;
    let bytes = serde_json::to_vec(envelope)?;
    transaction
        .replace_snapshot(&path, &bytes, durability)
        .map_err(|err| ForgeError::Store(err.to_string()))
}

pub fn v1_snapshot_from_v2(envelope: SnapshotEnvelopeV2) -> SnapshotEnvelope {
    SnapshotEnvelope {
        applied_seq: envelope.applied_seq,
        item: envelope.item,
    }
}

/// Test helper: install injectable transaction faults on the store.
pub fn with_transaction_faults(
    store: &FsWorkStore,
    faults: Arc<dyn TransactionFaults>,
) -> FileTransaction {
    FileTransaction::with_faults(Arc::clone(store.store_root_arc()), faults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventPayload;
    use crate::model::{ActorKind, ActorRef, GitOid, GitWorkTarget, WorkItem, WorkTarget};
    use std::sync::atomic::AtomicUsize;
    use tempfile::TempDir;

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "forge".into(),
        }
    }

    fn item() -> WorkItem {
        WorkItem::new(
            "t",
            "b",
            WorkTarget::Git(GitWorkTarget {
                repo_path: PathBuf::from("/tmp/repo"),
                base_ref: "main".into(),
                base_oid: GitOid::new("a".repeat(40)),
            }),
            "user-1",
        )
    }

    struct FailMigration {
        target: MigrationFaultPoint,
        hits: AtomicUsize,
    }

    impl MigrationFaults for FailMigration {
        fn check(&self, point: MigrationFaultPoint) -> Result<()> {
            self.hits.fetch_add(1, Ordering::Relaxed);
            if point == self.target {
                return Err(ForgeError::Store(format!(
                    "injected migration fault at {point:?}"
                )));
            }
            Ok(())
        }
    }

    #[test]
    fn frame_round_trip_chain_and_partial_tail() {
        let work = item();
        let event = TransitionEvent::new(
            work.id.clone(),
            1,
            actor(),
            EventPayload::ItemRegistered {
                item: Box::new(work),
            },
        );
        let mut bytes = encode_frame(&event, &[0; 32]).unwrap();
        let (header, decoded, consumed) = decode_frame(&bytes).unwrap();
        assert_eq!(decoded.seq, 1);
        assert_eq!(consumed, bytes.len());
        assert_eq!(header.prev_hash, [0; 32]);
        bytes.extend_from_slice(b"partial");
        let path = Path::new("memory");
        let (events, tail) = scan_v2_bytes(path, &bytes).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(tail.last_seq, 1);
        assert_eq!(tail.last_offset, consumed as u64);
    }

    #[test]
    fn middle_frame_corruption_fails_closed() {
        let work = item();
        let e1 = TransitionEvent::new(
            work.id.clone(),
            1,
            actor(),
            EventPayload::ItemRegistered {
                item: Box::new(work.clone()),
            },
        );
        let e2 = TransitionEvent::new(
            work.id.clone(),
            2,
            actor(),
            EventPayload::StateChanged {
                from: crate::model::WorkState::Draft,
                to: crate::model::WorkState::Ready,
                reason: None,
            },
        );
        let mut bytes = encode_frame(&e1, &[0; 32]).unwrap();
        let prev = {
            let mut h = [0u8; 32];
            h.copy_from_slice(&bytes[24..56]);
            h
        };
        let mut second = encode_frame(&e2, &prev).unwrap();
        // Flip a payload byte but keep length — checksum must fail.
        let payload_index = FRAME_HEADER_LEN;
        second[payload_index] ^= 0xff;
        bytes.extend_from_slice(&second);
        // Append another valid-looking trailing frame so this is not EOF-only.
        let mut third = encode_frame(&e2, &prev).unwrap();
        third[12..20].copy_from_slice(&3u64.to_le_bytes());
        bytes.extend_from_slice(&third);
        let err = scan_v2_bytes(Path::new("memory"), &bytes).unwrap_err();
        assert!(matches!(err, ForgeError::Store(_)));
    }

    #[test]
    fn chain_break_is_rejected() {
        let work = item();
        let e1 = TransitionEvent::new(
            work.id.clone(),
            1,
            actor(),
            EventPayload::ItemRegistered {
                item: Box::new(work.clone()),
            },
        );
        let e2 = TransitionEvent::new(
            work.id.clone(),
            2,
            actor(),
            EventPayload::StateChanged {
                from: crate::model::WorkState::Draft,
                to: crate::model::WorkState::Ready,
                reason: None,
            },
        );
        let mut bytes = encode_frame(&e1, &[0; 32]).unwrap();
        bytes.extend_from_slice(&encode_frame(&e2, &[9; 32]).unwrap());
        let err = scan_v2_bytes(Path::new("memory"), &bytes).unwrap_err();
        assert!(err.to_string().contains("chain break"));
    }

    #[test]
    fn migrate_writes_v2_and_switches_authority() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let work = item();
        store
            .append(
                &work.id,
                &actor(),
                EventPayload::ItemRegistered {
                    item: Box::new(work.clone()),
                },
            )
            .unwrap();
        assert_eq!(
            select_log_authority(&store, &work.id).unwrap(),
            LogAuthority::V1
        );
        let seq = migrate_item_to_v2(&store, &work.id).unwrap();
        assert_eq!(seq, 1);
        assert!(events_v2_path(&store, &work.id).exists());
        assert!(store.events_path(&work.id).exists());
        assert_eq!(current_store_generation(&store, &work.id), LOG_V2_SCHEMA);
        assert_eq!(
            select_log_authority(&store, &work.id).unwrap(),
            LogAuthority::V2
        );
        let replayed = store.replay(&work.id).unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].seq, 1);
    }

    #[test]
    fn production_append_and_replay_use_v2_after_migration() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let work = item();
        store
            .append(
                &work.id,
                &actor(),
                EventPayload::ItemRegistered {
                    item: Box::new(work.clone()),
                },
            )
            .unwrap();
        migrate_item_to_v2(&store, &work.id).unwrap();
        let next = store
            .append(
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from: crate::model::WorkState::Draft,
                    to: crate::model::WorkState::Ready,
                    reason: None,
                },
            )
            .unwrap();
        assert_eq!(next.seq, 2);
        // New writes must land in v2, not extend v1.
        let v1_lines = fs::read_to_string(store.events_path(&work.id))
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        assert_eq!(v1_lines, 1);
        let events = store.replay(&work.id).unwrap();
        assert_eq!(events.len(), 2);
        let tail = store.recover_tail(&work.id).unwrap();
        assert_eq!(tail.last_seq, 2);
    }

    #[test]
    fn migration_is_restartable_at_every_boundary() {
        let points = [
            MigrationFaultPoint::AfterV1Validate,
            MigrationFaultPoint::AfterV2StagingWrite,
            MigrationFaultPoint::AfterEquivalenceCheck,
            MigrationFaultPoint::AfterV2Publish,
            MigrationFaultPoint::AfterMarkerPublish,
            MigrationFaultPoint::AfterCleanup,
        ];
        for point in points {
            let tmp = TempDir::new().unwrap();
            let store = FsWorkStore::open(tmp.path()).unwrap();
            let work = item();
            store
                .append(
                    &work.id,
                    &actor(),
                    EventPayload::ItemRegistered {
                        item: Box::new(work.clone()),
                    },
                )
                .unwrap();
            let faults = FailMigration {
                target: point,
                hits: AtomicUsize::new(0),
            };
            let err = migrate_item_to_v2_with_faults(&store, &work.id, &faults).unwrap_err();
            assert!(err.to_string().contains("injected migration fault"));

            // Authority must remain v1 until the marker publishes, except when
            // the fault is after marker publish (then v2) or after cleanup.
            match point {
                MigrationFaultPoint::AfterMarkerPublish | MigrationFaultPoint::AfterCleanup => {
                    assert_eq!(
                        select_log_authority(&store, &work.id).unwrap(),
                        LogAuthority::V2
                    );
                }
                _ => {
                    assert_eq!(
                        select_log_authority(&store, &work.id).unwrap(),
                        LogAuthority::V1
                    );
                    // Readers still serve v1 while migration is interrupted.
                    assert_eq!(store.replay(&work.id).unwrap().len(), 1);
                }
            }

            // Restart completes (or no-ops if already committed).
            let seq = migrate_item_to_v2(&store, &work.id).unwrap();
            assert_eq!(seq, 1);
            assert_eq!(
                select_log_authority(&store, &work.id).unwrap(),
                LogAuthority::V2
            );
            assert_eq!(store.replay(&work.id).unwrap().len(), 1);
            // No stranded fixed tmp name.
            let dir = store.item_dir(&work.id);
            for entry in fs::read_dir(&dir).unwrap() {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                assert_ne!(name, "events.v2.tmp");
                assert!(!name.ends_with(".v2.tmp"));
            }
        }
    }

    #[test]
    fn interrupted_cleanup_selects_v2_and_removes_staging() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let work = item();
        store
            .append(
                &work.id,
                &actor(),
                EventPayload::ItemRegistered {
                    item: Box::new(work.clone()),
                },
            )
            .unwrap();
        migrate_item_to_v2(&store, &work.id).unwrap();
        let leftover = store.item_dir(&work.id).join("events.v2.mig-leftover");
        fs::write(&leftover, b"junk").unwrap();
        assert_eq!(
            select_log_authority(&store, &work.id).unwrap(),
            LogAuthority::V2
        );
        migrate_item_to_v2(&store, &work.id).unwrap();
        assert!(!leftover.exists());
    }
}
