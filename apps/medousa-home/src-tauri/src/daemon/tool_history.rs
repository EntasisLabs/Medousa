use crate::daemon::types::{
    ToolHistoryListResponse, WorkflowFromSliceRequest, WorkflowFromSliceResponse,
};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::workshop_http;

#[tauri::command]
pub async fn tool_history_list_slices(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    limit: Option<usize>,
    session_limit: Option<usize>,
    session_id: Option<String>,
    tool_filter: Option<String>,
    keyword: Option<String>,
) -> Result<ToolHistoryListResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_tool_history(medousa_types::daemon_api::ToolHistoryListQuery {
                limit,
                session_limit,
                session_id,
                tool_filter,
                keyword,
            })
            .map_err(|error| error.to_string());
    }
    let mut params = Vec::new();
    if let Some(value) = limit {
        params.push(("limit", value.to_string()));
    }
    if let Some(value) = session_limit {
        params.push(("session_limit", value.to_string()));
    }
    if let Some(value) = session_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        params.push(("session_id", value.to_string()));
    }
    if let Some(value) = tool_filter
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
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
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: WorkflowFromSliceRequest,
) -> Result<WorkflowFromSliceResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .workflow_from_slice(request)
            .await
            .map_err(|error| error.to_string());
    }
    workshop_http::post_json(&state, "/v1/workflows/from-slice", &request).await
}
