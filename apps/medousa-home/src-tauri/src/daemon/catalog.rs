use crate::daemon::types::{
    CapabilityListResponse, CapabilityResolveResponse, ManuscriptCatalogResponse,
    ManuscriptDetailResponse, ManuscriptImportRequest, ManuscriptImportResponse,
    UpdateManuscriptRequest,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn catalog_list_manuscripts(
    state: State<'_, DaemonState>,
    prefix: Option<String>,
    limit: Option<usize>,
    skills_only: Option<bool>,
) -> Result<ManuscriptCatalogResponse, String> {
    let mut query = Vec::new();
    if let Some(value) = prefix.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        query.push(("prefix", value.to_string()));
    }
    if let Some(value) = limit {
        query.push(("limit", value.to_string()));
    }
    if let Some(value) = skills_only {
        query.push(("skills_only", value.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/manuscripts", &query).await
}

#[tauri::command]
pub async fn catalog_list_capabilities(
    state: State<'_, DaemonState>,
) -> Result<CapabilityListResponse, String> {
    super::sdk::client(&state)
        .capabilities()
        .list()
        .await
        .map_err(super::sdk::sdk_error)
}

#[tauri::command]
pub async fn catalog_get_capability(
    state: State<'_, DaemonState>,
    capability_id: String,
) -> Result<CapabilityResolveResponse, String> {
    super::sdk::client(&state)
        .capabilities()
        .get(capability_id.trim())
        .await
        .map_err(super::sdk::sdk_error)
}

#[tauri::command]
pub async fn catalog_reindex_capabilities(
    state: State<'_, DaemonState>,
) -> Result<serde_json::Value, String> {
    super::sdk::client(&state)
        .capabilities()
        .reindex()
        .await
        .map_err(super::sdk::sdk_error)
}

#[tauri::command]
pub async fn catalog_get_manuscript(
    state: State<'_, DaemonState>,
    manuscript_id: String,
) -> Result<ManuscriptDetailResponse, String> {
    let id = urlencoding::encode(manuscript_id.trim());
    workshop_http::get_json(&state, &format!("/v1/manuscripts/{id}")).await
}

#[tauri::command]
pub async fn catalog_update_manuscript(
    state: State<'_, DaemonState>,
    manuscript_id: String,
    request: UpdateManuscriptRequest,
) -> Result<ManuscriptDetailResponse, String> {
    let id = urlencoding::encode(manuscript_id.trim());
    workshop_http::patch_json(&state, &format!("/v1/manuscripts/{id}"), &request).await
}

#[tauri::command]
pub async fn catalog_import_manuscripts(
    state: State<'_, DaemonState>,
    request: ManuscriptImportRequest,
) -> Result<ManuscriptImportResponse, String> {
    workshop_http::post_json(&state, "/v1/manuscripts", &request).await
}
