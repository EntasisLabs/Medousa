use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use stasis::ports::outbound::runtime::delivery_endpoint_store::DeliveryEndpointStore;
use stasis::prelude::RuntimeComposition;
use stasis::runtime_prelude_ext::{DeliveryProtocol, NewDeliveryEndpoint, SurrealDeliveryEndpointStore};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;

use crate::session::{
    load_discord_bot_token, load_slack_bot_token, load_telegram_bot_token,
};

const DELIVERY_SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE delivery_endpoint SCHEMALESS",
    "DEFINE TABLE endpoint_delivery_status SCHEMALESS",
];

/// True when Surreal returns a missing-table error for runtime control-plane tables.
pub fn is_missing_runtime_table_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("the table '") && lowered.contains("does not exist")
}

/// Ensure delivery control-plane tables exist on Surreal backends.
pub async fn ensure_surreal_delivery_schema(db: &Surreal<Any>) -> Result<()> {
    for statement in DELIVERY_SCHEMA_STATEMENTS {
        if let Err(err) = db.query(*statement).await {
            let text = err.to_string();
            if text.contains("already exists")
                || text.contains("already defined")
                || text.contains("Overwrite index")
            {
                continue;
            }
            return Err(anyhow!("delivery schema bootstrap ({statement}): {text}"));
        }
    }

    Ok(())
}

pub const INTERNAL_OUTBOX_ENDPOINT_ID: &str = "medousa.internal.outbox";
pub const INTERNAL_OUTBOX_DELIVER_PATH: &str = "/v1/deliver/outbox";

pub const CHANNEL_HOME: &str = "home";
pub const CHANNEL_HOME_DESKTOP: &str = "home-desktop";
pub const CHANNEL_HOME_IOS: &str = "home-ios";
pub const CHANNEL_HOME_ANDROID: &str = "home-android";
pub const CHANNEL_TUI: &str = "tui";
pub const CHANNEL_INTERACTIVE: &str = "interactive";

/// Normalize legacy/generic home surface tags to explicit workshop channels.
pub fn normalize_channel_surface(channel: &str) -> String {
    match channel.trim().to_ascii_lowercase().as_str() {
        "" => CHANNEL_INTERACTIVE.to_string(),
        CHANNEL_HOME => CHANNEL_HOME_DESKTOP.to_string(),
        other => other.to_string(),
    }
}

pub fn is_home_channel(channel: &str) -> bool {
    matches!(
        channel.trim().to_ascii_lowercase().as_str(),
        CHANNEL_HOME | CHANNEL_HOME_DESKTOP | CHANNEL_HOME_IOS | CHANNEL_HOME_ANDROID
    )
}

/// Principal-facing surfaces where continuation synthesis and legacy loop extras stay off.
pub fn is_principal_interactive_channel(channel: &str) -> bool {
    let normalized = normalize_channel_surface(channel);
    normalized == CHANNEL_INTERACTIVE
        || normalized == CHANNEL_TUI
        || is_home_channel(&normalized)
}

pub fn is_external_push_channel(channel: &str) -> bool {
    matches!(
        channel.trim().to_ascii_lowercase().as_str(),
        "telegram" | "discord" | "slack" | "whatsapp"
    )
}

pub fn work_deep_link_url(card_id: &str) -> String {
    format!("medousa://work/{}", card_id.trim())
}

pub fn delivery_target_from_interactive_turn(
    request: &crate::daemon_api::InteractiveTurnRequest,
    turn_id: &str,
) -> ChannelDeliveryTarget {
    let session_id = request.session_id.clone();
    if let Some(surface) = request.surface.as_ref() {
        let channel = surface
            .channel_surface
            .as_deref()
            .map(normalize_channel_surface)
            .unwrap_or_else(|| CHANNEL_INTERACTIVE.to_string());
        let channel_id = surface
            .channel_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| session_id.clone());
        let user_id = surface
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| session_id.clone());
        return ChannelDeliveryTarget {
            channel,
            user_id,
            channel_id,
            session_id,
            stream_id: Some(turn_id.to_string()),
        };
    }

    ChannelDeliveryTarget {
        channel: CHANNEL_INTERACTIVE.to_string(),
        user_id: session_id.clone(),
        channel_id: session_id.clone(),
        session_id,
        stream_id: Some(turn_id.to_string()),
    }
}

/// Where to deliver a completed ingest job (keyed by `job_id` in the daemon).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDeliveryTarget {
    pub channel: String,
    pub user_id: String,
    pub channel_id: String,
    pub session_id: String,
    pub stream_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobDeliveryState {
    Pending,
    Delivered,
    Failed,
}

#[derive(Debug, Clone)]
pub struct JobDeliveryRecord {
    pub state: JobDeliveryState,
    pub delivered_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub latency_ms: Option<u64>,
}

/// Resolve bearer token for the internal outbox webhook (env overrides product config).
pub fn resolve_deliver_webhook_token() -> Option<String> {
    std::env::var("MEDOUSA_DELIVER_WEBHOOK_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            crate::load_product_config()
                .daemon
                .deliver_webhook_token
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

pub fn verify_deliver_webhook_bearer(
    authorization: Option<&str>,
    expected_token: Option<&str>,
) -> bool {
    let Some(expected) = expected_token.filter(|value| !value.is_empty()) else {
        return true;
    };

    authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
}

/// Stasis `HttpWebhookTransportPublisher` payload shape (snake_case JSON).
#[derive(Debug, Clone, Deserialize)]
pub struct OutboxDeliveryWebhook {
    pub event_id: String,
    pub event_type: String,
    pub job_id: String,
    pub message: Option<String>,
}

/// Loopback URL for the internal outbox webhook (Stasis POSTs here after publish).
pub fn internal_deliver_webhook_url(bind: &str) -> String {
    let host_port = bind.trim();
    let base = if let Some(port) = host_port.strip_prefix("0.0.0.0:") {
        format!("http://127.0.0.1:{port}")
    } else if let Some(port) = host_port.strip_prefix("[::]:") {
        format!("http://127.0.0.1:{port}")
    } else {
        format!("http://{host_port}")
    };
    format!("{base}{INTERNAL_OUTBOX_DELIVER_PATH}")
}

pub async fn seed_internal_outbox_endpoint(
    store: &dyn DeliveryEndpointStore,
    target_url: &str,
) -> Result<()> {
    tracing::info!(
        endpoint_id = INTERNAL_OUTBOX_ENDPOINT_ID,
        "reading delivery endpoint"
    );
    let existing = match store.get(INTERNAL_OUTBOX_ENDPOINT_ID).await {
        Ok(record) => record,
        Err(err) if is_missing_runtime_table_error(&err.to_string()) => None,
        Err(err) => {
            return Err(err).with_context(|| {
                format!("delivery_endpoint.get id={INTERNAL_OUTBOX_ENDPOINT_ID}")
            });
        }
    };

    if existing.is_some() {
        tracing::info!(
            endpoint_id = INTERNAL_OUTBOX_ENDPOINT_ID,
            "delivery endpoint already present — skip insert"
        );
        return Ok(());
    }

    tracing::info!(
        endpoint_id = INTERNAL_OUTBOX_ENDPOINT_ID,
        target = target_url,
        "inserting delivery endpoint"
    );
    store
        .insert(NewDeliveryEndpoint {
            endpoint_id: INTERNAL_OUTBOX_ENDPOINT_ID.to_string(),
            name: "Medousa internal outbox delivery".to_string(),
            protocol: DeliveryProtocol::HttpWebhook,
            target: target_url.to_string(),
            metadata: Some("medousa:channel-delivery".to_string()),
            created_at: Utc::now(),
        })
        .await
        .with_context(|| {
            format!(
                "delivery_endpoint.insert id={INTERNAL_OUTBOX_ENDPOINT_ID} target={target_url}"
            )
        })?;

    Ok(())
}

pub async fn seed_internal_outbox_endpoint_for_runtime(
    runtime: &RuntimeComposition,
    in_memory_endpoint_store: Option<Arc<dyn DeliveryEndpointStore>>,
    target_url: &str,
) -> Result<()> {
    use crate::runtime::surreal_startup::timed_step;

    let store: Arc<dyn DeliveryEndpointStore> = match in_memory_endpoint_store {
        Some(store) => store,
        None => match runtime {
            RuntimeComposition::InMemory(_) => {
                return Err(anyhow!(
                    "in-memory runtime requires pre-built delivery endpoint store"
                ));
            }
            RuntimeComposition::Surreal(rt) => {
                let db = rt.job_store.db();
                timed_step("delivery_endpoint schema", || async {
                    ensure_surreal_delivery_schema(&db).await
                })
                .await?;
                Arc::new(SurrealDeliveryEndpointStore::new(db))
            }
        },
    };

    timed_step("delivery_endpoint seed", || async {
        seed_internal_outbox_endpoint(store.as_ref(), target_url).await
    })
    .await
}

pub fn truncate_for_telegram(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

/// Escape model markdown for Telegram `MarkdownV2` delivery (preserves structure, escapes specials).
pub fn format_for_telegram_markdown_v2(text: &str) -> String {
    telegram_escape::tg_escape(&truncate_for_telegram(text))
}

pub fn parse_telegram_chat_id(channel_id: &str) -> Result<i64> {
    channel_id
        .strip_prefix("telegram:chat:")
        .or_else(|| channel_id.strip_prefix("telegram:"))
        .context("telegram channel_id must be telegram:chat:<id>")?
        .parse::<i64>()
        .context("telegram chat id must be numeric")
}

pub fn truncate_for_discord(text: &str) -> String {
    const MAX_CHARS: usize = 1900;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

pub fn truncate_for_slack(text: &str) -> String {
    const MAX_CHARS: usize = 3900;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

pub fn truncate_for_whatsapp(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
}

pub fn parse_discord_channel_id(channel_id: &str) -> Result<u64> {
    channel_id
        .strip_prefix("discord:channel:")
        .or_else(|| channel_id.strip_prefix("discord:"))
        .context("discord channel_id must be discord:channel:<id>")?
        .parse::<u64>()
        .context("discord channel id must be numeric")
}

pub fn parse_slack_channel_id(channel_id: &str) -> Result<String> {
    channel_id
        .strip_prefix("slack:channel:")
        .or_else(|| channel_id.strip_prefix("slack:"))
        .context("slack channel_id must be slack:channel:<id>")
        .map(str::to_string)
}

pub fn parse_whatsapp_chat_jid(channel_id: &str) -> Result<String> {
    channel_id
        .strip_prefix("whatsapp:chat:")
        .or_else(|| channel_id.strip_prefix("whatsapp:"))
        .context("whatsapp channel_id must be whatsapp:chat:<jid>")
        .map(str::to_string)
}

pub fn resolve_whatsapp_deliver_url() -> String {
    std::env::var("MEDOUSA_WHATSAPP_DELIVER_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            crate::load_product_config()
                .whatsapp
                .deliver_url
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| {
            let bind = std::env::var("MEDOUSA_WHATSAPP_DELIVER_BIND")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| {
                    crate::load_product_config()
                        .whatsapp
                        .deliver_bind
                        .clone()
                });
            format!("http://{bind}/v1/deliver")
        })
}

pub async fn dispatch_channel_message(
    client: &reqwest::Client,
    target: &ChannelDeliveryTarget,
    text: &str,
) -> Result<()> {
    match target.channel.as_str() {
        "telegram" => dispatch_telegram_message(client, &target.channel_id, text).await,
        "discord" => dispatch_discord_message(client, &target.channel_id, text).await,
        "slack" => dispatch_slack_message(client, &target.channel_id, text).await,
        "whatsapp" => dispatch_whatsapp_message(client, &target.channel_id, text).await,
        "cli" => Ok(()),
        other => Err(anyhow!("unsupported delivery channel: {other}")),
    }
}

async fn dispatch_telegram_message(
    client: &reqwest::Client,
    channel_id: &str,
    text: &str,
) -> Result<()> {
    let token = load_telegram_bot_token()
        .context("telegram bot token missing; run medousa setup to configure Telegram")?;
    let chat_id = parse_telegram_chat_id(channel_id)?;
    let plain = truncate_for_telegram(text);
    let markdown = format_for_telegram_markdown_v2(text);
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");

    let markdown_response = client
        .post(&url)
        .json(&json!({
            "chat_id": chat_id,
            "text": markdown,
            "parse_mode": "MarkdownV2",
        }))
        .send()
        .await
        .context("telegram sendMessage request failed")?;

    if markdown_response.status().is_success() {
        return Ok(());
    }

    let markdown_status = markdown_response.status();
    let markdown_detail = markdown_response.text().await.unwrap_or_default();

    let plain_response = client
        .post(&url)
        .json(&json!({
            "chat_id": chat_id,
            "text": plain,
        }))
        .send()
        .await
        .context("telegram plain sendMessage request failed")?;

    if !plain_response.status().is_success() {
        let plain_status = plain_response.status();
        let plain_detail = plain_response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "telegram sendMessage failed (markdown {markdown_status}: {markdown_detail}; plain {plain_status}: {plain_detail})"
        ));
    }

    Ok(())
}

async fn dispatch_discord_message(
    client: &reqwest::Client,
    channel_id: &str,
    text: &str,
) -> Result<()> {
    let token = load_discord_bot_token()
        .context("discord bot token missing; run medousa setup to configure Discord")?;
    let channel = parse_discord_channel_id(channel_id)?;
    let body = truncate_for_discord(text);
    let url = format!("https://discord.com/api/v10/channels/{channel}/messages");

    let response = client
        .post(&url)
        .header("Authorization", format!("Bot {token}"))
        .json(&json!({ "content": body }))
        .send()
        .await
        .context("discord channel message request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "discord channel message returned {status}: {detail}"
        ));
    }

    Ok(())
}

async fn dispatch_slack_message(
    client: &reqwest::Client,
    channel_id: &str,
    text: &str,
) -> Result<()> {
    let token = load_slack_bot_token()
        .context("slack bot token missing; run medousa setup or set MEDOUSA_SLACK_BOT_TOKEN")?;
    let channel = parse_slack_channel_id(channel_id)?;
    let body = truncate_for_slack(text);

    let response = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(token)
        .json(&json!({
            "channel": channel,
            "text": body,
        }))
        .send()
        .await
        .context("slack chat.postMessage request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "slack chat.postMessage returned {status}: {detail}"
        ));
    }

    let payload: serde_json::Value = response.json().await.context("decode slack response")?;
    if payload.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        return Err(anyhow!(
            "slack chat.postMessage failed: {}",
            payload
                .get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
        ));
    }

    Ok(())
}

async fn dispatch_whatsapp_message(
    client: &reqwest::Client,
    channel_id: &str,
    text: &str,
) -> Result<()> {
    let jid = parse_whatsapp_chat_jid(channel_id)?;
    let url = resolve_whatsapp_deliver_url();
    let body = truncate_for_whatsapp(text);

    let response = client
        .post(&url)
        .json(&json!({
            "channel_id": format!("whatsapp:chat:{jid}"),
            "text": body,
        }))
        .send()
        .await
        .with_context(|| format!("whatsapp deliver request failed ({url})"))?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "whatsapp deliver endpoint returned {status}: {detail}"
        ));
    }

    Ok(())
}

pub fn format_heartbeat_nudge(summary: &str) -> String {
    summary.to_string()
}

/// Push configured heartbeat nudges through the same channel dispatch path as outbox delivery.
pub async fn dispatch_configured_heartbeat_nudges(
    client: &reqwest::Client,
    config: &crate::ProductConfig,
    summary: &str,
) {
    let text = format_heartbeat_nudge(summary);

    for chat_id in &config.telegram.heartbeat_chat_ids {
        let target = ChannelDeliveryTarget {
            channel: "telegram".to_string(),
            user_id: "medousa:system:heartbeat".to_string(),
            channel_id: format!("telegram:chat:{chat_id}"),
            session_id: "medousa-heartbeat".to_string(),
            stream_id: None,
        };
        if let Err(err) = dispatch_channel_message(client, &target, &text).await {
            eprintln!("heartbeat telegram dispatch chat_id={chat_id} err={err:#}");
        }
    }

    for channel_id in &config.discord.heartbeat_channel_ids {
        let target = ChannelDeliveryTarget {
            channel: "discord".to_string(),
            user_id: "medousa:system:heartbeat".to_string(),
            channel_id: format!("discord:channel:{channel_id}"),
            session_id: "medousa-heartbeat".to_string(),
            stream_id: None,
        };
        if let Err(err) = dispatch_channel_message(client, &target, &text).await {
            eprintln!("heartbeat discord dispatch channel_id={channel_id} err={err:#}");
        }
    }

    for channel_id in &config.slack.heartbeat_channel_ids {
        let target = ChannelDeliveryTarget {
            channel: "slack".to_string(),
            user_id: "medousa:system:heartbeat".to_string(),
            channel_id: format!("slack:channel:{channel_id}"),
            session_id: "medousa-heartbeat".to_string(),
            stream_id: None,
        };
        if let Err(err) = dispatch_channel_message(client, &target, &text).await {
            eprintln!("heartbeat slack dispatch channel_id={channel_id} err={err:#}");
        }
    }

    for chat_jid in &config.whatsapp.heartbeat_chat_jids {
        let target = ChannelDeliveryTarget {
            channel: "whatsapp".to_string(),
            user_id: "medousa:system:heartbeat".to_string(),
            channel_id: format!("whatsapp:chat:{chat_jid}"),
            session_id: "medousa-heartbeat".to_string(),
            stream_id: None,
        };
        if let Err(err) = dispatch_channel_message(client, &target, &text).await {
            eprintln!("heartbeat whatsapp dispatch chat_jid={chat_jid} err={err:#}");
        }
    }
}

pub fn extract_output_text_from_diagnostics(diagnostics_raw: Option<&str>) -> Option<String> {
    let diagnostics_raw = diagnostics_raw?.trim();
    if diagnostics_raw.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_str(diagnostics_raw).ok()?;
    find_output_text(&parsed)
}

fn find_output_text(payload: &Value) -> Option<String> {
    const ROOT_KEYS: [&str; 9] = [
        "output_text",
        "final_output_text",
        "response_text",
        "assistant_message",
        "final_text",
        "answer",
        "content",
        "text",
        "transcript_preview",
    ];

    for key in ROOT_KEYS {
        if let Some(text) = read_non_empty_text(payload.get(key)) {
            return Some(text);
        }
    }

    if let Some(transcript) = payload.get("transcript").and_then(|value| value.as_array()) {
        for entry in transcript.iter().rev() {
            if let Some(text) = read_non_empty_text(Some(entry)) {
                return Some(text);
            }
        }
    }

    for key in ["result", "response", "output", "final", "completion"] {
        let Some(section) = payload.get(key) else {
            continue;
        };

        for nested_key in ROOT_KEYS {
            if let Some(text) = read_non_empty_text(section.get(nested_key)) {
                return Some(text);
            }
        }
    }

    if let Some(choices) = payload.get("choices").and_then(|value| value.as_array()) {
        for choice in choices.iter().rev() {
            if let Some(text) = read_non_empty_text(choice.get("text")) {
                return Some(text);
            }
            if let Some(text) = read_non_empty_text(
                choice
                    .get("message")
                    .and_then(|message| message.get("content")),
            ) {
                return Some(text);
            }
        }
    }

    if let Some(messages) = payload.get("messages").and_then(|value| value.as_array()) {
        for message in messages.iter().rev() {
            let role = message
                .get("role")
                .and_then(|value| value.as_str())
                .map(|value| value.to_ascii_lowercase());
            if (role.as_deref() == Some("assistant") || role.is_none())
                && let Some(text) = read_non_empty_text(message.get("content")) {
                    return Some(text);
                }
        }
    }

    read_non_empty_text(Some(payload))
}

fn read_non_empty_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        delivery_target_from_interactive_turn, extract_output_text_from_diagnostics,
        format_for_telegram_markdown_v2, internal_deliver_webhook_url, is_home_channel,
        is_missing_runtime_table_error, normalize_channel_surface, parse_slack_channel_id,
        parse_telegram_chat_id, truncate_for_slack, truncate_for_telegram,
    };
    use crate::daemon_api::{InteractiveTurnRequest, TurnSurfaceContext};
    use crate::stage_routing::StageRoutingMatrix;

    #[test]
    fn missing_runtime_table_error_detection() {
        assert!(is_missing_runtime_table_error(
            "decode delivery endpoint: The table 'delivery_endpoint' does not exist"
        ));
    }

    #[test]
    fn internal_webhook_url_uses_loopback_for_wildcard_bind() {
        assert_eq!(
            internal_deliver_webhook_url("0.0.0.0:7419"),
            "http://127.0.0.1:7419/v1/deliver/outbox"
        );
    }

    #[test]
    fn parse_telegram_chat_id_from_channel_id() {
        assert_eq!(
            parse_telegram_chat_id("telegram:chat:12345").unwrap(),
            12345
        );
    }

    #[test]
    fn truncate_for_telegram_caps_length() {
        let long = "x".repeat(5000);
        assert_eq!(truncate_for_telegram(&long).chars().count(), 4003);
    }

    #[test]
    fn telegram_markdown_escape_preserves_bold_markers() {
        let escaped = format_for_telegram_markdown_v2("*hello* _world_");
        assert!(escaped.contains('*'));
        assert!(escaped.contains('_'));
    }

    #[test]
    fn parse_slack_channel_id_from_channel_id() {
        assert_eq!(
            parse_slack_channel_id("slack:channel:C123").unwrap(),
            "C123"
        );
    }

    #[test]
    fn truncate_for_slack_caps_length() {
        let long = "x".repeat(5000);
        assert_eq!(truncate_for_slack(&long).chars().count(), 3903);
    }

    #[test]
    fn extract_output_text_from_diagnostics_json() {
        let diagnostics = r#"{"output_text":"hello world"}"#;
        assert_eq!(
            extract_output_text_from_diagnostics(Some(diagnostics)).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn normalize_legacy_home_surface() {
        assert_eq!(normalize_channel_surface("home"), "home-desktop");
        assert_eq!(normalize_channel_surface("home-ios"), "home-ios");
    }

    #[test]
    fn interactive_turn_resolves_home_ios_delivery_target() {
        let request = InteractiveTurnRequest {
            session_id: "sess-1".to_string(),
            prompt: "hi".to_string(),
            persist_user_turn: true,
            response_depth_mode: "standard".to_string(),
            reasoning_effort: crate::reasoning_effort::REASONING_EFFORT_DEFAULT.to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            stage_routing: StageRoutingMatrix::default_for("openai", "gpt-4o-mini"),
            surface: Some(TurnSurfaceContext {
                channel_surface: Some("home-ios".to_string()),
                channel_id: None,
                user_id: None,
                supports_ui_artifacts: true,
                supports_browser_host: true,
            }),
            max_tool_rounds: None,
            retry_runtime_max_rounds: None,
            manuscript_id: None,
            additional_manuscript_ids: None,
            suggested_capability_ids: None,
            scheduled_tool_allowlist: None,
            voice_preset_id: None,
            voice_appendix: None,
            media_refs: Vec::new(),
            identity_user_id: None,
        };
        let target = delivery_target_from_interactive_turn(&request, "turn-1");
        assert_eq!(target.channel, "home-ios");
        assert_eq!(target.channel_id, "sess-1");
        assert!(is_home_channel(&target.channel));
    }
}
