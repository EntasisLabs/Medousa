use crate::daemon::types::{
    ContinuationStatusResponse, DaemonStatsResponse, DeliveryHealthResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeDefaultsResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

async fn daemon_get<T: serde::de::DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .get(format!("{base}{path}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("daemon GET {path} failed ({status}): {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

async fn daemon_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .post(format!("{base}{path}"))
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("daemon POST {path} failed ({status}): {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn runtime_get_stats(
    state: State<'_, DaemonState>,
) -> Result<DaemonStatsResponse, String> {
    daemon_get(&state, "/v1/stats").await
}

#[tauri::command]
pub async fn runtime_get_defaults(
    state: State<'_, DaemonState>,
) -> Result<RuntimeDefaultsResponse, String> {
    daemon_get(&state, "/v1/runtime/defaults").await
}

#[tauri::command]
pub async fn runtime_get_delivery_status(
    state: State<'_, DaemonState>,
) -> Result<DeliveryHealthResponse, String> {
    daemon_get(&state, "/v1/delivery/status").await
}

#[tauri::command]
pub async fn runtime_get_continuation_status(
    state: State<'_, DaemonState>,
) -> Result<ContinuationStatusResponse, String> {
    daemon_get(&state, "/v1/continuations/status").await
}

#[tauri::command]
pub async fn runtime_config_command(
    state: State<'_, DaemonState>,
    request: RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse, String> {
    daemon_post(&state, "/v1/runtime/config/command", &request).await
}

#[tauri::command]
pub async fn runtime_stage_route_command(
    state: State<'_, DaemonState>,
    request: StageRouteCommandRequest,
) -> Result<StageRouteCommandResponse, String> {
    daemon_post(&state, "/v1/runtime/stage-route/command", &request).await
}
