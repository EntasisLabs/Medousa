//! Client rendezvous intros — consent + endpoint-hint exchange (M4+).
//!
//! Two Homes already paired to this workshop can request/accept an introduction.
//! Endpoints are redacted until accept. Not mesh mail; not ambient people sync.

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mesh::registry::{self, MeshPeerEndpoints};
use crate::mesh::store_io::{MESH_IO_LOCK, intros_path, read_json_default, write_json};

pub const DEFAULT_INTRO_TTL_SECS: i64 = 24 * 60 * 60;
const INTROS_CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshIntroStatus {
    Pending,
    Accepted,
    Declined,
    Expired,
}

impl MeshIntroStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "declined" => Some(Self::Declined),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshIntroRecord {
    pub id: String,
    pub requester_device_id: String,
    pub requester_display_name: String,
    pub target_device_id: String,
    pub target_display_name: String,
    pub status: MeshIntroStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub requester_endpoints: MeshPeerEndpoints,
    #[serde(default)]
    pub target_endpoints: MeshPeerEndpoints,
    /// Response-only: `"requester"` or `"target"` for the authenticated caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub you_are: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshIntrosFile {
    #[serde(default)]
    intros: Vec<MeshIntroRecord>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshIntroCandidate {
    pub device_id: String,
    pub display_name: String,
    pub role: String,
    pub last_seen: DateTime<Utc>,
}

fn empty_endpoints() -> MeshPeerEndpoints {
    MeshPeerEndpoints::default()
}

fn merge_endpoints(base: MeshPeerEndpoints, overlay: Option<MeshPeerEndpoints>) -> MeshPeerEndpoints {
    let Some(overlay) = overlay else {
        return base;
    };
    MeshPeerEndpoints {
        lan_base_url: overlay
            .lan_base_url
            .or(base.lan_base_url)
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty()),
        iroh_ticket: overlay
            .iroh_ticket
            .or(base.iroh_ticket)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        iroh_endpoint_id: overlay
            .iroh_endpoint_id
            .or(base.iroh_endpoint_id)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

fn device_ids_match(left: &str, right: &str) -> bool {
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return left == right;
    }
    left == right
        || (right.len() >= 8 && left.starts_with(&right[..8]))
        || (left.len() >= 8 && right.starts_with(&left[..8]))
}

fn expire_stale(file: &mut MeshIntrosFile, now: DateTime<Utc>) {
    for intro in &mut file.intros {
        if intro.status == MeshIntroStatus::Pending && intro.expires_at <= now {
            intro.status = MeshIntroStatus::Expired;
        }
    }
}

/// Redact the other party's endpoints until the intro is accepted.
pub fn view_for_caller(intro: &MeshIntroRecord, caller_device_id: &str) -> MeshIntroRecord {
    let mut view = intro.clone();
    view.you_are = if device_ids_match(&view.requester_device_id, caller_device_id) {
        Some("requester".to_string())
    } else if device_ids_match(&view.target_device_id, caller_device_id) {
        Some("target".to_string())
    } else {
        None
    };
    if view.status != MeshIntroStatus::Accepted {
        if view.you_are.as_deref() == Some("requester") {
            view.target_endpoints = empty_endpoints();
        } else if view.you_are.as_deref() == Some("target") {
            view.requester_endpoints = empty_endpoints();
        } else {
            view.requester_endpoints = empty_endpoints();
            view.target_endpoints = empty_endpoints();
        }
    }
    view
}

fn with_file_mut<T>(f: impl FnOnce(&mut MeshIntrosFile) -> Result<T>) -> Result<T> {
    let _guard = MESH_IO_LOCK.lock().expect("mesh io lock");
    let path = intros_path();
    let mut file: MeshIntrosFile = read_json_default(&path)?;
    expire_stale(&mut file, Utc::now());
    let result = f(&mut file)?;
    write_json(&path, &file)?;
    Ok(result)
}

pub fn list_for_caller(
    caller_device_id: &str,
    status_filter: Option<MeshIntroStatus>,
) -> Result<Vec<MeshIntroRecord>> {
    with_file_mut(|file| {
        Ok(file
            .intros
            .iter()
            .filter(|intro| {
                device_ids_match(&intro.requester_device_id, caller_device_id)
                    || device_ids_match(&intro.target_device_id, caller_device_id)
            })
            .filter(|intro| status_filter.is_none_or(|wanted| intro.status == wanted))
            .map(|intro| view_for_caller(intro, caller_device_id))
            .collect())
    })
}

pub fn get_intro(intro_id: &str) -> Result<Option<MeshIntroRecord>> {
    with_file_mut(|file| {
        Ok(file
            .intros
            .iter()
            .find(|intro| intro.id == intro_id.trim())
            .cloned())
    })
}

pub fn request_intro(
    requester_device_id: &str,
    requester_display_name: &str,
    target_device_id: &str,
    target_display_name: &str,
    note: Option<String>,
    endpoints_override: Option<MeshPeerEndpoints>,
) -> Result<MeshIntroRecord> {
    let requester = requester_device_id.trim();
    let target = target_device_id.trim();
    if requester.is_empty() || target.is_empty() {
        bail!("requester and target device ids are required");
    }
    if device_ids_match(requester, target) {
        bail!("cannot introduce yourself");
    }

    // Read registry before intros lock (shared MESH_IO_LOCK).
    let registry_endpoints = registry::get_peer(requester)?
        .map(|peer| peer.endpoints)
        .unwrap_or_default();
    let requester_endpoints = merge_endpoints(registry_endpoints, endpoints_override);

    with_file_mut(|file| {
        if file.intros.iter().any(|intro| {
            intro.status == MeshIntroStatus::Pending
                && device_ids_match(&intro.requester_device_id, requester)
                && device_ids_match(&intro.target_device_id, target)
        }) {
            bail!("a pending intro to this peer already exists");
        }

        let now = Utc::now();
        let intro = MeshIntroRecord {
            id: format!("mint_{}", Uuid::new_v4()),
            requester_device_id: requester.to_string(),
            requester_display_name: requester_display_name.trim().to_string(),
            target_device_id: target.to_string(),
            target_display_name: target_display_name.trim().to_string(),
            status: MeshIntroStatus::Pending,
            note: note
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            created_at: now,
            expires_at: now + Duration::seconds(DEFAULT_INTRO_TTL_SECS),
            accepted_at: None,
            requester_endpoints: requester_endpoints.clone(),
            target_endpoints: empty_endpoints(),
            you_are: None,
        };
        file.intros.push(intro.clone());
        while file.intros.len() > INTROS_CAP {
            file.intros.remove(0);
        }
        Ok(view_for_caller(&intro, requester))
    })
}

pub fn accept_intro(
    intro_id: &str,
    caller_device_id: &str,
    endpoints_override: Option<MeshPeerEndpoints>,
) -> Result<MeshIntroRecord> {
    // Read registry before intros lock (shared MESH_IO_LOCK).
    let registry_endpoints = registry::get_peer(caller_device_id)?
        .map(|peer| peer.endpoints)
        .unwrap_or_default();
    let target_endpoints = merge_endpoints(registry_endpoints, endpoints_override);

    with_file_mut(|file| {
        let now = Utc::now();
        let intro = file
            .intros
            .iter_mut()
            .find(|intro| intro.id == intro_id.trim())
            .ok_or_else(|| anyhow::anyhow!("intro not found"))?;

        if intro.status == MeshIntroStatus::Expired || intro.expires_at <= now {
            intro.status = MeshIntroStatus::Expired;
            bail!("intro expired");
        }
        if intro.status != MeshIntroStatus::Pending {
            bail!("intro is not pending");
        }
        if !device_ids_match(&intro.target_device_id, caller_device_id) {
            bail!("only the target can accept this intro");
        }

        intro.status = MeshIntroStatus::Accepted;
        intro.accepted_at = Some(now);
        intro.target_endpoints = target_endpoints.clone();
        Ok(view_for_caller(intro, caller_device_id))
    })
}

pub fn decline_intro(intro_id: &str, caller_device_id: &str) -> Result<MeshIntroRecord> {
    with_file_mut(|file| {
        let now = Utc::now();
        let intro = file
            .intros
            .iter_mut()
            .find(|intro| intro.id == intro_id.trim())
            .ok_or_else(|| anyhow::anyhow!("intro not found"))?;

        if intro.status == MeshIntroStatus::Expired || intro.expires_at <= now {
            intro.status = MeshIntroStatus::Expired;
            bail!("intro expired");
        }
        if intro.status != MeshIntroStatus::Pending {
            bail!("intro is not pending");
        }
        if !device_ids_match(&intro.target_device_id, caller_device_id) {
            bail!("only the target can decline this intro");
        }

        intro.status = MeshIntroStatus::Declined;
        Ok(view_for_caller(intro, caller_device_id))
    })
}

/// Opt-in candidate list: peers that already have `client.rendezvous` — never includes endpoints.
pub fn list_candidates(
    caller_device_id: &str,
    has_rendezvous: impl Fn(&str) -> bool,
) -> Result<Vec<MeshIntroCandidate>> {
    let peers = registry::list_peers()?;
    Ok(peers
        .into_iter()
        .filter(|peer| !device_ids_match(&peer.device_id, caller_device_id))
        .filter(|peer| peer.mesh_enabled)
        .filter(|peer| has_rendezvous(&peer.device_id))
        .map(|peer| MeshIntroCandidate {
            device_id: peer.device_id,
            display_name: peer.display_name,
            role: match peer.role {
                crate::pairing::PairingRole::Peer => "peer".to_string(),
                crate::pairing::PairingRole::Portal => "portal".to_string(),
            },
            last_seen: peer.last_seen,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_until_accepted() {
        let intro = MeshIntroRecord {
            id: "mint_1".into(),
            requester_device_id: "aaa".into(),
            requester_display_name: "A".into(),
            target_device_id: "bbb".into(),
            target_display_name: "B".into(),
            status: MeshIntroStatus::Pending,
            note: None,
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
            accepted_at: None,
            requester_endpoints: MeshPeerEndpoints {
                lan_base_url: Some("http://a.local:7419".into()),
                iroh_ticket: None,
                iroh_endpoint_id: None,
            },
            target_endpoints: MeshPeerEndpoints {
                lan_base_url: Some("http://b.local:7419".into()),
                iroh_ticket: None,
                iroh_endpoint_id: None,
            },
            you_are: None,
        };
        let for_a = view_for_caller(&intro, "aaa");
        assert!(for_a.requester_endpoints.lan_base_url.is_some());
        assert!(for_a.target_endpoints.lan_base_url.is_none());
        let for_b = view_for_caller(&intro, "bbb");
        assert!(for_b.requester_endpoints.lan_base_url.is_none());
        assert!(for_b.target_endpoints.lan_base_url.is_some());
    }
}
