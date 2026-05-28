use std::str::SplitWhitespace;

use anyhow::Result;
use reqwest::Client;

use medousa::{
    EnqueueAskRequest, EnqueueResponse, HealthResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse,
};

use super::{
    EventOutcome, TuiState, WorkerCommand, next_worker_request_id, push_obs, queue_worker_command,
};

pub(crate) fn handle_daemon_command(
    parts: &mut SplitWhitespace<'_>,
    state: &mut TuiState,
) -> EventOutcome {
    let sub = parts.next().unwrap_or("");
    match sub {
        "" => {
            push_obs(
                state,
                format!(
                    "daemon url={} | commands: /daemon health | /daemon ask <prompt> | /daemon url <url>",
                    state.daemon_url
                ),
            );
        }
        "url" => {
            let next = parts.collect::<Vec<_>>().join(" ");
            if next.trim().is_empty() {
                push_obs(state, format!("daemon url={}", state.daemon_url));
            } else {
                state.daemon_url = next.trim().to_string();
                push_obs(state, format!("✓ daemon url set to {}", state.daemon_url));
            }
        }
        "health" => {
            let request_id = next_worker_request_id(state);
            state.latest_daemon_health_request_id = request_id;
            let daemon_url = state.daemon_url.clone();
            let queued = queue_worker_command(
                state,
                WorkerCommand::DaemonHealth {
                    request_id,
                    daemon_url: daemon_url.clone(),
                },
                true,
            );
            if queued {
                push_obs(
                    state,
                    format!("↻ daemon health check queued #{request_id}: {daemon_url}"),
                );
            }
        }
        "ask" => {
            let prompt = parts.collect::<Vec<_>>().join(" ");
            if prompt.trim().is_empty() {
                push_obs(state, "⚠ usage: /daemon ask <prompt>".to_string());
            } else {
                let request_id = next_worker_request_id(state);
                state.latest_daemon_ask_request_id = request_id;
                let daemon_url = state.daemon_url.clone();
                let queued = queue_worker_command(
                    state,
                    WorkerCommand::DaemonAsk {
                        request_id,
                        daemon_url,
                        prompt,
                    },
                    false,
                );
                if queued {
                    push_obs(state, format!("↻ daemon ask queued #{request_id}"));
                }
            }
        }
        _ => {
            push_obs(
                state,
                "⚠ unknown /daemon command. try /daemon health | /daemon ask <prompt> | /daemon url <url>"
                    .to_string(),
            );
        }
    }

    EventOutcome::Continue
}

pub(crate) fn handle_watch_command(
    parts: &mut SplitWhitespace<'_>,
    state: &mut TuiState,
) -> EventOutcome {
    let sub = parts.next().unwrap_or("");
    if sub != "add" {
        push_obs(
            state,
            "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
        );
        return EventOutcome::Continue;
    }

    let cron_expr = match parts.next() {
        Some(value) => value,
        None => {
            push_obs(
                state,
                "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
            );
            return EventOutcome::Continue;
        }
    };

    let prompt = parts.collect::<Vec<_>>().join(" ");
    if prompt.trim().is_empty() {
        push_obs(
            state,
            "⚠ usage: /watch add <cron_expr> <prompt...>".to_string(),
        );
        return EventOutcome::Continue;
    }

    let request_id = next_worker_request_id(state);
    state.latest_watch_add_request_id = request_id;
    let daemon_url = state.daemon_url.clone();
    let cron_expr = cron_expr.to_string();
    let queued = queue_worker_command(
        state,
        WorkerCommand::WatchAdd {
            request_id,
            daemon_url,
            cron_expr: cron_expr.clone(),
            prompt,
        },
        false,
    );
    if queued {
        push_obs(
            state,
            format!("↻ watch add queued #{request_id} ({cron_expr})"),
        );
    }

    EventOutcome::Continue
}

pub(crate) async fn daemon_health(daemon_url: &str) -> Result<HealthResponse> {
    let client = Client::new();
    let response = client
        .get(format!("{daemon_url}/health"))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<HealthResponse>().await?)
}

pub(crate) async fn daemon_enqueue_ask(daemon_url: &str, prompt: &str) -> Result<EnqueueResponse> {
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
    Ok(response.json::<EnqueueResponse>().await?)
}

pub(crate) async fn daemon_register_recurring_prompt(
    daemon_url: &str,
    cron_expr: &str,
    prompt: &str,
) -> Result<RegisterRecurringResponse> {
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
        timezone: Some("UTC".to_string()),
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
    Ok(response.json::<RegisterRecurringResponse>().await?)
}
