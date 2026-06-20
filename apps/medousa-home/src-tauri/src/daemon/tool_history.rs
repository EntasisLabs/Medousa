use crate::daemon::types::{
    ToolHistoryListResponse, WorkflowFromSliceRequest, WorkflowFromSliceResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn tool_history_list_slices(
    state: State<'_, DaemonState>,
    limit: Option<usize>,
    session_limit: Option<usize>,
    session_id: Option<String>,
    tool_filter: Option<String>,
    keyword: Option<String>,
) -> Result<ToolHistoryListResponse, String> {
    let mut params = Vec::new();
    if let Some(value) = limit {
        params.push(("limit", value.to_string()));
    }
    if let Some(value) = session_limit {
        params.push(("session_limit", value.to_string()));
    }
    if let Some(value) = session_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("session_id", value.to_string()));
    }
    if let Some(value) = tool_filter.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("tool_filter", value.to_string()));
    }
    if let Some(value) = keyword.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("keyword", value.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/tool-history/slices", &params).await
}

#[tauri::command]
pub async fn workflow_from_slice(
    state: State<'_, DaemonState>,
    request: WorkflowFromSliceRequest,
) -> Result<WorkflowFromSliceResponse, String> {
    workshop_http::post_json(&state, "/v1/workflows/from-slice", &request).await
}
