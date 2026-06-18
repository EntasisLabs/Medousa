use crate::daemon::DaemonState;
use reqwest::Client;
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

fn pairing_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
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

fn pairing_unavailable_message(status: reqwest::StatusCode, body: &str) -> String {
    if status.as_u16() == 404 {
        return "LAN pairing is not enabled on Medousa Engine. Restart the engine without MEDOUSA_PAIRING_DISABLE.".to_string();
    }
    if body.trim().is_empty() {
        format!("Medousa Engine returned HTTP {}", status)
    } else {
        format!("Medousa Engine returned HTTP {}: {}", status, body.trim())
    }
}

#[tauri::command]
pub async fn pairing_fetch_qr(state: State<'_, DaemonState>) -> Result<PairingQrResponse, String> {
    let base = daemon_base(&state)?;
    let client = pairing_http_client()?;
    let response = client
        .get(format!("{base}/qr"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(pairing_unavailable_message(status, &body));
    }
    response
        .json::<PairingQrResponse>()
        .await
        .map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrImagePayload {
    url: String,
    expires_at: String,
    short_code: String,
    png_base64: String,
}

async fn fetch_qr_image_once(base: &str, client: &Client) -> Result<PairingQrImage, String> {
    let response = client
        .get(format!("{base}/qr/image"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(pairing_unavailable_message(status, &body));
    }
    let payload = response
        .json::<QrImagePayload>()
        .await
        .map_err(|err| err.to_string())?;
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
    let base = daemon_base(&state)?;
    let client = pairing_http_client()?;
    fetch_qr_image_once(&base, &client).await
}

#[tauri::command]
pub async fn pairing_wait_ready(
    state: State<'_, DaemonState>,
    timeout_seconds: Option<u64>,
) -> Result<PairingQrImage, String> {
    let timeout = Duration::from_secs(timeout_seconds.unwrap_or(45).max(1));
    let poll = Duration::from_millis(750);
    let started = Instant::now();
    let base = daemon_base(&state)?;
    let client = pairing_http_client()?;
    let mut last_error = "Pairing is still starting…".to_string();

    while started.elapsed() < timeout {
        match fetch_qr_image_once(&base, &client).await {
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
    let base = daemon_base(&state)?;
    let client = pairing_http_client()?;
    let response = client
        .get(format!("{base}/pair/status"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(pairing_unavailable_message(status, &body));
    }
    response
        .json::<PairingStatusResponse>()
        .await
        .map_err(|err| err.to_string())
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
    let base = daemon_base(&state)?;
    let client = pairing_http_client()?;
    let response = client
        .delete(format!("{base}/pair/{trimmed}"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Engine at {base}: {err}"))?;
    if response.status().is_success() || response.status().as_u16() == 204 {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(pairing_unavailable_message(status, &body))
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
