use crate::daemon::types::{
    VaultBacklinksResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultSearchResponse,
    VaultTagsListResponse, VaultWriteResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

fn encode_note_path(path: &str) -> String {
    path.split('/')
        .map(urlencoding::encode)
        .map(|segment| segment.into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[tauri::command]
pub async fn vault_list_notes(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
    tags: Option<String>,
    tag_prefix: Option<String>,
) -> Result<VaultNotesListResponse, String> {
    let mut query = Vec::new();
    if let Some(prefix) = prefix.filter(|value| !value.trim().is_empty()) {
        query.push(("prefix", prefix.trim().to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }
    if let Some(tags) = tags.filter(|value| !value.trim().is_empty()) {
        query.push(("tags", tags.trim().to_string()));
    }
    if let Some(tag_prefix) = tag_prefix.filter(|value| !value.trim().is_empty()) {
        query.push(("tag_prefix", tag_prefix.trim().to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/vault/notes", &query).await
}

#[tauri::command]
pub async fn vault_list_tags(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<VaultTagsListResponse, String> {
    let mut query = Vec::new();
    if let Some(prefix) = prefix.filter(|value| !value.trim().is_empty()) {
        query.push(("prefix", prefix.trim().to_string()));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/vault/tags", &query).await
}

#[tauri::command]
pub async fn vault_get_note(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultNoteContentResponse, String> {
    let encoded = encode_note_path(path.trim());
    workshop_http::get_json(&state, &format!("/v1/vault/notes/{encoded}")).await
}

#[tauri::command]
pub async fn vault_save_note(
    state: State<'_, DaemonState>,
    path: String,
    content: String,
    content_hash: Option<String>,
    session_id: Option<String>,
    auto_workshop_tags: Option<bool>,
) -> Result<VaultWriteResponse, String> {
    let encoded = encode_note_path(path.trim());
    let mut query = Vec::new();
    if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
        query.push(("session_id", session_id.trim().to_string()));
    }
    if let Some(auto_workshop_tags) = auto_workshop_tags {
        query.push(("auto_workshop_tags", auto_workshop_tags.to_string()));
    }
    let path = workshop_http::path_with_query(&format!("/v1/vault/notes/{encoded}"), &query);
    let mut extra_headers: Vec<(String, String)> = Vec::new();
    if let Some(hash) = content_hash.filter(|value| !value.trim().is_empty()) {
        extra_headers.push(("if-match".to_string(), hash));
    }
    workshop_http::put_raw(
        &state,
        &path,
        "text/markdown; charset=utf-8",
        content.as_bytes(),
        &extra_headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>(),
    )
    .await
}

#[tauri::command]
pub async fn vault_create_note(
    state: State<'_, DaemonState>,
    path: String,
    content: String,
    session_id: Option<String>,
    semantic_tags: Option<Vec<String>>,
    auto_workshop_tags: Option<bool>,
) -> Result<VaultWriteResponse, String> {
    let body = serde_json::json!({
        "path": path.trim(),
        "content": content,
        "session_id": session_id.filter(|value| !value.trim().is_empty()),
        "semantic_tags": semantic_tags.filter(|tags| !tags.is_empty()),
        "auto_workshop_tags": auto_workshop_tags.unwrap_or(true),
    });
    workshop_http::post_json(&state, "/v1/vault/notes", &body).await
}

#[tauri::command]
pub async fn vault_delete_note(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<serde_json::Value, String> {
    let encoded = encode_note_path(path.trim());
    workshop_http::delete_json(&state, &format!("/v1/vault/notes/{encoded}")).await
}

#[tauri::command]
pub async fn vault_search(
    state: State<'_, DaemonState>,
    query: String,
    limit: Option<usize>,
    tags: Option<String>,
) -> Result<VaultSearchResponse, String> {
    let limit = limit.unwrap_or(20);
    let mut params = vec![("limit", limit.to_string())];
    let trimmed = query.trim();
    if !trimmed.is_empty() {
        params.insert(0, ("q", trimmed.to_string()));
    }
    if let Some(tags) = tags.filter(|value| !value.trim().is_empty()) {
        params.push(("tags", tags.trim().to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/vault/search", &params).await
}

#[tauri::command]
pub async fn vault_backlinks(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultBacklinksResponse, String> {
    workshop_http::get_json_query(
        &state,
        "/v1/vault/backlinks",
        &[("path", path.trim().to_string())],
    )
    .await
}
