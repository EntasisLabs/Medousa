use crate::daemon::types::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactDeleteRequest, ArtifactDeleteResponse,
    ArtifactFetchRequest, ArtifactFetchResponse, ArtifactListUiRequest, ArtifactListUiResponse,
    ArtifactWriteRequest, ArtifactWriteResponse,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::sdk::{client, sdk_error};

#[tauri::command]
pub async fn artifact_command(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_command(request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .runtime()
        .artifact_command(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_fetch(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactFetchRequest,
) -> Result<ArtifactFetchResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_fetch(request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .runtime()
        .artifact_fetch(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_list_ui(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactListUiRequest,
) -> Result<ArtifactListUiResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_list_ui(request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .runtime()
        .artifact_list_ui(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_write(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactWriteRequest,
) -> Result<ArtifactWriteResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_write(request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .runtime()
        .artifact_write(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_delete(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactDeleteRequest,
) -> Result<ArtifactDeleteResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_delete(request)
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .runtime()
        .artifact_delete(&request)
        .await
        .map_err(sdk_error)
}
