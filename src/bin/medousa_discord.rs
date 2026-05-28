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
use serenity::all::GatewayIntents;
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::{Context as DiscordContext, EventHandler};
use serenity::{Client as DiscordClient, async_trait};
use tokio::time::{Instant, sleep};

#[derive(Clone, Debug)]
struct DiscordAdapterConfig {
    daemon_url: String,
    policy_profile: String,
    model_hint: Option<String>,
    max_turns: u32,
    identity_persona_id: Option<String>,
    allowed_commands: HashSet<String>,
    command_prefix: String,
    max_prompt_chars: usize,
    max_prompt_chars_by_channel: HashMap<u64, usize>,
    result_poll_interval_ms: u64,
    result_poll_timeout_ms: u64,
    heartbeat_nudges_enabled: bool,
    heartbeat_notify_channel_ids: Vec<u64>,
    heartbeat_poll_interval_ms: u64,
    heartbeat_min_significance: f32,
    heartbeat_adapter_cooldown_ms: u64,
}

#[derive(Clone)]
struct DiscordAdapterState {
    client: Client,
    config: DiscordAdapterConfig,
}

struct DiscordHandler {
    state: Arc<DiscordAdapterState>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, _ctx: DiscordContext, ready: Ready) {
        println!(
            "medousa_discord connected as {}#{}",
            ready.user.name,
            ready.user.discriminator.map(|v| v.get()).unwrap_or(0)
        );
    }

    async fn message(&self, ctx: DiscordContext, msg: Message) {
        if msg.author.bot {
            return;
        }

        if let Err(err) = handle_message(&ctx, &msg, self.state.clone()).await {
            eprintln!("medousa_discord message handling error: {err}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if has_flag(&args, "--help") || has_flag(&args, "-h") {
        print_usage();
        return Ok(());
    }

    let token = resolve_discord_token(find_arg_value(&args, "--token"))?;
    let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));

    let policy_profile = find_arg_value(&args, "--policy-profile")
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_POLICY_PROFILE"))
        .unwrap_or_else(|| {
            default_policy_profile_for_lane(EngineExecutionLane::Interactive).to_string()
        });

    let model_hint = find_arg_value(&args, "--model-hint")
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_MODEL_HINT"));

    let env_max_turns = non_empty_env("MEDOUSA_DISCORD_MAX_TURNS");
    let max_turns = parse_positive_u32(find_arg_value(&args, "--max-turns"), "--max-turns")?
        .or(parse_positive_u32(
            env_max_turns.as_deref(),
            "MEDOUSA_DISCORD_MAX_TURNS",
        )?)
        .unwrap_or(1);

    let identity_persona_id = find_arg_value(&args, "--identity-persona-id")
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_PERSONA_ID"));

    let env_allow_commands = non_empty_env("MEDOUSA_DISCORD_ALLOW_COMMANDS");
    let allowed_commands = parse_allowed_commands(
        find_arg_value(&args, "--allow-commands").or(env_allow_commands.as_deref()),
    )?;

    let env_command_prefix = non_empty_env("MEDOUSA_DISCORD_COMMAND_PREFIX");
    let command_prefix = find_arg_value(&args, "--command-prefix")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or(env_command_prefix)
        .unwrap_or_else(|| "!".to_string());

    let env_max_prompt_chars = non_empty_env("MEDOUSA_DISCORD_MAX_PROMPT_CHARS");
    let max_prompt_chars = parse_positive_usize(
        find_arg_value(&args, "--max-prompt-chars"),
        "--max-prompt-chars",
    )?
    .or(parse_positive_usize(
        env_max_prompt_chars.as_deref(),
        "MEDOUSA_DISCORD_MAX_PROMPT_CHARS",
    )?)
    .unwrap_or(1400);

    let env_max_prompt_chars_by_channel =
        non_empty_env("MEDOUSA_DISCORD_MAX_PROMPT_CHARS_BY_CHANNEL");
    let max_prompt_chars_by_channel = parse_channel_prompt_overrides(
        find_arg_value(&args, "--max-prompt-chars-by-channel")
            .or(env_max_prompt_chars_by_channel.as_deref()),
    )?;

    let env_result_poll_interval_ms = non_empty_env("MEDOUSA_DISCORD_RESULT_POLL_INTERVAL_MS");
    let result_poll_interval_ms = parse_positive_u64(
        find_arg_value(&args, "--result-poll-interval-ms"),
        "--result-poll-interval-ms",
    )?
    .or(parse_positive_u64(
        env_result_poll_interval_ms.as_deref(),
        "MEDOUSA_DISCORD_RESULT_POLL_INTERVAL_MS",
    )?)
    .unwrap_or(700);

    let env_result_poll_timeout_ms = non_empty_env("MEDOUSA_DISCORD_RESULT_POLL_TIMEOUT_MS");
    let result_poll_timeout_ms = parse_non_negative_u64(
        find_arg_value(&args, "--result-poll-timeout-ms"),
        "--result-poll-timeout-ms",
    )?
    .or(parse_non_negative_u64(
        env_result_poll_timeout_ms.as_deref(),
        "MEDOUSA_DISCORD_RESULT_POLL_TIMEOUT_MS",
    )?)
    .unwrap_or(15_000);

    let env_heartbeat_nudges_enabled = non_empty_env("MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED");
    let mut heartbeat_nudges_enabled = has_flag(&args, "--heartbeat-nudges")
        || parse_bool_value(find_arg_value(&args, "--heartbeat-nudges-enabled"), "--heartbeat-nudges-enabled")?
            .or(parse_bool_value(
                env_heartbeat_nudges_enabled.as_deref(),
                "MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED",
            )?)
            .unwrap_or(false);

    let env_heartbeat_channel_ids = non_empty_env("MEDOUSA_DISCORD_HEARTBEAT_CHANNEL_IDS");
    let heartbeat_notify_channel_ids = parse_u64_list(
        find_arg_value(&args, "--heartbeat-channel-ids")
            .or(env_heartbeat_channel_ids.as_deref()),
        "heartbeat channel ids",
    )?;

    let env_heartbeat_poll_interval_ms =
        non_empty_env("MEDOUSA_DISCORD_HEARTBEAT_POLL_INTERVAL_MS");
    let heartbeat_poll_interval_ms = parse_positive_u64(
        find_arg_value(&args, "--heartbeat-poll-interval-ms"),
        "--heartbeat-poll-interval-ms",
    )?
    .or(parse_positive_u64(
        env_heartbeat_poll_interval_ms.as_deref(),
        "MEDOUSA_DISCORD_HEARTBEAT_POLL_INTERVAL_MS",
    )?)
    .unwrap_or(5000);

    let env_heartbeat_min_significance =
        non_empty_env("MEDOUSA_DISCORD_HEARTBEAT_MIN_SIGNIFICANCE");
    let heartbeat_min_significance = parse_ratio_value(
        find_arg_value(&args, "--heartbeat-min-significance")
            .or(env_heartbeat_min_significance.as_deref()),
        "heartbeat min significance",
    )?
    .unwrap_or(0.70);

    let env_heartbeat_adapter_cooldown_ms =
        non_empty_env("MEDOUSA_DISCORD_HEARTBEAT_COOLDOWN_MS");
    let heartbeat_adapter_cooldown_ms = parse_non_negative_u64(
        find_arg_value(&args, "--heartbeat-cooldown-ms"),
        "--heartbeat-cooldown-ms",
    )?
    .or(parse_non_negative_u64(
        env_heartbeat_adapter_cooldown_ms.as_deref(),
        "MEDOUSA_DISCORD_HEARTBEAT_COOLDOWN_MS",
    )?)
    .unwrap_or(180_000);

    if heartbeat_nudges_enabled && heartbeat_notify_channel_ids.is_empty() {
        eprintln!(
            "medousa_discord heartbeat nudges requested but no heartbeat channel ids were configured; disabling nudges"
        );
        heartbeat_nudges_enabled = false;
    }

    let config = DiscordAdapterConfig {
        daemon_url,
        policy_profile,
        model_hint,
        max_turns,
        identity_persona_id,
        allowed_commands,
        command_prefix,
        max_prompt_chars,
        max_prompt_chars_by_channel,
        result_poll_interval_ms,
        result_poll_timeout_ms,
        heartbeat_nudges_enabled,
        heartbeat_notify_channel_ids,
        heartbeat_poll_interval_ms,
        heartbeat_min_significance,
        heartbeat_adapter_cooldown_ms,
    };

    println!(
        "medousa_discord started daemon_url={} policy_profile={} max_turns={} model_hint={} prefix={} allow_commands={} max_prompt_chars={} poll_timeout_ms={} poll_interval_ms={} heartbeat_nudges_enabled={} heartbeat_channel_count={} heartbeat_poll_interval_ms={} heartbeat_min_significance={:.2}",
        config.daemon_url,
        config.policy_profile,
        config.max_turns,
        config.model_hint.as_deref().unwrap_or("none"),
        config.command_prefix,
        display_allowed_commands(&config.allowed_commands),
        config.max_prompt_chars,
        config.result_poll_timeout_ms,
        config.result_poll_interval_ms,
        config.heartbeat_nudges_enabled,
        config.heartbeat_notify_channel_ids.len(),
        config.heartbeat_poll_interval_ms,
        config.heartbeat_min_significance,
    );
    println!(
        "medousa_discord first-run: send {0}help -> {0}health -> {0}ask <prompt>; plain-text ingress={1}",
        config.command_prefix,
        if command_enabled(&config, "text") {
            "enabled"
        } else {
            "disabled (safer default; add text to --allow-commands to enable)"
        }
    );
    println!(
        "medousa_discord safety posture lane=interactive policy_profile={}",
        config.policy_profile,
    );

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let state = Arc::new(DiscordAdapterState {
        client: Client::new(),
        config,
    });

    let handler = DiscordHandler {
        state: state.clone(),
    };

    if state.config.heartbeat_nudges_enabled {
        let heartbeat_http = Arc::new(Http::new(&token));
        let heartbeat_state = state.clone();
        tokio::spawn(async move {
            run_heartbeat_nudge_loop(heartbeat_http, heartbeat_state).await;
        });
    }

    let mut discord = DiscordClient::builder(token, intents)
        .event_handler(handler)
        .await
        .context("failed to create discord client")?;

    discord
        .start()
        .await
        .context("discord gateway client exited with error")
}

async fn handle_message(
    ctx: &DiscordContext,
    msg: &Message,
    state: Arc<DiscordAdapterState>,
) -> Result<()> {
    let input = msg.content.trim();
    if input.is_empty() {
        return Ok(());
    }

    let prompt = if let Some((command, args)) = parse_prefixed_command(input, &state.config.command_prefix) {
        match command.as_str() {
            "" => {
                send_channel_message(
                    &ctx.http,
                    msg.channel_id,
                    &format!(
                        "usage: {}help | {}health | {}ask <prompt>",
                        state.config.command_prefix,
                        state.config.command_prefix,
                        state.config.command_prefix
                    ),
                )
                .await?;
                return Ok(());
            }
            "start" | "help" => {
                if !command_enabled(&state.config, "help") {
                    send_channel_message(
                        &ctx.http,
                        msg.channel_id,
                        &command_disabled_message("help", &state.config.command_prefix),
                    )
                    .await?;
                    return Ok(());
                }

                send_channel_message(&ctx.http, msg.channel_id, &help_text(&state.config)).await?;
                return Ok(());
            }
            "health" => {
                if !command_enabled(&state.config, "health") {
                    send_channel_message(
                        &ctx.http,
                        msg.channel_id,
                        &command_disabled_message("health", &state.config.command_prefix),
                    )
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

                send_channel_message(&ctx.http, msg.channel_id, &reply).await?;
                return Ok(());
            }
            "heartbeat" => {
                if !command_enabled(&state.config, "heartbeat") {
                    send_channel_message(
                        &ctx.http,
                        msg.channel_id,
                        &command_disabled_message("heartbeat", &state.config.command_prefix),
                    )
                    .await?;
                    return Ok(());
                }

                let reply =
                    match query_daemon_heartbeat_status(&state.client, &state.config.daemon_url)
                        .await
                    {
                        Ok(status) => format_heartbeat_nudge(&status),
                        Err(err) => format!(
                            "daemon heartbeat failed: {}",
                            single_line_summary(&err.to_string(), 260)
                        ),
                    };

                send_channel_message(&ctx.http, msg.channel_id, &reply).await?;
                return Ok(());
            }
            "ask" => {
                if !command_enabled(&state.config, "ask") {
                    send_channel_message(
                        &ctx.http,
                        msg.channel_id,
                        &command_disabled_message("ask", &state.config.command_prefix),
                    )
                    .await?;
                    return Ok(());
                }

                if args.is_empty() {
                    send_channel_message(
                        &ctx.http,
                        msg.channel_id,
                        &format!("usage: {}ask <prompt>", state.config.command_prefix),
                    )
                    .await?;
                    return Ok(());
                }

                args
            }
            _ => {
                send_channel_message(
                    &ctx.http,
                    msg.channel_id,
                    &format!(
                        "unsupported command. use {}help",
                        state.config.command_prefix
                    ),
                )
                .await?;
                return Ok(());
            }
        }
    } else {
        if !command_enabled(&state.config, "text") {
            send_channel_message(
                &ctx.http,
                msg.channel_id,
                "plain text ingress is disabled for this adapter. use an allowed command.",
            )
            .await?;
            return Ok(());
        }

        input
    };

    let channel_id = msg.channel_id.get();
    let max_prompt_chars = max_prompt_chars_for_channel(&state.config, channel_id);
    let prompt_chars = prompt.chars().count();
    if prompt_chars > max_prompt_chars {
        send_channel_message(
            &ctx.http,
            msg.channel_id,
            &format!(
                "prompt too long for this channel: chars={} limit={}",
                prompt_chars, max_prompt_chars
            ),
        )
        .await?;
        return Ok(());
    }

    match enqueue_ask_from_message(state.as_ref(), msg, prompt).await {
        Ok(accepted) => {
            let reply = format!(
                "queued ask job_id={} queue={} at={}",
                accepted.job_id, accepted.queue, accepted.accepted_at_utc
            );
            send_channel_message(&ctx.http, msg.channel_id, &reply).await?;

            if state.config.result_poll_timeout_ms > 0 {
                let state_for_poll = state.clone();
                let http = ctx.http.clone();
                let channel_id = msg.channel_id;
                let job_id = accepted.job_id.clone();

                tokio::spawn(async move {
                    match wait_for_terminal_job_result(state_for_poll.as_ref(), &job_id).await {
                        Ok(Some(result)) => {
                            let render = format_terminal_result(&result, 1800);
                            if let Err(err) = send_channel_message(&http, channel_id, &render).await {
                                eprintln!("medousa_discord failed sending job result: {err}");
                            }
                        }
                        Ok(None) => {}
                        Err(err) => {
                            let warning = format!(
                                "job result polling failed: {}",
                                single_line_summary(&err.to_string(), 260)
                            );
                            if let Err(send_err) =
                                send_channel_message(&http, channel_id, &warning).await
                            {
                                eprintln!(
                                    "medousa_discord failed sending polling error: {send_err}"
                                );
                            }
                        }
                    }
                });
            }
        }
        Err(err) => {
            let reply = format!(
                "failed to enqueue ask: {}\ndaemon={}",
                single_line_summary(&err.to_string(), 280),
                state.config.daemon_url,
            );
            send_channel_message(&ctx.http, msg.channel_id, &reply).await?;
        }
    }

    Ok(())
}

async fn send_channel_message(http: &Arc<Http>, channel_id: ChannelId, text: &str) -> Result<()> {
    let rendered = truncate_text_for_budget(text, 1900);
    channel_id
        .say(http, rendered)
        .await
        .context("failed to send discord message")?;
    Ok(())
}

async fn enqueue_ask_from_message(
    state: &DiscordAdapterState,
    msg: &Message,
    prompt: &str,
) -> Result<EnqueueResponse> {
    let daemon_url = state.config.daemon_url.trim_end_matches('/');

    let request = EnqueueAskRequest {
        prompt: prompt.to_string(),
        policy_profile: Some(state.config.policy_profile.clone()),
        model_hint: state.config.model_hint.clone(),
        max_turns: Some(state.config.max_turns),
        identity_user_id: Some(format!("discord:user:{}", msg.author.id.get())),
        identity_persona_id: state.config.identity_persona_id.clone(),
        identity_channel_id: Some(format!("discord:channel:{}", msg.channel_id.get())),
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
    state: &DiscordAdapterState,
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

async fn run_heartbeat_nudge_loop(http: Arc<Http>, state: Arc<DiscordAdapterState>) {
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
                    for channel_id in &state.config.heartbeat_notify_channel_ids {
                        if let Err(err) =
                            send_channel_message(&http, ChannelId::new(*channel_id), &message).await
                        {
                            eprintln!(
                                "medousa_discord heartbeat nudge send failed channel_id={} err={}",
                                channel_id, err
                            );
                        }
                    }
                    last_sent_at = Some(Instant::now());
                }
            }
            Err(err) => {
                eprintln!("medousa_discord heartbeat polling error: {err}");
            }
        }

        sleep(interval).await;
    }
}

fn should_emit_heartbeat_nudge(
    config: &DiscordAdapterConfig,
    status: &HeartbeatStatusResponse,
    decision_increased: bool,
    last_sent_at: Option<Instant>,
) -> bool {
    if !config.heartbeat_nudges_enabled || config.heartbeat_notify_channel_ids.is_empty() {
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

fn help_text(config: &DiscordAdapterConfig) -> String {
    let text_ingress = if config.allowed_commands.contains("text") {
        "enabled"
    } else {
        "disabled (safer default; enable by adding text to allowlist)"
    };

    format!(
        "Medousa Discord ingress is online.\n\nCommands:\n{0}help - show this help\n{0}health - check daemon connectivity\n{0}heartbeat - show daemon heartbeat status\n{0}ask <prompt> - enqueue interactive ask job\n\nAlso accepts /help, /health, /heartbeat, /ask for compatibility.\nPlain text messages are treated like ask only when 'text' is enabled in allowlist.\nText ingress: {11}\nDaemon: {1}\nPolicy profile: {2}\nMax turns: {3}\nCommand prefix: {0}\nAllowed commands: {4}\nDefault max prompt chars: {5}\nPer-channel prompt overrides: {6}\nResult poll timeout ms: {7}\nHeartbeat nudges enabled: {8}\nHeartbeat nudge targets: {9}\nHeartbeat min significance: {10:.2}",
        config.command_prefix,
        config.daemon_url,
        config.policy_profile,
        config.max_turns,
        display_allowed_commands(&config.allowed_commands),
        config.max_prompt_chars,
        config.max_prompt_chars_by_channel.len(),
        config.result_poll_timeout_ms,
        config.heartbeat_nudges_enabled,
        config.heartbeat_notify_channel_ids.len(),
        config.heartbeat_min_significance,
        text_ingress,
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

fn resolve_discord_token(explicit: Option<&str>) -> Result<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_BOT_TOKEN"))
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_TOKEN"))
        .or_else(|| non_empty_env("DISCORD_TOKEN"))
        .ok_or_else(|| {
            anyhow!(
                "missing discord bot token: pass --token or set MEDOUSA_DISCORD_BOT_TOKEN / DISCORD_TOKEN"
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

fn parse_u64_list(value: Option<&str>, label: &str) -> Result<Vec<u64>> {
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
            .parse::<u64>()
            .with_context(|| format!("invalid {label} entry: {item}"))?;
        out.push(parsed);
    }

    Ok(out)
}

fn parse_allowed_commands(value: Option<&str>) -> Result<HashSet<String>> {
    let raw = value.unwrap_or("help,health,heartbeat,ask");
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
        .trim_start_matches('!')
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

fn parse_channel_prompt_overrides(value: Option<&str>) -> Result<HashMap<u64, usize>> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(HashMap::new());
    };

    let mut overrides = HashMap::new();
    for token in raw.split(',') {
        let item = token.trim();
        if item.is_empty() {
            continue;
        }

        let (channel_raw, limit_raw) = item.split_once(':').ok_or_else(|| {
            anyhow!(
                "invalid channel prompt override '{}'; expected <channel_id>:<max_chars>",
                item
            )
        })?;
        let channel_id = channel_raw
            .trim()
            .parse::<u64>()
            .with_context(|| format!("invalid channel id in override '{item}'"))?;
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

        overrides.insert(channel_id, max_chars);
    }

    Ok(overrides)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_prefixed_command<'a>(text: &'a str, command_prefix: &str) -> Option<(String, &'a str)> {
    parse_command_with_prefix(text, command_prefix)
        .or_else(|| if command_prefix == "/" { None } else { parse_command_with_prefix(text, "/") })
}

fn parse_command_with_prefix<'a>(text: &'a str, prefix: &str) -> Option<(String, &'a str)> {
    let body = text.strip_prefix(prefix)?.trim_start();
    if body.is_empty() {
        return Some(("".to_string(), ""));
    }

    let (command, args) = split_command_and_args(body);
    let normalized = command
        .split('@')
        .next()
        .unwrap_or(command)
        .trim()
        .to_ascii_lowercase();
    Some((normalized, args))
}

fn split_command_and_args(body: &str) -> (&str, &str) {
    if let Some((index, _)) = body.char_indices().find(|(_, ch)| ch.is_whitespace()) {
        (body[..index].trim(), body[index..].trim())
    } else {
        (body.trim(), "")
    }
}

fn command_enabled(config: &DiscordAdapterConfig, command: &str) -> bool {
    config.allowed_commands.contains(command)
}

fn command_disabled_message(command: &str, prefix: &str) -> String {
    format!(
        "command {}{} is disabled by adapter allowlist policy",
        prefix, command
    )
}

fn display_allowed_commands(allow: &HashSet<String>) -> String {
    let mut values = allow.iter().cloned().collect::<Vec<_>>();
    values.sort_unstable();
    values.join(",")
}

fn max_prompt_chars_for_channel(config: &DiscordAdapterConfig, channel_id: u64) -> usize {
    config
        .max_prompt_chars_by_channel
        .get(&channel_id)
        .copied()
        .unwrap_or(config.max_prompt_chars)
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
        "medousa_discord\n\n
description:\n  Discord ingress adapter that enqueues interactive asks through medousa daemon.\n\nusage:\n  cargo run -p medousa --bin medousa_discord -- [options]\n\noptions:\n  --daemon-url <url>                Daemon base URL (default: MEDOUSA_DAEMON_URL or http://127.0.0.1:7419)\n  --token <token>                   Discord bot token (or MEDOUSA_DISCORD_BOT_TOKEN / DISCORD_TOKEN)\n  --policy-profile <profile>        Ask policy profile (default: interactive)\n  --model-hint <model>              Optional model hint forwarded to daemon ask payload\n  --max-turns <n>                   Max turns per ask payload (default: 1)\n  --allow-commands <csv>            Allowed intents: help,health,heartbeat,ask,text (default: help,health,heartbeat,ask)\n  --command-prefix <prefix>         Prefix for command messages (default: !)\n  --max-prompt-chars <n>            Default max prompt chars per channel (default: 1400)\n  --max-prompt-chars-by-channel <m> Per-channel overrides: <channel_id>:<max_chars>,...\n  --result-poll-timeout-ms <n>      Polling budget for /v1/jobs/<id>/result (0 disables, default: 15000)\n  --result-poll-interval-ms <n>     Poll interval for result checks (default: 700)\n  --heartbeat-nudges                Enable proactive heartbeat nudges (requires --heartbeat-channel-ids)\n  --heartbeat-nudges-enabled <b>    Explicit heartbeat nudge toggle true/false\n  --heartbeat-channel-ids <csv>     Target channel ids for proactive heartbeat nudges\n  --heartbeat-poll-interval-ms <n>  Poll interval for heartbeat status checks (default: 5000)\n  --heartbeat-min-significance <f>  Minimum significance to emit nudges (default: 0.70)\n  --heartbeat-cooldown-ms <n>       Minimum adapter cooldown between nudges (default: 180000)\n  --identity-persona-id <id>        Optional persona override for Discord asks\n  -h, --help                        Show this message\n"
    );
}
