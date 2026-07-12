//! Interactive turns, turn tickets, and session active-turn handlers.

use std::sync::Arc;

use medousa_engine::{Principal, TurnEnvelope, TurnLifecyclePorts, TurnStreamRegistryPort, run_turn};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use std::convert::Infallible;

use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use crate::channel_delivery;
use crate::daemon::ingest::{publish_interactive_turn_event, record_job_delivery_pending, resolve_api_model_routing, resolve_session_runtime_config, stream_events_from_registry};
use crate::daemon_api::{
    CreateTurnTicketRequest, InteractiveTurnRequest, InteractiveTurnResponse,
    SessionActiveTurnsResponse, SessionDeleteQuery, SessionDeleteResponse, TurnTicketRecord, TurnTicketResponse,
};

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
        persist_user_turn: request.persist_user_turn,
        response_depth_mode: request.response_depth_mode.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        provider,
        model,
        stage_routing,
        surface: request.surface.clone(),
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
    turn_id: String,
    mode: crate::turn_ticket::TurnTicketMode,
    interactive_request: InteractiveTurnRequest,
    workspace_card_id: Option<String>,
) -> Result<TurnTicketResponse, (StatusCode, String)> {
    let session_id = interactive_request.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    let stream_port = crate::engine_adapters::turn_stream_registry_adapter(
        state.interactive_turn_streams.clone(),
    );
    if !stream_port.register_stream(&turn_id).await {
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
        session_id: session_id.clone(),
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

    if mode == crate::turn_ticket::TurnTicketMode::Background
        && let Some(job_id) = workspace_card_id.as_deref() {
            crate::workspace::ask_job_store::ask_job_store().register_pending(
                crate::workspace::ask_job_store::AskJobRecord {
                    job_id: job_id.to_string(),
                    prompt: interactive_request.prompt.clone(),
                    status: crate::workspace::ask_job_store::AskJobStatus::Pending,
                    output_text: None,
                    interim_text: None,
                    error: None,
                    session_id: session_id.clone(),
                    manuscript_id: interactive_request.manuscript_id.clone(),
                    additional_manuscript_ids: interactive_request.additional_manuscript_ids.clone(),
                    suggested_capability_ids: interactive_request
                        .suggested_capability_ids
                        .clone(),
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

    let delivery_target =
        channel_delivery::delivery_target_from_interactive_turn(&interactive_request, &turn_id);
    state.channel_deliveries.write().await.insert(
        turn_id.clone(),
        delivery_target.clone(),
    );
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
    let continuation_scope = crate::turn_continuation::TurnContinuationScope {
        turn_correlation_id: turn_id.clone(),
        session_id: interactive_request.session_id.clone(),
        original_prompt: interactive_request.prompt.clone(),
        delivery_target: Some(delivery_target),
        provider: interactive_request.provider.clone(),
        model: interactive_request.model.clone(),
        response_depth_mode: interactive_request.response_depth_mode.clone(),
        supports_ui_artifacts: crate::ui_present_tools::surface_supports_ui_artifacts(
            interactive_request.surface.as_ref(),
        ),
        supports_browser_host: crate::browser_tools::surface_supports_browser_host(
            interactive_request.surface.as_ref(),
        ),
        channel_surface: interactive_request
            .surface
            .as_ref()
            .and_then(|surface| surface.channel_surface.clone()),
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
    let envelope =
        TurnEnvelope::new(turn_id_for_task.clone(), Principal::operator())
            .with_correlation_id(turn_id_for_task.clone());
    let lifecycle_ports = TurnLifecyclePorts {
        tickets: Arc::new(crate::engine_adapters::TurnTicketPortAdapter(turn_tickets.clone())),
        streams: Arc::new(stream_port_for_task),
    };
    tokio::spawn(async move {
        let _handle = run_turn(lifecycle_ports, envelope, || async {
            crate::agent_runtime::run_daemon_interactive_turn(
                &turn_id_for_task,
                interactive_request,
                &backend,
                agent_runtime.as_ref(),
                stream_entry,
                Some(delivery),
                Some(continuation_scope),
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
        session_id,
        mode,
        phase: crate::turn_ticket::TurnTicketPhase::Accepted,
        accepted_at_utc: now,
        stream_url,
        stream_ready: true,
        workspace_card_id,
        daemon_notice: notice,
    })
}

pub async fn create_turn_ticket(
    State(state): State<AppState>,
    Json(request): Json<CreateTurnTicketRequest>,
) -> Result<Json<TurnTicketResponse>, (StatusCode, String)> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }
    if request.prompt.trim().is_empty() && request.media_refs.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    }

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
        && let Some(job_id) = workspace_card_id.as_deref() {
            interactive_request.session_id =
                crate::workspace::ask_job_store::ask_job_session_id(job_id);
        }

    spawn_turn_ticket(
        &state,
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
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("unknown turn id '{turn_id}'")))?;
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
) -> Json<SessionActiveTurnsResponse> {
    let turns =
        crate::turn_ticket::list_active_for_session(&state.turn_tickets, &session_id).await;

    Json(SessionActiveTurnsResponse {
        session_id,
        turns: turns.iter().map(ticket_record_from_ticket).collect(),
    })
}

pub async fn start_interactive_turn(
    State(state): State<AppState>,
    Json(request): Json<InteractiveTurnRequest>,
) -> Result<Json<InteractiveTurnResponse>, (StatusCode, String)> {
    let ticket_request = CreateTurnTicketRequest {
        session_id: request.session_id.clone(),
        prompt: request.prompt.clone(),
        mode: crate::turn_ticket::TurnTicketMode::Interactive,
        persist_user_turn: request.persist_user_turn,
        response_depth_mode: request.response_depth_mode.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        provider: request.provider.clone(),
        model: request.model.clone(),
        stage_routing: Some(request.stage_routing.clone()),
        surface: request.surface.clone(),
        model_hint: None,
        manuscript_id: request.manuscript_id.clone(),
        additional_manuscript_ids: request.additional_manuscript_ids.clone(),
        suggested_capability_ids: request.suggested_capability_ids.clone(),
        voice_preset_id: request.voice_preset_id.clone(),
        voice_appendix: request.voice_appendix.clone(),
        media_refs: request.media_refs.clone(),
        identity_user_id: request.identity_user_id.clone(),
    };

    let (provider, model) = (ticket_request.provider.clone(), ticket_request.model.clone());
    let stage_routing = ticket_request
        .stage_routing
        .clone()
        .unwrap_or_else(|| request.stage_routing.clone());
    let interactive_request =
        build_interactive_request_from_ticket(&ticket_request, provider, model, stage_routing);
    let turn_id = format!("daemon-turn-{}", Uuid::new_v4().simple());

    let ticket = spawn_turn_ticket(
        &state,
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
        }),
        AxumPath(session_id),
        Query(query),
    )
    .await
}

pub async fn get_active_session_turn(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Json<crate::turn_ticket::ActiveSessionTurnResponse> {
    Json(
        crate::turn_ticket::get_active_interactive_turn(&state.turn_tickets, &session_id).await,
    )
}

pub async fn cancel_active_session_turn(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Json<crate::turn_ticket::CancelActiveSessionTurnResponse> {
    let active = crate::turn_ticket::cancel_interactive_for_session(
        &state.turn_tickets,
        &session_id,
    )
    .await;

    let Some(active) = active else {
        return Json(crate::turn_ticket::CancelActiveSessionTurnResponse {
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

    Json(crate::turn_ticket::CancelActiveSessionTurnResponse {
        cancelled: true,
        turn_id: Some(active.turn_id),
        message: "interactive turn cancelled".to_string(),
    })
}

pub async fn interactive_turn_stream(
    State(state): State<AppState>,
    AxumPath(turn_id): AxumPath<String>,
    Query(query): Query<crate::daemon::ingest::StreamSinceQuery>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(&registry, &turn_id, "interactive turn", query.since).await
}
