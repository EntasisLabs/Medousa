use reqwest::Client;
use serde_json::Value;
use tauri::State;

use super::DaemonState;

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
pub async fn locus_list_nodes(
    state: State<'_, DaemonState>,
    session_id: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
) -> Result<Value, String> {
    let base = daemon_base(&state)?;
    let client = Client::new();
    let mut url = format!("{base}/v1/locus/nodes");
    let mut params = Vec::new();
    if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
        params.push(format!("session_id={}", urlencoding::encode(session_id.trim())));
    }
    if let Some(limit) = limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(q) = q.filter(|value| !value.trim().is_empty()) {
        params.push(format!("q={}", urlencoding::encode(q.trim())));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn locus_get_node(
    state: State<'_, DaemonState>,
    sync_key: String,
) -> Result<Value, String> {
    let base = daemon_base(&state)?;
    let trimmed = sync_key.trim();
    if trimmed.is_empty() {
        return Err("sync_key is required".to_string());
    }
    let encoded = urlencoding::encode(trimmed);
    let url = format!("{base}/v1/locus/nodes/{encoded}");
    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(map_http_error(response).await);
    }
    response.json().await.map_err(|err| err.to_string())
}
