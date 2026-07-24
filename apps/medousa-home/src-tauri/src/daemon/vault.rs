use crate::daemon::types::{
    VaultBacklinksResponse, VaultFileContentResponse, VaultNoteContentResponse,
    VaultNotesListResponse, VaultRootsResponse, VaultSearchResponse, VaultTagsListResponse,
    VaultWriteResponse,
};
use medousa_types::{
    VaultAddRootRequest, VaultBacklinksQuery, VaultNotesQuery, VaultPutQuery, VaultSearchQuery,
    VaultSetActiveRootRequest, VaultTagsQuery, VaultWriteRequest,
};
use tauri::State;

use super::sdk::{client, sdk_error};
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
    let query = VaultNotesQuery {
        prefix: prefix.filter(|value| !value.trim().is_empty()),
        limit,
        tags: tags.filter(|value| !value.trim().is_empty()),
        tag_prefix: tag_prefix.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
        .vault()
        .list_notes(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_tags(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<VaultTagsListResponse, String> {
    let query = VaultTagsQuery {
        prefix: prefix.filter(|value| !value.trim().is_empty()),
        limit,
    };
    client(&state)
        .vault()
        .list_tags(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_get_note(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultNoteContentResponse, String> {
    let encoded = encode_note_path(path.trim());
    client(&state)
        .vault()
        .get_note(&encoded)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_get_file(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultFileContentResponse, String> {
    let encoded = encode_note_path(path.trim());
    client(&state)
        .vault()
        .get_file(&encoded)
        .await
        .map_err(sdk_error)
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
    let query = VaultPutQuery {
        session_id: session_id.filter(|value| !value.trim().is_empty()),
        auto_workshop_tags,
    };
    client(&state)
        .vault()
        .update_note(
            &encoded,
            &content,
            &query,
            content_hash
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
        )
        .await
        .map_err(sdk_error)
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
    let request = VaultWriteRequest {
        path: Some(path.trim().to_string()),
        content,
        session_id: session_id.filter(|value| !value.trim().is_empty()),
        semantic_tags: semantic_tags.filter(|tags| !tags.is_empty()),
        auto_workshop_tags: auto_workshop_tags.unwrap_or(true),
    };
    client(&state)
        .vault()
        .create_note(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_delete_note(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<serde_json::Value, String> {
    let encoded = encode_note_path(path.trim());
    client(&state)
        .vault()
        .delete_note(&encoded)
        .await
        .map(|response| serde_json::to_value(response).unwrap_or_default())
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_search(
    state: State<'_, DaemonState>,
    query: String,
    limit: Option<usize>,
    tags: Option<String>,
) -> Result<VaultSearchResponse, String> {
    let trimmed = query.trim();
    let search = VaultSearchQuery {
        q: if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        },
        limit: limit.or(Some(20)),
        tags: tags.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
        .vault()
        .search(&search)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_backlinks(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<VaultBacklinksResponse, String> {
    let query = VaultBacklinksQuery {
        path: Some(path.trim().to_string()),
    };
    client(&state)
        .vault()
        .backlinks(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_roots(state: State<'_, DaemonState>) -> Result<VaultRootsResponse, String> {
    client(&state).vault().list_roots().await.map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_set_active_root(
    state: State<'_, DaemonState>,
    root_id: String,
) -> Result<VaultRootsResponse, String> {
    let request = VaultSetActiveRootRequest {
        root_id: root_id.trim().to_string(),
    };
    client(&state)
        .vault()
        .set_active_root(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_add_root(
    state: State<'_, DaemonState>,
    label: String,
    path: String,
    id: Option<String>,
) -> Result<VaultRootsResponse, String> {
    let request = VaultAddRootRequest {
        label: label.trim().to_string(),
        path: path.trim().to_string(),
        id: id.filter(|value| !value.trim().is_empty()),
    };
    client(&state)
        .vault()
        .add_root(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_trash(
    state: State<'_, DaemonState>,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    client(&state)
        .vault()
        .list_trash(limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_restore_trash(
    state: State<'_, DaemonState>,
    path: String,
) -> Result<serde_json::Value, String> {
    client(&state)
        .vault()
        .restore_trash(path.trim())
        .await
        .map_err(sdk_error)
}
