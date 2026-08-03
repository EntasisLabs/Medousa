//! M4+ client rendezvous — Home calls workshop `/v1/mesh/intros/*`.

use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;
use crate::workshop_registry::{self, PERSONAL_WORKSHOP_ID};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPeerEndpoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshIntroRecord {
    pub id: String,
    pub requester_device_id: String,
    pub requester_display_name: String,
    pub target_device_id: String,
    pub target_display_name: String,
    pub status: String,
    #[serde(default)]
    pub note: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub requester_endpoints: MeshPeerEndpoints,
    #[serde(default)]
    pub target_endpoints: MeshPeerEndpoints,
    #[serde(default)]
    pub you_are: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshIntroCandidate {
    pub device_id: String,
    pub display_name: String,
    pub role: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPeerGrantRow {
    pub device_id: String,
    pub display_name: String,
    pub role: String,
    pub mesh_grants: Vec<String>,
    pub rendezvous: bool,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroWorkshopSummary {
    pub workshop_id: String,
    pub label: String,
    pub kind: String,
    pub has_session_token: bool,
}

fn local_endpoints() -> MeshPeerEndpoints {
    let lan = crate::workshop_runtime::lan_pairing_status()
        .ok()
        .filter(|status| status.enabled)
        .map(|status| status.url.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    MeshPeerEndpoints {
        lan_base_url: lan,
        iroh_ticket: None,
        iroh_endpoint_id: None,
    }
}

fn transport_for_workshop(workshop_id: &str) -> Result<WorkshopTransportConfig, String> {
    let registry = workshop_registry::load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == workshop_id)
        .ok_or_else(|| format!("Unknown workshop '{workshop_id}'"))?;
    crate::pairing_client::load_workshop_transport_config_for_id(workshop_id, &workshop.url)
        .ok_or_else(|| "Workshop credentials missing or expired — reconnect this door.".to_string())
}

#[tauri::command]
pub fn list_intro_workshops() -> Result<Vec<IntroWorkshopSummary>, String> {
    let registry = workshop_registry::load_registry()?;
    let mut out = Vec::new();
    for workshop in registry.workshops {
        if workshop.id == PERSONAL_WORKSHOP_ID {
            continue;
        }
        let Some(pairing) = workshop.pairing.as_ref() else {
            continue;
        };
        if !workshop_registry::is_portal_kind(&workshop.kind)
            && !workshop_registry::is_peer_kind(&workshop.kind)
        {
            continue;
        }
        let has_session_token = crate::pairing_client::workshop_has_session_token(
            &workshop.id,
            &pairing.workshop_device_id,
        );
        out.push(IntroWorkshopSummary {
            workshop_id: workshop.id,
            label: workshop.label,
            kind: workshop.kind,
            has_session_token,
        });
    }
    out.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(out)
}

#[tauri::command]
pub async fn mesh_list_intros(
    workshop_id: String,
    status: Option<String>,
) -> Result<Vec<MeshIntroRecord>, String> {
    let config = transport_for_workshop(workshop_id.trim())?;
    let path = match status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(status) => format!("/v1/mesh/intros?status={status}"),
        None => "/v1/mesh/intros".to_string(),
    };
    let response =
        crate::workshop_transport::workshop_get_json::<serde_json::Value>(&config, &path).await?;
    Ok(response
        .get("intros")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn mesh_list_intro_candidates(
    workshop_id: String,
) -> Result<Vec<MeshIntroCandidate>, String> {
    let config = transport_for_workshop(workshop_id.trim())?;
    let response = crate::workshop_transport::workshop_get_json::<serde_json::Value>(
        &config,
        "/v1/mesh/intros/candidates",
    )
    .await?;
    Ok(response
        .get("candidates")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn mesh_request_intro(
    workshop_id: String,
    target_device_id: String,
    note: Option<String>,
) -> Result<MeshIntroRecord, String> {
    let config = transport_for_workshop(workshop_id.trim())?;
    let endpoints = local_endpoints();
    let body = serde_json::json!({
        "targetDeviceId": target_device_id.trim(),
        "note": note,
        "endpoints": endpoints,
    });
    crate::workshop_transport::workshop_post_json::<MeshIntroRecord, _>(
        &config,
        "/v1/mesh/intros",
        &body,
    )
    .await
}

#[tauri::command]
pub async fn mesh_accept_intro(
    workshop_id: String,
    intro_id: String,
) -> Result<MeshIntroRecord, String> {
    let config = transport_for_workshop(workshop_id.trim())?;
    let body = serde_json::json!({ "endpoints": local_endpoints() });
    crate::workshop_transport::workshop_post_json::<MeshIntroRecord, _>(
        &config,
        &format!("/v1/mesh/intros/{}/accept", intro_id.trim()),
        &body,
    )
    .await
}

#[tauri::command]
pub async fn mesh_decline_intro(
    workshop_id: String,
    intro_id: String,
) -> Result<MeshIntroRecord, String> {
    let config = transport_for_workshop(workshop_id.trim())?;
    crate::workshop_transport::workshop_post_empty_json::<MeshIntroRecord>(
        &config,
        &format!("/v1/mesh/intros/{}/decline", intro_id.trim()),
    )
    .await
}

fn daemon_base_from_state(state: &State<'_, DaemonState>) -> String {
    state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone()
        .trim_end_matches('/')
        .to_string()
}

async fn fetch_local_mesh_peers(base: &str) -> Result<Vec<MeshPeerGrantRow>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(format!("{base}/v1/mesh/peers"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("mesh peers HTTP {status}: {text}"));
    }
    let value: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    let peers = value
        .get("peers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(peers
        .into_iter()
        .filter_map(|peer| {
            let device_id = peer.get("deviceId")?.as_str()?.to_string();
            let display_name = peer
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or("Peer")
                .to_string();
            let role = peer
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("peer")
                .to_string();
            let mesh_grants = peer
                .get("meshGrants")
                .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                .unwrap_or_default();
            let rendezvous = mesh_grants
                .iter()
                .any(|grant| grant.eq_ignore_ascii_case("client.rendezvous"));
            let last_seen = peer
                .get("lastSeen")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(MeshPeerGrantRow {
                device_id,
                display_name,
                role,
                mesh_grants,
                rendezvous,
                last_seen,
            })
        })
        .collect())
}

/// Host-local: list mesh peers for grant management on the personal/active engine.
#[tauri::command]
pub async fn mesh_list_local_peers(
    state: State<'_, DaemonState>,
) -> Result<Vec<MeshPeerGrantRow>, String> {
    fetch_local_mesh_peers(&daemon_base_from_state(&state)).await
}

#[tauri::command]
pub async fn mesh_set_peer_rendezvous(
    state: State<'_, DaemonState>,
    device_id: String,
    enabled: bool,
) -> Result<MeshPeerGrantRow, String> {
    let base = daemon_base_from_state(&state);
    let peers = fetch_local_mesh_peers(&base).await?;
    let current = peers
        .iter()
        .find(|peer| peer.device_id == device_id.trim())
        .cloned()
        .ok_or_else(|| format!("mesh peer not found: {device_id}"))?;
    let mut grants = current
        .mesh_grants
        .iter()
        .filter(|grant| !grant.eq_ignore_ascii_case("client.rendezvous"))
        .cloned()
        .collect::<Vec<_>>();
    if enabled {
        grants.push("client.rendezvous".to_string());
    }
    if grants.is_empty() {
        grants = vec!["mesh.message".to_string(), "mesh.bundle.push".to_string()];
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .patch(format!("{base}/v1/mesh/peers/{}", device_id.trim()))
        .json(&serde_json::json!({ "meshGrants": grants }))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("mesh peer patch HTTP {status}: {text}"));
    }
    let peer: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    let mesh_grants = peer
        .get("meshGrants")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or(grants);
    Ok(MeshPeerGrantRow {
        device_id: peer
            .get("deviceId")
            .and_then(|v| v.as_str())
            .unwrap_or(device_id.trim())
            .to_string(),
        display_name: peer
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&current.display_name)
            .to_string(),
        role: peer
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or(&current.role)
            .to_string(),
        rendezvous: mesh_grants
            .iter()
            .any(|grant| grant.eq_ignore_ascii_case("client.rendezvous")),
        last_seen: peer
            .get("lastSeen")
            .and_then(|v| v.as_str())
            .unwrap_or(&current.last_seen)
            .to_string(),
        mesh_grants,
    })
}
