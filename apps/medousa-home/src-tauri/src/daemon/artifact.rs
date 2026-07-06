use crate::daemon::types::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactDeleteRequest, ArtifactDeleteResponse,
    ArtifactFetchRequest, ArtifactFetchResponse, ArtifactListUiRequest, ArtifactListUiResponse,
    ArtifactWriteRequest, ArtifactWriteResponse,
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

#[tauri::command]
pub async fn artifact_write(
    state: State<'_, DaemonState>,
    request: ArtifactWriteRequest,
) -> Result<ArtifactWriteResponse, String> {
    client(&state)
        .runtime()
        .artifact_write(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_delete(
    state: State<'_, DaemonState>,
    request: ArtifactDeleteRequest,
) -> Result<ArtifactDeleteResponse, String> {
    client(&state)
        .runtime()
        .artifact_delete(&request)
        .await
        .map_err(sdk_error)
}
