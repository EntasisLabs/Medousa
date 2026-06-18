use anyhow::{Context, Result, bail};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: &str = "1.0.0";
pub const QR_SCHEME: &str = "medousa://pair/1.0";
pub const QR_SCHEME_V2: &str = "medousa://pair/2.0";
pub const QR_PROTOCOL_V1: &str = "1.0";
pub const QR_PROTOCOL_V2: &str = "2.0";

pub fn device_id_from_public_key(public_key: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key);
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn base64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn base64url_decode(value: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value.trim())
        .context("invalid base64url value")
}

pub fn sign_message(signing_key: &SigningKey, message: &str) -> String {
    let signature = signing_key.sign(message.as_bytes());
    base64url_encode(signature.to_bytes().as_slice())
}

pub fn verify_message(verifying_key: &VerifyingKey, message: &str, signature_b64: &str) -> Result<()> {
    let bytes = base64url_decode(signature_b64)?;
    let signature = Signature::from_slice(&bytes).context("invalid ed25519 signature length")?;
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|err| anyhow::anyhow!("signature verification failed: {err}"))
}

pub fn qr_signing_message(address: &str, device_id: &str, token_b64: &str) -> String {
    format!("{address}|{device_id}|{token_b64}")
}

pub fn qr_signing_message_v2(
    address: &str,
    device_id: &str,
    token_b64: &str,
    iroh_ticket: &str,
) -> String {
    format!("{address}|{device_id}|{token_b64}|{iroh_ticket}")
}

pub fn verify_qr_url_signature(
    verifying_key: &VerifyingKey,
    address: &str,
    device_id: &str,
    token_b64: &str,
    signature_b64: &str,
) -> Result<()> {
    let message = qr_signing_message(address, device_id, token_b64);
    verify_message(verifying_key, &message, signature_b64)
}

pub fn verify_qr_url_signature_v2(
    verifying_key: &VerifyingKey,
    address: &str,
    device_id: &str,
    token_b64: &str,
    iroh_ticket: &str,
    signature_b64: &str,
) -> Result<()> {
    let message = qr_signing_message_v2(address, device_id, token_b64, iroh_ticket);
    verify_message(verifying_key, &message, signature_b64)
}

pub fn parse_verifying_key(public_key_b64: &str) -> Result<VerifyingKey> {
    let bytes = base64url_decode(public_key_b64)?;
    if bytes.len() != 32 {
        bail!("ed25519 public key must be 32 bytes");
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&key).context("invalid ed25519 public key")
}

pub fn verifying_key_to_b64(key: &VerifyingKey) -> String {
    base64url_encode(key.as_bytes())
}

pub fn hash_session_token(token: &str) -> String {
    digest_hex(Sha256::digest(token.as_bytes()).as_slice())
}

fn digest_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn v2_signing_message_includes_ticket() {
        let message = qr_signing_message_v2("192.168.1.2:7419", "abcd1234", "token", "ticket");
        assert_eq!(message, "192.168.1.2:7419|abcd1234|token|ticket");
    }

    #[test]
    fn v2_qr_signature_verifies() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let message = qr_signing_message_v2("host:7419", "deadbeef", "qr-token", "iroh-ticket");
        let signature = sign_message(&signing_key, &message);
        verify_qr_url_signature_v2(
            &verifying_key,
            "host:7419",
            "deadbeef",
            "qr-token",
            "iroh-ticket",
            &signature,
        )
        .expect("v2 signature should verify");
    }
}
