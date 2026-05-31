use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use medousa::{
    IngestRequest, IngestResponse, adapter_heartbeat, consume_ingest_stream, render_stream_body,
    resolve_daemon_url,
};
use reqwest::Client;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::ChatAction;

/// Thin Telegram adapter — listens for messages, forwards to daemon ingester,
/// consumes SSE stream for ask jobs, and renders the final response.

#[derive(Clone)]
struct TelegramAdapterState {
    client: Client,
    daemon_url: String,
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

    let state = Arc::new(TelegramAdapterState {
        client: Client::new(),
        daemon_url,
    });

    println!(
        "medousa_telegram thin adapter started — forwarding to daemon at {}",
        state.daemon_url
    );
    println!("medousa_telegram streaming mode — ingester SSE for ask jobs");

    let bot = Bot::new(token);
    maybe_start_heartbeat_nudges(bot.clone(), state.clone());
    let handler = Update::filter_message().endpoint(handle_message);

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

    let request = IngestRequest {
        channel: "telegram".to_string(),
        user_id: msg
            .from
            .as_ref()
            .map(|user| format!("telegram:user:{}", user.id.0))
            .unwrap_or_else(|| "telegram:user:unknown".to_string()),
        channel_id: format!("telegram:chat:{}", msg.chat.id.0),
        text: input.to_string(),
        attachments: Vec::new(),
    };

    let daemon_url = state.daemon_url.trim_end_matches('/');

    let response = match state
        .client
        .post(format!("{daemon_url}/v1/ingest"))
        .json(&request)
        .send()
        .await
    {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<IngestResponse>().await {
                Ok(response) => response,
                Err(err) => {
                    let error_msg = format!(
                        "ingester error: {}",
                        single_line_summary(&err.to_string(), 300)
                    );
                    bot.send_message(msg.chat.id, error_msg).await?;
                    return Ok(());
                }
            },
            Err(err) => {
                let error_msg = format!(
                    "ingester error: {}",
                    single_line_summary(&err.to_string(), 300)
                );
                bot.send_message(msg.chat.id, error_msg).await?;
                return Ok(());
            }
        },
        Err(err) => {
            let error_msg = format!(
                "ingester error: {}",
                single_line_summary(&err.to_string(), 300)
            );
            bot.send_message(msg.chat.id, error_msg).await?;
            return Ok(());
        }
    };

    if response.stream_ready {
        if let Some(stream_url) = response.stream_url.as_ref() {
            let typing_bot = bot.clone();
            let chat_id = msg.chat.id;
            let typing_task = tokio::spawn(async move {
                loop {
                    let _ = typing_bot
                        .send_chat_action(chat_id, ChatAction::Typing)
                        .await;
                    tokio::time::sleep(Duration::from_secs(4)).await;
                }
            });

            let stream_result = consume_ingest_stream(&state.client, stream_url).await;
            typing_task.abort();

            let body = match stream_result {
                Ok(result) => render_stream_body(&result),
                Err(err) => format!("stream error: {}", single_line_summary(&err.to_string(), 300)),
            };
            let reply = format_stream_reply(&response, &body);
            bot.send_message(msg.chat.id, truncate_for_telegram(&reply)).await?;
            return Ok(());
        }
    }

    let reply = if response.is_new_session {
        format!("🆕 {}\n\n{}", response.reply, explain_commands())
    } else {
        response.reply.clone()
    };
    bot.send_message(msg.chat.id, reply).await?;

    Ok(())
}

fn format_stream_reply(response: &IngestResponse, body: &str) -> String {
    if response.is_new_session {
        format!("🆕 {body}\n\n{}", explain_commands())
    } else {
        body.to_string()
    }
}

fn truncate_for_telegram(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

fn explain_commands() -> &'static str {
    "Commands: /new /help /history /model /depth /stop /regen /health /heartbeat — or send a message to chat."
}

fn maybe_start_heartbeat_nudges(bot: Bot, state: Arc<TelegramAdapterState>) {
    if !adapter_heartbeat::heartbeat_nudges_enabled("MEDOUSA_TELEGRAM") {
        return;
    }

    let Some(chat_ids_raw) = non_empty_env("MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS") else {
        eprintln!("medousa_telegram heartbeat nudges enabled but MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS is empty");
        return;
    };

    let chat_ids = chat_ids_raw
        .split(',')
        .filter_map(|value| value.trim().parse::<i64>().ok())
        .collect::<Vec<_>>();
    if chat_ids.is_empty() {
        return;
    }

    tokio::spawn(async move {
        loop {
            if let Some(summary) =
                adapter_heartbeat::fetch_heartbeat_summary(&state.client, &state.daemon_url).await
            {
                if summary.contains("action=notify") {
                    for chat_id in &chat_ids {
                        let _ = bot.send_message(ChatId(*chat_id), summary.clone()).await;
                    }
                }
            }
            tokio::time::sleep(adapter_heartbeat::heartbeat_poll_interval()).await;
        }
    });
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
        "medousa_telegram — thin adapter

Thin Telegram ingress adapter. Forwards all messages to the daemon's centralized
ingester endpoint (POST /v1/ingest) and consumes SSE streams for ask jobs.

usage:
  cargo run -p medousa --bin medousa_telegram -- [options]

options:
  --daemon-url <url>   Daemon base URL (default: MEDOUSA_DAEMON_URL or http://127.0.0.1:7419)
  --token <token>      Telegram bot token (or MEDOUSA_TELEGRAM_BOT_TOKEN / TELOXIDE_TOKEN)
  env: MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED=true + MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS=<csv>
  -h, --help           Show this message
"
    );
}
