use std::cmp::Reverse;
use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use medousa::identity_memory::resolve_identity_user_id;
use medousa::engine_context::{
    EngineExecutionLane, compile_default_lane_prompt,
    default_policy_profile_for_lane,
};
use medousa::{
    AdapterDeliveryOutcome, DaemonStatsResponse, EnqueueReportRequest, EnqueueResponse,
    HealthResponse, IngestRequest, IngestResponse, fetch_job_result, wait_for_ask_delivery,
    default_delivery_timeout,
    HeartbeatStatusResponse,
    IdentityContextRequest,
    JobReportResponse,
    RegisterRecurringPromptRequest, RegisterRecurringResponse, build_runtime, parse_backend,
    process_once, publish_pending, resolve_daemon_url, resolve_llm_base_url, resolve_llm_provider,
    resolve_llm_target,
};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep};
use stasis::application::orchestration::runtime_job_payloads::{
    AgentSessionJobPayload, AgentSessionParticipantPayload, AgentToolCallMode, PromptJobPayload,
};
use stasis::application::orchestration::runtime_workflow_job_builder::RuntimeWorkflowJobBuilder;
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, CommitEntityUpdateResponse, GetIdentityContextResponse,
    IdentityEntityType,
    ListEntityHistoryRequest, ListEntityHistoryResponse, ProposeEntityUpdateRequest,
    ProposeEntityUpdateResponse, RollbackEntityVersionRequest, RollbackEntityVersionResponse,
    UpdateSource,
};
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::prelude::RuntimeComposition;

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    match args[0].as_str() {
        "ask" => {
            let backend = parse_backend(find_arg_value(&args, "--backend"));
            let provider = find_arg_value(&args, "--provider");
            let model = find_arg_value(&args, "--model");
            let base_url = find_arg_value(&args, "--base-url");
            let runtime = build_runtime(backend, provider, model, base_url).await?;
            let prompt = args
                .get(1)
                .ok_or_else(|| anyhow!("missing prompt: medousa ask <prompt>"))?;
            run_ask(&runtime, prompt).await
        }
        "llm" => {
            let backend = parse_backend(find_arg_value(&args, "--backend"));
            let provider = find_arg_value(&args, "--provider");
            let base_url = find_arg_value(&args, "--base-url");
            let prompt = args
                .get(1)
                .ok_or_else(|| anyhow!("missing prompt: medousa llm <prompt>"))?;
            let model = find_arg_value(&args, "--model");
            let runtime = build_runtime(backend, provider, model, base_url).await?;
            run_llm(&runtime, prompt, provider, model, base_url).await
        }
        "daemon-health" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_health(&daemon_url).await
        }
        "daemon-stats" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_stats(&daemon_url).await
        }
        "daemon-heartbeat-status" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_heartbeat_status(&daemon_url).await
        }
        "daemon-first-run" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_first_run(&daemon_url, &args).await
        }
        "daemon-ask" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_ask(&daemon_url, &args).await
        }
        "daemon-report" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_report(&daemon_url, &args).await
        }
        "daemon-job-report" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            let job_id = args
                .get(1)
                .ok_or_else(|| anyhow!("missing job id: medousa-cli daemon-job-report <job_id>"))?;
            run_daemon_job_report(&daemon_url, job_id).await
        }
        "daemon-watch-add" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            let timezone = find_arg_value(&args, "--tz").unwrap_or("UTC");
            let cron_expr = args
                .get(1)
                .ok_or_else(|| anyhow!("missing cron expression: medousa-cli daemon-watch-add <cron_expr> <prompt> [--tz UTC]"))?;
            let prompt_parts = args
                .iter()
                .skip(2)
                .take_while(|arg| !arg.starts_with("--"))
                .cloned()
                .collect::<Vec<_>>();
            if prompt_parts.is_empty() {
                return Err(anyhow!(
                    "missing prompt: medousa-cli daemon-watch-add <cron_expr> <prompt> [--tz UTC]"
                ));
            }
            let prompt = prompt_parts.join(" ");
            run_daemon_watch_add(&daemon_url, cron_expr, timezone, &prompt).await
        }
        "daemon-identity-context" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_context(&daemon_url, &args).await
        }
        "daemon-identity-inspect" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_inspect(&daemon_url, &args).await
        }
        "daemon-identity-propose" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_propose(&daemon_url, &args).await
        }
        "daemon-identity-update" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_update(&daemon_url, &args).await
        }
        "daemon-identity-commit" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_commit(&daemon_url, &args).await
        }
        "daemon-identity-history" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_history(&daemon_url, &args).await
        }
        "daemon-identity-review" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_review(&daemon_url, &args).await
        }
        "daemon-identity-explain" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_explain(&daemon_url, &args).await
        }
        "daemon-identity-rollback" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_rollback(&daemon_url, &args).await
        }
        _ => {
            print_usage();
            Ok(())
        }
    }
}

async fn run_daemon_health(daemon_url: &str) -> Result<()> {
    let client = Client::new();
    let response = client
        .get(format!("{daemon_url}/health"))
        .send()
        .await?
        .error_for_status()?;
    let payload: HealthResponse = response.json().await?;
    println!(
        "status={} backend={} worker={} now={}",
        payload.status, payload.backend, payload.worker_id, payload.now_utc
    );
    println!(
        "agent_runtime_version={} tool_registry_count={} last_agent_turn_latency_ms={:?} last_agent_turn_at={:?}",
        payload.agent_runtime_version,
        payload.tool_registry_count,
        payload.last_agent_turn_latency_ms,
        payload.last_agent_turn_at_utc
    );
    Ok(())
}

async fn run_daemon_stats(daemon_url: &str) -> Result<()> {
    let client = Client::new();
    let response = client
        .get(format!("{daemon_url}/v1/stats"))
        .send()
        .await?
        .error_for_status()?;
    let payload: DaemonStatsResponse = response.json().await?;
    println!(
        "jobs: enqueued={} running={} succeeded={} failed={} dead_letter={}",
        payload.enqueued_jobs,
        payload.running_jobs,
        payload.succeeded_jobs,
        payload.failed_jobs,
        payload.dead_letter_jobs
    );
    println!(
        "outbox_pending={} recurring_definitions={} last_tick={:?}",
        payload.pending_outbox_events, payload.recurring_definitions, payload.last_tick_at_utc
    );
    Ok(())
}

async fn run_daemon_heartbeat_status(daemon_url: &str) -> Result<()> {
    let client = Client::new();
    let response = client
        .get(format!("{daemon_url}/v1/heartbeat/status"))
        .send()
        .await?
        .error_for_status()?;
    let payload: HeartbeatStatusResponse = response.json().await?;
    println!(
        "heartbeat action={} significance={:.2} lane={} policy={} reason={}",
        payload.action,
        payload.significance,
        payload.lane,
        payload.lane_policy_profile,
        payload.reason,
    );
    println!(
        "heartbeat policy min_significance={:.2} weights(dead_letter={:.2}, failed={:.2}, outbox={:.2}, activity={:.2})",
        payload.policy.min_significance,
        payload.policy.dead_letter_weight,
        payload.policy.failed_weight,
        payload.policy.outbox_weight,
        payload.policy.activity_weight,
    );
    println!(
        "heartbeat delivery min_interval_secs={} quiet_hours_start_utc={:?} quiet_hours_end_utc={:?} in_quiet_hours={}",
        payload.delivery_policy.min_notify_interval_secs,
        payload.delivery_policy.quiet_hours_start_utc,
        payload.delivery_policy.quiet_hours_end_utc,
        payload.delivery_policy.in_quiet_hours,
    );
    println!(
        "heartbeat delivery metrics ticks={} notify_decisions={} dispatched={} suppressed_quiet={} suppressed_interval={} last_notify_decision={:?} last_dispatched={:?}",
        payload.delivery_metrics.tick_evaluations,
        payload.delivery_metrics.notify_decisions,
        payload.delivery_metrics.dispatched_notifications,
        payload.delivery_metrics.suppressed_quiet_hours,
        payload.delivery_metrics.suppressed_min_interval,
        payload.delivery_metrics.last_notify_decision_at_utc,
        payload.delivery_metrics.last_dispatched_at_utc,
    );
    println!(
        "heartbeat signals materialized={} processed_job={} published={} failed={} dead_letter={} outbox_pending={} last_tick={:?} now={}",
        payload.materialized_jobs,
        payload.processed_job,
        payload.published_events,
        payload.failed_jobs,
        payload.dead_letter_jobs,
        payload.pending_outbox_events,
        payload.last_tick_at_utc,
        payload.now_utc,
    );
    Ok(())
}

async fn run_daemon_first_run(daemon_url: &str, args: &[String]) -> Result<()> {
    println!("medousa first-run check daemon_url={daemon_url}");
    run_daemon_health(daemon_url).await?;
    run_daemon_heartbeat_status(daemon_url).await?;

    let report_query = find_arg_value(args, "--report-query")
        .unwrap_or("Summarize runtime posture with citations");
    println!("next step: open Medousa and chat, or trigger a report from the CLI");
    println!(
        "  medousa-cli daemon-report \"{}\" --daemon-url {} --poll-timeout-ms 30000",
        report_query,
        daemon_url
    );
    println!(
        "offline brain: open Medousa welcome wizard, or: medousa models probe"
    );
    println!(
        "safety posture interactive_profile={} scheduled_profile={}",
        default_policy_profile_for_lane(EngineExecutionLane::Interactive),
        default_policy_profile_for_lane(EngineExecutionLane::Scheduled),
    );
    Ok(())
}

async fn run_daemon_ask(daemon_url: &str, args: &[String]) -> Result<()> {
    let prompt = args
        .get(1)
        .ok_or_else(|| anyhow!("missing prompt: medousa-cli daemon-ask <prompt>"))?;
    let wait = !args.iter().any(|arg| arg == "--no-wait");
    let client = Client::new();
    let user_id = find_arg_value(args, "--identity-user-id")
        .map(ToString::to_string)
        .unwrap_or_else(|| "cli:user:local".to_string());
    let channel_id = find_arg_value(args, "--identity-channel-id")
        .map(ToString::to_string)
        .unwrap_or_else(|| "cli:channel:default".to_string());

    let request = IngestRequest {
        channel: "cli".to_string(),
        user_id,
        channel_id,
        text: prompt.to_string(),
        attachments: Vec::new(),
    };

    let response = client
        .post(format!("{daemon_url}/v1/ingest"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: IngestResponse = response.json().await?;
    if payload.stream_ready {
        let job_id = payload.job_id.clone().unwrap_or_else(|| "none".to_string());
        if !wait {
            println!(
                "ingester accepted session_id={} job_id={} new_session={} reply={}",
                payload.session_id, job_id, payload.is_new_session, payload.reply
            );
            return Ok(());
        }

        let delivery_outcome = wait_for_ask_delivery(
            &client,
            daemon_url,
            &payload,
            default_delivery_timeout(),
        )
        .await?;

        match delivery_outcome {
            AdapterDeliveryOutcome::StreamError { message } => {
                return Err(anyhow!(message));
            }
            AdapterDeliveryOutcome::PushDelivered | AdapterDeliveryOutcome::Fallback { .. } => {
                let result = fetch_job_result(&client, daemon_url, &job_id).await?;
                println!(
                    "ingester complete session_id={} job_id={} new_session={} status={}",
                    payload.session_id, job_id, payload.is_new_session, result.status
                );
                if let Some(text) = result.output_text.filter(|value| !value.trim().is_empty()) {
                    println!("{text}");
                } else if let AdapterDeliveryOutcome::Fallback { text } = delivery_outcome {
                    println!("{text}");
                } else {
                    println!("(empty response)");
                }
            }
        }
        return Ok(());
    }

    println!(
        "ingester reply session_id={} job_id={} new_session={} reply={}",
        payload.session_id,
        payload.job_id.as_deref().unwrap_or("none"),
        payload.is_new_session,
        payload.reply
    );
    Ok(())
}

async fn run_daemon_report(daemon_url: &str, args: &[String]) -> Result<()> {
    let query = args
        .get(1)
        .ok_or_else(|| anyhow!("missing query: medousa-cli daemon-report <query>"))?;
    let client = Client::new();

    let poll_timeout_ms = find_arg_value(args, "--poll-timeout-ms")
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|err| anyhow!("invalid --poll-timeout-ms value '{raw}': {err}"))
        })
        .transpose()?
        .unwrap_or(25_000);
    let poll_interval_ms = find_arg_value(args, "--poll-interval-ms")
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|err| anyhow!("invalid --poll-interval-ms value '{raw}': {err}"))
        })
        .transpose()?
        .unwrap_or(700)
        .max(100);

    let request = EnqueueReportRequest {
        query: query.to_string(),
        policy_profile: Some(
            find_arg_value(args, "--policy-profile")
                .unwrap_or(default_policy_profile_for_lane(EngineExecutionLane::Interactive))
                .to_string(),
        ),
        model_hint: find_arg_value(args, "--model-hint").map(ToString::to_string),
        max_turns: find_arg_value(args, "--max-turns")
            .and_then(|raw| raw.parse::<u32>().ok())
            .or(Some(2)),
        identity_user_id: find_arg_value(args, "--identity-user-id").map(ToString::to_string),
        identity_persona_id: find_arg_value(args, "--identity-persona-id")
            .map(ToString::to_string),
        identity_channel_id: find_arg_value(args, "--identity-channel-id")
            .map(ToString::to_string),
    };

    let response = client
        .post(format!("{daemon_url}/v1/jobs/report"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: EnqueueResponse = response.json().await?;

    println!(
        "daemon accepted report job_id={} queue={} at={}",
        payload.job_id, payload.queue, payload.accepted_at_utc
    );

    if poll_timeout_ms == 0 {
        return Ok(());
    }

    match wait_for_terminal_daemon_report(
        &client,
        daemon_url,
        &payload.job_id,
        poll_timeout_ms,
        poll_interval_ms,
    )
    .await?
    {
        Some(report) => print_daemon_report(&report),
        None => {
            println!(
                "report still running after timeout={}ms. fetch later: medousa-cli daemon-job-report {}",
                poll_timeout_ms, payload.job_id
            );
        }
    }

    Ok(())
}

async fn run_daemon_job_report(daemon_url: &str, job_id: &str) -> Result<()> {
    let client = Client::new();
    let report = query_daemon_job_report(&client, daemon_url, job_id).await?;
    print_daemon_report(&report);
    Ok(())
}

async fn wait_for_terminal_daemon_report(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
    poll_timeout_ms: u64,
    poll_interval_ms: u64,
) -> Result<Option<JobReportResponse>> {
    if poll_timeout_ms == 0 {
        return Ok(None);
    }

    let interval = std::time::Duration::from_millis(poll_interval_ms.max(100));
    let deadline = Instant::now() + std::time::Duration::from_millis(poll_timeout_ms);

    loop {
        let report = query_daemon_job_report(client, daemon_url, job_id).await?;
        if report.is_terminal {
            return Ok(Some(report));
        }

        if Instant::now() >= deadline {
            return Ok(None);
        }

        sleep(interval).await;
    }
}

async fn query_daemon_job_report(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
) -> Result<JobReportResponse> {
    let response = client
        .get(format!("{daemon_url}/v1/jobs/{job_id}/report"))
        .send()
        .await?
        .error_for_status()?;
    response.json::<JobReportResponse>().await.map_err(Into::into)
}

fn print_daemon_report(report: &JobReportResponse) {
    println!(
        "report status job_id={} status={} terminal={} attempts={} outcome={}",
        report.job_id,
        report.status,
        report.is_terminal,
        report.attempt_count,
        report.latest_outcome.as_deref().unwrap_or("unknown"),
    );

    if let Some(text) = report.output_text.as_deref() {
        println!("report output:\n{}", text);
    } else {
        println!("report output: none");
    }

    if report.citations.is_empty() {
        println!("report citations: none");
    } else {
        println!("report citations ({}):", report.citations.len());
        for (index, citation) in report.citations.iter().enumerate() {
            println!(
                "  [C{}] {}{}",
                index + 1,
                citation.source,
                citation
                    .title
                    .as_deref()
                    .map(|title| format!(" ({title})"))
                    .unwrap_or_default(),
            );
        }
    }

    if let Some(evidence) = report.evidence_report.as_ref() {
        println!(
            "report evidence verification_state={} confidence={:.2} citation_coverage={:.2} supported_claim_ratio={:.2} supported_claims={}/{}",
            evidence.verification_state,
            evidence.confidence_score,
            evidence.citation_coverage,
            evidence.supported_claim_ratio,
            evidence.supported_claims,
            evidence.total_claims,
        );
        println!(
            "report evidence ids session={} artifact={} extraction_id={:?} pack_id={} verification_id={:?}",
            evidence.session_id,
            evidence.artifact_id,
            evidence.extraction_id,
            evidence.pack_id,
            evidence.verification_id,
        );
    } else {
        println!("report evidence: unavailable");
    }
}

async fn run_daemon_watch_add(
    daemon_url: &str,
    cron_expr: &str,
    timezone: &str,
    prompt: &str,
) -> Result<()> {
    let client = Client::new();
    let request = RegisterRecurringPromptRequest {
        id: None,
        queue: Some("default".to_string()),
        prompt: prompt.to_string(),
        system_prompt: Some(
            medousa::agent_runtime::LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT.to_string(),
        ),
        cron_expr: cron_expr.to_string(),
        timezone: Some(timezone.to_string()),
        jitter_seconds: Some(0),
        enabled: Some(true),
        max_attempts: Some(1),
        policy_profile: Some(
            default_policy_profile_for_lane(EngineExecutionLane::Scheduled).to_string(),
        ),
        model_hint: None,
        delivery: None,
        session_id: None,
        execution_mode: None,
        manuscript_id: None,
    };

    let response = client
        .post(format!("{daemon_url}/v1/recurring/prompt"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: RegisterRecurringResponse = response.json().await?;
    println!(
        "daemon recurring registered id={} next_run={} cron='{}' tz={} queue={}",
        payload.recurring_id,
        payload.next_run_at_utc,
        payload.cron_expr,
        payload.timezone,
        payload.queue
    );
    Ok(())
}

async fn fetch_identity_context(
    daemon_url: &str,
    args: &[String],
) -> Result<GetIdentityContextResponse> {
    let client = Client::new();
    let request = IdentityContextRequest {
        user_id: find_arg_value(args, "--user-id").map(ToString::to_string),
        persona_id: find_arg_value(args, "--persona-id").map(ToString::to_string),
        channel_id: find_arg_value(args, "--channel-id").map(ToString::to_string),
        policy_profile: find_arg_value(args, "--policy-profile").map(ToString::to_string),
        relationship_limit: find_arg_value(args, "--relationship-limit")
            .and_then(|raw| raw.parse::<usize>().ok()),
        mode: find_arg_value(args, "--mode").map(ToString::to_string),
    };

    let response = client
        .post(format!("{daemon_url}/v1/identity/context"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    response
        .json::<GetIdentityContextResponse>()
        .await
        .map_err(Into::into)
}

async fn run_daemon_identity_context(daemon_url: &str, args: &[String]) -> Result<()> {
    let payload = fetch_identity_context(daemon_url, args).await?;
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

async fn run_daemon_identity_inspect(daemon_url: &str, args: &[String]) -> Result<()> {
    let payload = fetch_identity_context(daemon_url, args).await?;
    if has_flag(args, "--raw") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_identity_context_summary(&payload);
    }
    Ok(())
}

async fn run_daemon_identity_propose(daemon_url: &str, args: &[String]) -> Result<()> {
    let entity_type_raw = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing entity type: medousa-cli daemon-identity-propose <entity_type> <entity_id> <patch_json>"
        )
    })?;
    let entity_id = args.get(2).ok_or_else(|| {
        anyhow!(
            "missing entity id: medousa-cli daemon-identity-propose <entity_type> <entity_id> <patch_json>"
        )
    })?;
    let patch_raw = args.get(3).ok_or_else(|| {
        anyhow!(
            "missing patch_json: medousa-cli daemon-identity-propose <entity_type> <entity_id> <patch_json>"
        )
    })?;

    let patch: Value = serde_json::from_str(patch_raw)
        .map_err(|err| anyhow!("invalid patch_json, expected JSON object: {err}"))?;
    let entity_type = parse_identity_entity_type(entity_type_raw)?;
    let source = parse_update_source(find_arg_value(args, "--source"))?;
    let confidence = find_arg_value(args, "--confidence")
        .and_then(|raw| raw.parse::<f32>().ok())
        .unwrap_or(0.75)
        .clamp(0.0, 1.0);
    let reason = find_arg_value(args, "--reason")
        .unwrap_or("manual update proposal")
        .to_string();
    let actor = find_arg_value(args, "--actor")
        .unwrap_or("medousa-cli")
        .to_string();
    let receipt_id = find_arg_value(args, "--receipt-id").map(ToString::to_string);
    let expires_at = parse_optional_utc(find_arg_value(args, "--expires-at"))?;

    let request = ProposeEntityUpdateRequest {
        entity_type,
        entity_id: entity_id.to_string(),
        patch,
        source,
        confidence,
        reason,
        actor,
        receipt_id,
        expires_at,
    };

    let client = Client::new();
    let response = client
        .post(format!("{daemon_url}/v1/identity/update/propose"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: ProposeEntityUpdateResponse = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

async fn run_daemon_identity_update(daemon_url: &str, args: &[String]) -> Result<()> {
    let entity_type_raw = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing entity type: medousa-cli daemon-identity-update <entity_type> <entity_id> <patch_json>"
        )
    })?;
    let entity_id = args.get(2).ok_or_else(|| {
        anyhow!(
            "missing entity id: medousa-cli daemon-identity-update <entity_type> <entity_id> <patch_json>"
        )
    })?;
    let patch_raw = args.get(3).ok_or_else(|| {
        anyhow!(
            "missing patch_json: medousa-cli daemon-identity-update <entity_type> <entity_id> <patch_json>"
        )
    })?;

    let patch: Value = serde_json::from_str(patch_raw)
        .map_err(|err| anyhow!("invalid patch_json, expected JSON object: {err}"))?;
    let entity_type = parse_identity_entity_type(entity_type_raw)?;
    let source = parse_update_source(find_arg_value(args, "--source"))?;
    let confidence = find_arg_value(args, "--confidence")
        .and_then(|raw| raw.parse::<f32>().ok())
        .unwrap_or(0.75)
        .clamp(0.0, 1.0);
    let reason = find_arg_value(args, "--reason")
        .unwrap_or("manual update proposal")
        .to_string();
    let actor = find_arg_value(args, "--actor")
        .unwrap_or("medousa-cli")
        .to_string();
    let receipt_id = find_arg_value(args, "--receipt-id").map(ToString::to_string);
    let expires_at = parse_optional_utc(find_arg_value(args, "--expires-at"))?;

    let request = ProposeEntityUpdateRequest {
        entity_type: entity_type.clone(),
        entity_id: entity_id.to_string(),
        patch,
        source,
        confidence,
        reason,
        actor,
        receipt_id,
        expires_at,
    };

    let client = Client::new();
    let response = client
        .post(format!("{daemon_url}/v1/identity/update/propose"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: ProposeEntityUpdateResponse = response.json().await?;

    if has_flag(args, "--raw") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_identity_proposal_summary(
            identity_entity_type_token(&entity_type),
            entity_id,
            &payload,
            daemon_url,
        );
    }

    if has_flag(args, "--auto-commit") {
        if payload.requires_approval {
            return Err(anyhow!(
                "--auto-commit blocked: proposal requires approval by policy"
            ));
        }
        if payload.proposal_ids.len() != 1 {
            return Err(anyhow!(
                "--auto-commit requires exactly one proposal id, got {}",
                payload.proposal_ids.len()
            ));
        }

        let expected_version = find_arg_value(args, "--expected-version")
            .ok_or_else(|| anyhow!("--auto-commit requires --expected-version <n>"))?
            .parse::<i32>()
            .map_err(|err| anyhow!("invalid --expected-version value: {err}"))?;

        let commit_request = CommitEntityUpdateRequest {
            proposal_id: payload
                .proposal_ids
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("missing proposal id for --auto-commit"))?,
            expected_version,
            approver: find_arg_value(args, "--approver").map(ToString::to_string),
        };

        let commit_response = client
            .post(format!("{daemon_url}/v1/identity/update/commit"))
            .json(&commit_request)
            .send()
            .await?
            .error_for_status()?;
        let commit_payload: CommitEntityUpdateResponse = commit_response.json().await?;

        if has_flag(args, "--raw") {
            println!("{}", serde_json::to_string_pretty(&commit_payload)?);
        } else {
            print_identity_commit_summary(&commit_payload);
            println!(
                "post-commit review: cargo run -p medousa --bin medousa_cli -- daemon-identity-review {} {} --daemon-url {}",
                identity_entity_type_token(&entity_type),
                entity_id,
                daemon_url,
            );
        }
    }

    Ok(())
}

async fn run_daemon_identity_commit(daemon_url: &str, args: &[String]) -> Result<()> {
    let proposal_id = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing proposal_id: medousa-cli daemon-identity-commit <proposal_id> <expected_version>"
        )
    })?;
    let expected_version = args
        .get(2)
        .ok_or_else(|| {
            anyhow!(
                "missing expected_version: medousa-cli daemon-identity-commit <proposal_id> <expected_version>"
            )
        })?
        .parse::<i32>()
        .map_err(|err| anyhow!("expected_version must be integer: {err}"))?;

    let request = CommitEntityUpdateRequest {
        proposal_id: proposal_id.to_string(),
        expected_version,
        approver: find_arg_value(args, "--approver").map(ToString::to_string),
    };

    let client = Client::new();
    let response = client
        .post(format!("{daemon_url}/v1/identity/update/commit"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: CommitEntityUpdateResponse = response.json().await?;
    if has_flag(args, "--raw") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_identity_commit_summary(&payload);
    }
    Ok(())
}

async fn fetch_identity_history(
    daemon_url: &str,
    entity_type: IdentityEntityType,
    entity_id: &str,
    limit: usize,
) -> Result<ListEntityHistoryResponse> {
    let request = ListEntityHistoryRequest {
        entity_type,
        entity_id: entity_id.to_string(),
        limit,
    };

    let client = Client::new();
    let response = client
        .post(format!("{daemon_url}/v1/identity/history"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    response
        .json::<ListEntityHistoryResponse>()
        .await
        .map_err(Into::into)
}

async fn run_daemon_identity_history(daemon_url: &str, args: &[String]) -> Result<()> {
    let entity_type_raw = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing entity type: medousa-cli daemon-identity-history <entity_type> <entity_id> [--limit <n>]"
        )
    })?;
    let entity_id = args.get(2).ok_or_else(|| {
        anyhow!(
            "missing entity id: medousa-cli daemon-identity-history <entity_type> <entity_id> [--limit <n>]"
        )
    })?;

    let entity_type = parse_identity_entity_type(entity_type_raw)?;
    let limit = find_arg_value(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(20);

    let payload = fetch_identity_history(daemon_url, entity_type.clone(), entity_id, limit).await?;
    if has_flag(args, "--raw") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_identity_history_review(
            identity_entity_type_token(&entity_type),
            entity_id,
            &payload,
            limit,
            daemon_url,
            false,
        );
    }
    Ok(())
}

async fn run_daemon_identity_review(daemon_url: &str, args: &[String]) -> Result<()> {
    let entity_type_raw = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing entity type: medousa-cli daemon-identity-review <entity_type> <entity_id> [--limit <n>]"
        )
    })?;
    let entity_id = args.get(2).ok_or_else(|| {
        anyhow!(
            "missing entity id: medousa-cli daemon-identity-review <entity_type> <entity_id> [--limit <n>]"
        )
    })?;

    let entity_type = parse_identity_entity_type(entity_type_raw)?;
    let limit = find_arg_value(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(20);

    let payload = fetch_identity_history(daemon_url, entity_type.clone(), entity_id, limit).await?;
    if has_flag(args, "--raw") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_identity_history_review(
            identity_entity_type_token(&entity_type),
            entity_id,
            &payload,
            limit,
            daemon_url,
            true,
        );
    }
    Ok(())
}

async fn run_daemon_identity_explain(daemon_url: &str, args: &[String]) -> Result<()> {
    let entity_type_raw = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing entity type: medousa-cli daemon-identity-explain <entity_type> <entity_id> [--limit <n>]"
        )
    })?;
    let entity_id = args.get(2).ok_or_else(|| {
        anyhow!(
            "missing entity id: medousa-cli daemon-identity-explain <entity_type> <entity_id> [--limit <n>]"
        )
    })?;

    let entity_type = parse_identity_entity_type(entity_type_raw)?;
    let limit = find_arg_value(args, "--limit")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(20);

    let payload = fetch_identity_history(daemon_url, entity_type.clone(), entity_id, limit).await?;
    print_identity_history_explain(
        identity_entity_type_token(&entity_type),
        entity_id,
        &payload,
        daemon_url,
    );
    Ok(())
}

async fn run_daemon_identity_rollback(daemon_url: &str, args: &[String]) -> Result<()> {
    let entity_type_raw = args.get(1).ok_or_else(|| {
        anyhow!(
            "missing entity type: medousa-cli daemon-identity-rollback <entity_type> <entity_id> <target_version> [--reason <text>] [--approver <actor>]"
        )
    })?;
    let entity_id = args.get(2).ok_or_else(|| {
        anyhow!(
            "missing entity id: medousa-cli daemon-identity-rollback <entity_type> <entity_id> <target_version> [--reason <text>] [--approver <actor>]"
        )
    })?;
    let target_version = args
        .get(3)
        .ok_or_else(|| {
            anyhow!(
                "missing target_version: medousa-cli daemon-identity-rollback <entity_type> <entity_id> <target_version> [--reason <text>] [--approver <actor>]"
            )
        })?
        .parse::<i32>()
        .map_err(|err| anyhow!("target_version must be integer: {err}"))?;

    let request = RollbackEntityVersionRequest {
        entity_type: parse_identity_entity_type(entity_type_raw)?,
        entity_id: entity_id.to_string(),
        target_version,
        reason: find_arg_value(args, "--reason")
            .unwrap_or("manual rollback via medousa-cli")
            .to_string(),
        approver: find_arg_value(args, "--approver")
            .unwrap_or("medousa-cli")
            .to_string(),
    };

    let client = Client::new();
    let response = client
        .post(format!("{daemon_url}/v1/identity/rollback"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: RollbackEntityVersionResponse = response.json().await?;
    if has_flag(args, "--raw") {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_identity_rollback_summary(&payload);
    }
    Ok(())
}

fn parse_identity_entity_type(raw: &str) -> Result<IdentityEntityType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "persona" | "persona_entity" | "personaentity" => Ok(IdentityEntityType::PersonaEntity),
        "user" | "user_entity" | "userentity" => Ok(IdentityEntityType::UserEntity),
        "contact" | "contact_entity" | "contactentity" => Ok(IdentityEntityType::ContactEntity),
        "channel" | "channel_profile" | "channel_profile_entity" | "channelprofileentity" => {
            Ok(IdentityEntityType::ChannelProfileEntity)
        }
        "policy" | "policy_profile" | "policy_profile_entity" | "policyprofileentity" => {
            Ok(IdentityEntityType::PolicyProfileEntity)
        }
        "relationship" | "relationship_entity" | "relationshipentity" => {
            Ok(IdentityEntityType::RelationshipEntity)
        }
        other => Err(anyhow!("unsupported identity entity type: {other}")),
    }
}

fn parse_update_source(raw: Option<&str>) -> Result<UpdateSource> {
    match raw.unwrap_or("model_inferred").trim().to_ascii_lowercase().as_str() {
        "user_direct" | "user" => Ok(UpdateSource::UserDirect),
        "model_inferred" | "model" => Ok(UpdateSource::ModelInferred),
        "system_event" | "system" => Ok(UpdateSource::SystemEvent),
        other => Err(anyhow!(
            "unsupported update source '{other}', expected user_direct|model_inferred|system_event"
        )),
    }
}

fn parse_optional_utc(raw: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|err| anyhow!("invalid --expires-at timestamp, expected RFC3339: {err}"))?;
    Ok(Some(parsed.with_timezone(&Utc)))
}

fn identity_entity_type_token(entity_type: &IdentityEntityType) -> &'static str {
    match entity_type {
        IdentityEntityType::PersonaEntity => "persona",
        IdentityEntityType::UserEntity => "user",
        IdentityEntityType::ContactEntity => "contact",
        IdentityEntityType::ChannelProfileEntity => "channel",
        IdentityEntityType::PolicyProfileEntity => "policy",
        IdentityEntityType::RelationshipEntity => "relationship",
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn single_line_summary(text: &str, max_chars: usize) -> String {
    let collapsed = text
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }

    let truncated = collapsed.chars().take(max_chars).collect::<String>();
    format!("{truncated}...")
}

fn increment_count(counts: &mut BTreeMap<String, usize>, key: String) {
    *counts.entry(key).or_insert(0) += 1;
}

fn format_count_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }

    counts
        .iter()
        .map(|(key, count)| format!("{key}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn proposal_patch_keys(patch: &Value) -> String {
    match patch {
        Value::Object(map) => {
            if map.is_empty() {
                return "none".to_string();
            }

            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort_unstable();
            keys.join(",")
        }
        _ => "non_object_patch".to_string(),
    }
}

fn print_identity_context_summary(payload: &GetIdentityContextResponse) {
    println!(
        "identity inspect graph_depth={} relationships={} policy_profiles={} flattened_claims={}",
        payload.graph_depth_used,
        payload.relationships.len(),
        payload.policy_profiles.len(),
        payload.flattened_claims.len(),
    );

    if let Some(persona) = payload.persona.as_ref() {
        println!(
            "persona id={} status={} version={} updated_at={}",
            persona.persona_id,
            persona.status,
            persona.version,
            persona.updated_at,
        );
    } else {
        println!("persona missing");
    }

    if let Some(user) = payload.user.as_ref() {
        println!(
            "user id={} timezone={} status={} version={} updated_at={}",
            user.user_id,
            user.timezone,
            user.status,
            user.version,
            user.updated_at,
        );
    } else {
        println!("user missing");
    }

    if let Some(channel) = payload.channel.as_ref() {
        println!(
            "channel id={} type={} proactive_allowed={} status={} version={} updated_at={}",
            channel.channel_id,
            channel.channel_type,
            channel.proactive_allowed,
            channel.status,
            channel.version,
            channel.updated_at,
        );
    } else {
        println!("channel missing");
    }

    if payload.policy_profiles.is_empty() {
        println!("policy profiles: none");
    } else {
        println!("policy profiles:");
        for profile in &payload.policy_profiles {
            println!(
                "  - id={} depth={} trust_delta_max_per_window={:.2} status={} version={} updated_at={}",
                profile.policy_profile_id,
                profile.graph_max_depth,
                profile.trust_delta_max_per_window,
                profile.status,
                profile.version,
                profile.updated_at,
            );
        }
    }

    if payload.relationships.is_empty() {
        println!("relationships: none");
    } else {
        let mut relationship_status_counts = BTreeMap::new();
        let mut continuity_links = 0usize;
        let mut continuity_receipts = 0usize;
        for relationship in &payload.relationships {
            increment_count(
                &mut relationship_status_counts,
                format!("{:?}", &relationship.status).to_ascii_lowercase(),
            );
            if relationship.derived_from_relationship_id.is_some() {
                continuity_links += 1;
            }
            if relationship.transition_receipt_id.is_some() {
                continuity_receipts += 1;
            }
        }

        println!(
            "relationships summary statuses={} continuity_links={} continuity_receipts={}",
            format_count_map(&relationship_status_counts),
            continuity_links,
            continuity_receipts,
        );

        for relationship in payload.relationships.iter().take(8) {
            println!(
                "  - id={} kind={} status={:?} trust={:.2} confidence={:.2} source={}:{} target={}:{}",
                relationship.relationship_id,
                relationship.relationship_kind.as_str(),
                &relationship.status,
                relationship.trust_level,
                relationship.confidence,
                relationship.source_entity_ref.entity_type,
                relationship.source_entity_ref.entity_id,
                relationship.target_entity_ref.entity_type,
                relationship.target_entity_ref.entity_id,
            );
        }
    }

    if payload.flattened_claims.is_empty() {
        println!("flattened claims: none");
    } else {
        println!("flattened claims:");
        for claim in payload.flattened_claims.iter().take(8) {
            println!(
                "  - claim_id={} confidence={:.2} sources={} summary={}",
                claim.claim_id,
                claim.confidence,
                claim.source_relationship_ids.len(),
                single_line_summary(&claim.summary, 120),
            );
        }
    }
}

fn print_identity_proposal_summary(
    entity_type_token: &str,
    entity_id: &str,
    payload: &ProposeEntityUpdateResponse,
    daemon_url: &str,
) {
    println!(
        "identity update proposed entity_type={} entity_id={} proposal_count={} requires_approval={} split_patch={}",
        entity_type_token,
        entity_id,
        payload.proposal_ids.len(),
        payload.requires_approval,
        payload.split_patch,
    );

    let mut tier_counts = BTreeMap::new();
    for tier in &payload.tiers {
        increment_count(&mut tier_counts, format!("{:?}", tier).to_ascii_lowercase());
    }
    println!("proposal tiers={}", format_count_map(&tier_counts));

    if payload.proposal_ids.is_empty() {
        println!("proposals: none");
    } else {
        println!("proposals:");
        for (index, proposal_id) in payload.proposal_ids.iter().enumerate() {
            let tier = payload
                .tiers
                .get(index)
                .map(|value| format!("{:?}", value).to_ascii_lowercase())
                .unwrap_or_else(|| "unknown".to_string());
            println!("  - proposal_id={} tier={}", proposal_id, tier);
        }
    }

    if payload.policy_notes.is_empty() {
        println!("policy notes: none");
    } else {
        println!("policy notes:");
        for note in &payload.policy_notes {
            println!("  - {}", note);
        }
    }

    println!(
        "review: cargo run -p medousa --bin medousa_cli -- daemon-identity-review {} {} --daemon-url {}",
        entity_type_token,
        entity_id,
        daemon_url,
    );
    if let Some(first_proposal_id) = payload.proposal_ids.first() {
        println!(
            "commit: cargo run -p medousa --bin medousa_cli -- daemon-identity-commit {} <expected_version> --daemon-url {}",
            first_proposal_id,
            daemon_url,
        );
    }
    println!(
        "rollback template: cargo run -p medousa --bin medousa_cli -- daemon-identity-rollback {} {} <target_version> --reason \"manual continuity rollback\" --approver medousa-cli --daemon-url {}",
        entity_type_token,
        entity_id,
        daemon_url,
    );
}

fn print_identity_commit_summary(payload: &CommitEntityUpdateResponse) {
    println!(
        "identity commit committed={} code={:?} entity_type={:?} entity_id={} new_version={:?}",
        payload.committed,
        payload.code.as_ref(),
        payload.entity_type.as_ref(),
        payload.entity_id.as_deref().unwrap_or("unknown"),
        payload.new_version,
    );
    println!(
        "identity commit receipt_id={:?} transition_event_id={:?}",
        payload.receipt_id,
        payload.transition_event_id,
    );
    if let Some(rationale) = payload.rationale.as_deref() {
        println!("identity commit rationale={}", single_line_summary(rationale, 180));
    }
    if let Some(sttp_node) = payload.sttp_bridge_node.as_deref() {
        println!("identity commit sttp_bridge_node={}", sttp_node);
    }
    if let Some(sttp_reason) = payload.sttp_bridge_reason.as_deref() {
        println!(
            "identity commit sttp_bridge_reason={}",
            single_line_summary(sttp_reason, 180)
        );
    }
}

fn print_identity_history_review(
    entity_type_token: &str,
    entity_id: &str,
    payload: &ListEntityHistoryResponse,
    limit: usize,
    daemon_url: &str,
    include_guidance: bool,
) {
    println!(
        "identity history entity_type={} entity_id={} proposals={} transitions={} limit={}",
        entity_type_token,
        entity_id,
        payload.proposals.len(),
        payload.transitions.len(),
        limit,
    );

    let mut state_counts = BTreeMap::new();
    let mut tier_counts = BTreeMap::new();
    let mut source_counts = BTreeMap::new();
    for proposal in &payload.proposals {
        increment_count(
            &mut state_counts,
            format!("{:?}", proposal.state).to_ascii_lowercase(),
        );
        increment_count(
            &mut tier_counts,
            format!("{:?}", proposal.tier).to_ascii_lowercase(),
        );
        increment_count(
            &mut source_counts,
            format!("{:?}", proposal.source).to_ascii_lowercase(),
        );
    }

    println!("proposal states={}", format_count_map(&state_counts));
    println!("proposal tiers={}", format_count_map(&tier_counts));
    println!("proposal sources={}", format_count_map(&source_counts));

    let mut proposals = payload.proposals.clone();
    proposals.sort_by_key(|proposal| Reverse(proposal.updated_at));
    if proposals.is_empty() {
        println!("recent proposals: none");
    } else {
        println!("recent proposals:");
        for proposal in proposals.iter().take(6) {
            println!(
                "  - id={} state={:?} tier={:?} source={:?} confidence={:.2} actor={} updated_at={} patch_keys={} reason={}",
                proposal.proposal_id,
                proposal.state,
                proposal.tier,
                proposal.source,
                proposal.confidence,
                proposal.actor,
                proposal.updated_at,
                proposal_patch_keys(&proposal.patch),
                single_line_summary(&proposal.reason, 120),
            );
        }
    }

    let mut transitions = payload.transitions.clone();
    transitions.sort_by_key(|transition| Reverse(transition.occurred_at));
    if transitions.is_empty() {
        println!("recent transitions: none");
    } else {
        println!("recent transitions:");
        for transition in transitions.iter().take(6) {
            let from_status = transition
                .from_status
                .as_ref()
                .map(|value| format!("{:?}", value).to_ascii_lowercase())
                .unwrap_or_else(|| "none".to_string());
            println!(
                "  - event_id={} relationship_id={} from={} to={:?} actor={} occurred_at={} reason={}",
                transition.event_id,
                transition.relationship_id,
                from_status,
                &transition.to_status,
                transition.actor,
                transition.occurred_at,
                single_line_summary(&transition.reason, 120),
            );
        }
    }

    if include_guidance {
        println!(
            "explain: cargo run -p medousa --bin medousa_cli -- daemon-identity-explain {} {} --daemon-url {}",
            entity_type_token,
            entity_id,
            daemon_url,
        );
        println!(
            "rollback template: cargo run -p medousa --bin medousa_cli -- daemon-identity-rollback {} {} <target_version> --reason \"manual continuity rollback\" --approver medousa-cli --daemon-url {}",
            entity_type_token,
            entity_id,
            daemon_url,
        );
    }
}

fn print_identity_history_explain(
    entity_type_token: &str,
    entity_id: &str,
    payload: &ListEntityHistoryResponse,
    daemon_url: &str,
) {
    println!(
        "identity explain entity_type={} entity_id={}",
        entity_type_token,
        entity_id,
    );

    if payload.proposals.is_empty() && payload.transitions.is_empty() {
        println!("no identity change records found for this entity");
        println!(
            "propose first change: cargo run -p medousa --bin medousa_cli -- daemon-identity-update {} {} '{{\"status\":\"active\"}}' --reason \"initial continuity baseline\" --daemon-url {}",
            entity_type_token,
            entity_id,
            daemon_url,
        );
        return;
    }

    let mut state_counts = BTreeMap::new();
    for proposal in &payload.proposals {
        increment_count(
            &mut state_counts,
            format!("{:?}", proposal.state).to_ascii_lowercase(),
        );
    }
    println!(
        "proposal activity total={} states={}",
        payload.proposals.len(),
        format_count_map(&state_counts),
    );

    let mut proposals = payload.proposals.clone();
    proposals.sort_by_key(|proposal| Reverse(proposal.updated_at));
    if let Some(latest) = proposals.first() {
        println!(
            "latest proposal id={} state={:?} tier={:?} source={:?} actor={} confidence={:.2} updated_at={}",
            latest.proposal_id,
            latest.state,
            latest.tier,
            latest.source,
            latest.actor,
            latest.confidence,
            latest.updated_at,
        );
        println!(
            "latest proposal patch_keys={} reason={}",
            proposal_patch_keys(&latest.patch),
            single_line_summary(&latest.reason, 180),
        );
    }

    let mut transitions = payload.transitions.clone();
    transitions.sort_by_key(|transition| Reverse(transition.occurred_at));
    if let Some(latest) = transitions.first() {
        let from_status = latest
            .from_status
            .as_ref()
            .map(|value| format!("{:?}", value).to_ascii_lowercase())
            .unwrap_or_else(|| "none".to_string());
        println!(
            "latest transition event_id={} relationship_id={} from={} to={:?} actor={} occurred_at={} reason={}",
            latest.event_id,
            latest.relationship_id,
            from_status,
            &latest.to_status,
            latest.actor,
            latest.occurred_at,
            single_line_summary(&latest.reason, 180),
        );
    }

    println!(
        "audit trail: cargo run -p medousa --bin medousa_cli -- daemon-identity-review {} {} --daemon-url {}",
        entity_type_token,
        entity_id,
        daemon_url,
    );
    println!(
        "reversible path: cargo run -p medousa --bin medousa_cli -- daemon-identity-rollback {} {} <target_version> --reason \"manual continuity rollback\" --approver medousa-cli --daemon-url {}",
        entity_type_token,
        entity_id,
        daemon_url,
    );
}

fn print_identity_rollback_summary(payload: &RollbackEntityVersionResponse) {
    println!(
        "identity rollback rolled_back={} new_version={:?} rollback_receipt_id={:?}",
        payload.rolled_back,
        payload.new_version,
        payload.rollback_receipt_id,
    );
    if let Some(rationale) = payload.rationale.as_deref() {
        println!("identity rollback rationale={}", single_line_summary(rationale, 180));
    }
}

async fn run_llm(
    runtime: &RuntimeComposition,
    prompt: &str,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    let job_id = format!("medousa-llm-{}", now.timestamp_millis());
    let identity_user_id = resolve_identity_user_id(None);
    let payload = PromptJobPayload {
        user_prompt: compile_lane_prompt(EngineExecutionLane::Interactive, prompt),
        system_prompt: Some(
            medousa::agent_runtime::LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT.to_string(),
        ),
        policy_profile: Some(
            default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string(),
        ),
        model_hint: model.map(|v| v.to_string()),
        memory_policy: None,
    };

    let new_job = RuntimeWorkflowJobBuilder::for_prompt(job_id.clone(), &payload)?
        .with_correlation_id(identity_user_id)
        .with_causation_id("medousa-cli:interactive")
        .with_sttp_input_node_id("sttp:in:medousa:cli:interactive:llm")
        .with_scheduled_at(now)
        .build();

    match runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(new_job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(new_job).await?,
    }

    process_once(runtime, "medousa-cli").await?;

    let attempts = match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(&job_id).await?,
        RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(&job_id).await?,
    };
    let diagnostics_raw = attempts
        .last()
        .and_then(|attempt| attempt.diagnostics.as_deref())
        .ok_or_else(|| anyhow!("missing prompt diagnostics for {job_id}"))?;
    let diagnostics: serde_json::Value = serde_json::from_str(diagnostics_raw)?;
    let completion = diagnostics
        .get("output_text")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("missing output_text in prompt diagnostics"))?;

    let resolved_provider = resolve_llm_provider(provider);
    let target = resolve_llm_target(provider, model);
    let resolved_base_url = resolve_llm_base_url(provider, base_url);
    println!("registered_provider={}", resolved_provider);
    println!("registered_model={}", target);
    if let Some(base_url) = resolved_base_url {
        println!("registered_base_url={}", base_url);
    }
    println!("completion:\n{}", completion);
    Ok(())
}

async fn run_ask(runtime: &RuntimeComposition, prompt: &str) -> Result<()> {
    let now = Utc::now();
    let job_id = format!("medousa-ask-{}", now.timestamp_millis());
    let identity_user_id = resolve_identity_user_id(None);
    let payload = AgentSessionJobPayload {
        thread_id: Some(job_id.clone()),
        initial_user_prompt: compile_lane_prompt(EngineExecutionLane::Interactive, prompt),
        participants: vec![AgentSessionParticipantPayload {
            agent_id: "medousa.researcher".to_string(),
            system_prompt: Some(
                medousa::agent_runtime::LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT.to_string(),
            ),
            tool_name: "stasis.web.search.mock".to_string(),
            tool_input: Some(json!({ "query": prompt })),
        }],
        policy_profile: Some(
            default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string(),
        ),
        model_hint: None,
        memory_policy: None,
        max_turns: Some(1),
        tool_call_mode: Some(AgentToolCallMode::Auto),
    };

    let new_job = RuntimeWorkflowJobBuilder::for_agent_session(job_id.clone(), &payload)?
        .with_correlation_id(identity_user_id)
        .with_causation_id("medousa-cli:interactive")
        .with_sttp_input_node_id("sttp:in:medousa:cli:interactive:ask")
        .with_scheduled_at(now)
        .build();

    match runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(new_job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(new_job).await?,
    }

    let processed = process_once(runtime, "medousa-cli").await?;
    let published = publish_pending(runtime, 50).await?;

    println!("Medousa run submitted");
    println!("job_id={}", job_id);
    println!("processed={:?}", processed);
    println!("published_events={}", published);
    println!("next: medousa-daemon can be used for continuous orchestration loops");

    Ok(())
}

fn find_arg_value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let idx = args.iter().position(|arg| arg == key)?;
    args.get(idx + 1).map(|s| s.as_str())
}

fn compile_lane_prompt(lane: EngineExecutionLane, prompt: &str) -> String {
    compile_default_lane_prompt(lane, prompt)
}

fn print_usage() {
    println!("medousa-cli usage:");
    println!(
        "  medousa-cli ask <prompt> [--backend in-memory|surreal-mem|surreal-kv[:path]] [--provider <provider>] [--model <model_name>] [--base-url <url>]"
    );
    println!(
        "  medousa-cli llm <prompt> [--provider <provider>] [--model <model_name>] [--base-url <url>] [--backend in-memory|surreal-mem|surreal-kv[:path]]"
    );
    println!("  medousa-cli daemon-health [--daemon-url <url>]");
    println!("  medousa-cli daemon-stats [--daemon-url <url>]");
    println!("  medousa-cli daemon-heartbeat-status [--daemon-url <url>]");
    println!(
        "  medousa-cli daemon-first-run [--daemon-url <url>] [--report-query <query>]"
    );
    println!(
        "  medousa-cli daemon-ask <prompt> [--no-wait] [--identity-user-id <id>] [--identity-channel-id <id>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-report <query> [--policy-profile <profile>] [--model-hint <model>] [--max-turns <n>] [--poll-timeout-ms <n>] [--poll-interval-ms <n>] [--identity-user-id <id>] [--identity-persona-id <id>] [--identity-channel-id <id>] [--daemon-url <url>]"
    );
    println!("  medousa-cli daemon-job-report <job_id> [--daemon-url <url>]");
    println!(
        "  medousa-cli daemon-watch-add <cron_expr> <prompt> [--tz <timezone>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-context [--user-id <id>] [--persona-id <id>] [--channel-id <id>] [--policy-profile <profile>] [--relationship-limit <n>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-inspect [--user-id <id>] [--persona-id <id>] [--channel-id <id>] [--policy-profile <profile>] [--relationship-limit <n>] [--raw] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-propose <entity_type> <entity_id> <patch_json> [--source user_direct|model_inferred|system_event] [--confidence <0..1>] [--reason <text>] [--actor <id>] [--expires-at <RFC3339>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-update <entity_type> <entity_id> <patch_json> [--source user_direct|model_inferred|system_event] [--confidence <0..1>] [--reason <text>] [--actor <id>] [--receipt-id <id>] [--expires-at <RFC3339>] [--auto-commit] [--expected-version <n>] [--approver <id>] [--raw] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-commit <proposal_id> <expected_version> [--approver <id>] [--raw] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-history <entity_type> <entity_id> [--limit <n>] [--raw] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-review <entity_type> <entity_id> [--limit <n>] [--raw] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-explain <entity_type> <entity_id> [--limit <n>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-rollback <entity_type> <entity_id> <target_version> [--reason <text>] [--approver <id>] [--raw] [--daemon-url <url>]"
    );
    println!("  identity workflow: daemon-identity-inspect -> daemon-identity-update -> daemon-identity-review -> daemon-identity-rollback (if needed)");
    println!(
        "  recommended first run: daemon-first-run -> daemon-report (citation-first)"
    );
    println!("  note: ask uses workflow.stasis.agent_session through Stasis runtime orchestration");
}
