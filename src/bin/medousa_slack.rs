//! Thin Slack adapter — Socket Mode via slack-morphism, forwards to daemon ingester.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use medousa::channel_delivery::truncate_for_slack;
use medousa::{
    AdapterDeliveryOutcome, IngestRequest, default_delivery_timeout,
    format_ingest_ack, resolve_daemon_url, wait_for_ask_delivery,
};
use medousa_sdk::{HttpTransport, MedousaClient};
use reqwest::Client;
use slack_morphism::hyper_tokio::SlackClientHyperHttpsConnector;
use slack_morphism::listener::SlackClientEventsListenerEnvironment;
use slack_morphism::prelude::*;
use slack_morphism::socket_mode::SlackClientSocketModeListener;
use slack_morphism::socket_mode::SlackSocketModeListenerCallbacks;

#[derive(Clone)]
struct SlackAdapterState {
    http_client: Client,
    daemon_url: String,
    bot_token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if has_flag(&args, "--help") || has_flag(&args, "-h") {
        print_usage();
        return Ok(());
    }

    let bot_token = resolve_slack_bot_token(find_arg_value(&args, "--bot-token"))?;
    let app_token = resolve_slack_app_token(find_arg_value(&args, "--app-token"))?;
    let daemon_url = resolve_daemon_url(find_arg_value(&args, "--daemon-url"));

    let state = Arc::new(SlackAdapterState {
        http_client: Client::new(),
        daemon_url,
        bot_token,
    });

    println!(
        "medousa_slack thin adapter started — forwarding to daemon at {}",
        state.daemon_url
    );
    println!("medousa_slack socket mode — final replies via outbox push or poll fallback");

    let connector = SlackClientHyperHttpsConnector::new().context("create slack hyper connector")?;
    let client = Arc::new(SlackClient::new(connector));
    let environment = Arc::new(
        SlackClientEventsListenerEnvironment::new(client).with_user_state((*state).clone()),
    );

    let app_token_value: SlackApiTokenValue = app_token.into();
    let app_token = SlackApiToken::new(app_token_value);

    let callbacks =
        SlackSocketModeListenerCallbacks::new().with_push_events(slack_push_events_callback);

    let listener = SlackClientSocketModeListener::new(
        &SlackClientSocketModeConfig::new(),
        environment,
        callbacks,
    );

    listener.listen_for(&app_token).await?;
    listener.serve().await;
    Ok(())
}

async fn slack_push_events_callback(
    event: SlackPushEventCallback,
    _client: Arc<SlackClient<SlackClientHyperHttpsConnector>>,
    states: SlackClientEventsUserState,
) -> UserCallbackResult<()> {
    let state = {
        let guard = states.read().await;
        guard.get_user_state::<SlackAdapterState>().cloned()
    };

    if let Some(state) = state {
        tokio::spawn(async move {
            if let Err(err) = handle_push_event(Arc::new(state), event).await {
                eprintln!("medousa_slack event handling error: {err:#}");
            }
        });
    }

    Ok(())
}

async fn handle_push_event(
    state: Arc<SlackAdapterState>,
    callback: SlackPushEventCallback,
) -> Result<()> {
    let SlackEventCallbackBody::Message(message) = callback.event else {
        return Ok(());
    };

    if message.subtype.is_some() || message.sender.bot_id.is_some() {
        return Ok(());
    }

    let Some(content) = message.content.as_ref() else {
        return Ok(());
    };
    let Some(text) = content.text.as_ref().map(|value| value.trim()) else {
        return Ok(());
    };
    if text.is_empty() {
        return Ok(());
    }

    let channel_id = message
        .origin
        .channel
        .as_ref()
        .map(|channel| channel.to_string())
        .context("slack message missing channel")?;
    let user_id = message
        .sender
        .user
        .as_ref()
        .map(|user| format!("slack:user:{user}"))
        .unwrap_or_else(|| "slack:user:unknown".to_string());

    let request = IngestRequest {
        channel: "slack".to_string(),
        user_id,
        channel_id: format!("slack:channel:{channel_id}"),
        text: text.to_string(),
        attachments: Vec::new(),
    };

    let daemon_url = state.daemon_url.trim_end_matches('/');
    let sdk = MedousaClient::with_transport(Arc::new(HttpTransport::new()), daemon_url);
    let response = sdk
        .ingest()
        .post(&request)
        .await
        .map_err(|err| anyhow!(err.to_string()))
        .context("ingest request failed")?;

    if response.stream_ready {
        let delivery_outcome = wait_for_ask_delivery(
            &state.http_client,
            &state.daemon_url,
            &response,
            default_delivery_timeout(),
        )
        .await;

        match delivery_outcome {
            Ok(AdapterDeliveryOutcome::PushDelivered) => {}
            Ok(AdapterDeliveryOutcome::Fallback { text }) => {
                post_slack_message(&state, &channel_id, &truncate_for_slack(&text)).await?;
            }
            Ok(AdapterDeliveryOutcome::StreamError { message }) => {
                post_slack_message(
                    &state,
                    &channel_id,
                    &truncate_for_slack(&message),
                )
                .await?;
            }
            Err(err) => {
                post_slack_message(
                    &state,
                    &channel_id,
                    &truncate_for_slack(&format!("delivery error: {err:#}")),
                )
                .await?;
            }
        }

        return Ok(());
    }

    if medousa::adapter_ingest::should_send_immediate_ingest_reply(&response) {
        post_slack_message(&state, &channel_id, &format_ingest_ack(&response)).await?;
    }

    Ok(())
}

async fn post_slack_message(state: &SlackAdapterState, channel: &str, text: &str) -> Result<()> {
    let response = state
        .http_client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(&state.bot_token)
        .json(&serde_json::json!({
            "channel": channel,
            "text": truncate_for_slack(text),
        }))
        .send()
        .await
        .context("slack chat.postMessage request failed")?;

    if !response.status().is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!("slack chat.postMessage http error: {detail}"));
    }

    let body: serde_json::Value = response.json().await.context("decode slack response")?;
    if body.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err(anyhow!(
            "slack chat.postMessage failed: {}",
            body.get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
        ));
    }

    Ok(())
}

fn resolve_slack_bot_token(explicit: Option<&str>) -> Result<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| medousa::session::load_slack_bot_token())
        .or_else(|| non_empty_env("MEDOUSA_SLACK_BOT_TOKEN"))
        .or_else(|| non_empty_env("SLACK_BOT_TOKEN"))
        .ok_or_else(|| {
            anyhow!(
                "missing slack bot token: pass --bot-token, run medousa setup, or set MEDOUSA_SLACK_BOT_TOKEN"
            )
        })
}

fn resolve_slack_app_token(explicit: Option<&str>) -> Result<String> {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| medousa::session::load_slack_app_token())
        .or_else(|| non_empty_env("MEDOUSA_SLACK_APP_TOKEN"))
        .or_else(|| non_empty_env("SLACK_APP_TOKEN"))
        .ok_or_else(|| {
            anyhow!(
                "missing slack app token: pass --app-token or set MEDOUSA_SLACK_APP_TOKEN (xapp-… for Socket Mode)"
            )
        })
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn find_arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn print_usage() {
    println!("medousa_slack — thin Slack adapter (Socket Mode)");
    println!();
    println!("USAGE:");
    println!("  medousa_slack [--daemon-url <url>] [--bot-token <xoxb-…>] [--app-token <xapp-…>]");
    println!();
    println!("ENV:");
    println!("  MEDOUSA_SLACK_BOT_TOKEN / SLACK_BOT_TOKEN");
    println!("  MEDOUSA_SLACK_APP_TOKEN / SLACK_APP_TOKEN");
}
