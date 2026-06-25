use crate::daemon::types::{ArtifactCommandRequest, ArtifactCommandResponse, ArtifactFetchRequest, ArtifactFetchResponse};
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

#[tauri::command]
pub async fn artifact_fetch(
    state: State<'_, DaemonState>,
    request: ArtifactFetchRequest,
) -> Result<ArtifactFetchResponse, String> {
    workshop_http::post_json(&state, "/v1/runtime/artifact/fetch", &request).await
}
