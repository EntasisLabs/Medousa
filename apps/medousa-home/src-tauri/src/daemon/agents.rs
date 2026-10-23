use medousa_types::{
    AgentPermissionRequestListResponse, AgentPermissionResolveRequest,
    AgentPermissionResolveResponse, AgentRuntimeListResponse, AgentSecretDenyRequest,
    AgentSecretFulfillRequest, AgentSecretRequestListResponse, AgentSecretResolveResponse,
    AgentSessionPromptRequest, AgentSessionPromptResponse, CancelAgentSessionResponse,
    CreateAgentSessionRequest, CreateAgentSessionResponse, SetAgentSessionConfigOptionRequest,
    SetAgentSessionConfigOptionResponse,
};

use super::window_request::WorkshopRequest;
use super::sdk::{client_for_request as client, sdk_error};

#[tauri::command]
pub async fn agents_list_runtimes(
    state: WorkshopRequest,
) -> Result<AgentRuntimeListResponse, String> {
    client(&state).await?
        .agents()
        .list_runtimes()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_create_session(
    state: WorkshopRequest,
    request: CreateAgentSessionRequest,
) -> Result<CreateAgentSessionResponse, String> {
    client(&state).await?
        .agents()
        .create_session(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_prompt(
    state: WorkshopRequest,
    agent_session_id: String,
    request: AgentSessionPromptRequest,
) -> Result<AgentSessionPromptResponse, String> {
    client(&state).await?
        .agents()
        .prompt(agent_session_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_set_config_option(
    state: WorkshopRequest,
    agent_session_id: String,
    request: SetAgentSessionConfigOptionRequest,
) -> Result<SetAgentSessionConfigOptionResponse, String> {
    client(&state).await?
        .agents()
        .set_config_option(agent_session_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_cancel(
    state: WorkshopRequest,
    agent_session_id: String,
) -> Result<CancelAgentSessionResponse, String> {
    client(&state).await?
        .agents()
        .cancel(agent_session_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_list_permission_requests(
    state: WorkshopRequest,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<AgentPermissionRequestListResponse, String> {
    client(&state).await?
        .agents()
        .list_permission_requests(status.as_deref(), limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_approve_permission(
    state: WorkshopRequest,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<AgentPermissionResolveResponse, String> {
    let request = AgentPermissionResolveRequest { resolved_by };
    client(&state).await?
        .agents()
        .approve_permission(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_deny_permission(
    state: WorkshopRequest,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<AgentPermissionResolveResponse, String> {
    let request = AgentPermissionResolveRequest { resolved_by };
    client(&state).await?
        .agents()
        .deny_permission(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_list_secret_requests(
    state: WorkshopRequest,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<AgentSecretRequestListResponse, String> {
    client(&state).await?
        .agents()
        .list_secret_requests(status.as_deref(), limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_fulfill_secret_request(
    state: WorkshopRequest,
    request_id: String,
    value: String,
    resolved_by: Option<String>,
) -> Result<AgentSecretResolveResponse, String> {
    let request = AgentSecretFulfillRequest { value, resolved_by };
    client(&state).await?
        .agents()
        .fulfill_secret_request(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agents_deny_secret_request(
    state: WorkshopRequest,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<AgentSecretResolveResponse, String> {
    let request = AgentSecretDenyRequest { resolved_by };
    client(&state).await?
        .agents()
        .deny_secret_request(request_id.trim(), &request)
        .await
        .map_err(sdk_error)
}
