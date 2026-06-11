use anyhow::{Context, Result, bail};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: &str = "1.0.0";
pub const QR_SCHEME: &str = "medousa://pair/1.0";

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
