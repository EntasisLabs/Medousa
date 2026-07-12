//! Bind recurring definition ids to channel delivery targets for outbox push.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use stasis::ports::outbound::runtime::job_store::JobStore;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::sync::RwLock as AsyncRwLock;

use crate::channel_delivery::ChannelDeliveryTarget;
use crate::channel_session_store::{self, parse_channel_mapping_key};
use crate::product_config::{self, ProductConfig};
use crate::turn_continuation::{StoredDeliveryTarget, TurnContinuationScope};

const TABLE: &str = "recurring_delivery_binding";

const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE recurring_delivery_binding SCHEMAFULL",
    "DEFINE FIELD recurring_id ON TABLE recurring_delivery_binding TYPE string",
    "DEFINE FIELD channel ON TABLE recurring_delivery_binding TYPE string",
    "DEFINE FIELD user_id ON TABLE recurring_delivery_binding TYPE string",
    "DEFINE FIELD channel_id ON TABLE recurring_delivery_binding TYPE string",
    "DEFINE FIELD session_id ON TABLE recurring_delivery_binding TYPE string",
    "DEFINE FIELD stream_id ON TABLE recurring_delivery_binding TYPE option<string>",
    "DEFINE FIELD created_at ON TABLE recurring_delivery_binding TYPE datetime",
    "DEFINE FIELD updated_at ON TABLE recurring_delivery_binding TYPE datetime",
    "DEFINE INDEX idx_recurring_delivery_id ON TABLE recurring_delivery_binding COLUMNS recurring_id UNIQUE",
];

const MIN_SCHEDULE_INTERVAL_SECS: i64 = 60;
const CRON_FORMAT_HINT: &str = "sec min hour day-of-month month day-of-week year (example every 4h: 0 0 */4 * * * *)";

static RECURRING_DELIVERY_STORE: Lazy<RwLock<Arc<dyn RecurringDeliveryStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(InMemoryRecurringDeliveryStore::default())));

pub fn recurring_delivery_store() -> Arc<dyn RecurringDeliveryStore> {
    RECURRING_DELIVERY_STORE.read().unwrap().clone()
}

pub fn set_recurring_delivery_store(store: Arc<dyn RecurringDeliveryStore>) {
    let mut guard = RECURRING_DELIVERY_STORE.write().unwrap();
    *guard = store;
}

pub async fn init_recurring_delivery_store_with_runtime(runtime: &RuntimeComposition) {
    if let RuntimeComposition::Surreal(rt) = runtime {
        let store = SurrealRecurringDeliveryStore::new(rt.job_store.db());
        if let Err(err) = store.ensure_schema().await {
            eprintln!(
                "Surreal recurring delivery store schema init error: {err}; keeping in-memory store"
            );
            return;
        }
        set_recurring_delivery_store(Arc::new(store));
        eprintln!(
            "Surreal runtime detected; recurring delivery store switched to SurrealDB backend"
        );
    }
}

#[derive(Debug, Clone)]
pub struct DeliveryResolveContext<'a> {
    pub ambient: Option<&'a ChannelDeliveryTarget>,
    pub fallback_session_id: String,
}

/// Delivery target from an active agent turn (ingest / daemon interactive), if any.
pub fn ambient_from_turn_scope(scope: Option<&TurnContinuationScope>) -> Option<ChannelDeliveryTarget> {
    scope
        .and_then(|turn| turn.delivery_target.as_ref())
        .cloned()
}

/// Validate cron, parse optional `delivery`, and persist binding for `recurring_id`.
pub async fn bind_recurring_delivery_for_registration(
    recurring_id: &str,
    cron_expr: &str,
    timezone: &str,
    input: &Value,
    ctx: DeliveryResolveContext<'_>,
) -> StasisResult<(bool, Option<ChannelDeliveryTarget>)> {
    validate_recurring_cron(cron_expr, timezone)?;
    let bound = persist_recurring_delivery_binding(recurring_id, input, ctx).await?;
    Ok((bound.is_some(), bound))
}

/// Validate cron and ensure the first two scheduled firings are not sub-minute.
pub fn validate_recurring_cron(cron_expr: &str, timezone: &str) -> StasisResult<()> {
    let definition = RecurringDefinition {
        id: "cron-validation".to_string(),
        queue: "default".to_string(),
        job_type: "workflow.stasis.prompt".to_string(),
        payload_template_ref: "validation".to_string(),
        cron_expr: cron_expr.to_string(),
        timezone: timezone.to_string(),
        jitter_seconds: 0,
        enabled: true,
        max_attempts: 1,
        next_run_at: Utc::now(),
        last_run_at: None,
        lease_owner: None,
        lease_expires_at: None,
    };

    let first = definition.compute_next_run_at(Utc::now())?;
    let second = definition.compute_next_run_at(first + chrono::Duration::seconds(1))?;
    let delta = second.signed_duration_since(first).num_seconds();
    if delta < MIN_SCHEDULE_INTERVAL_SECS {
        return Err(StasisError::PortFailure(format!(
            "cron schedule fires too frequently (interval={delta}s); minimum is {MIN_SCHEDULE_INTERVAL_SECS}s. Use 7-field cron: {CRON_FORMAT_HINT}"
        )));
    }

    Ok(())
}

/// Parse optional `delivery` from tool/API JSON and upsert binding for `recurring_id`.
pub async fn remove_recurring_delivery_binding(recurring_id: &str) -> anyhow::Result<()> {
    recurring_delivery_store().remove(recurring_id).await
}

pub async fn persist_recurring_delivery_binding(
    recurring_id: &str,
    input: &Value,
    ctx: DeliveryResolveContext<'_>,
) -> StasisResult<Option<ChannelDeliveryTarget>> {
    let Some(delivery_value) = input
        .get("delivery")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };

    let target = parse_delivery_spec(delivery_value, ctx).await?;
    recurring_delivery_store()
        .upsert(recurring_id, &target)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

    Ok(Some(target))
}

pub async fn parse_delivery_spec(
    value: &Value,
    ctx: DeliveryResolveContext<'_>,
) -> StasisResult<ChannelDeliveryTarget> {
    let config = product_config::load_product_config();

    let mode = value
        .get("mode")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("explicit")
        .to_ascii_lowercase();

    match mode.as_str() {
        "current_channel" => ctx.ambient.cloned().ok_or_else(|| {
            StasisError::PortFailure(
                "delivery.mode=current_channel requires an active channel context; \
                 provide explicit delivery (channel + chat/channel id) instead"
                    .to_string(),
            )
        }),
        "product_default" => {
            resolve_product_default_delivery(value, &config, &ctx.fallback_session_id)
        }
        "linked_channel" => resolve_linked_channel_delivery(value, &config, &ctx).await,
        "explicit" | "" => resolve_explicit_delivery(value, &config, &ctx.fallback_session_id),
        other => Err(StasisError::PortFailure(format!(
            "unsupported delivery.mode={other}; use explicit, current_channel, linked_channel, or product_default"
        ))),
    }
}

fn resolve_explicit_delivery(
    value: &Value,
    config: &ProductConfig,
    fallback_session_id: &str,
) -> StasisResult<ChannelDeliveryTarget> {
    let channel = required_string_field(value, &["channel"], "delivery.channel")?
        .to_ascii_lowercase();

    let channel_id = resolve_channel_id(&channel, value)?;
    let user_id = optional_string_field(value, &["user_id"])
        .unwrap_or_else(|| default_user_id_for_channel(&channel));
    let session_id = optional_string_field(value, &["session_id"])
        .unwrap_or_else(|| fallback_session_id.to_string());

    let target = ChannelDeliveryTarget {
        channel: channel.clone(),
        user_id,
        channel_id,
        session_id,
        stream_id: None,
    };

    enforce_delivery_policy(&target, config)?;
    Ok(target)
}

async fn resolve_linked_channel_delivery(
    value: &Value,
    config: &ProductConfig,
    ctx: &DeliveryResolveContext<'_>,
) -> StasisResult<ChannelDeliveryTarget> {
    let channel = value
        .get("channel")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| {
            StasisError::PortFailure(
                "delivery.mode=linked_channel requires delivery.channel (e.g. telegram)".to_string(),
            )
        })?;

    let session_id = optional_string_field(value, &["session_id"])
        .unwrap_or_else(|| ctx.fallback_session_id.clone());

    let mapping_key = channel_session_store::channel_session_store()
        .find_mapping_key_for_session(&channel, &session_id)
        .await
        .ok_or_else(|| {
            StasisError::PortFailure(format!(
                "delivery.mode=linked_channel: no {channel} ingest mapping for session_id={session_id}; \
                 message that channel first or use explicit telegram_chat_id"
            ))
        })?;

    let (_, channel_id, user_id) = parse_channel_mapping_key(&mapping_key).ok_or_else(|| {
        StasisError::PortFailure(format!(
            "delivery.mode=linked_channel: invalid mapping_key={mapping_key}"
        ))
    })?;

    let target = ChannelDeliveryTarget {
        channel: channel.clone(),
        user_id,
        channel_id,
        session_id,
        stream_id: None,
    };

    enforce_delivery_policy(&target, config)?;
    Ok(target)
}

fn resolve_product_default_delivery(
    value: &Value,
    config: &ProductConfig,
    fallback_session_id: &str,
) -> StasisResult<ChannelDeliveryTarget> {
    let channel = value
        .get("channel")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| {
            StasisError::PortFailure(
                "delivery.mode=product_default requires delivery.channel".to_string(),
            )
        })?;

    let channel_id = match channel.as_str() {
        "telegram" => config
            .telegram
            .heartbeat_chat_ids
            .first()
            .map(|id| format!("telegram:chat:{id}"))
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "delivery.mode=product_default: configure telegram.heartbeat_chat_ids in product config"
                        .to_string(),
                )
            })?,
        "discord" => config
            .discord
            .heartbeat_channel_ids
            .first()
            .map(|id| format!("discord:channel:{id}"))
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "delivery.mode=product_default: configure discord.heartbeat_channel_ids"
                        .to_string(),
                )
            })?,
        "slack" => config
            .slack
            .heartbeat_channel_ids
            .first()
            .map(|id| format!("slack:channel:{id}"))
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "delivery.mode=product_default: configure slack.heartbeat_channel_ids"
                        .to_string(),
                )
            })?,
        "whatsapp" => config
            .whatsapp
            .heartbeat_chat_jids
            .first()
            .map(|jid| format!("whatsapp:chat:{jid}"))
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "delivery.mode=product_default: configure whatsapp.heartbeat_chat_jids"
                        .to_string(),
                )
            })?,
        "cli" => {
            return Ok(ChannelDeliveryTarget {
                channel: "cli".to_string(),
                user_id: "cli:user:default".to_string(),
                channel_id: "cli:session:default".to_string(),
                session_id: fallback_session_id.to_string(),
                stream_id: None,
            });
        }
        other => {
            return Err(StasisError::PortFailure(format!(
                "delivery.mode=product_default: unsupported channel={other}"
            )));
        }
    };

    let target = ChannelDeliveryTarget {
        channel: channel.clone(),
        user_id: default_user_id_for_channel(&channel),
        channel_id,
        session_id: fallback_session_id.to_string(),
        stream_id: None,
    };

    enforce_delivery_policy(&target, config)?;
    Ok(target)
}

fn resolve_channel_id(channel: &str, value: &Value) -> StasisResult<String> {
    if let Some(id) = optional_string_field(value, &["channel_id"]) {
        return Ok(id);
    }

    match channel {
        "telegram" => optional_string_field(value, &["telegram_chat_id"]).map(|id| {
            if id.starts_with("telegram:chat:") {
                id
            } else {
                format!("telegram:chat:{id}")
            }
        }),
        "discord" => optional_string_field(value, &["discord_channel_id"]).map(|id| {
            if id.starts_with("discord:channel:") {
                id
            } else {
                format!("discord:channel:{id}")
            }
        }),
        "slack" => optional_string_field(value, &["slack_channel_id"]).map(|id| {
            if id.starts_with("slack:channel:") {
                id
            } else {
                format!("slack:channel:{id}")
            }
        }),
        "whatsapp" => optional_string_field(value, &["whatsapp_chat_jid", "whatsapp_chat_id"])
            .map(|id| {
                if id.starts_with("whatsapp:chat:") {
                    id
                } else {
                    format!("whatsapp:chat:{id}")
                }
            }),
        "cli" => Some("cli:session:default".to_string()),
        _ => None,
    }
    .ok_or_else(|| {
        StasisError::PortFailure(format!(
            "delivery for channel={channel} requires channel_id or channel-specific id field \
             (e.g. telegram_chat_id, discord_channel_id)"
        ))
    })
}

fn default_user_id_for_channel(channel: &str) -> String {
    match channel {
        "telegram" => "telegram:user:recurring".to_string(),
        "discord" => "discord:user:recurring".to_string(),
        "slack" => "slack:user:recurring".to_string(),
        "whatsapp" => "whatsapp:user:recurring".to_string(),
        "cli" => "cli:user:recurring".to_string(),
        other => format!("{other}:user:recurring"),
    }
}

fn enforce_delivery_policy(target: &ChannelDeliveryTarget, config: &ProductConfig) -> StasisResult<()> {
    if target.channel == "cli" {
        return Ok(());
    }

    if !product_config::ingest_sender_allowed(&target.channel, &target.user_id, config) {
        // For telegram with only chat id, also allow heartbeat-configured chats.
        if target.channel == "telegram"
            && let Some(chat_id) = parse_telegram_chat_numeric(&target.channel_id)
                && config.telegram.heartbeat_chat_ids.contains(&chat_id) {
                    return Ok(());
                }

        if heartbeat_channel_allowed(target, config) {
            return Ok(());
        }

        return Err(StasisError::PortFailure(format!(
            "delivery target not allowed by product policy: channel={} channel_id={}",
            target.channel, target.channel_id
        )));
    }

    Ok(())
}

fn heartbeat_channel_allowed(target: &ChannelDeliveryTarget, config: &ProductConfig) -> bool {
    match target.channel.as_str() {
        "telegram" => parse_telegram_chat_numeric(&target.channel_id)
            .map(|id| config.telegram.heartbeat_chat_ids.contains(&id))
            .unwrap_or(false),
        "discord" => parse_discord_channel_numeric(&target.channel_id)
            .map(|id| config.discord.heartbeat_channel_ids.contains(&id))
            .unwrap_or(false),
        "slack" => config
            .slack
            .heartbeat_channel_ids
            .iter()
            .any(|id| target.channel_id.contains(id.as_str())),
        "whatsapp" => config.whatsapp.heartbeat_chat_jids.iter().any(|jid| {
            target.channel_id.contains(jid) || jid == &target.channel_id
        }),
        _ => false,
    }
}

fn parse_telegram_chat_numeric(channel_id: &str) -> Option<i64> {
    channel_id
        .strip_prefix("telegram:chat:")
        .and_then(|value| value.parse::<i64>().ok())
}

fn parse_discord_channel_numeric(channel_id: &str) -> Option<u64> {
    channel_id
        .strip_prefix("discord:channel:")
        .and_then(|value| value.parse::<u64>().ok())
}

fn required_string_field(
    value: &Value,
    keys: &[&str],
    label: &str,
) -> StasisResult<String> {
    for key in keys {
        if let Some(found) = optional_string_field(value, &[*key]) {
            return Ok(found);
        }
    }
    Err(StasisError::PortFailure(format!("{label} is required")))
}

fn optional_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

pub async fn job_correlation_id(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Option<String> {
    let job = match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await,
    };
    job.ok().flatten().map(|job| job.correlation_id)
}

/// Resolve delivery target for outbox push: per-job registry first, then recurring binding.
pub async fn resolve_delivery_target_for_job(
    runtime: &RuntimeComposition,
    job_id: &str,
    per_job_targets: &std::collections::HashMap<String, ChannelDeliveryTarget>,
) -> Option<ChannelDeliveryTarget> {
    if let Some(target) = per_job_targets.get(job_id) {
        return Some(target.clone());
    }

    let correlation_id = job_correlation_id(runtime, job_id).await?;
    let stored = recurring_delivery_store().get(&correlation_id).await.ok()??;
    Some(ChannelDeliveryTarget::from(&stored))
}

pub fn delivery_binding_to_json(target: &StoredDeliveryTarget) -> Value {
    serde_json::json!({
        "channel": target.channel,
        "channel_id": target.channel_id,
        "user_id": target.user_id,
        "session_id": target.session_id,
        "stream_id": target.stream_id,
    })
}

pub async fn delivery_binding_for_recurring(recurring_id: &str) -> Option<StoredDeliveryTarget> {
    recurring_delivery_store()
        .get(recurring_id)
        .await
        .ok()
        .flatten()
}

pub fn delivery_spec_schema_fragment() -> Value {
    serde_json::json!({
        "delivery": {
            "type": "object",
            "description": "Where to push each successful run (independent of current UI channel). 7-field cron required separately.",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["explicit", "current_channel", "linked_channel", "product_default"],
                    "default": "explicit"
                },
                "channel": { "type": "string", "description": "telegram | discord | slack | whatsapp | cli" },
                "channel_id": { "type": "string", "description": "Canonical id, e.g. telegram:chat:123" },
                "telegram_chat_id": { "type": "string" },
                "discord_channel_id": { "type": "string" },
                "slack_channel_id": { "type": "string" },
                "whatsapp_chat_jid": { "type": "string" },
                "user_id": { "type": "string" },
                "session_id": { "type": "string", "description": "Medousa session for job context" }
            }
        }
    })
}

#[async_trait]
pub trait RecurringDeliveryStore: Send + Sync {
    async fn upsert(&self, recurring_id: &str, target: &ChannelDeliveryTarget) -> anyhow::Result<()>;
    async fn get(&self, recurring_id: &str) -> anyhow::Result<Option<StoredDeliveryTarget>>;
    async fn remove(&self, recurring_id: &str) -> anyhow::Result<()>;
    async fn count(&self) -> anyhow::Result<usize>;
}

#[derive(Default)]
struct InMemoryRecurringDeliveryStore {
    bindings: AsyncRwLock<std::collections::HashMap<String, StoredDeliveryTarget>>,
}

#[async_trait]
impl RecurringDeliveryStore for InMemoryRecurringDeliveryStore {
    async fn upsert(&self, recurring_id: &str, target: &ChannelDeliveryTarget) -> anyhow::Result<()> {
        self.bindings
            .write()
            .await
            .insert(recurring_id.to_string(), StoredDeliveryTarget {
                channel: target.channel.clone(),
                user_id: target.user_id.clone(),
                channel_id: target.channel_id.clone(),
                session_id: target.session_id.clone(),
                stream_id: target.stream_id.clone(),
            });
        Ok(())
    }

    async fn get(&self, recurring_id: &str) -> anyhow::Result<Option<StoredDeliveryTarget>> {
        Ok(self.bindings.read().await.get(recurring_id).cloned())
    }

    async fn remove(&self, recurring_id: &str) -> anyhow::Result<()> {
        self.bindings.write().await.remove(recurring_id);
        Ok(())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.bindings.read().await.len())
    }
}

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
struct RecurringDeliveryRecord {
    recurring_id: String,
    channel: String,
    user_id: String,
    channel_id: String,
    session_id: String,
    stream_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
struct SurrealRecurringDeliveryStore {
    db: Surreal<Any>,
}

impl SurrealRecurringDeliveryStore {
    fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    fn record_id(recurring_id: &str) -> String {
        recurring_id.replace(':', "_")
    }

    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        for statement in SCHEMA_STATEMENTS {
            if let Err(err) = self.db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    return Err(err);
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl RecurringDeliveryStore for SurrealRecurringDeliveryStore {
    async fn upsert(&self, recurring_id: &str, target: &ChannelDeliveryTarget) -> anyhow::Result<()> {
        let now = Utc::now();
        let record = RecurringDeliveryRecord {
            recurring_id: recurring_id.to_string(),
            channel: target.channel.clone(),
            user_id: target.user_id.clone(),
            channel_id: target.channel_id.clone(),
            session_id: target.session_id.clone(),
            stream_id: target.stream_id.clone(),
            created_at: now,
            updated_at: now,
        };
        let id = Self::record_id(recurring_id);
        self.db
            .query("UPSERT type::record($table, $id) CONTENT $data")
            .bind(("table", TABLE))
            .bind(("id", id))
            .bind(("data", record))
            .await?;
        Ok(())
    }

    async fn get(&self, recurring_id: &str) -> anyhow::Result<Option<StoredDeliveryTarget>> {
        let id = Self::record_id(recurring_id);
        let mut response = self
            .db
            .query("SELECT * FROM type::record($table, $id)")
            .bind(("table", TABLE))
            .bind(("id", id))
            .await?;

        let record: Option<RecurringDeliveryRecord> = response.take(0)?;
        Ok(record.map(|row| StoredDeliveryTarget {
            channel: row.channel,
            user_id: row.user_id,
            channel_id: row.channel_id,
            session_id: row.session_id,
            stream_id: row.stream_id,
        }))
    }

    async fn remove(&self, recurring_id: &str) -> anyhow::Result<()> {
        let id = Self::record_id(recurring_id);
        self.db
            .query("DELETE type::record($table, $id)")
            .bind(("table", TABLE))
            .bind(("id", id))
            .await?;
        Ok(())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let mut response = self
            .db
            .query("SELECT count() FROM type::table($table) GROUP ALL")
            .bind(("table", TABLE))
            .await?;
        let rows: Vec<Value> = response.take(0)?;
        let count = rows
            .first()
            .and_then(|row| row.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_telegram_delivery_normalizes_chat_id() {
        let config = ProductConfig::default();
        let target = resolve_explicit_delivery(
            &json!({
                "channel": "telegram",
                "telegram_chat_id": "999"
            }),
            &config,
            "recurring-test",
        )
        .expect("telegram delivery");

        assert_eq!(target.channel_id, "telegram:chat:999");
    }

    #[test]
    fn cron_rejects_subminute_schedule() {
        let err = validate_recurring_cron("0/1 * * * * * *", "UTC").unwrap_err();
        assert!(err.to_string().contains("too frequently"));
    }

    #[test]
    fn cron_accepts_four_hour_schedule() {
        validate_recurring_cron("0 0 */4 * * * *", "UTC").expect("valid 4h cron");
    }

    #[test]
    fn channel_mapping_key_roundtrip() {
        use crate::channel_session_store::{channel_mapping_key, parse_channel_mapping_key};

        let key = channel_mapping_key("telegram", "telegram:chat:42", "telegram:user:99");
        let (channel, channel_id, user_id) =
            parse_channel_mapping_key(&key).expect("parse mapping key");
        assert_eq!(channel, "telegram");
        assert_eq!(channel_id, "telegram:chat:42");
        assert_eq!(user_id, "telegram:user:99");
    }
}
