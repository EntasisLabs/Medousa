//! Mesh outbox — pending outbound enveloped deliveries awaiting receipt.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::mesh::envelope::{
    DEFAULT_ENVELOPE_TTL_SECS, MeshCapability, MeshEnvelope, MeshEnvelopedRequest,
    payload_hash_hex, sign_envelope,
};
use crate::mesh::receipts::{self, MeshReceipt, MeshReceiptStatus};
use crate::mesh::registry;
use crate::mesh::store_io::{MESH_IO_LOCK, outbox_path, read_json_default, write_json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshOutboxStatus {
    Pending,
    InFlight,
    Acked,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshOutboxItem {
    pub id: String,
    pub peer_device_id: String,
    pub seq: u64,
    pub capability: String,
    pub envelope: MeshEnvelope,
    pub payload: Value,
    pub status: MeshOutboxStatus,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acked_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshOutboxFile {
    #[serde(default)]
    items: Vec<MeshOutboxItem>,
}

const OUTBOX_CAP: usize = 2_000;

pub fn enqueue(
    signing_key: &SigningKey,
    local_device_id: &str,
    peer_device_id: &str,
    capability: MeshCapability,
    payload: Value,
) -> Result<MeshOutboxItem> {
    let peer = registry::get_peer(peer_device_id)?
        .with_context(|| format!("mesh peer not registered: {peer_device_id}"))?;
    if !peer.mesh_enabled {
        bail!("mesh disabled for peer {peer_device_id}");
    }
    let seq = registry::allocate_outbound_seq(&peer.device_id)?;
    let payload_hash = payload_hash_hex(&payload).map_err(|err| anyhow::anyhow!(err))?;
    let envelope = sign_envelope(
        signing_key,
        local_device_id,
        &peer.device_id,
        seq,
        capability,
        &payload_hash,
        Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
    );
    let now = Utc::now();
    let item = MeshOutboxItem {
        id: format!("mout_{}", Uuid::new_v4()),
        peer_device_id: peer.device_id,
        seq,
        capability: capability.as_str().to_string(),
        envelope,
        payload,
        status: MeshOutboxStatus::Pending,
        attempts: 0,
        last_error: None,
        created_at: now,
        updated_at: now,
        acked_at: None,
        receipt_id: None,
    };

    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = outbox_path();
    let mut file: MeshOutboxFile = read_json_default(&path)?;
    file.items.push(item.clone());
    if file.items.len() > OUTBOX_CAP {
        let drop = file.items.len() - OUTBOX_CAP;
        file.items.drain(0..drop);
    }
    write_json(&path, &file)?;
    Ok(item)
}

pub fn enveloped_request(item: &MeshOutboxItem) -> MeshEnvelopedRequest<Value> {
    MeshEnvelopedRequest {
        envelope: item.envelope.clone(),
        payload: item.payload.clone(),
    }
}

pub fn list_outbox(status: Option<MeshOutboxStatus>) -> Result<Vec<MeshOutboxItem>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshOutboxFile = read_json_default(&outbox_path())?;
    Ok(file
        .items
        .into_iter()
        .filter(|item| status.map(|wanted| item.status == wanted).unwrap_or(true))
        .collect())
}

pub fn get_outbox_item(id: &str) -> Result<Option<MeshOutboxItem>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshOutboxFile = read_json_default(&outbox_path())?;
    Ok(file.items.into_iter().find(|item| item.id == id.trim()))
}

pub fn mark_in_flight(id: &str) -> Result<MeshOutboxItem> {
    update_item(id, |item| {
        if item.status == MeshOutboxStatus::Acked {
            return;
        }
        item.status = MeshOutboxStatus::InFlight;
        item.attempts = item.attempts.saturating_add(1);
        item.updated_at = Utc::now();
        item.last_error = None;
    })
}

pub fn mark_acked(
    id: &str,
    receipt: &MeshReceipt,
    peer_public_key: &str,
    local_device_id: &str,
) -> Result<MeshOutboxItem> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = outbox_path();
    let mut file: MeshOutboxFile = read_json_default(&path)?;
    let item = file
        .items
        .iter_mut()
        .find(|item| item.id == id.trim())
        .with_context(|| format!("outbox item not found: {id}"))?;
    verify_receipt_for_item(item, receipt, peer_public_key, local_device_id)?;
    if item.status == MeshOutboxStatus::Acked {
        return Ok(item.clone());
    }
    item.status = MeshOutboxStatus::Acked;
    item.acked_at = Some(Utc::now());
    item.updated_at = Utc::now();
    item.receipt_id = Some(receipt.id.clone());
    item.last_error = None;
    let snapshot = item.clone();
    write_json(&path, &file)?;
    Ok(snapshot)
}

pub fn verify_receipt_for_item(
    item: &MeshOutboxItem,
    receipt: &MeshReceipt,
    peer_public_key: &str,
    local_device_id: &str,
) -> Result<()> {
    receipts::verify_receipt(
        receipt,
        peer_public_key,
        &item.peer_device_id,
        local_device_id,
    )?;
    if !matches!(
        receipt.status,
        MeshReceiptStatus::Delivered | MeshReceiptStatus::Duplicate
    ) {
        bail!("mesh receipt does not confirm delivery");
    }
    if receipt.ack_seq != item.seq || receipt.payload_hash != item.envelope.payload_hash {
        bail!("mesh receipt does not match outbox delivery");
    }
    if item.envelope.sender_device_id != local_device_id
        || item.envelope.recipient_device_id != item.peer_device_id
    {
        bail!("outbox envelope identity mismatch");
    }
    Ok(())
}

pub fn mark_failed(id: &str, error: &str) -> Result<MeshOutboxItem> {
    update_item(id, |item| {
        if item.status == MeshOutboxStatus::Acked {
            return;
        }
        item.status = MeshOutboxStatus::Failed;
        item.updated_at = Utc::now();
        item.last_error = Some(error.trim().to_string());
    })
}

pub fn find_by_peer_seq(peer_device_id: &str, seq: u64) -> Result<Option<MeshOutboxItem>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshOutboxFile = read_json_default(&outbox_path())?;
    Ok(file
        .items
        .into_iter()
        .rev()
        .find(|item| item.peer_device_id == peer_device_id.trim() && item.seq == seq))
}

fn update_item(id: &str, mutate: impl FnOnce(&mut MeshOutboxItem)) -> Result<MeshOutboxItem> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = outbox_path();
    let mut file: MeshOutboxFile = read_json_default(&path)?;
    let item = file
        .items
        .iter_mut()
        .find(|item| item.id == id.trim())
        .with_context(|| format!("outbox item not found: {id}"))?;
    mutate(item);
    let snapshot = item.clone();
    write_json(&path, &file)?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::receipts::{MESH_RECEIPT_VERSION, signing_message};
    use crate::pairing::crypto::{sign_message, verifying_key_to_b64};
    use rand::rngs::OsRng;

    fn signed_receipt(signing_key: &SigningKey, status: MeshReceiptStatus) -> MeshReceipt {
        let mut receipt = MeshReceipt {
            id: "mrc_test".into(),
            version: MESH_RECEIPT_VERSION,
            sender_device_id: "remote".into(),
            recipient_device_id: "local".into(),
            ack_seq: 7,
            payload_hash: "payload-hash".into(),
            status,
            issued_at: Utc::now(),
            signature: String::new(),
        };
        receipt.signature = sign_message(signing_key, &signing_message(&receipt));
        receipt
    }

    fn outbox_item() -> MeshOutboxItem {
        let signing_key = SigningKey::generate(&mut OsRng);
        let envelope = sign_envelope(
            &signing_key,
            "local",
            "remote",
            7,
            MeshCapability::Message,
            "payload-hash",
            Duration::seconds(60),
        );
        let now = Utc::now();
        MeshOutboxItem {
            id: "mout_test".into(),
            peer_device_id: "remote".into(),
            seq: 7,
            capability: MeshCapability::Message.as_str().into(),
            envelope,
            payload: serde_json::json!({"message": "hello"}),
            status: MeshOutboxStatus::InFlight,
            attempts: 1,
            last_error: None,
            created_at: now,
            updated_at: now,
            acked_at: None,
            receipt_id: None,
        }
    }

    #[test]
    fn only_matching_signed_success_or_duplicate_receipts_ack() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let public_key = verifying_key_to_b64(&signing_key.verifying_key());
        let item = outbox_item();
        for status in [MeshReceiptStatus::Delivered, MeshReceiptStatus::Duplicate] {
            let receipt = signed_receipt(&signing_key, status);
            verify_receipt_for_item(&item, &receipt, &public_key, "local").expect("valid ack");
        }
        let rejected = signed_receipt(&signing_key, MeshReceiptStatus::Rejected);
        assert!(verify_receipt_for_item(&item, &rejected, &public_key, "local").is_err());

        let tamper_cases: [fn(&mut MeshReceipt); 4] = [
            |receipt: &mut MeshReceipt| receipt.sender_device_id = "other".into(),
            |receipt: &mut MeshReceipt| receipt.recipient_device_id = "other".into(),
            |receipt: &mut MeshReceipt| receipt.ack_seq += 1,
            |receipt: &mut MeshReceipt| receipt.payload_hash = "other".into(),
        ];
        for tamper in tamper_cases {
            let mut receipt = signed_receipt(&signing_key, MeshReceiptStatus::Delivered);
            tamper(&mut receipt);
            receipt.signature = sign_message(&signing_key, &signing_message(&receipt));
            assert!(verify_receipt_for_item(&item, &receipt, &public_key, "local").is_err());
        }

        let forged = signed_receipt(
            &SigningKey::generate(&mut OsRng),
            MeshReceiptStatus::Delivered,
        );
        assert!(verify_receipt_for_item(&item, &forged, &public_key, "local").is_err());
    }
}
