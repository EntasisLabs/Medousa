use crate::daemon::types::{
    EnvironmentPendingResponse, EnvironmentSpecPutRequest, EnvironmentSpecResponse,
    EnvironmentStatusResponse,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn environment_get_status(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
    surface_id: Option<String>,
    include_runtime: Option<bool>,
) -> Result<EnvironmentStatusResponse, String> {
    client(&state)
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
    profile_id: Option<String>,
) -> Result<EnvironmentSpecResponse, String> {
    client(&state)
        .environment()
        .get_spec(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_put_spec(
    state: State<'_, DaemonState>,
    request: EnvironmentSpecPutRequest,
) -> Result<EnvironmentSpecResponse, String> {
    client(&state)
        .environment()
        .put_spec(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_get_pending(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentPendingResponse, String> {
    client(&state)
        .environment()
        .get_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_apply_pending(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentSpecResponse, String> {
    client(&state)
        .environment()
        .apply_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn environment_dismiss_pending(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<(), String> {
    client(&state)
        .environment()
        .dismiss_pending(profile_id.as_deref().filter(|id| !id.trim().is_empty()))
        .await
        .map_err(sdk_error)
}
