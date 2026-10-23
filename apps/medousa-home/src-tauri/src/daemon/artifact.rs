use crate::daemon::types::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactDeleteRequest, ArtifactDeleteResponse,
    ArtifactFetchRequest, ArtifactFetchResponse, ArtifactListUiRequest, ArtifactListUiResponse,
    ArtifactWriteRequest, ArtifactWriteResponse,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::window_request::WorkshopRequest;
use super::sdk::{client_for_request as client, sdk_error};

#[tauri::command]
pub async fn artifact_command(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_command(request)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .runtime()
        .artifact_command(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_fetch(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactFetchRequest,
) -> Result<ArtifactFetchResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_fetch(request)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .runtime()
        .artifact_fetch(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_list_ui(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactListUiRequest,
) -> Result<ArtifactListUiResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_list_ui(request)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .runtime()
        .artifact_list_ui(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_write(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactWriteRequest,
) -> Result<ArtifactWriteResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_write(request)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .runtime()
        .artifact_write(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn artifact_delete(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: ArtifactDeleteRequest,
) -> Result<ArtifactDeleteResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .artifact_delete(request)
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .runtime()
        .artifact_delete(&request)
        .await
        .map_err(sdk_error)
}
