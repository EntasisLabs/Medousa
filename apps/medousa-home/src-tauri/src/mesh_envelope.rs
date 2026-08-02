//! Client-side mesh envelope signing (mirrors daemon `medousa::mesh`).

use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::pairing_client::{WorkshopTransportConfig, load_phone_signing_key};

const MESH_ENVELOPE_VERSION: u32 = 1;
const DEFAULT_TTL_SECS: i64 = 15 * 60;

pub const CAP_MESH_MESSAGE: &str = "mesh.message";
pub const CAP_MESH_BUNDLE_PUSH: &str = "mesh.bundle.push";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshEnvelope {
    version: u32,
    sender_device_id: String,
    recipient_device_id: String,
    seq: u64,
    issued_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
    capability: String,
    payload_hash: String,
    signature: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshEnvelopedRequest<T> {
    pub envelope: MeshEnvelope,
    pub payload: T,
}

pub fn wrap_payload_for_workshop<T: Serialize>(
    config: &WorkshopTransportConfig,
    capability: &str,
    payload: T,
) -> Result<MeshEnvelopedRequest<T>, String> {
    if config.phone_id.trim().is_empty() || config.workshop_device_id.trim().is_empty() {
        return Err("Mesh envelope requires paired phone and workshop device ids".to_string());
    }
    let signing_key = load_phone_signing_key()?;
    let payload_hash = payload_hash_hex(&payload)?;
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(DEFAULT_TTL_SECS);
    let seq = issued_at.timestamp_millis().max(0) as u64;
    let mut envelope = MeshEnvelope {
        version: MESH_ENVELOPE_VERSION,
        sender_device_id: config.phone_id.clone(),
        recipient_device_id: config.workshop_device_id.clone(),
        seq,
        issued_at,
        expires_at,
        capability: capability.to_string(),
        payload_hash,
        signature: String::new(),
    };
    let message = signing_message(&envelope);
    envelope.signature = sign_message(&signing_key, &message);
    Ok(MeshEnvelopedRequest { envelope, payload })
}

pub fn wrap_json_for_workshop(
    config: &WorkshopTransportConfig,
    capability: &str,
    payload: Value,
) -> Result<Value, String> {
    let wrapped = wrap_payload_for_workshop(config, capability, payload)?;
    serde_json::to_value(wrapped).map_err(|err| err.to_string())
}

fn payload_hash_hex<T: Serialize>(payload: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(payload).map_err(|err| err.to_string())?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn signing_message(envelope: &MeshEnvelope) -> String {
    format!(
        "medousa-mesh/v{version}|{sender}|{recipient}|{seq}|{issued}|{expires}|{capability}|{hash}",
        version = envelope.version,
        sender = envelope.sender_device_id.trim(),
        recipient = envelope.recipient_device_id.trim(),
        seq = envelope.seq,
        issued = envelope.issued_at.to_rfc3339(),
        expires = envelope.expires_at.to_rfc3339(),
        capability = envelope.capability.trim(),
        hash = envelope.payload_hash.trim(),
    )
}

fn sign_message(signing_key: &SigningKey, message: &str) -> String {
    let signature = signing_key.sign(message.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())
}
