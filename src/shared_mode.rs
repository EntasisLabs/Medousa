//! Opt-in Shared mode — profiles as members on one team brain.
//!
//! Personal installs stay on [`DaemonWorkshopMode::Personal`]. Enabling Shared
//! bootstraps `root` (admin) and `general` (org agent persona for shared rooms).

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock as StdRwLock};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::session::medousa_data_dir;
use crate::user_profiles::{
    UserProfileRegistry, format_profile_id, init_workshop_profile_registry,
};

const SHARED_MODE_FILENAME: &str = "shared_mode.json";
pub const ROOT_PROFILE_SLUG: &str = "root";
pub const GENERAL_PROFILE_SLUG: &str = "general";
pub const ROOT_PROFILE_DISPLAY_NAME: &str = "Root";
pub const GENERAL_PROFILE_DISPLAY_NAME: &str = "General";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DaemonWorkshopMode {
    #[default]
    Personal,
    Shared,
}

impl DaemonWorkshopMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Shared => "shared",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "shared" => Self::Shared,
            _ => Self::Personal,
        }
    }

    pub fn is_shared(self) -> bool {
        matches!(self, Self::Shared)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SharedModeConfig {
    pub mode: DaemonWorkshopMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_at: Option<chrono::DateTime<Utc>>,
}

impl Default for SharedModeConfig {
    fn default() -> Self {
        Self {
            mode: DaemonWorkshopMode::Personal,
            enabled_at: None,
        }
    }
}

fn config_path() -> PathBuf {
    medousa_data_dir().join(SHARED_MODE_FILENAME)
}

static SHARED_MODE_CONFIG: OnceLock<StdRwLock<SharedModeConfig>> = OnceLock::new();

fn config_slot() -> &'static StdRwLock<SharedModeConfig> {
    SHARED_MODE_CONFIG.get_or_init(|| StdRwLock::new(load_config_from_disk()))
}

fn load_config_from_disk() -> SharedModeConfig {
    let path = config_path();
    if !path.is_file() {
        return SharedModeConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => SharedModeConfig::default(),
    }
}

fn persist_config(config: &SharedModeConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create shared mode dir {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(config).context("serialize shared_mode.json")?;
    crate::session::atomic_write(&path, raw.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Load config into the process cache (call once at daemon boot).
pub fn init_shared_mode() {
    let loaded = load_config_from_disk();
    *config_slot().write().expect("shared mode lock") = loaded;
}

pub fn current_mode() -> DaemonWorkshopMode {
    config_slot()
        .read()
        .expect("shared mode lock")
        .mode
}

pub fn is_shared_mode() -> bool {
    current_mode().is_shared()
}

pub fn root_profile_id() -> String {
    format_profile_id(ROOT_PROFILE_SLUG)
}

pub fn general_profile_id() -> String {
    format_profile_id(GENERAL_PROFILE_SLUG)
}

/// Enable Shared mode and ensure `root` + `general` profiles exist.
pub fn enable_shared_mode(registry: Arc<StdRwLock<UserProfileRegistry>>) -> Result<SharedModeConfig> {
    {
        let mut reg = registry.write().expect("profile registry lock");
        ensure_shared_bootstrap_profiles(&mut reg)?;
    }
    init_workshop_profile_registry(Arc::clone(&registry));

    let config = SharedModeConfig {
        mode: DaemonWorkshopMode::Shared,
        enabled_at: Some(Utc::now()),
    };
    persist_config(&config)?;
    *config_slot().write().expect("shared mode lock") = config.clone();
    Ok(config)
}

/// Disable Shared mode (keeps profiles; stops treating the daemon as org-mode).
pub fn disable_shared_mode() -> Result<SharedModeConfig> {
    let config = SharedModeConfig {
        mode: DaemonWorkshopMode::Personal,
        enabled_at: None,
    };
    persist_config(&config)?;
    *config_slot().write().expect("shared mode lock") = config.clone();
    Ok(config)
}

fn profile_exists(registry: &UserProfileRegistry, profile_id: &str) -> bool {
    registry
        .list_profiles()
        .iter()
        .any(|profile| profile.profile_id == profile_id)
}

fn ensure_shared_bootstrap_profiles(registry: &mut UserProfileRegistry) -> Result<()> {
    if !profile_exists(registry, &root_profile_id()) {
        registry
            .create_profile(ROOT_PROFILE_SLUG, ROOT_PROFILE_DISPLAY_NAME)
            .context("create root profile")?;
    }
    if !profile_exists(registry, &general_profile_id()) {
        registry
            .create_profile(GENERAL_PROFILE_SLUG, GENERAL_PROFILE_DISPLAY_NAME)
            .context("create general profile")?;
    }
    registry
        .set_active_profile(&root_profile_id())
        .context("set active root profile")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parse() {
        assert_eq!(DaemonWorkshopMode::parse("shared"), DaemonWorkshopMode::Shared);
        assert_eq!(DaemonWorkshopMode::parse("personal"), DaemonWorkshopMode::Personal);
        assert_eq!(DaemonWorkshopMode::parse(""), DaemonWorkshopMode::Personal);
    }

    #[test]
    fn profile_ids() {
        assert_eq!(root_profile_id(), "user:root");
        assert_eq!(general_profile_id(), "user:general");
    }
}
