use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use medousa::engine_context::{
    EngineExecutionLane, HeartbeatAction, HeartbeatSignals, LaneSafetyActionClass,
    compile_default_lane_prompt, default_heartbeat_lane_policy,
    default_policy_profile_for_lane, evaluate_heartbeat_significance,
    validate_lane_action, validate_lane_policy_profile,
};
use medousa::identity_memory::{
    resolve_identity_channel_id, resolve_identity_persona_id, resolve_identity_user_id,
};
use medousa::daemon_api::{
    DEFAULT_DAEMON_BIND, DaemonStatsResponse, EnqueueAskRequest, EnqueuePromptRequest,
    EnqueueResponse, HealthResponse, IdentityContextRequest, RegisterRecurringPromptRequest,
    RegisterRecurringResponse,
};
use medousa::{build_runtime_with_identity_store, parse_backend};
use serde::Serialize;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, watch};
use uuid::Uuid;

use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::application::runtime::identity_context_compiler::prepend_identity_snapshot;
use stasis::application::orchestration::runtime_job_payloads::{
    AgentSessionJobPayload, AgentSessionParticipantPayload, AgentToolCallMode, PromptJobPayload,
};
use stasis::application::orchestration::runtime_workflow_job_builder::RuntimeWorkflowJobBuilder;
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, CommitEntityUpdateResponse, GetIdentityContextRequest,
    GetIdentityContextResponse, ListEntityHistoryRequest, ListEntityHistoryResponse,
    ProposeEntityUpdateRequest, ProposeEntityUpdateResponse, RollbackEntityVersionRequest,
    RollbackEntityVersionResponse,
};
use stasis::dashboard::{
    DashboardState, InMemoryDashboardQueryService, router as dashboard_router,
};
use stasis::prelude::{RecurringDefinition, RuntimeComposition, RuntimeSdk};
use stasis::prelude_ext::{
    CompositeControlPlaneStore, ControlPlaneSdk, InMemoryClusterNodeStore,
    InMemoryDeliveryEndpointStore,
};

#[derive(Clone)]
struct AppState {
    runtime: Arc<RuntimeComposition>,
    backend: String,
    worker_id: String,
    identity_service: Arc<IdentityMemoryService>,
    identity_default_user_id: String,
    last_tick_at: Arc<RwLock<Option<chrono::DateTime<Utc>>>>,
    heartbeat_notify: HeartbeatNotifyConfig,
    webhook_client: Option<reqwest::Client>,
}

#[derive(Debug)]
struct TickReport {
    materialized: usize,
    processed_job: Option<String>,
    published: usize,
    lane: EngineExecutionLane,
    lane_policy_profile: &'static str,
    heartbeat_action: HeartbeatAction,
    heartbeat_significance: f32,
    heartbeat_reason: String,
    failed_jobs: usize,
    dead_letter_jobs: usize,
    pending_outbox_events: usize,
}

#[derive(Clone, Debug)]
struct HeartbeatNotifyConfig {
    webhook_url: Option<String>,
    jsonl_path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct HeartbeatNotification {
    timestamp_utc: chrono::DateTime<Utc>,
    backend: String,
    worker_id: String,
    lane: String,
    lane_policy_profile: String,
    heartbeat_action: String,
    heartbeat_significance: f32,
    heartbeat_reason: String,
    materialized_jobs: usize,
    processed_job: Option<String>,
    published_events: usize,
    failed_jobs: usize,
    dead_letter_jobs: usize,
    pending_outbox_events: usize,
}

#[derive(Debug, Clone)]
struct ResolvedIdentityContext {
    user_id: String,
    summary: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();

    let backend_name = find_arg_value(&args, "--backend")
        .unwrap_or("in-memory")
        .to_string();
    let backend = parse_backend(Some(&backend_name));
    let provider = find_arg_value(&args, "--provider");
    let model = find_arg_value(&args, "--model");
    let base_url = find_arg_value(&args, "--base-url");
    let bind = find_arg_value(&args, "--bind").unwrap_or(DEFAULT_DAEMON_BIND);
    let interval_ms = find_arg_value(&args, "--interval-ms")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1000);
    let once = args.iter().any(|arg| arg == "--once");
    let worker_id = find_arg_value(&args, "--worker-id")
        .unwrap_or("medousa-daemon")
        .to_string();
    let heartbeat_notify = HeartbeatNotifyConfig {
        webhook_url: parse_arg_or_env(
            &args,
            "--heartbeat-webhook-url",
            "MEDOUSA_HEARTBEAT_WEBHOOK_URL",
        ),
        jsonl_path: parse_arg_or_env(
            &args,
            "--heartbeat-jsonl",
            "MEDOUSA_HEARTBEAT_JSONL",
        )
        .map(PathBuf::from),
    };

    let webhook_client = heartbeat_notify
        .webhook_url
        .as_ref()
        .map(|_| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .build()
                .context("failed to build heartbeat webhook client")
        })
        .transpose()?;

    let identity_store = medousa::identity_memory::build_identity_memory_store_for_backend(&backend)
        .await?;
    let identity_service = Arc::new(IdentityMemoryService::new(identity_store.clone()));
    let identity_default_user_id = resolve_identity_user_id(None);

    let runtime = Arc::new(
        build_runtime_with_identity_store(backend, provider, model, base_url, Some(identity_store))
            .await?,
    );

    if once {
        let report = tick_runtime(runtime.as_ref(), &worker_id).await?;
        println!("{}", format_tick_report("medousa-daemon once", &report));

        if report.heartbeat_action == HeartbeatAction::Notify {
            dispatch_heartbeat_notifications(
                &heartbeat_notify,
                webhook_client.as_ref(),
                &backend_name,
                &worker_id,
                &report,
            )
            .await;
        }

        return Ok(());
    }

    let state = AppState {
        runtime: runtime.clone(),
        backend: backend_name,
        worker_id: worker_id.clone(),
        identity_service,
        identity_default_user_id,
        last_tick_at: Arc::new(RwLock::new(None)),
        heartbeat_notify,
        webhook_client,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/stats", get(stats))
        .route("/v1/jobs/ask", post(enqueue_ask))
        .route("/v1/jobs/prompt", post(enqueue_prompt))
        .route("/v1/recurring/prompt", post(register_recurring_prompt))
        .route("/v1/identity/context", post(identity_get_context))
        .route("/v1/identity/update/propose", post(identity_propose_update))
        .route("/v1/identity/update/commit", post(identity_commit_update))
        .route("/v1/identity/history", post(identity_list_history))
        .route("/v1/identity/rollback", post(identity_rollback_version))
        .with_state(state.clone());

    let app = if let RuntimeComposition::InMemory(ref inner) = *runtime {
        let inner_arc = Arc::new(inner.clone());
        let endpoint_store = InMemoryDeliveryEndpointStore::default();
        let cluster_store = InMemoryClusterNodeStore::default();
        let control_store = CompositeControlPlaneStore::new(endpoint_store, cluster_store);
        let control_plane = ControlPlaneSdk::new(control_store);
        let dashboard_service =
            Arc::new(InMemoryDashboardQueryService::new(inner_arc, control_plane));
        let dashboard = dashboard_router(DashboardState::new(dashboard_service));
        app.merge(dashboard)
    } else {
        println!("medousa-daemon dashboard skipped (only supported for in-memory backend)");
        app
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let scheduler_state = state.clone();
    let scheduler_worker_id = worker_id.clone();
    tokio::spawn(async move {
        run_scheduler_loop(
            scheduler_state,
            scheduler_worker_id,
            interval_ms,
            shutdown_rx,
        )
        .await;
    });

    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid --bind address: {bind}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind medousa daemon on {addr}"))?;

    println!("medousa-daemon listening on http://{addr}");
    println!("medousa-daemon dashboard at http://{addr}/dashboard");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(true);
            println!("medousa-daemon stopping");
        })
        .await
        .context("medousa-daemon server failed")?;

    Ok(())
}

async fn run_scheduler_loop(
    state: AppState,
    worker_id: String,
    interval_ms: u64,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    loop {
        match tick_runtime(state.runtime.as_ref(), &worker_id).await {
            Ok(report) => {
                *state.last_tick_at.write().await = Some(Utc::now());
                if report.materialized > 0
                    || report.processed_job.is_some()
                    || report.published > 0
                    || report.heartbeat_action == HeartbeatAction::Notify
                {
                    eprintln!("{}", format_tick_report("medousa-daemon tick", &report));
                }

                if report.heartbeat_action == HeartbeatAction::Notify {
                    dispatch_heartbeat_notifications(
                        &state.heartbeat_notify,
                        state.webhook_client.as_ref(),
                        &state.backend,
                        &state.worker_id,
                        &report,
                    )
                    .await;
                }
            }
            Err(err) => {
                eprintln!("medousa-daemon scheduler tick error: {err}");
            }
        }

        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {}
        }
    }
}

async fn tick_runtime(runtime: &RuntimeComposition, worker_id: &str) -> Result<TickReport> {
    let sdk = RuntimeSdk::new(runtime.clone());
    let lane = EngineExecutionLane::Scheduled;
    let lane_policy_profile = default_policy_profile_for_lane(lane);
    let lane_worker_id = format!("{worker_id}:{}", lane.as_str());

    let materialized = sdk.materialize_recurring_now(&lane_worker_id).await?;
    let processed_job = sdk.process_once("default", &lane_worker_id).await?;
    let published = sdk.publish_pending_events(200).await?;
    let snapshot = sdk.stats_snapshot(200).await?;

    let heartbeat_decision = evaluate_heartbeat_significance(
        &HeartbeatSignals {
            materialized_jobs: materialized,
            processed_job: processed_job.is_some(),
            published_events: published,
            failed_jobs: snapshot.failed_jobs,
            dead_letter_jobs: snapshot.dead_letter_jobs,
            pending_outbox_events: snapshot.pending_outbox_events,
        },
        default_heartbeat_lane_policy(),
    );

    Ok(TickReport {
        materialized,
        processed_job,
        published,
        lane,
        lane_policy_profile,
        heartbeat_action: heartbeat_decision.action,
        heartbeat_significance: heartbeat_decision.significance,
        heartbeat_reason: heartbeat_decision.reason,
        failed_jobs: snapshot.failed_jobs,
        dead_letter_jobs: snapshot.dead_letter_jobs,
        pending_outbox_events: snapshot.pending_outbox_events,
    })
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        backend: state.backend,
        worker_id: state.worker_id,
        now_utc: Utc::now(),
    })
}

async fn stats(
    State(state): State<AppState>,
) -> Result<Json<DaemonStatsResponse>, (StatusCode, String)> {
    let sdk = RuntimeSdk::new(state.runtime.as_ref().clone());
    let snapshot = sdk.stats_snapshot(5000).await.map_err(internal_error)?;

    let last_tick_at_utc = *state.last_tick_at.read().await;

    Ok(Json(DaemonStatsResponse {
        enqueued_jobs: snapshot.enqueued_jobs,
        running_jobs: snapshot.running_jobs,
        succeeded_jobs: snapshot.succeeded_jobs,
        failed_jobs: snapshot.failed_jobs,
        dead_letter_jobs: snapshot.dead_letter_jobs,
        pending_outbox_events: snapshot.pending_outbox_events,
        recurring_definitions: snapshot.recurring_definitions,
        last_tick_at_utc,
    }))
}

async fn enqueue_ask(
    State(state): State<AppState>,
    Json(request): Json<EnqueueAskRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    if request.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    }

    enforce_lane_safety(
        EngineExecutionLane::Interactive,
        LaneSafetyActionClass::InteractiveIngress,
        request.policy_profile.as_deref(),
    )?;

    let effective_policy_profile = request.policy_profile.unwrap_or_else(|| {
        default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
    });
    let identity_context = resolve_identity_context_for_request(
        &state,
        request.identity_user_id.as_deref(),
        request.identity_persona_id.as_deref(),
        request.identity_channel_id.as_deref(),
        Some(effective_policy_profile.as_str()),
        8,
    )
    .await?;

    let now = Utc::now();
    let job_id = format!("medousa-daemon-ask-{}", now.timestamp_millis());
    let raw_prompt = request.prompt;
    let prompt_with_identity = prepend_identity_snapshot(&raw_prompt, Some(&identity_context.summary));
    let compiled_prompt = compile_lane_prompt(EngineExecutionLane::Interactive, &prompt_with_identity);

    let payload = AgentSessionJobPayload {
        thread_id: Some(job_id.clone()),
        initial_user_prompt: compiled_prompt,
        participants: vec![AgentSessionParticipantPayload {
            agent_id: "medousa.researcher".to_string(),
            system_prompt: Some(
                "You are Medousa, a practical research assistant. Use tool evidence and cite findings succinctly.".to_string(),
            ),
            tool_name: "stasis.web.search.mock".to_string(),
            tool_input: Some(serde_json::json!({
                "query": raw_prompt
            })),
        }],
        policy_profile: Some(effective_policy_profile),
        model_hint: request.model_hint,
        memory_policy: None,
        max_turns: request.max_turns.map(|value| value as usize),
        tool_call_mode: Some(AgentToolCallMode::Auto),
    };

    let new_job = RuntimeWorkflowJobBuilder::for_agent_session(job_id.clone(), &payload)
        .map_err(internal_error)?
        .with_correlation_id(identity_context.user_id)
        .with_causation_id("medousa-daemon-api:interactive")
        .with_sttp_input_node_id("sttp:in:medousa:daemon:interactive:ask")
        .with_scheduled_at(now)
        .build();

    enqueue_runtime_job(state.runtime.as_ref(), new_job)
        .await
        .map_err(internal_error)?;

    Ok(Json(EnqueueResponse {
        job_id,
        queue: "default".to_string(),
        accepted_at_utc: now,
    }))
}

async fn enqueue_prompt(
    State(state): State<AppState>,
    Json(request): Json<EnqueuePromptRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    if request.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    }

    enforce_lane_safety(
        EngineExecutionLane::Interactive,
        LaneSafetyActionClass::InteractiveIngress,
        request.policy_profile.as_deref(),
    )?;

    let effective_policy_profile = request.policy_profile.unwrap_or_else(|| {
        default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
    });
    let identity_context = resolve_identity_context_for_request(
        &state,
        request.identity_user_id.as_deref(),
        request.identity_persona_id.as_deref(),
        request.identity_channel_id.as_deref(),
        Some(effective_policy_profile.as_str()),
        8,
    )
    .await?;

    let now = Utc::now();
    let job_id = format!("medousa-daemon-prompt-{}", now.timestamp_millis());
    let prompt_with_identity = prepend_identity_snapshot(&request.prompt, Some(&identity_context.summary));
    let compiled_prompt = compile_lane_prompt(EngineExecutionLane::Interactive, &prompt_with_identity);

    let payload = PromptJobPayload {
        user_prompt: compiled_prompt,
        system_prompt: request.system_prompt.or(Some(
            "You are Medousa, a practical assistant. Be concise and structured.".to_string(),
        )),
        policy_profile: Some(effective_policy_profile),
        model_hint: request.model_hint,
        memory_policy: None,
    };

    let new_job = RuntimeWorkflowJobBuilder::for_prompt(job_id.clone(), &payload)
        .map_err(internal_error)?
        .with_correlation_id(identity_context.user_id)
        .with_causation_id("medousa-daemon-api:interactive")
        .with_sttp_input_node_id("sttp:in:medousa:daemon:interactive:prompt")
        .with_scheduled_at(now)
        .build();

    enqueue_runtime_job(state.runtime.as_ref(), new_job)
        .await
        .map_err(internal_error)?;

    Ok(Json(EnqueueResponse {
        job_id,
        queue: "default".to_string(),
        accepted_at_utc: now,
    }))
}

async fn register_recurring_prompt(
    State(state): State<AppState>,
    Json(request): Json<RegisterRecurringPromptRequest>,
) -> Result<Json<RegisterRecurringResponse>, (StatusCode, String)> {
    if request.prompt.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "prompt is required".to_string()));
    }
    if request.cron_expr.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "cron_expr is required".to_string()));
    }

    enforce_lane_safety(
        EngineExecutionLane::Scheduled,
        LaneSafetyActionClass::RecurringRegistration,
        request.policy_profile.as_deref(),
    )?;

    let now = Utc::now();
    let queue = request.queue.unwrap_or_else(|| "default".to_string());
    let timezone = request.timezone.unwrap_or_else(|| "UTC".to_string());
    let recurring_id = request
        .id
        .unwrap_or_else(|| format!("medousa-recurring-{}", Uuid::new_v4().simple()));
    let compiled_prompt = compile_lane_prompt(EngineExecutionLane::Scheduled, &request.prompt);

    let prompt_payload = PromptJobPayload {
        user_prompt: compiled_prompt,
        system_prompt: request.system_prompt,
        policy_profile: request.policy_profile.or_else(|| {
            Some(default_policy_profile_for_lane(EngineExecutionLane::Scheduled).to_string())
        }),
        model_hint: request.model_hint,
        memory_policy: None,
    };

    let payload_template_ref = prompt_payload.to_payload_ref().map_err(internal_error)?;

    let mut definition = RecurringDefinition {
        id: recurring_id.clone(),
        queue: queue.clone(),
        job_type: "workflow.stasis.prompt".to_string(),
        payload_template_ref,
        cron_expr: request.cron_expr.clone(),
        timezone: timezone.clone(),
        jitter_seconds: request.jitter_seconds.unwrap_or(0),
        enabled: request.enabled.unwrap_or(true),
        max_attempts: request.max_attempts.unwrap_or(1),
        next_run_at: now,
        last_run_at: None,
        lease_owner: None,
        lease_expires_at: None,
    };

    definition.next_run_at = definition
        .compute_next_run_at(now)
        .map_err(internal_error)?;

    let sdk = RuntimeSdk::new(state.runtime.as_ref().clone());
    sdk.register_recurring(definition.clone())
        .await
        .map_err(internal_error)?;

    Ok(Json(RegisterRecurringResponse {
        recurring_id,
        queue,
        next_run_at_utc: definition.next_run_at,
        cron_expr: definition.cron_expr,
        timezone,
    }))
}

async fn resolve_identity_context_for_request(
    state: &AppState,
    user_id_override: Option<&str>,
    persona_id_override: Option<&str>,
    channel_id_override: Option<&str>,
    policy_profile: Option<&str>,
    relationship_limit: usize,
) -> Result<ResolvedIdentityContext, (StatusCode, String)> {
    let user_id = normalize_optional_text(user_id_override)
        .unwrap_or_else(|| state.identity_default_user_id.clone());
    let persona_id = normalize_optional_text(persona_id_override)
        .unwrap_or_else(resolve_identity_persona_id);
    let channel_id = normalize_optional_text(channel_id_override)
        .unwrap_or_else(|| resolve_identity_channel_id(policy_profile));

    let response = state
        .identity_service
        .get_identity_context(&GetIdentityContextRequest {
            user_id: user_id.clone(),
            persona_id,
            channel_id,
            relationship_limit: relationship_limit.clamp(1, 64),
        })
        .await
        .map_err(internal_error)?;

    Ok(ResolvedIdentityContext {
        user_id,
        summary: summarize_identity_context(&response),
    })
}

fn summarize_identity_context(context: &GetIdentityContextResponse) -> String {
    let continuity_links = context
        .relationships
        .iter()
        .filter(|relationship| relationship.derived_from_relationship_id.is_some())
        .count();
    let continuity_receipts = context
        .relationships
        .iter()
        .filter(|relationship| relationship.transition_receipt_id.is_some())
        .count();

    format!(
        "persona_present={} user_present={} channel_present={} relationships={} policies={} depth={} continuity_links={} continuity_receipts={}",
        context.persona.is_some(),
        context.user.is_some(),
        context.channel.is_some(),
        context.relationships.len(),
        context.policy_profiles.len(),
        context.graph_depth_used,
        continuity_links,
        continuity_receipts,
    )
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(ToString::to_string)
}

async fn identity_get_context(
    State(state): State<AppState>,
    Json(request): Json<IdentityContextRequest>,
) -> Result<Json<GetIdentityContextResponse>, (StatusCode, String)> {
    let user_id = normalize_optional_text(request.user_id.as_deref())
        .unwrap_or_else(|| state.identity_default_user_id.clone());
    let persona_id = normalize_optional_text(request.persona_id.as_deref())
        .unwrap_or_else(resolve_identity_persona_id);
    let channel_id = normalize_optional_text(request.channel_id.as_deref())
        .unwrap_or_else(|| resolve_identity_channel_id(request.policy_profile.as_deref()));
    let relationship_limit = request.relationship_limit.unwrap_or(8).clamp(1, 64);

    let response = state
        .identity_service
        .get_identity_context(&GetIdentityContextRequest {
            user_id,
            persona_id,
            channel_id,
            relationship_limit,
        })
        .await
        .map_err(internal_error)?;

    Ok(Json(response))
}

async fn identity_propose_update(
    State(state): State<AppState>,
    Json(request): Json<ProposeEntityUpdateRequest>,
) -> Result<Json<ProposeEntityUpdateResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .propose_entity_update(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

async fn identity_commit_update(
    State(state): State<AppState>,
    Json(request): Json<CommitEntityUpdateRequest>,
) -> Result<Json<CommitEntityUpdateResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .commit_entity_update(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

async fn identity_list_history(
    State(state): State<AppState>,
    Json(request): Json<ListEntityHistoryRequest>,
) -> Result<Json<ListEntityHistoryResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .list_entity_history(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

async fn identity_rollback_version(
    State(state): State<AppState>,
    Json(request): Json<RollbackEntityVersionRequest>,
) -> Result<Json<RollbackEntityVersionResponse>, (StatusCode, String)> {
    let response = state
        .identity_service
        .rollback_entity_version(&request)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

async fn enqueue_runtime_job(
    runtime: &RuntimeComposition,
    job: stasis::prelude::NewJob,
) -> Result<()> {
    let sdk = RuntimeSdk::new(runtime.clone());
    sdk.enqueue(job).await?;
    Ok(())
}

fn enforce_lane_safety(
    lane: EngineExecutionLane,
    action: LaneSafetyActionClass,
    policy_profile: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    if let Err(reason) = validate_lane_action(lane, action) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("lane safety violation: {reason}"),
        ));
    }

    if let Err(reason) = validate_lane_policy_profile(lane, policy_profile) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("lane safety violation: {reason}"),
        ));
    }

    Ok(())
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("medousa daemon error: {err}"),
    )
}

fn find_arg_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let idx = args.iter().position(|arg| arg == key)?;
    args.get(idx + 1).map(|s| s.as_str())
}

fn parse_arg_or_env(args: &[String], arg_key: &str, env_key: &str) -> Option<String> {
    find_arg_value(args, arg_key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            std::env::var(env_key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

async fn dispatch_heartbeat_notifications(
    notify: &HeartbeatNotifyConfig,
    webhook_client: Option<&reqwest::Client>,
    backend: &str,
    worker_id: &str,
    report: &TickReport,
) {
    if let Err(reason) = validate_lane_action(
        report.lane,
        LaneSafetyActionClass::HeartbeatNotificationDispatch,
    ) {
        eprintln!("medousa-daemon heartbeat dispatch blocked: {reason}");
        return;
    }

    let notification = HeartbeatNotification {
        timestamp_utc: Utc::now(),
        backend: backend.to_string(),
        worker_id: worker_id.to_string(),
        lane: report.lane.as_str().to_string(),
        lane_policy_profile: report.lane_policy_profile.to_string(),
        heartbeat_action: report.heartbeat_action.as_str().to_string(),
        heartbeat_significance: report.heartbeat_significance,
        heartbeat_reason: report.heartbeat_reason.clone(),
        materialized_jobs: report.materialized,
        processed_job: report.processed_job.clone(),
        published_events: report.published,
        failed_jobs: report.failed_jobs,
        dead_letter_jobs: report.dead_letter_jobs,
        pending_outbox_events: report.pending_outbox_events,
    };

    if let Some(path) = notify.jsonl_path.as_deref() {
        if let Err(err) = append_heartbeat_jsonl(path, &notification).await {
            eprintln!(
                "medousa-daemon heartbeat sink jsonl error path={} err={err}",
                path.display()
            );
        }
    }

    if let (Some(url), Some(client)) = (notify.webhook_url.as_deref(), webhook_client) {
        if let Err(err) = post_heartbeat_webhook(client, url, &notification).await {
            eprintln!("medousa-daemon heartbeat sink webhook error url={url} err={err}");
        }
    }
}

async fn append_heartbeat_jsonl(path: &Path, notification: &HeartbeatNotification) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed creating heartbeat sink directory {}", parent.display()))?;
    }

    let line = serde_json::to_string(notification).context("serialize heartbeat notification")?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("open heartbeat sink file {}", path.display()))?;
    file.write_all(line.as_bytes())
        .await
        .with_context(|| format!("write heartbeat sink file {}", path.display()))?;
    file.write_all(b"\n")
        .await
        .with_context(|| format!("write heartbeat sink newline {}", path.display()))?;

    Ok(())
}

async fn post_heartbeat_webhook(
    client: &reqwest::Client,
    url: &str,
    notification: &HeartbeatNotification,
) -> Result<()> {
    let response = client
        .post(url)
        .json(notification)
        .send()
        .await
        .with_context(|| format!("send heartbeat notification webhook {url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "(failed reading webhook response body)".to_string());
        anyhow::bail!(
            "status={} body={}",
            status,
            truncate_for_log(&body, 400)
        );
    }

    Ok(())
}

fn truncate_for_log(text: &str, max_chars: usize) -> String {
    let mut out = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn format_tick_report(prefix: &str, report: &TickReport) -> String {
    format!(
        "{prefix} lane={} policy={} materialized={} processed={:?} published={} failed={} dead_letter={} outbox_pending={} heartbeat_action={} heartbeat_significance={:.2} heartbeat_reason={}",
        report.lane.as_str(),
        report.lane_policy_profile,
        report.materialized,
        report.processed_job,
        report.published,
        report.failed_jobs,
        report.dead_letter_jobs,
        report.pending_outbox_events,
        report.heartbeat_action.as_str(),
        report.heartbeat_significance,
        report.heartbeat_reason,
    )
}

fn compile_lane_prompt(lane: EngineExecutionLane, prompt: &str) -> String {
    compile_default_lane_prompt(lane, prompt)
}

#[cfg(test)]
mod tests {
    use super::{
        EngineExecutionLane, HeartbeatAction, LaneSafetyActionClass, StatusCode,
        TickReport, compile_lane_prompt, enforce_lane_safety,
        format_tick_report, parse_backend, tick_runtime,
    };

    #[test]
    fn interactive_prompt_contains_context_compiler_metadata() {
        let compiled = compile_lane_prompt(EngineExecutionLane::Interactive, "Summarize status");
        assert!(compiled.contains("[MEDOUSA_CONTEXT_COMPILER]"));
        assert!(compiled.contains("lane=interactive"));
        assert!(compiled.contains("lane_policy_profile=interactive"));
    }

    #[test]
    fn scheduled_prompt_contains_scheduled_lane_profile() {
        let compiled = compile_lane_prompt(EngineExecutionLane::Scheduled, "Run nightly report");
        assert!(compiled.contains("[MEDOUSA_CONTEXT_COMPILER]"));
        assert!(compiled.contains("lane=scheduled"));
        assert!(compiled.contains("lane_policy_profile=scheduled"));
    }

    #[test]
    fn tick_report_formatter_contains_lane_and_heartbeat_summary() {
        let report = TickReport {
            materialized: 2,
            processed_job: Some("job-123".to_string()),
            published: 4,
            lane: EngineExecutionLane::Scheduled,
            lane_policy_profile: "scheduled",
            heartbeat_action: HeartbeatAction::Notify,
            heartbeat_significance: 0.72,
            heartbeat_reason: "dead_letter_pressure count=3 score=0.72".to_string(),
            failed_jobs: 1,
            dead_letter_jobs: 3,
            pending_outbox_events: 5,
        };

        let formatted = format_tick_report("medousa-daemon tick", &report);
        assert!(formatted.contains("lane=scheduled"));
        assert!(formatted.contains("policy=scheduled"));
        assert!(formatted.contains("heartbeat_action=notify"));
        assert!(formatted.contains("heartbeat_significance=0.72"));
        assert!(formatted.contains("heartbeat_reason=dead_letter_pressure"));
    }

    #[tokio::test]
    async fn tick_runtime_reports_scheduled_defaults_on_fresh_runtime() {
        let backend = parse_backend(Some("in-memory"));
        let runtime = medousa::build_runtime(backend, None, None, None)
            .await
            .expect("runtime should build");

        let report = tick_runtime(&runtime, "test-worker")
            .await
            .expect("tick should succeed");

        assert_eq!(report.lane, EngineExecutionLane::Scheduled);
        assert_eq!(report.lane_policy_profile, "scheduled");
        assert_eq!(report.materialized, 0);
        assert!(report.processed_job.is_none());
        assert_eq!(report.published, 0);
        assert_eq!(report.heartbeat_action, HeartbeatAction::Noop);
        assert!(report.heartbeat_reason.contains("below_threshold"));
    }

    #[test]
    fn lane_safety_rejects_profile_mismatch_for_interactive_ingress() {
        let err = enforce_lane_safety(
            EngineExecutionLane::Interactive,
            LaneSafetyActionClass::InteractiveIngress,
            Some("scheduled"),
        )
        .expect_err("mismatched policy profile should fail");

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("policy_profile"));
    }

    #[test]
    fn lane_safety_rejects_recurring_registration_on_interactive_lane() {
        let err = enforce_lane_safety(
            EngineExecutionLane::Interactive,
            LaneSafetyActionClass::RecurringRegistration,
            Some("interactive"),
        )
        .expect_err("recurring registration should be blocked on interactive lane");

        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("action=recurring_registration"));
    }
}
