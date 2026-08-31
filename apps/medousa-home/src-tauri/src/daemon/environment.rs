use crate::daemon::types::{
    EnvironmentPendingResponse, EnvironmentSpecPutRequest, EnvironmentSpecResponse,
    EnvironmentStatusResponse,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::sdk::{client, sdk_error};

#[tauri::command]
pub async fn environment_get_status(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    profile_id: Option<String>,
    surface_id: Option<String>,
    include_runtime: Option<bool>,
) -> Result<EnvironmentStatusResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .environment_status(
                profile_id.as_deref().filter(|id| !id.trim().is_empty()),
                surface_id.as_deref().filter(|id| !id.trim().is_empty()),
            )
            .await
            .map_err(|error| format!("embedded environment status: {error:#}"));
    }
    client(&state)?
        .environment()
        .get_status(
            profile_id.as_deref().filter(|id| !id.trim().is_empty()),
            surface_id.as_deref().filter(|id| !id.trim().is_empty()),
            include_runtime,
        )
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_get_spec(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentSpecResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .environment_spec(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
            .await
            .map_err(|error| format!("embedded environment spec: {error:#}"));
    }
    client(&state)?
        .environment()
        .get_spec(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_put_spec(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: EnvironmentSpecPutRequest,
) -> Result<EnvironmentSpecResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .put_environment_spec(request)
            .await
            .map_err(|error| format!("save embedded environment spec: {error:#}"));
    }
    client(&state)?
        .environment()
        .put_spec(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_get_pending(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentPendingResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .environment_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
            .await
            .map_err(|error| format!("read embedded environment proposal: {error:#}"));
    }
    client(&state)?
        .environment()
        .get_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_apply_pending(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentSpecResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .apply_environment_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
            .await
            .map_err(|error| format!("apply embedded environment proposal: {error:#}"));
    }
    client(&state)?
        .environment()
        .apply_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_dismiss_pending(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    profile_id: Option<String>,
) -> Result<(), String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .dismiss_environment_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
            .await
            .map_err(|error| format!("dismiss embedded environment proposal: {error:#}"));
    }
    client(&state)?
        .environment()
        .dismiss_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}
