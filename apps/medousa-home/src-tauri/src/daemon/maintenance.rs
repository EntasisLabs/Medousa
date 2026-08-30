use medousa_types::{
    ArtifactRetentionStatusResponse, StorageGovernorSettingsResponse,
    StorageMaintenanceReportResponse, StorageMaintenanceRequest, StorageUsageReportResponse,
    UpdateArtifactRetentionRequest, UpdateArtifactRetentionResponse,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::workshop_http;

#[tauri::command]
pub async fn artifact_retention_status(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<ArtifactRetentionStatusResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_retention_status()
            .await
            .map_err(|error| error.to_string());
    }
    workshop_http::get_json(&state, "/v1/maintenance/artifacts").await
}

#[tauri::command]
pub async fn storage_status(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<StorageUsageReportResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if _embedded_state.client_if_active().await?.is_some() {
        return Err(
            "Forge cache storage controls belong to a Shared workshop host; Personal has no Forge caches to maintain."
                .to_string(),
        );
    }
    workshop_http::get_json(&state, "/v1/maintenance/storage").await
}

#[tauri::command]
pub async fn storage_settings_update(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: StorageGovernorSettingsResponse,
) -> Result<StorageUsageReportResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if _embedded_state.client_if_active().await?.is_some() {
        return Err(
            "Forge cache storage controls belong to a Shared workshop host; Personal has no Forge caches to maintain."
                .to_string(),
        );
    }
    workshop_http::put_json(&state, "/v1/maintenance/storage", &request).await
}

#[tauri::command]
pub async fn storage_maintenance_run(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: StorageMaintenanceRequest,
) -> Result<StorageMaintenanceReportResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if _embedded_state.client_if_active().await?.is_some() {
        return Err(
            "Forge cache storage controls belong to a Shared workshop host; Personal has no Forge caches to maintain."
                .to_string(),
        );
    }
    workshop_http::post_json(&state, "/v1/maintenance/storage", &request).await
}

#[tauri::command]
pub async fn artifact_retention_update(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: UpdateArtifactRetentionRequest,
) -> Result<UpdateArtifactRetentionResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .update_artifact_retention(request)
            .await
            .map_err(|error| error.to_string());
    }
    workshop_http::put_json(&state, "/v1/maintenance/artifacts", &request).await
}
