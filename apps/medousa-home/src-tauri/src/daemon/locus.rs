use serde_json::Value;
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn locus_list_nodes(
    state: State<'_, DaemonState>,
    session_id: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
) -> Result<Value, String> {
    let mut query = Vec::new();
    if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
        query.push(("session_id", session_id.trim().to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }
    if let Some(q) = q.filter(|value| !value.trim().is_empty()) {
        query.push(("q", q.trim().to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/locus/nodes", &query).await
}

#[tauri::command]
pub async fn locus_get_node(
    state: State<'_, DaemonState>,
    sync_key: String,
) -> Result<Value, String> {
    let trimmed = sync_key.trim();
    if trimmed.is_empty() {
        return Err("sync_key is required".to_string());
    }
    let encoded = urlencoding::encode(trimmed);
    workshop_http::get_json(&state, &format!("/v1/locus/nodes/{encoded}")).await
}
