use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use medousa_types::authority_id::PairingDeviceId;

use super::paths::pairings_dir;
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};

const MAX_PAIRING_RECORD_BYTES: u64 = 1024 * 1024;
const REVOKED_PAIRINGS_FILE: &str = "revoked.json";

/// How this surface relates to the workshop.
/// - `portal`: full client of this brain (phone / workshop switcher)
/// - `peer`: inbox + share only
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PairingRole {
    #[default]
    Portal,
    Peer,
}

impl PairingRole {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("peer") => Self::Peer,
            _ => Self::Portal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Portal => "portal",
            Self::Peer => "peer",
        }
    }

    pub fn allows_peer_surface(self) -> bool {
        matches!(self, Self::Peer | Self::Portal)
    }

    pub fn allows_full_portal(self) -> bool {
        matches!(self, Self::Portal)
    }
}

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
    /// Monotonic generation captured by live connections and revocation events.
    #[serde(default = "initial_credential_generation")]
    pub credential_generation: u64,
    /// Defaults to portal for records written before role split.
    #[serde(default)]
    pub role: PairingRole,
    /// Shared-mode seat this device is bound to (`user:alice`, …). Absent on personal installs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Mesh capability grants (`mesh.message`, `mesh.bundle.push`, …). Empty = role defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mesh_grants: Vec<String>,
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

const fn initial_credential_generation() -> u64 {
    1
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
        let root = pairing_root()?;
        let revoked = self.load_revoked()?;
        let mut by_phone_id = HashMap::new();
        for entry in root.list_root()? {
            if entry.kind != StoreEntryKind::File || !entry.path.file_name().ends_with(".json") {
                continue;
            }
            if entry.path.file_name() == REVOKED_PAIRINGS_FILE {
                continue;
            }
            match self.read_record(&root, &entry.path) {
                Ok(record) => {
                    if !revoked.pairing_ids.contains(&record.pairing_id) {
                        let replace = by_phone_id.get(&record.phone_id).is_none_or(
                            |current: &PairedDeviceRecord| record.last_seen > current.last_seen,
                        );
                        if replace {
                            by_phone_id.insert(record.phone_id.clone(), record);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("medousa-daemon: skipping unreadable pairing record ({err:#})");
                }
            }
        }
        let mut out = by_phone_id.into_values().collect::<Vec<_>>();
        out.sort_by_key(|right| std::cmp::Reverse(right.last_seen));
        Ok(out)
    }

    pub fn get_by_phone_id(&self, phone_id: &str) -> Result<Option<PairedDeviceRecord>> {
        let root = pairing_root()?;
        let path = record_path(phone_id)?;
        let record = if root.is_file(&path)? {
            Some(self.read_record(&root, &path)?)
        } else {
            self.find_legacy_record(&root, phone_id)?
        };
        let Some(record) = record else {
            return Ok(None);
        };
        if record.phone_id != phone_id {
            bail!("pairing record ownership mismatch");
        }
        let revoked = self.load_revoked()?;
        if revoked.pairing_ids.contains(&record.pairing_id) {
            return Ok(None);
        }
        Ok(Some(record))
    }

    pub fn save_record(&self, record: &PairedDeviceRecord) -> Result<()> {
        let root = pairing_root()?;
        let path = record_path(&record.phone_id)?;
        self.write_record(&root, &path, record)
    }

    pub fn delete_record(&self, phone_id: &str) -> Result<()> {
        let root = pairing_root()?;
        let path = record_path(phone_id)?;
        if root.is_file(&path)? {
            root.remove_file(&path)?;
        }
        for entry in root.list_root()? {
            if entry.kind != StoreEntryKind::File
                || entry.path == path
                || entry.path.file_name() == REVOKED_PAIRINGS_FILE
            {
                continue;
            }
            if self
                .read_record(&root, &entry.path)
                .is_ok_and(|record| record.phone_id == phone_id)
            {
                root.remove_file(&entry.path)?;
            }
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

    fn read_record(&self, root: &StoreRoot, path: &StorePath) -> Result<PairedDeviceRecord> {
        let raw = root.read_limited(path, MAX_PAIRING_RECORD_BYTES)?;
        if let Ok(envelope) = serde_json::from_slice::<EncryptedEnvelope>(&raw)
            && let Ok(plaintext) = self.decrypt(&envelope)
            && let Ok(record) = serde_json::from_slice::<PairedDeviceRecord>(&plaintext)
        {
            return Ok(record);
        }
        serde_json::from_slice(&raw).context("parse plaintext pairing record")
    }

    fn write_record(
        &self,
        root: &StoreRoot,
        path: &StorePath,
        record: &PairedDeviceRecord,
    ) -> Result<()> {
        let plaintext = serde_json::to_vec(record).context("serialize pairing record")?;
        let envelope = self.encrypt(&plaintext)?;
        let encoded =
            serde_json::to_vec_pretty(&envelope).context("serialize encrypted envelope")?;
        root.atomic_write(path, &encoded)?;
        Ok(())
    }

    fn load_revoked(&self) -> Result<RevokedPairings> {
        let root = pairing_root()?;
        let path = revoked_pairings_store_path();
        match root.read_limited(&path, MAX_PAIRING_RECORD_BYTES) {
            Ok(raw) => serde_json::from_slice(&raw).context("parse revoked pairings"),
            Err(error) if error.is_not_found() => Ok(RevokedPairings::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_revoked(&self, revoked: &RevokedPairings) -> Result<()> {
        let root = pairing_root()?;
        let encoded = serde_json::to_vec_pretty(revoked).context("serialize revoked pairings")?;
        root.atomic_write(&revoked_pairings_store_path(), &encoded)?;
        Ok(())
    }

    fn find_legacy_record(
        &self,
        root: &StoreRoot,
        phone_id: &str,
    ) -> Result<Option<PairedDeviceRecord>> {
        for entry in root.list_root()? {
            if entry.kind != StoreEntryKind::File || entry.path.file_name() == REVOKED_PAIRINGS_FILE
            {
                continue;
            }
            if let Ok(record) = self.read_record(root, &entry.path)
                && record.phone_id == phone_id
            {
                return Ok(Some(record));
            }
        }
        Ok(None)
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

fn pairing_root() -> Result<StoreRoot> {
    Ok(StoreRoot::open_or_create_nofollow(&pairings_dir())?)
}

fn record_path(phone_id: &str) -> Result<StorePath> {
    let phone_id = PairingDeviceId::parse(phone_id)?;
    Ok(StorePath::parse(&format!(
        "{}.json",
        phone_id.storage_key().as_str()
    ))?)
}

fn revoked_pairings_store_path() -> StorePath {
    StorePath::parse(REVOKED_PAIRINGS_FILE).expect("static pairing revocation path")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::identity::DeviceIdentity;
    use std::fs;

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
            credential_generation: 1,
            role: PairingRole::Portal,
            profile_id: None,
            mesh_grants: Vec::new(),
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
    fn legacy_record_defaults_to_first_credential_generation() {
        let value = serde_json::json!({
            "pairingId": "pairing",
            "phoneId": "phone",
            "phoneName": "Phone",
            "phonePublicKey": "key",
            "pairedAt": Utc::now(),
            "lastSeen": Utc::now(),
            "sessionTokenHash": "hash",
            "sessionTokenExpiry": Utc::now(),
            "role": "portal"
        });
        let record: PairedDeviceRecord = serde_json::from_value(value).unwrap();
        assert_eq!(record.credential_generation, 1);
    }

    #[test]
    fn pairing_device_paths_are_opaque_and_collision_free() {
        let colon = record_path("phone:a").unwrap();
        let underscore = record_path("phone_a").unwrap();
        assert_ne!(colon, underscore);
        assert!(!colon.file_name().contains("phone"));
        assert!(record_path(" phone").is_err());
    }

    #[test]
    fn list_paired_skips_unreadable_records() {
        let identity = DeviceIdentity::generate_ephemeral();
        let store = PairingStore::new(identity.signing_key());
        fs::create_dir_all(pairings_dir()).expect("pairings dir");
        let corrupt_path = pairings_dir().join("corrupt-test-phone.json");
        fs::write(&corrupt_path, br#"{"nonce":"bad","ciphertext":"bad"}"#)
            .expect("write corrupt record");
        let listed = store.list_paired().expect("list should not fail");
        assert!(
            listed
                .iter()
                .all(|record| record.phone_id != "corrupt-test-phone"),
            "corrupt record should be skipped"
        );
        let _ = fs::remove_file(corrupt_path);
    }
}
