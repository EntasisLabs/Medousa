//! Daemon-side mesh peer registry (projection over pairing + dial endpoints).

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::mesh::grants::effective_mesh_grants;
use crate::mesh::store_io::{MESH_IO_LOCK, peers_path, read_json_default, write_json};
use crate::pairing::{PairedDeviceRecord, PairingRole};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshPeerEndpoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshPeerRecord {
    pub device_id: String,
    pub display_name: String,
    pub public_key_b64: String,
    pub pairing_id: String,
    pub role: PairingRole,
    #[serde(default)]
    pub mesh_grants: Vec<String>,
    #[serde(default = "default_true")]
    pub mesh_enabled: bool,
    pub last_seen: DateTime<Utc>,
    #[serde(default)]
    pub endpoints: MeshPeerEndpoints,
    /// Next outbound seq this workshop will assign toward this peer.
    #[serde(default)]
    pub outbound_seq: u64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshPeersFile {
    #[serde(default)]
    peers: Vec<MeshPeerRecord>,
}

pub fn list_peers() -> Result<Vec<MeshPeerRecord>> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let file: MeshPeersFile = read_json_default(&peers_path())?;
    Ok(file.peers)
}

pub fn get_peer(device_id: &str) -> Result<Option<MeshPeerRecord>> {
    let trimmed = device_id.trim();
    Ok(list_peers()?
        .into_iter()
        .find(|peer| device_ids_match(&peer.device_id, trimmed)))
}

pub fn upsert_from_pairing(record: &PairedDeviceRecord) -> Result<MeshPeerRecord> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = peers_path();
    let mut file: MeshPeersFile = read_json_default(&path)?;
    let grants = effective_mesh_grants(record);
    let idx = file
        .peers
        .iter()
        .position(|peer| peer.device_id == record.phone_id || peer.pairing_id == record.pairing_id);
    let peer = if let Some(idx) = idx {
        let existing = &mut file.peers[idx];
        existing.device_id = record.phone_id.clone();
        existing.display_name = record.phone_name.clone();
        existing.public_key_b64 = record.phone_public_key.clone();
        existing.pairing_id = record.pairing_id.clone();
        existing.role = record.role;
        existing.mesh_grants = grants;
        existing.last_seen = record.last_seen;
        existing.clone()
    } else {
        let peer = MeshPeerRecord {
            device_id: record.phone_id.clone(),
            display_name: record.phone_name.clone(),
            public_key_b64: record.phone_public_key.clone(),
            pairing_id: record.pairing_id.clone(),
            role: record.role,
            mesh_grants: grants,
            mesh_enabled: true,
            last_seen: record.last_seen,
            endpoints: MeshPeerEndpoints::default(),
            outbound_seq: 1,
        };
        file.peers.push(peer.clone());
        peer
    };
    write_json(&path, &file)?;
    Ok(peer)
}

pub fn set_mesh_enabled(device_id: &str, enabled: bool) -> Result<MeshPeerRecord> {
    update_peer(device_id, |peer| {
        peer.mesh_enabled = enabled;
    })
}

pub fn set_endpoints(device_id: &str, endpoints: MeshPeerEndpoints) -> Result<MeshPeerRecord> {
    update_peer(device_id, |peer| {
        if let Some(url) = endpoints
            .lan_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            peer.endpoints.lan_base_url = Some(url.trim_end_matches('/').to_string());
        }
        if let Some(ticket) = endpoints
            .iroh_ticket
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            peer.endpoints.iroh_ticket = Some(ticket.to_string());
        }
        if let Some(endpoint) = endpoints
            .iroh_endpoint_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            peer.endpoints.iroh_endpoint_id = Some(endpoint.to_string());
        }
        peer.last_seen = Utc::now();
    })
}

pub fn set_grants(device_id: &str, grants: Vec<String>) -> Result<MeshPeerRecord> {
    update_peer(device_id, |peer| {
        peer.mesh_grants = grants
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
    })
}

/// Allocate the next monotonic outbound seq for deliveries to `device_id`.
pub fn allocate_outbound_seq(device_id: &str) -> Result<u64> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = peers_path();
    let mut file: MeshPeersFile = read_json_default(&path)?;
    let peer = file
        .peers
        .iter_mut()
        .find(|peer| device_ids_match(&peer.device_id, device_id))
        .with_context(|| format!("mesh peer not registered: {device_id}"))?;
    if !peer.mesh_enabled {
        bail!("mesh disabled for peer {device_id}");
    }
    if peer.outbound_seq == 0 {
        peer.outbound_seq = 1;
    }
    let seq = peer.outbound_seq;
    peer.outbound_seq = peer.outbound_seq.saturating_add(1);
    write_json(&path, &file)?;
    Ok(seq)
}

pub fn touch_last_seen(device_id: &str) -> Result<()> {
    let _ = update_peer(device_id, |peer| {
        peer.last_seen = Utc::now();
    });
    Ok(())
}

fn update_peer(
    device_id: &str,
    mutate: impl FnOnce(&mut MeshPeerRecord),
) -> Result<MeshPeerRecord> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = peers_path();
    let mut file: MeshPeersFile = read_json_default(&path)?;
    let peer = file
        .peers
        .iter_mut()
        .find(|peer| device_ids_match(&peer.device_id, device_id))
        .with_context(|| format!("mesh peer not registered: {device_id}"))?;
    mutate(peer);
    let snapshot = peer.clone();
    write_json(&path, &file)?;
    Ok(snapshot)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::PairingRole;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn sample_pairing(phone_id: &str) -> PairedDeviceRecord {
        PairedDeviceRecord {
            pairing_id: format!("pair-{phone_id}"),
            phone_id: phone_id.into(),
            phone_name: "Peer".into(),
            phone_public_key: "pk".into(),
            paired_at: Utc::now(),
            last_seen: Utc::now(),
            session_token_hash: "h".into(),
            session_token_expiry: Utc::now(),
            credential_generation: 1,
            role: PairingRole::Peer,
            profile_id: None,
            mesh_grants: vec!["mesh.message".into()],
            apns_device_token: None,
            push_platform: None,
            push_updated_at: None,
            live_activity_push_token: None,
            live_activity_push_updated_at: None,
        }
    }

    #[test]
    fn upsert_and_allocate_seq() {
        let _guard = TEST_LOCK.lock().unwrap();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let phone_id = format!("peer-{suffix}");
        let tmp = std::env::temp_dir().join(format!("medousa-mesh-reg-{suffix}"));
        std::fs::create_dir_all(&tmp).unwrap();
        let _env = crate::test_env::set_var("XDG_DATA_HOME", &tmp);

        let peer = upsert_from_pairing(&sample_pairing(&phone_id)).expect("upsert");
        assert!(peer.mesh_enabled);
        assert_eq!(allocate_outbound_seq(&phone_id).unwrap(), 1);
        assert_eq!(allocate_outbound_seq(&phone_id).unwrap(), 2);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
