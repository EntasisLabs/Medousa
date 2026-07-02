use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::paths::{pairings_dir, revoked_pairings_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceRecord {
    pub pairing_id: String,
    pub phone_id: String,
    pub phone_name: String,
    pub phone_public_key: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub session_token_hash: String,
    pub session_token_expiry: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apns_device_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_updated_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_activity_push_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_activity_push_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RevokedPairings {
    #[serde(default)]
    pairing_ids: Vec<String>,
}

pub struct PairingStore {
    storage_key: [u8; 32],
}

impl PairingStore {
    pub fn new(signing_key: &SigningKey) -> Self {
        Self {
            storage_key: derive_storage_key(signing_key),
        }
    }

    pub fn list_paired(&self) -> Result<Vec<PairedDeviceRecord>> {
        let dir = pairings_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let revoked = self.load_revoked()?;
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("revoked.json") {
                continue;
            }
            match self.read_record(&path) {
                Ok(record) => {
                    if !revoked.pairing_ids.contains(&record.pairing_id) {
                        out.push(record);
                    }
                }
                Err(err) => {
                    eprintln!(
                        "medousa-daemon: skipping unreadable pairing record {} ({err:#})",
                        path.display()
                    );
                }
            }
        }
        out.sort_by(|left, right| right.last_seen.cmp(&left.last_seen));
        Ok(out)
    }

    pub fn get_by_phone_id(&self, phone_id: &str) -> Result<Option<PairedDeviceRecord>> {
        let path = record_path(phone_id);
        if !path.is_file() {
            return Ok(None);
        }
        let record = self.read_record(&path)?;
        let revoked = self.load_revoked()?;
        if revoked.pairing_ids.contains(&record.pairing_id) {
            return Ok(None);
        }
        Ok(Some(record))
    }

    pub fn save_record(&self, record: &PairedDeviceRecord) -> Result<()> {
        fs::create_dir_all(pairings_dir()).context("create pairings directory")?;
        let path = record_path(&record.phone_id);
        self.write_record(&path, record)
    }

    pub fn delete_record(&self, phone_id: &str) -> Result<()> {
        let path = record_path(phone_id);
        if path.is_file() {
            fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
        }
        Ok(())
    }

    pub fn revoke_pairing(&self, pairing_id: &str) -> Result<()> {
        let mut revoked = self.load_revoked()?;
        if !revoked.pairing_ids.iter().any(|id| id == pairing_id) {
            revoked.pairing_ids.push(pairing_id.to_string());
        }
        self.write_revoked(&revoked)
    }

    pub fn is_revoked(&self, pairing_id: &str) -> Result<bool> {
        let revoked = self.load_revoked()?;
        Ok(revoked.pairing_ids.iter().any(|id| id == pairing_id))
    }

    pub fn touch_last_seen(&self, phone_id: &str) -> Result<PairedDeviceRecord> {
        let mut record = self
            .get_by_phone_id(phone_id)?
            .ok_or_else(|| anyhow::anyhow!("paired device not found"))?;
        record.last_seen = Utc::now();
        self.save_record(&record)?;
        Ok(record)
    }

    fn read_record(&self, path: &Path) -> Result<PairedDeviceRecord> {
        let raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        if let Ok(envelope) = serde_json::from_slice::<EncryptedEnvelope>(&raw) {
            if let Ok(plaintext) = self.decrypt(&envelope) {
                if let Ok(record) = serde_json::from_slice::<PairedDeviceRecord>(&plaintext) {
                    return Ok(record);
                }
            }
        }
        serde_json::from_slice(&raw).context("parse plaintext pairing record")
    }

    fn write_record(&self, path: &Path, record: &PairedDeviceRecord) -> Result<()> {
        let plaintext = serde_json::to_vec(record).context("serialize pairing record")?;
        let envelope = self.encrypt(&plaintext)?;
        let encoded = serde_json::to_vec_pretty(&envelope).context("serialize encrypted envelope")?;
        fs::write(path, encoded).with_context(|| format!("write {}", path.display()))
    }

    fn load_revoked(&self) -> Result<RevokedPairings> {
        let path = revoked_pairings_path();
        if !path.is_file() {
            return Ok(RevokedPairings::default());
        }
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str(&raw).context("parse revoked pairings")
    }

    fn write_revoked(&self, revoked: &RevokedPairings) -> Result<()> {
        fs::create_dir_all(pairings_dir()).context("create pairings directory")?;
        let encoded =
            serde_json::to_string_pretty(revoked).context("serialize revoked pairings")?;
        fs::write(revoked_pairings_path(), encoded).context("write revoked pairings")
    }

    pub(crate) fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedEnvelope> {
        let cipher = XChaCha20Poly1305::new_from_slice(&self.storage_key)
            .context("initialize pairing cipher")?;
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(XNonce::from_slice(&nonce), plaintext)
            .map_err(|err| anyhow::anyhow!("encrypt pairing record: {err}"))?;
        Ok(EncryptedEnvelope {
            nonce: super::crypto::base64url_encode(&nonce),
            ciphertext: super::crypto::base64url_encode(&ciphertext),
        })
    }

    pub(crate) fn decrypt(&self, envelope: &EncryptedEnvelope) -> Result<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new_from_slice(&self.storage_key)
            .context("initialize pairing cipher")?;
        let nonce = super::crypto::base64url_decode(&envelope.nonce)?;
        if nonce.len() != 24 {
            bail!("invalid pairing nonce length");
        }
        let ciphertext = super::crypto::base64url_decode(&envelope.ciphertext)?;
        cipher
            .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|err| anyhow::anyhow!("decrypt pairing record: {err}"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EncryptedEnvelope {
    nonce: String,
    ciphertext: String,
}

fn derive_storage_key(signing_key: &SigningKey) -> [u8; 32] {
    Sha256::digest(signing_key.to_bytes()).into()
}

fn record_path(phone_id: &str) -> PathBuf {
    let safe = phone_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    pairings_dir().join(format!("{safe}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::identity::DeviceIdentity;

    #[test]
    fn encrypted_payload_roundtrip() {
        let identity = DeviceIdentity::generate_ephemeral();
        let store = PairingStore::new(identity.signing_key());
        let record = PairedDeviceRecord {
            pairing_id: uuid::Uuid::new_v4().to_string(),
            phone_id: "phone1234".to_string(),
            phone_name: "Test Phone".to_string(),
            phone_public_key: "abc".to_string(),
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            session_token_hash: "deadbeef".to_string(),
            session_token_expiry: Utc::now(),
            apns_device_token: None,
            push_platform: None,
            push_updated_at: None,
            live_activity_push_token: None,
            live_activity_push_updated_at: None,
        };
        let plaintext = serde_json::to_vec(&record).expect("serialize");
        let envelope = store.encrypt(&plaintext).expect("encrypt");
        let decoded = store.decrypt(&envelope).expect("decrypt");
        let loaded: PairedDeviceRecord = serde_json::from_slice(&decoded).expect("parse");
        assert_eq!(loaded.pairing_id, record.pairing_id);
    }

    #[test]
    fn list_paired_skips_unreadable_records() {
        let identity = DeviceIdentity::generate_ephemeral();
        let store = PairingStore::new(identity.signing_key());
        fs::create_dir_all(pairings_dir()).expect("pairings dir");
        let corrupt_path = pairings_dir().join("corrupt-test-phone.json");
        fs::write(
            &corrupt_path,
            br#"{"nonce":"bad","ciphertext":"bad"}"#,
        )
        .expect("write corrupt record");
        let listed = store.list_paired().expect("list should not fail");
        assert!(
            listed.iter().all(|record| record.phone_id != "corrupt-test-phone"),
            "corrupt record should be skipped"
        );
        let _ = fs::remove_file(corrupt_path);
    }
}
