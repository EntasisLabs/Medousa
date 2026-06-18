use crate::daemon::types::{
    ContinuationStatusResponse, DaemonStatsResponse, DeliveryHealthResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeDefaultsResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn runtime_get_stats(
    state: State<'_, DaemonState>,
) -> Result<DaemonStatsResponse, String> {
    workshop_http::get_json(&state, "/v1/stats").await
}

#[tauri::command]
pub async fn runtime_get_defaults(
    state: State<'_, DaemonState>,
) -> Result<RuntimeDefaultsResponse, String> {
    workshop_http::get_json(&state, "/v1/runtime/defaults").await
}

#[tauri::command]
pub async fn runtime_get_delivery_status(
    state: State<'_, DaemonState>,
) -> Result<DeliveryHealthResponse, String> {
    workshop_http::get_json(&state, "/v1/delivery/status").await
}

#[tauri::command]
pub async fn runtime_get_continuation_status(
    state: State<'_, DaemonState>,
) -> Result<ContinuationStatusResponse, String> {
    workshop_http::get_json(&state, "/v1/continuations/status").await
}

#[tauri::command]
pub async fn runtime_config_command(
    state: State<'_, DaemonState>,
    request: RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse, String> {
    workshop_http::post_json(&state, "/v1/runtime/config/command", &request).await
}

#[tauri::command]
pub async fn runtime_stage_route_command(
    state: State<'_, DaemonState>,
    request: StageRouteCommandRequest,
) -> Result<StageRouteCommandResponse, String> {
    workshop_http::post_json(&state, "/v1/runtime/stage-route/command", &request).await
}
