use crate::daemon::types::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactFetchRequest, ArtifactFetchResponse,
    ArtifactListUiRequest, ArtifactListUiResponse,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn artifact_command(
    state: State<'_, DaemonState>,
    request: ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse, String> {
    client(&state)
        .runtime()
        .artifact_command(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_fetch(
    state: State<'_, DaemonState>,
    request: ArtifactFetchRequest,
) -> Result<ArtifactFetchResponse, String> {
    client(&state)
        .runtime()
        .artifact_fetch(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_list_ui(
    state: State<'_, DaemonState>,
    request: ArtifactListUiRequest,
) -> Result<ArtifactListUiResponse, String> {
    client(&state)
        .runtime()
        .artifact_list_ui(&request)
        .await
        .map_err(sdk_error)
}
