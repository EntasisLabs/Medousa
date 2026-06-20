use crate::daemon::types::{
    WorkflowDetailResponse, WorkflowPlanRequest, WorkflowPlanResponse, WorkflowRunRequest,
    WorkflowRunResponse, WorkflowRunsResponse, WorkflowScheduleRequest, WorkflowScheduleResponse,
    WorkflowsListResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn workflow_list(
    state: State<'_, DaemonState>,
    limit: Option<usize>,
) -> Result<WorkflowsListResponse, String> {
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
    workflow_id: String,
) -> Result<WorkflowDetailResponse, String> {
    let id = urlencoding::encode(workflow_id.trim());
    workshop_http::get_json(&state, &format!("/v1/workflows/{id}")).await
}

#[tauri::command]
pub async fn workflow_run(
    state: State<'_, DaemonState>,
    request: WorkflowRunRequest,
) -> Result<WorkflowRunResponse, String> {
    workshop_http::post_json(&state, "/v1/workflows", &request).await
}

#[tauri::command]
pub async fn workflow_plan(
    state: State<'_, DaemonState>,
    request: WorkflowPlanRequest,
) -> Result<WorkflowPlanResponse, String> {
    workshop_http::post_json(&state, "/v1/workflows/plan", &request).await
}

#[tauri::command]
pub async fn workflow_schedule(
    state: State<'_, DaemonState>,
    request: WorkflowScheduleRequest,
) -> Result<WorkflowScheduleResponse, String> {
    workshop_http::post_json(&state, "/v1/workflows/schedule", &request).await
}

#[tauri::command]
pub async fn workflow_list_runs(
    state: State<'_, DaemonState>,
    workflow_id: String,
    limit: Option<usize>,
) -> Result<WorkflowRunsResponse, String> {
    let id = urlencoding::encode(workflow_id.trim());
    let path = if let Some(limit) = limit {
        format!("/v1/workflows/{id}/runs?limit={limit}")
    } else {
        format!("/v1/workflows/{id}/runs")
    };
    workshop_http::get_json(&state, &path).await
}
