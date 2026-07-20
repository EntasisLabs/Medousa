use crate::daemon::types::{
    CapabilityListResponse, CapabilityResolveResponse, CreateManuscriptRequest,
    ManuscriptCatalogQuery, ManuscriptCatalogResponse, ManuscriptDetailResponse,
    ManuscriptImportRequest, ManuscriptImportResponse, UpdateManuscriptRequest,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn catalog_list_manuscripts(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
    skills_only: Option<bool>,
) -> Result<ManuscriptCatalogResponse, String> {
    client(&state)
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
) -> Result<CapabilityListResponse, String> {
    client(&state)
        .capabilities()
        .list()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_get_capability(
    state: State<'_, DaemonState>,
    capability_id: String,
) -> Result<CapabilityResolveResponse, String> {
    client(&state)
        .capabilities()
        .get(capability_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_reindex_capabilities(
    state: State<'_, DaemonState>,
) -> Result<serde_json::Value, String> {
    client(&state)
        .capabilities()
        .reindex()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_get_manuscript(
    state: State<'_, DaemonState>,
    manuscript_id: String,
) -> Result<ManuscriptDetailResponse, String> {
    client(&state)
        .manuscripts()
        .get(manuscript_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_create_manuscript(
    state: State<'_, DaemonState>,
    request: CreateManuscriptRequest,
) -> Result<ManuscriptDetailResponse, String> {
    client(&state)
        .manuscripts()
        .create(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_update_manuscript(
    state: State<'_, DaemonState>,
    manuscript_id: String,
    request: UpdateManuscriptRequest,
) -> Result<ManuscriptDetailResponse, String> {
    client(&state)
        .manuscripts()
        .update(manuscript_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn catalog_import_manuscripts(
    state: State<'_, DaemonState>,
    request: ManuscriptImportRequest,
) -> Result<ManuscriptImportResponse, String> {
    client(&state)
        .manuscripts()
        .import(&request)
        .await
        .map_err(sdk_error)
}
