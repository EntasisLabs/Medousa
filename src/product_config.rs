use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const DEFAULT_DAEMON_BIND: &str = "127.0.0.1:7419";
const DEFAULT_DISCORD_PREFIX: &str = "!";
const DEFAULT_WHATSAPP_DELIVER_BIND: &str = "127.0.0.1:7422";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductConfig {
    #[serde(default)]
    pub daemon: DaemonProductConfig,
    #[serde(default)]
    pub telegram: TelegramProductConfig,
    #[serde(default)]
    pub discord: DiscordProductConfig,
    #[serde(default)]
    pub slack: SlackProductConfig,
    #[serde(default)]
    pub whatsapp: WhatsAppProductConfig,
    #[serde(default)]
    pub tui: TuiProductConfig,
    #[serde(default)]
    pub runtime: RuntimeProductConfig,
    #[serde(default)]
    pub identity: IdentityProductConfig,
    #[serde(default)]
    pub surreal: SurrealProductConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SurrealProductConfig {
    /// WebSocket RPC URL without the `surreal-ws:` prefix (e.g. `ws://127.0.0.1:8000/rpc`).
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonProductConfig {
    #[serde(default = "default_daemon_bind")]
    pub bind: String,
    #[serde(default = "default_heartbeat_min_significance")]
    pub heartbeat_min_significance: f32,
    #[serde(default)]
    pub deliver_webhook_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TelegramProductConfig {
    #[serde(default)]
    pub allowed_user_ids: Vec<u64>,
    #[serde(default)]
    pub heartbeat_nudges_enabled: bool,
    #[serde(default)]
    pub heartbeat_chat_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscordProductConfig {
    #[serde(default = "default_discord_prefix")]
    pub command_prefix: String,
    #[serde(default)]
    pub heartbeat_nudges_enabled: bool,
    #[serde(default)]
    pub heartbeat_channel_ids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SlackProductConfig {
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub heartbeat_nudges_enabled: bool,
    #[serde(default)]
    pub heartbeat_channel_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WhatsAppProductConfig {
    #[serde(default = "default_whatsapp_deliver_bind")]
    pub deliver_bind: String,
    #[serde(default)]
    pub deliver_url: Option<String>,
    #[serde(default)]
    pub session_db_path: Option<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub heartbeat_nudges_enabled: bool,
    #[serde(default)]
    pub heartbeat_chat_jids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeProductConfig {
    #[serde(default)]
    pub workflow: RuntimeWorkflowConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeWorkflowConfig {
    #[serde(default = "default_workflow_strategy")]
    pub default_strategy: String,
    #[serde(default = "default_parallel_tool_calls_enabled")]
    pub parallel_tool_calls_enabled: bool,
    #[serde(default = "default_max_parallel_tool_calls")]
    pub max_parallel_tool_calls: usize,
    #[serde(default = "default_max_concurrent_workflow_steps")]
    pub max_concurrent_workflow_steps: usize,
    #[serde(default)]
    pub allow_mutating_parallel: bool,
}

impl Default for RuntimeProductConfig {
    fn default() -> Self {
        Self {
            workflow: RuntimeWorkflowConfig::default(),
        }
    }
}

impl Default for RuntimeWorkflowConfig {
    fn default() -> Self {
        Self {
            default_strategy: default_workflow_strategy(),
            parallel_tool_calls_enabled: default_parallel_tool_calls_enabled(),
            max_parallel_tool_calls: default_max_parallel_tool_calls(),
            max_concurrent_workflow_steps: default_max_concurrent_workflow_steps(),
            allow_mutating_parallel: false,
        }
    }
}

fn default_workflow_strategy() -> String {
    "sequential".to_string()
}

fn default_parallel_tool_calls_enabled() -> bool {
    true
}

fn default_max_parallel_tool_calls() -> usize {
    4
}

fn default_max_concurrent_workflow_steps() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdentityProductConfig {
    #[serde(default = "default_identity_enabled")]
    pub enabled: bool,
    #[serde(default = "default_identity_auto_commit_min_confidence")]
    pub auto_commit_min_confidence: f32,
    #[serde(default = "default_identity_model_inferred_auto_commit_fields")]
    pub model_inferred_auto_commit_fields: Vec<String>,
    #[serde(default = "default_identity_bridge_to_locus")]
    pub bridge_to_locus: bool,
}

impl Default for IdentityProductConfig {
    fn default() -> Self {
        Self {
            enabled: default_identity_enabled(),
            auto_commit_min_confidence: default_identity_auto_commit_min_confidence(),
            model_inferred_auto_commit_fields: default_identity_model_inferred_auto_commit_fields(),
            bridge_to_locus: default_identity_bridge_to_locus(),
        }
    }
}

fn default_identity_enabled() -> bool {
    true
}

fn default_identity_auto_commit_min_confidence() -> f32 {
    0.85
}

fn default_identity_model_inferred_auto_commit_fields() -> Vec<String> {
    vec![
        "display_name".to_string(),
        "timezone".to_string(),
        "language_variant".to_string(),
        "preferences.*".to_string(),
        "policy_tags".to_string(),
        "last_transition_reason".to_string(),
        "aliases".to_string(),
        "recency_score".to_string(),
        "confidence".to_string(),
    ]
}

fn default_identity_bridge_to_locus() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TuiProductConfig {
    #[serde(default = "default_response_depth")]
    pub response_depth_mode: String,
}

impl Default for ProductConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonProductConfig::default(),
            telegram: TelegramProductConfig::default(),
            discord: DiscordProductConfig::default(),
            slack: SlackProductConfig::default(),
            whatsapp: WhatsAppProductConfig::default(),
            tui: TuiProductConfig::default(),
            runtime: RuntimeProductConfig::default(),
            identity: IdentityProductConfig::default(),
            surreal: SurrealProductConfig::default(),
        }
    }
}

impl Default for DaemonProductConfig {
    fn default() -> Self {
        Self {
            bind: default_daemon_bind(),
            heartbeat_min_significance: default_heartbeat_min_significance(),
            deliver_webhook_token: None,
        }
    }
}

impl Default for DiscordProductConfig {
    fn default() -> Self {
        Self {
            command_prefix: default_discord_prefix(),
            heartbeat_nudges_enabled: false,
            heartbeat_channel_ids: Vec::new(),
        }
    }
}

impl Default for WhatsAppProductConfig {
    fn default() -> Self {
        Self {
            deliver_bind: default_whatsapp_deliver_bind(),
            deliver_url: None,
            session_db_path: None,
            allowed_user_ids: Vec::new(),
            heartbeat_nudges_enabled: false,
            heartbeat_chat_jids: Vec::new(),
        }
    }
}

impl Default for TuiProductConfig {
    fn default() -> Self {
        Self {
            response_depth_mode: default_response_depth(),
        }
    }
}

fn default_daemon_bind() -> String {
    DEFAULT_DAEMON_BIND.to_string()
}

fn default_discord_prefix() -> String {
    DEFAULT_DISCORD_PREFIX.to_string()
}

fn default_whatsapp_deliver_bind() -> String {
    DEFAULT_WHATSAPP_DELIVER_BIND.to_string()
}

fn default_heartbeat_min_significance() -> f32 {
    0.65
}

fn default_response_depth() -> String {
    "standard".to_string()
}

pub fn product_config_path() -> PathBuf {
    medousa_data_dir().join("product_config.json")
}

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

pub fn load_product_config() -> ProductConfig {
    let path = product_config_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return ProductConfig::default();
    };

    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_product_config(config: &ProductConfig) -> anyhow::Result<()> {
    let path = product_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(config)?;
    std::fs::write(path, body)?;
    Ok(())
}

/// Merge legacy onboard profile telegram allowlist into product config when present.
pub fn migrate_from_onboard_profile(
    config: &mut ProductConfig,
    telegram_allow_user_ids: Option<&str>,
) {
    if !config.telegram.allowed_user_ids.is_empty() {
        return;
    }
    if let Some(ids) = parse_u64_csv(telegram_allow_user_ids.unwrap_or("")) {
        config.telegram.allowed_user_ids = ids;
    }
}

pub fn parse_u64_csv(raw: &str) -> Option<Vec<u64>> {
    let ids = raw
        .split(',')
        .filter_map(|token| token.trim().parse::<u64>().ok())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

pub fn parse_i64_csv(raw: &str) -> Vec<i64> {
    raw.split(',')
        .filter_map(|token| token.trim().parse::<i64>().ok())
        .collect()
}

pub fn format_u64_csv(values: &[u64]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn format_i64_csv(values: &[i64]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Returns true when the sender is allowed for this channel ingest request.
pub fn ingest_sender_allowed(channel: &str, user_id: &str, config: &ProductConfig) -> bool {
    match channel.to_ascii_lowercase().as_str() {
        "telegram" => {
            let allowed = &config.telegram.allowed_user_ids;
            if allowed.is_empty() {
                return true;
            }

            extract_numeric_user_id(user_id)
                .map(|id| allowed.contains(&id))
                .unwrap_or(false)
        }
        "slack" => {
            let allowed = &config.slack.allowed_user_ids;
            if allowed.is_empty() {
                return true;
            }

            extract_slack_user_id(user_id)
                .map(|id| allowed.iter().any(|entry| entry == &id))
                .unwrap_or(false)
        }
        "whatsapp" => {
            let allowed = &config.whatsapp.allowed_user_ids;
            if allowed.is_empty() {
                return true;
            }

            allowed
                .iter()
                .any(|entry| user_id == entry || user_id.ends_with(entry))
        }
        _ => true,
    }
}

fn extract_slack_user_id(user_id: &str) -> Option<String> {
    user_id
        .strip_prefix("slack:user:")
        .or_else(|| user_id.rsplit(':').next())
        .map(str::to_string)
}

fn extract_numeric_user_id(user_id: &str) -> Option<u64> {
    user_id
        .rsplit(':')
        .next()
        .and_then(|segment| segment.parse::<u64>().ok())
}

/// Apply daemon-facing environment variables from product config for child processes.
pub fn apply_surreal_env(config: &ProductConfig) {
    apply_surreal_env_from_fields(&config.surreal);
}

pub fn apply_surreal_env_from_fields(surreal: &SurrealProductConfig) {
    set_or_remove_env("MEDOUSA_SURREAL_ENDPOINT", surreal.endpoint.as_deref());
    set_or_remove_env("MEDOUSA_SURREAL_USERNAME", surreal.username.as_deref());
    let stored_password = crate::session::load_surreal_password();
    let password = surreal
        .password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            stored_password
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
    set_or_remove_env("MEDOUSA_SURREAL_PASSWORD", password);
    set_or_remove_env("MEDOUSA_SURREAL_NAMESPACE", surreal.namespace.as_deref());
    set_or_remove_env("MEDOUSA_SURREAL_DATABASE", surreal.database.as_deref());
}

fn set_or_remove_env(key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        unsafe { std::env::set_var(key, value) };
    } else {
        unsafe { std::env::remove_var(key) };
    }
}

pub fn apply_daemon_env(config: &ProductConfig) {
    apply_surreal_env(config);
    if let Some(token) = config
        .daemon
        .deliver_webhook_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        unsafe { std::env::set_var("MEDOUSA_DELIVER_WEBHOOK_TOKEN", token) };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_DELIVER_WEBHOOK_TOKEN") };
    }
}

/// Apply adapter-facing environment variables from product config for child processes.
pub fn apply_adapter_env(config: &ProductConfig) {
    apply_telegram_env(&config.telegram);
    apply_discord_env(&config.discord);
    apply_slack_env(&config.slack);
    apply_whatsapp_env(&config.whatsapp);
}

fn apply_telegram_env(config: &TelegramProductConfig) {
    if config.heartbeat_nudges_enabled {
        unsafe { std::env::set_var("MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED", "true") };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED") };
    }

    if config.heartbeat_chat_ids.is_empty() {
        unsafe { std::env::remove_var("MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS") };
    } else {
        unsafe {
            std::env::set_var(
                "MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS",
                format_i64_csv(&config.heartbeat_chat_ids),
            )
        };
    }
}

fn apply_discord_env(config: &DiscordProductConfig) {
    let prefix = config.command_prefix.trim();
    if prefix.is_empty() {
        unsafe { std::env::set_var("MEDOUSA_DISCORD_COMMAND_PREFIX", DEFAULT_DISCORD_PREFIX) };
    } else {
        unsafe { std::env::set_var("MEDOUSA_DISCORD_COMMAND_PREFIX", prefix) };
    }

    if config.heartbeat_nudges_enabled {
        unsafe { std::env::set_var("MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED", "true") };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED") };
    }

    if config.heartbeat_channel_ids.is_empty() {
        unsafe { std::env::remove_var("MEDOUSA_DISCORD_HEARTBEAT_CHANNEL_IDS") };
    } else {
        unsafe {
            std::env::set_var(
                "MEDOUSA_DISCORD_HEARTBEAT_CHANNEL_IDS",
                format_u64_csv(&config.heartbeat_channel_ids),
            )
        };
    }
}

fn apply_slack_env(config: &SlackProductConfig) {
    if config.heartbeat_nudges_enabled {
        unsafe { std::env::set_var("MEDOUSA_SLACK_HEARTBEAT_NUDGES_ENABLED", "true") };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_SLACK_HEARTBEAT_NUDGES_ENABLED") };
    }

    if config.heartbeat_channel_ids.is_empty() {
        unsafe { std::env::remove_var("MEDOUSA_SLACK_HEARTBEAT_CHANNEL_IDS") };
    } else {
        unsafe {
            std::env::set_var(
                "MEDOUSA_SLACK_HEARTBEAT_CHANNEL_IDS",
                config.heartbeat_channel_ids.join(","),
            )
        };
    }
}

fn apply_whatsapp_env(config: &WhatsAppProductConfig) {
    unsafe {
        std::env::set_var("MEDOUSA_WHATSAPP_DELIVER_BIND", config.deliver_bind.trim());
    }

    if let Some(url) = config
        .deliver_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        unsafe { std::env::set_var("MEDOUSA_WHATSAPP_DELIVER_URL", url) };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_WHATSAPP_DELIVER_URL") };
    }

    if let Some(path) = config
        .session_db_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        unsafe { std::env::set_var("MEDOUSA_WHATSAPP_SESSION_DB", path) };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_WHATSAPP_SESSION_DB") };
    }

    if config.heartbeat_nudges_enabled {
        unsafe { std::env::set_var("MEDOUSA_WHATSAPP_HEARTBEAT_NUDGES_ENABLED", "true") };
    } else {
        unsafe { std::env::remove_var("MEDOUSA_WHATSAPP_HEARTBEAT_NUDGES_ENABLED") };
    }

    if config.heartbeat_chat_jids.is_empty() {
        unsafe { std::env::remove_var("MEDOUSA_WHATSAPP_HEARTBEAT_CHAT_JIDS") };
    } else {
        unsafe {
            std::env::set_var(
                "MEDOUSA_WHATSAPP_HEARTBEAT_CHAT_JIDS",
                config.heartbeat_chat_jids.join(","),
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_allowlist_blocks_unknown_users() {
        let config = ProductConfig {
            telegram: TelegramProductConfig {
                allowed_user_ids: vec![123, 456],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(ingest_sender_allowed(
            "telegram",
            "telegram:user:123",
            &config
        ));
        assert!(!ingest_sender_allowed(
            "telegram",
            "telegram:user:999",
            &config
        ));
    }

    #[test]
    fn empty_allowlist_allows_all() {
        let config = ProductConfig::default();
        assert!(ingest_sender_allowed(
            "telegram",
            "telegram:user:999",
            &config
        ));
    }
}
