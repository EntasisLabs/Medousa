//! Messaging channels a workshop on this machine runs: their settings in
//! `product_config.json`, whether their tokens are stored, and the adapter
//! processes (`medousa_telegram`, …) that connect them to the engine.
//!
//! Carved from medousa-home's `messaging::product_config` and
//! `channel_adapters`. Tokens are written through the engine's
//! `/v1/integrations` routes; only an adapter's launch reads them here. All
//! of this is blocking file and process work.
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use medousa_types::secrets::{
    DaemonSecretPath, IntegrationConnection, IntegrationSecretSlot,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The channels, in medousa-home's order.
pub const CHANNELS: [&str; 4] = ["telegram", "discord", "slack", "whatsapp"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Telegram {
    pub allowed_user_ids: Vec<u64>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_chat_ids: Vec<i64>,
    pub credentials_set: bool,
    pub adapter_running: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Discord {
    pub command_prefix: String,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_channel_ids: Vec<u64>,
    pub credentials_set: bool,
    pub adapter_running: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Slack {
    pub allowed_user_ids: Vec<String>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_channel_ids: Vec<String>,
    pub bot_token_set: bool,
    pub app_token_set: bool,
    pub adapter_running: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WhatsApp {
    pub deliver_bind: String,
    pub deliver_url: Option<String>,
    pub session_db_path: Option<String>,
    pub allowed_user_ids: Vec<String>,
    pub heartbeat_nudges_enabled: bool,
    pub heartbeat_chat_jids: Vec<String>,
    pub adapter_running: bool,
}

/// What each channel is set to, and whether it runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub telegram: Telegram,
    pub discord: Discord,
    pub slack: Slack,
    pub whatsapp: WhatsApp,
}

/// A channel's state as medousa-home shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NeedsSetup,
    /// Set up; its adapter isn't running.
    Ready,
    Connected,
}

impl Summary {
    /// medousa-home's `channelStatus`.
    pub fn status(&self, channel: &str) -> Status {
        let (set_up, running) = match channel {
            "telegram" => (
                self.telegram.credentials_set && !self.telegram.allowed_user_ids.is_empty(),
                self.telegram.adapter_running,
            ),
            "discord" => (self.discord.credentials_set, self.discord.adapter_running),
            "slack" => (
                self.slack.bot_token_set
                    && self.slack.app_token_set
                    && !self.slack.allowed_user_ids.is_empty(),
                self.slack.adapter_running,
            ),
            "whatsapp" => (
                !self.whatsapp.deliver_bind.trim().is_empty()
                    && !self.whatsapp.allowed_user_ids.is_empty(),
                self.whatsapp.adapter_running,
            ),
            _ => (false, false),
        };
        match (set_up, running) {
            (false, _) => Status::NeedsSetup,
            (true, true) => Status::Connected,
            (true, false) => Status::Ready,
        }
    }
}

/// One channel's settings to save. Tokens are not here: they go to the
/// engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Save {
    Telegram {
        allowed_user_ids: Vec<u64>,
        heartbeat_nudges_enabled: bool,
        heartbeat_chat_ids: Vec<i64>,
    },
    Discord {
        command_prefix: String,
        heartbeat_nudges_enabled: bool,
        heartbeat_channel_ids: Vec<u64>,
    },
    Slack {
        allowed_user_ids: Vec<String>,
        heartbeat_nudges_enabled: bool,
        heartbeat_channel_ids: Vec<String>,
    },
    WhatsApp {
        deliver_bind: String,
        deliver_url: Option<String>,
        session_db_path: Option<String>,
        allowed_user_ids: Vec<String>,
        heartbeat_nudges_enabled: bool,
        heartbeat_chat_jids: Vec<String>,
    },
}

fn data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn read_config(dir: &Path) -> Value {
    fs::read_to_string(dir.join("product_config.json"))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn list<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Vec<T> {
    v.get(key)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// The engine's integration records, as it writes them in this data folder.
fn connections(dir: &Path) -> Vec<IntegrationConnection> {
    #[derive(Deserialize, Default)]
    struct Doc {
        #[serde(default)]
        connections: std::collections::BTreeMap<String, IntegrationConnection>,
    }
    fs::read_to_string(dir.join("integration_connections.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Doc>(&raw).ok())
        .unwrap_or_default()
        .connections
        .into_values()
        .collect()
}

fn token_set(conns: &[IntegrationConnection], kind: &str, slot: IntegrationSecretSlot) -> bool {
    conns.iter().any(|c| c.kind == kind && c.secrets.slot(slot))
}

/// Builds the summary from a data folder, adapters not checked.
fn summary_in(dir: &Path) -> Summary {
    let config = read_config(dir);
    let conns = connections(dir);
    let (t, d, s, w) = (
        &config["telegram"],
        &config["discord"],
        &config["slack"],
        &config["whatsapp"],
    );
    Summary {
        telegram: Telegram {
            allowed_user_ids: list(t, "allowed_user_ids"),
            heartbeat_nudges_enabled: flag(t, "heartbeat_nudges_enabled"),
            heartbeat_chat_ids: list(t, "heartbeat_chat_ids"),
            credentials_set: token_set(&conns, "telegram", IntegrationSecretSlot::BotToken),
            adapter_running: false,
        },
        discord: Discord {
            command_prefix: d
                .get("command_prefix")
                .and_then(Value::as_str)
                .unwrap_or("!")
                .to_string(),
            heartbeat_nudges_enabled: flag(d, "heartbeat_nudges_enabled"),
            heartbeat_channel_ids: list(d, "heartbeat_channel_ids"),
            credentials_set: token_set(&conns, "discord", IntegrationSecretSlot::BotToken),
            adapter_running: false,
        },
        slack: Slack {
            allowed_user_ids: list(s, "allowed_user_ids"),
            heartbeat_nudges_enabled: flag(s, "heartbeat_nudges_enabled"),
            heartbeat_channel_ids: list(s, "heartbeat_channel_ids"),
            bot_token_set: token_set(&conns, "slack", IntegrationSecretSlot::BotToken),
            app_token_set: token_set(&conns, "slack", IntegrationSecretSlot::AppToken),
            adapter_running: false,
        },
        whatsapp: WhatsApp {
            deliver_bind: w
                .get("deliver_bind")
                .and_then(Value::as_str)
                .unwrap_or("127.0.0.1:7423")
                .to_string(),
            deliver_url: w
                .get("deliver_url")
                .and_then(Value::as_str)
                .map(str::to_string),
            session_db_path: w
                .get("session_db_path")
                .and_then(Value::as_str)
                .map(str::to_string),
            allowed_user_ids: list(w, "allowed_user_ids"),
            heartbeat_nudges_enabled: flag(w, "heartbeat_nudges_enabled"),
            heartbeat_chat_jids: list(w, "heartbeat_chat_jids"),
            adapter_running: false,
        },
    }
}

/// Every channel's settings, whether its tokens are stored, and whether its
/// adapter runs.
pub fn summary() -> Summary {
    let dir = data_dir();
    let mut s = summary_in(&dir);
    s.telegram.adapter_running = adapter_running(&dir, "telegram");
    s.discord.adapter_running = adapter_running(&dir, "discord");
    s.slack.adapter_running = adapter_running(&dir, "slack");
    s.whatsapp.adapter_running = adapter_running(&dir, "whatsapp");
    s
}

fn save_in(dir: &Path, save: &Save) -> Result<(), String> {
    let (channel, value) = match save {
        Save::Telegram {
            allowed_user_ids,
            heartbeat_nudges_enabled,
            heartbeat_chat_ids,
        } => (
            "telegram",
            json!({
                "allowed_user_ids": allowed_user_ids,
                "heartbeat_nudges_enabled": heartbeat_nudges_enabled,
                "heartbeat_chat_ids": heartbeat_chat_ids,
            }),
        ),
        Save::Discord {
            command_prefix,
            heartbeat_nudges_enabled,
            heartbeat_channel_ids,
        } => (
            "discord",
            json!({
                "command_prefix": command_prefix,
                "heartbeat_nudges_enabled": heartbeat_nudges_enabled,
                "heartbeat_channel_ids": heartbeat_channel_ids,
            }),
        ),
        Save::Slack {
            allowed_user_ids,
            heartbeat_nudges_enabled,
            heartbeat_channel_ids,
        } => (
            "slack",
            json!({
                "allowed_user_ids": allowed_user_ids,
                "heartbeat_nudges_enabled": heartbeat_nudges_enabled,
                "heartbeat_channel_ids": heartbeat_channel_ids,
            }),
        ),
        Save::WhatsApp {
            deliver_bind,
            deliver_url,
            session_db_path,
            allowed_user_ids,
            heartbeat_nudges_enabled,
            heartbeat_chat_jids,
        } => (
            "whatsapp",
            json!({
                "deliver_bind": deliver_bind,
                "deliver_url": deliver_url,
                "session_db_path": session_db_path,
                "allowed_user_ids": allowed_user_ids,
                "heartbeat_nudges_enabled": heartbeat_nudges_enabled,
                "heartbeat_chat_jids": heartbeat_chat_jids,
            }),
        ),
    };
    let mut config = read_config(dir);
    config[channel] = value;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let body = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(dir.join("product_config.json"), body).map_err(|e| e.to_string())
}

/// Saves one channel's settings, then starts, restarts or stops adapters to
/// match, connecting them to `daemon_url`.
pub fn save(save: &Save, daemon_url: &str) -> Result<(), String> {
    save_in(&data_dir(), save)?;
    sync(daemon_url)
}

// --- Adapters ---------------------------------------------------------------

fn pid_path(dir: &Path, channel: &str) -> PathBuf {
    dir.join("adapters").join(format!("{channel}.pid"))
}

fn read_pid(dir: &Path, channel: &str) -> Option<u32> {
    fs::read_to_string(pid_path(dir, channel))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn adapter_running(dir: &Path, channel: &str) -> bool {
    read_pid(dir, channel).is_some_and(alive)
}

fn stop(dir: &Path, channel: &str) {
    if let Some(pid) = read_pid(dir, channel)
        && alive(pid)
    {
        #[cfg(unix)]
        {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
    let _ = fs::remove_file(pid_path(dir, channel));
}

fn should_run(s: &Summary, channel: &str) -> bool {
    s.status(channel) != Status::NeedsSetup
}

/// The adapter binary: an explicit `MEDOUSA_*_BIN`, next to this app, in the
/// shared bin folder, or on PATH.
fn adapter_binary(name: &str) -> Result<String, String> {
    use crate::workshop_runtime as rt;
    if let Ok(explicit) = std::env::var(format!("MEDOUSA_{}_BIN", name.to_ascii_uppercase())) {
        let path = PathBuf::from(explicit.trim());
        if path.exists() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name(rt::platform_binary_name(name));
        if sibling.exists() {
            return Ok(sibling.to_string_lossy().into_owned());
        }
    }
    if let Some(shared) = rt::shared_bin_binary(name) {
        return Ok(shared.to_string_lossy().into_owned());
    }
    if rt::find_command_in_path(name).is_some() {
        return Ok(rt::platform_binary_name(name));
    }
    Err(format!(
        "Could not find {name}. Install it from Settings → Packages, or run from a full app bundle."
    ))
}

/// A stored token, read where the engine keeps it.
fn token(dir: &Path, kind: &str, slot: IntegrationSecretSlot) -> Option<String> {
    let installation_id = medousa_secrets::load_installation_id(dir).ok()??;
    let conns = connections(dir);
    let conn = conns
        .iter()
        .filter(|c| c.kind == kind)
        .find(|c| c.secrets.slot(slot))?;
    let path = DaemonSecretPath::Integration {
        installation_id,
        connection_id: conn.connection_id.clone(),
        slot,
    };
    medousa_secrets::load_daemon_secret(dir, &path)
        .ok()
        .flatten()
        .map(|r| r.value)
}

fn loopback(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
    })
}

fn csv<T: ToString>(values: &[T]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn start(dir: &Path, channel: &str, daemon_url: &str, s: &Summary) -> Result<(), String> {
    let mut command = Command::new(adapter_binary(&format!("medousa_{channel}"))?);
    command.arg("--daemon-url").arg(daemon_url);
    command.env("MEDOUSA_DATA_DIR", dir);
    let missing = |what: &str| format!("{what} is missing.");
    match channel {
        "telegram" => {
            let t = token(dir, "telegram", IntegrationSecretSlot::BotToken)
                .ok_or_else(|| missing("Telegram bot token"))?;
            if s.telegram.heartbeat_nudges_enabled {
                command.env("MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED", "true");
            }
            if !s.telegram.heartbeat_chat_ids.is_empty() {
                command.env(
                    "MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS",
                    csv(&s.telegram.heartbeat_chat_ids),
                );
            }
            command.arg("--token").arg(t);
        }
        "discord" => {
            let t = token(dir, "discord", IntegrationSecretSlot::BotToken)
                .ok_or_else(|| missing("Discord bot token"))?;
            let prefix = s.discord.command_prefix.trim();
            command.env(
                "MEDOUSA_DISCORD_COMMAND_PREFIX",
                if prefix.is_empty() { "!" } else { prefix },
            );
            if s.discord.heartbeat_nudges_enabled {
                command.env("MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED", "true");
            }
            if !s.discord.heartbeat_channel_ids.is_empty() {
                command.env(
                    "MEDOUSA_DISCORD_HEARTBEAT_CHANNEL_IDS",
                    csv(&s.discord.heartbeat_channel_ids),
                );
            }
            command.arg("--token").arg(t);
        }
        "slack" | "whatsapp" => {
            if !loopback(daemon_url) {
                return Err(format!(
                    "{} must run on the connected workshop host",
                    if channel == "slack" { "Slack" } else { "WhatsApp" }
                ));
            }
            let credential = medousa_local_credential::load_named_secret(
                dir,
                medousa_local_credential::CLI_LOCAL_NAME,
            )
            .map_err(|e| format!("load workshop adapter credential: {e}"))?;
            command.env("MEDOUSA_DAEMON_BEARER", credential.token());
            if channel == "slack" {
                let bot = token(dir, "slack", IntegrationSecretSlot::BotToken)
                    .ok_or_else(|| missing("Slack bot token"))?;
                let app = token(dir, "slack", IntegrationSecretSlot::AppToken)
                    .ok_or_else(|| missing("Slack app token"))?;
                if s.slack.heartbeat_nudges_enabled {
                    command.env("MEDOUSA_SLACK_HEARTBEAT_NUDGES_ENABLED", "true");
                }
                if !s.slack.heartbeat_channel_ids.is_empty() {
                    command.env(
                        "MEDOUSA_SLACK_HEARTBEAT_CHANNEL_IDS",
                        s.slack.heartbeat_channel_ids.join(","),
                    );
                }
                command.arg("--bot-token").arg(bot);
                command.arg("--app-token").arg(app);
            } else {
                command
                    .arg("--deliver-bind")
                    .arg(s.whatsapp.deliver_bind.trim());
                let set = |v: &Option<String>| {
                    v.as_deref()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string)
                };
                if let Some(url) = set(&s.whatsapp.deliver_url) {
                    command.arg("--deliver-url").arg(url);
                }
                if let Some(path) = set(&s.whatsapp.session_db_path) {
                    command.arg("--session-db").arg(path);
                }
            }
        }
        other => return Err(format!("unknown channel '{other}'")),
    }
    let log = dir.join("logs").join(format!("{channel}.log"));
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    // The adapter outlives this app, as medousa-home's do.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let pid = command.spawn().map_err(|e| e.to_string())?.id();
    fs::create_dir_all(dir.join("adapters")).map_err(|e| e.to_string())?;
    fs::write(pid_path(dir, channel), pid.to_string()).map_err(|e| e.to_string())
}

/// Starts set-up channels, restarts running ones so new settings and tokens
/// apply, and stops the rest. A channel that can't start is reported; the
/// others still sync.
pub fn sync(daemon_url: &str) -> Result<(), String> {
    let dir = data_dir();
    let s = summary();
    let mut errors = Vec::new();
    for channel in CHANNELS {
        let running = adapter_running(&dir, channel);
        if running {
            stop(&dir, channel);
        }
        if should_run(&s, channel)
            && let Err(e) = start(&dir, channel, daemon_url, &s)
        {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" "))
    }
}

/// Starts the WhatsApp adapter if it isn't running, for a conversation that
/// pairs through it, as medousa-home's `messaging_ensure_whatsapp_adapter`.
pub fn ensure_whatsapp(daemon_url: &str) -> Result<(), String> {
    let dir = data_dir();
    if adapter_running(&dir, "whatsapp") {
        return Ok(());
    }
    let s = summary();
    if !should_run(&s, "whatsapp") {
        return Err("Configure WhatsApp in Settings → Sharing → Channels first.".into());
    }
    start(&dir, "whatsapp", daemon_url, &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_keep_other_channels() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("product_config.json"),
            r#"{"discord": {"command_prefix": "?"}, "other": 1}"#,
        )
        .unwrap();
        let s = summary_in(dir.path());
        assert_eq!(s.discord.command_prefix, "?");
        assert_eq!(s.whatsapp.deliver_bind, "127.0.0.1:7423");
        assert_eq!(s.status("telegram"), Status::NeedsSetup);
        save_in(
            dir.path(),
            &Save::Telegram {
                allowed_user_ids: vec![42],
                heartbeat_nudges_enabled: true,
                heartbeat_chat_ids: vec![-100],
            },
        )
        .unwrap();
        let s = summary_in(dir.path());
        assert_eq!(s.telegram.allowed_user_ids, [42]);
        assert_eq!(s.telegram.heartbeat_chat_ids, [-100]);
        assert_eq!(s.discord.command_prefix, "?", "other channels survive");
        let raw: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join("product_config.json")).unwrap())
                .unwrap();
        assert_eq!(raw["other"], 1);
        // No token yet: still needs setup.
        assert_eq!(s.status("telegram"), Status::NeedsSetup);
    }

    #[test]
    fn stored_tokens_count_by_kind_and_slot() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("integration_connections.json"),
            r#"{"connections": {"a": {
                "connection_id": "11111111-1111-4111-8111-111111111111", "kind": "slack",
                "secrets": {"bot_token": true},
                "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z"
            }}}"#,
        )
        .unwrap();
        save_in(
            dir.path(),
            &Save::Slack {
                allowed_user_ids: vec!["U1".into()],
                heartbeat_nudges_enabled: false,
                heartbeat_channel_ids: vec![],
            },
        )
        .unwrap();
        let s = summary_in(dir.path());
        assert!(s.slack.bot_token_set && !s.slack.app_token_set);
        assert_eq!(s.status("slack"), Status::NeedsSetup, "needs both tokens");
        assert!(loopback("http://127.0.0.1:7419") && loopback("http://[::1]:1"));
        assert!(!loopback("https://studio.example.com"));
    }
}
