use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use medousa::{
    AdapterDeliveryOutcome, IngestRequest, IngestResponse,
    default_delivery_timeout, wait_for_ask_delivery, resolve_daemon_url,
};
use medousa::channel_delivery::truncate_for_discord;
use reqwest::Client;
use serenity::all::GatewayIntents;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::{Context as DiscordContext, EventHandler};
use serenity::{Client as DiscordClient, async_trait};

/// Thin Discord adapter — forwards to daemon ingester, shows typing during processing,
/// and relies on outbox-driven push for final replies.

#[derive(Clone)]
struct DiscordAdapterState {
    client: Client,
    daemon_url: String,
    command_prefix: String,
}

struct DiscordHandler {
    state: Arc<DiscordAdapterState>,
}

#[async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, _ctx: DiscordContext, ready: Ready) {
        println!(
            "medousa_discord thin adapter connected as {}#{}",
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
    let command_prefix = find_arg_value(&args, "--command-prefix")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_COMMAND_PREFIX"))
        .unwrap_or_else(|| "!".to_string());

    let state = Arc::new(DiscordAdapterState {
        client: Client::new(),
        daemon_url,
        command_prefix,
    });

    println!(
        "medousa_discord thin adapter started — forwarding to daemon at {}",
        state.daemon_url
    );
    println!("medousa_discord outbox push mode — final replies delivered by daemon");

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let handler = DiscordHandler {
        state: state.clone(),
    };

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

    let text = normalize_for_ingester(input, &state.command_prefix);
    let request = IngestRequest {
        channel: "discord".to_string(),
        user_id: format!("discord:user:{}", msg.author.id.get()),
        channel_id: format!("discord:channel:{}", msg.channel_id.get()),
        text,
        attachments: Vec::new(),
    };

    let daemon_url = state.daemon_url.trim_end_matches('/');

    let response = state
        .client
        .post(format!("{daemon_url}/v1/ingest"))
        .json(&request)
        .send()
        .await
        .context("failed to reach daemon ingest endpoint")?
        .error_for_status()
        .context("daemon ingest endpoint returned error")?
        .json::<IngestResponse>()
        .await
        .context("failed to decode daemon ingest response")?;

    if response.stream_ready {
        let http = ctx.http.clone();
        let channel_id = msg.channel_id;
        let typing_task = tokio::spawn(async move {
            loop {
                let _ = channel_id.broadcast_typing(&http).await;
                tokio::time::sleep(Duration::from_secs(4)).await;
            }
        });

        let delivery_outcome = wait_for_ask_delivery(
            &state.client,
            &state.daemon_url,
            &response,
            default_delivery_timeout(),
        )
        .await;
        typing_task.abort();

        match delivery_outcome {
            Ok(AdapterDeliveryOutcome::PushDelivered) => {}
            Ok(AdapterDeliveryOutcome::Fallback { text }) => {
                msg.channel_id
                    .say(&ctx.http, truncate_for_discord(&text))
                    .await
                    .context("failed to send discord fallback reply")?;
            }
            Ok(AdapterDeliveryOutcome::StreamError { message }) => {
                msg.channel_id
                    .say(&ctx.http, truncate_for_discord(&message))
                    .await
                    .context("failed to send discord delivery error")?;
            }
            Err(err) => {
                let error_msg = format!(
                    "delivery error: {}",
                    single_line_summary(&err.to_string(), 300)
                );
                msg.channel_id
                    .say(&ctx.http, truncate_for_discord(&error_msg))
                    .await
                    .context("failed to send discord delivery error")?;
            }
        }
        return Ok(());
    }

    if medousa::adapter_ingest::should_send_immediate_ingest_reply(&response) {
        msg.channel_id
            .say(
                &ctx.http,
                format_discord_ack(&response, &state.command_prefix),
            )
            .await
            .context("failed to send discord reply")?;
    }

    Ok(())
}

fn format_discord_ack(response: &IngestResponse, command_prefix: &str) -> String {
    if response.is_new_session {
        format!(
            "🆕 {}\n\n{}",
            response.reply,
            explain_commands(command_prefix)
        )
    } else {
        response.reply.clone()
    }
}

/// Translate Discord prefix commands (`!help`) into ingester slash commands (`/help`).
fn normalize_for_ingester(input: &str, command_prefix: &str) -> String {
    if command_prefix.is_empty() {
        return input.to_string();
    }

    let Some(rest) = input.strip_prefix(command_prefix) else {
        return input.to_string();
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return "/help".to_string();
    }

    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(cmd, tail)| (cmd, tail.trim()))
        .unwrap_or((rest, ""));

    let normalized = match command.to_ascii_lowercase().as_str() {
        "start" | "help" => "/help".to_string(),
        "new" => "/new".to_string(),
        "ask" if !args.is_empty() => format!("/ask {args}"),
        "ask" => "/help".to_string(),
        other if args.is_empty() => format!("/{other}"),
        other => format!("/{other} {args}"),
    };

    normalized
}

fn explain_commands(prefix: &str) -> String {
    format!(
        "Commands: {prefix}new {prefix}help {prefix}history {prefix}model {prefix}depth {prefix}stop {prefix}regen {prefix}health {prefix}heartbeat — or send a message to chat."
    )
}

fn resolve_discord_token(explicit: Option<&str>) -> Result<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| non_empty_env("MEDOUSA_DISCORD_BOT_TOKEN"))
        .or_else(|| non_empty_env("DISCORD_TOKEN"))
        .ok_or_else(|| {
            anyhow!(
                "missing discord bot token: pass --token or set MEDOUSA_DISCORD_BOT_TOKEN / DISCORD_TOKEN"
            )
        })
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

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
        "medousa_discord — thin adapter

Thin Discord ingress adapter. Forwards all messages to the daemon's centralized
ingester endpoint (POST /v1/ingest). Final replies are delivered by the daemon via
outbox push; this process only sends acks, typing indicators, and fallback text.

usage:
  cargo run -p medousa --bin medousa_discord -- [options]

options:
  --daemon-url <url>        Daemon base URL (default: MEDOUSA_DAEMON_URL or http://127.0.0.1:7419)
  --token <token>           Discord bot token (or MEDOUSA_DISCORD_BOT_TOKEN / DISCORD_TOKEN)
  --command-prefix <prefix> Command prefix (default: MEDOUSA_DISCORD_COMMAND_PREFIX or !)
  -h, --help                Show this message
"
    );
}
