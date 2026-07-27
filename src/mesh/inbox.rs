//! Mesh inbox — durable inbound accept with sender+seq dedupe.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mesh::envelope::MeshEnvelope;
use crate::mesh::store_io::{MESH_IO_LOCK, inbox_path, read_json_default, write_json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshInboxStatus {
    Accepted,
    Duplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshInboxItem {
    pub id: String,
    pub sender_device_id: String,
    pub seq: u64,
    pub capability: String,
    pub payload_hash: String,
    pub received_at: DateTime<Utc>,
    pub status: MeshInboxStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<MeshEnvelope>,
}

#[derive(Debug, Clone)]
pub struct MeshInboxAccept {
    pub duplicate: bool,
    pub item: MeshInboxItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshInboxFile {
    #[serde(default)]
    items: Vec<MeshInboxItem>,
}

const INBOX_CAP: usize = 2_000;

pub fn accept(
    sender_device_id: &str,
    seq: u64,
    capability: &str,
    payload_hash: &str,
    envelope: Option<MeshEnvelope>,
) -> Result<MeshInboxAccept> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = inbox_path();
    let mut file: MeshInboxFile = read_json_default(&path)?;
    if let Some(existing) = file.items.iter().find(|item| {
        item.sender_device_id == sender_device_id.trim() && item.seq == seq
    }) {
        let mut item = existing.clone();
        item.status = MeshInboxStatus::Duplicate;
        return Ok(MeshInboxAccept {
            duplicate: true,
            item,
        });
    }

    let item = MeshInboxItem {
        id: format!("min_{}", Uuid::new_v4()),
        sender_device_id: sender_device_id.trim().to_string(),
        seq,
        capability: capability.trim().to_string(),
        payload_hash: payload_hash.trim().to_string(),
        received_at: Utc::now(),
        status: MeshInboxStatus::Accepted,
        local_ref: None,
        receipt_id: None,
        envelope,
    };
    file.items.push(item.clone());
    if file.items.len() > INBOX_CAP {
        let drop = file.items.len() - INBOX_CAP;
        file.items.drain(0..drop);
    }
    write_json(&path, &file)?;
    Ok(MeshInboxAccept {
        duplicate: false,
        item,
    })
}

pub fn bind_local_ref(inbox_id: &str, local_ref: &str, receipt_id: Option<&str>) -> Result<()> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = inbox_path();
    let mut file: MeshInboxFile = read_json_default(&path)?;
    if let Some(item) = file.items.iter_mut().find(|item| item.id == inbox_id) {
        item.local_ref = Some(local_ref.trim().to_string());
        if let Some(receipt_id) = receipt_id.map(str::trim).filter(|value| !value.is_empty()) {
            item.receipt_id = Some(receipt_id.to_string());
        }
        write_json(&path, &file)?;
    }
    Ok(())
}

pub fn list_inbox(limit: usize) -> Result<Vec<MeshInboxItem>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshInboxFile = read_json_default(&inbox_path())?;
    let mut items = file.items;
    if items.len() > limit {
        items = items.split_off(items.len() - limit);
    }
    Ok(items)
}

pub fn find_by_sender_seq(sender_device_id: &str, seq: u64) -> Result<Option<MeshInboxItem>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshInboxFile = read_json_default(&inbox_path())?;
    Ok(file
        .items
        .into_iter()
        .find(|item| item.sender_device_id == sender_device_id.trim() && item.seq == seq))
}
