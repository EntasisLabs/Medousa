use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use medousa::{
    IngestRequest, IngestResponse, consume_ingest_stream, render_stream_body, resolve_daemon_url,
};
use reqwest::Client;
use serenity::all::GatewayIntents;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::{Context as DiscordContext, EventHandler};
use serenity::{Client as DiscordClient, async_trait};

/// Thin Discord adapter — listens for messages, forwards to daemon ingester,
/// renders the response. All business logic lives in the daemon.

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
    println!("medousa_discord streaming mode — ingester SSE for ask jobs");

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
        if let Some(stream_url) = response.stream_url.as_ref() {
            let http = ctx.http.clone();
            let channel_id = msg.channel_id;
            let typing_task = tokio::spawn(async move {
                loop {
                    let _ = channel_id.broadcast_typing(&http).await;
                    tokio::time::sleep(Duration::from_secs(4)).await;
                }
            });

            let stream_result = consume_ingest_stream(&state.client, stream_url).await;
            typing_task.abort();

            let body = match stream_result {
                Ok(result) => render_stream_body(&result),
                Err(err) => format!("stream error: {}", single_line_summary(&err.to_string(), 300)),
            };
            let reply = format_stream_reply(&response, &body, &state.command_prefix);
            msg.channel_id
                .say(&ctx.http, truncate_for_discord(&reply))
                .await
                .context("failed to send discord stream reply")?;
            return Ok(());
        }
    }

    let reply = if response.is_new_session {
        format!(
            "🆕 {}\n\n{}",
            response.reply,
            explain_commands(&state.command_prefix)
        )
    } else {
        response.reply
    };
    msg.channel_id
        .say(&ctx.http, reply)
        .await
        .context("failed to send discord reply")?;

    Ok(())
}

fn format_stream_reply(
    response: &IngestResponse,
    body: &str,
    command_prefix: &str,
) -> String {
    if response.is_new_session {
        format!("🆕 {body}\n\n{}", explain_commands(command_prefix))
    } else {
        body.to_string()
    }
}

fn truncate_for_discord(text: &str) -> String {
    const MAX_CHARS: usize = 1900;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
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
        "Commands: {prefix}new (reset session), {prefix}help (show help), or just send a message to chat."
    )
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
ingester endpoint (POST /v1/ingest). Prefix commands are translated to slash
commands server-side; session and job logic live in the daemon.

usage:
  cargo run -p medousa --bin medousa_discord -- [options]

options:
  --daemon-url <url>       Daemon base URL (default: MEDOUSA_DAEMON_URL or http://127.0.0.1:7419)
  --token <token>          Discord bot token (or MEDOUSA_DISCORD_BOT_TOKEN / DISCORD_TOKEN)
  --command-prefix <pfx>   Prefix for command messages (default: !)
  -h, --help               Show this message
"
    );
}

#[cfg(test)]
mod tests {
    use super::normalize_for_ingester;

    #[test]
    fn prefix_commands_translate_to_slash() {
        assert_eq!(normalize_for_ingester("!help", "!"), "/help");
        assert_eq!(normalize_for_ingester("!new", "!"), "/new");
        assert_eq!(normalize_for_ingester("!ask hello", "!"), "/ask hello");
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(normalize_for_ingester("hello world", "!"), "hello world");
    }
}
