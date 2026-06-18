use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPrefs {
    #[serde(default)]
    pub public_bind: bool,
    #[serde(default)]
    pub autostart_enabled: bool,
}

fn prefs_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("connection_prefs.json")
}

pub fn load_connection_prefs() -> ConnectionPrefs {
    let path = prefs_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_connection_prefs(prefs: &ConnectionPrefs) -> Result<(), String> {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let body = serde_json::to_string_pretty(prefs).map_err(|err| err.to_string())?;
    std::fs::write(path, body).map_err(|err| err.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPrefsSummary {
    pub public_bind: bool,
    pub autostart_enabled: bool,
    pub autostart_supported: bool,
}

pub fn connection_prefs_summary() -> ConnectionPrefsSummary {
    let prefs = load_connection_prefs();
    ConnectionPrefsSummary {
        public_bind: prefs.public_bind,
        autostart_enabled: prefs.autostart_enabled,
        autostart_supported: crate::autostart::autostart_supported(),
    }
}

#[tauri::command]
pub fn connection_load_prefs() -> ConnectionPrefsSummary {
    connection_prefs_summary()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSetPublicBindRequest {
    pub enabled: bool,
}

#[tauri::command]
pub async fn connection_set_public_bind(
    request: ConnectionSetPublicBindRequest,
) -> Result<crate::daemon_service::DaemonStartResult, String> {
    let mut prefs = load_connection_prefs();
    prefs.public_bind = request.enabled;
    save_connection_prefs(&prefs)?;
    crate::daemon_service::daemon_restart(Some(crate::daemon_service::DaemonStartRequest {
        private_brain: false,
        public_bind: Some(request.enabled),
    }))
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSetAutostartRequest {
    pub enabled: bool,
}

#[tauri::command]
pub fn connection_set_autostart(request: ConnectionSetAutostartRequest) -> Result<(), String> {
    if request.enabled {
        crate::autostart::install_autostart()?;
    } else {
        crate::autostart::remove_autostart()?;
    }

    let mut prefs = load_connection_prefs();
    prefs.autostart_enabled = request.enabled;
    save_connection_prefs(&prefs)
}
