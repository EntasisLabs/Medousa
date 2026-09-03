//! Interactive turns, turn tickets, and session active-turn handlers.

use std::sync::Arc;

use medousa_engine::{
    Principal, TurnEnvelope, TurnLifecyclePorts, TurnStreamRegistryPort, run_turn,
};

use axum::Json;
use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::channel_delivery;
use crate::daemon::ingest::{
    publish_interactive_turn_event, record_job_delivery_pending, resolve_api_model_routing,
    resolve_session_runtime_config, stream_events_from_registry,
};
use crate::daemon_api::{
    CreateTurnTicketRequest, InteractiveTurnRequest, InteractiveTurnResponse,
    SessionActiveTurnsResponse, SessionDeleteQuery, SessionDeleteResponse, TurnTicketRecord,
    TurnTicketResponse,
};
use axum::response::Response;

use crate::daemon::state::AppState;

fn ticket_record_from_ticket(ticket: &crate::turn_ticket::TurnTicket) -> TurnTicketRecord {
    TurnTicketRecord {
        turn_id: ticket.turn_id.clone(),
        session_id: ticket.session_id.clone(),
        mode: ticket.mode,
        phase: ticket.phase,
        stream_url: ticket.stream_url.clone(),
        prompt_preview: ticket.prompt_preview.clone(),
        workspace_card_id: ticket.workspace_card_id.clone(),
        composer_handoff: ticket.composer_handoff(),
        started_at: ticket.started_at,
        updated_at: ticket.updated_at,
    }
}

pub fn build_interactive_request_from_ticket(
    request: &CreateTurnTicketRequest,
    provider: String,
    model: String,
    stage_routing: crate::stage_routing::StageRoutingMatrix,
) -> InteractiveTurnRequest {
    InteractiveTurnRequest {
        session_id: request.session_id.clone(),
        prompt: request.prompt.clone(),
        agent_mode: request.agent_mode,
        code_context: request.code_context.clone(),
        code_project_setup_authorized: request.code_project_setup_authorized,
        persist_user_turn: request.persist_user_turn,
        response_depth_mode: request.response_depth_mode.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        provider,
        model,
        stage_routing,
        surface: request.surface.clone(),
        host_context: request.host_context.clone(),
        max_tool_rounds: None,
        retry_runtime_max_rounds: None,
        manuscript_id: request.manuscript_id.clone(),
        additional_manuscript_ids: request.additional_manuscript_ids.clone(),
        suggested_capability_ids: request.suggested_capability_ids.clone(),
        scheduled_tool_allowlist: None,
        voice_preset_id: request.voice_preset_id.clone(),
        voice_appendix: request.voice_appendix.clone(),
        media_refs: request.media_refs.clone(),
        identity_user_id: request.identity_user_id.clone(),
    }
}

pub async fn spawn_turn_ticket(
    state: &AppState,
    principal: crate::request_principal::RequestPrincipal,
    turn_id: String,
    mode: crate::turn_ticket::TurnTicketMode,
    interactive_request: InteractiveTurnRequest,
    workspace_card_id: Option<String>,
) -> Result<TurnTicketResponse, (StatusCode, String)> {
    let session_id = crate::session_storage::SessionId::parse(&interactive_request.session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let admission = crate::session_deletion::acquire_mutation(&session_id)
        .map_err(|error| (StatusCode::CONFLICT, error))?;
    let session_id_text = session_id.to_string();
    let delivery_target =
        channel_delivery::delivery_target_from_interactive_turn(&interactive_request, &turn_id);
    let continuation_scope = crate::turn_continuation::TurnContinuationScope {
        turn_correlation_id: turn_id.clone(),
        session_id: interactive_request.session_id.clone(),
        identity_user_id: Some(
            interactive_request
                .identity_user_id
                .clone()
                .unwrap_or_else(|| state.workshop_identity_user_id()),
        ),
        original_prompt: interactive_request.prompt.clone(),
        delivery_target: Some(delivery_target.clone()),
        provider: interactive_request.provider.clone(),
        model: interactive_request.model.clone(),
        response_depth_mode: interactive_request.response_depth_mode.clone(),
        supports_ui_artifacts: crate::ui_present_tools::surface_supports_ui_artifacts(
            interactive_request.surface.as_ref(),
        ),
        supports_liquid_markdown: interactive_request
            .surface
            .as_ref()
            .is_some_and(|surface| surface.supports_liquid_markdown),
        supports_browser_host: crate::browser_tools::surface_supports_browser_host(
            interactive_request.surface.as_ref(),
        ),
        channel_surface: interactive_request
            .surface
            .as_ref()
            .and_then(|surface| surface.channel_surface.clone()),
    };
    let execution_context = crate::agent_runtime::execution_context::TurnExecutionContext::new(
        turn_id.clone(),
        turn_id.clone(),
        session_id,
        principal,
        crate::agent_runtime::execution_context::ProviderRoute::new(
            interactive_request.provider.clone(),
            interactive_request.model.clone(),
        ),
        crate::agent_runtime::execution_context::SurfaceCapabilities {
            ui_artifacts: continuation_scope.supports_ui_artifacts,
            liquid_markdown: continuation_scope.supports_liquid_markdown,
            browser_host: continuation_scope.supports_browser_host,
        },
        tokio_util::sync::CancellationToken::new(),
        std::time::Instant::now() + std::time::Duration::from_secs(2 * 60 * 60),
        continuation_scope.clone(),
    );
    let execution_lease = state
        .platform
        .agent_handle()
        .execution_registry
        .admit(execution_context)
        .map_err(|error| (StatusCode::TOO_MANY_REQUESTS, error.to_string()))?;
    let execution_context = execution_lease.context().clone();

    let stream_port = crate::engine_adapters::turn_stream_registry_adapter(
        state.interactive_turn_streams.clone(),
    );
    if !stream_port
        .register_stream_for_session(&turn_id, &session_id_text)
        .await
    {
        return Err((
            StatusCode::CONFLICT,
            format!("turn stream already registered for '{turn_id}'"),
        ));
    }
    let stream_entry = state
        .interactive_turn_streams
        .read()
        .await
        .get(&turn_id)
        .cloned()
        .expect("turn stream registered");

    let stream_url = format!(
        "{}/v1/interactive/turn/{}/stream",
        state.daemon_base_url.trim_end_matches('/'),
        turn_id
    );
    let now = Utc::now();
    let prompt_preview = crate::turn_ticket::prompt_preview(&interactive_request.prompt);
    let ticket = crate::turn_ticket::TurnTicket {
        turn_id: turn_id.clone(),
        session_id: session_id_text.clone(),
        mode,
        phase: crate::turn_ticket::TurnTicketPhase::Accepted,
        stream_url: stream_url.clone(),
        prompt_preview: prompt_preview.clone(),
        workspace_card_id: workspace_card_id.clone(),
        started_at: now,
        updated_at: now,
    };

    if let Err(conflict) = crate::turn_ticket::register_turn(&state.turn_tickets, ticket).await {
        stream_port.drop_stream(&turn_id).await;
        return Err((StatusCode::CONFLICT, conflict.message));
    }
    drop(admission);

    if mode == crate::turn_ticket::TurnTicketMode::Background
        && let Some(job_id) = workspace_card_id.as_deref()
    {
        crate::workspace::ask_job_store::ask_job_store().register_pending(
            crate::workspace::ask_job_store::AskJobRecord {
                job_id: job_id.to_string(),
                prompt: interactive_request.prompt.clone(),
                status: crate::workspace::ask_job_store::AskJobStatus::Pending,
                output_text: None,
                interim_text: None,
                error: None,
                session_id: session_id_text.clone(),
                manuscript_id: interactive_request.manuscript_id.clone(),
                additional_manuscript_ids: interactive_request.additional_manuscript_ids.clone(),
                suggested_capability_ids: interactive_request.suggested_capability_ids.clone(),
                model_hint: None,
                created_at_utc: now,
                updated_at_utc: now,
                finished_at_utc: None,
                archived: false,
                journal_path: None,
                notified_channel: None,
            },
        );
        crate::workspace::ask_job_store::ask_job_store().mark_running(job_id);
    }

    state
        .channel_deliveries
        .write()
        .await
        .insert(turn_id.clone(), delivery_target.clone());
    record_job_delivery_pending(state, &turn_id).await;

    let stream_registry = state.interactive_turn_streams.clone();
    let stream_port_for_task = stream_port.clone();
    let turn_tickets = state.turn_tickets.clone();
    let cancelled_interactive_turns = state.cancelled_interactive_turns.clone();
    let cancelled_turns_cleanup = state.cancelled_interactive_turns.clone();
    let _composition = state.composition().clone();
    let agent_runtime = state.platform.agent_handle();
    let backend = state.backend.clone();
    let delivery_records = state.job_delivery_records.clone();
    let channel_deliveries = state.channel_deliveries.clone();
    let last_agent_turn_at = state.last_agent_turn_at.clone();
    let last_agent_turn_latency_ms = state.last_agent_turn_latency_ms.clone();
    let delivery = crate::agent_runtime::InteractiveTurnDeliveryContext {
        turn_key: turn_id.clone(),
        delivery_records,
        channel_deliveries,
        last_turn_at: last_agent_turn_at,
        last_turn_latency_ms: last_agent_turn_latency_ms,
        started: std::time::Instant::now(),
    };
    let ask_job_id = workspace_card_id.clone();
    let ask_job_id_for_notify = ask_job_id.clone();
    let session_hooks = crate::agent_runtime::InteractiveTurnSessionHooks {
        cancelled_turns: Some(cancelled_interactive_turns),
        turn_ticket_registry: Some(turn_tickets.clone()),
        ask_job_id,
        context_usage_by_session: Some(state.last_context_usage_by_session.clone()),
    };

    let turn_id_for_task = turn_id.clone();
    let project_state = state.clone();
    let envelope = TurnEnvelope::new(turn_id_for_task.clone(), Principal::operator())
        .with_correlation_id(turn_id_for_task.clone());
    let lifecycle_ports = TurnLifecyclePorts {
        tickets: Arc::new(crate::engine_adapters::TurnTicketPortAdapter(
            turn_tickets.clone(),
        )),
        streams: Arc::new(stream_port_for_task),
    };
    tokio::spawn(async move {
        let _execution_lease = execution_lease;
        let _handle = run_turn(lifecycle_ports, envelope, || async {
            crate::agent_runtime::run_daemon_interactive_turn(
                &turn_id_for_task,
                interactive_request,
                &backend,
                agent_runtime.as_ref(),
                project_state,
                stream_entry,
                Some(delivery),
                Some(continuation_scope),
                execution_context,
                Some(session_hooks),
            )
            .await;

            if let Some(job_id) = ask_job_id_for_notify.as_deref() {
                crate::workspace::notify_workspace_event(
                    crate::workspace::WorkspaceDomainEvent::AskJobChanged {
                        job_id: job_id.to_string(),
                    },
                );
            } else {
                crate::workspace::notify_workspace_invalidate();
            }
        })
        .await;
        // The cancellation tombstone is only meaningful while this turn runs;
        // drop it now that the turn is finalized (the bounded set also caps it).
        cancelled_turns_cleanup
            .write()
            .await
            .remove(&turn_id_for_task);
        let _ = stream_registry;
    });

    let notice = match mode {
        crate::turn_ticket::TurnTicketMode::Interactive => {
            Some("interactive turn accepted; daemon agent runtime streaming active".to_string())
        }
        crate::turn_ticket::TurnTicketMode::Background => {
            Some("background turn accepted; streaming to attached clients".to_string())
        }
    };

    Ok(TurnTicketResponse {
        turn_id,
        session_id: session_id_text,
        mode,
        phase: crate::turn_ticket::TurnTicketPhase::Accepted,
        accepted_at_utc: now,
        stream_url,
        stream_ready: true,
        workspace_card_id,
        daemon_notice: notice,
    })
}

fn apply_principal_identity(
    principal: &crate::request_principal::RequestPrincipal,
    request_identity: &mut Option<String>,
) {
    // Portal/shared seats: bound pairing profile wins over client-supplied identity.
    if let Some(bound) = principal.profile_id() {
        *request_identity = Some(bound.to_string());
    }
}

pub async fn create_turn_ticket(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(mut request): Json<CreateTurnTicketRequest>,
) -> Result<Json<TurnTicketResponse>, (StatusCode, String)> {
    let session_id = crate::session_storage::validate_session_id(&request.session_id)
        .map(str::to_string)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if request.prompt.trim().is_empty() && request.media_refs.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    }
    apply_principal_identity(&principal, &mut request.identity_user_id);

    let (provider, model) = if request.provider.trim().is_empty() || request.model.trim().is_empty()
    {
        resolve_api_model_routing(request.model_hint.as_deref(), &state.default_runtime_config)
    } else {
        (request.provider.clone(), request.model.clone())
    };
    let stage_routing = request.stage_routing.clone().unwrap_or_else(|| {
        crate::stage_routing::StageRoutingMatrix::default_for(
            if provider.is_empty() {
                "openai"
            } else {
                provider.as_str()
            },
            if model.is_empty() {
                "gpt-4o-mini"
            } else {
                model.as_str()
            },
        )
    });

    let mut interactive_request =
        build_interactive_request_from_ticket(&request, provider, model, stage_routing);

    let runtime_config = resolve_session_runtime_config(&state, &session_id).await;
    if interactive_request.reasoning_effort.trim().is_empty() {
        interactive_request.reasoning_effort = runtime_config.reasoning_effort.clone();
    }

    let (turn_id, workspace_card_id) = match request.mode {
        crate::turn_ticket::TurnTicketMode::Interactive => {
            (format!("daemon-turn-{}", Uuid::new_v4().simple()), None)
        }
        crate::turn_ticket::TurnTicketMode::Background => {
            let now = Utc::now();
            let job_id = format!("medousa-daemon-ask-{}", now.timestamp_millis());
            (job_id.clone(), Some(job_id))
        }
    };

    if request.mode == crate::turn_ticket::TurnTicketMode::Background
        && let Some(job_id) = workspace_card_id.as_deref()
    {
        interactive_request.session_id =
            crate::workspace::ask_job_store::ask_job_session_id(job_id);
    }

    spawn_turn_ticket(
        &state,
        principal,
        turn_id,
        request.mode,
        interactive_request,
        workspace_card_id,
    )
    .await
    .map(Json)
}

pub async fn get_turn_ticket(
    State(state): State<AppState>,
    AxumPath(turn_id): AxumPath<String>,
) -> Result<Json<TurnTicketRecord>, (StatusCode, String)> {
    let ticket = crate::turn_ticket::get_turn(&state.turn_tickets, &turn_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("unknown turn id '{turn_id}'"),
            )
        })?;
    Ok(Json(ticket_record_from_ticket(&ticket)))
}

#[derive(Debug, Deserialize)]
pub struct ListSessionTurnsQuery {
    /// Accepted for API compatibility; listing currently always returns active turns.
    #[allow(dead_code)]
    active: Option<bool>,
}

pub async fn list_session_turns(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Query(_query): Query<ListSessionTurnsQuery>,
) -> Result<Json<SessionActiveTurnsResponse>, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let turns = crate::turn_ticket::list_active_for_session(&state.turn_tickets, &session_id).await;

    Ok(Json(SessionActiveTurnsResponse {
        session_id,
        turns: turns.iter().map(ticket_record_from_ticket).collect(),
    }))
}

pub async fn start_interactive_turn(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(request): Json<InteractiveTurnRequest>,
) -> Result<Json<InteractiveTurnResponse>, (StatusCode, String)> {
    let mut identity_user_id = request.identity_user_id.clone();
    apply_principal_identity(&principal, &mut identity_user_id);
    let ticket_request = CreateTurnTicketRequest {
        session_id: request.session_id.clone(),
        prompt: request.prompt.clone(),
        agent_mode: request.agent_mode,
        code_context: request.code_context.clone(),
        code_project_setup_authorized: request.code_project_setup_authorized,
        mode: crate::turn_ticket::TurnTicketMode::Interactive,
        persist_user_turn: request.persist_user_turn,
        response_depth_mode: request.response_depth_mode.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        provider: request.provider.clone(),
        model: request.model.clone(),
        stage_routing: Some(request.stage_routing.clone()),
        surface: request.surface.clone(),
        host_context: request.host_context.clone(),
        model_hint: None,
        manuscript_id: request.manuscript_id.clone(),
        additional_manuscript_ids: request.additional_manuscript_ids.clone(),
        suggested_capability_ids: request.suggested_capability_ids.clone(),
        voice_preset_id: request.voice_preset_id.clone(),
        voice_appendix: request.voice_appendix.clone(),
        media_refs: request.media_refs.clone(),
        identity_user_id,
    };

    let (provider, model) = (
        ticket_request.provider.clone(),
        ticket_request.model.clone(),
    );
    let stage_routing = ticket_request
        .stage_routing
        .clone()
        .unwrap_or_else(|| request.stage_routing.clone());
    let interactive_request =
        build_interactive_request_from_ticket(&ticket_request, provider, model, stage_routing);
    let turn_id = format!("daemon-turn-{}", Uuid::new_v4().simple());

    let ticket = spawn_turn_ticket(
        &state,
        principal,
        turn_id,
        crate::turn_ticket::TurnTicketMode::Interactive,
        interactive_request,
        None,
    )
    .await?;

    Ok(Json(InteractiveTurnResponse {
        turn_id: ticket.turn_id,
        accepted_at_utc: ticket.accepted_at_utc,
        stream_url: ticket.stream_url,
        stream_ready: ticket.stream_ready,
        fallback_to_local: false,
        fallback_reason: None,
        daemon_notice: ticket.daemon_notice,
    }))
}

pub async fn delete_session_handler(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<SessionDeleteQuery>,
) -> Result<Json<SessionDeleteResponse>, (StatusCode, String)> {
    crate::daemon_handlers::delete_session(
        State(crate::daemon_handlers::SessionDeleteState {
            memory_operations: Some(state.platform.memory_operations()),
            turn_tickets: state.turn_tickets.clone(),
            turn_streams: Some(state.interactive_turn_streams.clone()),
        }),
        AxumPath(session_id),
        Query(query),
    )
    .await
}

pub async fn get_active_session_turn(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::turn_ticket::ActiveSessionTurnResponse>, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(Json(
        crate::turn_ticket::get_active_interactive_turn(&state.turn_tickets, &session_id).await,
    ))
}

pub async fn cancel_active_session_turn(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::turn_ticket::CancelActiveSessionTurnResponse>, (StatusCode, String)> {
    cancel_active_session_turn_for_session(&state, &session_id)
        .await
        .map(Json)
}

pub async fn cancel_active_session_turn_for_session(
    state: &AppState,
    session_id: &str,
) -> Result<crate::turn_ticket::CancelActiveSessionTurnResponse, (StatusCode, String)> {
    let typed_session_id = crate::session_storage::SessionId::parse(session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let active =
        crate::turn_ticket::cancel_interactive_for_session(&state.turn_tickets, session_id).await;

    let Some(active) = active else {
        return Ok(crate::turn_ticket::CancelActiveSessionTurnResponse {
            cancelled: false,
            turn_id: None,
            message: "no active turn for session".to_string(),
        });
    };

    state
        .cancelled_interactive_turns
        .write()
        .await
        .insert(active.turn_id.clone());
    state
        .platform
        .agent_handle()
        .execution_registry
        .cancel_matching_turn(&typed_session_id, &active.turn_id);
    crate::turn_ticket::mark_cancelled(&state.turn_tickets, &active.turn_id).await;

    if let Some(entry) = state
        .interactive_turn_streams
        .read()
        .await
        .get(&active.turn_id)
        .cloned()
    {
        publish_interactive_turn_event(
            &entry,
            crate::interactive_turn_runtime::error_stream_event(
                &active.turn_id,
                "interactive turn cancelled",
            ),
        );
    }

    state
        .channel_deliveries
        .write()
        .await
        .remove(&active.turn_id);
    state
        .job_delivery_records
        .write()
        .await
        .remove(&active.turn_id);

    Ok(crate::turn_ticket::CancelActiveSessionTurnResponse {
        cancelled: true,
        turn_id: Some(active.turn_id),
        message: "interactive turn cancelled".to_string(),
    })
}

pub async fn interactive_turn_stream(
    State(state): State<AppState>,
    AxumPath(turn_id): AxumPath<String>,
    Query(query): Query<crate::daemon::ingest::StreamSinceQuery>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(
        &registry,
        &turn_id,
        "interactive turn",
        query.since,
        &headers,
    )
    .await
}
