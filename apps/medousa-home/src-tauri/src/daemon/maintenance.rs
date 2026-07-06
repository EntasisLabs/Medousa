use medousa_types::{
    ArtifactRetentionStatusResponse, UpdateArtifactRetentionRequest, UpdateArtifactRetentionResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn artifact_retention_status(
    state: State<'_, DaemonState>,
) -> Result<ArtifactRetentionStatusResponse, String> {
    workshop_http::get_json(&state, "/v1/maintenance/artifacts").await
}

#[tauri::command]
pub async fn artifact_retention_update(
    state: State<'_, DaemonState>,
    request: UpdateArtifactRetentionRequest,
) -> Result<UpdateArtifactRetentionResponse, String> {
    workshop_http::put_json(&state, "/v1/maintenance/artifacts", &request).await
}
