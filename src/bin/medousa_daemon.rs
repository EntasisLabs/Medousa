use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use medousa::engine_context::{
    ContextCompilerInput, EngineExecutionLane, HeartbeatAction, HeartbeatSignals,
    RecallReadiness, compile_context_prompt, default_heartbeat_lane_policy,
    default_policy_profile_for_lane, evaluate_heartbeat_significance,
};
use medousa::daemon_api::{
    DEFAULT_DAEMON_BIND, DaemonStatsResponse, EnqueueAskRequest, EnqueuePromptRequest,
    EnqueueResponse, HealthResponse, RegisterRecurringPromptRequest, RegisterRecurringResponse,
};
use medousa::{build_runtime, parse_backend};
use tokio::sync::{RwLock, watch};
use uuid::Uuid;

use stasis::application::orchestration::runtime_job_payloads::{
    AgentSessionJobPayload, AgentSessionParticipantPayload, AgentToolCallMode, PromptJobPayload,
};
use stasis::application::orchestration::runtime_workflow_job_builder::RuntimeWorkflowJobBuilder;
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
    last_tick_at: Arc<RwLock<Option<chrono::DateTime<Utc>>>>,
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

    let runtime = Arc::new(build_runtime(backend, provider, model, base_url).await?);

    if once {
        let report = tick_runtime(runtime.as_ref(), &worker_id).await?;
        println!(
            "medousa-daemon once: lane={} policy={} materialized={} processed={:?} published={} heartbeat_action={} heartbeat_significance={:.2} heartbeat_reason={}",
            report.lane.as_str(),
            report.lane_policy_profile,
            report.materialized,
            report.processed_job,
            report.published,
            report.heartbeat_action.as_str(),
            report.heartbeat_significance,
            report.heartbeat_reason,
        );
        return Ok(());
    }

    let state = AppState {
        runtime: runtime.clone(),
        backend: backend_name,
        worker_id: worker_id.clone(),
        last_tick_at: Arc::new(RwLock::new(None)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/stats", get(stats))
        .route("/v1/jobs/ask", post(enqueue_ask))
        .route("/v1/jobs/prompt", post(enqueue_prompt))
        .route("/v1/recurring/prompt", post(register_recurring_prompt))
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
                    eprintln!(
                        "medousa-daemon tick lane={} policy={} materialized={} processed={:?} published={} heartbeat_action={} heartbeat_significance={:.2} heartbeat_reason={}",
                        report.lane.as_str(),
                        report.lane_policy_profile,
                        report.materialized,
                        report.processed_job,
                        report.published,
                        report.heartbeat_action.as_str(),
                        report.heartbeat_significance,
                        report.heartbeat_reason,
                    );
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

    let now = Utc::now();
    let job_id = format!("medousa-daemon-ask-{}", now.timestamp_millis());
    let raw_prompt = request.prompt;
    let compiled_prompt = compile_lane_prompt(EngineExecutionLane::Interactive, &raw_prompt);

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
        policy_profile: request.policy_profile.or_else(|| {
            Some(default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string())
        }),
        model_hint: request.model_hint,
        memory_policy: None,
        max_turns: request.max_turns.map(|value| value as usize),
        tool_call_mode: Some(AgentToolCallMode::Auto),
    };

    let new_job = RuntimeWorkflowJobBuilder::for_agent_session(job_id.clone(), &payload)
        .map_err(internal_error)?
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

    let now = Utc::now();
    let job_id = format!("medousa-daemon-prompt-{}", now.timestamp_millis());
    let compiled_prompt = compile_lane_prompt(EngineExecutionLane::Interactive, &request.prompt);

    let payload = PromptJobPayload {
        user_prompt: compiled_prompt,
        system_prompt: request.system_prompt.or(Some(
            "You are Medousa, a practical assistant. Be concise and structured.".to_string(),
        )),
        policy_profile: request.policy_profile.or_else(|| {
            Some(default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string())
        }),
        model_hint: request.model_hint,
        memory_policy: None,
    };

    let new_job = RuntimeWorkflowJobBuilder::for_prompt(job_id.clone(), &payload)
        .map_err(internal_error)?
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

async fn enqueue_runtime_job(
    runtime: &RuntimeComposition,
    job: stasis::prelude::NewJob,
) -> Result<()> {
    let sdk = RuntimeSdk::new(runtime.clone());
    sdk.enqueue(job).await?;
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

fn compile_lane_prompt(lane: EngineExecutionLane, prompt: &str) -> String {
    compile_context_prompt(ContextCompilerInput {
        lane,
        user_prompt: prompt,
        response_depth_mode: "standard",
        stage_route: None,
        recall_readiness: RecallReadiness::Missing,
    })
    .compiled_prompt
}
