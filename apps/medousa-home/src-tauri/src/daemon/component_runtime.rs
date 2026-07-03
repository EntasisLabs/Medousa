use medousa_types::component_runtime::{
    ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult,
};
use tauri::State;

use super::workshop_http::{self, path_with_query, post_json};
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
    let path = format!("/v1/components/{component_id}/runtime/events");
    post_json(&state, &path, &request).await
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

    let mut query = Vec::new();
    if let Some(profile_id) = profile_id.filter(|id| !id.trim().is_empty()) {
        query.push(("profile_id", profile_id));
    }
    if let Some(limit) = limit {
        query.push(("limit", limit.to_string()));
    }
    let path = if query.is_empty() {
        format!("/v1/components/{component_id}/runtime/events")
    } else {
        path_with_query(
            &format!("/v1/components/{component_id}/runtime/events"),
            &query.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        )
    };
    workshop_http::get_json(&state, &path).await
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
    let path = format!("/v1/components/{component_id}/runtime/probe/{probe_id}/result");
    post_json(&state, &path, &result).await
}
