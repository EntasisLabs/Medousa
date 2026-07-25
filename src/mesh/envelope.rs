//! Signed mesh envelopes for daemon↔daemon share/message deliveries.

use std::fmt;

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::SigningKey;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::pairing::crypto::{parse_verifying_key, sign_message, verify_message};

pub const MESH_ENVELOPE_VERSION: u32 = 1;
pub const DEFAULT_ENVELOPE_TTL_SECS: i64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshCapability {
    Message,
    BundlePush,
    TaskRequest,
}

impl MeshCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Message => super::grants::CAP_MESH_MESSAGE,
            Self::BundlePush => super::grants::CAP_MESH_BUNDLE_PUSH,
            Self::TaskRequest => super::grants::CAP_TASK_REQUEST,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            super::grants::CAP_MESH_MESSAGE => Some(Self::Message),
            super::grants::CAP_MESH_BUNDLE_PUSH => Some(Self::BundlePush),
            super::grants::CAP_TASK_REQUEST => Some(Self::TaskRequest),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshEnvelope {
    pub version: u32,
    pub sender_device_id: String,
    pub recipient_device_id: String,
    pub seq: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
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

#[derive(Debug)]
pub enum MeshEnvelopeError {
    UnsupportedVersion(u32),
    Expired,
    NotYetValid,
    UnknownCapability,
    CapabilityNotGranted(String),
    SenderMismatch,
    RecipientMismatch,
    PayloadHashMismatch,
    MissingEnvelope,
    BadSignature(String),
    BadPublicKey(String),
    Serialize(String),
}

impl fmt::Display for MeshEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(f, "mesh envelope version {version} is unsupported")
            }
            Self::Expired => write!(f, "mesh envelope is expired"),
            Self::NotYetValid => write!(f, "mesh envelope is not yet valid"),
            Self::UnknownCapability => {
                write!(f, "mesh envelope capability is missing or unknown")
            }
            Self::CapabilityNotGranted(cap) => {
                write!(f, "mesh envelope capability `{cap}` is not granted")
            }
            Self::SenderMismatch => write!(f, "mesh envelope sender does not match pairing"),
            Self::RecipientMismatch => {
                write!(f, "mesh envelope recipient does not match this workshop")
            }
            Self::PayloadHashMismatch => write!(f, "mesh envelope payload hash mismatch"),
            Self::MissingEnvelope => {
                write!(f, "signed mesh envelope required for remote delivery")
            }
            Self::BadSignature(err) => write!(f, "mesh envelope signature invalid: {err}"),
            Self::BadPublicKey(err) => write!(f, "mesh envelope public key invalid: {err}"),
            Self::Serialize(err) => write!(f, "mesh envelope payload serialize failed: {err}"),
        }
    }
}

impl std::error::Error for MeshEnvelopeError {}

pub fn payload_hash_hex<T: Serialize>(payload: &T) -> Result<String, MeshEnvelopeError> {
    let bytes =
        serde_json::to_vec(payload).map_err(|err| MeshEnvelopeError::Serialize(err.to_string()))?;
    Ok(digest_hex(&bytes))
}

pub fn signing_message(envelope: &MeshEnvelope) -> String {
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

pub fn sign_envelope(
    signing_key: &SigningKey,
    sender_device_id: &str,
    recipient_device_id: &str,
    seq: u64,
    capability: MeshCapability,
    payload_hash: &str,
    ttl: Duration,
) -> MeshEnvelope {
    let issued_at = Utc::now();
    let expires_at = issued_at + ttl;
    let mut envelope = MeshEnvelope {
        version: MESH_ENVELOPE_VERSION,
        sender_device_id: sender_device_id.trim().to_string(),
        recipient_device_id: recipient_device_id.trim().to_string(),
        seq,
        issued_at,
        expires_at,
        capability: capability.as_str().to_string(),
        payload_hash: payload_hash.trim().to_string(),
        signature: String::new(),
    };
    let message = signing_message(&envelope);
    envelope.signature = sign_message(signing_key, &message);
    envelope
}

pub struct VerifyEnvelopeParams<'a> {
    pub envelope: &'a MeshEnvelope,
    pub payload_hash: &'a str,
    pub sender_public_key_b64: &'a str,
    pub expected_sender_device_id: &'a str,
    pub expected_recipient_device_id: &'a str,
    pub required_capability: MeshCapability,
    pub capability_granted: bool,
    pub now: DateTime<Utc>,
}

pub fn verify_envelope(params: VerifyEnvelopeParams<'_>) -> Result<(), MeshEnvelopeError> {
    let envelope = params.envelope;
    if envelope.version != MESH_ENVELOPE_VERSION {
        return Err(MeshEnvelopeError::UnsupportedVersion(envelope.version));
    }
    match MeshCapability::parse(&envelope.capability) {
        None => return Err(MeshEnvelopeError::UnknownCapability),
        Some(cap) if cap != params.required_capability => {
            return Err(MeshEnvelopeError::CapabilityNotGranted(
                envelope.capability.clone(),
            ));
        }
        Some(_) => {}
    }
    if !params.capability_granted {
        return Err(MeshEnvelopeError::CapabilityNotGranted(
            envelope.capability.clone(),
        ));
    }
    if !device_ids_match(&envelope.sender_device_id, params.expected_sender_device_id) {
        return Err(MeshEnvelopeError::SenderMismatch);
    }
    if !device_ids_match(
        &envelope.recipient_device_id,
        params.expected_recipient_device_id,
    ) {
        return Err(MeshEnvelopeError::RecipientMismatch);
    }
    if envelope.payload_hash.trim() != params.payload_hash.trim() {
        return Err(MeshEnvelopeError::PayloadHashMismatch);
    }
    // Allow a small clock skew window on issued_at.
    if envelope.issued_at > params.now + Duration::minutes(2) {
        return Err(MeshEnvelopeError::NotYetValid);
    }
    if envelope.expires_at <= params.now {
        return Err(MeshEnvelopeError::Expired);
    }

    let verifying_key = parse_verifying_key(params.sender_public_key_b64)
        .map_err(|err| MeshEnvelopeError::BadPublicKey(err.to_string()))?;
    let message = signing_message(envelope);
    verify_message(&verifying_key, &message, &envelope.signature)
        .map_err(|err| MeshEnvelopeError::BadSignature(err.to_string()))?;
    Ok(())
}

pub fn verify_enveloped_payload<T: Serialize>(
    request: &MeshEnvelopedRequest<T>,
    sender_public_key_b64: &str,
    expected_sender_device_id: &str,
    expected_recipient_device_id: &str,
    required_capability: MeshCapability,
    capability_granted: bool,
) -> Result<(), MeshEnvelopeError> {
    let hash = payload_hash_hex(&request.payload)?;
    verify_envelope(VerifyEnvelopeParams {
        envelope: &request.envelope,
        payload_hash: &hash,
        sender_public_key_b64,
        expected_sender_device_id,
        expected_recipient_device_id,
        required_capability,
        capability_granted,
        now: Utc::now(),
    })
}

/// Deserialize either a bare payload (trusted local) or an enveloped wrapper.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MeshInboundBody<T> {
    Enveloped(MeshEnvelopedRequest<T>),
    Bare(T),
}

impl<T> MeshInboundBody<T> {
    pub fn into_parts(self) -> (Option<MeshEnvelope>, T) {
        match self {
            Self::Enveloped(wrapped) => (Some(wrapped.envelope), wrapped.payload),
            Self::Bare(payload) => (None, payload),
        }
    }
}

pub fn require_remote_envelope_json(
    body: MeshInboundBody<serde_json::Value>,
    require_envelope: bool,
    sender_public_key_b64: &str,
    expected_sender_device_id: &str,
    expected_recipient_device_id: &str,
    required_capability: MeshCapability,
    capability_granted: bool,
) -> Result<(serde_json::Value, Option<MeshEnvelope>), MeshEnvelopeError> {
    match body {
        MeshInboundBody::Bare(payload) => {
            if require_envelope {
                return Err(MeshEnvelopeError::MissingEnvelope);
            }
            Ok((payload, None))
        }
        MeshInboundBody::Enveloped(wrapped) => {
            // Hash the wire JSON value (not a re-typed struct) so clients and
            // the daemon agree on payload_hash bytes.
            verify_enveloped_payload(
                &wrapped,
                sender_public_key_b64,
                expected_sender_device_id,
                expected_recipient_device_id,
                required_capability,
                capability_granted,
            )?;
            Ok((wrapped.payload, Some(wrapped.envelope)))
        }
    }
}

pub fn require_remote_envelope<T: DeserializeOwned>(
    body: MeshInboundBody<serde_json::Value>,
    require_envelope: bool,
    sender_public_key_b64: &str,
    expected_sender_device_id: &str,
    expected_recipient_device_id: &str,
    required_capability: MeshCapability,
    capability_granted: bool,
) -> Result<(T, Option<MeshEnvelope>), MeshEnvelopeError> {
    let (payload, envelope) = require_remote_envelope_json(
        body,
        require_envelope,
        sender_public_key_b64,
        expected_sender_device_id,
        expected_recipient_device_id,
        required_capability,
        capability_granted,
    )?;
    let typed = serde_json::from_value(payload)
        .map_err(|err| MeshEnvelopeError::Serialize(err.to_string()))?;
    Ok((typed, envelope))
}

fn device_ids_match(left: &str, right: &str) -> bool {
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return left == right;
    }
    left == right
        || left.starts_with(&right[..right.len().min(8)])
        || right.starts_with(&left[..left.len().min(8)])
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rand::rngs::OsRng;
    use serde_json::json;

    fn keypair() -> (SigningKey, VerifyingKey) {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        (signing, verifying)
    }

    #[test]
    fn signed_envelope_roundtrip_verifies() {
        let (signing, verifying) = keypair();
        let payload = json!({"body": "hello", "fromDeviceId": "aaaa1111"});
        let hash = payload_hash_hex(&payload).expect("hash");
        let envelope = sign_envelope(
            &signing,
            "aaaa1111",
            "bbbb2222",
            7,
            MeshCapability::Message,
            &hash,
            Duration::minutes(10),
        );
        let pk = crate::pairing::crypto::verifying_key_to_b64(&verifying);
        verify_envelope(VerifyEnvelopeParams {
            envelope: &envelope,
            payload_hash: &hash,
            sender_public_key_b64: &pk,
            expected_sender_device_id: "aaaa1111",
            expected_recipient_device_id: "bbbb2222",
            required_capability: MeshCapability::Message,
            capability_granted: true,
            now: Utc::now(),
        })
        .expect("verify");
    }

    #[test]
    fn rejects_tampered_payload_hash() {
        let (signing, verifying) = keypair();
        let payload = json!({"body": "hello"});
        let hash = payload_hash_hex(&payload).expect("hash");
        let envelope = sign_envelope(
            &signing,
            "aaaa1111",
            "bbbb2222",
            1,
            MeshCapability::BundlePush,
            &hash,
            Duration::minutes(10),
        );
        let pk = crate::pairing::crypto::verifying_key_to_b64(&verifying);
        let err = verify_envelope(VerifyEnvelopeParams {
            envelope: &envelope,
            payload_hash: "deadbeef",
            sender_public_key_b64: &pk,
            expected_sender_device_id: "aaaa1111",
            expected_recipient_device_id: "bbbb2222",
            required_capability: MeshCapability::BundlePush,
            capability_granted: true,
            now: Utc::now(),
        })
        .expect_err("hash mismatch");
        assert!(matches!(err, MeshEnvelopeError::PayloadHashMismatch));
    }

    #[test]
    fn rejects_expired_envelope() {
        let (signing, verifying) = keypair();
        let payload = json!({"body": "hello"});
        let hash = payload_hash_hex(&payload).expect("hash");
        let mut envelope = sign_envelope(
            &signing,
            "aaaa1111",
            "bbbb2222",
            1,
            MeshCapability::Message,
            &hash,
            Duration::minutes(10),
        );
        envelope.expires_at = Utc::now() - Duration::seconds(1);
        // Re-sign after mutating expiry so only expiry fails.
        let message = signing_message(&envelope);
        envelope.signature = sign_message(&signing, &message);
        let pk = crate::pairing::crypto::verifying_key_to_b64(&verifying);
        let err = verify_envelope(VerifyEnvelopeParams {
            envelope: &envelope,
            payload_hash: &hash,
            sender_public_key_b64: &pk,
            expected_sender_device_id: "aaaa1111",
            expected_recipient_device_id: "bbbb2222",
            required_capability: MeshCapability::Message,
            capability_granted: true,
            now: Utc::now(),
        })
        .expect_err("expired");
        assert!(matches!(err, MeshEnvelopeError::Expired));
    }

    #[test]
    fn rejects_ungranted_capability() {
        let (signing, verifying) = keypair();
        let payload = json!({"body": "hello"});
        let hash = payload_hash_hex(&payload).expect("hash");
        let envelope = sign_envelope(
            &signing,
            "aaaa1111",
            "bbbb2222",
            1,
            MeshCapability::Message,
            &hash,
            Duration::minutes(10),
        );
        let pk = crate::pairing::crypto::verifying_key_to_b64(&verifying);
        let err = verify_envelope(VerifyEnvelopeParams {
            envelope: &envelope,
            payload_hash: &hash,
            sender_public_key_b64: &pk,
            expected_sender_device_id: "aaaa1111",
            expected_recipient_device_id: "bbbb2222",
            required_capability: MeshCapability::Message,
            capability_granted: false,
            now: Utc::now(),
        })
        .expect_err("ungranted");
        assert!(matches!(err, MeshEnvelopeError::CapabilityNotGranted(_)));
    }

    #[test]
    fn remote_requires_envelope_wrapper() {
        let body = MeshInboundBody::Bare(json!({"body": "x"}));
        let err = require_remote_envelope_json(
            body,
            true,
            "pk",
            "a",
            "b",
            MeshCapability::Message,
            true,
        )
        .expect_err("remote bare");
        assert!(matches!(err, MeshEnvelopeError::MissingEnvelope));
    }

    #[test]
    fn require_remote_returns_envelope() {
        let (signing, verifying) = keypair();
        let payload = json!({"body": "hello"});
        let hash = payload_hash_hex(&payload).expect("hash");
        let envelope = sign_envelope(
            &signing,
            "aaaa1111",
            "bbbb2222",
            3,
            MeshCapability::Message,
            &hash,
            Duration::minutes(10),
        );
        let pk = crate::pairing::crypto::verifying_key_to_b64(&verifying);
        let body = MeshInboundBody::Enveloped(MeshEnvelopedRequest {
            envelope: envelope.clone(),
            payload: payload.clone(),
        });
        let (got, got_env) = require_remote_envelope_json(
            body,
            true,
            &pk,
            "aaaa1111",
            "bbbb2222",
            MeshCapability::Message,
            true,
        )
        .expect("ok");
        assert_eq!(got, payload);
        assert_eq!(got_env.as_ref().map(|e| e.seq), Some(3));
    }
}
