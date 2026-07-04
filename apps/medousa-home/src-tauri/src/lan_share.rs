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
    if workshop.kind != "paired" {
        return Err("Share push requires a trusted paired workshop".to_string());
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

#[tauri::command]
pub async fn trust_workshop_from_qr(
    request: TrustWorkshopFromQrRequest,
) -> Result<crate::pairing_client::PairCompleteFromQrResult, String> {
    let workshop_name = request
        .workshop_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Trusted workshop")
        .to_string();
    crate::pairing_client::pair_complete_from_qr(crate::pairing_client::PairCompleteFromQrRequest {
        qr_url: request.qr_url,
        daemon_url: request.daemon_url,
        phone_name: Some(workshop_name),
    })
    .await
}

#[tauri::command]
pub fn list_trusted_workshops() -> Result<Vec<TrustedWorkshopSummary>, String> {
    let registry = crate::workshop_registry::load_registry()?;
    let mut out = Vec::new();
    for workshop in registry.workshops {
        if workshop.kind != "paired" {
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
    if removed.kind != "paired" {
        return Err("Only trusted workshops can be revoked".to_string());
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
