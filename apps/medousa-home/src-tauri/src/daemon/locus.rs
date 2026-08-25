use serde_json::Value;
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn locus_list_nodes(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    tags: Option<String>,
    tag_prefix: Option<String>,
) -> Result<Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let response = client
            .list_locus_nodes(medousa_types::LocusNodesQuery {
                session_id: session_id.clone(),
                limit,
                q: q.clone(),
                tags: tags.clone(),
                tag_prefix: tag_prefix.clone(),
            })
            .await
            .map_err(|error| error.to_string())?;
        return serde_json::to_value(response).map_err(|error| error.to_string());
    }
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
    if let Some(tags) = tags.filter(|value| !value.trim().is_empty()) {
        query.push(("tags", tags.trim().to_string()));
    }
    if let Some(tag_prefix) = tag_prefix.filter(|value| !value.trim().is_empty()) {
        query.push(("tag_prefix", tag_prefix.trim().to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/locus/nodes", &query).await
}

#[tauri::command]
pub async fn locus_list_tags(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: Option<String>,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let response = client
            .list_locus_tags(medousa_types::LocusTagsQuery {
                session_id: session_id.clone(),
                prefix: prefix.clone(),
                limit,
            })
            .await
            .map_err(|error| error.to_string())?;
        return serde_json::to_value(response).map_err(|error| error.to_string());
    }
    let mut query = Vec::new();
    if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
        query.push(("session_id", session_id.trim().to_string()));
    }
    if let Some(prefix) = prefix.filter(|value| !value.trim().is_empty()) {
        query.push(("prefix", prefix.trim().to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/locus/tags", &query).await
}

#[tauri::command]
pub async fn locus_get_node(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    sync_key: String,
) -> Result<Value, String> {
    let trimmed = sync_key.trim();
    if trimmed.is_empty() {
        return Err("sync_key is required".to_string());
    }
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let response = client
            .get_locus_node(trimmed)
            .await
            .map_err(|error| error.to_string())?;
        return serde_json::to_value(response).map_err(|error| error.to_string());
    }
    let encoded = urlencoding::encode(trimmed);
    workshop_http::get_json(&state, &format!("/v1/locus/nodes/{encoded}")).await
}
