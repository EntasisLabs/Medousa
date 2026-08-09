//! Bind recurring definition ids to channel delivery targets for outbox push.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;
use tokio::sync::RwLock as AsyncRwLock;

use crate::channel_delivery::ChannelDeliveryTarget;
use crate::channel_session_store::{self, parse_channel_mapping_key};
use crate::product_config::{self, ProductConfig};
use crate::recurring_schedule::RecurringScheduleSpec;
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::turn_continuation::{StoredDeliveryTarget, TurnContinuationScope};
use crate::typed_tools::CompatOption;

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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecurringDeliveryMode {
    #[default]
    Explicit,
    CurrentChannel,
    LinkedChannel,
    ProductDefault,
}

impl RecurringDeliveryMode {
    fn parse(raw: Option<&str>) -> StasisResult<Self> {
        match raw
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("explicit")
            .to_ascii_lowercase()
            .as_str()
        {
            "explicit" => Ok(Self::Explicit),
            "current_channel" => Ok(Self::CurrentChannel),
            "linked_channel" => Ok(Self::LinkedChannel),
            "product_default" => Ok(Self::ProductDefault),
            other => Err(StasisError::PortFailure(format!(
                "unsupported delivery.mode={other}; use explicit, current_channel, linked_channel, or product_default"
            ))),
        }
    }
}

/// Typed model-visible delivery binding used by recurring tool contracts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecurringDeliverySpec {
    #[serde(default)]
    #[schemars(default)]
    pub mode: RecurringDeliveryMode,
    /// telegram | discord | slack | whatsapp | cli
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Canonical id, e.g. telegram:chat:123
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub telegram_chat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub discord_channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub slack_channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub whatsapp_chat_jid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub whatsapp_chat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Medousa session for job context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl RecurringDeliverySpec {
    pub async fn try_into_target(
        &self,
        ctx: &DeliveryResolveContext<'_>,
    ) -> StasisResult<ChannelDeliveryTarget> {
        let config = product_config::load_product_config();
        match self.mode {
            RecurringDeliveryMode::CurrentChannel => ctx.ambient.cloned().ok_or_else(|| {
                StasisError::PortFailure(
                    "delivery.mode=current_channel requires an active channel context; \
                     provide explicit delivery (channel + chat/channel id) instead"
                        .to_string(),
                )
            }),
            RecurringDeliveryMode::ProductDefault => {
                resolve_product_default_delivery(self, &config, &ctx.fallback_session_id)
            }
            RecurringDeliveryMode::LinkedChannel => {
                resolve_linked_channel_delivery(self, &config, ctx).await
            }
            RecurringDeliveryMode::Explicit => {
                resolve_explicit_delivery(self, &config, &ctx.fallback_session_id)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct LegacyRecurringDeliverySpec {
    #[serde(default)]
    mode: CompatOption<String>,
    #[serde(default)]
    channel: CompatOption<String>,
    #[serde(default)]
    channel_id: CompatOption<String>,
    #[serde(default)]
    telegram_chat_id: CompatOption<String>,
    #[serde(default)]
    discord_channel_id: CompatOption<String>,
    #[serde(default)]
    slack_channel_id: CompatOption<String>,
    #[serde(default)]
    whatsapp_chat_jid: CompatOption<String>,
    #[serde(default)]
    whatsapp_chat_id: CompatOption<String>,
    #[serde(default)]
    user_id: CompatOption<String>,
    #[serde(default)]
    session_id: CompatOption<String>,
}

impl TryFrom<LegacyRecurringDeliverySpec> for RecurringDeliverySpec {
    type Error = StasisError;

    fn try_from(value: LegacyRecurringDeliverySpec) -> Result<Self, Self::Error> {
        Ok(Self {
            mode: RecurringDeliveryMode::parse(value.mode.as_ref().map(String::as_str))?,
            channel: value.channel.into_option(),
            channel_id: value.channel_id.into_option(),
            telegram_chat_id: value.telegram_chat_id.into_option(),
            discord_channel_id: value.discord_channel_id.into_option(),
            slack_channel_id: value.slack_channel_id.into_option(),
            whatsapp_chat_jid: value.whatsapp_chat_jid.into_option(),
            whatsapp_chat_id: value.whatsapp_chat_id.into_option(),
            user_id: value.user_id.into_option(),
            session_id: value.session_id.into_option(),
        })
    }
}

/// Delivery target from an active agent turn (ingest / daemon interactive), if any.
pub fn ambient_from_turn_scope(
    scope: Option<&TurnContinuationScope>,
) -> Option<ChannelDeliveryTarget> {
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

pub async fn bind_recurring_delivery_spec_for_registration(
    recurring_id: &str,
    cron_expr: &str,
    timezone: &str,
    delivery: Option<&RecurringDeliverySpec>,
    ctx: DeliveryResolveContext<'_>,
) -> StasisResult<(bool, Option<ChannelDeliveryTarget>)> {
    validate_recurring_cron(cron_expr, timezone)?;
    let Some(delivery) = delivery else {
        return Ok((false, None));
    };
    let target = delivery.try_into_target(&ctx).await?;
    recurring_delivery_store()
        .upsert(recurring_id, &target)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
    Ok((true, Some(target)))
}

/// Validate cron and ensure the first two scheduled firings are not sub-minute.
pub fn validate_recurring_cron(cron_expr: &str, timezone: &str) -> StasisResult<()> {
    RecurringScheduleSpec::new(
        "cron-validation",
        "default",
        "workflow.stasis.prompt",
        "validation",
        cron_expr,
        timezone,
    )
    .build(Utc::now())
    .map(|_| ())
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
    let Some(delivery_value) = input.get("delivery").filter(|value| !value.is_null()) else {
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
    let wire: LegacyRecurringDeliverySpec =
        serde_json::from_value(value.clone()).map_err(|error| {
            StasisError::PortFailure(format!("invalid recurring delivery spec: {error}"))
        })?;
    let spec = RecurringDeliverySpec::try_from(wire)?;
    spec.try_into_target(&ctx).await
}

fn resolve_explicit_delivery(
    spec: &RecurringDeliverySpec,
    config: &ProductConfig,
    fallback_session_id: &str,
) -> StasisResult<ChannelDeliveryTarget> {
    let channel = spec
        .channel
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| StasisError::PortFailure("delivery.channel is required".to_string()))?;

    let channel_id = resolve_channel_id(&channel, spec)?;
    let user_id = spec
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| default_user_id_for_channel(&channel));
    let session_id = spec
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
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
    spec: &RecurringDeliverySpec,
    config: &ProductConfig,
    ctx: &DeliveryResolveContext<'_>,
) -> StasisResult<ChannelDeliveryTarget> {
    let channel = spec
        .channel
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            StasisError::PortFailure(
                "delivery.mode=linked_channel requires delivery.channel (e.g. telegram)"
                    .to_string(),
            )
        })?;

    let session_id = spec
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
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
    spec: &RecurringDeliverySpec,
    config: &ProductConfig,
    fallback_session_id: &str,
) -> StasisResult<ChannelDeliveryTarget> {
    let channel = spec
        .channel
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
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

fn resolve_channel_id(channel: &str, spec: &RecurringDeliverySpec) -> StasisResult<String> {
    if let Some(id) = spec
        .channel_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    {
        return Ok(id);
    }

    match channel {
        "telegram" => spec
            .telegram_chat_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|id| {
                if id.starts_with("telegram:chat:") {
                    id.to_string()
                } else {
                    format!("telegram:chat:{id}")
                }
            }),
        "discord" => spec
            .discord_channel_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|id| {
                if id.starts_with("discord:channel:") {
                    id.to_string()
                } else {
                    format!("discord:channel:{id}")
                }
            }),
        "slack" => spec
            .slack_channel_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|id| {
                if id.starts_with("slack:channel:") {
                    id.to_string()
                } else {
                    format!("slack:channel:{id}")
                }
            }),
        "whatsapp" => spec
            .whatsapp_chat_jid
            .as_deref()
            .or(spec.whatsapp_chat_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|id| {
                if id.starts_with("whatsapp:chat:") {
                    id.to_string()
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

fn enforce_delivery_policy(
    target: &ChannelDeliveryTarget,
    config: &ProductConfig,
) -> StasisResult<()> {
    if target.channel == "cli" {
        return Ok(());
    }

    if !product_config::ingest_sender_allowed(&target.channel, &target.user_id, config) {
        // For telegram with only chat id, also allow heartbeat-configured chats.
        if target.channel == "telegram"
            && let Some(chat_id) = parse_telegram_chat_numeric(&target.channel_id)
            && config.telegram.heartbeat_chat_ids.contains(&chat_id)
        {
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
        "whatsapp" => config
            .whatsapp
            .heartbeat_chat_jids
            .iter()
            .any(|jid| target.channel_id.contains(jid) || jid == &target.channel_id),
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

pub async fn job_correlation_id(runtime: &RuntimeComposition, job_id: &str) -> Option<String> {
    runtime
        .get_job(job_id)
        .await
        .ok()
        .flatten()
        .map(|job| job.correlation_id)
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
    let stored = recurring_delivery_store()
        .get(&correlation_id)
        .await
        .ok()??;
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
    async fn upsert(
        &self,
        recurring_id: &str,
        target: &ChannelDeliveryTarget,
    ) -> anyhow::Result<()>;
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
    async fn upsert(
        &self,
        recurring_id: &str,
        target: &ChannelDeliveryTarget,
    ) -> anyhow::Result<()> {
        self.bindings.write().await.insert(
            recurring_id.to_string(),
            StoredDeliveryTarget {
                channel: target.channel.clone(),
                user_id: target.user_id.clone(),
                channel_id: target.channel_id.clone(),
                session_id: target.session_id.clone(),
                stream_id: target.stream_id.clone(),
            },
        );
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
    async fn upsert(
        &self,
        recurring_id: &str,
        target: &ChannelDeliveryTarget,
    ) -> anyhow::Result<()> {
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

    #[test]
    fn explicit_telegram_delivery_normalizes_chat_id() {
        let config = ProductConfig::default();
        let spec = RecurringDeliverySpec {
            mode: RecurringDeliveryMode::Explicit,
            channel: Some("telegram".to_string()),
            channel_id: None,
            telegram_chat_id: Some("999".to_string()),
            discord_channel_id: None,
            slack_channel_id: None,
            whatsapp_chat_jid: None,
            whatsapp_chat_id: None,
            user_id: None,
            session_id: None,
        };
        let target =
            resolve_explicit_delivery(&spec, &config, "recurring-test").expect("telegram delivery");

        assert_eq!(target.channel_id, "telegram:chat:999");
    }

    #[tokio::test]
    async fn legacy_delivery_json_is_only_an_adapter_to_typed_resolution() {
        let target = parse_delivery_spec(
            &serde_json::json!({
                "channel": "cli"
            }),
            DeliveryResolveContext {
                ambient: None,
                fallback_session_id: "recurring-test".to_string(),
            },
        )
        .await
        .expect("legacy delivery adapter");

        assert_eq!(target.channel_id, "cli:session:default");
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
