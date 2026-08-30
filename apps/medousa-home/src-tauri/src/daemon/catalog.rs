use crate::daemon::types::{
    CapabilityListResponse, CapabilityResolveResponse, CreateManuscriptRequest,
    ManuscriptCatalogQuery, ManuscriptCatalogResponse, ManuscriptDetailResponse,
    ManuscriptImportRequest, ManuscriptImportResponse, UpdateManuscriptRequest,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::sdk::{client, sdk_error};

#[tauri::command]
pub async fn catalog_list_manuscripts(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
    skills_only: Option<bool>,
) -> Result<ManuscriptCatalogResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_manuscripts(ManuscriptCatalogQuery {
                prefix,
                limit,
                skills_only,
            })
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .manuscripts()
        .list(&ManuscriptCatalogQuery {
            prefix,
            limit,
            skills_only,
        })
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_list_capabilities(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<CapabilityListResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let value = client
            .list_capabilities()
            .await
            .map_err(|error| error.to_string());
        return serde_json::from_value(value?).map_err(|error| error.to_string());
    }
    client(&state)?
        .capabilities()
        .list()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_get_capability(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    capability_id: String,
) -> Result<CapabilityResolveResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let value = client
            .get_capability(&capability_id)
            .await
            .map_err(|error| error.to_string());
        return serde_json::from_value(value?).map_err(|error| error.to_string());
    }
    client(&state)?
        .capabilities()
        .get(capability_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_reindex_capabilities(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<serde_json::Value, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .reindex_capabilities()
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .capabilities()
        .reindex()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_get_manuscript(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    manuscript_id: String,
) -> Result<ManuscriptDetailResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .get_manuscript(manuscript_id)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .manuscripts()
        .get(manuscript_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_create_manuscript(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: CreateManuscriptRequest,
) -> Result<ManuscriptDetailResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .create_manuscript(request)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .manuscripts()
        .create(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_update_manuscript(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    manuscript_id: String,
    request: UpdateManuscriptRequest,
) -> Result<ManuscriptDetailResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .update_manuscript(manuscript_id, request)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .manuscripts()
        .update(manuscript_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_import_manuscripts(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ManuscriptImportRequest,
) -> Result<ManuscriptImportResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .import_manuscripts(request)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .manuscripts()
        .import(&request)
        .await
        .map_err(sdk_error)
}
