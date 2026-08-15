//! Framed Forge log v2 with snapshot anchors and v1 migration (H06.2B).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use medousa_store::{DurabilityLevel, FileTransaction, StorePath};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::events::TransitionEvent;
use crate::model::WorkId;
use crate::store::{FsWorkStore, SnapshotEnvelope, STORE_SCHEMA_VERSION};

pub const LOG_V2_MAGIC: &[u8; 8] = b"FRGLOG02";
pub const LOG_V2_SCHEMA: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFrameHeader {
    pub seq: u64,
    pub payload_len: u32,
    pub checksum: [u8; 32],
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

pub fn events_v2_path(store: &FsWorkStore, work_id: &WorkId) -> std::path::PathBuf {
    store.item_dir(work_id).join("events.v2")
}

pub fn encode_frame(event: &TransitionEvent) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(event)?;
    let checksum = Sha256::digest(&payload);
    let mut frame = Vec::with_capacity(8 + 8 + 4 + 32 + payload.len());
    frame.extend_from_slice(LOG_V2_MAGIC);
    frame.extend_from_slice(&event.seq.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&checksum);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_frame(bytes: &[u8]) -> Result<(LogFrameHeader, TransitionEvent, usize)> {
    if bytes.len() < 52 {
        return Err(ForgeError::Store("truncated v2 frame".into()));
    }
    if &bytes[..8] != LOG_V2_MAGIC {
        return Err(ForgeError::Store("invalid v2 magic".into()));
    }
    let seq = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let payload_len = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&bytes[20..52]);
    let end = 52 + payload_len;
    if bytes.len() < end {
        return Err(ForgeError::Store("partial v2 frame".into()));
    }
    let payload = &bytes[52..end];
    if Sha256::digest(payload).as_slice() != checksum {
        return Err(ForgeError::Store("v2 frame checksum mismatch".into()));
    }
    let event: TransitionEvent = serde_json::from_slice(payload)?;
    if event.seq != seq {
        return Err(ForgeError::Store("v2 frame sequence mismatch".into()));
    }
    Ok((
        LogFrameHeader {
            seq,
            payload_len: payload_len as u32,
            checksum,
        },
        event,
        end,
    ))
}

pub fn append_v2_frame(path: &Path, event: &TransitionEvent) -> Result<usize> {
    let frame = encode_frame(event)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&frame)?;
    file.sync_all()?;
    Ok(frame.len())
}

pub fn replay_v2(path: &Path) -> Result<Vec<TransitionEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    let mut events = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        match decode_frame(&bytes[offset..]) {
            Ok((_, event, consumed)) => {
                events.push(event);
                offset += consumed;
            }
            Err(_) if offset > 0 => break,
            Err(err) => return Err(err),
        }
    }
    Ok(events)
}

/// Dual-read / single-write v1 → v2 migration. v1 remains read-only.
pub fn migrate_item_to_v2(store: &FsWorkStore, work_id: &WorkId) -> Result<u64> {
    let v2 = events_v2_path(store, work_id);
    if v2.exists() {
        return Ok(replay_v2(&v2)?.last().map(|event| event.seq).unwrap_or(0));
    }
    let events = store.replay(work_id)?;
    if let Some(parent) = v2.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = v2.with_extension("v2.tmp");
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)?;
        for event in &events {
            file.write_all(&encode_frame(event)?)?;
        }
        file.sync_all()?;
    }
    fs::rename(&tmp, &v2)?;
    let marker = store.item_dir(work_id).join("store_generation");
    fs::write(&marker, format!("{LOG_V2_SCHEMA}\n"))?;
    Ok(events.last().map(|event| event.seq).unwrap_or(0))
}

pub fn write_snapshot_v2(
    transaction: &FileTransaction,
    relative: &str,
    envelope: &SnapshotEnvelopeV2,
    durability: DurabilityLevel,
) -> Result<usize> {
    let path = StorePath::parse(relative)
        .map_err(|err| ForgeError::Store(err.to_string()))?;
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

pub fn current_store_generation(store: &FsWorkStore, work_id: &WorkId) -> u32 {
    let marker = store.item_dir(work_id).join("store_generation");
    fs::read_to_string(marker)
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(STORE_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventPayload;
    use crate::model::{ActorKind, ActorRef, GitOid, GitWorkTarget, WorkItem, WorkTarget};
    use std::path::PathBuf;
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

    #[test]
    fn frame_round_trip_and_partial_tail() {
        let work = item();
        let event = TransitionEvent::new(
            work.id.clone(),
            1,
            actor(),
            EventPayload::ItemRegistered {
                item: Box::new(work),
            },
        );
        let mut bytes = encode_frame(&event).unwrap();
        let (_, decoded, consumed) = decode_frame(&bytes).unwrap();
        assert_eq!(decoded.seq, 1);
        assert_eq!(consumed, bytes.len());
        bytes.extend_from_slice(b"partial");
        let (_, _, _) = decode_frame(&bytes).unwrap();
    }

    #[test]
    fn migrate_writes_v2_beside_v1() {
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
        let seq = migrate_item_to_v2(&store, &work.id).unwrap();
        assert_eq!(seq, 1);
        assert!(events_v2_path(&store, &work.id).exists());
        assert!(store.events_path(&work.id).exists());
        assert_eq!(current_store_generation(&store, &work.id), LOG_V2_SCHEMA);
    }
}
