use crate::daemon::DaemonState;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
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

fn pairing_client() -> Result<Client, String> {
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
        return "LAN pairing is not enabled on Medousa Core. Restart Core without MEDOUSA_PAIRING_DISABLE.".to_string();
    }
    if body.trim().is_empty() {
        format!("Medousa Core returned HTTP {}", status)
    } else {
        format!("Medousa Core returned HTTP {}: {}", status, body.trim())
    }
}

#[tauri::command]
pub async fn pairing_fetch_qr(state: State<'_, DaemonState>) -> Result<PairingQrResponse, String> {
    let base = daemon_base(&state)?;
    let client = pairing_client()?;
    let response = client
        .get(format!("{base}/qr"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Core at {base}: {err}"))?;
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

#[tauri::command]
pub async fn pairing_fetch_qr_image(
    state: State<'_, DaemonState>,
) -> Result<PairingQrImage, String> {
    let base = daemon_base(&state)?;
    let client = pairing_client()?;
    let qr_response = client
        .get(format!("{base}/qr"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Core at {base}: {err}"))?;
    if !qr_response.status().is_success() {
        let status = qr_response.status();
        let body = qr_response.text().await.unwrap_or_default();
        return Err(pairing_unavailable_message(status, &body));
    }
    let qr = qr_response
        .json::<PairingQrResponse>()
        .await
        .map_err(|err| err.to_string())?;
    let png = client
        .get(format!("{base}/qr.png"))
        .send()
        .await
        .map_err(|err| format!("cannot load QR image from {base}: {err}"))?;
    if !png.status().is_success() {
        let status = png.status();
        let body = png.text().await.unwrap_or_default();
        return Err(pairing_unavailable_message(status, &body));
    }
    let bytes = png.bytes().await.map_err(|err| err.to_string())?;
    let encoded = STANDARD.encode(bytes);
    Ok(PairingQrImage {
        data_url: format!("data:image/png;base64,{encoded}"),
        url: qr.url,
        expires_at: qr.expires_at,
        short_code: qr.short_code,
    })
}

#[tauri::command]
pub async fn pairing_fetch_status(
    state: State<'_, DaemonState>,
) -> Result<PairingStatusResponse, String> {
    let base = daemon_base(&state)?;
    let client = pairing_client()?;
    let response = client
        .get(format!("{base}/pair/status"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Core at {base}: {err}"))?;
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
    let client = pairing_client()?;
    let response = client
        .delete(format!("{base}/pair/{trimmed}"))
        .send()
        .await
        .map_err(|err| format!("cannot reach Medousa Core at {base}: {err}"))?;
    if response.status().is_success() || response.status().as_u16() == 204 {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(pairing_unavailable_message(status, &body))
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
