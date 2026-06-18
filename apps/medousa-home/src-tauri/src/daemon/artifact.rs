use crate::daemon::types::{ArtifactCommandRequest, ArtifactCommandResponse};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn artifact_command(
    state: State<'_, DaemonState>,
    request: ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse, String> {
    workshop_http::post_json(&state, "/v1/runtime/artifact/command", &request).await
}
