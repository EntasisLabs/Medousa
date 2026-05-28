use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use medousa::identity_memory::resolve_identity_user_id;
use medousa::engine_context::{
    EngineExecutionLane, compile_default_lane_prompt,
    default_policy_profile_for_lane,
};
use medousa::{
    DaemonStatsResponse, EnqueueAskRequest, EnqueueReportRequest, EnqueueResponse,
    HealthResponse,
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
    CommitEntityUpdateRequest, GetIdentityContextResponse, IdentityEntityType,
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
        "daemon-identity-propose" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_propose(&daemon_url, &args).await
        }
        "daemon-identity-commit" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_commit(&daemon_url, &args).await
        }
        "daemon-identity-history" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            run_daemon_identity_history(&daemon_url, &args).await
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

async fn run_daemon_ask(daemon_url: &str, args: &[String]) -> Result<()> {
    let prompt = args
        .get(1)
        .ok_or_else(|| anyhow!("missing prompt: medousa-cli daemon-ask <prompt>"))?;
    let client = Client::new();
    let request = EnqueueAskRequest {
        prompt: prompt.to_string(),
        policy_profile: Some(
            find_arg_value(args, "--policy-profile")
                .unwrap_or(default_policy_profile_for_lane(EngineExecutionLane::Interactive))
                .to_string(),
        ),
        model_hint: find_arg_value(args, "--model-hint").map(ToString::to_string),
        max_turns: find_arg_value(args, "--max-turns")
            .and_then(|raw| raw.parse::<u32>().ok())
            .or(Some(1)),
        identity_user_id: find_arg_value(args, "--identity-user-id").map(ToString::to_string),
        identity_persona_id: find_arg_value(args, "--identity-persona-id")
            .map(ToString::to_string),
        identity_channel_id: find_arg_value(args, "--identity-channel-id")
            .map(ToString::to_string),
    };

    let response = client
        .post(format!("{daemon_url}/v1/jobs/ask"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: EnqueueResponse = response.json().await?;
    println!(
        "daemon accepted ask job_id={} queue={} at={}",
        payload.job_id, payload.queue, payload.accepted_at_utc
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
            "You are Medousa, a practical research assistant. Be concise and evidence-driven."
                .to_string(),
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

async fn run_daemon_identity_context(daemon_url: &str, args: &[String]) -> Result<()> {
    let client = Client::new();
    let request = IdentityContextRequest {
        user_id: find_arg_value(args, "--user-id").map(ToString::to_string),
        persona_id: find_arg_value(args, "--persona-id").map(ToString::to_string),
        channel_id: find_arg_value(args, "--channel-id").map(ToString::to_string),
        policy_profile: find_arg_value(args, "--policy-profile").map(ToString::to_string),
        relationship_limit: find_arg_value(args, "--relationship-limit")
            .and_then(|raw| raw.parse::<usize>().ok()),
    };

    let response = client
        .post(format!("{daemon_url}/v1/identity/context"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: GetIdentityContextResponse = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&payload)?);
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
    let payload: serde_json::Value = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
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

    let request = ListEntityHistoryRequest {
        entity_type: parse_identity_entity_type(entity_type_raw)?,
        entity_id: entity_id.to_string(),
        limit: find_arg_value(args, "--limit")
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(20),
    };

    let client = Client::new();
    let response = client
        .post(format!("{daemon_url}/v1/identity/history"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let payload: ListEntityHistoryResponse = response.json().await?;
    println!("{}", serde_json::to_string_pretty(&payload)?);
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
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn parse_identity_entity_type(raw: &str) -> Result<IdentityEntityType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "persona" | "persona_entity" | "personaentity" => Ok(IdentityEntityType::PersonaEntity),
        "user" | "user_entity" | "userentity" => Ok(IdentityEntityType::UserEntity),
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
            "You are Medousa, a practical research assistant. Be concise and structured."
                .to_string(),
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
                "You are Medousa, a practical research assistant. Use tool evidence and cite findings succinctly.".to_string(),
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
        "  medousa-cli daemon-ask <prompt> [--policy-profile <profile>] [--model-hint <model>] [--max-turns <n>] [--identity-user-id <id>] [--identity-persona-id <id>] [--identity-channel-id <id>] [--daemon-url <url>]"
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
        "  medousa-cli daemon-identity-propose <entity_type> <entity_id> <patch_json> [--source user_direct|model_inferred|system_event] [--confidence <0..1>] [--reason <text>] [--actor <id>] [--expires-at <RFC3339>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-commit <proposal_id> <expected_version> [--approver <id>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-history <entity_type> <entity_id> [--limit <n>] [--daemon-url <url>]"
    );
    println!(
        "  medousa-cli daemon-identity-rollback <entity_type> <entity_id> <target_version> [--reason <text>] [--approver <id>] [--daemon-url <url>]"
    );
    println!("  note: ask uses workflow.stasis.agent_session through Stasis runtime orchestration");
}
