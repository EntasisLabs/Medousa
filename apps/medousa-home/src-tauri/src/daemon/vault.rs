use crate::daemon::types::{
    VaultBacklinksResponse, VaultChangesResponse, VaultFileContentResponse,
    VaultNoteContentResponse, VaultNotesListResponse, VaultRootsResponse, VaultSearchResponse,
    VaultTagsListResponse, VaultWriteResponse,
};
use medousa_types::{
    VaultAddRootRequest, VaultBacklinksQuery, VaultChangesQuery, VaultNotesQuery, VaultPutQuery,
    VaultSearchQuery, VaultSetActiveRootRequest, VaultTagsQuery, VaultWriteRequest,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

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
    _embedded_state: State<'_, EmbeddedDaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
    tags: Option<String>,
    tag_prefix: Option<String>,
    cursor: Option<String>,
    generation: Option<u64>,
) -> Result<VaultNotesListResponse, String> {
    let query = VaultNotesQuery {
        prefix: prefix.filter(|value| !value.trim().is_empty()),
        limit,
        tags: tags.filter(|value| !value.trim().is_empty()),
        tag_prefix: tag_prefix.filter(|value| !value.trim().is_empty()),
        cursor: cursor.filter(|value| !value.trim().is_empty()),
        generation,
    };
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_vault_notes(query)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .list_notes(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_changes(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    since_generation: Option<u64>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<VaultChangesResponse, String> {
    let query = VaultChangesQuery {
        since_generation,
        cursor: cursor.filter(|value| !value.trim().is_empty()),
        limit,
    };
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_vault_changes(query)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .list_changes(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_tags(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<VaultTagsListResponse, String> {
    let query = VaultTagsQuery {
        prefix: prefix.filter(|value| !value.trim().is_empty()),
        limit,
    };
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_vault_tags(query)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .list_tags(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_get_note(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: String,
) -> Result<VaultNoteContentResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .get_vault_note(path.trim().to_string())
            .await
            .map_err(|error| error.to_string());
    }
    let encoded = encode_note_path(path.trim());
    client(&state)?
        .vault()
        .get_note(&encoded)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_get_file(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: String,
) -> Result<VaultFileContentResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .get_vault_file(path.trim().to_string())
            .await
            .map_err(|error| error.to_string());
    }
    let encoded = encode_note_path(path.trim());
    client(&state)?
        .vault()
        .get_file(&encoded)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_save_note(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: String,
    content: String,
    content_hash: Option<String>,
    session_id: Option<String>,
    auto_workshop_tags: Option<bool>,
) -> Result<VaultWriteResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let request = VaultWriteRequest {
            path: None,
            content,
            session_id: session_id.filter(|value| !value.trim().is_empty()),
            semantic_tags: None,
            auto_workshop_tags: auto_workshop_tags.unwrap_or(true),
        };
        return client
            .save_vault_note(
                path.trim().to_string(),
                request,
                content_hash.filter(|value| !value.trim().is_empty()),
            )
            .await
            .map_err(|error| error.to_string());
    }
    let encoded = encode_note_path(path.trim());
    let query = VaultPutQuery {
        session_id: session_id.filter(|value| !value.trim().is_empty()),
        auto_workshop_tags,
    };
    client(&state)?
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
    _embedded_state: State<'_, EmbeddedDaemonState>,
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
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .create_vault_note(request)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .create_note(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_delete_note(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: String,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .delete_vault_note(path.trim().to_string())
            .await
            .map(|response| serde_json::to_value(response).unwrap_or_default())
            .map_err(|error| error.to_string());
    }
    let encoded = encode_note_path(path.trim());
    client(&state)?
        .vault()
        .delete_note(&encoded)
        .await
        .map(|response| serde_json::to_value(response).unwrap_or_default())
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_search(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
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
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .search_vault(search)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .search(&search)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_backlinks(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: String,
) -> Result<VaultBacklinksResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .vault_backlinks(path.trim().to_string())
            .await
            .map_err(|error| error.to_string());
    }
    let query = VaultBacklinksQuery {
        path: Some(path.trim().to_string()),
    };
    client(&state)?
        .vault()
        .backlinks(&query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_roots(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<VaultRootsResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client.list_vault_roots().map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .list_roots()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_set_active_root(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    root_id: String,
) -> Result<VaultRootsResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .set_active_vault_root(root_id.trim())
            .map_err(|error| error.to_string());
    }
    let request = VaultSetActiveRootRequest {
        root_id: root_id.trim().to_string(),
    };
    client(&state)?
        .vault()
        .set_active_root(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_add_root(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    label: String,
    path: String,
    id: Option<String>,
) -> Result<VaultRootsResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .add_vault_root(label.trim(), path.trim(), id.as_deref())
            .map_err(|error| error.to_string());
    }
    let request = VaultAddRootRequest {
        label: label.trim().to_string(),
        path: path.trim().to_string(),
        id: id.filter(|value| !value.trim().is_empty()),
    };
    client(&state)?
        .vault()
        .add_root(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_list_trash(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_vault_trash(limit.unwrap_or(100))
            .await
            .map(|response| serde_json::to_value(response).unwrap_or_default())
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .list_trash(limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_restore_trash(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    path: String,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .restore_vault_trash(path.trim().to_string())
            .await
            .map(|response| serde_json::to_value(response).unwrap_or_default())
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .vault()
        .restore_trash(path.trim())
        .await
        .map_err(sdk_error)
}
