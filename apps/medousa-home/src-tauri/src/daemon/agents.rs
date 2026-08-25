use medousa_types::{
    AgentPermissionRequestListResponse, AgentPermissionResolveRequest,
    AgentPermissionResolveResponse, AgentRuntimeListResponse, AgentSecretDenyRequest,
    AgentSecretFulfillRequest, AgentSecretRequestListResponse, AgentSecretResolveResponse,
    AgentSessionPromptRequest, AgentSessionPromptResponse, CancelAgentSessionResponse,
    CreateAgentSessionRequest, CreateAgentSessionResponse, SetAgentSessionConfigOptionRequest,
    SetAgentSessionConfigOptionResponse,
};
use tauri::State;

use super::DaemonState;
use super::sdk::{client, sdk_error};

#[tauri::command]
pub async fn agents_list_runtimes(
    state: State<'_, DaemonState>,
) -> Result<AgentRuntimeListResponse, String> {
    client(&state)?
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
    client(&state)?
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
    client(&state)?
        .agents()
        .prompt(agent_session_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_set_config_option(
    state: State<'_, DaemonState>,
    agent_session_id: String,
    request: SetAgentSessionConfigOptionRequest,
) -> Result<SetAgentSessionConfigOptionResponse, String> {
    client(&state)?
        .agents()
        .set_config_option(agent_session_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_cancel(
    state: State<'_, DaemonState>,
    agent_session_id: String,
) -> Result<CancelAgentSessionResponse, String> {
    client(&state)?
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
    client(&state)?
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
    client(&state)?
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
    client(&state)?
        .agents()
        .deny_permission(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_list_secret_requests(
    state: State<'_, DaemonState>,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<AgentSecretRequestListResponse, String> {
    client(&state)?
        .agents()
        .list_secret_requests(status.as_deref(), limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_fulfill_secret_request(
    state: State<'_, DaemonState>,
    request_id: String,
    value: String,
    resolved_by: Option<String>,
) -> Result<AgentSecretResolveResponse, String> {
    let request = AgentSecretFulfillRequest { value, resolved_by };
    client(&state)?
        .agents()
        .fulfill_secret_request(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_deny_secret_request(
    state: State<'_, DaemonState>,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<AgentSecretResolveResponse, String> {
    let request = AgentSecretDenyRequest { resolved_by };
    client(&state)?
        .agents()
        .deny_secret_request(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}
