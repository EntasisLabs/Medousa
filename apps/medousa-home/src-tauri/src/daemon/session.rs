use crate::daemon::types::{
    ActiveSessionTurnResponse, AgentModeId, AgentModeListResponse, AgentModeProposalListResponse,
    AgentModeProposalResponse, AgentModeScope, AgentModeTransitionPolicy,
    CancelActiveSessionTurnResponse, CodeIntentContext, MediaRef, SessionAgentModeResponse,
    SessionCodeBindingResponse, SessionDeleteQuery, SessionDeleteResponse,
    SessionCodeProjectResponse, StartSessionCodeProjectRequest,
    SessionHistoryListResponse, SessionHistoryResponse, SessionSetDisplayNameResponse,
    SetSessionAgentModeRequest, StageRoutingMatrix, TurnSurfaceContext,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::DaemonState;
use super::sdk::{client, sdk_error};
use super::workshop_http;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_profile_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub catalog: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub member_profile_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile_id: Option<String>,
}

#[tauri::command]
pub async fn session_create(
    state: State<'_, DaemonState>,
    catalog: Option<String>,
    member_profile_ids: Option<Vec<String>>,
    agent_profile_id: Option<String>,
    display_name: Option<String>,
) -> Result<CreateSessionResponse, String> {
    workshop_http::post_json(
        &state,
        "/v1/sessions",
        &CreateSessionRequest {
            catalog,
            member_profile_ids,
            agent_profile_id,
            display_name,
        },
    )
    .await
}

#[tauri::command]
pub async fn session_list(
    state: State<'_, DaemonState>,
    limit: Option<usize>,
    include_verification: Option<bool>,
    q: Option<String>,
    cursor: Option<String>,
) -> Result<SessionHistoryListResponse, String> {
    let capped = limit.unwrap_or(50).clamp(1, 200);
    let include_verification = include_verification.unwrap_or(false);
    let mut query = vec![
        ("limit", capped.to_string()),
        ("include_verification", include_verification.to_string()),
    ];
    if let Some(search) = q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query.push(("q", search.to_string()));
    }
    if let Some(page_cursor) = cursor
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query.push(("cursor", page_cursor.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/sessions", &query).await
}

#[tauri::command]
pub async fn session_set_display_name(
    state: State<'_, DaemonState>,
    session_id: String,
    display_name: String,
) -> Result<SessionSetDisplayNameResponse, String> {
    let trimmed_id = session_id.trim();
    if trimmed_id.is_empty() {
        return Err("session_id is required".to_string());
    }
    let trimmed_name = display_name.trim();
    if trimmed_name.is_empty() {
        return Err("display name must not be empty".to_string());
    }

    client(&state)
        .sessions()
        .set_display_name(trimmed_id, trimmed_name)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agent_mode_list(
    state: State<'_, DaemonState>,
) -> Result<AgentModeListResponse, String> {
    client(&state)
        .runtime()
        .agent_modes()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agent_mode_transition_policy_get(
    state: State<'_, DaemonState>,
) -> Result<AgentModeTransitionPolicy, String> {
    client(&state)
        .runtime()
        .agent_mode_transition_policy()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn agent_mode_transition_policy_set(
    state: State<'_, DaemonState>,
    policy: AgentModeTransitionPolicy,
) -> Result<AgentModeTransitionPolicy, String> {
    client(&state)
        .runtime()
        .set_agent_mode_transition_policy(&policy)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_get_agent_mode(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<SessionAgentModeResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    client(&state)
        .sessions()
        .agent_mode(trimmed)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_set_agent_mode(
    state: State<'_, DaemonState>,
    session_id: String,
    mode: AgentModeId,
) -> Result<SessionAgentModeResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    client(&state)
        .sessions()
        .set_agent_mode(
            trimmed,
            &SetSessionAgentModeRequest {
                mode,
                scope: AgentModeScope::Session,
                task_id: None,
                expires_at_utc: None,
            },
        )
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_list_agent_mode_proposals(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<AgentModeProposalListResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    client(&state)
        .sessions()
        .agent_mode_proposals(trimmed)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_decide_agent_mode_proposal(
    state: State<'_, DaemonState>,
    session_id: String,
    proposal_id: String,
    accept: bool,
) -> Result<AgentModeProposalResponse, String> {
    client(&state)
        .sessions()
        .decide_agent_mode_proposal(session_id.trim(), proposal_id.trim(), accept)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_get_code_binding(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<SessionCodeBindingResponse, String> {
    client(&state)
        .sessions()
        .code_binding(session_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_set_code_binding(
    state: State<'_, DaemonState>,
    session_id: String,
    work_id: String,
) -> Result<SessionCodeBindingResponse, String> {
    client(&state)
        .sessions()
        .set_code_binding(session_id.trim(), work_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_clear_code_binding(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<SessionCodeBindingResponse, String> {
    client(&state)
        .sessions()
        .clear_code_binding(session_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_start_code_project(
    state: State<'_, DaemonState>,
    session_id: String,
    request: StartSessionCodeProjectRequest,
) -> Result<SessionCodeProjectResponse, String> {
    client(&state)
        .sessions()
        .start_code_project(session_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_delete(
    state: State<'_, DaemonState>,
    session_id: String,
    purge_memory: Option<bool>,
) -> Result<SessionDeleteResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let query = SessionDeleteQuery {
        purge_memory: purge_memory.unwrap_or(true),
    };
    client(&state)
        .sessions()
        .delete(trimmed, &query)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_get_history(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<SessionHistoryResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    client(&state)
        .sessions()
        .history(trimmed)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_get_active_turn(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<ActiveSessionTurnResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    client(&state)
        .sessions()
        .active_turn(trimmed)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn session_cancel_active_turn(
    state: State<'_, DaemonState>,
    activation_state: State<'_, super::local_inference::LocalInferenceActivationState>,
    session_id: String,
) -> Result<CancelActiveSessionTurnResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    if activation_state.cancel(trimmed) {
        return Ok(CancelActiveSessionTurnResponse {
            cancelled: true,
            turn_id: None,
            message: "Cancelled local model loading".to_string(),
        });
    }
    client(&state)
        .sessions()
        .cancel_active_turn(trimmed)
        .await
        .map_err(sdk_error)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopSteerRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopSteerResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub async fn session_steer_bound_workshop(
    state: State<'_, DaemonState>,
    session_id: String,
    message: String,
) -> Result<WorkshopSteerResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let body = WorkshopSteerRequest { message };
    workshop_http::post_json(
        &state,
        &format!("/v1/sessions/{trimmed}/workshop/steer"),
        &body,
    )
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TurnTicketMode {
    #[default]
    Interactive,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnTicketPhase {
    Accepted,
    Streaming,
    WorkerHandoff,
    BudgetBlocked,
    Done,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTicketResponse {
    pub turn_id: String,
    pub session_id: String,
    pub mode: TurnTicketMode,
    pub phase: TurnTicketPhase,
    pub accepted_at_utc: chrono::DateTime<chrono::Utc>,
    pub stream_url: String,
    pub stream_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_card_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTicketRecord {
    pub turn_id: String,
    pub session_id: String,
    pub mode: TurnTicketMode,
    pub phase: TurnTicketPhase,
    pub stream_url: String,
    pub prompt_preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_card_id: Option<String>,
    pub composer_handoff: bool,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTurnsResponse {
    pub session_id: String,
    pub turns: Vec<TurnTicketRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateTurnTicketBody {
    session_id: String,
    prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_mode: Option<AgentModeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code_context: Option<CodeIntentContext>,
    #[serde(default)]
    code_project_setup_authorized: bool,
    #[serde(default)]
    mode: TurnTicketMode,
    #[serde(default = "default_persist_user_turn")]
    persist_user_turn: bool,
    #[serde(default = "default_response_depth_mode")]
    response_depth_mode: String,
    #[serde(default)]
    reasoning_effort: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    stage_routing: Option<StageRoutingMatrix>,
    #[serde(default)]
    surface: Option<TurnSurfaceContext>,
    #[serde(default)]
    media_refs: Vec<MediaRef>,
    #[serde(default)]
    voice_preset_id: Option<String>,
    #[serde(default)]
    voice_appendix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity_user_id: Option<String>,
}

fn default_persist_user_turn() -> bool {
    true
}

fn default_response_depth_mode() -> String {
    "standard".to_string()
}

#[tauri::command]
pub async fn turn_create(
    state: State<'_, DaemonState>,
    activation_state: State<'_, super::local_inference::LocalInferenceActivationState>,
    session_id: String,
    prompt: String,
    agent_mode: Option<AgentModeId>,
    code_context: Option<CodeIntentContext>,
    code_project_setup_authorized: Option<bool>,
    mode: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    response_depth_mode: Option<String>,
    reasoning_effort: Option<String>,
    stage_routing: Option<StageRoutingMatrix>,
    channel_surface: Option<String>,
    media_refs: Option<Vec<MediaRef>>,
    voice_preset_id: Option<String>,
    voice_appendix: Option<String>,
    identity_user_id: Option<String>,
) -> Result<TurnTicketResponse, String> {
    let trimmed_session = session_id.trim();
    if trimmed_session.is_empty() {
        return Err("session_id is required".to_string());
    }
    if prompt.trim().is_empty() && media_refs.as_ref().is_none_or(|refs| refs.is_empty()) {
        return Err("prompt is required".to_string());
    }

    let ticket_mode = match mode.as_deref().map(str::trim).unwrap_or("interactive") {
        "background" => TurnTicketMode::Background,
        "interactive" => TurnTicketMode::Interactive,
        other => return Err(format!("unknown turn mode '{other}'")),
    };

    let provider = provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let defaults = crate::medousa_paths::load_tui_defaults_summary();
    let selected_provider = if provider.is_empty() {
        defaults.provider.as_deref().unwrap_or_default()
    } else {
        provider.as_str()
    };
    let selected_model = if model.is_empty() {
        defaults.model.as_deref()
    } else {
        Some(model.as_str())
    };
    if selected_provider.eq_ignore_ascii_case("medousa-local") {
        super::local_inference::ensure_local_engine_for_turn(
            &activation_state,
            trimmed_session,
            selected_model,
        )
        .await?;
    }
    let response_depth_mode = response_depth_mode
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "standard".to_string());
    let reasoning_effort = reasoning_effort
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".to_string());
    let stage_routing = stage_routing.unwrap_or_else(|| {
        StageRoutingMatrix::default_for(
            if provider.is_empty() {
                "openai"
            } else {
                provider.as_str()
            },
            if model.is_empty() {
                "gpt-5.4-mini"
            } else {
                model.as_str()
            },
        )
    });
    let channel_surface = channel_surface
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let supports_browser_host = super::resolve_supports_browser_host().await;

    let surface = channel_surface.map(|channel_surface| TurnSurfaceContext {
        channel_surface: Some(channel_surface),
        channel_id: Some(trimmed_session.to_string()),
        user_id: None,
        supports_ui_artifacts: true,
        supports_liquid_markdown: true,
        supports_browser_host,
    });

    let body = CreateTurnTicketBody {
        session_id: trimmed_session.to_string(),
        prompt,
        agent_mode,
        code_context,
        code_project_setup_authorized: code_project_setup_authorized.unwrap_or(false),
        mode: ticket_mode,
        persist_user_turn: true,
        response_depth_mode,
        reasoning_effort,
        provider,
        model,
        stage_routing: Some(stage_routing),
        surface,
        media_refs: media_refs.unwrap_or_default(),
        voice_preset_id: voice_preset_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        voice_appendix: voice_appendix
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        identity_user_id: identity_user_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    };

    workshop_http::post_json(&state, "/v1/turns", &body).await
}

#[tauri::command]
pub async fn turn_list_session(
    state: State<'_, DaemonState>,
    session_id: String,
    active_only: Option<bool>,
) -> Result<SessionTurnsResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    let active = active_only.unwrap_or(true);
    workshop_http::get_json(
        &state,
        &format!("/v1/sessions/{trimmed}/turns?active={active}"),
    )
    .await
}
