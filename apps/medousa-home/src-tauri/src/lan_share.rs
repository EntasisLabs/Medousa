use std::time::Duration;

use crate::daemon::DaemonState;
use crate::peer_inbox_sink::{self, PeerSendMessageRequest, TrustedWorkshopSummary};
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
    peer_inbox_sink::daemon_base(state)
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

fn normalize_lan_daemon_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Workshop URL is required".to_string());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    Ok(format!("http://{trimmed}"))
}

#[tauri::command]
pub async fn lan_connect_workshop(
    request: LanConnectWorkshopRequest,
) -> Result<crate::pairing_client::PairCompleteFromQrResult, String> {
    let daemon_url = normalize_lan_daemon_url(&request.daemon_url)?;
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
pub async fn list_trusted_workshops(
    state: State<'_, DaemonState>,
) -> Result<Vec<TrustedWorkshopSummary>, String> {
    let registry = crate::workshop_registry::load_registry()?;
    let mut out = Vec::new();
    let mut known_device_ids = Vec::new();
    for workshop in registry.workshops {
        if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
            continue;
        }
        let Some(pairing) = workshop.pairing else {
            continue;
        };
        known_device_ids.push(pairing.workshop_device_id.clone());
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
            inbound: false,
        });
    }

    peer_inbox_sink::append_inbound_peers(&state, &mut out, &mut known_device_ids).await?;

    out.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(out)
}

async fn fetch_pair_status_value(
    state: &State<'_, DaemonState>,
) -> Result<serde_json::Value, String> {
    let base = daemon_base(state)?;
    let client = daemon_http_client()?;
    let response = client
        .get(format!("{base}/pair/status"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("pair status HTTP {}", response.status()));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn revoke_trusted_workshop(
    state: State<'_, DaemonState>,
    workshop_id: String,
) -> Result<(), String> {
    let trimmed = workshop_id.trim();
    if trimmed.is_empty() {
        return Err("Workshop id is required".to_string());
    }

    if let Some(phone_id) = trimmed.strip_prefix("inbound-") {
        let status = fetch_pair_status_value(&state).await?;
        let pairing_id = status
            .get("pairedDevices")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .find(|device| {
                device.get("phoneId").and_then(|v| v.as_str()) == Some(phone_id)
                    && device.get("role").and_then(|v| v.as_str()) == Some("peer")
            })
            .and_then(|device| device.get("pairingId").and_then(|v| v.as_str()))
            .ok_or_else(|| format!("Inbound peer '{phone_id}' not found"))?
            .to_string();
        let base = daemon_base(&state)?;
        let client = daemon_http_client()?;
        let response = client
            .delete(format!("{base}/pair/{pairing_id}"))
            .send()
            .await
            .map_err(|err| err.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Failed to revoke inbound peer HTTP {}",
                response.status()
            ));
        }
        return Ok(());
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
    crate::workshop_transport::invalidate_all_route_caches();
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

#[tauri::command]
pub async fn peer_send_message(
    state: State<'_, DaemonState>,
    request: PeerSendMessageRequest,
) -> Result<serde_json::Value, String> {
    peer_inbox_sink::send_message(&state, request).await
}


#[tauri::command]
pub async fn peer_list_messages(
    state: State<'_, DaemonState>,
    unread_only: Option<bool>,
) -> Result<serde_json::Value, String> {
    let unread_only = unread_only.unwrap_or(false);
    let messages = peer_inbox_sink::list_messages(&state, unread_only).await?;
    Ok(serde_json::json!({ "messages": messages }))
}

#[tauri::command]
pub async fn peer_unread_count(state: State<'_, DaemonState>) -> Result<serde_json::Value, String> {
    let unread = peer_inbox_sink::unread_count(&state).await?;
    Ok(serde_json::json!({ "unread": unread }))
}

#[tauri::command]
pub async fn peer_mark_read(
    state: State<'_, DaemonState>,
    message_id: String,
    sink_kind: Option<String>,
    workshop_id: Option<String>,
) -> Result<serde_json::Value, String> {
    peer_inbox_sink::mark_read(&state, message_id, sink_kind, workshop_id).await
}

#[tauri::command]
pub async fn peer_mark_thread_read(
    state: State<'_, DaemonState>,
    peer_device_id: String,
) -> Result<serde_json::Value, String> {
    let marked = peer_inbox_sink::mark_thread_read(&state, peer_device_id).await?;
    Ok(serde_json::json!({ "marked": marked }))
}

#[tauri::command]
pub async fn peer_compose_identity(
    state: State<'_, DaemonState>,
) -> Result<serde_json::Value, String> {
    let workshop_name = peer_inbox_sink::workshop_display_name(&state).await;
    let client_name = peer_inbox_sink::client_surface_name(&state).await;
    Ok(serde_json::json!({
        "workshopName": workshop_name,
        "clientName": client_name,
    }))
}
