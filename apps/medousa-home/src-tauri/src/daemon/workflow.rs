use crate::daemon::types::{
    WorkflowDetailResponse, WorkflowPlanRequest, WorkflowPlanResponse, WorkflowRunRequest,
    WorkflowRunResponse, WorkflowRunsResponse, WorkflowScheduleRequest, WorkflowScheduleResponse,
    WorkflowsListResponse,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::workshop_http;

#[tauri::command]
pub async fn workflow_list(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    limit: Option<usize>,
) -> Result<WorkflowsListResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_workflows(limit)
            .await
            .map_err(|error| error.to_string());
    }
    let path = if let Some(limit) = limit {
        format!("/v1/workflows?limit={limit}")
    } else {
        "/v1/workflows".to_string()
    };
    workshop_http::get_json(&state, &path).await
}

#[tauri::command]
pub async fn workflow_get(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    workflow_id: String,
) -> Result<WorkflowDetailResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .get_workflow(workflow_id)
            .await
            .map_err(|error| error.to_string());
    }
    let id = urlencoding::encode(workflow_id.trim());
    workshop_http::get_json(&state, &format!("/v1/workflows/{id}")).await
}

#[tauri::command]
pub async fn workflow_run(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRunResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .run_workflow(request)
            .await
            .map_err(|error| error.to_string());
    }
    workshop_http::post_json(&state, "/v1/workflows", &request).await
}

#[tauri::command]
pub async fn workflow_plan(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: WorkflowPlanRequest,
) -> Result<WorkflowPlanResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .plan_workflow(request)
            .map_err(|error| error.to_string());
    }
    workshop_http::post_json(&state, "/v1/workflows/plan", &request).await
}

#[tauri::command]
pub async fn workflow_schedule(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: WorkflowScheduleRequest,
) -> Result<WorkflowScheduleResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .schedule_workflow(request)
            .await
            .map_err(|error| error.to_string());
    }
    workshop_http::post_json(&state, "/v1/workflows/schedule", &request).await
}

#[tauri::command]
pub async fn workflow_list_runs(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    workflow_id: String,
    limit: Option<usize>,
) -> Result<WorkflowRunsResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_workflow_runs(workflow_id, limit)
            .await
            .map_err(|error| error.to_string());
    }
    let id = urlencoding::encode(workflow_id.trim());
    let path = if let Some(limit) = limit {
        format!("/v1/workflows/{id}/runs?limit={limit}")
    } else {
        format!("/v1/workflows/{id}/runs")
    };
    workshop_http::get_json(&state, &path).await
}
