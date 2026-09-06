//! Mobile stubs for desktop-only BrowserHost HTTP + in-process bridge.
//! Client registration uses daemon HTTP directly; local :7422 host is unavailable.

use medousa_browser_lite::{SearchResponse, search_ddg_html_cached};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostStatusDto {
    pub running: bool,
    pub healthy: bool,
    pub base_url: String,
    pub driver_id: String,
}

async fn register_browser_client_with_workshop(
    state: &State<'_, crate::daemon::DaemonState>,
    channel_surface: &str,
) -> Result<(), String> {
    let client_id = crate::browser_driver::client_id(channel_surface);
    let body = serde_json::json!({
        "client_id": client_id,
        "channel_surface": channel_surface,
        "supports_browser_host": true,
        "browser_host_url": null,
        "world_drivers": [crate::browser_driver::registration()],
    });
    let _: serde_json::Value = crate::daemon::workshop_http::post_json(
        state,
        medousa_sdk::generated::ops::CLIENTS_REGISTER_POST.path,
        &body,
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn browser_host_search(
    query: String,
    max_results: Option<usize>,
) -> Result<SearchResponse, String> {
    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Err("empty query".to_string());
    }
    let limit = max_results.unwrap_or(5).clamp(1, 10);
    tokio::task::spawn_blocking(move || search_ddg_html_cached(&trimmed, limit))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn browser_host_register_client(
    state: State<'_, crate::daemon::DaemonState>,
    embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    daemon_url: String,
    channel_surface: String,
) -> Result<(), String> {
    let _ = daemon_url;
    if embedded_state.client_if_active().await?.is_some() {
        // Personal is in-process. Its exact Home driver is admitted with every
        // turn instead of being registered through a nonexistent HTTP socket.
        return Ok(());
    }
    register_browser_client_with_workshop(&state, &channel_surface).await
}

#[tauri::command]
pub async fn browser_host_status() -> Result<BrowserHostStatusDto, String> {
    Ok(BrowserHostStatusDto {
        running: false,
        healthy: false,
        base_url: String::new(),
        driver_id: crate::browser_driver::id().to_string(),
    })
}

#[tauri::command]
pub async fn browser_host_restart() -> Result<BrowserHostStatusDto, String> {
    browser_host_status().await
}

#[tauri::command]
pub async fn browser_host_resume_session(
    session_id: String,
    daemon_url: Option<String>,
) -> Result<serde_json::Value, String> {
    let base = daemon_url
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "daemon URL required on mobile".to_string())?;
    let url = format!("{}/v1/browser/sessions/{}/resume", base, session_id.trim());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("daemon resume failed: {}", response.status()));
    }
    response.json().await.map_err(|err| err.to_string())
}
