use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use medousa::{
    AdapterDeliveryOutcome, IngestRequest, IngestResponse, default_delivery_timeout,
    format_ingest_ack, wait_for_ask_delivery, resolve_daemon_url,
};
use medousa::channel_delivery::truncate_for_telegram;
use reqwest::Client;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::ChatAction;

/// Thin Telegram adapter — forwards to daemon ingester, shows typing during processing,
/// and relies on outbox-driven push for final replies.

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
    println!("medousa_telegram outbox push mode — final replies delivered by daemon");

    let bot = Bot::new(token);
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
        bot.send_message(msg.chat.id, format_ingest_ack(&response))
            .await?;

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
                bot.send_message(msg.chat.id, truncate_for_telegram(&text))
                    .await?;
            }
            Ok(AdapterDeliveryOutcome::StreamError { message }) => {
                bot.send_message(msg.chat.id, truncate_for_telegram(&message))
                    .await?;
            }
            Err(err) => {
                let error_msg = format!(
                    "delivery error: {}",
                    single_line_summary(&err.to_string(), 300)
                );
                bot.send_message(msg.chat.id, truncate_for_telegram(&error_msg))
                    .await?;
            }
        }
        return Ok(());
    }

    bot.send_message(msg.chat.id, format_ingest_ack(&response)).await?;
    Ok(())
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
ingester endpoint (POST /v1/ingest). Final replies are delivered by the daemon via
outbox push; this process only sends acks, typing indicators, and fallback text.

usage:
  cargo run -p medousa --bin medousa_telegram -- [options]

options:
  --daemon-url <url>   Daemon base URL (default: MEDOUSA_DAEMON_URL or http://127.0.0.1:7419)
  --token <token>      Telegram bot token (or MEDOUSA_TELEGRAM_BOT_TOKEN / TELOXIDE_TOKEN)
  -h, --help           Show this message
"
    );
}
