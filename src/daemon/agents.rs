//! Daemon `/v1/agents` — hot-swappable external agent runtimes (ACP via SDK).

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use chrono::Utc;
use medousa_acp_client::{
    AcpClient, AcpEvent, AgentRuntimeKind, ExternalAcpClient, RuntimeAuthStatus,
    external_runtime_config, runtime_auth_probe, runtime_availability,
};
use medousa_forge::model::WorkId;
use medousa_types::{
    AgentModeId, AgentPermissionRequestListQuery, AgentPermissionRequestListResponse,
    AgentPermissionResolveRequest, AgentPermissionResolveResponse, AgentRuntimeInfo,
    AgentRuntimeListResponse, AgentSecretDenyRequest, AgentSecretFulfillRequest,
    AgentSecretRequestListQuery, AgentSecretRequestListResponse, AgentSecretResolveResponse,
    AgentSessionConfigOption, AgentSessionPromptRequest, AgentSessionPromptResponse,
    CancelAgentSessionResponse, CodeIntentContext, CreateAgentSessionRequest,
    CreateAgentSessionResponse, InteractiveTurnStreamEvent, SetAgentSessionConfigOptionRequest,
    SetAgentSessionConfigOptionResponse,
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::agent_permission_request::{
    CreateAgentPermissionRequest, PermissionResolution, agent_permission_request_store,
};
use crate::agent_secret_request::agent_secret_request_store;
use crate::daemon::acp_forge_adapter;
use crate::daemon::ingest::{publish_interactive_turn_event, stream_events_from_registry};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::daemon::turn_stream_registry::{TurnStreamEntry, TurnStreamRegistryPortAdapter};
use crate::runtime::agent_platform::{AcpTerminalKind, publish_acp_terminal};
use crate::semantic_values::{RequiredContent, TrimmedText};
use medousa_engine::TurnStreamRegistryPort;
use serde_json::json;

#[derive(Clone)]
struct LiveAgentSession {
    agent_session_id: String,
    session_id: String,
    runtime: String,
    acp_session_id: medousa_acp_client::AcpSessionId,
    /// ACP wire `sessionId` (from session/new or session/resume) — stashed on
    /// interrupt as `ResumeSupported`. Distinct from the Medousa handle above.
    acp_wire_session_id: Option<String>,
    cancelled: Arc<Mutex<bool>>,
    /// Forge undertaking this session is bound to (governed cwd + leases).
    forge_work_id: Option<WorkId>,
    forge_lease: Option<medousa_forge::model::ExecutionLease>,
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

pub fn permission_surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(
            permission_policy(
                axum::http::Method::GET,
                "/v1/agents/permission-requests",
                1024,
            ),
            get(list_agent_permission_requests),
        )
        .route(
            permission_policy(
                axum::http::Method::POST,
                "/v1/agents/permission-requests/{request_id}/approve",
                16 * 1024,
            ),
            post(approve_agent_permission_request),
        )
        .route(
            permission_policy(
                axum::http::Method::POST,
                "/v1/agents/permission-requests/{request_id}/deny",
                16 * 1024,
            ),
            post(deny_agent_permission_request),
        )
        .route(
            permission_policy(axum::http::Method::GET, "/v1/agents/secret-requests", 1024),
            get(list_agent_secret_requests),
        )
        .route(
            permission_policy(
                axum::http::Method::POST,
                "/v1/agents/secret-requests/{request_id}/fulfill",
                20 * 1024,
            ),
            post(fulfill_agent_secret_request),
        )
        .route(
            permission_policy(
                axum::http::Method::POST,
                "/v1/agents/secret-requests/{request_id}/deny",
                4 * 1024,
            ),
            post(deny_agent_secret_request),
        )
}

fn permission_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(crate::request_principal::Capability::AdminExecute),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::Administration,
    }
}

#[derive(Debug)]
struct CreateAgentSessionCommand {
    session_id: TrimmedText,
    runtime: AgentRuntimeKind,
    prompt: Option<RequiredContent>,
    cwd: Option<TrimmedText>,
    command: Option<TrimmedText>,
    args: Option<Vec<String>>,
    work_id: Option<TrimmedText>,
    resume_provider_token: Option<TrimmedText>,
    code_context: Option<CodeIntentContext>,
}

impl TryFrom<CreateAgentSessionRequest> for CreateAgentSessionCommand {
    type Error = String;

    fn try_from(input: CreateAgentSessionRequest) -> Result<Self, Self::Error> {
        let session_id =
            TrimmedText::new(input.session_id).map_err(|_| "session_id is required".to_string())?;
        let runtime = AgentRuntimeKind::parse(input.runtime.trim())
            .ok_or_else(|| format!("unknown runtime '{}'", input.runtime.trim()))?;
        let prompt = input
            .prompt
            .and_then(|value| RequiredContent::new(value).ok());
        Ok(Self {
            session_id,
            runtime,
            prompt,
            cwd: input.cwd.and_then(|value| TrimmedText::new(value).ok()),
            command: input.command.and_then(|value| TrimmedText::new(value).ok()),
            args: input.args,
            work_id: input.work_id.and_then(|value| TrimmedText::new(value).ok()),
            resume_provider_token: input
                .resume_provider_token
                .and_then(|value| TrimmedText::new(value).ok()),
            code_context: input.code_context,
        })
    }
}

fn coder_session_missing_project(mode: AgentModeId, has_work_id: bool) -> bool {
    mode == AgentModeId::Coder && !has_work_id
}

#[derive(Debug)]
struct AgentSessionPromptCommand {
    prompt: RequiredContent,
    code_context: Option<CodeIntentContext>,
}

impl TryFrom<AgentSessionPromptRequest> for AgentSessionPromptCommand {
    type Error = String;

    fn try_from(input: AgentSessionPromptRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            prompt: RequiredContent::new(input.prompt)
                .map_err(|_| "prompt is required".to_string())?,
            code_context: input.code_context,
        })
    }
}

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
            let probe = runtime_auth_probe(kind);
            let auth_status = match probe.status {
                RuntimeAuthStatus::SignedIn => Some("signed_in".to_string()),
                RuntimeAuthStatus::SignedOut => Some("signed_out".to_string()),
                RuntimeAuthStatus::Unknown => Some("unknown".to_string()),
            };
            AgentRuntimeInfo {
                runtime: kind.as_str().to_string(),
                available,
                command,
                detail,
                uses_native_turns: matches!(kind, AgentRuntimeKind::Medousa),
                auth_status,
                binary_present: probe.binary_present,
                auth_detail: probe.detail,
            }
        })
        .collect();
    Json(AgentRuntimeListResponse { runtimes })
}

pub async fn create_agent_session(
    State(state): State<AppState>,
    Json(body): Json<CreateAgentSessionRequest>,
) -> Result<Json<CreateAgentSessionResponse>, (StatusCode, String)> {
    let command = CreateAgentSessionCommand::try_from(body)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let session_id = command.session_id.into_string();
    let kind = command.runtime;
    if matches!(kind, AgentRuntimeKind::Medousa) {
        return Err((
            StatusCode::BAD_REQUEST,
            "medousa runtime uses /v1/turns — pick cursor or codex for /v1/agents".into(),
        ));
    }

    let session_mode = crate::agent_mode_state::get_session_mode(&session_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    if coder_session_missing_project(session_mode.effective_mode, command.work_id.is_some()) {
        return Err((
            StatusCode::CONFLICT,
            "choose or create a project before starting Cursor or Codex in Coder mode".into(),
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

    let mut config =
        external_runtime_config(kind).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if let Some(cwd) = command.cwd.as_ref().map(TrimmedText::as_str) {
        config.cwd = Some(cwd.to_string());
    }
    if let Some(command) = command.command.as_ref().map(TrimmedText::as_str) {
        config.command = command.to_string();
    }
    if let Some(args) = command.args.clone() {
        config.args = args;
    }

    // Fail before taking Forge custody when the provider cannot start.
    let probe = runtime_auth_probe(kind);
    if probe.binary_present && matches!(probe.status, RuntimeAuthStatus::SignedOut) {
        let hint = probe
            .detail
            .unwrap_or_else(|| "vendor CLI not signed in".into());
        return Err((
            StatusCode::UNAUTHORIZED,
            format!("{hint} — sign in from Settings → Connections"),
        ));
    }

    let agent_session_id = format!("agent-{}", Uuid::new_v4());
    let mut forge_lease = None;

    // Forge undertaking binding: acquire the isolated lease before provider
    // creation so the provider process starts in the lease-owned worktree.
    let forge_work_id = if let Some(work_id_raw) = command.work_id.as_ref() {
        let work_id = WorkId::from(work_id_raw.as_str().to_string());
        let item = state.forge.load(&work_id).map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("forge work '{work_id_raw}' not found: {e}"),
            )
        })?;
        if !matches!(
            item.state,
            medousa_forge::model::WorkState::Ready | medousa_forge::model::WorkState::Executing
        ) {
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "forge work '{work_id_raw}' is {} — provision it before starting an agent",
                    item.state
                ),
            ));
        }
        let executor = medousa_forge::model::ExecutorDescriptor {
            kind: format!("acp-{}", kind.as_str()),
            detail: json!({
                "agent_session_id": agent_session_id,
                "chat_session_id": session_id,
                "runtime": kind.as_str(),
                "phase": "provider_starting",
            }),
        };
        let (item, lease) = state
            .forge
            .begin_isolated_attempt(
                &work_id,
                executor,
                None,
                &medousa_forge::forge::Forge::system_actor(),
            )
            .map_err(|error| (StatusCode::CONFLICT, error.to_string()))?;
        let env = match item.environment_for_attempt(&lease.attempt_id).cloned() {
            Some(environment) => environment,
            None => {
                let _ = state.forge.interrupt_attempt(
                    &lease,
                    medousa_forge::model::RecoveryDisposition::RestartAllowed,
                    &medousa_forge::forge::Forge::system_actor(),
                );
                return Err((
                    StatusCode::CONFLICT,
                    format!("forge attempt for '{work_id_raw}' has no isolated environment"),
                ));
            }
        };
        if !env.worktree.exists() {
            let _ = state.forge.interrupt_attempt(
                &lease,
                medousa_forge::model::RecoveryDisposition::RestartAllowed,
                &medousa_forge::forge::Forge::system_actor(),
            );
            return Err((
                StatusCode::CONFLICT,
                format!(
                    "forge worktree missing for '{work_id_raw}': {}",
                    env.worktree.display()
                ),
            ));
        }
        config.cwd = Some(env.worktree.to_string_lossy().into_owned());
        forge_lease = Some(lease);
        Some(work_id)
    } else {
        None
    };

    // Resume token: explicit request wins; otherwise look up the latest
    // ResumeSupported token on the bound work item.
    let resume_token = {
        let explicit = command
            .resume_provider_token
            .as_ref()
            .map(TrimmedText::as_str)
            .map(str::to_string);
        if explicit.is_some() {
            explicit
        } else if let Some(ref work_id) = forge_work_id {
            state.forge.latest_resume_token(work_id).ok().flatten()
        } else {
            None
        }
    };

    let (acp_session, acp_wire_session_id, resumed) = ACP_CLIENT
        .create_or_resume_session(&config, resume_token.as_deref())
        .await
        .map_err(|e| {
            if let Some(lease) = forge_lease.as_ref() {
                let _ = state.forge.fail_attempt(
                    lease,
                    "ACP provider session could not start",
                    &medousa_forge::forge::Forge::system_actor(),
                );
            }
            let message = e.to_string();
            let lower = message.to_lowercase();
            if lower.contains("auth")
                || lower.contains("login")
                || lower.contains("unauthorized")
                || lower.contains("401")
            {
                return (
                    StatusCode::UNAUTHORIZED,
                    format!(
                        "{} sign-in expired or missing — sign in from Settings → Connections",
                        kind.as_str()
                    ),
                );
            }
            (
                StatusCode::BAD_GATEWAY,
                format!("ACP create_or_resume_session failed: {e}"),
            )
        })?;
    let config_options =
        parse_config_options(ACP_CLIENT.session_config_options(&acp_session).await);

    let adapter = TurnStreamRegistryPortAdapter::new(state.interactive_turn_streams.clone());
    if !adapter.register_stream(&agent_session_id).await {
        if let Some(lease) = forge_lease.as_ref() {
            let _ = state.forge.interrupt_attempt(
                lease,
                medousa_forge::model::RecoveryDisposition::RestartAllowed,
                &medousa_forge::forge::Forge::system_actor(),
            );
        }
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
        acp_wire_session_id: acp_wire_session_id.clone(),
        cancelled: Arc::new(Mutex::new(false)),
        forge_work_id: forge_work_id.clone(),
        forge_lease,
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
    if let Some(entry) = state
        .interactive_turn_streams
        .read()
        .await
        .get(&agent_session_id)
    {
        publish_agent_event(
            entry,
            &agent_session_id,
            &session_id,
            kind.as_str(),
            "status",
            "accepted",
            "connected",
            false,
            None,
            None,
        );
    }

    if let Some(prompt) = command.prompt.map(RequiredContent::into_string) {
        spawn_prompt_pump(
            state.clone(),
            live.clone(),
            prompt_with_code_context(prompt, command.code_context.as_ref()),
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
        work_id: forge_work_id.map(|id| id.to_string()),
        resumed: Some(resumed),
        config_options,
    }))
}

fn parse_config_options(values: Vec<serde_json::Value>) -> Vec<AgentSessionConfigOption> {
    values
        .into_iter()
        .filter_map(|value| match serde_json::from_value(value) {
            Ok(option) => Some(option),
            Err(error) => {
                tracing::warn!(%error, "ignoring malformed ACP session config option");
                None
            }
        })
        .collect()
}

pub async fn set_agent_session_config_option(
    AxumPath(agent_session_id): AxumPath<String>,
    Json(body): Json<SetAgentSessionConfigOptionRequest>,
) -> Result<Json<SetAgentSessionConfigOptionResponse>, (StatusCode, String)> {
    let config_id = TrimmedText::new(body.config_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "config_id is required".into()))?;
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
    let values = ACP_CLIENT
        .set_session_config_option(&live.acp_session_id, config_id.as_str(), body.value)
        .await
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(SetAgentSessionConfigOptionResponse {
        agent_session_id: live.agent_session_id,
        config_options: parse_config_options(values),
    }))
}

pub async fn prompt_agent_session(
    State(state): State<AppState>,
    AxumPath(agent_session_id): AxumPath<String>,
    Json(body): Json<AgentSessionPromptRequest>,
) -> Result<Json<AgentSessionPromptResponse>, (StatusCode, String)> {
    let command = AgentSessionPromptCommand::try_from(body)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let prompt = command.prompt.into_string();
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
    spawn_prompt_pump(
        state,
        live.clone(),
        prompt_with_code_context(prompt, command.code_context.as_ref()),
    );
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

    // Interrupt the Forge attempt (if bound) — stash the ACP *wire* session id
    // so a future session/resume can reattach instead of restarting.
    if let Some(lease) = live.forge_lease.clone() {
        let adapter = acp_forge_adapter::AcpForgeAdapter::new(&state.forge);
        let wire = live
            .acp_wire_session_id
            .as_deref()
            .or(Some(live.acp_session_id.0.as_str()));
        match adapter.interrupt_attempt(&lease, "agent session cancelled", wire) {
            Ok(_) => {
                update_registry_forge(&agent_session_id, live.forge_work_id.clone(), None).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, "forge interrupt_attempt on cancel");
            }
        }
    }

    if let Some(entry) = state
        .interactive_turn_streams
        .read()
        .await
        .get(&agent_session_id)
    {
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

    publish_acp_terminal(
        AcpTerminalKind::Cancelled,
        &live.session_id,
        Some(&agent_session_id),
        &agent_session_id,
        &live.runtime,
        "agent session cancelled",
        json!({}),
    )
    .await;

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
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(
        &registry,
        &agent_session_id,
        "agent session",
        query.since,
        &headers,
    )
    .await
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

pub async fn list_agent_secret_requests(
    Query(query): Query<AgentSecretRequestListQuery>,
) -> Json<AgentSecretRequestListResponse> {
    let limit = query.limit.unwrap_or(50);
    let pending_only = query
        .status
        .as_deref()
        .map(|status| status.eq_ignore_ascii_case("pending"))
        .unwrap_or(true);
    let requests = if pending_only {
        agent_secret_request_store().list_pending(limit)
    } else {
        agent_secret_request_store().list_all(limit)
    };
    Json(AgentSecretRequestListResponse { requests })
}

pub async fn fulfill_agent_secret_request(
    AxumPath(request_id): AxumPath<String>,
    Json(body): Json<AgentSecretFulfillRequest>,
) -> Result<Json<AgentSecretResolveResponse>, (StatusCode, String)> {
    let request = agent_secret_request_store()
        .fulfill(request_id.trim(), body.value, body.resolved_by)
        .await
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok(Json(AgentSecretResolveResponse { request }))
}

pub async fn deny_agent_secret_request(
    AxumPath(request_id): AxumPath<String>,
    Json(body): Json<AgentSecretDenyRequest>,
) -> Result<Json<AgentSecretResolveResponse>, (StatusCode, String)> {
    let request = agent_secret_request_store()
        .deny(request_id.trim(), body.resolved_by)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok(Json(AgentSecretResolveResponse { request }))
}

fn spawn_prompt_pump(state: AppState, live: LiveAgentSession, prompt: String) {
    tokio::spawn(async move {
        let fail_lease = live.forge_lease.clone();
        if let Err(err) = run_prompt_pump(state.clone(), live.clone(), prompt).await {
            tracing::warn!(error = %err, "agent prompt pump failed");
            if let (Some(work_id), Some(lease)) = (live.forge_work_id.clone(), fail_lease) {
                let adapter = acp_forge_adapter::AcpForgeAdapter::new(&state.forge);
                match adapter.fail_attempt(&lease, &err.to_string()) {
                    Ok(_) => {
                        update_registry_forge(&live.agent_session_id, Some(work_id), None).await;
                    }
                    Err(ferr) => {
                        tracing::warn!(error = %ferr, "forge fail_attempt after pump error");
                    }
                }
            }
            // Surface the failure into the chat stream so the turn fails loudly
            // instead of dying silently (or looking like a stub/no-op).
            publish_agent_pump_error(&state, &live, &err.to_string()).await;
        }
    });
}

fn prompt_with_code_context(prompt: String, context: Option<&CodeIntentContext>) -> String {
    let Some(context) = context else {
        return prompt;
    };
    let mut lines = Vec::new();
    if let Some(value) = context
        .project_title
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        lines.push(format!("Project: {}", bounded_trimmed(value, 500)));
    }
    if let Some(value) = context.outcome.as_deref().filter(|v| !v.trim().is_empty()) {
        lines.push(format!(
            "Intended outcome: {}",
            bounded_trimmed(value, 4_000)
        ));
    }
    if let Some(path) = context
        .active_path
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        let path = bounded_trimmed(path, 1_000);
        let line = context.selection_start_line.or(context.cursor_line);
        let end = context.selection_end_line.filter(|end| Some(*end) != line);
        let location = match (line, end) {
            (Some(start), Some(end)) => format!("{path}:{start}-{end}"),
            (Some(start), None) => format!("{path}:{start}"),
            _ => path,
        };
        lines.push(format!("Current location: {location}"));
    }
    if let Some(value) = context
        .containing_symbol
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        lines.push(format!(
            "Containing symbol: {}",
            bounded_trimmed(value, 500)
        ));
    }
    let open_files = context
        .open_files
        .iter()
        .filter(|v| !v.trim().is_empty())
        .take(12)
        .map(|v| bounded_trimmed(v, 1_000))
        .collect::<Vec<_>>();
    if !open_files.is_empty() {
        lines.push(format!("Open files: {}", open_files.join(", ")));
    }
    let diagnostics = context
        .diagnostics
        .iter()
        .filter(|v| !v.trim().is_empty())
        .take(20)
        .map(|v| bounded_trimmed(v, 1_000))
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        lines.push(format!("Relevant issues:\n- {}", diagnostics.join("\n- ")));
    }
    if let Some(value) = context
        .last_verification
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        lines.push(format!(
            "Last project check: {}",
            bounded_trimmed(value, 2_000)
        ));
    }
    if let Some(value) = context.selected_text.as_deref().filter(|v| !v.is_empty()) {
        let bounded: String = value.chars().take(16_000).collect();
        lines.push(format!("Selected code:\n```\n{bounded}\n```"));
    }
    if lines.is_empty() {
        return prompt;
    }
    format!(
        "{prompt}\n\n<medousa_code_context>\n{}\n</medousa_code_context>",
        lines.join("\n")
    )
}

fn bounded_trimmed(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

/// Clone the live session out of the registry, mutate its forge binding, and
/// write it back under the write lock.
async fn update_registry_forge(
    agent_session_id: &str,
    work_id: Option<medousa_forge::model::WorkId>,
    lease: Option<medousa_forge::model::ExecutionLease>,
) {
    let mut guard = AGENT_SESSIONS.write().await;
    if let Some(live) = guard.by_agent_session.get_mut(agent_session_id) {
        live.forge_work_id = work_id;
        live.forge_lease = lease;
    }
}

async fn publish_agent_pump_error(state: &AppState, live: &LiveAgentSession, message: &str) {
    let entry = {
        state
            .interactive_turn_streams
            .read()
            .await
            .get(&live.agent_session_id)
            .cloned()
    };
    if let Some(entry) = entry {
        publish_agent_event(
            &entry,
            &live.agent_session_id,
            &live.session_id,
            &live.runtime,
            "error",
            "error",
            message,
            true,
            None,
            None,
        );
        entry.channel.mark_closed();
    }
    publish_acp_terminal(
        AcpTerminalKind::Failed,
        &live.session_id,
        Some(&live.agent_session_id),
        &live.agent_session_id,
        &live.runtime,
        message,
        json!({ "error": message }),
    )
    .await;
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
        "working",
        false,
        None,
        None,
    );

    // Forge lease begins on the first prompt of a bound session.
    let mut live = live;
    if let (Some(work_id), None) = (live.forge_work_id.clone(), live.forge_lease.clone()) {
        let adapter = acp_forge_adapter::AcpForgeAdapter::new(&state.forge);
        let wire = live
            .acp_wire_session_id
            .clone()
            .unwrap_or_else(|| live.acp_session_id.0.clone());
        let ctx = acp_forge_adapter::AcpForgeContext {
            agent_session_id: &live.agent_session_id,
            acp_session_id: &wire,
            chat_session_id: &live.session_id,
            runtime: &live.runtime,
            pid: None,
        };
        match adapter.begin_attempt(&work_id, &ctx) {
            Ok((_item, lease)) => {
                live.forge_lease = Some(lease.clone());
                update_registry_forge(&live.agent_session_id, Some(work_id), Some(lease)).await;
            }
            Err(err) => {
                tracing::warn!(error = %err, work_id = %work_id, "forge begin_attempt failed");
                publish_agent_event(
                    &entry,
                    &live.agent_session_id,
                    &live.session_id,
                    &live.runtime,
                    "error",
                    "error",
                    &format!("forge lease begin failed: {err}"),
                    true,
                    None,
                    None,
                );
                entry.channel.mark_closed();
                return Err(anyhow::anyhow!("forge begin_attempt: {err}"));
            }
        }
    }
    let forge_adapter = live
        .forge_work_id
        .as_ref()
        .map(|_| acp_forge_adapter::AcpForgeAdapter::new(&state.forge));
    let mut last_heartbeat = std::time::Instant::now();

    if let (Some(adapter), Some(lease)) = (forge_adapter.as_ref(), live.forge_lease.as_ref()) {
        let _ = adapter.record_prompt(lease, prompt.len());
    }

    // Durable transcript (Synara/T3 reopen gap). SSE path unchanged.
    crate::daemon::acp_turn_persist::persist_user_prompt(&live.session_id, &prompt).await?;
    let mut persist = crate::daemon::acp_turn_persist::AcpPromptPersistState::new();

    ACP_CLIENT
        .prompt(&live.acp_session_id, &prompt)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut idle_empty = 0u32;
    loop {
        if *live.cancelled.lock().await {
            break;
        }
        if let (Some(adapter), Some(lease)) = (forge_adapter.as_ref(), live.forge_lease.as_ref())
            && last_heartbeat.elapsed() >= std::time::Duration::from_secs(15)
        {
            if let Err(err) = adapter.heartbeat(lease) {
                tracing::warn!(error = %err, "forge heartbeat failed");
            } else {
                last_heartbeat = std::time::Instant::now();
            }
        }
        let event = ACP_CLIENT.next_event(&live.acp_session_id).await?;
        let Some(event) = event else {
            idle_empty = idle_empty.saturating_add(1);
            if idle_empty > 250 {
                // ~10s of idle emptiness — treat as complete
                crate::daemon::acp_turn_persist::persist_assistant_if_needed(
                    &live.session_id,
                    &mut persist,
                    None,
                )
                .await?;
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
                publish_acp_terminal(
                    AcpTerminalKind::Completed,
                    &live.session_id,
                    Some(&live.agent_session_id),
                    &live.agent_session_id,
                    &live.runtime,
                    "agent prompt complete (idle)",
                    json!({ "reason": "idle" }),
                )
                .await;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            continue;
        };
        idle_empty = 0;
        persist.observe(&event);
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
            AcpEvent::ReasoningDelta { text } => {
                publish_agent_reasoning_event(&entry, &live.agent_session_id, &live.runtime, text);
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
                if let (Some(adapter), Some(lease)) =
                    (forge_adapter.as_ref(), live.forge_lease.as_ref())
                {
                    let _ = adapter.record_tool(lease, &name, &id);
                }
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
            AcpEvent::PermissionRequest { id, summary } => {
                let record =
                    agent_permission_request_store().create(CreateAgentPermissionRequest {
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
                let approved = matches!(resolution, PermissionResolution::Approved);
                if let Err(err) = ACP_CLIENT
                    .respond_permission(&live.acp_session_id, &id, approved)
                    .await
                {
                    tracing::warn!(
                        error = %err,
                        acp_permission_id = %id,
                        "failed to reply to ACP permission request"
                    );
                }
                let msg = if approved {
                    "permission approved"
                } else {
                    "permission denied"
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
                crate::daemon::acp_turn_persist::persist_assistant_if_needed(
                    &live.session_id,
                    &mut persist,
                    Some("error"),
                )
                .await?;
                if let (Some(adapter), Some(lease)) =
                    (forge_adapter.as_ref(), live.forge_lease.as_ref())
                {
                    match adapter.fail_attempt(lease, &message) {
                        Ok(_) => {
                            update_registry_forge(
                                &live.agent_session_id,
                                live.forge_work_id.clone(),
                                None,
                            )
                            .await;
                            live.forge_lease = None;
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "forge fail_attempt on ACP error");
                        }
                    }
                }
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
                publish_acp_terminal(
                    AcpTerminalKind::Failed,
                    &live.session_id,
                    Some(&live.agent_session_id),
                    &live.agent_session_id,
                    &live.runtime,
                    &message,
                    json!({ "error": message }),
                )
                .await;
            }
            AcpEvent::Done => {
                // Done is *not* a seal — lease stays live; heartbeat only.
                crate::daemon::acp_turn_persist::persist_assistant_if_needed(
                    &live.session_id,
                    &mut persist,
                    None,
                )
                .await?;
                if let (Some(adapter), Some(lease)) =
                    (forge_adapter.as_ref(), live.forge_lease.as_ref())
                    && let Err(err) = adapter.heartbeat(lease)
                {
                    tracing::warn!(error = %err, "forge heartbeat on Done");
                }
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
                publish_acp_terminal(
                    AcpTerminalKind::Completed,
                    &live.session_id,
                    Some(&live.agent_session_id),
                    &live.agent_session_id,
                    &live.runtime,
                    "agent prompt complete",
                    json!({}),
                )
                .await;
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
        response_provider: None,
        response_model: None,
        terminal: false,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_input_params: None,
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
        secret_request_id: None,
        secret_label: None,
        secret_provider_type: None,
        secret_credential_key: None,
        secret_backend: None,
        secret_allowed_hosts: None,
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
        response_provider: None,
        response_model: None,
        terminal,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_input_params: None,
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
        secret_request_id: None,
        secret_label: None,
        secret_provider_type: None,
        secret_credential_key: None,
        secret_backend: None,
        secret_allowed_hosts: None,
    };
    publish_interactive_turn_event(entry, Ok(event));
}

/// Forward an ACP thinking/reasoning trace chunk into the chat stream's
/// `reasoning_delta` channel so Home renders it in the collapsed thinking tray.
fn publish_agent_reasoning_event(
    entry: &TurnStreamEntry,
    agent_session_id: &str,
    runtime: &str,
    text: String,
) {
    let event = InteractiveTurnStreamEvent {
        turn_id: agent_session_id.to_string(),
        seq: 0,
        event_type: "reasoning_delta".into(),
        phase: "thinking".into(),
        message: String::new(),
        content_delta: None,
        reasoning_delta: Some(text),
        final_text: None,
        tool_names: None,
        response_provider: None,
        response_model: None,
        terminal: false,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_input_params: None,
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
        secret_request_id: None,
        secret_label: None,
        secret_provider_type: None,
        secret_credential_key: None,
        secret_backend: None,
        secret_allowed_hosts: None,
    };
    publish_interactive_turn_event(entry, Ok(event));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_inventory_requires_execution_administration() {
        let entries = permission_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 6);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Administration
                && entry.required_capability == Some("admin.execute")
                && entry.rate_limit_class == RateLimitClass::Administration
        }));
    }

    #[test]
    fn code_context_is_structured_and_bounded() {
        let context = CodeIntentContext {
            project_title: Some("Keep the engineer in flow".into()),
            outcome: Some("Preserve intent across handoff".into()),
            active_path: Some("src/flow.rs".into()),
            cursor_line: Some(14),
            selection_start_line: Some(12),
            selection_end_line: Some(15),
            selected_text: Some("fn handoff() {}".into()),
            open_files: vec!["src/flow.rs".into(), "src/lib.rs".into()],
            diagnostics: vec!["line 14: unused result".into()],
            ..CodeIntentContext::default()
        };

        let prompt = prompt_with_code_context("Fix this".into(), Some(&context));
        assert!(prompt.starts_with("Fix this\n\n<medousa_code_context>"));
        assert!(prompt.contains("Current location: src/flow.rs:12-15"));
        assert!(prompt.contains("Open files: src/flow.rs, src/lib.rs"));
        assert!(prompt.contains("Relevant issues:\n- line 14: unused result"));
        assert!(prompt.ends_with("</medousa_code_context>"));
    }

    #[test]
    fn empty_code_context_does_not_change_prompt() {
        assert_eq!(
            prompt_with_code_context("Keep going".into(), Some(&CodeIntentContext::default())),
            "Keep going"
        );
    }

    #[test]
    fn agent_commands_normalize_session_and_runtime_identifiers() {
        let raw_prompt = "  Fix the provider handoff.  \n";
        let command = CreateAgentSessionCommand::try_from(CreateAgentSessionRequest {
            session_id: " chat-1 ".to_string(),
            runtime: " codex ".to_string(),
            prompt: Some(raw_prompt.to_string()),
            cwd: Some(" /worktree ".to_string()),
            command: Some(" codex-cli ".to_string()),
            args: Some(vec!["--resume".to_string()]),
            surface: None,
            work_id: Some(" work-1 ".to_string()),
            resume_provider_token: Some(" token-1 ".to_string()),
            code_context: Some(CodeIntentContext::default()),
        })
        .expect("agent session command");
        assert_eq!(command.session_id.as_str(), "chat-1");
        assert_eq!(command.runtime.as_str(), "codex");
        assert_eq!(
            command.prompt.as_ref().map(RequiredContent::as_str),
            Some(raw_prompt)
        );
        assert_eq!(
            command.cwd.as_ref().map(TrimmedText::as_str),
            Some("/worktree")
        );
        assert_eq!(
            command.work_id.as_ref().map(TrimmedText::as_str),
            Some("work-1")
        );

        let prompt = AgentSessionPromptCommand::try_from(AgentSessionPromptRequest {
            prompt: raw_prompt.to_string(),
            code_context: None,
        })
        .expect("prompt command");
        assert_eq!(prompt.prompt.as_str(), raw_prompt);
    }

    #[test]
    fn agent_commands_reject_blank_required_values() {
        let runtime_error = CreateAgentSessionCommand::try_from(CreateAgentSessionRequest {
            session_id: "chat-1".to_string(),
            runtime: "unknown".to_string(),
            prompt: None,
            cwd: None,
            command: None,
            args: None,
            surface: None,
            work_id: None,
            resume_provider_token: None,
            code_context: None,
        })
        .expect_err("unknown runtime should fail");
        assert!(runtime_error.contains("unknown runtime"));

        let prompt_error = AgentSessionPromptCommand::try_from(AgentSessionPromptRequest {
            prompt: " \n\t".to_string(),
            code_context: None,
        })
        .expect_err("blank prompt should fail");
        assert_eq!(prompt_error, "prompt is required");
    }

    #[test]
    fn coder_external_agents_require_a_project_binding() {
        assert!(coder_session_missing_project(AgentModeId::Coder, false));
        assert!(!coder_session_missing_project(AgentModeId::Coder, true));
        assert!(!coder_session_missing_project(AgentModeId::General, false));
    }
}
