pub use medousa_types::session::{ConversationTurn, SessionHistorySummary, TuiDefaults};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn last_session_path() -> PathBuf {
    medousa_data_dir().join("last_session")
}

fn tui_defaults_path() -> PathBuf {
    medousa_data_dir().join("terminal").join("preferences.json")
}

pub(crate) fn atomic_write(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = path.with_extension(format!("tmp-{ts}"));
    std::fs::write(&temp_path, bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path
            .parent()
            .is_some_and(|parent| parent.ends_with("secrets"))
            || path.ends_with("api_key")
            || path.ends_with("discord_bot_token")
            || path.ends_with("telegram_bot_token")
            || path.ends_with("slack_bot_token")
            || path.ends_with("slack_app_token")
            || path.ends_with("surreal_password")
        {
            let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600));
        }
    }

    std::fs::rename(temp_path, path)?;
    Ok(())
}

pub fn load_last_session_id() -> Option<String> {
    std::fs::read_to_string(last_session_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_last_session_id(session_id: &str) {
    let path = last_session_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, session_id);
}

pub fn load_tui_defaults() -> TuiDefaults {
    let path = tui_defaults_path();
    std::fs::read_to_string(path)
        .or_else(|_| std::fs::read_to_string(medousa_data_dir().join("tui_defaults.json")))
        .ok()
        .and_then(|raw| serde_json::from_str::<TuiDefaults>(&raw).ok())
        .unwrap_or_default()
}

pub fn save_tui_defaults(defaults: &TuiDefaults) {
    let path = tui_defaults_path();
    let local = serde_json::json!({
        "theme_id": defaults.theme_id,
        "command_usage_counts": defaults.command_usage_counts,
    });
    if let Ok(json) = serde_json::to_string_pretty(&local) {
        let _ = atomic_write(&path, json.as_bytes());
    }
}

pub fn format_session_history_label(session_id: &str, display_name: Option<&str>) -> String {
    let id_short: String = session_id.chars().take(8).collect();
    match display_name.filter(|name| !name.trim().is_empty()) {
        Some(name) => format!("{name} ({id_short})"),
        None => format!("{id_short}…"),
    }
}
