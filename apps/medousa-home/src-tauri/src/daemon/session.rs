use crate::daemon::types::{
    MediaRef, SessionHistoryListResponse, SessionHistoryResponse, StageRoutingMatrix,
    TurnSurfaceContext,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

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
    if let Some(search) = q.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        query.push(("q", search.to_string()));
    }
    if let Some(page_cursor) = cursor.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        query.push(("cursor", page_cursor.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/sessions", &query).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSetDisplayNameResponse {
    pub session_id: String,
    pub display_name: String,
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

    workshop_http::put_json(
        &state,
        &format!("/v1/sessions/{trimmed_id}/name"),
        &serde_json::json!({ "display_name": trimmed_name }),
    )
    .await
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
    workshop_http::get_json(&state, &format!("/v1/sessions/{trimmed}/history")).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessionTurn {
    pub turn_id: String,
    pub session_id: String,
    pub stream_url: String,
    pub phase: String,
    pub composer_handoff: bool,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessionTurnResponse {
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<ActiveSessionTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelActiveSessionTurnResponse {
    pub cancelled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub message: String,
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
    workshop_http::get_json(&state, &format!("/v1/sessions/{trimmed}/active-turn")).await
}

#[tauri::command]
pub async fn session_cancel_active_turn(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<CancelActiveSessionTurnResponse, String> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err("session_id is required".to_string());
    }
    workshop_http::post_empty_json(&state, &format!("/v1/sessions/{trimmed}/active-turn")).await
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
    session_id: String,
    prompt: String,
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
            if provider.is_empty() { "openai" } else { provider.as_str() },
            if model.is_empty() { "gpt-5.4-mini" } else { model.as_str() },
        )
    });
    let channel_surface = channel_surface
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let surface = channel_surface.map(|channel_surface| TurnSurfaceContext {
        channel_surface: Some(channel_surface),
        channel_id: Some(trimmed_session.to_string()),
        user_id: Some(trimmed_session.to_string()),
    });

    let body = CreateTurnTicketBody {
        session_id: trimmed_session.to_string(),
        prompt,
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
