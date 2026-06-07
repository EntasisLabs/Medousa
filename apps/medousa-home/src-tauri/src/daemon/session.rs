use crate::daemon::types::{SessionHistoryListResponse, SessionHistoryResponse};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn session_list(
    state: State<'_, DaemonState>,
    limit: Option<usize>,
) -> Result<SessionHistoryListResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let capped = limit.unwrap_or(50).clamp(1, 200);
    let url = format!("{base}/v1/sessions?limit={capped}");
    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("session list failed ({status}): {body}"));
    }
    response.json::<SessionHistoryListResponse>().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn session_get_history(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<SessionHistoryResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let url = format!("{base}/v1/sessions/{trimmed}/history");
    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("session history failed ({status}): {body}"));
    }
    response.json::<SessionHistoryResponse>().await.map_err(|err| err.to_string())
}
