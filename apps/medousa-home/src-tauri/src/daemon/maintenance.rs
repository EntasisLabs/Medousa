use medousa_types::{
    ArtifactRetentionStatusResponse, StorageGovernorSettingsResponse,
    StorageMaintenanceReportResponse, StorageMaintenanceRequest, StorageUsageReportResponse,
    UpdateArtifactRetentionRequest, UpdateArtifactRetentionResponse,
};
use tauri::State;

use super::DaemonState;
use super::workshop_http;

#[tauri::command]
pub async fn artifact_retention_status(
    state: State<'_, DaemonState>,
) -> Result<ArtifactRetentionStatusResponse, String> {
    workshop_http::get_json(&state, "/v1/maintenance/artifacts").await
}

#[tauri::command]
pub async fn storage_status(
    state: State<'_, DaemonState>,
) -> Result<StorageUsageReportResponse, String> {
    workshop_http::get_json(&state, "/v1/maintenance/storage").await
}

#[tauri::command]
pub async fn storage_settings_update(
    state: State<'_, DaemonState>,
    request: StorageGovernorSettingsResponse,
) -> Result<StorageUsageReportResponse, String> {
    workshop_http::put_json(&state, "/v1/maintenance/storage", &request).await
}

#[tauri::command]
pub async fn storage_maintenance_run(
    state: State<'_, DaemonState>,
    request: StorageMaintenanceRequest,
) -> Result<StorageMaintenanceReportResponse, String> {
    workshop_http::post_json(&state, "/v1/maintenance/storage", &request).await
}

#[tauri::command]
pub async fn artifact_retention_update(
    state: State<'_, DaemonState>,
    request: UpdateArtifactRetentionRequest,
) -> Result<UpdateArtifactRetentionResponse, String> {
    workshop_http::put_json(&state, "/v1/maintenance/artifacts", &request).await
}
