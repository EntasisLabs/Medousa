use crate::daemon::types::{CapabilityListResponse, ManuscriptCatalogResponse};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn catalog_list_manuscripts(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
    skills_only: Option<bool>,
) -> Result<ManuscriptCatalogResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let mut url = format!("{base}/v1/manuscripts");
    let mut params = Vec::new();
    if let Some(value) = prefix.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(format!("prefix={}", urlencoding::encode(value)));
    }
    if let Some(value) = limit {
        params.push(format!("limit={value}"));
    }
    if let Some(value) = skills_only {
        params.push(format!("skills_only={value}"));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("manuscript catalog failed ({status}): {body}"));
    }
    response
        .json::<ManuscriptCatalogResponse>()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn catalog_list_capabilities(
    state: State<'_, DaemonState>,
) -> Result<CapabilityListResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let url = format!("{base}/v1/capabilities");
    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("capability catalog failed ({status}): {body}"));
    }
    response
        .json::<CapabilityListResponse>()
        .await
        .map_err(|err| err.to_string())
}
