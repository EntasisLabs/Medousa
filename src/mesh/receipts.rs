//! Signed mesh delivery receipts (ack of inbound envelope seq).

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mesh::store_io::{MESH_IO_LOCK, read_json_default, receipts_path, write_json};
use crate::pairing::crypto::{parse_verifying_key, sign_message, verify_message};

pub const MESH_RECEIPT_VERSION: u32 = 1;
pub const CAP_MESH_RECEIPT: &str = "mesh.receipt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshReceiptStatus {
    Delivered,
    Duplicate,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshReceipt {
    pub id: String,
    pub version: u32,
    /// Workshop that received the delivery and signs this receipt.
    pub sender_device_id: String,
    /// Peer that originally sent the enveloped delivery.
    pub recipient_device_id: String,
    pub ack_seq: u64,
    pub payload_hash: String,
    pub status: MeshReceiptStatus,
    pub issued_at: DateTime<Utc>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshReceiptsFile {
    #[serde(default)]
    receipts: Vec<MeshReceipt>,
}

const RECEIPTS_CAP: usize = 2_000;

pub fn signing_message(receipt: &MeshReceipt) -> String {
    format!(
        "medousa-mesh-receipt/v{version}|{sender}|{recipient}|{seq}|{hash}|{status}|{issued}",
        version = receipt.version,
        sender = receipt.sender_device_id.trim(),
        recipient = receipt.recipient_device_id.trim(),
        seq = receipt.ack_seq,
        hash = receipt.payload_hash.trim(),
        status = match receipt.status {
            MeshReceiptStatus::Delivered => "delivered",
            MeshReceiptStatus::Duplicate => "duplicate",
            MeshReceiptStatus::Rejected => "rejected",
        },
        issued = receipt.issued_at.to_rfc3339(),
    )
}

pub fn issue_receipt(
    signing_key: &SigningKey,
    host_device_id: &str,
    peer_device_id: &str,
    ack_seq: u64,
    payload_hash: &str,
    status: MeshReceiptStatus,
) -> Result<MeshReceipt> {
    let mut receipt = MeshReceipt {
        id: format!("mrc_{}", Uuid::new_v4()),
        version: MESH_RECEIPT_VERSION,
        sender_device_id: host_device_id.trim().to_string(),
        recipient_device_id: peer_device_id.trim().to_string(),
        ack_seq,
        payload_hash: payload_hash.trim().to_string(),
        status,
        issued_at: Utc::now(),
        signature: String::new(),
    };
    let message = signing_message(&receipt);
    receipt.signature = sign_message(signing_key, &message);
    store_issued(&receipt)?;
    Ok(receipt)
}

pub fn verify_receipt(
    receipt: &MeshReceipt,
    host_public_key_b64: &str,
    expected_host_device_id: &str,
    expected_peer_device_id: &str,
) -> Result<()> {
    if receipt.version != MESH_RECEIPT_VERSION {
        bail!("unsupported mesh receipt version {}", receipt.version);
    }
    if receipt.sender_device_id.trim() != expected_host_device_id.trim() {
        bail!("mesh receipt host mismatch");
    }
    if receipt.recipient_device_id.trim() != expected_peer_device_id.trim() {
        bail!("mesh receipt peer mismatch");
    }
    let key = parse_verifying_key(host_public_key_b64).context("receipt host public key")?;
    verify_message(&key, &signing_message(receipt), &receipt.signature)
        .context("mesh receipt signature")?;
    Ok(())
}

pub fn store_issued(receipt: &MeshReceipt) -> Result<()> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = receipts_path();
    let mut file: MeshReceiptsFile = read_json_default(&path)?;
    file.receipts.push(receipt.clone());
    if file.receipts.len() > RECEIPTS_CAP {
        let drop = file.receipts.len() - RECEIPTS_CAP;
        file.receipts.drain(0..drop);
    }
    write_json(&path, &file)?;
    Ok(())
}

pub fn store_received(receipt: &MeshReceipt) -> Result<()> {
    store_issued(receipt)
}

pub fn find_for_ack(peer_device_id: &str, ack_seq: u64) -> Result<Option<MeshReceipt>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshReceiptsFile = read_json_default(&receipts_path())?;
    Ok(file.receipts.into_iter().rev().find(|receipt| {
        receipt.recipient_device_id == peer_device_id.trim() && receipt.ack_seq == ack_seq
    }))
}

pub fn list_receipts(limit: usize) -> Result<Vec<MeshReceipt>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshReceiptsFile = read_json_default(&receipts_path())?;
    let mut receipts = file.receipts;
    if receipts.len() > limit {
        receipts = receipts.split_off(receipts.len() - limit);
    }
    Ok(receipts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::crypto::verifying_key_to_b64;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn receipt_roundtrip_verifies() {
        let signing = SigningKey::generate(&mut OsRng);
        let pk = verifying_key_to_b64(&signing.verifying_key());
        let receipt = {
            let mut receipt = MeshReceipt {
                id: "mrc_test".into(),
                version: MESH_RECEIPT_VERSION,
                sender_device_id: "host".into(),
                recipient_device_id: "peer".into(),
                ack_seq: 9,
                payload_hash: "abc".into(),
                status: MeshReceiptStatus::Delivered,
                issued_at: Utc::now(),
                signature: String::new(),
            };
            receipt.signature = sign_message(&signing, &signing_message(&receipt));
            receipt
        };
        verify_receipt(&receipt, &pk, "host", "peer").expect("verify");
    }
}
