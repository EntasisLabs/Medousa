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
    /// True when they connected to us (no outbound credentials); replies stay on this host for them to poll.
    #[serde(default)]
    pub inbound: bool,
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

/// True when Home is co-located with the workshop engine (loopback).
/// Phone / remote portal clients are separate surfaces — they must not use the host peer inbox.
fn is_host_engine(state: &State<'_, DaemonState>) -> bool {
    let Ok(base) = daemon_base(state) else {
        return false;
    };
    let host = if let Ok(url) = url::Url::parse(&base) {
        url.host_str().unwrap_or("").to_string()
    } else {
        base.trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(['/', ':'])
            .next()
            .unwrap_or("")
            .to_string()
    };
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]")
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

    // Inbound peers only exist on the host engine (people who Connected to this workshop).
    // A phone portal must not list the host's inbound peers as its own.
    if is_host_engine(&state) {
        if let Ok(status) = fetch_pair_status_value(&state).await {
            if let Some(devices) = status.get("pairedDevices").and_then(|v| v.as_array()) {
                for device in devices {
                    let role = device
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("portal");
                    if role != "peer" {
                        continue;
                    }
                    let phone_id = device
                        .get("phoneId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if phone_id.is_empty() {
                        continue;
                    }
                    if known_device_ids.iter().any(|id| {
                        id == &phone_id
                            || id.starts_with(&phone_id[..phone_id.len().min(8)])
                            || phone_id.starts_with(&id[..id.len().min(8)])
                    }) {
                        continue;
                    }
                    let label = device
                        .get("phoneName")
                        .and_then(|v| v.as_str())
                        .filter(|value| !value.is_empty())
                        .unwrap_or("Peer")
                        .to_string();
                    let paired_at = device
                        .get("pairedAt")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    out.push(TrustedWorkshopSummary {
                        workshop_id: format!("inbound-{phone_id}"),
                        label,
                        daemon_url: String::new(),
                        workshop_device_id: phone_id,
                        paired_at,
                        has_session_token: true,
                        has_iroh_ticket: false,
                        inbound: true,
                    });
                }
            }
        }
    }

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
    let (from_device_id, from_name) = local_identity(&state).await;
    let (to_device_id, to_name, remote_config) =
        resolve_peer_send_target(&state, &request.workshop_id).await?;

    let deliver_body = serde_json::json!({
        "body": request.body,
        "fromDeviceId": from_device_id,
        "fromName": from_name,
        "attachment": request.attachment,
    });

    // Deliver to their workshop when we have outbound credentials (Connect).
    if let Some(config) = remote_config {
        crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
            &config,
            "/v1/peer/messages",
            &deliver_body,
        )
        .await?;
    } else if !is_host_engine(&state) {
        return Err("Messaging requires a peer connection from this device".to_string());
    }

    let outbound = serde_json::json!({
        "id": format!("out_{}", uuid::Uuid::new_v4()),
        "body": request.body,
        "fromDeviceId": from_device_id,
        "fromName": from_name,
        "toDeviceId": to_device_id,
        "toName": to_name,
        "direction": "out",
        "sentAt": chrono::Utc::now().to_rfc3339(),
        "readAt": null,
        "attachment": request.attachment,
    });

    // Host engine keeps outbound copies in its own inbox (inbound peers poll them).
    // Phone / portal clients are separate surfaces — conversation lives on the peer host only.
    if is_host_engine(&state) {
        let local_body = serde_json::json!({
            "body": request.body,
            "fromDeviceId": from_device_id,
            "fromName": from_name,
            "toDeviceId": to_device_id,
            "toName": to_name,
            "direction": "out",
            "attachment": request.attachment,
        });
        let base = daemon_base(&state)?;
        let client = daemon_http_client()?;
        let response = client
            .post(format!("{base}/v1/peer/messages"))
            .json(&local_body)
            .send()
            .await
            .map_err(|err| format!("failed to record sent message: {err}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("failed to record sent message HTTP {status}: {body}"));
        }
        return response
            .json::<serde_json::Value>()
            .await
            .map_err(|err| err.to_string());
    }

    Ok(outbound)
}

async fn resolve_peer_send_target(
    state: &State<'_, DaemonState>,
    workshop_id: &str,
) -> Result<
    (
        String,
        String,
        Option<crate::pairing_client::WorkshopTransportConfig>,
    ),
    String,
> {
    if let Some(phone_id) = workshop_id.strip_prefix("inbound-") {
        let status = fetch_pair_status_value(state).await?;
        let device = status
            .get("pairedDevices")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .find(|entry| {
                entry.get("phoneId").and_then(|v| v.as_str()) == Some(phone_id)
                    && entry.get("role").and_then(|v| v.as_str()).unwrap_or("portal") == "peer"
            })
            .ok_or_else(|| format!("Inbound peer '{phone_id}' not found"))?;
        let label = device
            .get("phoneName")
            .and_then(|v| v.as_str())
            .unwrap_or("Peer")
            .to_string();
        return Ok((phone_id.to_string(), label, None));
    }

    let registry = crate::workshop_registry::load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == workshop_id)
        .ok_or_else(|| format!("Unknown workshop '{workshop_id}'"))?;
    if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
        return Err("Messaging requires a peer connection".to_string());
    }
    let pairing = workshop
        .pairing
        .as_ref()
        .ok_or_else(|| "Peer credentials are missing".to_string())?;
    let config = crate::pairing_client::load_workshop_transport_config_for_id(
        workshop_id,
        &workshop.url,
    )
    .ok_or_else(|| "Trusted workshop credentials are missing or expired".to_string())?;
    Ok((
        pairing.workshop_device_id.clone(),
        workshop.label.clone(),
        Some(config),
    ))
}

#[tauri::command]
pub async fn peer_list_messages(
    state: State<'_, DaemonState>,
    unread_only: Option<bool>,
) -> Result<serde_json::Value, String> {
    let unread_only = unread_only.unwrap_or(false);

    // Phone / portal client: conversations live on peer hosts we Connected to — not the portal inbox.
    if !is_host_engine(&state) {
        let messages = fetch_remote_peer_conversations(&state, unread_only, true).await;
        return Ok(serde_json::json!({ "messages": messages }));
    }

    let base = daemon_base(&state)?;
    let client = daemon_http_client()?;
    let mut url = format!("{base}/v1/peer/messages");
    if unread_only {
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
    let mut payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())?;

    // Host: pull replies from workshops we connected to (their outbound copies).
    let remote = fetch_remote_peer_conversations(&state, unread_only, false).await;
    if !remote.is_empty() {
        let messages = payload
            .get_mut("messages")
            .and_then(|value| value.as_array_mut());
        if let Some(messages) = messages {
            let mut seen: std::collections::HashSet<String> = messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("id")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .collect();
            for message in remote {
                let id = message
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() || seen.insert(id) {
                    messages.push(message);
                }
            }
        }
    }

    Ok(payload)
}

/// Pull conversations from peer hosts we Connected to (uses peer bearer credentials).
///
/// `full_thread`: when true (client surfaces), include our sends mirrored there as outbound.
/// When false (host), only their replies — our sends already live in the host inbox.
async fn fetch_remote_peer_conversations(
    state: &State<'_, DaemonState>,
    unread_only: bool,
    full_thread: bool,
) -> Vec<serde_json::Value> {
    let Ok(registry) = crate::workshop_registry::load_registry() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for workshop in registry.workshops {
        if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
            continue;
        }
        let Some(pairing) = workshop.pairing.as_ref() else {
            continue;
        };
        let Some(config) = crate::pairing_client::load_workshop_transport_config_for_id(
            &workshop.id,
            &workshop.url,
        ) else {
            continue;
        };
        let path = if unread_only {
            "/v1/peer/messages?unreadOnly=true"
        } else {
            "/v1/peer/messages"
        };
        let Ok(payload) =
            crate::workshop_transport::workshop_get_json::<serde_json::Value>(&config, path).await
        else {
            continue;
        };
        let Some(messages) = payload.get("messages").and_then(|value| value.as_array()) else {
            continue;
        };
        for message in messages {
            let direction = message
                .get("direction")
                .and_then(|value| value.as_str())
                .unwrap_or("in");
            // Remote "out" = they sent to us. Remote "in" = we sent to them.
            if direction == "out" {
                if unread_only && message.get("readAt").and_then(|value| value.as_str()).is_some() {
                    continue;
                }
                let mut mapped = message.clone();
                if let Some(object) = mapped.as_object_mut() {
                    object.insert("direction".into(), serde_json::json!("in"));
                    object.insert(
                        "fromDeviceId".into(),
                        serde_json::json!(pairing.workshop_device_id),
                    );
                    object.insert("fromName".into(), serde_json::json!(workshop.label));
                }
                out.push(mapped);
            } else if full_thread && direction == "in" {
                if unread_only {
                    continue;
                }
                let mut mapped = message.clone();
                if let Some(object) = mapped.as_object_mut() {
                    object.insert("direction".into(), serde_json::json!("out"));
                    object.insert(
                        "toDeviceId".into(),
                        serde_json::json!(pairing.workshop_device_id),
                    );
                    object.insert("toName".into(), serde_json::json!(workshop.label));
                }
                out.push(mapped);
            }
        }
    }
    out
}

#[tauri::command]
pub async fn peer_unread_count(state: State<'_, DaemonState>) -> Result<serde_json::Value, String> {
    if !is_host_engine(&state) {
        let unread = fetch_remote_peer_conversations(&state, true, true).await.len();
        return Ok(serde_json::json!({ "unread": unread }));
    }

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
    let mut payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())?;

    let remote_unread = fetch_remote_peer_conversations(&state, true, false).await.len();
    if remote_unread > 0 {
        let local = payload
            .get("unread")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        if let Some(object) = payload.as_object_mut() {
            object.insert("unread".into(), serde_json::json!(local + remote_unread as u64));
        }
    }
    Ok(payload)
}

#[tauri::command]
pub async fn peer_mark_read(
    state: State<'_, DaemonState>,
    message_id: String,
) -> Result<serde_json::Value, String> {
    if is_host_engine(&state) {
        let base = daemon_base(&state)?;
        let client = daemon_http_client()?;
        let response = client
            .post(format!("{base}/v1/peer/messages/{message_id}/read"))
            .send()
            .await
            .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
        if response.status().is_success() {
            return response
                .json::<serde_json::Value>()
                .await
                .map_err(|err| err.to_string());
        }
    }

    // Message may live on a remote peer host.
    let Ok(registry) = crate::workshop_registry::load_registry() else {
        return Err("message not found".to_string());
    };
    for workshop in registry.workshops {
        if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
            continue;
        }
        let Some(config) = crate::pairing_client::load_workshop_transport_config_for_id(
            &workshop.id,
            &workshop.url,
        ) else {
            continue;
        };
        if let Ok(value) = crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
            &config,
            &format!("/v1/peer/messages/{message_id}/read"),
            &serde_json::json!({}),
        )
        .await
        {
            return Ok(value);
        }
    }

    Err("message not found".to_string())
}

async fn local_identity(state: &State<'_, DaemonState>) -> (String, String) {
    // Host: workshop device identity. Client surface: this install's phone identity.
    if !is_host_engine(state) {
        if let Ok(identity) = crate::pairing_client::client_surface_identity() {
            return identity;
        }
        return ("client".to_string(), "Medousa".to_string());
    }

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
