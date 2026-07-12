//! Channel ingest (`POST /v1/ingest`), SSE stream, and delivery webhook handlers.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use axum::extract::{Path as AxumPath, Query as AxumQuery, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use chrono::{DateTime, Utc};
use futures_util::stream::{self, Stream};
use serde_json::Value;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::prelude::{JobState, RuntimeComposition};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::agent_runtime::stream_sink::AgentStreamSink;
use crate::channel_delivery;
use crate::daemon::heartbeat::is_missing_runtime_table_error;
use crate::daemon::bounded_set::BoundedDedupSet;
use crate::daemon::state::{AgentTurnJobRecord, AppState};
use medousa_engine::TurnStreamRegistryPort;

use crate::daemon::turn_event_channel::TurnEventChannel;
use crate::daemon::turn_stream_registry::{TurnStreamEntry, TurnStreamRegistry};
use medousa_engine::TurnEventLog;
use crate::daemon_api::{
    DeliverPollResponse, DeliveryHealthResponse, IngestRequest, IngestResponse,
    InteractiveTurnStreamEvent, RuntimeConfigCommandRequest, RuntimeConfigCommandResponse,
    RuntimeConfigCommandSpec,
};
use crate::session_mapping;

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        err.to_string(),
    )
}

/// `?since=<seq>` query param shared by the interactive-turn and ingest SSE
/// routes. A reconnecting client passes the last seq it rendered so the server
/// replays exactly the events it missed.
#[derive(Debug, Default, serde::Deserialize)]
pub struct StreamSinceQuery {
    #[serde(default)]
    pub since: Option<u64>,
}

pub async fn ingest_stream(
    State(state): State<AppState>,
    AxumPath(stream_id): AxumPath<String>,
    AxumQuery(query): AxumQuery<StreamSinceQuery>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(&registry, &stream_id, "ingest stream", query.since).await
}

/// State carried through the SSE unfold: live fan-out channel, durable replay log,
/// broadcast receiver, pending replay queue, and dedupe cursor.
struct SseUnfoldState {
    channel: Arc<TurnEventChannel>,
    log: Arc<TurnEventLog>,
    receiver: broadcast::Receiver<InteractiveTurnStreamEvent>,
    pending: std::collections::VecDeque<InteractiveTurnStreamEvent>,
    last_seq: u64,
    drained: bool,
}

fn replay_from_log(log: &TurnEventLog, since: u64) -> std::collections::VecDeque<InteractiveTurnStreamEvent> {
    log.snapshot_since(since)
        .iter()
        .map(crate::sse_turn_projection::sequenced_to_stream_event)
        .collect()
}

fn sse_event_from_payload(payload: InteractiveTurnStreamEvent) -> Event {
    let event_type = payload.event_type.clone();
    match Event::default().event(event_type).json_data(payload) {
        Ok(value) => value,
        Err(err) => Event::default()
            .event("error")
            .data(format!("stream serialization error: {err}")),
    }
}

pub async fn stream_events_from_registry(
    registry: &TurnStreamRegistry,
    stream_id: &str,
    label: &str,
    since: Option<u64>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let (channel, log) = {
        let guard = registry.read().await;
        guard
            .get(stream_id)
            .map(|entry| (entry.channel.clone(), entry.log.clone()))
    }
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("unknown {} id '{}'", label, stream_id),
        )
    })?;

    // Subscribe BEFORE snapshotting so no event can slip between the snapshot
    // and the live subscription. Any event in both is deduped by seq below.
    let receiver = channel.subscribe();
    let since = since.unwrap_or(0);
    let pending = replay_from_log(&log, since);

    let initial = SseUnfoldState {
        channel,
        log,
        receiver,
        pending,
        last_seq: since,
        drained: false,
    };

    let stream = stream::unfold(initial, |mut state| async move {
        loop {
            // 1) Flush any buffered/replayed events first (exactly-once by seq).
            if let Some(payload) = state.pending.pop_front() {
                if payload.seq != 0 && payload.seq <= state.last_seq {
                    continue;
                }
                state.last_seq = state.last_seq.max(payload.seq);
                return Some((Ok::<Event, Infallible>(sse_event_from_payload(payload)), state));
            }
            if state.drained {
                return None;
            }

            // 2) Otherwise pull the next live event.
            match state.receiver.recv().await {
                Ok(payload) => {
                    // Skip anything already covered by the replay snapshot / prior emits.
                    if payload.seq != 0 && payload.seq <= state.last_seq {
                        continue;
                    }
                    state.last_seq = state.last_seq.max(payload.seq);
                    return Some((Ok::<Event, Infallible>(sse_event_from_payload(payload)), state));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // We fell behind the live ring; recover the gap from the
                    // durable spine rather than dropping events outright.
                    state.pending.extend(replay_from_log(&state.log, state.last_seq));
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    // Senders gone: drain any buffered tail from the spine so a
                    // client reconnecting right at the end still sees it.
                    state
                        .pending
                        .extend(replay_from_log(&state.log, state.last_seq));
                    state.drained = true;
                    continue;
                }
            }
        }
    });

    Ok(
        Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive")),
    )
}

pub fn publish_interactive_turn_event(
    entry: &TurnStreamEntry,
    event: AnyhowResult<InteractiveTurnStreamEvent>,
) {
    if let Ok(mut payload) = event {
        let journal =
            crate::sse_turn_projection::journal_turn_event_for_stream(&payload, None);
        let sequenced = entry.log.append(journal);
        payload.seq = sequenced.seq();
        entry.channel.publish(payload);
    }
}

pub fn publish_interactive_turn_event_legacy(
    channel: &TurnEventChannel,
    event: AnyhowResult<InteractiveTurnStreamEvent>,
) {
    if let Ok(mut payload) = event {
        if payload.seq == 0 {
            payload.seq = 1;
        }
        channel.publish(payload);
    }
}

/// POST /v1/ingest — centralized ingester handler.
pub async fn ingest_handler(
    State(state): State<AppState>,
    Json(request): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    if request.channel.trim().is_empty() || request.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "channel and text are required".to_string(),
        ));
    }

    let product_config = crate::load_product_config();
    if !crate::ingest_sender_allowed(&request.channel, &request.user_id, &product_config) {
        let mapping_key = format!(
            "{}:{}:{}",
            request.channel, request.channel_id, request.user_id
        );
        let session_id = crate::channel_session_store::channel_session_store()
            .get_session_id(&mapping_key)
            .await
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        return Ok(build_ingest_response(
            session_id,
            None,
            "This sender is not on the Telegram allowlist for this bot.".to_string(),
            false,
            None,
            None,
            false,
        ));
    }

    let mapping_key = format!("{}:{}:{}", request.channel, request.channel_id, request.user_id);
    let existing_session_id = crate::channel_session_store::channel_session_store()
        .get_session_id(&mapping_key)
        .await;

    if request.text.trim().eq_ignore_ascii_case("/new")
        && let Some(old_session_id) = existing_session_id.clone() {
            push_channel_session_history(&mapping_key, old_session_id).await;
        }

    let outcome =
        session_mapping::process_ingest(&request, &mapping_key, existing_session_id.clone());

    let mut job_id = None;
    let mut stream_id = None;
    let mut stream_url = None;
    let mut stream_ready = false;
    let mut reply = outcome.reply.clone();

    match outcome.action {
        session_mapping::IngestAction::Reply => {}
        session_mapping::IngestAction::EnqueueAsk {
            prompt,
            manuscript_id,
        } => {
            let stream = start_ingest_ask_stream(
                &state,
                &mapping_key,
                &outcome.session_id,
                prompt,
                manuscript_id,
                &request,
            )
            .await?;
            job_id = Some(stream.job_id);
            stream_id = Some(stream.stream_id);
            stream_url = Some(stream.stream_url);
            stream_ready = true;
            reply.clear();
        }
        session_mapping::IngestAction::CancelActiveJob => {
            reply = cancel_active_ingest_job(&state, &mapping_key).await;
        }
        session_mapping::IngestAction::Regenerate => {
            let Some(prompt) = session_mapping::last_user_prompt_for_regen(&outcome.session_id)
            else {
                reply = "no user prompt available to regenerate".to_string();
                return Ok(build_ingest_response(
                    outcome.session_id,
                    job_id,
                    reply,
                    outcome.is_new_session,
                    stream_id,
                    stream_url,
                    stream_ready,
                ));
            };
            let stream = start_ingest_ask_stream(
                &state,
                &mapping_key,
                &outcome.session_id,
                prompt,
                None,
                &request,
            )
            .await?;
            job_id = Some(stream.job_id);
            stream_id = Some(stream.stream_id);
            stream_url = Some(stream.stream_url);
            stream_ready = true;
            reply.clear();
        }
        session_mapping::IngestAction::ListHistory => {
            reply = format_channel_session_history(&mapping_key, &outcome.session_id).await;
        }
        session_mapping::IngestAction::ResumeSession { target_session_id } => {
            push_channel_session_history(&mapping_key, outcome.session_id.clone()).await;
            crate::channel_session_store::channel_session_store()
                .set_session_id(&mapping_key, target_session_id.clone())
                .await;
            reply = format!("resumed session {target_session_id}");
            return Ok(build_ingest_response(
                target_session_id,
                job_id,
                reply,
                false,
                stream_id,
                stream_url,
                stream_ready,
            ));
        }
        session_mapping::IngestAction::ConfigureModel { args } => {
            reply = apply_session_model_config(&state, &outcome.session_id, args).await?;
        }
        session_mapping::IngestAction::ConfigureDepth { mode } => {
            reply = apply_session_depth_config(&state, &outcome.session_id, mode).await?;
        }
        session_mapping::IngestAction::SetDisplayName { .. } => {
            // Reply text already set in process_ingest (name show/set).
        }
        session_mapping::IngestAction::QueryHealth => {
            reply = format!(
                "daemon status=ok backend={} worker={} now={}",
                state.backend,
                state.worker_id,
                Utc::now()
            );
        }
        session_mapping::IngestAction::QueryHeartbeat => {
            reply = format_ingest_heartbeat_reply(&state).await;
        }
        session_mapping::IngestAction::QueryContextUsage => {
            reply = format_ingest_context_usage_reply(&state, &outcome.session_id).await;
        }
    }

    crate::channel_session_store::channel_session_store()
        .set_session_id(&mapping_key, outcome.session_id.clone())
        .await;

    Ok(build_ingest_response(
        outcome.session_id,
        job_id,
        reply,
        outcome.is_new_session,
        stream_id,
        stream_url,
        stream_ready,
    ))
}

struct IngestAskStream {
    job_id: String,
    stream_id: String,
    stream_url: String,
}

fn build_ingest_response(
    session_id: String,
    job_id: Option<String>,
    reply: String,
    is_new_session: bool,
    stream_id: Option<String>,
    stream_url: Option<String>,
    stream_ready: bool,
) -> Json<IngestResponse> {
    Json(IngestResponse {
        session_id,
        job_id,
        reply,
        is_new_session,
        stream_id,
        stream_url,
        stream_ready,
    })
}

async fn push_channel_session_history(mapping_key: &str, session_id: String) {
    crate::channel_session_store::channel_session_store()
        .push_session_history(mapping_key, session_id)
        .await;
}

async fn format_channel_session_history(
    mapping_key: &str,
    active_session_id: &str,
) -> String {
    let entries = crate::channel_session_store::channel_session_store()
        .list_session_history(mapping_key, 20)
        .await;

    let active_label = crate::session::format_session_history_label(
        active_session_id,
        crate::session::get_session_display_name(active_session_id).as_deref(),
    );
    let mut lines = vec![format!(
        "* {active_label} (active, {} turns)",
        crate::session::session_turn_count(active_session_id)
    )];

    for session_id in entries.into_iter().take(9) {
        if session_id == active_session_id {
            continue;
        }
        let label = crate::session::format_session_history_label(
            &session_id,
            crate::session::get_session_display_name(&session_id).as_deref(),
        );
        lines.push(format!(
            "* {label} ({} turns)",
            crate::session::session_turn_count(&session_id)
        ));
    }

    format!(
        "Recent sessions for this channel/user:\n{}\n\nUse /history <name or session id> to resume.",
        lines.join("\n")
    )
}

pub async fn resolve_session_runtime_config(
    state: &AppState,
    session_id: &str,
) -> session_mapping::IngestSessionRuntimeConfig {
    state
        .session_runtime_configs
        .read()
        .await
        .get(session_id)
        .cloned()
        .unwrap_or_else(|| state.default_runtime_config.clone())
}

async fn apply_session_model_config(
    state: &AppState,
    session_id: &str,
    args: Vec<String>,
) -> Result<String, (StatusCode, String)> {
    let current = resolve_session_runtime_config(state, session_id).await;
    let request = RuntimeConfigCommandRequest {
        current_provider: current.draft_provider.clone(),
        current_model: current.draft_model.clone(),
        draft_provider: current.draft_provider.clone(),
        draft_model: current.draft_model.clone(),
        current_response_depth_mode: current.response_depth_mode.clone(),
        current_reasoning_effort: current.reasoning_effort.clone(),
        command: RuntimeConfigCommandSpec::Model { args },
    };
    let response = crate::runtime_config_command_runtime::execute_runtime_config_command(request)
        .map_err(internal_error)?;
    persist_session_runtime_config(state, session_id, &current, &response).await;
    Ok(response
        .rendered_output
        .unwrap_or_else(|| format!("model {}:{}", response.next_draft_provider, response.next_draft_model)))
}

async fn apply_session_depth_config(
    state: &AppState,
    session_id: &str,
    mode: Option<String>,
) -> Result<String, (StatusCode, String)> {
    let current = resolve_session_runtime_config(state, session_id).await;
    let request = RuntimeConfigCommandRequest {
        current_provider: current.draft_provider.clone(),
        current_model: current.draft_model.clone(),
        draft_provider: current.draft_provider.clone(),
        draft_model: current.draft_model.clone(),
        current_response_depth_mode: current.response_depth_mode.clone(),
        current_reasoning_effort: current.reasoning_effort.clone(),
        command: RuntimeConfigCommandSpec::Depth { mode },
    };
    let response = crate::runtime_config_command_runtime::execute_runtime_config_command(request)
        .map_err(internal_error)?;
    persist_session_runtime_config(state, session_id, &current, &response).await;
    Ok(response
        .rendered_output
        .unwrap_or_else(|| format!("depth mode={}", response.next_response_depth_mode)))
}

async fn persist_session_runtime_config(
    state: &AppState,
    session_id: &str,
    _current: &session_mapping::IngestSessionRuntimeConfig,
    response: &RuntimeConfigCommandResponse,
) {
    let next = session_mapping::IngestSessionRuntimeConfig {
        draft_provider: response.next_draft_provider.clone(),
        draft_model: response.next_draft_model.clone(),
        response_depth_mode: response.next_response_depth_mode.clone(),
        reasoning_effort: response.next_reasoning_effort.clone(),
    };
    state
        .session_runtime_configs
        .write()
        .await
        .insert(session_id.to_string(), next);
}

async fn cancel_active_ingest_job(state: &AppState, mapping_key: &str) -> String {
    let active = state.active_ingest_jobs.write().await.remove(mapping_key);
    let Some(active) = active else {
        return "no active ingest job to stop".to_string();
    };

    state
        .cancelled_ingest_streams
        .write()
        .await
        .insert(active.stream_id.clone());
    state
        .channel_deliveries
        .write()
        .await
        .remove(&active.job_id);
    state.job_delivery_records.write().await.remove(&active.job_id);

    format!("stopped active job {}", active.job_id)
}

async fn format_ingest_heartbeat_reply(state: &AppState) -> String {
    let now_utc = Utc::now();
    let last_tick_at_utc = *state.last_tick_at.read().await;
    let report = state.last_heartbeat_report.read().await.clone();
    let metrics = state.heartbeat_metrics.read().await.clone();

    if let Some(report) = report {
        format!(
            "heartbeat action={} significance={:.2} reason={}\nfailed={} dead_letter={} outbox_pending={}\ndelivery dispatched={} suppressed_quiet={} suppressed_interval={} last_tick={:?} now={}",
            report.heartbeat_action.as_str(),
            report.heartbeat_significance,
            report.heartbeat_reason,
            report.failed_jobs,
            report.dead_letter_jobs,
            report.pending_outbox_events,
            metrics.dispatched_notifications,
            metrics.suppressed_quiet_hours,
            metrics.suppressed_min_interval,
            last_tick_at_utc,
            now_utc,
        )
    } else {
        format!("heartbeat status unavailable last_tick={last_tick_at_utc:?} now={now_utc}")
    }
}

async fn format_ingest_context_usage_reply(state: &AppState, session_id: &str) -> String {
    let report = state
        .last_context_usage_by_session
        .read()
        .await
        .get(session_id)
        .cloned();
    match report {
        Some(report) => crate::agent_runtime::context_usage::format_context_usage_text(&report),
        None => "No context usage snapshot yet — send a message first.".to_string(),
    }
}

pub async fn deliver_outbox_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<channel_delivery::OutboxDeliveryWebhook>,
) -> Result<StatusCode, (StatusCode, String)> {
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if !channel_delivery::verify_deliver_webhook_bearer(
        auth,
        state.deliver_webhook_token.as_deref(),
    ) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "deliver webhook bearer token required".to_string(),
        ));
    }

    {
        let mut delivered = state.delivered_outbox_events.write().await;
        if !delivered.insert(payload.event_id.clone()) {
            return Ok(StatusCode::OK);
        }
    }

    let started = std::time::Instant::now();
    let target = {
        let per_job = state.channel_deliveries.read().await;
        crate::recurring_delivery::resolve_delivery_target_for_job(
            state.composition(),
            &payload.job_id,
            &per_job,
        )
        .await
    };

    match payload.event_type.as_str() {
        "job_succeeded" => {
            if maybe_resume_agent_turn_from_child_job(&state, &payload.job_id).await {
                return Ok(StatusCode::OK);
            }

            let job_title = resolve_job_title_for_vault_footer(state.composition(), &payload.job_id)
                .await
                .unwrap_or_else(|| payload.job_id.clone());
            let appended = crate::vault::job_footer::maybe_append_job_success_footers(
                &payload.job_id,
                &job_title,
                Utc::now(),
            );
            if appended > 0 {
                eprintln!(
                    "vault job_success_footer appended job_id={} notes={appended}",
                    payload.job_id
                );
            }

            let Some(target) = target else {
                eprintln!(
                    "deliver/outbox job_succeeded missing delivery target job_id={}",
                    payload.job_id
                );
                return Ok(StatusCode::OK);
            };

            let output = if let Some(message) = payload
                .message
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                message.to_string()
            } else {
                resolve_job_output_text(state.composition(), &payload.job_id).await?
            };

            channel_delivery::dispatch_channel_message(
                &state.channel_dispatch_client,
                &target,
                &output,
            )
            .await
            .map_err(|err| {
                eprintln!(
                    "deliver/outbox channel dispatch failed job_id={} channel={}: {err:#}",
                    payload.job_id, target.channel
                );
                (StatusCode::BAD_GATEWAY, err.to_string())
            })?;

            if let Some(stream_id) = target.stream_id.as_deref()
                && let Some(entry) = state
                    .interactive_turn_streams
                    .read()
                    .await
                    .get(stream_id)
                    .cloned()
                {
                    publish_interactive_turn_event(
                        &entry,
                        crate::interactive_turn_runtime::final_stream_event(
                            stream_id,
                            &output,
                        ),
                    );
                }

            record_job_delivery_success(
                &state,
                &payload.job_id,
                started.elapsed().as_millis() as u64,
                None,
            )
            .await;
            state
                .channel_deliveries
                .write()
                .await
                .remove(&payload.job_id);
            Ok(StatusCode::OK)
        }
        "job_dead_lettered" => {
            let _ = crate::turn_continuation::turn_continuation_store()
                .mark_child_dead_letter(&payload.job_id)
                .await;

            if let Some(target) = target {
                let error_text = payload
                    .message
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "your request could not be completed".to_string());
                let user_message = format!("Sorry — {error_text}");
                let _ = channel_delivery::dispatch_channel_message(
                    &state.channel_dispatch_client,
                    &target,
                    &user_message,
                )
                .await;
                record_job_delivery_success(
                    &state,
                    &payload.job_id,
                    started.elapsed().as_millis() as u64,
                    Some(error_text),
                )
                .await;
                state
                    .channel_deliveries
                    .write()
                    .await
                    .remove(&payload.job_id);
            }
            Ok(StatusCode::OK)
        }
        _ => Ok(StatusCode::OK),
    }
}

pub async fn deliver_poll(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<DeliverPollResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    Ok(Json(build_deliver_poll_response(&state, &job_id).await))
}

pub async fn delivery_status(State(state): State<AppState>) -> Json<DeliveryHealthResponse> {
    let pending_job_deliveries = state
        .job_delivery_records
        .read()
        .await
        .values()
        .filter(|record| record.state == channel_delivery::JobDeliveryState::Pending)
        .count();

    Json(DeliveryHealthResponse {
        endpoint_id: channel_delivery::INTERNAL_OUTBOX_ENDPOINT_ID.to_string(),
        endpoint_seeded: true,
        endpoint_target: state.deliver_webhook_target.clone(),
        deliver_webhook_auth_configured: state.deliver_webhook_token.is_some(),
        pending_job_deliveries,
        last_delivery_at_utc: *state.last_delivery_at.read().await,
        last_delivery_latency_ms: *state.last_delivery_latency_ms.read().await,
    })
}
async fn build_deliver_poll_response(state: &AppState, job_id: &str) -> DeliverPollResponse {
    if let Some(record) = state.job_delivery_records.read().await.get(job_id) {
        return DeliverPollResponse {
            job_id: job_id.to_string(),
            status: job_delivery_status_label(&record.state).to_string(),
            delivered_at_utc: record.delivered_at,
            error: record.error.clone(),
        };
    }

    DeliverPollResponse {
        job_id: job_id.to_string(),
        status: "not_registered".to_string(),
        delivered_at_utc: None,
        error: None,
    }
}

fn job_delivery_status_label(state: &channel_delivery::JobDeliveryState) -> &'static str {
    match state {
        channel_delivery::JobDeliveryState::Pending => "pending",
        channel_delivery::JobDeliveryState::Delivered => "delivered",
        channel_delivery::JobDeliveryState::Failed => "failed",
    }
}

pub async fn record_job_delivery_pending(state: &AppState, job_id: &str) {
    state.job_delivery_records.write().await.insert(
        job_id.to_string(),
        channel_delivery::JobDeliveryRecord {
            state: channel_delivery::JobDeliveryState::Pending,
            delivered_at: None,
            error: None,
            latency_ms: None,
        },
    );
}

async fn record_job_delivery_success(
    state: &AppState,
    job_id: &str,
    latency_ms: u64,
    error: Option<String>,
) {
    let now = Utc::now();
    let failed = error.as_ref().is_some_and(|value| !value.trim().is_empty());
    state.job_delivery_records.write().await.insert(
        job_id.to_string(),
        channel_delivery::JobDeliveryRecord {
            state: if failed {
                channel_delivery::JobDeliveryState::Failed
            } else {
                channel_delivery::JobDeliveryState::Delivered
            },
            delivered_at: Some(now),
            error,
            latency_ms: Some(latency_ms),
        },
    );
    *state.last_delivery_at.write().await = Some(now);
    *state.last_delivery_latency_ms.write().await = Some(latency_ms);
}

async fn resolve_job_output_text(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Result<String, (StatusCode, String)> {
    let attempts = get_job_attempts_graceful(runtime, job_id).await?;
    let output = attempts.iter().rev().find_map(|attempt| {
        channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
    });

    output.ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            format!("job {job_id} succeeded but no output text was found"),
        )
    })
}

async fn mark_job_delivery_success(
    job_id: &str,
    latency_ms: u64,
    error: Option<String>,
    delivery_records: &Arc<RwLock<HashMap<String, channel_delivery::JobDeliveryRecord>>>,
    last_delivery_at: &Arc<RwLock<Option<DateTime<Utc>>>>,
    last_delivery_latency_ms: &Arc<RwLock<Option<u64>>>,
) {
    let now = Utc::now();
    let failed = error.as_ref().is_some_and(|value| !value.trim().is_empty());
    delivery_records.write().await.insert(
        job_id.to_string(),
        channel_delivery::JobDeliveryRecord {
            state: if failed {
                channel_delivery::JobDeliveryState::Failed
            } else {
                channel_delivery::JobDeliveryState::Delivered
            },
            delivered_at: Some(now),
            error,
            latency_ms: Some(latency_ms),
        },
    );
    *last_delivery_at.write().await = Some(now);
    *last_delivery_latency_ms.write().await = Some(latency_ms);
}

pub fn resolve_api_model_routing(
    model_hint: Option<&str>,
    defaults: &session_mapping::IngestSessionRuntimeConfig,
) -> (String, String) {
    let hint = model_hint.map(str::trim).filter(|value| !value.is_empty());
    if let Some(hint) = hint {
        if let Some((provider, model)) = hint.split_once(':') {
            let provider = provider.trim();
            let model = model.trim();
            if !provider.is_empty() && !model.is_empty() {
                return (
                    crate::resolve_llm_provider(Some(provider)),
                    crate::resolve_llm_model(Some(model)),
                );
            }
        }
        return (
            defaults.draft_provider.clone(),
            crate::resolve_llm_model(Some(hint)),
        );
    }

    (
        defaults.draft_provider.clone(),
        defaults.draft_model.clone(),
    )
}
async fn resolve_job_title_for_vault_footer(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Option<String> {
    crate::workspace::WorkspaceService::get_card_detail(
        std::sync::Arc::new(runtime.clone()),
        job_id,
    )
    .await
    .ok()
    .flatten()
    .map(|detail| detail.card.title)
}

pub async fn job_succeeded(runtime: &RuntimeComposition, job_id: &str) -> bool {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt
            .job_store
            .get(job_id)
            .await
            .ok()
            .flatten()
            .is_some_and(|job| job.state == JobState::Succeeded),
        RuntimeComposition::Surreal(rt) => rt
            .job_store
            .get(job_id)
            .await
            .ok()
            .flatten()
            .is_some_and(|job| job.state == JobState::Succeeded),
    }
}

pub async fn maybe_resume_agent_turn_from_child_job(state: &AppState, child_job_id: &str) -> bool {
    let store = crate::turn_continuation::turn_continuation_store();
    let Some(record) = store.get(child_job_id).await else {
        return false;
    };
    if !record.should_resume() {
        return false;
    }
    if !store.mark_resumed(child_job_id).await.unwrap_or(false) {
        return false;
    }

    let job_output = crate::turn_continuation::resolve_succeeded_job_output_text(
        state.composition(),
        child_job_id,
    )
    .await
    .unwrap_or_else(|| "Job succeeded but output text was unavailable.".to_string());

    let resume_prompt = crate::turn_continuation::build_turn_resume_prompt(
        &record.original_prompt,
        &record.tool_name,
        &record.job_type,
        child_job_id,
        &job_output,
    );

    eprintln!(
        "turn continuation resume child_job_id={child_job_id} turn_correlation_id={} session_id={}",
        record.turn_correlation_id, record.session_id
    );

    spawn_continuation_agent_turn(state, &record, resume_prompt).await;
    crate::turn_continuation::record_continuation_resume(
        crate::turn_continuation::TurnContinuationResumeEvent {
            child_job_id: child_job_id.to_string(),
            turn_correlation_id: record.turn_correlation_id.clone(),
            session_id: record.session_id.clone(),
            resumed_at: Utc::now(),
        },
    );
    true
}

async fn spawn_continuation_agent_turn(
    state: &AppState,
    record: &crate::turn_continuation::TurnContinuationRecord,
    resume_prompt: String,
) {
    let now = Utc::now();
    let job_id = format!(
        "medousa-daemon-continue-{}-{}",
        now.timestamp_millis(),
        &record.session_id[..record.session_id.len().min(8)]
    );

    let mut interactive_request = session_mapping::build_interactive_turn_request_for_ingest(
        &record.session_id,
        resume_prompt,
        &record.provider,
        &record.model,
        &record.response_depth_mode,
        crate::reasoning_effort::REASONING_EFFORT_DEFAULT,
        None,
        None,
        None,
        None,
    );
    interactive_request.persist_user_turn = false;

    let continuation_scope = crate::turn_continuation::TurnContinuationScope {
        turn_correlation_id: job_id.clone(),
        session_id: record.session_id.clone(),
        original_prompt: record.original_prompt.clone(),
        delivery_target: record
            .delivery_target
            .as_ref()
            .map(channel_delivery::ChannelDeliveryTarget::from),
        provider: record.provider.clone(),
        model: record.model.clone(),
        response_depth_mode: record.response_depth_mode.clone(),
        supports_ui_artifacts: false,
        supports_browser_host: false,
        channel_surface: interactive_request
            .surface
            .as_ref()
            .and_then(|surface| surface.channel_surface.clone()),
    };

    if let Some(target) = record
        .delivery_target
        .as_ref()
        .map(channel_delivery::ChannelDeliveryTarget::from)
    {
        state
            .channel_deliveries
            .write()
            .await
            .insert(job_id.clone(), target.clone());
        record_job_delivery_pending(state, &job_id).await;

        let stream_id = format!("continue-{}", Uuid::new_v4().simple());
        {
            let port = crate::engine_adapters::turn_stream_registry_adapter(
                state.interactive_turn_streams.clone(),
            );
            port.register_stream(&stream_id).await;
        }
        let stream_entry = state
            .interactive_turn_streams
            .read()
            .await
            .get(&stream_id)
            .cloned()
            .expect("continue stream registered");

        let sink: Arc<dyn AgentStreamSink> = Arc::new(IngestAgentStreamSink {
            stream_id: stream_id.clone(),
            session_id: record.session_id.clone(),
            job_id: job_id.clone(),
            stream: stream_entry,
            delivery_target: target,
            dispatch_client: state.channel_dispatch_client.clone(),
            delivery_records: state.job_delivery_records.clone(),
            channel_deliveries: state.channel_deliveries.clone(),
            last_delivery_at: state.last_delivery_at.clone(),
            last_delivery_latency_ms: state.last_delivery_latency_ms.clone(),
            cancelled_streams: state.cancelled_ingest_streams.clone(),
            delivery_started: std::time::Instant::now(),
            parts: std::sync::Mutex::new(crate::turn_parts::TurnPartsAccumulator::default()),
        });

        let agent_runtime = state.platform.agent_handle();
        let backend = state.backend.clone();
        tokio::spawn(async move {
            crate::agent_runtime::run_agent_turn(
                &stream_id,
                interactive_request,
                &backend,
                agent_runtime.as_ref(),
                sink,
                Some(continuation_scope),
                None,
            )
            .await;
        });
        return;
    }

    spawn_daemon_api_agent_turn_with_scope(
        state,
        job_id,
        record.session_id.clone(),
        interactive_request.prompt,
        record.response_depth_mode.clone(),
        interactive_request.reasoning_effort.clone(),
        record.provider.clone(),
        record.model.clone(),
        continuation_scope,
        interactive_request.manuscript_id.clone(),
        interactive_request.additional_manuscript_ids.clone(),
        interactive_request.suggested_capability_ids.clone(),
    )
    .await;
}

pub async fn spawn_daemon_api_agent_turn(
    state: &AppState,
    job_id: String,
    session_id: String,
    prompt: String,
    response_depth_mode: String,
    reasoning_effort: String,
    provider: String,
    model: String,
    manuscript_id: Option<String>,
    additional_manuscript_ids: Option<Vec<String>>,
    suggested_capability_ids: Option<Vec<String>>,
) {
    let continuation_scope = crate::turn_continuation::TurnContinuationScope {
        turn_correlation_id: job_id.clone(),
        session_id: session_id.clone(),
        original_prompt: prompt.clone(),
        delivery_target: None,
        provider: provider.clone(),
        model: model.clone(),
        response_depth_mode: response_depth_mode.clone(),
        supports_ui_artifacts: false,
        supports_browser_host: false,
        channel_surface: Some("api".to_string()),
    };
    spawn_daemon_api_agent_turn_with_scope(
        state,
        job_id,
        session_id,
        prompt,
        response_depth_mode,
        reasoning_effort,
        provider,
        model,
        continuation_scope,
        manuscript_id,
        additional_manuscript_ids,
        suggested_capability_ids,
    )
    .await;
}

pub async fn spawn_daemon_api_agent_turn_with_scope(
    state: &AppState,
    job_id: String,
    session_id: String,
    prompt: String,
    response_depth_mode: String,
    reasoning_effort: String,
    provider: String,
    model: String,
    continuation_scope: crate::turn_continuation::TurnContinuationScope,
    manuscript_id: Option<String>,
    additional_manuscript_ids: Option<Vec<String>>,
    suggested_capability_ids: Option<Vec<String>>,
) {
    state.agent_turn_jobs.write().await.insert(
        job_id.clone(),
        AgentTurnJobRecord::pending(),
    );

    if crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&job_id) {
        crate::workspace::ask_job_store::ask_job_store().mark_running(&job_id);
    }

    let interactive_request = session_mapping::build_interactive_turn_request_for_ingest(
        &session_id,
        prompt,
        &provider,
        &model,
        &response_depth_mode,
        &reasoning_effort,
        None,
        manuscript_id,
        additional_manuscript_ids,
        suggested_capability_ids,
    );

    let agent_runtime = state.platform.agent_handle();
    let backend = state.backend.clone();
    let agent_turn_jobs = state.agent_turn_jobs.clone();
    let last_agent_turn_at = state.last_agent_turn_at.clone();
    let last_agent_turn_latency_ms = state.last_agent_turn_latency_ms.clone();
    let job_id_for_task = job_id.clone();
    let session_id_for_sink = session_id.clone();

    tokio::spawn(async move {
        let sink: Arc<dyn AgentStreamSink> = Arc::new(ApiAgentStreamSink {
            job_id: job_id_for_task.clone(),
            session_id: session_id_for_sink,
            agent_turn_jobs,
            last_agent_turn_at,
            last_agent_turn_latency_ms,
            started: std::time::Instant::now(),
        });

        crate::agent_runtime::run_agent_turn(
            &job_id_for_task,
            interactive_request,
            &backend,
            agent_runtime.as_ref(),
            sink,
            Some(continuation_scope),
            None,
        )
        .await;
    });
}

struct ApiAgentStreamSink {
    job_id: String,
    session_id: String,
    agent_turn_jobs: Arc<RwLock<HashMap<String, AgentTurnJobRecord>>>,
    last_agent_turn_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_agent_turn_latency_ms: Arc<RwLock<Option<u64>>>,
    started: std::time::Instant,
}

#[async_trait]
impl AgentStreamSink for ApiAgentStreamSink {
    async fn content_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn reasoning_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn agent_worker_ack(
        &self,
        _turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        _work_id: Option<String>,
    ) {
        crate::session::append_turn(
            &self.session_id,
            &crate::turn_parts::conversation_turn_from_parts(
                "assistant",
                text.clone(),
                tool_names.clone(),
                None,
                vec![crate::turn_parts::TurnPart::Text {
                    markdown: text.clone(),
                }],
            ),
        );

        if crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&self.job_id) {
            crate::workspace::ask_job_store::ask_job_store()
                .set_interim_text(&self.job_id, text);
            return;
        }

        self.agent_turn_jobs.write().await.insert(
            self.job_id.clone(),
            AgentTurnJobRecord {
                status: "running".to_string(),
                output_text: None,
                error: None,
                finished_at: None,
            },
        );
    }

    async fn agent_response(&self, _turn_id: u64, text: String, _tool_names: Vec<String>) {
        crate::session::append_turn(
            &self.session_id,
            &crate::session::ConversationTurn::plain(
                "assistant",
                text.clone(),
                Utc::now(),
                _tool_names,
                None,
            ),
        );

        if crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&self.job_id) {
            crate::workspace::ask_job_store::ask_job_store()
                .mark_succeeded(&self.job_id, text.clone());
        }

        let latency_ms = self.started.elapsed().as_millis() as u64;
        let now = Utc::now();
        self.agent_turn_jobs.write().await.insert(
            self.job_id.clone(),
            AgentTurnJobRecord {
                status: "succeeded".to_string(),
                output_text: Some(text),
                error: None,
                finished_at: Some(now),
            },
        );
        *self.last_agent_turn_at.write().await = Some(now);
        *self.last_agent_turn_latency_ms.write().await = Some(latency_ms);
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        if crate::workspace::ask_job_store::AskJobStore::is_ask_job_id(&self.job_id) {
            crate::workspace::ask_job_store::ask_job_store()
                .mark_failed(&self.job_id, message.clone());
        }

        let latency_ms = self.started.elapsed().as_millis() as u64;
        let now = Utc::now();
        self.agent_turn_jobs.write().await.insert(
            self.job_id.clone(),
            AgentTurnJobRecord {
                status: "failed".to_string(),
                output_text: None,
                error: Some(message),
                finished_at: Some(now),
            },
        );
        *self.last_agent_turn_at.write().await = Some(now);
        *self.last_agent_turn_latency_ms.write().await = Some(latency_ms);
    }

    async fn notice(&self, _message: String) {}

    async fn tool_invoked(&self, _tool_name: String, _input_summary: String) {}

    async fn tool_payload(
        &self,
        _tool_name: String,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        _output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
    ) {
    }
}

struct IngestAgentStreamSink {
    stream_id: String,
    session_id: String,
    job_id: String,
    stream: TurnStreamEntry,
    delivery_target: channel_delivery::ChannelDeliveryTarget,
    dispatch_client: reqwest::Client,
    delivery_records: Arc<RwLock<HashMap<String, channel_delivery::JobDeliveryRecord>>>,
    channel_deliveries: Arc<RwLock<HashMap<String, channel_delivery::ChannelDeliveryTarget>>>,
    last_delivery_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_delivery_latency_ms: Arc<RwLock<Option<u64>>>,
    cancelled_streams: Arc<RwLock<BoundedDedupSet>>,
    delivery_started: std::time::Instant,
    parts: std::sync::Mutex<crate::turn_parts::TurnPartsAccumulator>,
}

impl IngestAgentStreamSink {
    fn persist_assistant_turn(
        &self,
        content: String,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) {
        let turn = self
            .parts
            .lock()
            .map(|mut parts| parts.finalize_assistant_turn(content.clone(), tool_names.clone(), answer_state.clone()))
            .unwrap_or_else(|_| {
                crate::turn_parts::conversation_turn_from_parts(
                    "assistant",
                    content,
                    tool_names,
                    answer_state,
                    vec![],
                )
            });
        crate::session::append_turn(&self.session_id, &turn);
    }
}

#[async_trait]
impl AgentStreamSink for IngestAgentStreamSink {
    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::content_delta_stream_event(&self.stream_id, &delta),
        );
    }

    async fn reasoning_chunk(&self, _turn_id: u64, delta: String) {
        if let Ok(mut parts) = self.parts.lock() {
            parts.push_reasoning_delta(&delta);
        }
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::reasoning_delta_stream_event(
                &self.stream_id,
                &delta,
            ),
        );
    }

    async fn agent_worker_ack(
        &self,
        _turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            return;
        }

        let assistant_turn = self
            .parts
            .lock()
            .map(|mut parts| {
                parts.finalize_worker_ack_turn(text.clone(), tool_names.clone(), work_id.clone())
            })
            .unwrap_or_else(|_| {
                crate::turn_parts::conversation_turn_from_parts(
                    "assistant",
                    text.clone(),
                    tool_names.clone(),
                    Some("worker_ack".to_string()),
                    vec![crate::turn_parts::TurnPart::Text {
                        markdown: text.clone(),
                    }],
                )
            });
        crate::session::append_turn(&self.session_id, &assistant_turn);

        if crate::channel_delivery::is_external_push_channel(&self.delivery_target.channel) {
            let payload = crate::turn_worker_notify::TurnWorkerSpawnNotifyPayload {
                work_id: work_id.clone().unwrap_or_else(|| self.job_id.clone()),
                user_ack: text.clone(),
                intent: None,
            };
            if let Err(err) = crate::turn_worker_notify::notify_turn_worker_spawned(
                &self.dispatch_client,
                &self.delivery_target,
                payload,
            )
            .await
            {
                eprintln!(
                    "ingest worker spawn channel notify failed job_id={} channel={}: {err:#}",
                    self.job_id, self.delivery_target.channel
                );
            }
        }

        if let Ok(event) = crate::interactive_turn_runtime::worker_ack_stream_event_with_tools(
            &self.stream_id,
            &text,
            tool_names,
            work_id.as_deref(),
        ) {
            publish_interactive_turn_event(&self.stream, Ok(event));
        }
    }

    async fn agent_final_pending(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            return;
        }

        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::turn_progress_stream_event(
                &self.stream_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_turn_progress(&self, _turn_id: u64, message: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            return;
        }

        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::turn_progress_stream_event(
                &self.stream_id,
                &message,
                tool_names,
            ),
        );
    }

    async fn agent_needs_input(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            publish_interactive_turn_event(
                &self.stream,
                crate::interactive_turn_runtime::error_stream_event(
                    &self.stream_id,
                    "ingest turn cancelled by /stop",
                ),
            );
            return;
        }

        self.persist_assistant_turn(
            text.clone(),
            tool_names.clone(),
            Some("needs_input".to_string()),
        );

        let latency_ms = self.delivery_started.elapsed().as_millis() as u64;
        let delivery_text = crate::agent_runtime::format_channel_delivery_text(
            &text,
            &tool_names,
            &self.delivery_target.channel,
        );
        if let Err(err) = channel_delivery::dispatch_channel_message(
            &self.dispatch_client,
            &self.delivery_target,
            &delivery_text,
        )
        .await
        {
            eprintln!(
                "ingest agent turn channel dispatch failed job_id={} channel={}: {err:#}",
                self.job_id, self.delivery_target.channel
            );
            mark_job_delivery_success(
                &self.job_id,
                latency_ms,
                Some(err.to_string()),
                &self.delivery_records,
                &self.last_delivery_at,
                &self.last_delivery_latency_ms,
            )
            .await;
        } else {
            mark_job_delivery_success(
                &self.job_id,
                latency_ms,
                None,
                &self.delivery_records,
                &self.last_delivery_at,
                &self.last_delivery_latency_ms,
            )
            .await;
        }

        self.channel_deliveries.write().await.remove(&self.job_id);

        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::needs_input_stream_event_with_tools(
                &self.stream_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            publish_interactive_turn_event(
                &self.stream,
                crate::interactive_turn_runtime::error_stream_event(
                    &self.stream_id,
                    "ingest turn cancelled by /stop",
                ),
            );
            return;
        }

        self.persist_assistant_turn(text.clone(), tool_names.clone(), None);

        let latency_ms = self.delivery_started.elapsed().as_millis() as u64;
        let delivery_text = crate::agent_runtime::format_channel_delivery_text(
            &text,
            &tool_names,
            &self.delivery_target.channel,
        );
        if let Err(err) = channel_delivery::dispatch_channel_message(
            &self.dispatch_client,
            &self.delivery_target,
            &delivery_text,
        )
        .await
        {
            eprintln!(
                "ingest agent turn channel dispatch failed job_id={} channel={}: {err:#}",
                self.job_id, self.delivery_target.channel
            );
            mark_job_delivery_success(
                &self.job_id,
                latency_ms,
                Some(err.to_string()),
                &self.delivery_records,
                &self.last_delivery_at,
                &self.last_delivery_latency_ms,
            )
            .await;
        } else {
            mark_job_delivery_success(
                &self.job_id,
                latency_ms,
                None,
                &self.delivery_records,
                &self.last_delivery_at,
                &self.last_delivery_latency_ms,
            )
            .await;
        }

        self.channel_deliveries.write().await.remove(&self.job_id);

        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::final_stream_event_with_tools(
                &self.stream_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        let failure = crate::turn_failure::TurnFailure::from_debug(&message);
        let latency_ms = self.delivery_started.elapsed().as_millis() as u64;
        let user_message = format!("Sorry — {}", failure.operator_message);
        let _ = channel_delivery::dispatch_channel_message(
            &self.dispatch_client,
            &self.delivery_target,
            &user_message,
        )
        .await;
        mark_job_delivery_success(
            &self.job_id,
            latency_ms,
            Some(failure.debug_message.clone()),
            &self.delivery_records,
            &self.last_delivery_at,
            &self.last_delivery_latency_ms,
        )
        .await;
        self.channel_deliveries.write().await.remove(&self.job_id);

        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::error_stream_event_from_failure(
                &self.stream_id,
                &failure,
            ),
        );
    }

    async fn notice(&self, message: String) {
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::debug_status_stream_event(
                &self.stream_id,
                "orchestration",
                &message,
            ),
        );
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::debug_status_stream_event(
                &self.stream_id,
                "tool",
                &format!("tool={tool_name} {input_summary}"),
            ),
        );
    }

    async fn tool_run_started(
        &self,
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        tool_round: usize,
    ) {
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_started(&tool_run_id, &tool_name, &input_summary, tool_round);
        }
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::tool_started_stream_event(
                &self.stream_id,
                &tool_run_id,
                &tool_name,
                &input_summary,
                tool_round,
            ),
        );
    }

    async fn tool_run_finished(
        &self,
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        output_summary: Option<String>,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        tool_round: usize,
    ) {
        let safe_input = crate::settings_guard::redact_json_value(&tool_input);
        let safe_output = crate::settings_guard::redact_json_value(&tool_output);
        let input_receipt = input_receipt.or_else(|| {
            crate::payload_receipt::receipt_meta(
                &safe_input,
                crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
            )
        });
        let output_receipt = output_receipt.or_else(|| {
            crate::payload_receipt::receipt_meta(
                &safe_output,
                crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
            )
        });
        let artifact_refs = crate::agent_runtime::tool_stream::artifact_refs_from_receipts(
            input_receipt.as_ref(),
            output_receipt.as_ref(),
        );
        if let Ok(mut parts) = self.parts.lock() {
            parts.tool_finished(
                &tool_run_id,
                &status,
                output_summary.clone(),
                crate::turn_parts::artifact_refs_from_stream(&artifact_refs),
            );
        }
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::tool_finished_stream_event(
                &self.stream_id,
                &tool_run_id,
                &tool_name,
                &status,
                &input_summary,
                output_summary.as_deref(),
                tool_round,
                artifact_refs,
            ),
        );
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        _output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
    ) {
        publish_interactive_turn_event(
            &self.stream,
            crate::interactive_turn_runtime::status_stream_event(
                &self.stream_id,
                "tool",
                &format!("tool_payload={tool_name}"),
            ),
        );
    }
}

async fn start_ingest_ask_stream(
    state: &AppState,
    mapping_key: &str,
    session_id: &str,
    prompt: String,
    manuscript_id: Option<String>,
    request: &IngestRequest,
) -> Result<IngestAskStream, (StatusCode, String)> {
    let runtime_config = resolve_session_runtime_config(state, session_id).await;

    let now = Utc::now();
    let job_id_str = format!(
        "medousa-daemon-ingest-{}-{}",
        now.timestamp_millis(),
        &session_id[..8.min(session_id.len())]
    );

    let interactive_request = session_mapping::build_interactive_turn_request_for_ingest(
        session_id,
        prompt,
        &runtime_config.draft_provider,
        &runtime_config.draft_model,
        &runtime_config.response_depth_mode,
        &runtime_config.reasoning_effort,
        Some(request),
        manuscript_id,
        None,
        None,
    );

    let stream_id = format!("ingest-{}", Uuid::new_v4().simple());
    {
        let port = crate::engine_adapters::turn_stream_registry_adapter(
            state.interactive_turn_streams.clone(),
        );
        port.register_stream(&stream_id).await;
    }
    let stream_entry = state
        .interactive_turn_streams
        .read()
        .await
        .get(&stream_id)
        .cloned()
        .expect("ingest stream registered");
    let stream_url =
        crate::ingest_stream::build_ingest_stream_url(&state.daemon_base_url, &stream_id);

    state.active_ingest_jobs.write().await.insert(
        mapping_key.to_string(),
        session_mapping::ActiveIngestJob {
            job_id: job_id_str.clone(),
            stream_id: stream_id.clone(),
            channel: request.channel.clone(),
            user_id: request.user_id.clone(),
            channel_id: request.channel_id.clone(),
            session_id: session_id.to_string(),
        },
    );
    state.channel_deliveries.write().await.insert(
        job_id_str.clone(),
        channel_delivery::ChannelDeliveryTarget {
            channel: request.channel.clone(),
            user_id: request.user_id.clone(),
            channel_id: request.channel_id.clone(),
            session_id: session_id.to_string(),
            stream_id: Some(stream_id.clone()),
        },
    );
    record_job_delivery_pending(state, &job_id_str).await;

    let stream_registry = state.interactive_turn_streams.clone();
    let cancelled_streams = state.cancelled_ingest_streams.clone();
    let cancelled_streams_cleanup = state.cancelled_ingest_streams.clone();
    let agent_runtime = state.platform.agent_handle();
    let backend = state.backend.clone();
    let dispatch_client = state.channel_dispatch_client.clone();
    let delivery_records = state.job_delivery_records.clone();
    let channel_deliveries = state.channel_deliveries.clone();
    let last_delivery_at = state.last_delivery_at.clone();
    let last_delivery_latency_ms = state.last_delivery_latency_ms.clone();
    let active_jobs = state.active_ingest_jobs.clone();
    let mapping_key_for_cleanup = mapping_key.to_string();
    let stream_id_for_task = stream_id.clone();
    let stream_id_for_cleanup = stream_id.clone();
    let session_id_owned = session_id.to_string();
    let delivery_target = channel_delivery::ChannelDeliveryTarget {
        channel: request.channel.clone(),
        user_id: request.user_id.clone(),
        channel_id: request.channel_id.clone(),
        session_id: session_id_owned.clone(),
        stream_id: Some(stream_id.clone()),
    };

    let job_id_for_sink = job_id_str.clone();
    let continuation_scope = crate::turn_continuation::TurnContinuationScope {
        turn_correlation_id: job_id_str.clone(),
        session_id: session_id_owned.clone(),
        original_prompt: interactive_request.prompt.clone(),
        delivery_target: Some(channel_delivery::ChannelDeliveryTarget {
            channel: request.channel.clone(),
            user_id: request.user_id.clone(),
            channel_id: request.channel_id.clone(),
            session_id: session_id_owned.clone(),
            stream_id: Some(stream_id.clone()),
        }),
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
    tokio::spawn(async move {
        // Brief guard so the client's SSE subscribe wins the race against the first
        // (cosmetic) status event; answer tokens arrive far later regardless.
        tokio::time::sleep(Duration::from_millis(25)).await;

        publish_interactive_turn_event(
            &stream_entry,
            crate::interactive_turn_runtime::status_stream_event(
                &stream_id_for_task,
                "accepted",
                "ingest accepted; agent runtime started",
            ),
        );

        let sink: Arc<dyn AgentStreamSink> = Arc::new(IngestAgentStreamSink {
            stream_id: stream_id_for_task.clone(),
            session_id: session_id_owned,
            job_id: job_id_for_sink,
            stream: stream_entry.clone(),
            delivery_target,
            dispatch_client,
            delivery_records,
            channel_deliveries,
            last_delivery_at,
            last_delivery_latency_ms,
            cancelled_streams,
            delivery_started: std::time::Instant::now(),
            parts: std::sync::Mutex::new(crate::turn_parts::TurnPartsAccumulator::default()),
        });

        crate::agent_runtime::run_agent_turn(
            &stream_id_for_task,
            interactive_request,
            &backend,
            agent_runtime.as_ref(),
            sink,
            Some(continuation_scope),
            None,
        )
        .await;

        active_jobs
            .write()
            .await
            .remove(&mapping_key_for_cleanup);

        // The cancellation tombstone is only meaningful while this stream runs;
        // drop it now that the turn is finalized (the bounded set also caps it).
        cancelled_streams_cleanup
            .write()
            .await
            .remove(&stream_id_for_cleanup);

        // Mark the channel closed but keep it (and its replay buffer) in the
        // registry for a grace window so a client reconnecting right at the end
        // can still replay the terminal event with `?since=`.
        stream_entry.channel.mark_closed();
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut guard = stream_registry.write().await;
        guard.remove(&stream_id_for_cleanup);
    });

    Ok(IngestAskStream {
        job_id: job_id_str,
        stream_id,
        stream_url,
    })
}

pub async fn get_job_attempts_graceful(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> std::result::Result<Vec<stasis::domain::runtime::job_attempt::JobAttempt>, (StatusCode, String)> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt
            .job_attempt_store
            .list_by_job_id(job_id)
            .await
            .map_err(internal_error),
        RuntimeComposition::Surreal(rt) => {
            match rt.job_attempt_store.list_by_job_id(job_id).await {
                Ok(attempts) => Ok(attempts),
                Err(err) => {
                    if is_missing_runtime_table_error(&err.to_string()) {
                        Ok(Vec::new())
                    } else {
                        Err(internal_error(err))
                    }
                }
            }
        }
    }
}
