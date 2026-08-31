use crate::daemon::types::{
    ContinuationStatusResponse, DaemonStatsResponse, DeliveryHealthResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeDefaultsResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};
use crate::medousa_paths::TuiDefaultsDto;
use tauri::State;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeWorkerConfigDto {
    pub max_in_flight: usize,
    pub agents: usize,
    pub scheduled: usize,
    pub delivery: usize,
    pub maintenance: usize,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct RuntimeWorkerConfigWire {
    max_in_flight: usize,
    agents: usize,
    scheduled: usize,
    delivery: usize,
    maintenance: usize,
}

impl From<RuntimeWorkerConfigWire> for RuntimeWorkerConfigDto {
    fn from(value: RuntimeWorkerConfigWire) -> Self {
        Self {
            max_in_flight: value.max_in_flight,
            agents: value.agents,
            scheduled: value.scheduled,
            delivery: value.delivery,
            maintenance: value.maintenance,
        }
    }
}

impl From<RuntimeWorkerConfigDto> for RuntimeWorkerConfigWire {
    fn from(value: RuntimeWorkerConfigDto) -> Self {
        Self {
            max_in_flight: value.max_in_flight,
            agents: value.agents,
            scheduled: value.scheduled,
            delivery: value.delivery,
            maintenance: value.maintenance,
        }
    }
}

use super::sdk::{client, sdk_error};
use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn runtime_get_stats(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<DaemonStatsResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .runtime_stats()
            .await
            .map_err(|error| format!("embedded runtime stats: {error:#}"));
    }
    workshop_http::get_json(&state, "/v1/stats").await
}

#[tauri::command]
pub async fn runtime_get_tui_defaults(
    state: State<'_, DaemonState>,
) -> Result<TuiDefaultsDto, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if matches!(
        crate::active_workshop::resolve()?,
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal
    ) {
        return crate::embedded_daemon::normalize_inference_defaults(
            crate::medousa_paths::load_tui_defaults(),
        );
    }
    let value: serde_json::Value =
        workshop_http::get_json(&state, "/v1/runtime/tui-defaults").await?;
    Ok(crate::medousa_paths::tui_defaults_dto_from_value(&value))
}

#[tauri::command]
pub async fn runtime_put_tui_defaults(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    dto: TuiDefaultsDto,
) -> Result<(), String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if matches!(
        crate::active_workshop::resolve()?,
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal
    ) {
        _embedded_state.validate_inference_defaults(&dto)?;
        let previous = crate::medousa_paths::load_tui_defaults();
        crate::medousa_paths::persist_tui_defaults(dto.clone())?;
        if let Err(error) = _embedded_state.reconfigure_active(&dto).await {
            let _ = crate::medousa_paths::persist_tui_defaults(previous.clone());
            let _ = _embedded_state.reconfigure_active(&previous).await;
            return Err(error);
        }
        return Ok(());
    }
    let body = crate::medousa_paths::tui_defaults_value_from_dto(&dto);
    let _: serde_json::Value =
        workshop_http::put_json(&state, "/v1/runtime/tui-defaults", &body).await?;
    Ok(())
}

#[tauri::command]
pub async fn migrate_global_tui_defaults_to_engine(
    state: State<'_, DaemonState>,
) -> Result<bool, String> {
    let co_located = match crate::active_workshop::resolve()? {
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal => true,
        crate::active_workshop::ActiveWorkshopTarget::Transport { workshop, .. } => {
            workshop.kind == "local"
        }
    };
    if !co_located {
        return Ok(false);
    }
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
pub async fn runtime_get_worker_config(
    state: State<'_, DaemonState>,
) -> Result<RuntimeWorkerConfigDto, String> {
    let config: RuntimeWorkerConfigWire =
        workshop_http::get_json(&state, "/v1/runtime/workers").await?;
    Ok(config.into())
}

#[tauri::command]
pub async fn runtime_put_worker_config(
    state: State<'_, DaemonState>,
    config: RuntimeWorkerConfigDto,
) -> Result<RuntimeWorkerConfigDto, String> {
    let body: RuntimeWorkerConfigWire = config.into();
    let saved: RuntimeWorkerConfigWire =
        workshop_http::put_json(&state, "/v1/runtime/workers", &body).await?;
    Ok(saved.into())
}

#[tauri::command]
pub async fn runtime_get_delivery_status(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<DeliveryHealthResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .runtime_delivery_status()
            .await
            .map_err(|error| format!("embedded delivery stats: {error:#}"));
    }
    workshop_http::get_json(&state, "/v1/delivery/status").await
}

#[tauri::command]
pub async fn runtime_get_continuation_status(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<ContinuationStatusResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .runtime_continuation_status()
            .map_err(|error| format!("embedded continuation stats: {error:#}"));
    }
    workshop_http::get_json(&state, "/v1/continuations/status").await
}

#[tauri::command]
pub async fn runtime_config_command(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if matches!(
        crate::active_workshop::resolve()?,
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal
    ) {
        let response =
            medousa::runtime_config_command_runtime::execute_runtime_config_command(request)
                .map_err(|error| format!("embedded runtime config command: {error:#}"))?;
        if response.should_apply_settings {
            let mut defaults = crate::medousa_paths::load_tui_defaults();
            defaults.provider = Some(response.next_draft_provider.clone());
            defaults.model = Some(response.next_draft_model.clone());
            _embedded_state.validate_inference_defaults(&defaults)?;
        }
        return Ok(response);
    }
    client(&state)?
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
    client(&state)?
        .runtime()
        .stage_route_command(&request)
        .await
        .map_err(sdk_error)
}
