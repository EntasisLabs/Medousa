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

use crate::session::{load_discord_bot_token, load_telegram_bot_token};

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
            if !(text.contains("already exists")
                || text.contains("already defined")
                || text.contains("Overwrite index"))
            {
                return Err(anyhow!("delivery schema bootstrap ({statement}): {text}"));
            }
        }
    }

    Ok(())
}

pub const INTERNAL_OUTBOX_ENDPOINT_ID: &str = "medousa.internal.outbox";
pub const INTERNAL_OUTBOX_DELIVER_PATH: &str = "/v1/deliver/outbox";

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
    let existing = match store.get(INTERNAL_OUTBOX_ENDPOINT_ID).await {
        Ok(record) => record,
        Err(err) if is_missing_runtime_table_error(&err.to_string()) => None,
        Err(err) => {
            return Err(err).context("failed to read internal outbox delivery endpoint");
        }
    };

    if existing.is_some() {
        return Ok(());
    }

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
        .context("failed to seed internal outbox delivery endpoint")?;

    Ok(())
}

pub async fn seed_internal_outbox_endpoint_for_runtime(
    runtime: &RuntimeComposition,
    in_memory_endpoint_store: Option<Arc<dyn DeliveryEndpointStore>>,
    target_url: &str,
) -> Result<()> {
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
                ensure_surreal_delivery_schema(&db).await?;
                Arc::new(SurrealDeliveryEndpointStore::new(db))
            }
        },
    };

    seed_internal_outbox_endpoint(store.as_ref(), target_url).await
}

pub fn truncate_for_telegram(text: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }

    let truncated = text.chars().take(MAX_CHARS).collect::<String>();
    format!("{truncated}...")
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

pub fn parse_discord_channel_id(channel_id: &str) -> Result<u64> {
    channel_id
        .strip_prefix("discord:channel:")
        .or_else(|| channel_id.strip_prefix("discord:"))
        .context("discord channel_id must be discord:channel:<id>")?
        .parse::<u64>()
        .context("discord channel id must be numeric")
}

pub async fn dispatch_channel_message(
    client: &reqwest::Client,
    target: &ChannelDeliveryTarget,
    text: &str,
) -> Result<()> {
    match target.channel.as_str() {
        "telegram" => dispatch_telegram_message(client, &target.channel_id, text).await,
        "discord" => dispatch_discord_message(client, &target.channel_id, text).await,
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
    let body = truncate_for_telegram(text);
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");

    let response = client
        .post(&url)
        .json(&json!({
            "chat_id": chat_id,
            "text": body,
        }))
        .send()
        .await
        .context("telegram sendMessage request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "telegram sendMessage returned {status}: {detail}"
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
    const ROOT_KEYS: [&str; 8] = [
        "output_text",
        "final_output_text",
        "response_text",
        "assistant_message",
        "final_text",
        "answer",
        "content",
        "text",
    ];

    for key in ROOT_KEYS {
        if let Some(text) = read_non_empty_text(payload.get(key)) {
            return Some(text);
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
            if role.as_deref() == Some("assistant") || role.is_none() {
                if let Some(text) = read_non_empty_text(message.get("content")) {
                    return Some(text);
                }
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
        extract_output_text_from_diagnostics, internal_deliver_webhook_url,
        is_missing_runtime_table_error, parse_telegram_chat_id, truncate_for_telegram,
    };

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
    fn extract_output_text_from_diagnostics_json() {
        let diagnostics = r#"{"output_text":"hello world"}"#;
        assert_eq!(
            extract_output_text_from_diagnostics(Some(diagnostics)).as_deref(),
            Some("hello world")
        );
    }
}
