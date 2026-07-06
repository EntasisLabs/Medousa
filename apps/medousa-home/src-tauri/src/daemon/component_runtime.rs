use medousa_types::component_runtime::{
    ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProbeCompleteOk {
    pub ok: bool,
}

#[tauri::command]
pub async fn component_runtime_append_events(
    state: State<'_, DaemonState>,
    component_id: String,
    request: ComponentRuntimeEventsRequest,
) -> Result<ComponentRuntimeEventsResponse, String> {
    let component_id = component_id.trim();
    if component_id.is_empty() {
        return Err("component_id is required".to_string());
    }
    client(&state)
        .components()
        .runtime_append_events(component_id, &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn component_runtime_tail_events(
    state: State<'_, DaemonState>,
    component_id: String,
    profile_id: Option<String>,
    limit: Option<usize>,
) -> Result<ComponentRuntimeEventsTailResponse, String> {
    let component_id = component_id.trim();
    if component_id.is_empty() {
        return Err("component_id is required".to_string());
    }

    client(&state)
        .components()
        .runtime_tail_events(
            component_id,
            profile_id.as_deref().filter(|id| !id.trim().is_empty()),
            limit,
        )
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn component_runtime_complete_probe(
    state: State<'_, DaemonState>,
    component_id: String,
    probe_id: String,
    result: ComponentRuntimeProbeResult,
) -> Result<ProbeCompleteOk, String> {
    let component_id = component_id.trim();
    let probe_id = probe_id.trim();
    if component_id.is_empty() || probe_id.is_empty() {
        return Err("component_id and probe_id are required".to_string());
    }
    client(&state)
        .components()
        .runtime_complete_probe(component_id, probe_id, &result)
        .await
        .map(|value| {
            value
                .get("ok")
                .and_then(|ok| ok.as_bool())
                .map(|ok| ProbeCompleteOk { ok })
                .unwrap_or(ProbeCompleteOk { ok: true })
        })
        .map_err(sdk_error)
}
