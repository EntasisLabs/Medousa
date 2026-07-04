use std::time::Duration;

use crate::daemon::DaemonState;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredWorkshop {
    pub instance_name: String,
    pub host: String,
    pub port: u16,
    pub device_id: Option<String>,
    pub peer_name: Option<String>,
    pub protocol_version: Option<String>,
    pub capability_flags: Option<String>,
    pub auth_required: Option<bool>,
    pub model_descriptor: Option<String>,
    pub daemon_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanWorkshopsResponse {
    pub workshops: Vec<DiscoveredWorkshop>,
    pub browse_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedWorkshopSummary {
    pub workshop_id: String,
    pub label: String,
    pub daemon_url: String,
    pub workshop_device_id: String,
    pub paired_at: String,
    pub has_session_token: bool,
    pub has_iroh_ticket: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShareExportInvokeRequest {
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub vault_paths: Vec<String>,
    #[serde(default)]
    pub include_environment: bool,
    #[serde(default)]
    pub surface_ids: Vec<String>,
    #[serde(default)]
    pub component_ids: Vec<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareImportInvokeRequest {
    pub bundle: serde_json::Value,
    #[serde(default)]
    pub conflict_strategy: String,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharePushInvokeRequest {
    pub workshop_id: String,
    pub bundle: serde_json::Value,
    #[serde(default)]
    pub conflict_strategy: String,
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustWorkshopFromQrRequest {
    pub qr_url: String,
    pub daemon_url: String,
    #[serde(default)]
    pub workshop_name: Option<String>,
}

fn daemon_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())
}

fn daemon_base(state: &State<'_, DaemonState>) -> Result<String, String> {
    Ok(state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone()
        .trim_end_matches('/')
        .to_string())
}

#[tauri::command]
pub fn lan_pairing_status() -> Result<crate::workshop_runtime::LanPairingStatus, String> {
    crate::workshop_runtime::lan_pairing_status()
}

#[tauri::command]
pub async fn set_lan_pairing_enabled(
    state: State<'_, DaemonState>,
    enabled: bool,
) -> Result<crate::workshop_runtime::LanPairingStatus, String> {
    crate::workshop_runtime::set_lan_pairing_enabled(&state, enabled).await
}

#[tauri::command]
pub async fn lan_discover_workshops(
    state: State<'_, DaemonState>,
) -> Result<LanWorkshopsResponse, String> {
    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let response = client
        .get(format!("{base}/v1/lan/workshops"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Medousa Engine returned HTTP {status}: {body}"));
    }
    response
        .json::<LanWorkshopsResponse>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn share_export_bundle(
    state: State<'_, DaemonState>,
    request: ShareExportInvokeRequest,
) -> Result<serde_json::Value, String> {
    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let response = client
        .post(format!("{base}/v1/share/export"))
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Share export failed HTTP {status}: {body}"));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn share_import_bundle(
    state: State<'_, DaemonState>,
    request: ShareImportInvokeRequest,
) -> Result<serde_json::Value, String> {
    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let response = client
        .post(format!("{base}/v1/share/import"))
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Share import failed HTTP {status}: {body}"));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn share_push_to_workshop(
    request: SharePushInvokeRequest,
) -> Result<serde_json::Value, String> {
    let registry = crate::workshop_registry::load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == request.workshop_id)
        .ok_or_else(|| format!("Unknown workshop '{}'", request.workshop_id))?;
    if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
        return Err("Share push requires a peer connection (inbox-only)".to_string());
    }
    let config = crate::pairing_client::load_workshop_transport_config_for_id(
        &request.workshop_id,
        &workshop.url,
    )
    .ok_or_else(|| "Trusted workshop credentials are missing or expired".to_string())?;

    let body = serde_json::json!({
        "bundle": request.bundle,
        "conflictStrategy": request.conflict_strategy,
        "profileId": request.profile_id,
    });

    crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
        &config,
        "/v1/share/push",
        &body,
    )
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanConnectWorkshopRequest {
    pub daemon_url: String,
    #[serde(default)]
    pub peer_name: Option<String>,
}

#[tauri::command]
pub async fn lan_connect_workshop(
    request: LanConnectWorkshopRequest,
) -> Result<crate::pairing_client::PairCompleteFromQrResult, String> {
    let daemon_url = request
        .daemon_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    if daemon_url.is_empty() {
        return Err("Workshop URL is required".to_string());
    }
    let client = daemon_http_client()?;
    let response = client
        .get(format!("{daemon_url}/qr"))
        .send()
        .await
        .map_err(|err| format!("cannot reach workshop at {daemon_url}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Could not fetch invite from workshop (HTTP {status}): {body}"
        ));
    }
    let qr = response
        .json::<PairingQrResponse>()
        .await
        .map_err(|err| format!("invalid workshop invite response: {err}"))?;
    let workshop_name = request
        .peer_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Trusted workshop")
        .to_string();
    crate::pairing_client::pair_complete_from_qr(crate::pairing_client::PairCompleteFromQrRequest {
        qr_url: qr.url,
        daemon_url,
        phone_name: Some(workshop_name),
        role: Some("peer".to_string()),
    })
    .await
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingQrResponse {
    url: String,
    expires_at: String,
    short_code: String,
}

#[tauri::command]
pub async fn trust_workshop_from_qr(
    request: TrustWorkshopFromQrRequest,
) -> Result<crate::pairing_client::PairCompleteFromQrResult, String> {
    let workshop_name = request
        .workshop_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Trusted peer")
        .to_string();
    crate::pairing_client::pair_complete_from_qr(crate::pairing_client::PairCompleteFromQrRequest {
        qr_url: request.qr_url,
        daemon_url: request.daemon_url,
        phone_name: Some(workshop_name),
        role: Some("peer".to_string()),
    })
    .await
}

#[tauri::command]
pub fn list_trusted_workshops() -> Result<Vec<TrustedWorkshopSummary>, String> {
    let registry = crate::workshop_registry::load_registry()?;
    let mut out = Vec::new();
    for workshop in registry.workshops {
        if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
            continue;
        }
        let Some(pairing) = workshop.pairing else {
            continue;
        };
        let has_session_token = crate::pairing_client::workshop_has_session_token(
            &workshop.id,
            &pairing.workshop_device_id,
        );
        out.push(TrustedWorkshopSummary {
            workshop_id: workshop.id,
            label: workshop.label,
            daemon_url: workshop.url,
            workshop_device_id: pairing.workshop_device_id,
            paired_at: pairing.paired_at,
            has_session_token,
            has_iroh_ticket: pairing.has_iroh_ticket.unwrap_or(false),
        });
    }
    out.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(out)
}

#[tauri::command]
pub fn revoke_trusted_workshop(workshop_id: String) -> Result<(), String> {
    let trimmed = workshop_id.trim();
    if trimmed.is_empty() {
        return Err("Workshop id is required".to_string());
    }
    let mut registry = crate::workshop_registry::load_registry()?;
    let removed = registry
        .workshops
        .iter()
        .find(|entry| entry.id == trimmed)
        .cloned()
        .ok_or_else(|| format!("Unknown workshop '{trimmed}'"))?;
    if !crate::workshop_registry::is_peer_kind(&removed.kind) {
        return Err("Only peer connections can be revoked here".to_string());
    }
    let device_id = removed
        .pairing
        .as_ref()
        .map(|pairing| pairing.workshop_device_id.as_str());
    crate::pairing_client::remove_workshop_credentials(trimmed, device_id)?;
    registry.workshops.retain(|entry| entry.id != trimmed);
    if registry.active_workshop_id == trimmed {
        registry.active_workshop_id = crate::workshop_registry::PERSONAL_WORKSHOP_ID.to_string();
    }
    crate::workshop_registry::save_registry(&registry)?;
    crate::workshop_transport::invalidate_workshop_route_cache();
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareItemToPeerRequest {
    pub workshop_id: String,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub vault_path: Option<String>,
    #[serde(default)]
    pub conflict_strategy: String,
}

#[tauri::command]
pub async fn share_item_to_peer(
    state: State<'_, DaemonState>,
    request: ShareItemToPeerRequest,
) -> Result<serde_json::Value, String> {
    let artifact_id = request
        .artifact_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let vault_path = request
        .vault_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if artifact_id.is_none() && vault_path.is_none() {
        return Err("artifactId or vaultPath is required".to_string());
    }
    if artifact_id.is_some() && vault_path.is_some() {
        return Err("Share one item at a time (artifact or note)".to_string());
    }

    let export_request = ShareExportInvokeRequest {
        artifact_ids: artifact_id.map(|id| vec![id.to_string()]).unwrap_or_default(),
        vault_paths: vault_path.map(|path| vec![path.to_string()]).unwrap_or_default(),
        ..Default::default()
    };
    let bundle = share_export_bundle(state, export_request).await?;
    share_push_to_workshop(SharePushInvokeRequest {
        workshop_id: request.workshop_id,
        bundle,
        conflict_strategy: if request.conflict_strategy.trim().is_empty() {
            "rename".to_string()
        } else {
            request.conflict_strategy
        },
        profile_id: None,
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerSendMessageRequest {
    pub workshop_id: String,
    pub body: String,
    #[serde(default)]
    pub attachment: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn peer_send_message(
    state: State<'_, DaemonState>,
    request: PeerSendMessageRequest,
) -> Result<serde_json::Value, String> {
    let registry = crate::workshop_registry::load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == request.workshop_id)
        .ok_or_else(|| format!("Unknown workshop '{}'", request.workshop_id))?;
    if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
        return Err("Messaging requires a peer connection".to_string());
    }
    let config = crate::pairing_client::load_workshop_transport_config_for_id(
        &request.workshop_id,
        &workshop.url,
    )
    .ok_or_else(|| "Trusted workshop credentials are missing or expired".to_string())?;

    let (from_device_id, from_name) = local_identity(&state).await;

    let body = serde_json::json!({
        "body": request.body,
        "fromDeviceId": from_device_id,
        "fromName": from_name,
        "attachment": request.attachment,
    });

    crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
        &config,
        "/v1/peer/messages",
        &body,
    )
    .await
}

#[tauri::command]
pub async fn peer_list_messages(
    state: State<'_, DaemonState>,
    unread_only: Option<bool>,
) -> Result<serde_json::Value, String> {
    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let mut url = format!("{base}/v1/peer/messages");
    if unread_only.unwrap_or(false) {
        url.push_str("?unreadOnly=true");
    }
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("List peer messages failed HTTP {status}: {body}"));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn peer_unread_count(state: State<'_, DaemonState>) -> Result<serde_json::Value, String> {
    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let response = client
        .get(format!("{base}/v1/peer/messages/unread-count"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Unread count failed HTTP {status}: {body}"));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn peer_mark_read(
    state: State<'_, DaemonState>,
    message_id: String,
) -> Result<serde_json::Value, String> {
    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let response = client
        .post(format!("{base}/v1/peer/messages/{message_id}/read"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Mark read failed HTTP {status}: {body}"));
    }
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())
}

async fn local_identity(state: &State<'_, DaemonState>) -> (String, String) {
    let base = match daemon_base(state) {
        Ok(base) => base,
        Err(_) => return ("local".to_string(), "Medousa".to_string()),
    };
    let client = match daemon_http_client() {
        Ok(client) => client,
        Err(_) => return ("local".to_string(), "Medousa".to_string()),
    };
    let Ok(response) = client.get(format!("{base}/pair/status")).send().await else {
        return ("local".to_string(), "Medousa".to_string());
    };
    if !response.status().is_success() {
        return ("local".to_string(), "Medousa".to_string());
    }
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return ("local".to_string(), "Medousa".to_string());
    };
    let device_id = value
        .get("deviceId")
        .and_then(|v| v.as_str())
        .unwrap_or("local")
        .to_string();
    let peer_name = value
        .get("peerName")
        .and_then(|v| v.as_str())
        .unwrap_or("Medousa")
        .to_string();
    (device_id, peer_name)
}
