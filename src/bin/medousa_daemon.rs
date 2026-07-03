use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use medousa::daemon::heartbeat::{
    HeartbeatDeliveryMetrics, HeartbeatDispatchDecision, SchedulerHeartbeatContext, SchedulerTickSideEffects,
    build_operator_first_run_guide, decide_heartbeat_dispatch, dispatch_heartbeat_notifications,
    format_tick_report, heartbeat_dispatch_decision_label, parse_heartbeat_delivery_policy,
    parse_heartbeat_notify_config, parse_heartbeat_policy, run_scheduler_loop, tick_runtime,
};
use medousa::daemon::ingest::{
    job_succeeded, maybe_resume_agent_turn_from_child_job, resolve_api_model_routing,
};
use medousa::daemon::router::{
    build_daemon_router, parse_dashboard_action_auth,
};
use medousa::daemon::state::AppState;
use medousa::daemon_api::DEFAULT_DAEMON_BIND;
use medousa::session_mapping;
use medousa::user_profiles::UserProfileRegistry;
use medousa::{
    PlatformBuildConfig, apply_daemon_env, build_daemon_platform, channel_delivery,
    load_product_config, parse_backend, remove_surrealkv_lock,
};
use async_trait::async_trait;
use tokio::sync::{RwLock, watch};


struct DaemonSchedulerSideEffects {
    state: AppState,
}

#[async_trait]
impl SchedulerTickSideEffects for DaemonSchedulerSideEffects {
    async fn on_processed_job(&self, job_id: &str) {
        medousa::workspace::notify_workspace_event(
            medousa::workspace::WorkspaceDomainEvent::StasisJobChanged {
                job_id: job_id.to_string(),
            },
        );
        medousa::feed_sink::maybe_publish_recurring_job_feed(self.state.composition(), job_id).await;
        if job_succeeded(self.state.composition(), job_id).await {
            let _ = maybe_resume_agent_turn_from_child_job(&self.state, job_id).await;
        }
    }

    async fn run_retention_if_due(&self, now_utc: DateTime<Utc>) {
        if !self.state.retention_config.enabled() {
            return;
        }

        let interval = medousa::session_retention::retention_tick_interval();
        let should_run = {
            let last = *self.state.last_retention_at.read().await;
            last.is_none_or(|at| {
                now_utc
                    .signed_duration_since(at)
                    .to_std()
                    .unwrap_or(interval)
                    >= interval
            })
        };
        if !should_run {
            return;
        }

        let report = medousa::session_retention::run_retention_pass(
            &self.state.retention_config,
            Some(self.state.platform.memory_operations()),
            self.state.composition(),
        )
        .await;
        *self.state.last_retention_at.write().await = Some(now_utc);
        if report.locus_raw_deleted > 0
            || report.runtime_jobs_pruned > 0
            || report.runtime_attempts_pruned > 0
            || report.runtime_outbox_pruned > 0
        {
            tracing::info!(
                locus_raw = report.locus_raw_deleted,
                jobs = report.runtime_jobs_pruned,
                attempts = report.runtime_attempts_pruned,
                outbox = report.runtime_outbox_pruned,
                "retention pass completed"
            );
        }
    }
}

/// Default backstop on concurrently in-flight HTTP requests. A high ceiling
/// that protects against connection/FD exhaustion (e.g. a mobile reconnect
/// storm) without affecting normal load. Override with
/// `MEDOUSA_DAEMON_MAX_CONCURRENCY`.
const DEFAULT_MAX_CONCURRENCY: usize = 1024;

fn resolve_max_concurrency() -> usize {
    std::env::var("MEDOUSA_DAEMON_MAX_CONCURRENCY")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_CONCURRENCY)
}

#[tokio::main]
async fn main() -> Result<()> {
    medousa::observability::init_tracing_from_env();

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }

    // Raise the soft FD limit before binding sockets. The daemon multiplexes
    // many sockets (HTTP clients, SSE streams, DB, Iroh, grapheme); the default
    // soft RLIMIT_NOFILE (often 256 on macOS) is the FD-pressure failure class
    // behind persist drops and wasix panics under reconnect storms.
    match medousa::comms::raise_nofile_limit(medousa::comms::DEFAULT_TARGET_NOFILE) {
        Ok(limits) => tracing::info!(
            soft = limits.soft,
            hard = limits.hard,
            "raised RLIMIT_NOFILE"
        ),
        Err(err) => tracing::warn!(error = %err, "could not raise RLIMIT_NOFILE"),
    }

    let backend_name = find_arg_value(&args, "--backend")
        .unwrap_or("in-memory")
        .to_string();
    // Load an optional `.env` overlay before any native env application so the
    // file can supply stasis/grapheme settings (timezone, module timeouts,
    // feature toggles) without overriding values the native config flow sets.
    if let Some(path) = medousa::load_dotenv_overlay() {
        tracing::info!(path = %path.display(), "loaded env overlay");
    }
    // Default local SurrealKV to grouped fsync (~200ms) instead of fsync-per-commit so
    // chat-turn writes don't stall on disk. Gated strictly to the surrealkv backend:
    // SurrealDB's in-memory engine errors if `sync` is set without a persist path.
    // An explicit value from the environment or `.env` overlay (incl. `every`) wins.
    {
        let lowered = backend_name.to_ascii_lowercase();
        let is_surrealkv = lowered == "surreal-kv" || lowered.starts_with("surreal-kv:");
        if is_surrealkv && std::env::var_os("SURREAL_DATASTORE_SYNC_DATA").is_none() {
            // SAFETY: set before any DB connect (and before extra threads read it).
            unsafe { std::env::set_var("SURREAL_DATASTORE_SYNC_DATA", "200ms") };
            tracing::info!("SurrealKV sync mode defaulted to 200ms (grouped fsync)");
        }
    }
    apply_daemon_env(&load_product_config());
    medousa::runtime::stasis_otel::prepare_stasis_otel_from_tui_defaults();
    medousa::apply_workshop_llm_env();
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
    let heartbeat_notify = parse_heartbeat_notify_config(&args);
    let heartbeat_policy = parse_heartbeat_policy(&args)?;
    let heartbeat_delivery_policy = parse_heartbeat_delivery_policy(&args)?;
    let dashboard_action_auth = parse_dashboard_action_auth(&args)?;

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
    tracing::info!(%addr, "acquired bind address, initializing runtime");

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
        session_id: medousa::runtime_session::runtime_bootstrap_session_id().to_string(),
        backend_label: backend_name.clone(),
    };

    let platform = build_daemon_platform(backend.clone(), platform_config)
        .await
        .context("failed to build medousa platform runtime")?;

    let identity_service = platform.identity_service();
    let profile_registry = Arc::new(std::sync::RwLock::new(UserProfileRegistry::load_or_bootstrap()));
    medousa::user_profiles::init_workshop_profile_registry(profile_registry.clone());

    if once {
        let report = tick_runtime(platform.composition(), &worker_id, heartbeat_policy, None).await?;
        tracing::info!("{}", format_tick_report("medousa-daemon once", &report));
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
        } else if report.heartbeat_action == medousa::engine_context::HeartbeatAction::Notify {
            tracing::debug!(
                decision = heartbeat_dispatch_decision_label(dispatch_decision),
                "heartbeat notify suppressed (once mode)"
            );
        }
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
    let retention_config = medousa::session_retention::SessionRetentionConfig::from_env();

    let state = AppState {
        platform: platform.clone(),
        daemon_base_url: medousa::daemon_api::resolve_daemon_public_base_url(&bind),
        interactive_turn_streams: medousa::daemon::turn_stream_registry::new_turn_stream_registry(),
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
        cancelled_interactive_turns: Arc::new(RwLock::new(HashSet::new())),
        turn_tickets: medousa::turn_ticket::new_registry(),
        session_runtime_configs: Arc::new(RwLock::new(HashMap::new())),
        backend: backend_name,
        worker_id: worker_id.clone(),
        identity_service,
        profile_registry,
        last_tick_at: Arc::new(RwLock::new(None)),
        last_heartbeat_report: Arc::new(RwLock::new(None)),
        heartbeat_policy,
        heartbeat_delivery_policy,
        heartbeat_metrics: Arc::new(RwLock::new(HeartbeatDeliveryMetrics::default())),
        heartbeat_notify,
        webhook_client,
        retention_config,
        last_retention_at: Arc::new(RwLock::new(None)),
        last_context_usage_by_session: Arc::new(RwLock::new(HashMap::new())),
        client_registry: medousa::browser_handlers::ClientRegistry::new(),
    };

    medousa::turn_worker_notify::register_ingest_channel_delivery_bridge(
        medousa::turn_worker_notify::IngestChannelDeliveryBridge::new(
            state.channel_dispatch_client.clone(),
            state.job_delivery_records.clone(),
            state.channel_deliveries.clone(),
            state.last_delivery_at.clone(),
            state.last_delivery_latency_ms.clone(),
        ),
    );
    medousa::turn_worker_notify::register_parent_turn_stream_registry(
        state.interactive_turn_streams.clone(),
    );

    medousa::workspace::init_persist_writer();
    medousa::engine_recovery::run_startup_turn_recovery().await;
    medousa::workspace::init_workspace_hub(Arc::new(state.composition().clone()));
    if let Some(hub) = medousa::workspace::workspace_hub() {
        hub.refresh_now().await;
    }

    let mut mdns_advertiser: Option<medousa::pairing::mdns::MdnsAdvertiser> = None;
    #[cfg(feature = "iroh-transport")]
    let mut iroh_gateway_hold: Option<medousa::iroh_transport::WorkshopGateway> = None;
    let pairing_router = if medousa::pairing::pairing_enabled_from_env() {
        let identity = medousa::pairing::DeviceIdentity::load_or_create()
            .context("failed to load pairing device identity")?;
        #[cfg(feature = "iroh-transport")]
        let iroh_info = if medousa::iroh_transport::iroh_enabled_from_env() {
            let upstream = format!("http://{addr}");
            let secret = medousa::iroh_transport::secret_key_from_pairing_identity(
                identity.signing_key(),
            );
            match medousa::iroh_transport::spawn_workshop_gateway_with_secret(&upstream, secret)
                .await
            {
                Ok(gateway) => {
                    let info = medousa::pairing::IrohWorkshopInfo {
                        ticket: gateway.info().ticket.clone(),
                        endpoint_id: gateway.info().endpoint_id.clone(),
                    };
                    tracing::info!(
                        endpoint_id = %info.endpoint_id,
                        ticket = %info.ticket,
                        "iroh gateway active"
                    );
                    iroh_gateway_hold = Some(gateway);
                    Some(info)
                }
                Err(err) => {
                    tracing::warn!(error = %err, "iroh gateway failed");
                    None
                }
            }
        } else {
            None
        };
        #[cfg(not(feature = "iroh-transport"))]
        let iroh_info: Option<medousa::pairing::IrohWorkshopInfo> =
            if medousa::iroh_transport::iroh_enabled_from_env() {
                tracing::warn!(
                    "MEDOUSA_IROH=1 requires rebuild with --features iroh-transport"
                );
                None
            } else {
                None
            };
        let pairing_service = Arc::new(medousa::pairing::PairingService::new(
            identity,
            medousa::pairing::resolve_advertise_address(&bind),
            medousa::pairing::resolve_peer_name(),
            model.map(|value| value.to_string()),
            iroh_info,
        ));
        if medousa::pairing::mdns_should_advertise(&bind) {
            let mut txt = std::collections::HashMap::new();
            txt.insert("dv".to_string(), pairing_service.device_id().to_string());
            txt.insert("pn".to_string(), pairing_service.peer_name().to_string());
            txt.insert(
                "pv".to_string(),
                medousa::pairing::PROTOCOL_VERSION.to_string(),
            );
            txt.insert("pf".to_string(), pairing_service.capability_flags());
            txt.insert(
                "ar".to_string(),
                pairing_service.auth_required_flag().to_string(),
            );
            if let Some(model_name) = pairing_service.model_descriptor() {
                txt.insert("md".to_string(), model_name.to_string());
            }
            match medousa::pairing::mdns::MdnsAdvertiser::register(
                pairing_service.peer_name(),
                "medousa-core.local.",
                pairing_service.parse_advertise_port(),
                txt,
            ) {
                Ok(advertiser) => {
                    tracing::info!(
                        port = pairing_service.parse_advertise_port(),
                        "mDNS pairing service _medousa._tcp"
                    );
                    mdns_advertiser = Some(advertiser);
                }
                Err(err) => {
                    tracing::warn!(error = %err, "mDNS pairing advertise failed");
                }
            }
        }
        tracing::info!(
            device_id = %pairing_service.device_id(),
            "LAN pairing ready (GET /qr)"
        );
        let warm_service = pairing_service.clone();
        medousa::home_push::register_home_push(Arc::new(medousa::home_push::HomePushService::new(
            pairing_service.clone(),
        )));
        medousa::home_live_activity::register_home_live_activity(Arc::new(
            medousa::home_live_activity::HomeLiveActivityService::new(pairing_service.clone()),
        ));
        medousa::home_widget_push::register_home_widget_push(Arc::new(
            medousa::home_widget_push::HomeWidgetPushService::new(pairing_service.clone()),
        ));
        tokio::spawn(async move {
            if let Err(err) = warm_service.current_qr().await {
                tracing::warn!(error = %err, "pairing QR warm-up failed");
            }
        });
        Some(
            medousa::pairing_handlers::routes().with_state(medousa::pairing_handlers::PairingApiState {
                service: pairing_service,
            }),
        )
    } else {
        None
    };

    let mut app = build_daemon_router(state.clone(), &dashboard_action_auth);
    if let Some(pairing_router) = pairing_router {
        app = app.merge(pairing_router);
    }
    let _mdns_advertiser = mdns_advertiser;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let scheduler_ctx = SchedulerHeartbeatContext {
        platform: state.platform.clone(),
        heartbeat_policy: state.heartbeat_policy,
        heartbeat_delivery_policy: state.heartbeat_delivery_policy,
        heartbeat_notify: state.heartbeat_notify.clone(),
        webhook_client: state.webhook_client.clone(),
        channel_dispatch_client: state.channel_dispatch_client.clone(),
        backend: state.backend.clone(),
        worker_id: state.worker_id.clone(),
        last_tick_at: state.last_tick_at.clone(),
        last_heartbeat_report: state.last_heartbeat_report.clone(),
        heartbeat_metrics: state.heartbeat_metrics.clone(),
        default_runtime_config: state.default_runtime_config.clone(),
        resolve_agent_routing: resolve_api_model_routing,
    };
    let scheduler_side_effects = Arc::new(DaemonSchedulerSideEffects {
        state: state.clone(),
    });
    tokio::spawn(async move {
        run_scheduler_loop(
            scheduler_ctx,
            worker_id,
            interval_ms,
            shutdown_rx,
            scheduler_side_effects,
        )
        .await;
    });

    tokio::spawn(async {
        let registry = medousa::model_capability_registry::registry();
        let providers = medousa::model_capability_registry::default_refresh_providers();
        if registry.any_stale(&providers) {
            let result = registry.refresh(None).await;
            if !result.refreshed.is_empty() {
                tracing::info!(
                    providers = ?result.refreshed,
                    "model catalog refreshed"
                );
            }
            for failure in result.failures {
                tracing::warn!(
                    provider = %failure.provider,
                    error = %failure.message,
                    "model catalog refresh failed"
                );
            }
        }
    });

    tracing::info!(%addr, "listening");
    tracing::info!(url = %format!("http://{addr}/dashboard"), "dashboard");
    if dashboard_action_auth.bearer_token.is_some() {
        tracing::info!("dashboard actions require bearer token auth");
    }
    if let Some(required_role) = dashboard_action_auth.required_role.as_deref() {
        let role_claim_header = dashboard_action_auth
            .role_claim_header
            .as_deref()
            .unwrap_or("x-stasis-role");
        tracing::info!(
            role = required_role,
            header = role_claim_header,
            "dashboard actions require role claim"
        );
    }
    tracing::info!(
        "{}",
        build_operator_first_run_guide(
            &format!("http://{addr}"),
            heartbeat_policy,
            heartbeat_delivery_policy,
        )
    );
    tracing::info!(
        status = %medousa::observability::tracing_status_line(),
        "observability initialized"
    );

    #[cfg(feature = "iroh-transport")]
    let _iroh_gateway_hold = iroh_gateway_hold;

    // Connection-limit backstop: a shared (global) tower concurrency limit so the
    // daemon sheds load instead of being FD-exhausted under a request/reconnect
    // storm. The semaphore is shared across all cloned per-connection services.
    let max_concurrency = resolve_max_concurrency();
    let app = app.layer(tower::limit::GlobalConcurrencyLimitLayer::new(max_concurrency));
    tracing::info!(max_concurrency, "max in-flight request concurrency");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx.send(true);
        medousa::workspace::flush_persist_writer().await;
        tracing::info!("stopping");
        remove_surrealkv_lock(&parse_backend(Some(&state.backend)));
    })
    .await
    .context("medousa-daemon server failed")?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, TimeZone, Utc};

    use medousa::daemon::heartbeat::{
        HeartbeatDeliveryMetrics, HeartbeatDeliveryPolicy, HeartbeatDispatchDecision,
        QuietHoursWindow, TickReport, build_operator_first_run_guide, decide_heartbeat_dispatch,
        format_tick_report, is_missing_runtime_table_error, normalize_heartbeat_weights,
        parse_heartbeat_delivery_policy, parse_heartbeat_policy, tick_runtime,
    };
    use medousa::daemon::jobs::{
        build_report_prompt, derive_job_result_status, enforce_lane_safety,
        extract_citations_from_payload,
    };
    use medousa::daemon::router::{
        DashboardActionAuthConfig, apply_dashboard_action_auth, parse_dashboard_action_auth,
    };
    use stasis::dashboard::{
        DashboardState, RuntimeDashboardQueryService, router as dashboard_router,
    };

    use medousa::engine_context::{
        EngineExecutionLane, HeartbeatAction, LaneSafetyActionClass,
        default_heartbeat_lane_policy,
    };
    use super::parse_backend;
    use medousa::engine_context::compile_default_lane_prompt;

    fn compile_lane_prompt(lane: EngineExecutionLane, prompt: &str) -> String {
        compile_default_lane_prompt(lane, prompt)
    }
    use axum::http::StatusCode;
    use axum::Router;
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
        let dashboard = dashboard_router(dashboard_state);
        let app = Router::new().merge(dashboard);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve dashboard");
        });
        (format!("http://{addr}"), handle)
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

        let report = tick_runtime(&runtime, "test-worker", default_heartbeat_lane_policy(), None)
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
