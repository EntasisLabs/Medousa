use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::secrets;

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn product_config_path() -> PathBuf {
    medousa_data_dir().join("product_config.json")
}

fn read_product_config() -> Value {
    let path = product_config_path();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Value::Object(serde_json::Map::new());
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| Value::Object(serde_json::Map::new()))
}

fn write_product_config(config: &Value) -> Result<(), String> {
    let path = product_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let body = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    std::fs::write(path, body).map_err(|err| err.to_string())
}

fn merge_channel_config(channel: &str, patch: Value) -> Result<(), String> {
    let mut config = read_product_config();
    let root = config
        .as_object_mut()
        .ok_or_else(|| "product_config root must be an object".to_string())?;
    root.insert(channel.to_string(), patch);
    write_product_config(&config)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TelegramChannelSummary {
    pub allowed_user_ids: Vec<u64>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_chat_ids: Vec<i64>,
    pub credentials_set: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscordChannelSummary {
    pub command_prefix: String,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_channel_ids: Vec<u64>,
    pub credentials_set: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SlackChannelSummary {
    pub allowed_user_ids: Vec<String>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_channel_ids: Vec<String>,
    pub bot_token_set: bool,
    pub app_token_set: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppChannelSummary {
    pub deliver_bind: String,
    pub deliver_url: Option<String>,
    pub session_db_path: Option<String>,
    pub allowed_user_ids: Vec<String>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_chat_jids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductConfigSummary {
    pub telegram: TelegramChannelSummary,
    pub discord: DiscordChannelSummary,
    pub slack: SlackChannelSummary,
    pub whatsapp: WhatsAppChannelSummary,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramConfigSave {
    pub allowed_user_ids: Vec<u64>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_chat_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordConfigSave {
    pub command_prefix: String,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_channel_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlackConfigSave {
    pub allowed_user_ids: Vec<String>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_channel_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppConfigSave {
    pub deliver_bind: String,
    pub deliver_url: Option<String>,
    pub session_db_path: Option<String>,
    pub allowed_user_ids: Vec<String>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_chat_jids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "channel")]
pub enum ChannelConfigSave {
    Telegram(TelegramConfigSave),
    Discord(DiscordConfigSave),
    Slack(SlackConfigSave),
    Whatsapp(WhatsAppConfigSave),
}

pub fn load_product_config_summary() -> Result<ProductConfigSummary, String> {
    let config = read_product_config();
    let telegram = config.get("telegram").cloned().unwrap_or_default();
    let discord = config.get("discord").cloned().unwrap_or_default();
    let slack = config.get("slack").cloned().unwrap_or_default();
    let whatsapp = config.get("whatsapp").cloned().unwrap_or_default();

    Ok(ProductConfigSummary {
        telegram: TelegramChannelSummary {
            allowed_user_ids: telegram
                .get("allowed_user_ids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            heartbeat_nudges_enabled: telegram
                .get("heartbeat_nudges_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            heartbeat_chat_ids: telegram
                .get("heartbeat_chat_ids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            credentials_set: secrets::secret_is_set("telegram_bot_token")?,
        },
        discord: DiscordChannelSummary {
            command_prefix: discord
                .get("command_prefix")
                .and_then(|value| value.as_str())
                .unwrap_or("!")
                .to_string(),
            heartbeat_nudges_enabled: discord
                .get("heartbeat_nudges_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            heartbeat_channel_ids: discord
                .get("heartbeat_channel_ids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            credentials_set: secrets::secret_is_set("discord_bot_token")?,
        },
        slack: SlackChannelSummary {
            allowed_user_ids: slack
                .get("allowed_user_ids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            heartbeat_nudges_enabled: slack
                .get("heartbeat_nudges_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            heartbeat_channel_ids: slack
                .get("heartbeat_channel_ids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            bot_token_set: secrets::secret_is_set("slack_bot_token")?,
            app_token_set: secrets::secret_is_set("slack_app_token")?,
        },
        whatsapp: WhatsAppChannelSummary {
            deliver_bind: whatsapp
                .get("deliver_bind")
                .and_then(|value| value.as_str())
                .unwrap_or("127.0.0.1:7422")
                .to_string(),
            deliver_url: whatsapp
                .get("deliver_url")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            session_db_path: whatsapp
                .get("session_db_path")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            allowed_user_ids: whatsapp
                .get("allowed_user_ids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
            heartbeat_nudges_enabled: whatsapp
                .get("heartbeat_nudges_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            heartbeat_chat_jids: whatsapp
                .get("heartbeat_chat_jids")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default(),
        },
    })
}

pub fn save_channel_product_config(request: ChannelConfigSave) -> Result<(), String> {
    match request {
        ChannelConfigSave::Telegram(config) => merge_channel_config(
            "telegram",
            json!({
                "allowed_user_ids": config.allowed_user_ids,
                "heartbeat_nudges_enabled": config.heartbeat_nudges_enabled,
                "heartbeat_chat_ids": config.heartbeat_chat_ids,
            }),
        ),
        ChannelConfigSave::Discord(config) => merge_channel_config(
            "discord",
            json!({
                "command_prefix": config.command_prefix,
                "heartbeat_nudges_enabled": config.heartbeat_nudges_enabled,
                "heartbeat_channel_ids": config.heartbeat_channel_ids,
            }),
        ),
        ChannelConfigSave::Slack(config) => merge_channel_config(
            "slack",
            json!({
                "allowed_user_ids": config.allowed_user_ids,
                "heartbeat_nudges_enabled": config.heartbeat_nudges_enabled,
                "heartbeat_channel_ids": config.heartbeat_channel_ids,
            }),
        ),
        ChannelConfigSave::Whatsapp(config) => merge_channel_config(
            "whatsapp",
            json!({
                "deliver_bind": config.deliver_bind,
                "deliver_url": config.deliver_url,
                "session_db_path": config.session_db_path,
                "allowed_user_ids": config.allowed_user_ids,
                "heartbeat_nudges_enabled": config.heartbeat_nudges_enabled,
                "heartbeat_chat_jids": config.heartbeat_chat_jids,
            }),
        ),
    }
}
