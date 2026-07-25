//! Apply accepted mesh inbox items and issue receipts.

use anyhow::Result;
use ed25519_dalek::SigningKey;

use crate::mesh::envelope::MeshEnvelope;
use crate::mesh::inbox::{self, MeshInboxAccept};
use crate::mesh::receipts::{self, MeshReceipt, MeshReceiptStatus};
use crate::mesh::registry;

pub struct InboundMeshAccept {
    pub duplicate: bool,
    pub inbox_id: String,
    pub receipt: MeshReceipt,
    pub local_ref: Option<String>,
}

/// Record an inbound enveloped delivery (dedupe by sender+seq) and issue a receipt.
pub fn accept_inbound_delivery(
    signing_key: &SigningKey,
    host_device_id: &str,
    envelope: &MeshEnvelope,
    payload_hash: &str,
) -> Result<InboundMeshAccept> {
    let _ = registry::touch_last_seen(&envelope.sender_device_id);
    let accepted: MeshInboxAccept = inbox::accept(
        &envelope.sender_device_id,
        envelope.seq,
        &envelope.capability,
        payload_hash,
        Some(envelope.clone()),
    )?;

    if accepted.duplicate {
        let status = MeshReceiptStatus::Duplicate;
        let receipt = if let Some(existing) =
            receipts::find_for_ack(&envelope.sender_device_id, envelope.seq)?
        {
            existing
        } else {
            receipts::issue_receipt(
                signing_key,
                host_device_id,
                &envelope.sender_device_id,
                envelope.seq,
                payload_hash,
                status,
            )?
        };
        return Ok(InboundMeshAccept {
            duplicate: true,
            inbox_id: accepted.item.id,
            receipt,
            local_ref: accepted.item.local_ref,
        });
    }

    let receipt = receipts::issue_receipt(
        signing_key,
        host_device_id,
        &envelope.sender_device_id,
        envelope.seq,
        payload_hash,
        MeshReceiptStatus::Delivered,
    )?;
    Ok(InboundMeshAccept {
        duplicate: false,
        inbox_id: accepted.item.id,
        receipt,
        local_ref: None,
    })
}

pub fn bind_delivery_local_ref(inbox_id: &str, local_ref: &str, receipt_id: &str) -> Result<()> {
    inbox::bind_local_ref(inbox_id, local_ref, Some(receipt_id))
}

pub fn receipt_header_value(receipt: &MeshReceipt) -> Result<String> {
    Ok(serde_json::to_string(receipt)?)
}
