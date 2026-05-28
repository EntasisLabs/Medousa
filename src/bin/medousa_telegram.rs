use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use medousa::engine_context::{EngineExecutionLane, default_policy_profile_for_lane};
use medousa::{
    EnqueueAskRequest, EnqueueResponse, HealthResponse, HeartbeatStatusResponse,
    JobResultResponse, resolve_daemon_url,
};
use reqwest::Client;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::dptree;
use teloxide::prelude::*;
use tokio::time::{Instant, sleep};

#[derive(Clone, Debug)]
struct TelegramAdapterConfig {
    daemon_url: String,
    policy_profile: String,
    model_hint: Option<String>,
    max_turns: u32,
    identity_persona_id: Option<String>,
    allowed_commands: HashSet<String>,
    max_prompt_chars: usize,
    max_prompt_chars_by_chat: HashMap<i64, usize>,
    result_poll_interval_ms: u64,
    result_poll_timeout_ms: u64,
    heartbeat_nudges_enabled: bool,
    heartbeat_notify_chat_ids: Vec<i64>,
    heartbeat_poll_interval_ms: u64,
    heartbeat_min_significance: f32,
    heartbeat_adapter_cooldown_ms: u64,
}

#[derive(Clone)]
struct TelegramAdapterState {
    client: Client,
    config: TelegramAdapterConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if has_flag(&args, "--help") || has_flag(&args, "-h") {
        print_usage();
        return Ok(());
    }

    let token = resolve_telegram_token(find_arg_value(&args, "--token"))?;
    let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));

    let policy_profile = find_arg_value(&args, "--policy-profile")
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_TELEGRAM_POLICY_PROFILE"))
        .unwrap_or_else(|| {
            default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
        });

    let model_hint = find_arg_value(&args, "--model-hint")
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_TELEGRAM_MODEL_HINT"));

    let env_max_turns = non_empty_env("MEDOUSA_TELEGRAM_MAX_TURNS");
    let max_turns = parse_positive_u32(find_arg_value(&args, "--max-turns"), "--max-turns")?
        .or(parse_positive_u32(
            env_max_turns.as_deref(),
            "MEDOUSA_TELEGRAM_MAX_TURNS",
        )?)
        .unwrap_or(1);

    let identity_persona_id = find_arg_value(&args, "--identity-persona-id")
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_TELEGRAM_PERSONA_ID"));

    let env_allow_commands = non_empty_env("MEDOUSA_TELEGRAM_ALLOW_COMMANDS");
    let allowed_commands = parse_allowed_commands(
        find_arg_value(&args, "--allow-commands").or(env_allow_commands.as_deref()),
    )?;

    let env_max_prompt_chars = non_empty_env("MEDOUSA_TELEGRAM_MAX_PROMPT_CHARS");
    let max_prompt_chars = parse_positive_usize(
        find_arg_value(&args, "--max-prompt-chars"),
        "--max-prompt-chars",
    )?
    .or(parse_positive_usize(
        env_max_prompt_chars.as_deref(),
        "MEDOUSA_TELEGRAM_MAX_PROMPT_CHARS",
    )?)
    .unwrap_or(1400);

    let env_max_prompt_chars_by_chat = non_empty_env("MEDOUSA_TELEGRAM_MAX_PROMPT_CHARS_BY_CHAT");
    let max_prompt_chars_by_chat = parse_chat_prompt_overrides(
        find_arg_value(&args, "--max-prompt-chars-by-chat")
            .or(env_max_prompt_chars_by_chat.as_deref()),
    )?;

    let env_result_poll_interval_ms = non_empty_env("MEDOUSA_TELEGRAM_RESULT_POLL_INTERVAL_MS");
    let result_poll_interval_ms = parse_positive_u64(
        find_arg_value(&args, "--result-poll-interval-ms"),
        "--result-poll-interval-ms",
    )?
    .or(parse_positive_u64(
        env_result_poll_interval_ms.as_deref(),
        "MEDOUSA_TELEGRAM_RESULT_POLL_INTERVAL_MS",
    )?)
    .unwrap_or(700);

    let env_result_poll_timeout_ms = non_empty_env("MEDOUSA_TELEGRAM_RESULT_POLL_TIMEOUT_MS");
    let result_poll_timeout_ms = parse_non_negative_u64(
        find_arg_value(&args, "--result-poll-timeout-ms"),
        "--result-poll-timeout-ms",
    )?
    .or(parse_non_negative_u64(
        env_result_poll_timeout_ms.as_deref(),
        "MEDOUSA_TELEGRAM_RESULT_POLL_TIMEOUT_MS",
    )?)
    .unwrap_or(15_000);

    let env_heartbeat_nudges_enabled = non_empty_env("MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED");
    let mut heartbeat_nudges_enabled = has_flag(&args, "--heartbeat-nudges")
        || parse_bool_value(find_arg_value(&args, "--heartbeat-nudges-enabled"), "--heartbeat-nudges-enabled")?
            .or(parse_bool_value(
                env_heartbeat_nudges_enabled.as_deref(),
                "MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED",
            )?)
            .unwrap_or(false);

    let env_heartbeat_chat_ids = non_empty_env("MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS");
    let heartbeat_notify_chat_ids = parse_i64_list(
        find_arg_value(&args, "--heartbeat-chat-ids").or(env_heartbeat_chat_ids.as_deref()),
        "heartbeat chat ids",
    )?;

    let env_heartbeat_poll_interval_ms =
        non_empty_env("MEDOUSA_TELEGRAM_HEARTBEAT_POLL_INTERVAL_MS");
    let heartbeat_poll_interval_ms = parse_positive_u64(
        find_arg_value(&args, "--heartbeat-poll-interval-ms"),
        "--heartbeat-poll-interval-ms",
    )?
    .or(parse_positive_u64(
        env_heartbeat_poll_interval_ms.as_deref(),
        "MEDOUSA_TELEGRAM_HEARTBEAT_POLL_INTERVAL_MS",
    )?)
    .unwrap_or(5000);

    let env_heartbeat_min_significance =
        non_empty_env("MEDOUSA_TELEGRAM_HEARTBEAT_MIN_SIGNIFICANCE");
    let heartbeat_min_significance = parse_ratio_value(
        find_arg_value(&args, "--heartbeat-min-significance")
            .or(env_heartbeat_min_significance.as_deref()),
        "heartbeat min significance",
    )?
    .unwrap_or(0.70);

    let env_heartbeat_adapter_cooldown_ms =
        non_empty_env("MEDOUSA_TELEGRAM_HEARTBEAT_COOLDOWN_MS");
    let heartbeat_adapter_cooldown_ms = parse_non_negative_u64(
        find_arg_value(&args, "--heartbeat-cooldown-ms"),
        "--heartbeat-cooldown-ms",
    )?
    .or(parse_non_negative_u64(
        env_heartbeat_adapter_cooldown_ms.as_deref(),
        "MEDOUSA_TELEGRAM_HEARTBEAT_COOLDOWN_MS",
    )?)
    .unwrap_or(180_000);

    if heartbeat_nudges_enabled && heartbeat_notify_chat_ids.is_empty() {
        eprintln!(
            "medousa_telegram heartbeat nudges requested but no heartbeat chat ids were configured; disabling nudges"
        );
        heartbeat_nudges_enabled = false;
    }

    let config = TelegramAdapterConfig {
        daemon_url,
        policy_profile,
        model_hint,
        max_turns,
        identity_persona_id,
        allowed_commands,
        max_prompt_chars,
        max_prompt_chars_by_chat,
        result_poll_interval_ms,
        result_poll_timeout_ms,
        heartbeat_nudges_enabled,
        heartbeat_notify_chat_ids,
        heartbeat_poll_interval_ms,
        heartbeat_min_significance,
        heartbeat_adapter_cooldown_ms,
    };

    println!(
        "medousa_telegram started daemon_url={} policy_profile={} max_turns={} model_hint={} allow_commands={} max_prompt_chars={} poll_timeout_ms={} poll_interval_ms={} heartbeat_nudges_enabled={} heartbeat_chat_count={} heartbeat_poll_interval_ms={} heartbeat_min_significance={:.2}",
        config.daemon_url,
        config.policy_profile,
        config.max_turns,
        config.model_hint.as_deref().unwrap_or("none"),
        display_allowed_commands(&config.allowed_commands),
        config.max_prompt_chars,
        config.result_poll_timeout_ms,
        config.result_poll_interval_ms,
        config.heartbeat_nudges_enabled,
        config.heartbeat_notify_chat_ids.len(),
        config.heartbeat_poll_interval_ms,
        config.heartbeat_min_significance,
    );

    let state = Arc::new(TelegramAdapterState {
        client: Client::new(),
        config,
    });

    let bot = Bot::new(token);
    let handler = Update::filter_message().endpoint(handle_message);

    if state.config.heartbeat_nudges_enabled {
        let heartbeat_bot = bot.clone();
        let heartbeat_state = state.clone();
        tokio::spawn(async move {
            run_heartbeat_nudge_loop(heartbeat_bot, heartbeat_state).await;
        });
    }

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    state: Arc<TelegramAdapterState>,
) -> ResponseResult<()> {
    let Some(text) = msg.text() else {
        return Ok(());
    };

    let input = text.trim();
    if input.is_empty() {
        return Ok(());
    }

    if command_is(input, "/start") || command_is(input, "/help") {
        if !command_enabled(&state.config, "help") {
            bot.send_message(msg.chat.id, command_disabled_message("/help"))
                .await?;
            return Ok(());
        }

        bot.send_message(msg.chat.id, help_text(&state.config)).await?;
        return Ok(());
    }

    if command_is(input, "/health") {
        if !command_enabled(&state.config, "health") {
            bot.send_message(msg.chat.id, command_disabled_message("/health"))
                .await?;
            return Ok(());
        }

        let reply = match query_daemon_health(&state.client, &state.config.daemon_url).await {
            Ok(health) => format!(
                "daemon status={} backend={} worker={} now={}",
                health.status, health.backend, health.worker_id, health.now_utc
            ),
            Err(err) => format!(
                "daemon health failed: {}",
                single_line_summary(&err.to_string(), 260)
            ),
        };

        bot.send_message(msg.chat.id, reply).await?;
        return Ok(());
    }

    if command_is(input, "/heartbeat") {
        if !command_enabled(&state.config, "heartbeat") {
            bot.send_message(msg.chat.id, command_disabled_message("/heartbeat"))
                .await?;
            return Ok(());
        }

        let reply = match query_daemon_heartbeat_status(&state.client, &state.config.daemon_url).await {
            Ok(status) => format_heartbeat_nudge(&status),
            Err(err) => format!(
                "daemon heartbeat failed: {}",
                single_line_summary(&err.to_string(), 260)
            ),
        };

        bot.send_message(msg.chat.id, reply).await?;
        return Ok(());
    }

    let prompt = if command_is(input, "/ask") {
        if !command_enabled(&state.config, "ask") {
            bot.send_message(msg.chat.id, command_disabled_message("/ask"))
                .await?;
            return Ok(());
        }

        let value = extract_ask_prompt(input);
        if value.is_empty() {
            bot.send_message(msg.chat.id, "usage: /ask <prompt>").await?;
            return Ok(());
        }
        value
    } else if command_token(input).is_some() {
        bot.send_message(msg.chat.id, "unsupported command. use /help")
            .await?;
        return Ok(());
    } else {
        if !command_enabled(&state.config, "text") {
            bot.send_message(
                msg.chat.id,
                "plain text ingress is disabled for this adapter. use an allowed command.",
            )
            .await?;
            return Ok(());
        }

        input
    };

    let max_prompt_chars = max_prompt_chars_for_chat(&state.config, msg.chat.id.0);
    let prompt_chars = prompt.chars().count();
    if prompt_chars > max_prompt_chars {
        bot.send_message(
            msg.chat.id,
            format!(
                "prompt too long for this chat: chars={} limit={}",
                prompt_chars, max_prompt_chars
            ),
        )
        .await?;
        return Ok(());
    }

    match enqueue_ask_from_message(state.as_ref(), &msg, prompt).await {
        Ok(accepted) => {
            let reply = format!(
                "queued ask job_id={} queue={} at={}",
                accepted.job_id, accepted.queue, accepted.accepted_at_utc
            );
            bot.send_message(msg.chat.id, reply).await?;

            match wait_for_terminal_job_result(state.as_ref(), &accepted.job_id).await {
                Ok(Some(result)) => {
                    let render = format_terminal_result(&result, 3400);
                    bot.send_message(msg.chat.id, render).await?;
                }
                Ok(None) => {}
                Err(err) => {
                    let warning = format!(
                        "job result polling failed: {}",
                        single_line_summary(&err.to_string(), 260)
                    );
                    bot.send_message(msg.chat.id, warning).await?;
                }
            }
        }
        Err(err) => {
            let reply = format!(
                "failed to enqueue ask: {}\ndaemon={}",
                single_line_summary(&err.to_string(), 280),
                state.config.daemon_url,
            );
            bot.send_message(msg.chat.id, reply).await?;
        }
    }

    Ok(())
}

async fn enqueue_ask_from_message(
    state: &TelegramAdapterState,
    msg: &Message,
    prompt: &str,
) -> Result<EnqueueResponse> {
    let daemon_url = state.config.daemon_url.trim_end_matches('/');

    let request = EnqueueAskRequest {
        prompt: prompt.to_string(),
        policy_profile: Some(state.config.policy_profile.clone()),
        model_hint: state.config.model_hint.clone(),
        max_turns: Some(state.config.max_turns),
        identity_user_id: msg
            .from
            .as_ref()
            .map(|user| format!("telegram:user:{}", user.id.0)),
        identity_persona_id: state.config.identity_persona_id.clone(),
        identity_channel_id: Some(format!("telegram:chat:{}", msg.chat.id.0)),
    };

    let response = state
        .client
        .post(format!("{daemon_url}/v1/jobs/ask"))
        .json(&request)
        .send()
        .await
        .context("failed to reach daemon ask endpoint")?
        .error_for_status()
        .context("daemon rejected ask request")?;

    response
        .json::<EnqueueResponse>()
        .await
        .context("failed to decode daemon ask response")
}

async fn wait_for_terminal_job_result(
    state: &TelegramAdapterState,
    job_id: &str,
) -> Result<Option<JobResultResponse>> {
    if state.config.result_poll_timeout_ms == 0 {
        return Ok(None);
    }

    let interval = Duration::from_millis(state.config.result_poll_interval_ms.max(100));
    let deadline = Instant::now() + Duration::from_millis(state.config.result_poll_timeout_ms);

    loop {
        let response = query_daemon_job_result(&state.client, &state.config.daemon_url, job_id).await?;
        if response.is_terminal {
            return Ok(Some(response));
        }

        if Instant::now() >= deadline {
            return Ok(None);
        }

        sleep(interval).await;
    }
}

async fn query_daemon_job_result(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
) -> Result<JobResultResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/jobs/{job_id}/result"))
        .send()
        .await
        .context("failed to reach daemon job result endpoint")?
        .error_for_status()
        .context("daemon job result endpoint returned error")?;

    response
        .json::<JobResultResponse>()
        .await
        .context("failed to decode daemon job result response")
}

async fn query_daemon_health(client: &Client, daemon_url: &str) -> Result<HealthResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/health"))
        .send()
        .await
        .context("failed to reach daemon health endpoint")?
        .error_for_status()
        .context("daemon health endpoint returned error")?;

    response
        .json::<HealthResponse>()
        .await
        .context("failed to decode daemon health response")
}

async fn query_daemon_heartbeat_status(
    client: &Client,
    daemon_url: &str,
) -> Result<HeartbeatStatusResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/heartbeat/status"))
        .send()
        .await
        .context("failed to reach daemon heartbeat endpoint")?
        .error_for_status()
        .context("daemon heartbeat endpoint returned error")?;

    response
        .json::<HeartbeatStatusResponse>()
        .await
        .context("failed to decode daemon heartbeat response")
}

async fn run_heartbeat_nudge_loop(bot: Bot, state: Arc<TelegramAdapterState>) {
    let interval = Duration::from_millis(state.config.heartbeat_poll_interval_ms.max(1000));
    let mut last_seen_notify_decisions = 0u64;
    let mut has_baseline = false;
    let mut last_sent_at: Option<Instant> = None;

    loop {
        match query_daemon_heartbeat_status(&state.client, &state.config.daemon_url).await {
            Ok(status) => {
                let notify_decisions = status.delivery_metrics.notify_decisions;
                let decision_increased = if has_baseline {
                    notify_decisions > last_seen_notify_decisions
                } else {
                    notify_decisions > 0
                };
                has_baseline = true;
                last_seen_notify_decisions = notify_decisions;

                if should_emit_heartbeat_nudge(
                    &state.config,
                    &status,
                    decision_increased,
                    last_sent_at,
                ) {
                    let message = format_heartbeat_nudge(&status);
                    for chat_id in &state.config.heartbeat_notify_chat_ids {
                        if let Err(err) = bot.send_message(ChatId(*chat_id), message.clone()).await {
                            eprintln!(
                                "medousa_telegram heartbeat nudge send failed chat_id={} err={}",
                                chat_id, err
                            );
                        }
                    }
                    last_sent_at = Some(Instant::now());
                }
            }
            Err(err) => {
                eprintln!("medousa_telegram heartbeat polling error: {err}");
            }
        }

        sleep(interval).await;
    }
}

fn should_emit_heartbeat_nudge(
    config: &TelegramAdapterConfig,
    status: &HeartbeatStatusResponse,
    decision_increased: bool,
    last_sent_at: Option<Instant>,
) -> bool {
    if !config.heartbeat_nudges_enabled || config.heartbeat_notify_chat_ids.is_empty() {
        return false;
    }
    if !decision_increased {
        return false;
    }
    if !status.action.eq_ignore_ascii_case("notify") {
        return false;
    }
    if status.significance < config.heartbeat_min_significance {
        return false;
    }
    if status.delivery_policy.in_quiet_hours {
        return false;
    }

    if config.heartbeat_adapter_cooldown_ms > 0 {
        if let Some(last_sent_at) = last_sent_at {
            if last_sent_at.elapsed() < Duration::from_millis(config.heartbeat_adapter_cooldown_ms)
            {
                return false;
            }
        }
    }

    true
}

fn format_heartbeat_nudge(status: &HeartbeatStatusResponse) -> String {
    format!(
        "heartbeat action={} significance={:.2} reason={}\nfailed={} dead_letter={} outbox_pending={}\ndelivery dispatched={} suppressed_quiet={} suppressed_interval={} last_tick={:?}",
        status.action,
        status.significance,
        single_line_summary(&status.reason, 220),
        status.failed_jobs,
        status.dead_letter_jobs,
        status.pending_outbox_events,
        status.delivery_metrics.dispatched_notifications,
        status.delivery_metrics.suppressed_quiet_hours,
        status.delivery_metrics.suppressed_min_interval,
        status.last_tick_at_utc,
    )
}

fn help_text(config: &TelegramAdapterConfig) -> String {
    format!(
        "Medousa Telegram ingress is online.\n\nCommands:\n/help - show this help\n/health - check daemon connectivity\n/heartbeat - show daemon heartbeat status\n/ask <prompt> - enqueue interactive ask job\n\nPlain text messages are treated like /ask only when 'text' is enabled in allowlist.\nDaemon: {}\nPolicy profile: {}\nMax turns: {}\nAllowed commands: {}\nDefault max prompt chars: {}\nPer-chat prompt overrides: {}\nResult poll timeout ms: {}\nHeartbeat nudges enabled: {}\nHeartbeat nudge targets: {}\nHeartbeat min significance: {:.2}",
        config.daemon_url,
        config.policy_profile,
        config.max_turns,
        display_allowed_commands(&config.allowed_commands),
        config.max_prompt_chars,
        config.max_prompt_chars_by_chat.len(),
        config.result_poll_timeout_ms,
        config.heartbeat_nudges_enabled,
        config.heartbeat_notify_chat_ids.len(),
        config.heartbeat_min_significance,
    )
}

fn format_terminal_result(result: &JobResultResponse, max_chars: usize) -> String {
    match result.status.as_str() {
        "succeeded" => {
            if let Some(text) = result.output_text.as_deref() {
                format!(
                    "result job_id={}\n{}",
                    result.job_id,
                    truncate_text_for_budget(text, max_chars)
                )
            } else {
                format!(
                    "job {} succeeded but no output_text was available in diagnostics",
                    result.job_id
                )
            }
        }
        "failed" => format!(
            "job {} failed outcome={}",
            result.job_id,
            result.latest_outcome.as_deref().unwrap_or("unknown")
        ),
        _ => format!(
            "job {} status={} attempts={} outcome={}",
            result.job_id,
            result.status,
            result.attempt_count,
            result.latest_outcome.as_deref().unwrap_or("unknown"),
        ),
    }
}

fn truncate_text_for_budget(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn resolve_telegram_token(explicit: Option<&str>) -> Result<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_TELEGRAM_BOT_TOKEN"))
        .or_else(|| non_empty_env("MEDOUSA_TELEGRAM_TOKEN"))
        .or_else(|| non_empty_env("TELOXIDE_TOKEN"))
        .ok_or_else(|| {
            anyhow!(
                "missing telegram bot token: pass --token or set MEDOUSA_TELEGRAM_BOT_TOKEN / TELOXIDE_TOKEN"
            )
        })
}

fn parse_positive_u32(value: Option<&str>, label: &str) -> Result<Option<u32>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = raw
        .parse::<u32>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if parsed == 0 {
        return Err(anyhow!("{label} must be greater than zero"));
    }

    Ok(Some(parsed))
}

fn parse_positive_usize(value: Option<&str>, label: &str) -> Result<Option<usize>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = raw
        .parse::<usize>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if parsed == 0 {
        return Err(anyhow!("{label} must be greater than zero"));
    }

    Ok(Some(parsed))
}

fn parse_positive_u64(value: Option<&str>, label: &str) -> Result<Option<u64>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = raw
        .parse::<u64>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if parsed == 0 {
        return Err(anyhow!("{label} must be greater than zero"));
    }

    Ok(Some(parsed))
}

fn parse_non_negative_u64(value: Option<&str>, label: &str) -> Result<Option<u64>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = raw
        .parse::<u64>()
        .with_context(|| format!("invalid {label}: {raw}"))?;

    Ok(Some(parsed))
}

fn parse_bool_value(value: Option<&str>, label: &str) -> Result<Option<bool>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(Some(true)),
        "0" | "false" | "no" | "off" => Ok(Some(false)),
        _ => Err(anyhow!(
            "invalid {label}: {raw} (expected true/false)",
        )),
    }
}

fn parse_ratio_value(value: Option<&str>, label: &str) -> Result<Option<f32>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = raw
        .parse::<f32>()
        .with_context(|| format!("invalid {label}: {raw}"))?;
    if !(0.0..=1.0).contains(&parsed) {
        return Err(anyhow!("{label} must be between 0.0 and 1.0"));
    }

    Ok(Some(parsed))
}

fn parse_i64_list(value: Option<&str>, label: &str) -> Result<Vec<i64>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for token in raw.split(',') {
        let item = token.trim();
        if item.is_empty() {
            continue;
        }
        let parsed = item
            .parse::<i64>()
            .with_context(|| format!("invalid {label} entry: {item}"))?;
        out.push(parsed);
    }

    Ok(out)
}

fn parse_allowed_commands(value: Option<&str>) -> Result<HashSet<String>> {
    let raw = value.unwrap_or("help,health,heartbeat,ask,text");
    let mut allow = HashSet::new();

    for token in raw.split(',') {
        let normalized = normalize_allowed_command(token)?;
        allow.insert(normalized);
    }

    if allow.is_empty() {
        return Err(anyhow!(
            "allowlist resolved to empty set; expected one or more of help,health,heartbeat,ask,text"
        ));
    }

    Ok(allow)
}

fn normalize_allowed_command(raw: &str) -> Result<String> {
    let token = raw
        .trim()
        .trim_start_matches('/')
        .split('@')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    if token.is_empty() {
        return Err(anyhow!("invalid empty command token in allowlist"));
    }

    match token.as_str() {
        "start" | "help" => Ok("help".to_string()),
        "health" | "heartbeat" | "ask" | "text" => Ok(token),
        _ => Err(anyhow!(
            "unsupported allowlist command '{}' (expected help,health,heartbeat,ask,text)",
            raw.trim()
        )),
    }
}

fn parse_chat_prompt_overrides(value: Option<&str>) -> Result<HashMap<i64, usize>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(HashMap::new());
    };

    let mut overrides = HashMap::new();
    for token in raw.split(',') {
        let item = token.trim();
        if item.is_empty() {
            continue;
        }

        let (chat_raw, limit_raw) = item.split_once(':').ok_or_else(|| {
            anyhow!(
                "invalid chat prompt override '{}'; expected <chat_id>:<max_chars>",
                item
            )
        })?;
        let chat_id = chat_raw
            .trim()
            .parse::<i64>()
            .with_context(|| format!("invalid chat id in override '{item}'"))?;
        let max_chars = limit_raw
            .trim()
            .parse::<usize>()
            .with_context(|| format!("invalid max chars in override '{item}'"))?;
        if max_chars == 0 {
            return Err(anyhow!(
                "invalid max chars in override '{}': value must be greater than zero",
                item
            ));
        }

        overrides.insert(chat_id, max_chars);
    }

    Ok(overrides)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn command_token(text: &str) -> Option<&str> {
    let token = text.split_whitespace().next()?;
    if token.starts_with('/') {
        Some(token)
    } else {
        None
    }
}

fn command_is(text: &str, expected: &str) -> bool {
    let Some(token) = command_token(text) else {
        return false;
    };
    let normalized = token.split('@').next().unwrap_or(token);
    normalized.eq_ignore_ascii_case(expected)
}

fn command_enabled(config: &TelegramAdapterConfig, command: &str) -> bool {
    config.allowed_commands.contains(command)
}

fn command_disabled_message(command: &str) -> String {
    format!("command {} is disabled by adapter allowlist policy", command)
}

fn display_allowed_commands(allow: &HashSet<String>) -> String {
    let mut values = allow.iter().cloned().collect::<Vec<_>>();
    values.sort_unstable();
    values.join(",")
}

fn max_prompt_chars_for_chat(config: &TelegramAdapterConfig, chat_id: i64) -> usize {
    config
        .max_prompt_chars_by_chat
        .get(&chat_id)
        .copied()
        .unwrap_or(config.max_prompt_chars)
}

fn extract_ask_prompt(text: &str) -> &str {
    if let Some((index, _)) = text.char_indices().find(|(_, ch)| ch.is_whitespace()) {
        text[index..].trim()
    } else {
        ""
    }
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

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn find_arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn print_usage() {
    println!(
        "medousa_telegram\n\n
description:\n  Telegram ingress adapter that enqueues interactive asks through medousa daemon.\n\nusage:\n  cargo run -p medousa --bin medousa_telegram -- [options]\n\noptions:\n  --daemon-url <url>              Daemon base URL (default: MEDOUSA_DAEMON_URL or http://127.0.0.1:7419)\n  --token <token>                 Telegram bot token (or MEDOUSA_TELEGRAM_BOT_TOKEN / TELOXIDE_TOKEN)\n  --policy-profile <profile>      Ask policy profile (default: interactive)\n  --model-hint <model>            Optional model hint forwarded to daemon ask payload\n  --max-turns <n>                 Max turns per ask payload (default: 1)\n  --allow-commands <csv>          Allowed intents: help,health,heartbeat,ask,text (default: help,health,heartbeat,ask,text)\n  --max-prompt-chars <n>          Default max prompt chars per chat (default: 1400)\n  --max-prompt-chars-by-chat <m>  Per-chat overrides: <chat_id>:<max_chars>,...\n  --result-poll-timeout-ms <n>    Polling budget for /v1/jobs/<id>/result (0 disables, default: 15000)\n  --result-poll-interval-ms <n>   Poll interval for result checks (default: 700)\n  --heartbeat-nudges              Enable proactive heartbeat nudges (requires --heartbeat-chat-ids)\n  --heartbeat-nudges-enabled <b>  Explicit heartbeat nudge toggle true/false\n  --heartbeat-chat-ids <csv>      Target chat ids for proactive heartbeat nudges\n  --heartbeat-poll-interval-ms <n> Poll interval for heartbeat status checks (default: 5000)\n  --heartbeat-min-significance <f> Minimum significance to emit nudges (default: 0.70)\n  --heartbeat-cooldown-ms <n>     Minimum adapter cooldown between nudges (default: 180000)\n  --identity-persona-id <id>      Optional persona override for Telegram asks\n  -h, --help                      Show this message\n"
    );
}
