//! Client-side mesh envelope signing (mirrors daemon `medousa::mesh`).

use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::pairing_client::{
    WorkshopTransportConfig, allocate_mesh_sequence, load_phone_signing_key, parse_verifying_key,
};

const MESH_ENVELOPE_VERSION: u32 = 1;
const DEFAULT_TTL_SECS: i64 = 15 * 60;

pub const CAP_MESH_MESSAGE: &str = "mesh.message";
pub const CAP_MESH_BUNDLE_PUSH: &str = "mesh.bundle.push";
pub const CAP_TASK_REQUEST: &str = "task.request";
pub const CAP_TASK_RESULT: &str = "task.result";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshEnvelope {
    pub version: u32,
    pub sender_device_id: String,
    pub recipient_device_id: String,
    pub seq: u64,
    pub issued_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
    pub capability: String,
    pub payload_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    let seq = allocate_mesh_sequence(&config.workshop_device_id)?;
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

/// Verify a signed response against the exact daemon identity pinned during
/// pairing. Route selection alone is never accepted as identity proof.
pub fn verify_payload_from_workshop<T: Serialize>(
    config: &WorkshopTransportConfig,
    wrapped: &MeshEnvelopedRequest<T>,
    required_capability: &str,
) -> Result<(), String> {
    let envelope = &wrapped.envelope;
    if envelope.version != MESH_ENVELOPE_VERSION {
        return Err(format!(
            "Unsupported mesh envelope version {}",
            envelope.version
        ));
    }
    if envelope.capability.trim() != required_capability {
        return Err(format!(
            "Mesh response capability '{}' is not '{}'",
            envelope.capability, required_capability
        ));
    }
    if envelope.sender_device_id.trim() != config.workshop_device_id.trim() {
        return Err("Mesh response sender does not match the paired workshop".to_string());
    }
    if envelope.recipient_device_id.trim() != config.phone_id.trim() {
        return Err("Mesh response recipient does not match this client identity".to_string());
    }
    if envelope.payload_hash.trim() != payload_hash_hex(&wrapped.payload)? {
        return Err("Mesh response payload hash mismatch".to_string());
    }
    let now = Utc::now();
    if envelope.issued_at > now + Duration::minutes(2) {
        return Err("Mesh response is not yet valid".to_string());
    }
    if envelope.expires_at <= now {
        return Err("Mesh response has expired".to_string());
    }
    let pinned_key = config
        .daemon_public_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Paired workshop identity is not pinned; pair it again before delegating work"
                .to_string()
        })?;
    let verifying_key = parse_verifying_key(pinned_key)?;
    let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(envelope.signature.trim())
        .map_err(|error| error.to_string())?;
    let signature = ed25519_dalek::Signature::from_slice(&signature_bytes)
        .map_err(|_| "Invalid mesh response signature".to_string())?;
    verifying_key
        .verify(signing_message(envelope).as_bytes(), &signature)
        .map_err(|error| format!("Mesh response signature invalid: {error}"))
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
