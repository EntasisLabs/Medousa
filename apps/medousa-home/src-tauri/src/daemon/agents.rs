use medousa_types::{
    AgentPermissionRequestListResponse, AgentPermissionResolveRequest,
    AgentPermissionResolveResponse, AgentRuntimeListResponse, AgentSessionPromptRequest,
    AgentSessionPromptResponse, CancelAgentSessionResponse, CreateAgentSessionRequest,
    CreateAgentSessionResponse,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn agents_list_runtimes(
    state: State<'_, DaemonState>,
) -> Result<AgentRuntimeListResponse, String> {
    client(&state)
        .agents()
        .list_runtimes()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_create_session(
    state: State<'_, DaemonState>,
    request: CreateAgentSessionRequest,
) -> Result<CreateAgentSessionResponse, String> {
    client(&state)
        .agents()
        .create_session(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_prompt(
    state: State<'_, DaemonState>,
    agent_session_id: String,
    request: AgentSessionPromptRequest,
) -> Result<AgentSessionPromptResponse, String> {
    client(&state)
        .agents()
        .prompt(agent_session_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_cancel(
    state: State<'_, DaemonState>,
    agent_session_id: String,
) -> Result<CancelAgentSessionResponse, String> {
    client(&state)
        .agents()
        .cancel(agent_session_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_list_permission_requests(
    state: State<'_, DaemonState>,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<AgentPermissionRequestListResponse, String> {
    client(&state)
        .agents()
        .list_permission_requests(status.as_deref(), limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_approve_permission(
    state: State<'_, DaemonState>,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<AgentPermissionResolveResponse, String> {
    let request = AgentPermissionResolveRequest { resolved_by };
    client(&state)
        .agents()
        .approve_permission(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_deny_permission(
    state: State<'_, DaemonState>,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<AgentPermissionResolveResponse, String> {
    let request = AgentPermissionResolveRequest { resolved_by };
    client(&state)
        .agents()
        .deny_permission(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}
