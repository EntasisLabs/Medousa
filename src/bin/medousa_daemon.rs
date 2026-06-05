use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use axum::extract::{Path as AxumPath,  State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Timelike, Utc};
use futures_util::stream::{self, Stream};
use medousa::artifact_chunking::chunk_json_payload;
use medousa::artifact_extraction::{extract_claims_from_chunks, persist_extraction_run};
use medousa::context_pack::{
    BuildContextPackInput, ContextPackBudgetProfile, build_context_pack, persist_context_pack,
};
use medousa::engine_context::{
    EngineExecutionLane, HeartbeatAction, HeartbeatLanePolicy, HeartbeatSignals,
    LaneSafetyActionClass,
    compile_default_lane_prompt, default_heartbeat_lane_policy,
    default_policy_profile_for_lane, evaluate_heartbeat_significance,
    validate_lane_action, validate_lane_policy_profile,
};
use medousa::verifier::{VerificationPolicy, verify_context_pack};
use medousa::verification_store::persist_verification;
use medousa::identity_memory::{
    resolve_identity_channel_id, resolve_identity_persona_id, resolve_identity_user_id,
};
use medousa::daemon_api::{
    ArtifactCommandRequest, ArtifactCommandResponse, DEFAULT_DAEMON_BIND,
    DaemonStatsResponse, EnqueueAskRequest, EnqueuePromptRequest, EnqueueReportRequest,
    EnqueueResponse, HealthResponse, HeartbeatDeliveryMetricsResponse,
    HeartbeatDeliveryPolicyResponse, HeartbeatPolicyResponse, HeartbeatStatusResponse,
    IngestRequest, IngestResponse, DeliverPollResponse, DeliveryHealthResponse,
    ContinuationStatusResponse, TurnContinuationLineageResponse, ReplayAndResumeResponse,
    InteractiveTurnRequest, InteractiveTurnResponse,
    IdentityContextRequest, JobCitationResponse, JobEvidenceReportResponse, JobReportResponse,
    JobResultResponse, InteractiveTurnStreamEvent,
    RegisterRecurringPromptRequest, RegisterRecurringResponse, RuntimeConfigCommandRequest,
    RuntimeConfigCommandResponse, RuntimeConfigCommandSpec,
    StageRouteCommandRequest, StageRouteCommandResponse,
};
use medousa::session_mapping;
use medousa::{
    PlatformBuildConfig, apply_daemon_env, build_daemon_platform, channel_delivery,
    load_product_config, parse_backend, remove_surrealkv_lock,
};
use medousa::agent_runtime::stream_sink::AgentStreamSink;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, broadcast, watch};
use uuid::Uuid;

use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::application::runtime::identity_context_compiler::prepend_identity_snapshot;
use stasis::application::orchestration::runtime_job_payloads::PromptJobPayload;
use stasis::application::orchestration::runtime_workflow_job_builder::RuntimeWorkflowJobBuilder;
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, CommitEntityUpdateResponse, GetIdentityContextRequest,
    GetIdentityContextResponse, ListEntityHistoryRequest, ListEntityHistoryResponse,
    ProposeEntityUpdateRequest, ProposeEntityUpdateResponse, RollbackEntityVersionRequest,
    RollbackEntityVersionResponse,
};
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::dashboard::{
    DashboardState, RuntimeDashboardQueryService, router as dashboard_router,
};
use stasis::prelude::{JobState, RecurringDefinition, RuntimeComposition, RuntimeSdk};
use stasis::sdk::runtime_sdk::RuntimeStatsSnapshot;

#[derive(Clone)]
struct AppState {
    platform: Arc<medousa::MedousaPlatformRuntime>,
    daemon_base_url: String,
    interactive_turn_streams: Arc<RwLock<HashMap<String, broadcast::Sender<InteractiveTurnStreamEvent>>>>,
    active_ingest_jobs: Arc<RwLock<HashMap<String, medousa::session_mapping::ActiveIngestJob>>>,
    channel_deliveries: Arc<RwLock<HashMap<String, channel_delivery::ChannelDeliveryTarget>>>,
    job_delivery_records: Arc<RwLock<HashMap<String, channel_delivery::JobDeliveryRecord>>>,
    delivered_outbox_events: Arc<RwLock<HashSet<String>>>,
    channel_dispatch_client: reqwest::Client,
    deliver_webhook_token: Option<String>,
    deliver_webhook_target: String,
    last_delivery_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_delivery_latency_ms: Arc<RwLock<Option<u64>>>,
    last_agent_turn_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_agent_turn_latency_ms: Arc<RwLock<Option<u64>>>,
    agent_tool_registry_count: usize,
    agent_turn_jobs: Arc<RwLock<HashMap<String, AgentTurnJobRecord>>>,
    default_runtime_config: session_mapping::IngestSessionRuntimeConfig,
    cancelled_ingest_streams: Arc<RwLock<HashSet<String>>>,
    session_runtime_configs:
        Arc<RwLock<HashMap<String, medousa::session_mapping::IngestSessionRuntimeConfig>>>,
    backend: String,
    worker_id: String,
    identity_service: Arc<IdentityMemoryService>,
    identity_default_user_id: String,
    last_tick_at: Arc<RwLock<Option<chrono::DateTime<Utc>>>>,
    last_heartbeat_report: Arc<RwLock<Option<TickReport>>>,
    heartbeat_policy: HeartbeatLanePolicy,
    heartbeat_delivery_policy: HeartbeatDeliveryPolicy,
    heartbeat_metrics: Arc<RwLock<HeartbeatDeliveryMetrics>>,
    heartbeat_notify: HeartbeatNotifyConfig,
    webhook_client: Option<reqwest::Client>,
}

impl AppState {
    fn composition(&self) -> &RuntimeComposition {
        self.platform.composition()
    }

    fn agent(&self) -> &medousa::TuiRuntime {
        self.platform.agent()
    }
}

#[derive(Debug, Clone)]
struct AgentTurnJobRecord {
    status: String,
    output_text: Option<String>,
    error: Option<String>,
}

impl AgentTurnJobRecord {
    fn pending() -> Self {
        Self {
            status: "pending".to_string(),
            output_text: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Clone, Debug, Default)]
struct DashboardActionAuthConfig {
    bearer_token: Option<String>,
    required_role: Option<String>,
    role_claim_header: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct HeartbeatDeliveryPolicy {
    min_notify_interval_secs: u64,
    quiet_hours: Option<QuietHoursWindow>,
}

impl Default for HeartbeatDeliveryPolicy {
    fn default() -> Self {
        Self {
            min_notify_interval_secs: 0,
            quiet_hours: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct QuietHoursWindow {
    start_hour_utc: u8,
    end_hour_utc: u8,
}

impl QuietHoursWindow {
    fn contains_utc_hour(self, hour: u8) -> bool {
        if self.start_hour_utc < self.end_hour_utc {
            hour >= self.start_hour_utc && hour < self.end_hour_utc
        } else {
            hour >= self.start_hour_utc || hour < self.end_hour_utc
        }
    }
}

#[derive(Clone, Debug, Default)]
struct HeartbeatDeliveryMetrics {
    tick_evaluations: u64,
    notify_decisions: u64,
    dispatched_notifications: u64,
    suppressed_quiet_hours: u64,
    suppressed_min_interval: u64,
    last_notify_decision_at_utc: Option<DateTime<Utc>>,
    last_dispatched_at_utc: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeartbeatDispatchDecision {
    NotRequired,
    Dispatch,
    SuppressedQuietHours,
    SuppressedMinInterval,
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

const DAEMON_REPORT_SESSION_ID: &str = "medousa-daemon-reports";
const MAX_REPORT_CITATIONS: usize = 24;

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }

    let backend_name = find_arg_value(&args, "--backend")
        .unwrap_or("in-memory")
        .to_string();
    apply_daemon_env(&load_product_config());
    medousa::runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
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
    let heartbeat_policy = parse_heartbeat_policy(&args)?;
    let heartbeat_delivery_policy = parse_heartbeat_delivery_policy(&args)?;
    let dashboard_action_auth = parse_dashboard_action_auth(&args)?;

    // Hold the HTTP port before opening SurrealKV so a second launcher cannot
    // race on the database LOCK while this process is still initializing.
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid --bind address: {bind}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| {
            format!(
                "failed to bind medousa daemon on {addr} — another daemon may already be running"
            )
        })?;
    eprintln!("medousa-daemon acquired {addr}, initializing runtime…");

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

    let deliver_webhook_url = channel_delivery::internal_deliver_webhook_url(bind);
    let platform_config = PlatformBuildConfig {
        provider: provider.map(str::to_string),
        model: model.map(str::to_string),
        base_url: base_url.map(str::to_string),
        deliver_webhook_url: deliver_webhook_url.clone(),
        allowed_grapheme_modules: Vec::new(),
        session_id: "daemon-agent-runtime".to_string(),
        backend_label: backend_name.clone(),
    };

    let platform = build_daemon_platform(backend.clone(), platform_config)
        .await
        .context("failed to build medousa platform runtime")?;

    let identity_service = platform.identity_service();
    let identity_default_user_id = resolve_identity_user_id(None);

    if once {
        let report = tick_runtime(platform.composition(), &worker_id, heartbeat_policy).await?;
        println!("{}", format_tick_report("medousa-daemon once", &report));
        let mut heartbeat_metrics = HeartbeatDeliveryMetrics::default();
        let dispatch_decision = decide_heartbeat_dispatch(
            &report,
            Utc::now(),
            heartbeat_delivery_policy,
            &mut heartbeat_metrics,
        );

        if dispatch_decision == HeartbeatDispatchDecision::Dispatch {
            let channel_client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .context("failed to build heartbeat channel dispatch client")?;
            dispatch_heartbeat_notifications(
                &heartbeat_notify,
                webhook_client.as_ref(),
                &channel_client,
                &backend_name,
                &worker_id,
                &report,
                None,
            )
            .await;
        } else if report.heartbeat_action == HeartbeatAction::Notify {
            eprintln!(
                "medousa-daemon heartbeat notify suppressed decision={}",
                heartbeat_dispatch_decision_label(dispatch_decision)
            );
        }

        remove_surrealkv_lock(&parse_backend(Some(&backend_name)));
        return Ok(());
    }

    let agent_tool_registry_count = platform
        .agent()
        .tool_registry
        .list_tools()
        .await
        .map(|tools| tools.len())
        .unwrap_or(0);
    let default_runtime_config = session_mapping::IngestSessionRuntimeConfig::from_saved_defaults();

    let state = AppState {
        platform: platform.clone(),
        daemon_base_url: format!("http://{bind}"),
        interactive_turn_streams: Arc::new(RwLock::new(HashMap::new())),
        active_ingest_jobs: Arc::new(RwLock::new(HashMap::new())),
        channel_deliveries: Arc::new(RwLock::new(HashMap::new())),
        job_delivery_records: Arc::new(RwLock::new(HashMap::new())),
        delivered_outbox_events: Arc::new(RwLock::new(HashSet::new())),
        channel_dispatch_client: reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("failed to build channel dispatch client")?,
        deliver_webhook_token: channel_delivery::resolve_deliver_webhook_token(),
        deliver_webhook_target: deliver_webhook_url.clone(),
        last_delivery_at: Arc::new(RwLock::new(None)),
        last_delivery_latency_ms: Arc::new(RwLock::new(None)),
        last_agent_turn_at: Arc::new(RwLock::new(None)),
        last_agent_turn_latency_ms: Arc::new(RwLock::new(None)),
        agent_tool_registry_count,
        agent_turn_jobs: Arc::new(RwLock::new(HashMap::new())),
        default_runtime_config,
        cancelled_ingest_streams: Arc::new(RwLock::new(HashSet::new())),
        session_runtime_configs: Arc::new(RwLock::new(HashMap::new())),
        backend: backend_name,
        worker_id: worker_id.clone(),
        identity_service,
        identity_default_user_id,
        last_tick_at: Arc::new(RwLock::new(None)),
        last_heartbeat_report: Arc::new(RwLock::new(None)),
        heartbeat_policy,
        heartbeat_delivery_policy,
        heartbeat_metrics: Arc::new(RwLock::new(HeartbeatDeliveryMetrics::default())),
        heartbeat_notify,
        webhook_client,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/stats", get(stats))
        .route("/v1/sessions", get(medousa::daemon_handlers::list_session_history))
        .route(
            "/v1/sessions/{session_id}/history",
            get(medousa::daemon_handlers::get_session_history),
        )
        .route(
            "/v1/sessions/{session_id}/turns",
            post(medousa::daemon_handlers::append_session_turn),
        )
        .route(
            "/v1/sessions/{session_id}/name",
            axum::routing::put(medousa::daemon_handlers::set_session_display_name),
        )
        .route("/v1/heartbeat/status", get(heartbeat_status))
        .route("/v1/jobs/{job_id}/result", get(get_job_result))
        .route("/v1/jobs/{job_id}/report", get(get_job_report))
        .route("/v1/jobs/ask", post(enqueue_ask))
        .route("/v1/jobs/report", post(enqueue_report))
        .route("/v1/jobs/prompt", post(enqueue_prompt))
        .route("/v1/recurring/prompt", post(register_recurring_prompt))
        .route("/v1/interactive/turn", post(start_interactive_turn))
        .route(
            "/v1/interactive/turn/{turn_id}/stream",
            get(interactive_turn_stream),
        )
        .route("/v1/runtime/artifact/command", post(artifact_command))
        .route("/v1/runtime/config/command", post(runtime_config_command))
        .route("/v1/runtime/stage-route/command", post(stage_route_command))
        .route("/v1/identity/context", post(identity_get_context))
        .route("/v1/identity/update/propose", post(identity_propose_update))
        .route("/v1/identity/update/commit", post(identity_commit_update))
        .route("/v1/identity/history", post(identity_list_history))
        .route("/v1/identity/rollback", post(identity_rollback_version))
        .route("/v1/ingest", post(ingest_handler))
        .route("/v1/ingest/{stream_id}/stream", get(ingest_stream))
        .route("/v1/deliver/outbox", post(deliver_outbox_webhook))
        .route("/v1/deliver/poll/{job_id}", get(deliver_poll))
        .route("/v1/delivery/status", get(delivery_status))
        .route("/v1/continuations/status", get(continuation_status))
        .route(
            "/v1/continuations/lineage/{turn_correlation_id}",
            get(continuation_lineage),
        )
        .route(
            "/v1/jobs/{job_id}/replay-and-resume",
            post(replay_and_resume_job),
        )
        .with_state(state.clone());

    let capability_router = Router::new()
        .route(
            "/v1/capabilities",
            get(medousa::mcp_daemon_handlers::list_capabilities),
        )
        .route(
            "/v1/capabilities/{capability_id}",
            get(medousa::mcp_daemon_handlers::get_capability),
        )
        .route(
            "/v1/capabilities/reindex",
            post(medousa::mcp_daemon_handlers::reindex_capabilities),
        )
        .with_state(medousa::mcp_daemon_handlers::CapabilityApiState {
            agent_runtime: state.platform.agent_handle(),
        });

    let policy_router = Router::new()
        .route(
            "/v1/mcp/policy/evaluate",
            post(medousa::mcp_daemon_handlers::mcp_policy_evaluate),
        )
        .with_state(medousa::mcp_daemon_handlers::McpPolicyApiState {
            identity_service: state.identity_service.clone(),
        });

    let app = app.merge(capability_router).merge(policy_router);

    let dashboard_service = Arc::new(RuntimeDashboardQueryService::from_runtime_composition(
        state.composition().clone(),
    ));
    let dashboard_state =
        apply_dashboard_action_auth(DashboardState::new(dashboard_service), &dashboard_action_auth);
    let dashboard = dashboard_router(dashboard_state);
    let app = app.merge(dashboard);

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

    println!("medousa-daemon listening on http://{addr}");
    println!("medousa-daemon dashboard at http://{addr}/dashboard");
    if dashboard_action_auth.bearer_token.is_some() {
        println!("medousa-daemon dashboard actions require bearer token auth");
    }
    if let Some(required_role) = dashboard_action_auth.required_role.as_deref() {
        let role_claim_header = dashboard_action_auth
            .role_claim_header
            .as_deref()
            .unwrap_or("x-stasis-role");
        println!(
            "medousa-daemon dashboard actions require role={} via header={}",
            required_role, role_claim_header
        );
    }
    println!(
        "{}",
        build_operator_first_run_guide(
            &format!("http://{addr}"),
            heartbeat_policy,
            heartbeat_delivery_policy,
        )
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(true);
            println!("medousa-daemon stopping");
            remove_surrealkv_lock(&parse_backend(Some(&state.backend)));
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
        match tick_runtime(state.composition(), &worker_id, state.heartbeat_policy).await {
            Ok(report) => {
                let now_utc = Utc::now();
                *state.last_tick_at.write().await = Some(now_utc);
                *state.last_heartbeat_report.write().await = Some(report.clone());
                if report.materialized > 0
                    || report.processed_job.is_some()
                    || report.published > 0
                    || report.heartbeat_action == HeartbeatAction::Notify
                {
                    eprintln!("{}", format_tick_report("medousa-daemon tick", &report));
                }

                if let Some(ref job_id) = report.processed_job {
                    if job_succeeded(state.composition(), job_id).await {
                        let _ = maybe_resume_agent_turn_from_child_job(&state, job_id).await;
                    }
                }

                let dispatch_decision = {
                    let mut metrics = state.heartbeat_metrics.write().await;
                    decide_heartbeat_dispatch(
                        &report,
                        now_utc,
                        state.heartbeat_delivery_policy,
                        &mut metrics,
                    )
                };

                if dispatch_decision == HeartbeatDispatchDecision::Dispatch {
                    let (provider, model) =
                        resolve_api_model_routing(None, &state.default_runtime_config);
                    let agent = HeartbeatAgentContext {
                        backend: state.backend.clone(),
                        provider,
                        model,
                        response_depth_mode: state
                            .default_runtime_config
                            .response_depth_mode
                            .clone(),
                        agent_runtime: state.platform.agent_handle(),
                    };
                    dispatch_heartbeat_notifications(
                        &state.heartbeat_notify,
                        state.webhook_client.as_ref(),
                        &state.channel_dispatch_client,
                        &state.backend,
                        &state.worker_id,
                        &report,
                        Some(&agent),
                    )
                    .await;
                } else if report.heartbeat_action == HeartbeatAction::Notify {
                    eprintln!(
                        "medousa-daemon heartbeat notify suppressed decision={} significance={:.2} reason={}",
                        heartbeat_dispatch_decision_label(dispatch_decision),
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

async fn tick_runtime(
    runtime: &RuntimeComposition,
    worker_id: &str,
    heartbeat_policy: HeartbeatLanePolicy,
) -> Result<TickReport> {
    let sdk = RuntimeSdk::new(runtime.clone());
    let lane = EngineExecutionLane::Scheduled;
    let lane_policy_profile = default_policy_profile_for_lane(lane);
    let lane_worker_id = format!("{worker_id}:{}", lane.as_str());

    let materialized = safe_materialize_recurring_now(&sdk, &lane_worker_id).await?;
    let processed_job = safe_process_once(&sdk, "default", &lane_worker_id).await?;
    let published = safe_publish_pending_events(&sdk, 200).await?;
    let snapshot = safe_stats_snapshot(&sdk, 200).await?;

    let heartbeat_decision = evaluate_heartbeat_significance(
        &HeartbeatSignals {
            materialized_jobs: materialized,
            processed_job: processed_job.is_some(),
            published_events: published,
            failed_jobs: snapshot.failed_jobs,
            dead_letter_jobs: snapshot.dead_letter_jobs,
            pending_outbox_events: snapshot.pending_outbox_events,
        },
        heartbeat_policy,
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
        agent_runtime_version: medousa::agent_runtime::AGENT_RUNTIME_VERSION.to_string(),
        tool_registry_count: state.agent_tool_registry_count,
        last_agent_turn_latency_ms: *state.last_agent_turn_latency_ms.read().await,
        last_agent_turn_at_utc: *state.last_agent_turn_at.read().await,
    })
}

async fn stats(
    State(state): State<AppState>,
) -> Result<Json<DaemonStatsResponse>, (StatusCode, String)> {
    let sdk = RuntimeSdk::new(state.composition().clone());
    let snapshot = safe_stats_snapshot(&sdk, 5000)
        .await
        .map_err(internal_error)?;

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



async fn heartbeat_status(
    State(state): State<AppState>,
) -> Result<Json<HeartbeatStatusResponse>, (StatusCode, String)> {
    let now_utc = Utc::now();
    let last_tick_at_utc = *state.last_tick_at.read().await;
    let maybe_report = state.last_heartbeat_report.read().await.clone();
    let metrics = state.heartbeat_metrics.read().await.clone();
    let report = match maybe_report {
        Some(report) => report,
        None => compute_heartbeat_snapshot_report(&state).await?,
    };

    let in_quiet_hours = state
        .heartbeat_delivery_policy
        .quiet_hours
        .map(|window| window.contains_utc_hour(now_utc.hour() as u8))
        .unwrap_or(false);

    Ok(Json(HeartbeatStatusResponse {
        lane: report.lane.as_str().to_string(),
        lane_policy_profile: report.lane_policy_profile.to_string(),
        action: report.heartbeat_action.as_str().to_string(),
        significance: report.heartbeat_significance,
        reason: report.heartbeat_reason,
        policy: to_heartbeat_policy_response(state.heartbeat_policy),
        delivery_policy: to_heartbeat_delivery_policy_response(
            state.heartbeat_delivery_policy,
            in_quiet_hours,
        ),
        delivery_metrics: to_heartbeat_delivery_metrics_response(&metrics),
        materialized_jobs: report.materialized,
        processed_job: report.processed_job.is_some(),
        published_events: report.published,
        failed_jobs: report.failed_jobs,
        dead_letter_jobs: report.dead_letter_jobs,
        pending_outbox_events: report.pending_outbox_events,
        last_tick_at_utc,
        now_utc,
    }))
}

async fn compute_heartbeat_snapshot_report(
    state: &AppState,
) -> Result<TickReport, (StatusCode, String)> {
    let sdk = RuntimeSdk::new(state.composition().clone());
    let snapshot = safe_stats_snapshot(&sdk, 5000)
        .await
        .map_err(internal_error)?;
    let lane = EngineExecutionLane::Scheduled;

    let heartbeat_decision = evaluate_heartbeat_significance(
        &HeartbeatSignals {
            materialized_jobs: 0,
            processed_job: false,
            published_events: 0,
            failed_jobs: snapshot.failed_jobs,
            dead_letter_jobs: snapshot.dead_letter_jobs,
            pending_outbox_events: snapshot.pending_outbox_events,
        },
        state.heartbeat_policy,
    );

    Ok(TickReport {
        materialized: 0,
        processed_job: None,
        published: 0,
        lane,
        lane_policy_profile: default_policy_profile_for_lane(lane),
        heartbeat_action: heartbeat_decision.action,
        heartbeat_significance: heartbeat_decision.significance,
        heartbeat_reason: heartbeat_decision.reason,
        failed_jobs: snapshot.failed_jobs,
        dead_letter_jobs: snapshot.dead_letter_jobs,
        pending_outbox_events: snapshot.pending_outbox_events,
    })
}

fn to_heartbeat_policy_response(policy: HeartbeatLanePolicy) -> HeartbeatPolicyResponse {
    HeartbeatPolicyResponse {
        min_significance: policy.min_significance,
        dead_letter_weight: policy.dead_letter_weight,
        failed_weight: policy.failed_weight,
        outbox_weight: policy.outbox_weight,
        activity_weight: policy.activity_weight,
    }
}

fn to_heartbeat_delivery_policy_response(
    policy: HeartbeatDeliveryPolicy,
    in_quiet_hours: bool,
) -> HeartbeatDeliveryPolicyResponse {
    HeartbeatDeliveryPolicyResponse {
        min_notify_interval_secs: policy.min_notify_interval_secs,
        quiet_hours_start_utc: policy.quiet_hours.map(|window| window.start_hour_utc),
        quiet_hours_end_utc: policy.quiet_hours.map(|window| window.end_hour_utc),
        in_quiet_hours,
    }
}

fn to_heartbeat_delivery_metrics_response(
    metrics: &HeartbeatDeliveryMetrics,
) -> HeartbeatDeliveryMetricsResponse {
    HeartbeatDeliveryMetricsResponse {
        tick_evaluations: metrics.tick_evaluations,
        notify_decisions: metrics.notify_decisions,
        dispatched_notifications: metrics.dispatched_notifications,
        suppressed_quiet_hours: metrics.suppressed_quiet_hours,
        suppressed_min_interval: metrics.suppressed_min_interval,
        last_notify_decision_at_utc: metrics.last_notify_decision_at_utc,
        last_dispatched_at_utc: metrics.last_dispatched_at_utc,
    }
}

fn decide_heartbeat_dispatch(
    report: &TickReport,
    now_utc: DateTime<Utc>,
    delivery_policy: HeartbeatDeliveryPolicy,
    metrics: &mut HeartbeatDeliveryMetrics,
) -> HeartbeatDispatchDecision {
    metrics.tick_evaluations = metrics.tick_evaluations.saturating_add(1);

    if report.heartbeat_action != HeartbeatAction::Notify {
        return HeartbeatDispatchDecision::NotRequired;
    }

    metrics.notify_decisions = metrics.notify_decisions.saturating_add(1);
    metrics.last_notify_decision_at_utc = Some(now_utc);

    if let Some(window) = delivery_policy.quiet_hours {
        if window.contains_utc_hour(now_utc.hour() as u8) {
            metrics.suppressed_quiet_hours = metrics.suppressed_quiet_hours.saturating_add(1);
            return HeartbeatDispatchDecision::SuppressedQuietHours;
        }
    }

    if delivery_policy.min_notify_interval_secs > 0 {
        if let Some(last_dispatched) = metrics.last_dispatched_at_utc {
            let elapsed_seconds = now_utc.signed_duration_since(last_dispatched).num_seconds();
            if elapsed_seconds >= 0
                && (elapsed_seconds as u64) < delivery_policy.min_notify_interval_secs
            {
                metrics.suppressed_min_interval = metrics.suppressed_min_interval.saturating_add(1);
                return HeartbeatDispatchDecision::SuppressedMinInterval;
            }
        }
    }

    metrics.dispatched_notifications = metrics.dispatched_notifications.saturating_add(1);
    metrics.last_dispatched_at_utc = Some(now_utc);
    HeartbeatDispatchDecision::Dispatch
}

fn heartbeat_dispatch_decision_label(decision: HeartbeatDispatchDecision) -> &'static str {
    match decision {
        HeartbeatDispatchDecision::NotRequired => "not_required",
        HeartbeatDispatchDecision::Dispatch => "dispatch",
        HeartbeatDispatchDecision::SuppressedQuietHours => "suppressed_quiet_hours",
        HeartbeatDispatchDecision::SuppressedMinInterval => "suppressed_min_interval",
    }
}

async fn get_job_result(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<JobResultResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    if let Some(record) = state.agent_turn_jobs.read().await.get(&job_id) {
        return Ok(Json(job_result_from_agent_turn(&job_id, record)));
    }

    let attempts = match get_job_attempts_graceful(&state.composition(), &job_id).await {
        Ok(attempts) => attempts,
        Err(err) => return Err(err),
    };

    let latest = attempts.last();
    let latest_outcome = latest.map(|attempt| format!("{:?}", attempt.outcome));
    let latest_execution_id = latest.and_then(|attempt| attempt.execution_id.clone());
    let output_text = latest
        .and_then(|attempt| {
            channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
        });

    let (status, is_terminal) = derive_job_result_status(latest_outcome.as_deref(), attempts.len());

    Ok(Json(JobResultResponse {
        job_id,
        status,
        is_terminal,
        attempt_count: attempts.len(),
        latest_outcome,
        latest_execution_id,
        output_text,
    }))
}

async fn get_job_report(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<JobReportResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    if let Some(record) = state.agent_turn_jobs.read().await.get(&job_id) {
        let base = job_result_from_agent_turn(&job_id, record);
        return Ok(Json(JobReportResponse {
            job_id: base.job_id,
            status: base.status,
            is_terminal: base.is_terminal,
            attempt_count: base.attempt_count,
            latest_outcome: base.latest_outcome,
            latest_execution_id: base.latest_execution_id,
            output_text: base.output_text,
            citations: Vec::new(),
            evidence_report: None,
        }));
    }

    let attempts = match get_job_attempts_graceful(&state.composition(), &job_id).await {
        Ok(attempts) => attempts,
        Err(err) => return Err(err),
    };

    let latest = attempts.last();
    let latest_outcome = latest.map(|attempt| format!("{:?}", attempt.outcome));
    let latest_execution_id = latest.and_then(|attempt| attempt.execution_id.clone());
    let output_text = latest
        .and_then(|attempt| {
            channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
        });
    let diagnostics = latest
        .and_then(|attempt| attempt.diagnostics.as_deref())
        .and_then(parse_diagnostics_json);

    let (status, is_terminal) = derive_job_result_status(latest_outcome.as_deref(), attempts.len());
    let citations = diagnostics
        .as_ref()
        .map(extract_citations_from_payload)
        .unwrap_or_default();
    let evidence_report = if is_terminal && status == "succeeded" {
        diagnostics
            .as_ref()
            .and_then(|payload| build_job_evidence_report(&job_id, payload))
    } else {
        None
    };

    Ok(Json(JobReportResponse {
        job_id,
        status,
        is_terminal,
        attempt_count: attempts.len(),
        latest_outcome,
        latest_execution_id,
        output_text,
        citations,
        evidence_report,
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
    let session_id = format!("daemon-api:{}", identity_context.user_id);
    let (provider, model) =
        resolve_api_model_routing(request.model_hint.as_deref(), &state.default_runtime_config);

    spawn_daemon_api_agent_turn(
        &state,
        job_id.clone(),
        session_id,
        request.prompt,
        state.default_runtime_config.response_depth_mode.clone(),
        provider,
        model,
    )
    .await;

    Ok(Json(EnqueueResponse {
        job_id,
        queue: "agent-runtime".to_string(),
        accepted_at_utc: now,
    }))
}

async fn enqueue_report(
    State(state): State<AppState>,
    Json(request): Json<EnqueueReportRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    if request.query.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "query is required".to_string()));
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
    let job_id = format!("medousa-daemon-report-{}", now.timestamp_millis());
    let session_id = format!("daemon-report:{}", identity_context.user_id);
    let (provider, model) =
        resolve_api_model_routing(request.model_hint.as_deref(), &state.default_runtime_config);
    let prompt = build_report_prompt(&request.query);

    spawn_daemon_api_agent_turn(
        &state,
        job_id.clone(),
        session_id,
        prompt,
        state.default_runtime_config.response_depth_mode.clone(),
        provider,
        model,
    )
    .await;

    Ok(Json(EnqueueResponse {
        job_id,
        queue: "agent-runtime".to_string(),
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

    enqueue_runtime_job(state.composition(), new_job)
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

    let timezone = request.timezone.as_deref().unwrap_or("UTC");
    medousa::recurring_delivery::validate_recurring_cron(&request.cron_expr, timezone)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    enforce_lane_safety(
        EngineExecutionLane::Scheduled,
        LaneSafetyActionClass::RecurringRegistration,
        request.policy_profile.as_deref(),
    )?;

    let now = Utc::now();
    let queue = request.queue.unwrap_or_else(|| "default".to_string());
    let timezone = timezone.to_string();
    let recurring_id = request
        .id
        .unwrap_or_else(|| format!("medousa-recurring-{}", Uuid::new_v4().simple()));
    let fallback_session_id = request
        .session_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("recurring-{recurring_id}"));
    let execution_mode = request
        .execution_mode
        .as_deref()
        .unwrap_or("prompt")
        .trim()
        .to_ascii_lowercase();

    let (job_type, payload_template_ref) = match execution_mode.as_str() {
        "prompt" => {
            let compiled_prompt =
                compile_lane_prompt(EngineExecutionLane::Scheduled, &request.prompt);
            let prompt_payload = PromptJobPayload {
                user_prompt: compiled_prompt,
                system_prompt: request.system_prompt.clone(),
                policy_profile: request.policy_profile.or_else(|| {
                    Some(
                        default_policy_profile_for_lane(EngineExecutionLane::Scheduled)
                            .to_string(),
                    )
                }),
                model_hint: request.model_hint.clone(),
                memory_policy: None,
            };
            (
                "workflow.stasis.prompt".to_string(),
                prompt_payload.to_payload_ref().map_err(internal_error)?,
            )
        }
        "agent_turn" | "agent-turn" => {
            let provider = medousa::resolve_llm_provider(None);
            let model = medousa::resolve_llm_model(
                request
                    .model_hint
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            );
            let agent_payload = medousa::recurring_agent_turn::build_recurring_agent_turn_payload(
                &request.prompt,
                &fallback_session_id,
                request.system_prompt.clone(),
                request.policy_profile.clone(),
                request.model_hint.clone(),
                Some(provider),
                Some(model),
            );
            (
                medousa::recurring_agent_turn::RECURRING_AGENT_TURN_JOB_TYPE.to_string(),
                agent_payload.to_payload_ref().map_err(|err| {
                    (StatusCode::BAD_REQUEST, err.to_string())
                })?,
            )
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "execution_mode={other} is invalid; use prompt or agent_turn"
                ),
            ));
        }
    };

    let mut definition = RecurringDefinition {
        id: recurring_id.clone(),
        queue: queue.clone(),
        job_type,
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

    let delivery_input = serde_json::json!({ "delivery": request.delivery });
    medousa::recurring_delivery::persist_recurring_delivery_binding(
        &recurring_id,
        &delivery_input,
        medousa::recurring_delivery::DeliveryResolveContext {
            ambient: None,
            fallback_session_id,
        },
    )
    .await
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    let sdk = RuntimeSdk::new(state.composition().clone());
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

async fn start_interactive_turn(
    State(state): State<AppState>,
    Json(request): Json<InteractiveTurnRequest>,
) -> Result<Json<InteractiveTurnResponse>, (StatusCode, String)> {
    let turn_id = format!("daemon-turn-{}", Uuid::new_v4().simple());
    let (stream_tx, _stream_rx) = broadcast::channel::<InteractiveTurnStreamEvent>(512);

    {
        let mut guard = state.interactive_turn_streams.write().await;
        guard.insert(turn_id.clone(), stream_tx.clone());
    }

    let response = medousa::interactive_turn_runtime::build_interactive_turn_response(
        &request,
        &state.daemon_base_url,
        &turn_id,
        true,
        false,
        None,
        Some("interactive turn accepted; daemon agent runtime streaming active".to_string()),
    )
    .map_err(internal_error)?;

    state.channel_deliveries.write().await.insert(
        turn_id.clone(),
        channel_delivery::ChannelDeliveryTarget {
            channel: "tui".to_string(),
            user_id: request.session_id.clone(),
            channel_id: request.session_id.clone(),
            session_id: request.session_id.clone(),
            stream_id: Some(turn_id.clone()),
        },
    );
    record_job_delivery_pending(&state, &turn_id).await;

    let stream_registry = state.interactive_turn_streams.clone();
    let agent_runtime = state.platform.agent_handle();
    let backend = state.backend.clone();
    let delivery_records = state.job_delivery_records.clone();
    let channel_deliveries = state.channel_deliveries.clone();
    let last_agent_turn_at = state.last_agent_turn_at.clone();
    let last_agent_turn_latency_ms = state.last_agent_turn_latency_ms.clone();
    let delivery = medousa::agent_runtime::InteractiveTurnDeliveryContext {
        turn_key: turn_id.clone(),
        delivery_records,
        channel_deliveries,
        last_turn_at: last_agent_turn_at,
        last_turn_latency_ms: last_agent_turn_latency_ms,
        started: std::time::Instant::now(),
    };
    let continuation_scope = medousa::turn_continuation::TurnContinuationScope {
        turn_correlation_id: turn_id.clone(),
        session_id: request.session_id.clone(),
        original_prompt: request.prompt.clone(),
        delivery_target: Some(channel_delivery::ChannelDeliveryTarget {
            channel: "tui".to_string(),
            user_id: request.session_id.clone(),
            channel_id: request.session_id.clone(),
            session_id: request.session_id.clone(),
            stream_id: Some(turn_id.clone()),
        }),
        provider: request.provider.clone(),
        model: request.model.clone(),
        response_depth_mode: request.response_depth_mode.clone(),
    };
    tokio::spawn(async move {
        // Give the client a brief window to subscribe before first deltas.
        tokio::time::sleep(Duration::from_millis(120)).await;
        medousa::agent_runtime::run_daemon_interactive_turn(
            &turn_id,
            request,
            &backend,
            agent_runtime.as_ref(),
            stream_tx,
            Some(delivery),
            Some(continuation_scope),
        )
        .await;

        // Keep stream available briefly for reconnect/debug then clean up.
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut guard = stream_registry.write().await;
        guard.remove(&turn_id);
    });

    Ok(Json(response))
}

async fn interactive_turn_stream(
    State(state): State<AppState>,
    AxumPath(turn_id): AxumPath<String>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(&registry, &turn_id, "interactive turn").await
}

async fn ingest_stream(
    State(state): State<AppState>,
    AxumPath(stream_id): AxumPath<String>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let registry = state.interactive_turn_streams.clone();
    stream_events_from_registry(&registry, &stream_id, "ingest stream").await
}

async fn stream_events_from_registry(
    registry: &Arc<RwLock<HashMap<String, broadcast::Sender<InteractiveTurnStreamEvent>>>>,
    stream_id: &str,
    label: &str,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>>, (StatusCode, String)>
{
    let sender = {
        let guard = registry.read().await;
        guard.get(stream_id).cloned()
    }
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("unknown {} id '{}'", label, stream_id),
        )
    })?;

    let stream = stream::unfold(sender.subscribe(), |mut receiver| async move {
        match receiver.recv().await {
            Ok(payload) => {
                let event = match Event::default()
                    .event(payload.event_type.clone())
                    .json_data(payload)
                {
                    Ok(value) => value,
                    Err(err) => Event::default()
                        .event("error")
                        .data(format!("stream serialization error: {err}")),
                };
                Some((Ok::<Event, Infallible>(event), receiver))
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                let event = Event::default()
                    .event("status")
                    .data(format!("stream lag detected; skipped {} event(s)", skipped));
                Some((Ok::<Event, Infallible>(event), receiver))
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    });

    Ok(
        Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive")),
    )
}

fn publish_interactive_turn_event(
    stream_tx: &broadcast::Sender<InteractiveTurnStreamEvent>,
    event: Result<InteractiveTurnStreamEvent>,
) {
    if let Ok(payload) = event {
        let _ = stream_tx.send(payload);
    }
}

async fn artifact_command(
    Json(request): Json<ArtifactCommandRequest>,
) -> Result<Json<ArtifactCommandResponse>, (StatusCode, String)> {
    if request.session_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    let response = medousa::artifact_command_runtime::execute_artifact_command(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}

async fn runtime_config_command(
    Json(request): Json<RuntimeConfigCommandRequest>,
) -> Result<Json<RuntimeConfigCommandResponse>, (StatusCode, String)> {
    let response = medousa::runtime_config_command_runtime::execute_runtime_config_command(request)
        .map_err(internal_error)?;
    Ok(Json(response))
}

async fn stage_route_command(
    Json(request): Json<StageRouteCommandRequest>,
) -> Result<Json<StageRouteCommandResponse>, (StatusCode, String)> {
    let response = medousa::stage_route_command_runtime::execute_stage_route_command(request)
        .map_err(internal_error)?;
    Ok(Json(response))
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

/// POST /v1/ingest — centralized ingester handler.
async fn ingest_handler(
    State(state): State<AppState>,
    Json(request): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    if request.channel.trim().is_empty() || request.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "channel and text are required".to_string(),
        ));
    }

    let product_config = medousa::load_product_config();
    if !medousa::ingest_sender_allowed(&request.channel, &request.user_id, &product_config) {
        let mapping_key = format!(
            "{}:{}:{}",
            request.channel, request.channel_id, request.user_id
        );
        let session_id = medousa::channel_session_store::channel_session_store()
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
    let existing_session_id = medousa::channel_session_store::channel_session_store()
        .get_session_id(&mapping_key)
        .await;

    if request.text.trim().eq_ignore_ascii_case("/new") {
        if let Some(old_session_id) = existing_session_id.clone() {
            push_channel_session_history(&mapping_key, old_session_id).await;
        }
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
        session_mapping::IngestAction::EnqueueAsk { prompt } => {
            let stream = start_ingest_ask_stream(
                &state,
                &mapping_key,
                &outcome.session_id,
                prompt,
                &request,
            )
            .await?;
            job_id = Some(stream.job_id);
            stream_id = Some(stream.stream_id);
            stream_url = Some(stream.stream_url);
            stream_ready = true;
            reply = "processing your request…".to_string();
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
                &request,
            )
            .await?;
            job_id = Some(stream.job_id);
            stream_id = Some(stream.stream_id);
            stream_url = Some(stream.stream_url);
            stream_ready = true;
            reply = "regenerating last response…".to_string();
        }
        session_mapping::IngestAction::ListHistory => {
            reply = format_channel_session_history(&mapping_key, &outcome.session_id).await;
        }
        session_mapping::IngestAction::ResumeSession { target_session_id } => {
            push_channel_session_history(&mapping_key, outcome.session_id.clone()).await;
            medousa::channel_session_store::channel_session_store()
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
    }

    medousa::channel_session_store::channel_session_store()
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
    medousa::channel_session_store::channel_session_store()
        .push_session_history(mapping_key, session_id)
        .await;
}

async fn format_channel_session_history(
    mapping_key: &str,
    active_session_id: &str,
) -> String {
    let entries = medousa::channel_session_store::channel_session_store()
        .list_session_history(mapping_key, 20)
        .await;

    let active_label = medousa::session::format_session_history_label(
        active_session_id,
        medousa::session::get_session_display_name(active_session_id).as_deref(),
    );
    let mut lines = vec![format!(
        "* {active_label} (active, {} turns)",
        medousa::session::load_history(active_session_id).len()
    )];

    for session_id in entries.into_iter().take(9) {
        if session_id == active_session_id {
            continue;
        }
        let label = medousa::session::format_session_history_label(
            &session_id,
            medousa::session::get_session_display_name(&session_id).as_deref(),
        );
        lines.push(format!(
            "* {label} ({} turns)",
            medousa::session::load_history(&session_id).len()
        ));
    }

    format!(
        "Recent sessions for this channel/user:\n{}\n\nUse /history <name or session id> to resume.",
        lines.join("\n")
    )
}

async fn resolve_session_runtime_config(
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
        command: RuntimeConfigCommandSpec::Model { args },
    };
    let response = medousa::runtime_config_command_runtime::execute_runtime_config_command(request)
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
        command: RuntimeConfigCommandSpec::Depth { mode },
    };
    let response = medousa::runtime_config_command_runtime::execute_runtime_config_command(request)
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

async fn deliver_outbox_webhook(
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
        medousa::recurring_delivery::resolve_delivery_target_for_job(
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

            if let Some(stream_id) = target.stream_id.as_deref() {
                if let Some(stream_tx) = state
                    .interactive_turn_streams
                    .read()
                    .await
                    .get(stream_id)
                    .cloned()
                {
                    publish_interactive_turn_event(
                        &stream_tx,
                        medousa::interactive_turn_runtime::final_stream_event(
                            stream_id,
                            &output,
                        ),
                    );
                }
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
            let _ = medousa::turn_continuation::turn_continuation_store()
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

async fn deliver_poll(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<DeliverPollResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    Ok(Json(build_deliver_poll_response(&state, &job_id).await))
}

async fn delivery_status(State(state): State<AppState>) -> Json<DeliveryHealthResponse> {
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

async fn continuation_status(_state: State<AppState>) -> Json<ContinuationStatusResponse> {
    let snapshot = medousa::turn_continuation::continuation_snapshot().await;
    let last = medousa::turn_continuation::last_continuation_resume();
    Json(ContinuationStatusResponse {
        pending_count: snapshot.pending_count,
        consumed_count: snapshot.consumed_count,
        resumed_count: snapshot.resumed_count,
        dead_letter_pending_count: snapshot.dead_letter_pending_count,
        total_count: snapshot.total_count,
        last_resume_at_utc: last.as_ref().map(|event| event.resumed_at),
        last_resume_child_job_id: last.as_ref().map(|event| event.child_job_id.clone()),
        last_resume_turn_correlation_id: last
            .as_ref()
            .map(|event| event.turn_correlation_id.clone()),
    })
}

async fn continuation_lineage(
    AxumPath(turn_correlation_id): AxumPath<String>,
) -> Result<Json<TurnContinuationLineageResponse>, (StatusCode, String)> {
    let turn_correlation_id = turn_correlation_id.trim().to_string();
    if turn_correlation_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "turn_correlation_id is required".to_string(),
        ));
    }

    let records = medousa::turn_continuation::continuation_lineage_for_turn(&turn_correlation_id, 50)
        .await;
    Ok(Json(TurnContinuationLineageResponse {
        turn_correlation_id,
        records: records
            .iter()
            .map(medousa::turn_continuation::lineage_entry_from_record)
            .collect(),
    }))
}

async fn replay_and_resume_job(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<ReplayAndResumeResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    let replayed = medousa::turn_continuation::replay_dead_letter_job(state.composition(), &job_id)
        .await
        .map_err(internal_error)?;
    if !replayed {
        return Ok(Json(ReplayAndResumeResponse {
            job_id,
            replayed: false,
            job_succeeded: false,
            agent_turn_resumed: false,
            message: "job is not in dead_letter state or does not exist".to_string(),
        }));
    }

    let sdk = RuntimeSdk::new(state.composition().clone());
    let worker_id = format!("{}:replay-resume", state.worker_id);
    for _ in 0..8 {
        let _ = safe_process_once(&sdk, "default", &worker_id).await.map_err(internal_error)?;
        if job_succeeded(state.composition(), &job_id).await {
            break;
        }
    }
    let _ = safe_publish_pending_events(&sdk, 50).await.map_err(internal_error)?;

    let succeeded = job_succeeded(state.composition(), &job_id).await;
    let agent_turn_resumed = if succeeded {
        maybe_resume_agent_turn_from_child_job(&state, &job_id).await
    } else {
        false
    };

    let message = if !succeeded {
        "job replayed from dead_letter but did not reach succeeded state".to_string()
    } else if agent_turn_resumed {
        "job replayed and agent continuation turn started".to_string()
    } else {
        "job replayed and succeeded; no pending agent continuation applied".to_string()
    };

    Ok(Json(ReplayAndResumeResponse {
        job_id,
        replayed: true,
        job_succeeded: succeeded,
        agent_turn_resumed,
        message,
    }))
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

async fn record_job_delivery_pending(state: &AppState, job_id: &str) {
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
    state.job_delivery_records.write().await.insert(
        job_id.to_string(),
        channel_delivery::JobDeliveryRecord {
            state: channel_delivery::JobDeliveryState::Delivered,
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
    delivery_records.write().await.insert(
        job_id.to_string(),
        channel_delivery::JobDeliveryRecord {
            state: channel_delivery::JobDeliveryState::Delivered,
            delivered_at: Some(now),
            error,
            latency_ms: Some(latency_ms),
        },
    );
    *last_delivery_at.write().await = Some(now);
    *last_delivery_latency_ms.write().await = Some(latency_ms);
}

fn resolve_api_model_routing(
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
                    medousa::resolve_llm_provider(Some(provider)),
                    medousa::resolve_llm_model(Some(model)),
                );
            }
        }
        return (
            defaults.draft_provider.clone(),
            medousa::resolve_llm_model(Some(hint)),
        );
    }

    (
        defaults.draft_provider.clone(),
        defaults.draft_model.clone(),
    )
}

fn job_result_from_agent_turn(job_id: &str, record: &AgentTurnJobRecord) -> JobResultResponse {
    let is_terminal = record.status != "pending";
    JobResultResponse {
        job_id: job_id.to_string(),
        status: record.status.clone(),
        is_terminal,
        attempt_count: usize::from(is_terminal),
        latest_outcome: record
            .error
            .clone()
            .or_else(|| Some(format!("status={}", record.status))),
        latest_execution_id: None,
        output_text: record.output_text.clone(),
    }
}

async fn job_succeeded(runtime: &RuntimeComposition, job_id: &str) -> bool {
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

async fn maybe_resume_agent_turn_from_child_job(state: &AppState, child_job_id: &str) -> bool {
    let store = medousa::turn_continuation::turn_continuation_store();
    let Some(record) = store.get(child_job_id).await else {
        return false;
    };
    if !record.should_resume() {
        return false;
    }
    if !store.mark_resumed(child_job_id).await.unwrap_or(false) {
        return false;
    }

    let job_output = medousa::turn_continuation::resolve_succeeded_job_output_text(
        state.composition(),
        child_job_id,
    )
    .await
    .unwrap_or_else(|| "Job succeeded but output text was unavailable.".to_string());

    let resume_prompt = medousa::turn_continuation::build_turn_resume_prompt(
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
    medousa::turn_continuation::record_continuation_resume(
        medousa::turn_continuation::TurnContinuationResumeEvent {
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
    record: &medousa::turn_continuation::TurnContinuationRecord,
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
        None,
    );
    interactive_request.persist_user_turn = false;

    let continuation_scope = medousa::turn_continuation::TurnContinuationScope {
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
        let (stream_tx, _stream_rx) =
            broadcast::channel::<InteractiveTurnStreamEvent>(64);

        let sink: Arc<dyn AgentStreamSink> = Arc::new(IngestAgentStreamSink {
            stream_id: stream_id.clone(),
            session_id: record.session_id.clone(),
            job_id: job_id.clone(),
            stream_tx,
            delivery_target: target,
            dispatch_client: state.channel_dispatch_client.clone(),
            delivery_records: state.job_delivery_records.clone(),
            channel_deliveries: state.channel_deliveries.clone(),
            last_delivery_at: state.last_delivery_at.clone(),
            last_delivery_latency_ms: state.last_delivery_latency_ms.clone(),
            cancelled_streams: state.cancelled_ingest_streams.clone(),
            delivery_started: std::time::Instant::now(),
        });

        let agent_runtime = state.platform.agent_handle();
        let backend = state.backend.clone();
        tokio::spawn(async move {
            medousa::agent_runtime::run_agent_turn(
                &stream_id,
                interactive_request,
                &backend,
                agent_runtime.as_ref(),
                sink,
                Some(continuation_scope),
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
        record.provider.clone(),
        record.model.clone(),
        continuation_scope,
    )
    .await;
}

async fn spawn_daemon_api_agent_turn(
    state: &AppState,
    job_id: String,
    session_id: String,
    prompt: String,
    response_depth_mode: String,
    provider: String,
    model: String,
) {
    let continuation_scope = medousa::turn_continuation::TurnContinuationScope {
        turn_correlation_id: job_id.clone(),
        session_id: session_id.clone(),
        original_prompt: prompt.clone(),
        delivery_target: None,
        provider: provider.clone(),
        model: model.clone(),
        response_depth_mode: response_depth_mode.clone(),
    };
    spawn_daemon_api_agent_turn_with_scope(
        state,
        job_id,
        session_id,
        prompt,
        response_depth_mode,
        provider,
        model,
        continuation_scope,
    )
    .await;
}

async fn spawn_daemon_api_agent_turn_with_scope(
    state: &AppState,
    job_id: String,
    session_id: String,
    prompt: String,
    response_depth_mode: String,
    provider: String,
    model: String,
    continuation_scope: medousa::turn_continuation::TurnContinuationScope,
) {
    state.agent_turn_jobs.write().await.insert(
        job_id.clone(),
        AgentTurnJobRecord::pending(),
    );

    let interactive_request = session_mapping::build_interactive_turn_request_for_ingest(
        &session_id,
        prompt,
        &provider,
        &model,
        &response_depth_mode,
        None,
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

        medousa::agent_runtime::run_agent_turn(
            &job_id_for_task,
            interactive_request,
            &backend,
            agent_runtime.as_ref(),
            sink,
            Some(continuation_scope),
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

    async fn agent_response(&self, _turn_id: u64, text: String, _tool_names: Vec<String>) {
        medousa::session::append_turn(
            &self.session_id,
            &medousa::session::ConversationTurn {
                role: "assistant".to_string(),
                content: text.clone(),
                timestamp: Utc::now(),
                tool_names: _tool_names,
                answer_state: None,
            },
        );

        let latency_ms = self.started.elapsed().as_millis() as u64;
        let now = Utc::now();
        self.agent_turn_jobs.write().await.insert(
            self.job_id.clone(),
            AgentTurnJobRecord {
                status: "succeeded".to_string(),
                output_text: Some(text),
                error: None,
            },
        );
        *self.last_agent_turn_at.write().await = Some(now);
        *self.last_agent_turn_latency_ms.write().await = Some(latency_ms);
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        let latency_ms = self.started.elapsed().as_millis() as u64;
        let now = Utc::now();
        self.agent_turn_jobs.write().await.insert(
            self.job_id.clone(),
            AgentTurnJobRecord {
                status: "failed".to_string(),
                output_text: None,
                error: Some(message),
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
        _input_receipt: Option<medousa::payload_receipt::ArtifactReceiptMeta>,
        _output_receipt: Option<medousa::payload_receipt::ArtifactReceiptMeta>,
    ) {
    }
}

struct IngestAgentStreamSink {
    stream_id: String,
    session_id: String,
    job_id: String,
    stream_tx: broadcast::Sender<InteractiveTurnStreamEvent>,
    delivery_target: channel_delivery::ChannelDeliveryTarget,
    dispatch_client: reqwest::Client,
    delivery_records: Arc<RwLock<HashMap<String, channel_delivery::JobDeliveryRecord>>>,
    channel_deliveries: Arc<RwLock<HashMap<String, channel_delivery::ChannelDeliveryTarget>>>,
    last_delivery_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    last_delivery_latency_ms: Arc<RwLock<Option<u64>>>,
    cancelled_streams: Arc<RwLock<HashSet<String>>>,
    delivery_started: std::time::Instant,
}

#[async_trait]
impl AgentStreamSink for IngestAgentStreamSink {
    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::content_delta_stream_event(&self.stream_id, &delta),
        );
    }

    async fn reasoning_chunk(&self, _turn_id: u64, delta: String) {
        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::reasoning_delta_stream_event(
                &self.stream_id,
                &delta,
            ),
        );
    }

    async fn agent_final_pending(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            return;
        }

        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::final_pending_stream_event_with_tools(
                &self.stream_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_needs_input(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            publish_interactive_turn_event(
                &self.stream_tx,
                medousa::interactive_turn_runtime::error_stream_event(
                    &self.stream_id,
                    "ingest turn cancelled by /stop",
                ),
            );
            return;
        }

        medousa::session::append_turn(
            &self.session_id,
            &medousa::session::ConversationTurn {
                role: "assistant".to_string(),
                content: text.clone(),
                timestamp: Utc::now(),
                tool_names: tool_names.clone(),
                answer_state: Some("needs_input".to_string()),
            },
        );

        let latency_ms = self.delivery_started.elapsed().as_millis() as u64;
        if let Err(err) = channel_delivery::dispatch_channel_message(
            &self.dispatch_client,
            &self.delivery_target,
            &text,
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
            &self.stream_tx,
            medousa::interactive_turn_runtime::needs_input_stream_event_with_tools(
                &self.stream_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        if self.cancelled_streams.read().await.contains(&self.stream_id) {
            publish_interactive_turn_event(
                &self.stream_tx,
                medousa::interactive_turn_runtime::error_stream_event(
                    &self.stream_id,
                    "ingest turn cancelled by /stop",
                ),
            );
            return;
        }

        medousa::session::append_turn(
            &self.session_id,
            &medousa::session::ConversationTurn {
                role: "assistant".to_string(),
                content: text.clone(),
                timestamp: Utc::now(),
                tool_names: tool_names.clone(),
                answer_state: None,
            },
        );

        let latency_ms = self.delivery_started.elapsed().as_millis() as u64;
        if let Err(err) = channel_delivery::dispatch_channel_message(
            &self.dispatch_client,
            &self.delivery_target,
            &text,
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
            &self.stream_tx,
            medousa::interactive_turn_runtime::final_stream_event_with_tools(
                &self.stream_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        let latency_ms = self.delivery_started.elapsed().as_millis() as u64;
        let user_message = format!("Sorry — {message}");
        let _ = channel_delivery::dispatch_channel_message(
            &self.dispatch_client,
            &self.delivery_target,
            &user_message,
        )
        .await;
        mark_job_delivery_success(
            &self.job_id,
            latency_ms,
            Some(message.clone()),
            &self.delivery_records,
            &self.last_delivery_at,
            &self.last_delivery_latency_ms,
        )
        .await;
        self.channel_deliveries.write().await.remove(&self.job_id);

        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::error_stream_event(&self.stream_id, &message),
        );
    }

    async fn notice(&self, message: String) {
        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::status_stream_event(
                &self.stream_id,
                "orchestration",
                &message,
            ),
        );
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::status_stream_event(
                &self.stream_id,
                "tool",
                &format!("tool={tool_name} {input_summary}"),
            ),
        );
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<medousa::payload_receipt::ArtifactReceiptMeta>,
        _output_receipt: Option<medousa::payload_receipt::ArtifactReceiptMeta>,
    ) {
        publish_interactive_turn_event(
            &self.stream_tx,
            medousa::interactive_turn_runtime::status_stream_event(
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
        Some(request),
    );

    let stream_id = format!("ingest-{}", Uuid::new_v4().simple());
    let (stream_tx, _stream_rx) = broadcast::channel::<InteractiveTurnStreamEvent>(512);
    {
        let mut guard = state.interactive_turn_streams.write().await;
        guard.insert(stream_id.clone(), stream_tx.clone());
    }
    let stream_url =
        medousa::ingest_stream::build_ingest_stream_url(&state.daemon_base_url, &stream_id);

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
    let continuation_scope = medousa::turn_continuation::TurnContinuationScope {
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
    };
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(120)).await;

        publish_interactive_turn_event(
            &stream_tx,
            medousa::interactive_turn_runtime::status_stream_event(
                &stream_id_for_task,
                "accepted",
                "ingest accepted; agent runtime started",
            ),
        );

        let sink: Arc<dyn AgentStreamSink> = Arc::new(IngestAgentStreamSink {
            stream_id: stream_id_for_task.clone(),
            session_id: session_id_owned,
            job_id: job_id_for_sink,
            stream_tx: stream_tx.clone(),
            delivery_target,
            dispatch_client,
            delivery_records,
            channel_deliveries,
            last_delivery_at,
            last_delivery_latency_ms,
            cancelled_streams,
            delivery_started: std::time::Instant::now(),
        });

        medousa::agent_runtime::run_agent_turn(
            &stream_id_for_task,
            interactive_request,
            &backend,
            agent_runtime.as_ref(),
            sink,
            Some(continuation_scope),
        )
        .await;

        active_jobs
            .write()
            .await
            .remove(&mapping_key_for_cleanup);

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

async fn enqueue_runtime_job(
    runtime: &RuntimeComposition,
    job: stasis::prelude::NewJob,
) -> Result<()> {
    let sdk = RuntimeSdk::new(runtime.clone());
    sdk.enqueue(job).await?;
    Ok(())
}

fn parse_diagnostics_json(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    serde_json::from_str::<Value>(trimmed).ok()
}

fn build_report_prompt(query: &str) -> String {
    format!(
        "research question:\n{query}\n\nproduce a concise evidence-first report using this structure:\n1) executive summary\n2) key findings\n3) evidence table with explicit citations [C1], [C2], ...\n4) risks and unknowns\n5) next actions\n\nrequirements:\n- every non-trivial claim must include at least one citation marker\n- include a final citations section mapping markers to sources\n- if evidence is weak, say so explicitly"
    )
}

fn extract_citations_from_payload(payload: &Value) -> Vec<JobCitationResponse> {
    let mut seen = HashSet::new();
    let mut citations = Vec::new();
    collect_citations(payload, &mut seen, &mut citations);
    citations.truncate(MAX_REPORT_CITATIONS);
    citations
}

fn collect_citations(
    value: &Value,
    seen: &mut HashSet<String>,
    citations: &mut Vec<JobCitationResponse>,
) {
    match value {
        Value::Object(map) => {
            let source = map
                .get("source")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .or_else(|| {
                    map.get("url")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                });

            if let Some(source) = source {
                let title = map
                    .get("title")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string);
                let key = format!("{}|{}", source, title.clone().unwrap_or_default());
                if seen.insert(key) {
                    citations.push(JobCitationResponse { source, title });
                }
            }

            for nested in map.values() {
                collect_citations(nested, seen, citations);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_citations(nested, seen, citations);
            }
        }
        _ => {}
    }
}

fn build_job_evidence_report(job_id: &str, payload: &Value) -> Option<JobEvidenceReportResponse> {
    let artifact_id = format!("artifact:{job_id}:diagnostics");
    let chunk_refs = chunk_json_payload(&artifact_id, payload, 360, 40);
    if chunk_refs.is_empty() {
        return None;
    }

    let claims = extract_claims_from_chunks(&artifact_id, payload, &chunk_refs);
    let extraction_record = persist_extraction_run(DAEMON_REPORT_SESSION_ID, &artifact_id, &claims)
        .map_err(|err| {
            eprintln!(
                "medousa-daemon report extraction persist error job_id={} err={err}",
                job_id
            );
            err
        })
        .ok();

    let pack = build_context_pack(BuildContextPackInput {
        session_id: DAEMON_REPORT_SESSION_ID.to_string(),
        artifact_id: artifact_id.clone(),
        claims,
        chunk_refs,
        budget_profile: ContextPackBudgetProfile {
            max_tokens: 6000,
            max_claims: 12,
            max_chunks: 24,
        },
    });

    if let Err(err) = persist_context_pack(&pack) {
        eprintln!(
            "medousa-daemon report context-pack persist error job_id={} err={err}",
            job_id
        );
    }

    let policy = VerificationPolicy::default();
    let verification = verify_context_pack(&pack, &policy);
    let verification_record = persist_verification(
        DAEMON_REPORT_SESSION_ID,
        job_id,
        "daemon_job_report",
        &policy,
        &verification,
    )
    .map_err(|err| {
        eprintln!(
            "medousa-daemon report verification persist error job_id={} err={err}",
            job_id
        );
        err
    })
    .ok();

    Some(JobEvidenceReportResponse {
        session_id: DAEMON_REPORT_SESSION_ID.to_string(),
        artifact_id,
        extraction_id: extraction_record.map(|record| record.extraction_id),
        pack_id: pack.pack_id,
        verification_id: verification_record.map(|record| record.verification_id),
        verification_state: if verification.is_verified {
            "verified".to_string()
        } else {
            "provisional".to_string()
        },
        confidence_score: verification.confidence_score,
        citation_coverage: verification.citation_coverage,
        supported_claim_ratio: verification.supported_claim_ratio,
        total_claims: verification.total_claims,
        supported_claims: verification.supported_claims,
    })
}

/// Fetches job attempts, gracefully handling the case where the backend table
/// does not exist yet (fresh database without auto-migration).
async fn get_job_attempts_graceful(
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

fn derive_job_result_status(latest_outcome: Option<&str>, attempt_count: usize) -> (String, bool) {
    if attempt_count == 0 {
        return ("queued".to_string(), false);
    }

    match latest_outcome {
        Some("Succeeded") => ("succeeded".to_string(), true),
        Some("FatalFailure") => ("failed".to_string(), true),
        Some("RetryableFailure") => ("running".to_string(), false),
        _ => ("running".to_string(), false),
    }
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

fn is_missing_runtime_table_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("the table '") && lowered.contains("does not exist")
}

async fn safe_materialize_recurring_now(
    sdk: &RuntimeSdk,
    scheduler_id: &str,
) -> Result<usize> {
    match sdk.materialize_recurring_now(scheduler_id).await {
        Ok(materialized) => Ok(materialized),
        Err(err) => {
            if is_missing_runtime_table_error(&err.to_string()) {
                Ok(0)
            } else {
                Err(err.into())
            }
        }
    }
}

async fn safe_process_once(
    sdk: &RuntimeSdk,
    queue: &str,
    worker_id: &str,
) -> Result<Option<String>> {
    match sdk.process_once(queue, worker_id).await {
        Ok(processed_job) => Ok(processed_job),
        Err(err) => {
            if is_missing_runtime_table_error(&err.to_string()) {
                Ok(None)
            } else {
                Err(err.into())
            }
        }
    }
}

async fn safe_publish_pending_events(sdk: &RuntimeSdk, limit: usize) -> Result<usize> {
    match sdk.publish_pending_events(limit).await {
        Ok(published) => Ok(published),
        Err(err) => {
            if is_missing_runtime_table_error(&err.to_string()) {
                Ok(0)
            } else {
                Err(err.into())
            }
        }
    }
}

async fn safe_stats_snapshot(sdk: &RuntimeSdk, pending_limit: usize) -> Result<RuntimeStatsSnapshot> {
    match sdk.stats_snapshot(pending_limit).await {
        Ok(snapshot) => Ok(snapshot),
        Err(err) => {
            if is_missing_runtime_table_error(&err.to_string()) {
                Ok(RuntimeStatsSnapshot::default())
            } else {
                Err(err.into())
            }
        }
    }
}

fn build_operator_first_run_guide(
    daemon_url: &str,
    heartbeat_policy: HeartbeatLanePolicy,
    heartbeat_delivery_policy: HeartbeatDeliveryPolicy,
) -> String {
    let quiet_hours = heartbeat_delivery_policy
        .quiet_hours
        .map(|window| {
            format!(
                "{:02}:00-{:02}:00 UTC",
                window.start_hour_utc, window.end_hour_utc
            )
        })
        .unwrap_or_else(|| "disabled".to_string());

    format!(
        "medousa-daemon first-run guide:\n  1) health: cargo run -p medousa --bin medousa_cli -- daemon-health --daemon-url {daemon_url}\n  2) heartbeat: cargo run -p medousa --bin medousa_cli -- daemon-heartbeat-status --daemon-url {daemon_url}\n  3) report flow: cargo run -p medousa --bin medousa_cli -- daemon-report \"Summarize runtime posture with citations\" --daemon-url {daemon_url} --poll-timeout-ms 30000\n  safety defaults: interactive_profile={} scheduled_profile={} heartbeat_min_significance={:.2} heartbeat_quiet_hours={}\n  lane safety: interactive ingress accepts interactive profiles; recurring registration allowed on interactive and scheduled lanes (set MEDOUSA_LANE_SAFETY_BLOCK_RECURRING_ON_INTERACTIVE=true to restrict chat scheduling)",
        default_policy_profile_for_lane(EngineExecutionLane::Interactive),
        default_policy_profile_for_lane(EngineExecutionLane::Scheduled),
        heartbeat_policy.min_significance,
        quiet_hours,
    )
}

fn print_usage() {
    println!(
        "medousa_daemon\n\nusage:\n  cargo run -p medousa --bin medousa_daemon -- [options]\n\ncore options:\n  --backend <backend>                       Runtime backend: in-memory|surreal-mem|surreal-kv:<path>\n  --provider <provider>                     Optional LLM provider override\n  --model <model>                           Optional LLM model override\n  --base-url <url>                          Optional provider base URL override\n  --bind <host:port>                        HTTP bind address (default: 127.0.0.1:7419)\n  --interval-ms <n>                         Scheduler tick interval ms (default: 1000)\n  --worker-id <id>                          Scheduler worker id (default: medousa-daemon)\n  --once                                    Run a single scheduler tick and exit\n\nheartbeat options:\n  --heartbeat-min-significance <0..1>       Notify threshold (default: 0.65)\n  --heartbeat-dead-letter-weight <f32>      Dead-letter contribution weight\n  --heartbeat-failed-weight <f32>           Failed-jobs contribution weight\n  --heartbeat-outbox-weight <f32>           Pending-outbox contribution weight\n  --heartbeat-activity-weight <f32>         Runtime activity contribution weight\n  --heartbeat-min-notify-interval-secs <n>  Min notify interval seconds (default: 0)\n  --heartbeat-quiet-start-hour-utc <0..23>  Quiet-hours start hour UTC\n  --heartbeat-quiet-end-hour-utc <0..23>    Quiet-hours end hour UTC\n  --heartbeat-webhook-url <url>             Optional outbound heartbeat webhook\n  --heartbeat-jsonl <path>                  Optional heartbeat JSONL sink path\n\ndashboard action auth options:\n  --dashboard-action-bearer-token <token>       Require bearer token on /action/* routes\n  --dashboard-action-required-role <role>       Require role claim on /action/* routes\n  --dashboard-action-role-claim-header <name>   Role header (default: x-stasis-role)\n\nfirst-run flow:\n  1) start daemon\n  2) run: cargo run -p medousa --bin medousa_cli -- daemon-first-run --daemon-url http://127.0.0.1:7419\n  3) run report flow from the printed guidance\n"
    );
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

fn parse_dashboard_action_auth(args: &[String]) -> Result<DashboardActionAuthConfig> {
    let bearer_token = parse_arg_or_env(
        args,
        "--dashboard-action-bearer-token",
        "MEDOUSA_DASHBOARD_ACTION_BEARER_TOKEN",
    );
    let required_role = parse_arg_or_env(
        args,
        "--dashboard-action-required-role",
        "MEDOUSA_DASHBOARD_ACTION_REQUIRED_ROLE",
    );
    let role_claim_header = parse_arg_or_env(
        args,
        "--dashboard-action-role-claim-header",
        "MEDOUSA_DASHBOARD_ACTION_ROLE_CLAIM_HEADER",
    );

    if role_claim_header.is_some() && required_role.is_none() {
        return Err(anyhow!(
            "dashboard action role claim header requires --dashboard-action-required-role"
        ));
    }

    if let Some(header) = role_claim_header.as_ref()
        && header.chars().any(char::is_whitespace)
    {
        return Err(anyhow!(
            "dashboard action role claim header must not contain whitespace"
        ));
    }

    Ok(DashboardActionAuthConfig {
        bearer_token,
        required_role,
        role_claim_header,
    })
}

fn apply_dashboard_action_auth(
    mut state: DashboardState,
    config: &DashboardActionAuthConfig,
) -> DashboardState {
    if let Some(token) = config.bearer_token.as_deref() {
        state = state.with_action_auth_bearer_token(token);
    }
    if let Some(role) = config.required_role.as_deref() {
        state = state.with_action_required_role(role);
    }
    if let Some(header_name) = config.role_claim_header.as_deref() {
        state = state.with_action_role_claim_header(header_name);
    }
    state
}

fn parse_heartbeat_policy(args: &[String]) -> Result<HeartbeatLanePolicy> {
    let mut policy = default_heartbeat_lane_policy();

    if let Some(raw) = parse_arg_or_env(
        args,
        "--heartbeat-min-significance",
        "MEDOUSA_HEARTBEAT_MIN_SIGNIFICANCE",
    ) {
        policy.min_significance = parse_ratio_value(&raw, "heartbeat min significance")?;
    }

    if let Some(raw) = parse_arg_or_env(
        args,
        "--heartbeat-dead-letter-weight",
        "MEDOUSA_HEARTBEAT_DEAD_LETTER_WEIGHT",
    ) {
        policy.dead_letter_weight =
            parse_non_negative_f32_value(&raw, "heartbeat dead-letter weight")?;
    }

    if let Some(raw) = parse_arg_or_env(
        args,
        "--heartbeat-failed-weight",
        "MEDOUSA_HEARTBEAT_FAILED_WEIGHT",
    ) {
        policy.failed_weight = parse_non_negative_f32_value(&raw, "heartbeat failed weight")?;
    }

    if let Some(raw) = parse_arg_or_env(
        args,
        "--heartbeat-outbox-weight",
        "MEDOUSA_HEARTBEAT_OUTBOX_WEIGHT",
    ) {
        policy.outbox_weight = parse_non_negative_f32_value(&raw, "heartbeat outbox weight")?;
    }

    if let Some(raw) = parse_arg_or_env(
        args,
        "--heartbeat-activity-weight",
        "MEDOUSA_HEARTBEAT_ACTIVITY_WEIGHT",
    ) {
        policy.activity_weight =
            parse_non_negative_f32_value(&raw, "heartbeat activity weight")?;
    }

    normalize_heartbeat_weights(&mut policy)?;
    Ok(policy)
}

fn parse_heartbeat_delivery_policy(args: &[String]) -> Result<HeartbeatDeliveryPolicy> {
    let min_notify_interval_secs = parse_arg_or_env(
        args,
        "--heartbeat-min-notify-interval-secs",
        "MEDOUSA_HEARTBEAT_MIN_NOTIFY_INTERVAL_SECS",
    )
    .map(|raw| {
        parse_non_negative_u64_value(&raw, "heartbeat min notify interval seconds")
    })
    .transpose()?
    .unwrap_or(0);

    let quiet_start_hour_utc = parse_arg_or_env(
        args,
        "--heartbeat-quiet-start-hour-utc",
        "MEDOUSA_HEARTBEAT_QUIET_START_HOUR_UTC",
    )
    .map(|raw| parse_hour_value(&raw, "heartbeat quiet start hour (UTC)"))
    .transpose()?;

    let quiet_end_hour_utc = parse_arg_or_env(
        args,
        "--heartbeat-quiet-end-hour-utc",
        "MEDOUSA_HEARTBEAT_QUIET_END_HOUR_UTC",
    )
    .map(|raw| parse_hour_value(&raw, "heartbeat quiet end hour (UTC)"))
    .transpose()?;

    Ok(HeartbeatDeliveryPolicy {
        min_notify_interval_secs,
        quiet_hours: parse_quiet_hours_window(quiet_start_hour_utc, quiet_end_hour_utc)?,
    })
}

fn parse_quiet_hours_window(
    start_hour_utc: Option<u8>,
    end_hour_utc: Option<u8>,
) -> Result<Option<QuietHoursWindow>> {
    match (start_hour_utc, end_hour_utc) {
        (None, None) => Ok(None),
        (Some(start_hour_utc), Some(end_hour_utc)) => {
            if start_hour_utc == end_hour_utc {
                return Err(anyhow!(
                    "heartbeat quiet-hours start and end must differ"
                ));
            }
            Ok(Some(QuietHoursWindow {
                start_hour_utc,
                end_hour_utc,
            }))
        }
        _ => Err(anyhow!(
            "heartbeat quiet-hours requires both start and end hour values"
        )),
    }
}

fn parse_ratio_value(raw: &str, label: &str) -> Result<f32> {
    let parsed = raw
        .trim()
        .parse::<f32>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if !(0.0..=1.0).contains(&parsed) {
        return Err(anyhow!("{label} must be between 0.0 and 1.0"));
    }
    Ok(parsed)
}

fn parse_non_negative_f32_value(raw: &str, label: &str) -> Result<f32> {
    let parsed = raw
        .trim()
        .parse::<f32>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if parsed < 0.0 {
        return Err(anyhow!("{label} must be non-negative"));
    }
    Ok(parsed)
}

fn parse_non_negative_u64_value(raw: &str, label: &str) -> Result<u64> {
    let parsed = raw
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    Ok(parsed)
}

fn parse_hour_value(raw: &str, label: &str) -> Result<u8> {
    let parsed = raw
        .trim()
        .parse::<u8>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if parsed > 23 {
        return Err(anyhow!("{label} must be between 0 and 23"));
    }
    Ok(parsed)
}

fn normalize_heartbeat_weights(policy: &mut HeartbeatLanePolicy) -> Result<()> {
    let weight_sum = policy.dead_letter_weight
        + policy.failed_weight
        + policy.outbox_weight
        + policy.activity_weight;
    if weight_sum <= f32::EPSILON {
        return Err(anyhow!(
            "heartbeat weights must sum to greater than zero (dead_letter/failed/outbox/activity)"
        ));
    }

    policy.dead_letter_weight /= weight_sum;
    policy.failed_weight /= weight_sum;
    policy.outbox_weight /= weight_sum;
    policy.activity_weight /= weight_sum;

    Ok(())
}

struct HeartbeatAgentContext {
    backend: String,
    provider: String,
    model: String,
    response_depth_mode: String,
    agent_runtime: Arc<medousa::agent_runtime::MedousaAgentRuntime>,
}

fn heartbeat_snapshot_from_report(report: &TickReport) -> medousa::agent_runtime::HeartbeatRuntimeSnapshot {
    medousa::agent_runtime::HeartbeatRuntimeSnapshot {
        significance: report.heartbeat_significance,
        reason: report.heartbeat_reason.clone(),
        failed_jobs: report.failed_jobs,
        dead_letter_jobs: report.dead_letter_jobs,
        pending_outbox_events: report.pending_outbox_events,
        materialized_jobs: report.materialized,
        processed_job: report.processed_job.clone(),
        published_events: report.published,
    }
}

async fn compose_heartbeat_summary(
    report: &TickReport,
    agent: Option<&HeartbeatAgentContext>,
) -> String {
    if let Some(ctx) = agent {
        if medousa::agent_runtime::heartbeat_agent_turn_enabled() {
            let snapshot = heartbeat_snapshot_from_report(report);
            if let Some(text) = medousa::agent_runtime::run_heartbeat_agent_turn(
                &snapshot,
                &ctx.backend,
                &ctx.provider,
                &ctx.model,
                &ctx.response_depth_mode,
                ctx.agent_runtime.as_ref(),
            )
            .await
            {
                return text;
            }
        }
    }

    format!(
        "heartbeat action={} significance={:.2} reason={}\nfailed={} dead_letter={} outbox_pending={}",
        report.heartbeat_action.as_str(),
        report.heartbeat_significance,
        report.heartbeat_reason,
        report.failed_jobs,
        report.dead_letter_jobs,
        report.pending_outbox_events,
    )
}

async fn dispatch_heartbeat_notifications(
    notify: &HeartbeatNotifyConfig,
    webhook_client: Option<&reqwest::Client>,
    channel_dispatch_client: &reqwest::Client,
    backend: &str,
    worker_id: &str,
    report: &TickReport,
    agent: Option<&HeartbeatAgentContext>,
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

    let summary = compose_heartbeat_summary(report, agent).await;
    let product_config = medousa::load_product_config();
    channel_delivery::dispatch_configured_heartbeat_nudges(
        channel_dispatch_client,
        &product_config,
        &summary,
    )
    .await;
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
    use std::sync::Arc;

    use chrono::{Duration, TimeZone, Utc};

    use super::{
        EngineExecutionLane, HeartbeatAction, HeartbeatDeliveryMetrics,
        HeartbeatDeliveryPolicy, HeartbeatDispatchDecision, LaneSafetyActionClass,
        QuietHoursWindow, StatusCode,
        DashboardActionAuthConfig, DashboardState, RuntimeDashboardQueryService,
        TickReport, build_report_prompt, compile_lane_prompt,
        apply_dashboard_action_auth, dashboard_router,
        default_heartbeat_lane_policy,
        decide_heartbeat_dispatch,
        derive_job_result_status, enforce_lane_safety, normalize_heartbeat_weights,
        is_missing_runtime_table_error,
        parse_dashboard_action_auth,
        extract_citations_from_payload,
        parse_heartbeat_delivery_policy,
        parse_heartbeat_policy,
        build_operator_first_run_guide,
        format_tick_report, parse_backend, tick_runtime,
    };
    use medousa::channel_delivery::extract_output_text_from_diagnostics;
    use serde_json::json;

    fn sample_notify_report() -> TickReport {
        TickReport {
            materialized: 1,
            processed_job: Some("job-123".to_string()),
            published: 1,
            lane: EngineExecutionLane::Scheduled,
            lane_policy_profile: "scheduled",
            heartbeat_action: HeartbeatAction::Notify,
            heartbeat_significance: 0.81,
            heartbeat_reason: "dead_letter_pressure count=2 score=0.81".to_string(),
            failed_jobs: 0,
            dead_letter_jobs: 2,
            pending_outbox_events: 0,
        }
    }

    async fn spawn_dashboard_test_server(
        auth_config: DashboardActionAuthConfig,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let backend = parse_backend(Some("in-memory"));
        let runtime = medousa::build_runtime(backend, None, None, None)
            .await
            .expect("runtime should build");
        let dashboard_service = Arc::new(
            RuntimeDashboardQueryService::from_runtime_composition(runtime),
        );
        let dashboard_state = apply_dashboard_action_auth(
            DashboardState::new(dashboard_service),
            &auth_config,
        );
        let app = dashboard_router(dashboard_state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("dashboard test listener should bind");
        let addr = listener
            .local_addr()
            .expect("dashboard test listener should expose local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("dashboard test server should run");
        });

        (format!("http://{}", addr), server)
    }

    async fn post_scheduler_materialize(
        base_url: &str,
        bearer_token: Option<&str>,
        role_header: Option<(&str, &str)>,
    ) -> reqwest::Response {
        let client = reqwest::Client::new();
        let mut request = client.post(format!("{base_url}/action/scheduler/materialize"));

        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some((header_name, value)) = role_header {
            request = request.header(header_name, value);
        }

        request
            .send()
            .await
            .expect("dashboard action request should succeed")
    }

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

    #[test]
    fn first_run_guide_includes_health_heartbeat_report_and_lane_safety() {
        let guide = build_operator_first_run_guide(
            "http://127.0.0.1:7419",
            default_heartbeat_lane_policy(),
            HeartbeatDeliveryPolicy::default(),
        );

        assert!(guide.contains("daemon-health"));
        assert!(guide.contains("daemon-heartbeat-status"));
        assert!(guide.contains("daemon-report"));
        assert!(guide.contains("lane safety"));
    }

    #[tokio::test]
    async fn tick_runtime_reports_scheduled_defaults_on_fresh_runtime() {
        let backend = parse_backend(Some("in-memory"));
        let runtime = medousa::build_runtime(backend, None, None, None)
            .await
            .expect("runtime should build");

        let report = tick_runtime(&runtime, "test-worker", default_heartbeat_lane_policy())
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
    fn heartbeat_weight_normalization_preserves_non_zero_sum() {
        let mut policy = default_heartbeat_lane_policy();
        policy.dead_letter_weight = 2.0;
        policy.failed_weight = 1.0;
        policy.outbox_weight = 1.0;
        policy.activity_weight = 0.0;

        normalize_heartbeat_weights(&mut policy).expect("normalization should succeed");

        let sum = policy.dead_letter_weight
            + policy.failed_weight
            + policy.outbox_weight
            + policy.activity_weight;
        assert!((sum - 1.0).abs() < 0.0001);
    }

    #[test]
    fn heartbeat_policy_parser_rejects_zero_weight_configuration() {
        let args = vec![
            "--heartbeat-dead-letter-weight".to_string(),
            "0".to_string(),
            "--heartbeat-failed-weight".to_string(),
            "0".to_string(),
            "--heartbeat-outbox-weight".to_string(),
            "0".to_string(),
            "--heartbeat-activity-weight".to_string(),
            "0".to_string(),
        ];

        let err = parse_heartbeat_policy(&args).expect_err("zero weights should fail");
        assert!(err
            .to_string()
            .contains("heartbeat weights must sum to greater than zero"));
    }

    #[test]
    fn heartbeat_delivery_policy_parser_requires_complete_quiet_window() {
        let args = vec![
            "--heartbeat-quiet-start-hour-utc".to_string(),
            "22".to_string(),
        ];

        let err = parse_heartbeat_delivery_policy(&args)
            .expect_err("partial quiet-window settings should fail");
        assert!(err
            .to_string()
            .contains("heartbeat quiet-hours requires both start and end hour values"));
    }

    #[test]
    fn dashboard_action_auth_parser_reads_cli_values() {
        let args = vec![
            "--dashboard-action-bearer-token".to_string(),
            "token-1".to_string(),
            "--dashboard-action-required-role".to_string(),
            "scheduler.admin".to_string(),
            "--dashboard-action-role-claim-header".to_string(),
            "x-medousa-role".to_string(),
        ];

        let config = parse_dashboard_action_auth(&args)
            .expect("dashboard action auth config should parse");
        assert_eq!(config.bearer_token.as_deref(), Some("token-1"));
        assert_eq!(config.required_role.as_deref(), Some("scheduler.admin"));
        assert_eq!(config.role_claim_header.as_deref(), Some("x-medousa-role"));
    }

    #[test]
    fn dashboard_action_auth_parser_rejects_role_header_without_role() {
        let args = vec![
            "--dashboard-action-role-claim-header".to_string(),
            "x-medousa-role".to_string(),
        ];

        let err = parse_dashboard_action_auth(&args)
            .expect_err("role claim header without required role should fail");
        assert!(err
            .to_string()
            .contains("requires --dashboard-action-required-role"));
    }

    #[test]
    fn dashboard_action_auth_parser_rejects_whitespace_header_name() {
        let args = vec![
            "--dashboard-action-required-role".to_string(),
            "scheduler.admin".to_string(),
            "--dashboard-action-role-claim-header".to_string(),
            "x medousa role".to_string(),
        ];

        let err = parse_dashboard_action_auth(&args)
            .expect_err("whitespace header names should fail");
        assert!(err
            .to_string()
            .contains("must not contain whitespace"));
    }

    #[tokio::test]
    async fn dashboard_action_auth_http_rejects_missing_bearer_token() {
        let auth_config = DashboardActionAuthConfig {
            bearer_token: Some("token-1".to_string()),
            required_role: None,
            role_claim_header: None,
        };

        let (base_url, server) = spawn_dashboard_test_server(auth_config).await;
        let response = post_scheduler_materialize(&base_url, None, None).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        server.abort();
    }

    #[tokio::test]
    async fn dashboard_action_auth_http_rejects_missing_required_role() {
        let auth_config = DashboardActionAuthConfig {
            bearer_token: Some("token-1".to_string()),
            required_role: Some("scheduler.admin".to_string()),
            role_claim_header: None,
        };

        let (base_url, server) = spawn_dashboard_test_server(auth_config).await;
        let response = post_scheduler_materialize(&base_url, Some("token-1"), None).await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        server.abort();
    }

    #[tokio::test]
    async fn dashboard_action_auth_http_accepts_valid_bearer_and_role_claim() {
        let auth_config = DashboardActionAuthConfig {
            bearer_token: Some("token-1".to_string()),
            required_role: Some("scheduler.admin".to_string()),
            role_claim_header: None,
        };

        let (base_url, server) = spawn_dashboard_test_server(auth_config).await;
        let response = post_scheduler_materialize(
            &base_url,
            Some("token-1"),
            Some(("x-stasis-role", "scheduler.viewer, scheduler.admin")),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        server.abort();
    }

    #[test]
    fn runtime_table_missing_error_detection_matches_expected_shape() {
        let message = "port failure: decode lease candidate: The table 'job' does not exist";
        assert!(is_missing_runtime_table_error(message));
    }

    #[test]
    fn quiet_hours_window_supports_wraparound_ranges() {
        let window = QuietHoursWindow {
            start_hour_utc: 22,
            end_hour_utc: 6,
        };

        assert!(window.contains_utc_hour(23));
        assert!(window.contains_utc_hour(1));
        assert!(!window.contains_utc_hour(12));
    }

    #[test]
    fn heartbeat_dispatch_suppresses_notifications_during_quiet_hours() {
        let report = sample_notify_report();
        let mut metrics = HeartbeatDeliveryMetrics::default();
        let policy = HeartbeatDeliveryPolicy {
            min_notify_interval_secs: 0,
            quiet_hours: Some(QuietHoursWindow {
                start_hour_utc: 22,
                end_hour_utc: 6,
            }),
        };
        let now_utc = Utc
            .with_ymd_and_hms(2026, 5, 28, 23, 0, 0)
            .single()
            .expect("datetime should be valid");

        let decision = decide_heartbeat_dispatch(&report, now_utc, policy, &mut metrics);

        assert_eq!(decision, HeartbeatDispatchDecision::SuppressedQuietHours);
        assert_eq!(metrics.notify_decisions, 1);
        assert_eq!(metrics.dispatched_notifications, 0);
        assert_eq!(metrics.suppressed_quiet_hours, 1);
    }

    #[test]
    fn heartbeat_dispatch_suppresses_notifications_when_interval_not_elapsed() {
        let report = sample_notify_report();
        let mut metrics = HeartbeatDeliveryMetrics::default();
        let policy = HeartbeatDeliveryPolicy {
            min_notify_interval_secs: 120,
            quiet_hours: None,
        };

        let first = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("datetime should be valid");
        let second = first + Duration::seconds(30);

        let first_decision = decide_heartbeat_dispatch(&report, first, policy, &mut metrics);
        let second_decision = decide_heartbeat_dispatch(&report, second, policy, &mut metrics);

        assert_eq!(first_decision, HeartbeatDispatchDecision::Dispatch);
        assert_eq!(second_decision, HeartbeatDispatchDecision::SuppressedMinInterval);
        assert_eq!(metrics.notify_decisions, 2);
        assert_eq!(metrics.dispatched_notifications, 1);
        assert_eq!(metrics.suppressed_min_interval, 1);
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
    fn lane_safety_allows_recurring_registration_on_interactive_lane() {
        let result = enforce_lane_safety(
            EngineExecutionLane::Interactive,
            LaneSafetyActionClass::RecurringRegistration,
            Some("interactive"),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn output_text_extraction_supports_prompt_diagnostics_shape() {
        let diagnostics = r#"{"output_text":"final response text"}"#;
        let output = extract_output_text_from_diagnostics(Some(diagnostics));
        assert_eq!(output.as_deref(), Some("final response text"));
    }

    #[test]
    fn output_text_extraction_supports_chat_choice_shape() {
        let diagnostics = r#"{"choices":[{"message":{"content":"assistant completion"}}]}"#;
        let output = extract_output_text_from_diagnostics(Some(diagnostics));
        assert_eq!(output.as_deref(), Some("assistant completion"));
    }

    #[test]
    fn report_prompt_builder_includes_query_and_citation_requirements() {
        let query = "Assess three practical async Rust operations trends";
        let prompt = build_report_prompt(query);

        assert!(prompt.contains(query));
        assert!(prompt.contains("evidence-first report"));
        assert!(prompt.contains("citations section"));
    }

    #[test]
    fn citation_extraction_collects_and_deduplicates_sources() {
        let payload = json!({
            "results": [
                {"title": "A", "source": "mock://one"},
                {"title": "A", "source": "mock://one"},
                {"title": "B", "url": "https://example.test/two"}
            ],
            "meta": {
                "source": "mock://meta"
            }
        });

        let citations = extract_citations_from_payload(&payload);
        let sources = citations
            .iter()
            .map(|citation| citation.source.as_str())
            .collect::<Vec<_>>();

        assert!(sources.contains(&"mock://one"));
        assert!(sources.contains(&"https://example.test/two"));
        assert!(sources.contains(&"mock://meta"));
        assert_eq!(
            sources.iter().filter(|source| **source == "mock://one").count(),
            1
        );
    }

    #[test]
    fn job_result_status_is_queued_when_no_attempts_exist() {
        let (status, terminal) = derive_job_result_status(None, 0);
        assert_eq!(status, "queued");
        assert!(!terminal);
    }

    #[test]
    fn job_result_status_is_succeeded_for_successful_attempt() {
        let (status, terminal) = derive_job_result_status(Some("Succeeded"), 1);
        assert_eq!(status, "succeeded");
        assert!(terminal);
    }

    #[test]
    fn job_result_status_keeps_retryable_failure_non_terminal() {
        let (status, terminal) = derive_job_result_status(Some("RetryableFailure"), 1);
        assert_eq!(status, "running");
        assert!(!terminal);
    }

    #[test]
    fn job_result_status_marks_fatal_failure_terminal() {
        let (status, terminal) = derive_job_result_status(Some("FatalFailure"), 2);
        assert_eq!(status, "failed");
        assert!(terminal);
    }
}
