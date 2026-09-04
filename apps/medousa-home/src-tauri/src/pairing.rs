use crate::daemon::DaemonState;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingQrResponse {
    pub url: String,
    pub expires_at: String,
    pub short_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceSummary {
    pub pairing_id: String,
    pub phone_id: String,
    pub phone_name: String,
    pub paired_at: String,
    pub last_seen: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
    #[serde(default = "default_true")]
    pub trust_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStatusResponse {
    pub paired_devices: Vec<PairedDeviceSummary>,
    pub qr_active: bool,
    pub device_id: String,
    pub peer_name: String,
    pub protocol_version: String,
    pub daemon_public_key: String,
    pub iroh_available: bool,
    pub qr_protocol_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingQrImage {
    pub data_url: String,
    pub url: String,
    pub expires_at: String,
    pub short_code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BonjourStatus {
    pub pairing_available: bool,
    pub likely_advertising: bool,
    pub service_type: String,
    pub device_id: Option<String>,
    pub peer_name: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PairHeartbeatInvokeRequest {
    #[serde(default)]
    pub apns_device_token: Option<String>,
    #[serde(default)]
    pub push_platform: Option<String>,
    #[serde(default)]
    pub live_activity_push_token: Option<String>,
    /// Optional mesh dial-back hints (M4+ rendezvous).
    #[serde(default)]
    pub mesh_lan_base_url: Option<String>,
    #[serde(default)]
    pub mesh_iroh_ticket: Option<String>,
    #[serde(default)]
    pub mesh_iroh_endpoint_id: Option<String>,
}

fn daemon_base(_state: &State<'_, DaemonState>) -> Result<String, String> {
    crate::active_workshop::transport_base_url()
}

#[tauri::command]
pub async fn pairing_fetch_qr(
    _state: State<'_, DaemonState>,
    full: Option<bool>,
) -> Result<PairingQrResponse, String> {
    let config = crate::active_workshop::transport_config()?;
    let path = if full.unwrap_or(false) {
        "/qr?full=true"
    } else {
        "/qr"
    };
    crate::workshop_transport::workshop_get_json(&config, path).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrImagePayload {
    url: String,
    expires_at: String,
    short_code: String,
    png_base64: String,
}

async fn fetch_qr_image_once(
    config: &crate::pairing_client::WorkshopTransportConfig,
) -> Result<PairingQrImage, String> {
    let payload: QrImagePayload =
        crate::workshop_transport::workshop_get_json(config, "/qr/image").await?;
    Ok(PairingQrImage {
        data_url: format!("data:image/png;base64,{}", payload.png_base64),
        url: payload.url,
        expires_at: payload.expires_at,
        short_code: payload.short_code,
    })
}

#[tauri::command]
pub async fn pairing_fetch_qr_image(
    state: State<'_, DaemonState>,
) -> Result<PairingQrImage, String> {
    let _ = state;
    fetch_qr_image_once(&crate::active_workshop::transport_config()?).await
}

#[tauri::command]
pub async fn pairing_wait_ready(
    state: State<'_, DaemonState>,
    timeout_seconds: Option<u64>,
) -> Result<PairingQrImage, String> {
    let timeout = Duration::from_secs(timeout_seconds.unwrap_or(45).max(1));
    let poll = Duration::from_millis(750);
    let started = Instant::now();
    let _ = state;
    let config = crate::active_workshop::transport_config()?;
    let mut last_error = "Pairing is still starting…".to_string();

    while started.elapsed() < timeout {
        match fetch_qr_image_once(&config).await {
            Ok(image) => return Ok(image),
            Err(err) => last_error = err,
        }
        tokio::time::sleep(poll).await;
    }

    Err(last_error)
}

#[tauri::command]
pub async fn pairing_fetch_status(
    state: State<'_, DaemonState>,
) -> Result<PairingStatusResponse, String> {
    let _ = state;
    crate::workshop_transport::workshop_get_json(
        &crate::active_workshop::transport_config()?,
        "/pair/status",
    )
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RotateInviteBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

#[tauri::command]
pub async fn pairing_rotate_invite(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<PairingQrResponse, String> {
    let _ = state;
    let body = RotateInviteBody {
        profile_id: profile_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    };
    crate::workshop_transport::workshop_post_json(
        &crate::active_workshop::transport_config()?,
        "/qr/rotate",
        &body,
    )
    .await
}

#[tauri::command]
pub async fn pairing_revoke(
    state: State<'_, DaemonState>,
    pairing_id: String,
) -> Result<(), String> {
    let trimmed = pairing_id.trim();
    if trimmed.is_empty() {
        return Err("pairing_id is required".to_string());
    }
    let _ = state;
    crate::workshop_transport::workshop_json_request(
        &crate::active_workshop::transport_config()?,
        "DELETE",
        &format!("/pair/{trimmed}"),
        None,
    )
    .await
    .map(|_| ())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingTrustPolicyBody {
    trust_expires_at: Option<String>,
    idle_timeout_seconds: Option<u64>,
}

#[tauri::command]
pub async fn pairing_update_policy(
    _state: State<'_, DaemonState>,
    pairing_id: String,
    trust_expires_at: Option<String>,
    idle_timeout_seconds: Option<u64>,
) -> Result<PairedDeviceSummary, String> {
    let pairing_id = pairing_id.trim();
    if pairing_id.is_empty() {
        return Err("pairing_id is required".to_string());
    }
    let body = PairingTrustPolicyBody {
        trust_expires_at: trust_expires_at
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        idle_timeout_seconds,
    };
    crate::workshop_transport::workshop_put_json(
        &crate::active_workshop::transport_config()?,
        &format!("/pair/{pairing_id}/policy"),
        &body,
    )
    .await
}

#[tauri::command]
pub async fn pairing_fetch_execution_policies(
    _state: State<'_, DaemonState>,
) -> Result<serde_json::Value, String> {
    crate::workshop_transport::workshop_get_json(
        &crate::active_workshop::transport_config()?,
        "/v1/peers/execution-policies",
    )
    .await
}

#[tauri::command]
pub async fn pairing_update_execution_policy(
    _state: State<'_, DaemonState>,
    device_id: String,
    policy: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return Err("device_id is required".to_string());
    }
    crate::workshop_transport::workshop_put_json(
        &crate::active_workshop::transport_config()?,
        &format!(
            "/v1/peers/{}/execution-policy",
            urlencoding::encode(device_id)
        ),
        &policy,
    )
    .await
}

#[tauri::command]
pub async fn pairing_complete_from_qr(
    request: crate::pairing_client::PairCompleteFromQrRequest,
) -> Result<crate::pairing_client::PairCompleteFromQrResult, String> {
    crate::pairing_client::pair_complete_from_qr(request).await
}

#[tauri::command]
pub fn pairing_load_credentials() -> Option<crate::pairing_client::PairingCredentialsSummary> {
    crate::pairing_client::load_pairing_credentials_summary()
}

#[tauri::command]
pub async fn pairing_send_heartbeat(
    state: State<'_, DaemonState>,
    request: Option<PairHeartbeatInvokeRequest>,
) -> Result<(), String> {
    let base = daemon_base(&state)?;
    if let Some(token) = request
        .as_ref()
        .and_then(|body| body.apns_device_token.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        crate::push::set_apns_device_token(Some(token.to_string()));
    }
    crate::pairing_client::send_pair_heartbeat(&base, request.as_ref()).await
}

#[tauri::command]
pub async fn bonjour_status(state: State<'_, DaemonState>) -> Result<BonjourStatus, String> {
    let base = daemon_base(&state)?;
    let service_type = "_medousa._tcp.local.".to_string();

    match pairing_fetch_status(state).await {
        Ok(status) => {
            let likely_advertising = infer_likely_mdns_advertising(&base);
            let message = if likely_advertising {
                format!(
                    "Bonjour service {service_type} should be visible on your LAN as \"{}\".",
                    status.peer_name
                )
            } else {
                "Pairing works via QR on this network. Bonjour browse needs Core bound publicly — run `medousa start daemon --public` or set MEDOUSA_PAIRING_ADVERTISE=1.".to_string()
            };
            Ok(BonjourStatus {
                pairing_available: true,
                likely_advertising,
                service_type,
                device_id: Some(status.device_id),
                peer_name: Some(status.peer_name),
                message,
            })
        }
        Err(err) => Ok(BonjourStatus {
            pairing_available: false,
            likely_advertising: false,
            service_type,
            device_id: None,
            peer_name: None,
            message: err,
        }),
    }
}

fn infer_likely_mdns_advertising(daemon_url: &str) -> bool {
    if let Ok(parsed) = reqwest::Url::parse(daemon_url) {
        if let Some(host) = parsed.host_str() {
            if host != "127.0.0.1" && host != "localhost" && host != "::1" {
                return true;
            }
        }
    }
    false
}
