use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use axum::http::StatusCode;
use chrono::{DateTime, Timelike, Utc};
use serde::Serialize;
use stasis::prelude::RuntimeComposition;
use stasis::sdk::runtime_sdk::{RuntimeSdk, RuntimeStatsSnapshot};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, watch};

use crate::MedousaPlatformRuntime;
use crate::daemon_api::{
    HeartbeatDeliveryMetricsResponse, HeartbeatDeliveryPolicyResponse, HeartbeatPolicyResponse,
    HeartbeatStatusResponse,
};
use crate::engine_context::{
    EngineExecutionLane, HeartbeatAction, HeartbeatLanePolicy, HeartbeatSignals,
    LaneSafetyActionClass, default_heartbeat_lane_policy, default_policy_profile_for_lane,
    evaluate_heartbeat_significance, validate_lane_action,
};
use crate::session_mapping;

#[derive(Debug, Clone)]
pub struct TickReport {
    pub materialized: usize,
    pub processed_job: Option<String>,
    pub published: usize,
    pub lane: EngineExecutionLane,
    pub lane_policy_profile: &'static str,
    pub heartbeat_action: HeartbeatAction,
    pub heartbeat_significance: f32,
    pub heartbeat_reason: String,
    pub failed_jobs: usize,
    pub dead_letter_jobs: usize,
    pub pending_outbox_events: usize,
}

#[derive(Clone, Debug)]
pub struct HeartbeatNotifyConfig {
    pub webhook_url: Option<String>,
    pub jsonl_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
pub struct HeartbeatDeliveryPolicy {
    pub min_notify_interval_secs: u64,
    pub quiet_hours: Option<QuietHoursWindow>,
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
pub struct QuietHoursWindow {
    pub start_hour_utc: u8,
    pub end_hour_utc: u8,
}

impl QuietHoursWindow {
    pub fn contains_utc_hour(self, hour: u8) -> bool {
        if self.start_hour_utc < self.end_hour_utc {
            hour >= self.start_hour_utc && hour < self.end_hour_utc
        } else {
            hour >= self.start_hour_utc || hour < self.end_hour_utc
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HeartbeatDeliveryMetrics {
    pub tick_evaluations: u64,
    pub notify_decisions: u64,
    pub dispatched_notifications: u64,
    pub suppressed_quiet_hours: u64,
    pub suppressed_min_interval: u64,
    pub last_notify_decision_at_utc: Option<DateTime<Utc>>,
    pub last_dispatched_at_utc: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartbeatDispatchDecision {
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

pub struct HeartbeatAgentDispatchContext {
    pub backend: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub agent_runtime: Arc<crate::agent_runtime::MedousaAgentRuntime>,
}

pub type AgentRoutingResolver =
    fn(Option<&str>, &session_mapping::IngestSessionRuntimeConfig) -> (String, String);

#[derive(Clone)]
pub struct SchedulerHeartbeatContext {
    pub platform: Arc<MedousaPlatformRuntime>,
    pub heartbeat_policy: HeartbeatLanePolicy,
    pub heartbeat_delivery_policy: HeartbeatDeliveryPolicy,
    pub heartbeat_notify: HeartbeatNotifyConfig,
    pub webhook_client: Option<reqwest::Client>,
    pub channel_dispatch_client: reqwest::Client,
    pub backend: String,
    pub worker_id: String,
    pub last_tick_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub last_heartbeat_report: Arc<RwLock<Option<TickReport>>>,
    pub heartbeat_metrics: Arc<RwLock<HeartbeatDeliveryMetrics>>,
    pub default_runtime_config: session_mapping::IngestSessionRuntimeConfig,
    pub resolve_agent_routing: AgentRoutingResolver,
}

#[async_trait]
pub trait SchedulerTickSideEffects: Send + Sync {
    async fn on_processed_job(&self, job_id: &str);
    async fn run_retention_if_due(&self, now_utc: DateTime<Utc>);
}

pub async fn run_scheduler_loop(
    ctx: SchedulerHeartbeatContext,
    worker_id: String,
    interval_ms: u64,
    mut shutdown_rx: watch::Receiver<bool>,
    side_effects: Arc<dyn SchedulerTickSideEffects>,
) {
    loop {
        match tick_runtime(
            ctx.platform.composition(),
            &worker_id,
            ctx.heartbeat_policy,
        )
        .await
        {
            Ok(report) => {
                let now_utc = Utc::now();
                *ctx.last_tick_at.write().await = Some(now_utc);
                *ctx.last_heartbeat_report.write().await = Some(report.clone());
                if report.materialized > 0
                    || report.processed_job.is_some()
                    || report.published > 0
                    || report.heartbeat_action == HeartbeatAction::Notify
                {
                    eprintln!("{}", format_tick_report("medousa-daemon tick", &report));
                }

                if let Some(ref job_id) = report.processed_job {
                    side_effects.on_processed_job(job_id).await;
                }

                let dispatch_decision = {
                    let mut metrics = ctx.heartbeat_metrics.write().await;
                    decide_heartbeat_dispatch(
                        &report,
                        now_utc,
                        ctx.heartbeat_delivery_policy,
                        &mut metrics,
                    )
                };

                if dispatch_decision == HeartbeatDispatchDecision::Dispatch {
                    let (provider, model) = (ctx.resolve_agent_routing)(
                        None,
                        &ctx.default_runtime_config,
                    );
                    let agent = HeartbeatAgentDispatchContext {
                        backend: ctx.backend.clone(),
                        provider,
                        model,
                        response_depth_mode: ctx
                            .default_runtime_config
                            .response_depth_mode
                            .clone(),
                        agent_runtime: ctx.platform.agent_handle(),
                    };
                    dispatch_heartbeat_notifications(
                        &ctx.heartbeat_notify,
                        ctx.webhook_client.as_ref(),
                        &ctx.channel_dispatch_client,
                        &ctx.backend,
                        &ctx.worker_id,
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

                side_effects.run_retention_if_due(now_utc).await;
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

pub async fn tick_runtime(
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

pub async fn build_heartbeat_status_response(
    composition: &RuntimeComposition,
    heartbeat_policy: HeartbeatLanePolicy,
    heartbeat_delivery_policy: HeartbeatDeliveryPolicy,
    last_tick_at_utc: Option<DateTime<Utc>>,
    maybe_report: Option<TickReport>,
    metrics: HeartbeatDeliveryMetrics,
    now_utc: DateTime<Utc>,
) -> Result<HeartbeatStatusResponse, (StatusCode, String)> {
    let report = match maybe_report {
        Some(report) => report,
        None => compute_heartbeat_snapshot_report(composition, heartbeat_policy).await?,
    };

    let in_quiet_hours = heartbeat_delivery_policy
        .quiet_hours
        .map(|window| window.contains_utc_hour(now_utc.hour() as u8))
        .unwrap_or(false);

    Ok(HeartbeatStatusResponse {
        lane: report.lane.as_str().to_string(),
        lane_policy_profile: report.lane_policy_profile.to_string(),
        action: report.heartbeat_action.as_str().to_string(),
        significance: report.heartbeat_significance,
        reason: report.heartbeat_reason,
        policy: to_heartbeat_policy_response(heartbeat_policy),
        delivery_policy: to_heartbeat_delivery_policy_response(
            heartbeat_delivery_policy,
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
    })
}

async fn compute_heartbeat_snapshot_report(
    composition: &RuntimeComposition,
    heartbeat_policy: HeartbeatLanePolicy,
) -> Result<TickReport, (StatusCode, String)> {
    let sdk = RuntimeSdk::new(composition.clone());
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
        heartbeat_policy,
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

pub fn decide_heartbeat_dispatch(
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

pub fn heartbeat_dispatch_decision_label(decision: HeartbeatDispatchDecision) -> &'static str {
    match decision {
        HeartbeatDispatchDecision::NotRequired => "not_required",
        HeartbeatDispatchDecision::Dispatch => "dispatch",
        HeartbeatDispatchDecision::SuppressedQuietHours => "suppressed_quiet_hours",
        HeartbeatDispatchDecision::SuppressedMinInterval => "suppressed_min_interval",
    }
}

pub fn parse_heartbeat_notify_config(args: &[String]) -> HeartbeatNotifyConfig {
    HeartbeatNotifyConfig {
        webhook_url: parse_arg_or_env(
            args,
            "--heartbeat-webhook-url",
            "MEDOUSA_HEARTBEAT_WEBHOOK_URL",
        ),
        jsonl_path: parse_arg_or_env(args, "--heartbeat-jsonl", "MEDOUSA_HEARTBEAT_JSONL")
            .map(PathBuf::from),
    }
}

pub fn parse_heartbeat_policy(args: &[String]) -> Result<HeartbeatLanePolicy> {
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

pub fn parse_heartbeat_delivery_policy(args: &[String]) -> Result<HeartbeatDeliveryPolicy> {
    let min_notify_interval_secs = parse_arg_or_env(
        args,
        "--heartbeat-min-notify-interval-secs",
        "MEDOUSA_HEARTBEAT_MIN_NOTIFY_INTERVAL_SECS",
    )
    .map(|raw| parse_non_negative_u64_value(&raw, "heartbeat min notify interval seconds"))
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

pub fn normalize_heartbeat_weights(policy: &mut HeartbeatLanePolicy) -> Result<()> {
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

fn heartbeat_snapshot_from_report(report: &TickReport) -> crate::agent_runtime::HeartbeatRuntimeSnapshot {
    crate::agent_runtime::HeartbeatRuntimeSnapshot {
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
    agent: Option<&HeartbeatAgentDispatchContext>,
) -> String {
    if let Some(ctx) = agent {
        if crate::agent_runtime::heartbeat_agent_turn_enabled() {
            let snapshot = heartbeat_snapshot_from_report(report);
            if let Some(text) = crate::agent_runtime::run_heartbeat_agent_turn(
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

pub async fn dispatch_heartbeat_notifications(
    notify: &HeartbeatNotifyConfig,
    webhook_client: Option<&reqwest::Client>,
    channel_dispatch_client: &reqwest::Client,
    backend: &str,
    worker_id: &str,
    report: &TickReport,
    agent: Option<&HeartbeatAgentDispatchContext>,
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
    let product_config = crate::load_product_config();
    crate::channel_delivery::dispatch_configured_heartbeat_nudges(
        channel_dispatch_client,
        &product_config,
        &summary,
    )
    .await;
}

async fn append_heartbeat_jsonl(path: &Path, notification: &HeartbeatNotification) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "failed creating heartbeat sink directory {}",
                parent.display()
            )
        })?;
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

pub fn format_tick_report(prefix: &str, report: &TickReport) -> String {
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

pub fn build_operator_first_run_guide(
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

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("medousa daemon error: {err}"),
    )
}

pub fn is_missing_runtime_table_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("the table '") && lowered.contains("does not exist")
}

pub async fn safe_materialize_recurring_now(
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

pub async fn safe_process_once(
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

pub async fn safe_publish_pending_events(sdk: &RuntimeSdk, limit: usize) -> Result<usize> {
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

pub async fn safe_stats_snapshot(
    sdk: &RuntimeSdk,
    pending_limit: usize,
) -> Result<RuntimeStatsSnapshot> {
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
