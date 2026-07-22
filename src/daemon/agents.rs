//! Daemon `/v1/agents` — hot-swappable external agent runtimes (ACP via SDK).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::sse::Sse;
use axum::Json;
use chrono::Utc;
use futures_util::Stream;
use medousa_acp_client::{
    AcpClient, AcpEvent, AgentRuntimeKind, ExternalAcpClient, external_runtime_config,
    runtime_availability,
};
use medousa_types::{
    AgentPermissionRequestListQuery, AgentPermissionRequestListResponse,
    AgentPermissionResolveRequest, AgentPermissionResolveResponse, AgentRuntimeInfo,
    AgentRuntimeListResponse, AgentSessionPromptRequest, AgentSessionPromptResponse,
    CancelAgentSessionResponse, CreateAgentSessionRequest, CreateAgentSessionResponse,
    InteractiveTurnStreamEvent,
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::agent_permission_request::{
    agent_permission_request_store, CreateAgentPermissionRequest, PermissionResolution,
};
use crate::daemon::ingest::{publish_interactive_turn_event, stream_events_from_registry};
use crate::daemon::state::AppState;
use crate::daemon::turn_stream_registry::{TurnStreamEntry, TurnStreamRegistryPortAdapter};
use medousa_engine::TurnStreamRegistryPort;

#[derive(Clone)]
struct LiveAgentSession {
    agent_session_id: String,
    session_id: String,
    runtime: String,
    acp_session_id: medousa_acp_client::AcpSessionId,
    cancelled: Arc<Mutex<bool>>,
}

#[derive(Default)]
struct AgentSessionRegistry {
    /// Medousa chat session_id → active agent session
    by_chat_session: HashMap<String, String>,
    by_agent_session: HashMap<String, LiveAgentSession>,
}

static AGENT_SESSIONS: once_cell::sync::Lazy<RwLock<AgentSessionRegistry>> =
    once_cell::sync::Lazy::new(|| RwLock::new(AgentSessionRegistry::default()));

static ACP_CLIENT: once_cell::sync::Lazy<ExternalAcpClient> =
    once_cell::sync::Lazy::new(ExternalAcpClient::new);

pub async fn list_agent_runtimes() -> Json<AgentRuntimeListResponse> {
    let kinds = [
        AgentRuntimeKind::Medousa,
        AgentRuntimeKind::Cursor,
        AgentRuntimeKind::Codex,
    ];
    let runtimes = kinds
        .into_iter()
        .map(|kind| {
            let (available, command, detail) = runtime_availability(kind);
            AgentRuntimeInfo {
                runtime: kind.as_str().to_string(),
                available,
                command,
                detail,
                uses_native_turns: matches!(kind, AgentRuntimeKind::Medousa),
            }
        })
        .collect();
    Json(AgentRuntimeListResponse { runtimes })
}

pub async fn create_agent_session(
    State(state): State<AppState>,
    Json(body): Json<CreateAgentSessionRequest>,
) -> Result<Json<CreateAgentSessionResponse>, (StatusCode, String)> {
    let session_id = body.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".into()));
    }
    let kind = AgentRuntimeKind::parse(&body.runtime).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            format!("unknown runtime '{}'", body.runtime),
        )
    })?;
    if matches!(kind, AgentRuntimeKind::Medousa) {
        return Err((
            StatusCode::BAD_REQUEST,
            "medousa runtime uses /v1/turns — pick cursor or codex for /v1/agents".into(),
        ));
    }

    {
        let guard = AGENT_SESSIONS.read().await;
        if guard.by_chat_session.contains_key(&session_id) {
            return Err((
                StatusCode::CONFLICT,
                format!("session '{session_id}' already has an active agent session"),
            ));
        }
    }

    let mut config = external_runtime_config(kind).map_err(|e| {
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;
    if let Some(cwd) = body.cwd.clone().filter(|s| !s.trim().is_empty()) {
        config.cwd = Some(cwd);
    }
    if let Some(command) = body.command.clone().filter(|s| !s.trim().is_empty()) {
        config.command = command;
    }
    if let Some(args) = body.args.clone() {
        config.args = args;
    }

    let acp_session = ACP_CLIENT.create_session(&config).await.map_err(|e| {
        (StatusCode::BAD_GATEWAY, format!("ACP create_session failed: {e}"))
    })?;

    let agent_session_id = format!("agent-{}", Uuid::new_v4());
    let adapter = TurnStreamRegistryPortAdapter::new(state.interactive_turn_streams.clone());
    if !adapter.register_stream(&agent_session_id).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to register agent stream".into(),
        ));
    }

    let live = LiveAgentSession {
        agent_session_id: agent_session_id.clone(),
        session_id: session_id.clone(),
        runtime: kind.as_str().to_string(),
        acp_session_id: acp_session.clone(),
        cancelled: Arc::new(Mutex::new(false)),
    };

    {
        let mut guard = AGENT_SESSIONS.write().await;
        guard
            .by_chat_session
            .insert(session_id.clone(), agent_session_id.clone());
        guard
            .by_agent_session
            .insert(agent_session_id.clone(), live.clone());
    }

    let accepted_at_utc = Utc::now();
    let stream_url = format!("/v1/agents/sessions/{agent_session_id}/stream");

    // Opening status on the stream
    if let Some(entry) = state.interactive_turn_streams.read().await.get(&agent_session_id) {
        publish_agent_event(
            entry,
            &agent_session_id,
            &session_id,
            kind.as_str(),
            "status",
            "accepted",
            &format!("agent session started ({})", kind.as_str()),
            false,
            None,
            None,
        );
    }

    if let Some(prompt) = body.prompt.filter(|p| !p.trim().is_empty()) {
        spawn_prompt_pump(
            state.clone(),
            live.clone(),
            prompt,
        );
    }

    Ok(Json(CreateAgentSessionResponse {
        agent_session_id,
        session_id,
        runtime: kind.as_str().to_string(),
        phase: "accepted".into(),
        stream_url,
        stream_ready: true,
        accepted_at_utc,
    }))
}

pub async fn prompt_agent_session(
    State(state): State<AppState>,
    AxumPath(agent_session_id): AxumPath<String>,
    Json(body): Json<AgentSessionPromptRequest>,
) -> Result<Json<AgentSessionPromptResponse>, (StatusCode, String)> {
    let prompt = body.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".into()));
    }
    let live = {
        let guard = AGENT_SESSIONS.read().await;
        guard
            .by_agent_session
            .get(agent_session_id.trim())
            .cloned()
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    format!("unknown agent session '{agent_session_id}'"),
                )
            })?
    };
    if *live.cancelled.lock().await {
        return Err((StatusCode::CONFLICT, "agent session cancelled".into()));
    }
    spawn_prompt_pump(state, live.clone(), prompt);
    Ok(Json(AgentSessionPromptResponse {
        accepted: true,
        agent_session_id: live.agent_session_id,
    }))
}

pub async fn cancel_agent_session(
    State(state): State<AppState>,
    AxumPath(agent_session_id): AxumPath<String>,
) -> Result<Json<CancelAgentSessionResponse>, (StatusCode, String)> {
    let agent_session_id = agent_session_id.trim().to_string();
    let live = {
        let mut guard = AGENT_SESSIONS.write().await;
        let live = guard
            .by_agent_session
            .remove(&agent_session_id)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    format!("unknown agent session '{agent_session_id}'"),
                )
            })?;
        guard.by_chat_session.remove(&live.session_id);
        live
    };
    *live.cancelled.lock().await = true;
    let _ = ACP_CLIENT.cancel(&live.acp_session_id).await;

    if let Some(entry) = state.interactive_turn_streams.read().await.get(&agent_session_id) {
        publish_agent_event(
            entry,
            &agent_session_id,
            &live.session_id,
            &live.runtime,
            "error",
            "cancelled",
            "agent session cancelled",
            true,
            None,
            None,
        );
        entry.channel.mark_closed();
    }

    Ok(Json(CancelAgentSessionResponse {
        cancelled: true,
        agent_session_id,
        message: "agent session cancelled".into(),
    }))
}

pub async fn agent_session_stream(
    State(state): State<AppState>,
    AxumPath(agent_session_id): AxumPath<String>,
    Query(query): Query<crate::daemon::ingest::StreamSinceQuery>,
) -> Result<Sse<impl Stream<Item = std::result::Result<axum::response::sse::Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(&registry, &agent_session_id, "agent session", query.since).await
}

pub async fn list_agent_permission_requests(
    Query(query): Query<AgentPermissionRequestListQuery>,
) -> Json<AgentPermissionRequestListResponse> {
    let limit = query.limit.unwrap_or(50);
    let pending_only = query
        .status
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("pending"))
        .unwrap_or(true);
    let requests = if pending_only {
        agent_permission_request_store().list_pending(limit)
    } else {
        agent_permission_request_store().list_all(limit)
    };
    Json(AgentPermissionRequestListResponse { requests })
}

pub async fn approve_agent_permission_request(
    AxumPath(request_id): AxumPath<String>,
    Json(body): Json<AgentPermissionResolveRequest>,
) -> Result<Json<AgentPermissionResolveResponse>, (StatusCode, String)> {
    let request = agent_permission_request_store()
        .approve(request_id.trim(), body.resolved_by)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(AgentPermissionResolveResponse { request }))
}

pub async fn deny_agent_permission_request(
    AxumPath(request_id): AxumPath<String>,
    Json(body): Json<AgentPermissionResolveRequest>,
) -> Result<Json<AgentPermissionResolveResponse>, (StatusCode, String)> {
    let request = agent_permission_request_store()
        .deny(request_id.trim(), body.resolved_by)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(AgentPermissionResolveResponse { request }))
}

fn spawn_prompt_pump(state: AppState, live: LiveAgentSession, prompt: String) {
    tokio::spawn(async move {
        if let Err(err) = run_prompt_pump(state, live, prompt).await {
            tracing::warn!(error = %err, "agent prompt pump failed");
        }
    });
}

async fn run_prompt_pump(
    state: AppState,
    live: LiveAgentSession,
    prompt: String,
) -> anyhow::Result<()> {
    let entry = state
        .interactive_turn_streams
        .read()
        .await
        .get(&live.agent_session_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("stream missing"))?;

    publish_agent_event(
        &entry,
        &live.agent_session_id,
        &live.session_id,
        &live.runtime,
        "status",
        "running",
        "prompt accepted",
        false,
        None,
        None,
    );

    ACP_CLIENT
        .prompt(&live.acp_session_id, &prompt)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut idle_empty = 0u32;
    loop {
        if *live.cancelled.lock().await {
            break;
        }
        let event = ACP_CLIENT.next_event(&live.acp_session_id).await?;
        let Some(event) = event else {
            idle_empty = idle_empty.saturating_add(1);
            if idle_empty > 250 {
                // ~10s of idle emptiness — treat as complete
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "done",
                    "completed",
                    "agent prompt complete (idle)",
                    true,
                    None,
                    None,
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            continue;
        };
        idle_empty = 0;
        match event {
            AcpEvent::MessageDelta { text } => {
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "content_delta",
                    "streaming",
                    "",
                    false,
                    Some(text),
                    None,
                );
            }
            AcpEvent::MessageDone { text } => {
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "assistant_message",
                    "streaming",
                    &text,
                    false,
                    None,
                    Some(text.clone()),
                );
            }
            AcpEvent::ToolCall { id, name, input } => {
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "tool_started",
                    "tool",
                    &format!("{name} ({id})"),
                    false,
                    None,
                    None,
                );
                let _ = input;
            }
            AcpEvent::PermissionRequest { id: _, summary } => {
                let record = agent_permission_request_store().create(CreateAgentPermissionRequest {
                    agent_session_id: live.agent_session_id.clone(),
                    session_id: live.session_id.clone(),
                    runtime: live.runtime.clone(),
                    summary: summary.clone(),
                });
                publish_permission_event(&entry, &live, &record.request_id, &summary);
                let resolution = agent_permission_request_store()
                    .wait_for_resolution(&record.request_id)
                    .await
                    .unwrap_or(PermissionResolution::Denied);
                let msg = match resolution {
                    PermissionResolution::Approved => "permission approved",
                    PermissionResolution::Denied => "permission denied",
                };
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "status",
                    "permission_resolved",
                    msg,
                    false,
                    None,
                    None,
                );
            }
            AcpEvent::Error { message } => {
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "error",
                    "error",
                    &message,
                    false,
                    None,
                    None,
                );
            }
            AcpEvent::Done => {
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "done",
                    "completed",
                    "agent prompt complete",
                    true,
                    None,
                    None,
                );
                break;
            }
        }
    }
    Ok(())
}

fn publish_permission_event(
    entry: &TurnStreamEntry,
    live: &LiveAgentSession,
    permission_request_id: &str,
    summary: &str,
) {
    let event = InteractiveTurnStreamEvent {
        turn_id: live.agent_session_id.clone(),
        seq: 0,
        event_type: "permission_request".into(),
        phase: "permission".into(),
        message: summary.to_string(),
        content_delta: None,
        reasoning_delta: None,
        final_text: None,
        tool_names: None,
        terminal: false,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_output_summary: None,
        tool_round: None,
        tool_artifact_refs: None,
        ui_artifact: None,
        previous_artifact_id: None,
        root_artifact_id: None,
        ui_scene: None,
        operator_message: Some(summary.to_string()),
        debug_message: None,
        browser_session_id: None,
        browser_challenge_url: None,
        context_usage: None,
        permission_request_id: Some(permission_request_id.to_string()),
        agent_session_id: Some(live.agent_session_id.clone()),
        agent_runtime: Some(live.runtime.clone()),
    };
    publish_interactive_turn_event(entry, Ok(event));
}

#[allow(clippy::too_many_arguments)]
fn publish_agent_event(
    entry: &TurnStreamEntry,
    agent_session_id: &str,
    _session_id: &str,
    runtime: &str,
    event_type: &str,
    phase: &str,
    message: &str,
    terminal: bool,
    content_delta: Option<String>,
    final_text: Option<String>,
) {
    let event = InteractiveTurnStreamEvent {
        turn_id: agent_session_id.to_string(),
        seq: 0,
        event_type: event_type.to_string(),
        phase: phase.to_string(),
        message: message.to_string(),
        content_delta,
        reasoning_delta: None,
        final_text,
        tool_names: None,
        terminal,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_output_summary: None,
        tool_round: None,
        tool_artifact_refs: None,
        ui_artifact: None,
        previous_artifact_id: None,
        root_artifact_id: None,
        ui_scene: None,
        operator_message: None,
        debug_message: None,
        browser_session_id: None,
        browser_challenge_url: None,
        context_usage: None,
        permission_request_id: None,
        agent_session_id: Some(agent_session_id.to_string()),
        agent_runtime: Some(runtime.to_string()),
    };
    publish_interactive_turn_event(entry, Ok(event));
}
