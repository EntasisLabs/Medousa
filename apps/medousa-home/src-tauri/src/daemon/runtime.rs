use crate::daemon::types::{
    ContinuationStatusResponse, DaemonStatsResponse, DeliveryHealthResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeDefaultsResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};
use crate::medousa_paths::TuiDefaultsDto;
use tauri::State;

use super::sdk::{client, sdk_error};
use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn runtime_get_stats(
    state: State<'_, DaemonState>,
) -> Result<DaemonStatsResponse, String> {
    workshop_http::get_json(&state, "/v1/stats").await
}

#[tauri::command]
pub async fn runtime_get_tui_defaults(
    state: State<'_, DaemonState>,
) -> Result<TuiDefaultsDto, String> {
    let value: serde_json::Value =
        workshop_http::get_json(&state, "/v1/runtime/tui-defaults").await?;
    Ok(crate::medousa_paths::tui_defaults_dto_from_value(&value))
}

#[tauri::command]
pub async fn runtime_put_tui_defaults(
    state: State<'_, DaemonState>,
    dto: TuiDefaultsDto,
) -> Result<(), String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = (state, dto);
        return Err(
            "Workshop charter is read-only on mobile — edit tui_defaults.json on the host."
                .to_string(),
        );
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let body = crate::medousa_paths::tui_defaults_value_from_dto(&dto);
        let _: serde_json::Value =
            workshop_http::put_json(&state, "/v1/runtime/tui-defaults", &body).await?;
        Ok(())
    }
}

#[tauri::command]
pub async fn migrate_global_tui_defaults_to_engine(
    state: State<'_, DaemonState>,
) -> Result<bool, String> {
    let legacy = crate::medousa_paths::global_host_tui_defaults_path();
    if !legacy.is_file()
        || crate::medousa_paths::global_host_tui_defaults_migrated_marker().is_file()
    {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&legacy).map_err(|err| err.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| format!("legacy defaults invalid: {err}"))?;
    let _: serde_json::Value =
        workshop_http::put_json(&state, "/v1/runtime/tui-defaults", &value).await?;
    crate::medousa_paths::migrate_global_tui_defaults_if_needed()?;
    Ok(true)
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
    client(&state)
        .runtime()
        .config_command(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn runtime_stage_route_command(
    state: State<'_, DaemonState>,
    request: StageRouteCommandRequest,
) -> Result<StageRouteCommandResponse, String> {
    client(&state)
        .runtime()
        .stage_route_command(&request)
        .await
        .map_err(sdk_error)
}
