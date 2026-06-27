//! Mobile stubs for desktop-only BrowserHost HTTP + in-process bridge.
//! Client registration uses daemon HTTP directly; local :7422 host is unavailable.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostStatusDto {
    pub running: bool,
    pub healthy: bool,
    pub base_url: String,
}

async fn register_browser_client_with_daemon(daemon_url: &str, channel_surface: &str) {
    let client_id = format!("home-{channel_surface}");
    let body = serde_json::json!({
        "client_id": client_id,
        "channel_surface": channel_surface,
        "supports_browser_host": true,
        "browser_host_url": null,
    });
    let url = format!("{}/v1/clients/register", daemon_url.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    else {
        return;
    };
    let _ = client.post(url).json(&body).send().await;
}

#[tauri::command]
pub async fn browser_host_register_client(
    daemon_url: String,
    channel_surface: String,
) -> Result<(), String> {
    register_browser_client_with_daemon(&daemon_url, &channel_surface).await;
    Ok(())
}

#[tauri::command]
pub async fn browser_host_status() -> Result<BrowserHostStatusDto, String> {
    Ok(BrowserHostStatusDto {
        running: false,
        healthy: false,
        base_url: String::new(),
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
    let url = format!(
        "{}/v1/browser/sessions/{}/resume",
        base,
        session_id.trim()
    );
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
