use anyhow::{Result, anyhow};
use chrono::Utc;
use medousa::{
    DaemonStatsResponse, EnqueueAskRequest, EnqueueResponse, HealthResponse,
    RegisterRecurringPromptRequest, RegisterRecurringResponse, build_runtime, parse_backend,
    process_once, publish_pending, resolve_daemon_url, resolve_llm_base_url, resolve_llm_provider,
    resolve_llm_target,
};
use reqwest::Client;
use serde_json::json;
use stasis::application::orchestration::runtime_job_payloads::{
    AgentSessionJobPayload, AgentSessionParticipantPayload, AgentToolCallMode, PromptJobPayload,
};
use stasis::application::orchestration::runtime_workflow_job_builder::RuntimeWorkflowJobBuilder;
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
        "daemon-ask" => {
            let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));
            let prompt = args
                .get(1)
                .ok_or_else(|| anyhow!("missing prompt: medousa-cli daemon-ask <prompt>"))?;
            run_daemon_ask(&daemon_url, prompt).await
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

async fn run_daemon_ask(daemon_url: &str, prompt: &str) -> Result<()> {
    let client = Client::new();
    let request = EnqueueAskRequest {
        prompt: prompt.to_string(),
        policy_profile: Some("default".to_string()),
        model_hint: None,
        max_turns: Some(1),
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
        policy_profile: Some("default".to_string()),
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

async fn run_llm(
    runtime: &RuntimeComposition,
    prompt: &str,
    provider: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    let job_id = format!("medousa-llm-{}", now.timestamp_millis());
    let payload = PromptJobPayload {
        user_prompt: prompt.to_string(),
        system_prompt: Some(
            "You are Medousa, a practical research assistant. Be concise and structured."
                .to_string(),
        ),
        policy_profile: Some("default".to_string()),
        model_hint: model.map(|v| v.to_string()),
        memory_policy: None,
    };

    let new_job = RuntimeWorkflowJobBuilder::for_prompt(job_id.clone(), &payload)?
        .with_causation_id("medousa-cli")
        .with_sttp_input_node_id("sttp:in:medousa:llm")
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
    let payload = AgentSessionJobPayload {
        thread_id: Some(job_id.clone()),
        initial_user_prompt: prompt.to_string(),
        participants: vec![AgentSessionParticipantPayload {
            agent_id: "medousa.researcher".to_string(),
            system_prompt: Some(
                "You are Medousa, a practical research assistant. Use tool evidence and cite findings succinctly.".to_string(),
            ),
            tool_name: "stasis.web.search.mock".to_string(),
            tool_input: Some(json!({ "query": prompt })),
        }],
        policy_profile: Some("default".to_string()),
        model_hint: None,
        memory_policy: None,
        max_turns: Some(1),
        tool_call_mode: Some(AgentToolCallMode::Auto),
    };

    let new_job = RuntimeWorkflowJobBuilder::for_agent_session(job_id.clone(), &payload)?
        .with_causation_id("medousa-cli")
        .with_sttp_input_node_id("sttp:in:medousa:ask")
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

fn print_usage() {
    println!("medousa-cli usage:");
    println!(
        "  medousa-cli ask <prompt> [--backend in-memory|surreal-mem] [--provider <provider>] [--model <model_name>] [--base-url <url>]"
    );
    println!(
        "  medousa-cli llm <prompt> [--provider <provider>] [--model <model_name>] [--base-url <url>] [--backend in-memory|surreal-mem]"
    );
    println!("  medousa-cli daemon-health [--daemon-url <url>]");
    println!("  medousa-cli daemon-stats [--daemon-url <url>]");
    println!("  medousa-cli daemon-ask <prompt> [--daemon-url <url>]");
    println!(
        "  medousa-cli daemon-watch-add <cron_expr> <prompt> [--tz <timezone>] [--daemon-url <url>]"
    );
    println!("  note: ask uses workflow.stasis.agent_session through Stasis runtime orchestration");
}
