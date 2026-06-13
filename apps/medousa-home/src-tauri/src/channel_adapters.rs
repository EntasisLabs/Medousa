use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::messaging::product_config::ProductConfigSummary;
use crate::messaging::secrets;

const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";

struct ComponentCommand {
    program: String,
    pre_args: Vec<String>,
}

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn adapters_dir() -> PathBuf {
    medousa_data_dir().join("adapters")
}

fn adapter_pid_path(channel: &str) -> PathBuf {
    adapters_dir().join(format!("{channel}.pid"))
}

fn adapter_log_path(channel: &str) -> PathBuf {
    medousa_data_dir().join("logs").join(format!("{channel}.log"))
}

fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|path| path.join(command))
        .find(|candidate| candidate.exists())
}

fn resolve_component_command(binary_name: &str) -> Result<ComponentCommand, String> {
    let env_key = format!("MEDOUSA_{}_BIN", binary_name.to_ascii_uppercase());
    if let Ok(explicit) = std::env::var(&env_key) {
        let path = PathBuf::from(explicit.trim());
        if path.exists() {
            return Ok(ComponentCommand {
                program: path.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        let sibling = current_exe.with_file_name(binary_name);
        if sibling.exists() {
            return Ok(ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if find_command_in_path(binary_name).is_some() {
        return Ok(ComponentCommand {
            program: binary_name.to_string(),
            pre_args: Vec::new(),
        });
    }

    Err(format!(
        "Could not find {binary_name}. Install Medousa command-line tools or run from a full app bundle."
    ))
}

#[cfg(unix)]
fn detach_new_session(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_new_session(_command: &mut Command) {}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn read_pid(channel: &str) -> Option<u32> {
    fs::read_to_string(adapter_pid_path(channel))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn write_pid(channel: &str, pid: u32) -> Result<(), String> {
    fs::create_dir_all(adapters_dir()).map_err(|err| err.to_string())?;
    fs::write(adapter_pid_path(channel), pid.to_string()).map_err(|err| err.to_string())
}

fn clear_pid(channel: &str) {
    let _ = fs::remove_file(adapter_pid_path(channel));
}

pub fn adapter_is_running(channel: &str) -> bool {
    read_pid(channel).is_some_and(is_process_alive)
}

pub fn stop_adapter(channel: &str) {
    if let Some(pid) = read_pid(channel) {
        if is_process_alive(pid) {
            #[cfg(unix)]
            {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
    }
    clear_pid(channel);
}

fn spawn_background(command: &mut Command, log_path: &PathBuf) -> Result<u32, String> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| err.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|err| err.to_string())?;
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));
    detach_new_session(command);
    command
        .spawn()
        .map_err(|err| err.to_string())
        .map(|child| child.id())
}

fn format_u64_csv(values: &[u64]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_i64_csv(values: &[i64]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn apply_telegram_env(command: &mut Command, summary: &ProductConfigSummary) {
    let channel = &summary.telegram;
    if channel.heartbeat_nudges_enabled {
        command.env("MEDOUSA_TELEGRAM_HEARTBEAT_NUDGES_ENABLED", "true");
    }
    if !channel.heartbeat_chat_ids.is_empty() {
        command.env(
            "MEDOUSA_TELEGRAM_HEARTBEAT_CHAT_IDS",
            format_i64_csv(&channel.heartbeat_chat_ids),
        );
    }
}

fn apply_discord_env(command: &mut Command, summary: &ProductConfigSummary) {
    let channel = &summary.discord;
    let prefix = channel.command_prefix.trim();
    command.env(
        "MEDOUSA_DISCORD_COMMAND_PREFIX",
        if prefix.is_empty() { "!" } else { prefix },
    );
    if channel.heartbeat_nudges_enabled {
        command.env("MEDOUSA_DISCORD_HEARTBEAT_NUDGES_ENABLED", "true");
    }
    if !channel.heartbeat_channel_ids.is_empty() {
        command.env(
            "MEDOUSA_DISCORD_HEARTBEAT_CHANNEL_IDS",
            format_u64_csv(&channel.heartbeat_channel_ids),
        );
    }
}

fn apply_slack_env(command: &mut Command, summary: &ProductConfigSummary) {
    let channel = &summary.slack;
    if channel.heartbeat_nudges_enabled {
        command.env("MEDOUSA_SLACK_HEARTBEAT_NUDGES_ENABLED", "true");
    }
    if !channel.heartbeat_channel_ids.is_empty() {
        command.env(
            "MEDOUSA_SLACK_HEARTBEAT_CHANNEL_IDS",
            channel.heartbeat_channel_ids.join(","),
        );
    }
}

fn channel_should_run(summary: &ProductConfigSummary, channel: &str) -> bool {
    match channel {
        "telegram" => {
            summary.telegram.credentials_set && !summary.telegram.allowed_user_ids.is_empty()
        }
        "discord" => summary.discord.credentials_set,
        "slack" => {
            summary.slack.bot_token_set
                && summary.slack.app_token_set
                && !summary.slack.allowed_user_ids.is_empty()
        }
        "whatsapp" => {
            !summary.whatsapp.deliver_bind.trim().is_empty()
                && !summary.whatsapp.allowed_user_ids.is_empty()
        }
        _ => false,
    }
}

fn start_adapter(
    channel: &str,
    daemon_url: &str,
    summary: &ProductConfigSummary,
) -> Result<(), String> {
    let adapter = resolve_component_command(match channel {
        "telegram" => "medousa_telegram",
        "discord" => "medousa_discord",
        "slack" => "medousa_slack",
        "whatsapp" => "medousa_whatsapp",
        other => return Err(format!("unknown channel '{other}'")),
    })?;

    let mut command = Command::new(&adapter.program);
    command.args(&adapter.pre_args);
    command.arg("--daemon-url").arg(daemon_url);

    match channel {
        "telegram" => {
            let token = secrets::load_secret_value("telegram_bot_token")?
                .ok_or_else(|| "Telegram bot token is missing.".to_string())?;
            apply_telegram_env(&mut command, summary);
            command.arg("--token").arg(token);
        }
        "discord" => {
            let token = secrets::load_secret_value("discord_bot_token")?
                .ok_or_else(|| "Discord bot token is missing.".to_string())?;
            apply_discord_env(&mut command, summary);
            command.arg("--token").arg(token);
        }
        "slack" => {
            let bot_token = secrets::load_secret_value("slack_bot_token")?
                .ok_or_else(|| "Slack bot token is missing.".to_string())?;
            let app_token = secrets::load_secret_value("slack_app_token")?
                .ok_or_else(|| "Slack app token is missing.".to_string())?;
            apply_slack_env(&mut command, summary);
            command.arg("--bot-token").arg(bot_token);
            command.arg("--app-token").arg(app_token);
        }
        "whatsapp" => {
            command
                .arg("--deliver-bind")
                .arg(summary.whatsapp.deliver_bind.trim());
            if let Some(url) = summary
                .whatsapp
                .deliver_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                command.arg("--deliver-url").arg(url);
            }
            if let Some(path) = summary
                .whatsapp
                .session_db_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                command.arg("--session-db").arg(path);
            }
        }
        _ => {}
    }

    let pid = spawn_background(&mut command, &adapter_log_path(channel))?;
    write_pid(channel, pid)?;
    Ok(())
}

pub fn sync_channel_adapters(daemon_url: Option<&str>) -> Result<(), String> {
    let summary = crate::messaging::product_config::load_product_config_summary()?;
    let daemon_url = daemon_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_DAEMON_URL);

    for channel in ["telegram", "discord", "slack", "whatsapp"] {
        let should_run = channel_should_run(&summary, channel);
        let running = adapter_is_running(channel);
        if should_run && !running {
            if let Err(err) = start_adapter(channel, daemon_url, &summary) {
                eprintln!("channel adapter {channel} start error: {err}");
            }
        } else if !should_run && running {
            stop_adapter(channel);
        } else if should_run && running {
            // Config may have changed — restart for token/env updates.
            stop_adapter(channel);
            if let Err(err) = start_adapter(channel, daemon_url, &summary) {
                eprintln!("channel adapter {channel} restart error: {err}");
            }
        }
    }
    Ok(())
}

pub fn adapter_status_for_summary(summary: &mut ProductConfigSummary) {
    summary.telegram.adapter_running = adapter_is_running("telegram");
    summary.discord.adapter_running = adapter_is_running("discord");
    summary.slack.adapter_running = adapter_is_running("slack");
    summary.whatsapp.adapter_running = adapter_is_running("whatsapp");
}

#[tauri::command]
pub fn messaging_sync_adapters(daemon_url: Option<String>) -> Result<(), String> {
    sync_channel_adapters(daemon_url.as_deref())
}
