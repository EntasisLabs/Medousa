use crate::daemon::types::{WorkCardDetail, WorkspaceCardActionResponse};
use crate::daemon::DaemonState;
use reqwest::Client;
use tauri::State;

fn daemon_base(state: &State<'_, DaemonState>) -> Result<String, String> {
    Ok(state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone())
}

async fn map_http_error(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    format!("HTTP {status}: {body}")
}

#[tauri::command]
pub async fn workspace_get_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkCardDetail, String> {
    let base = daemon_base(&state)?;
    let encoded = urlencoding::encode(card_id.trim());
    let client = Client::new();
    let response = client
        .get(format!("{base}/v1/workspace/cards/{encoded}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn workspace_cancel_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkspaceCardActionResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = urlencoding::encode(card_id.trim());
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/workspace/cards/{encoded}/cancel"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn workspace_archive_card(
    state: State<'_, DaemonState>,
    card_id: String,
    purge_output: Option<bool>,
) -> Result<WorkspaceCardActionResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = urlencoding::encode(card_id.trim());
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/workspace/cards/{encoded}/archive"))
        .json(&serde_json::json!({ "purge_output": purge_output.unwrap_or(true) }))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn workspace_retry_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkspaceCardActionResponse, String> {
    let base = daemon_base(&state)?;
    let encoded = urlencoding::encode(card_id.trim());
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/workspace/cards/{encoded}/retry"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn workspace_fetch_snapshot(
    state: State<'_, DaemonState>,
    since_revision: Option<u64>,
) -> Result<crate::daemon::types::WorkspaceSnapshot, String> {
    let base = daemon_base(&state)?;
    let mut url = format!("{base}/v1/workspace/snapshot");
    if let Some(revision) = since_revision {
        url.push_str(&format!("?since_revision={revision}"));
    }
    let client = Client::new();
    let response = client.get(url).send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}
