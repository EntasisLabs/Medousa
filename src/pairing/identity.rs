use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;

use super::crypto::{base64url_decode, device_id_from_public_key};
use super::paths::{identity_dir, identity_secret_path};

pub struct DeviceIdentity {
    pub device_id: String,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl DeviceIdentity {
    pub fn load_or_create() -> Result<Self> {
        fs::create_dir_all(identity_dir()).context("failed to create identity directory")?;
        let path = identity_secret_path();
        if path.is_file() {
            Self::load_from_path(&path)
        } else {
            Self::create_at_path(&path)
        }
    }

    pub fn generate_ephemeral() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let device_id = device_id_from_public_key(verifying_key.as_bytes());
        Self {
            device_id,
            signing_key,
            verifying_key,
        }
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let trimmed = raw.trim();
        let bytes = decode_secret_bytes(trimmed)?;
        if bytes.len() != 32 {
            bail!("identity secret must be 32 bytes");
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let device_id = device_id_from_public_key(verifying_key.as_bytes());
        Ok(Self {
            device_id,
            signing_key,
            verifying_key,
        })
    }

    fn create_at_path(path: &Path) -> Result<Self> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let encoded = encode_hex(signing_key.to_bytes().as_slice());
        fs::write(path, format!("{encoded}\n")).with_context(|| format!("write {}", path.display()))?;
        let verifying_key = signing_key.verifying_key();
        let device_id = device_id_from_public_key(verifying_key.as_bytes());
        Ok(Self {
            device_id,
            signing_key,
            verifying_key,
        })
    }
}

fn decode_secret_bytes(raw: &str) -> Result<Vec<u8>> {
    if raw.len() == 64 && raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return (0..raw.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&raw[index..index + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .context("invalid hex identity secret");
    }
    base64url_decode(raw)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_is_eight_hex_chars() {
        let identity = DeviceIdentity::generate_ephemeral();
        assert_eq!(identity.device_id.len(), 8);
        assert!(identity.device_id.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
