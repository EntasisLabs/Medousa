//! TUI Connection helpers — workshop-scoped layout keys + recent / Home registry.
//!
//! Aligns with Home's Settings → **Connection** (store id may still be `basement`):
//! workshops live in `{dataDir}/workshops.json`. The TUI reads that registry
//! read-only and scopes pane layout by workshop id (or URL hash).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use medousa_types::daemon_api::DEFAULT_DAEMON_URL;

const RECENT_FILE: &str = "tui_recent_daemons_v1.json";
const ACTIVE_FILE: &str = "tui_active_connection_v1.json";
const WORKSHOPS_FILE: &str = "workshops.json";
const PERSONAL_WORKSHOP_ID: &str = "personal";
const MAX_RECENT: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveConnection {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workshop_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentDaemon {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workshop_id: Option<String>,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RecentFile {
    #[serde(default)]
    entries: Vec<RecentDaemon>,
}

/// Thin read of Home's workshops.json (camelCase).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopRegistryView {
    #[serde(default)]
    pub active_workshop_id: String,
    #[serde(default)]
    pub workshops: Vec<WorkshopServerView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopServerView {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionChoice {
    pub url: String,
    pub label: String,
    pub workshop_id: Option<String>,
    pub source: ConnectionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSource {
    Local,
    HomeRegistry,
    Recent,
    /// mDNS LAN discovery (`_medousa._tcp`).
    Lan,
    Custom,
}

pub fn normalize_daemon_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return DEFAULT_DAEMON_URL.to_string();
    }
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

pub fn is_default_local_url(url: &str) -> bool {
    normalize_daemon_url(url) == DEFAULT_DAEMON_URL
}

/// Scope key for pane-layout persistence (Home v4 analogue).
///
/// Prefer a Home registry workshop id when the URL matches; otherwise a stable
/// short hash of the normalized URL. Default local URL → `personal`.
pub fn workshop_scope_key(url: &str, registry: Option<&WorkshopRegistryView>) -> String {
    let normalized = normalize_daemon_url(url);
    if let Some(reg) = registry
        && let Some(match_id) = reg
            .workshops
            .iter()
            .find(|w| normalize_daemon_url(&w.url) == normalized)
            .map(|w| w.id.clone())
    {
        return match_id;
    }
    if is_default_local_url(&normalized) {
        return PERSONAL_WORKSHOP_ID.to_string();
    }
    let digest = Sha256::digest(normalized.as_bytes());
    format!("url-{}", hex_prefix(&digest, 12))
}

fn hex_prefix(bytes: &[u8], n: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
        if out.len() >= n {
            break;
        }
    }
    out.truncate(n);
    out
}

pub fn workshops_registry_path() -> PathBuf {
    crate::paths::medousa_data_dir().join(WORKSHOPS_FILE)
}

pub fn load_workshop_registry() -> Option<WorkshopRegistryView> {
    let raw = fs::read_to_string(workshops_registry_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn recent_daemons_path() -> PathBuf {
    crate::paths::medousa_data_dir().join(RECENT_FILE)
}

pub fn active_connection_path() -> PathBuf {
    crate::paths::medousa_data_dir().join(ACTIVE_FILE)
}

pub fn load_active_connection() -> Option<ActiveConnection> {
    let raw = fs::read_to_string(active_connection_path()).ok()?;
    let mut active: ActiveConnection = serde_json::from_str(&raw).ok()?;
    active.url = normalize_daemon_url(&active.url);
    Some(active)
}

pub fn save_active_connection(active: &ActiveConnection) -> std::io::Result<()> {
    let path = active_connection_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(active)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn load_recent_daemons() -> Vec<RecentDaemon> {
    let Ok(raw) = fs::read_to_string(recent_daemons_path()) else {
        return Vec::new();
    };
    serde_json::from_str::<RecentFile>(&raw)
        .map(|f| f.entries)
        .unwrap_or_default()
}

pub fn remember_daemon(
    url: &str,
    label: Option<&str>,
    workshop_id: Option<&str>,
) -> std::io::Result<()> {
    let url = normalize_daemon_url(url);
    let now = chrono::Utc::now().to_rfc3339();
    let mut entries = load_recent_daemons();
    entries.retain(|e| normalize_daemon_url(&e.url) != url);
    entries.insert(
        0,
        RecentDaemon {
            url: url.clone(),
            label: label.map(str::to_string).filter(|s| !s.is_empty()),
            workshop_id: workshop_id.map(str::to_string).filter(|s| !s.is_empty()),
            last_used_at: now.clone(),
        },
    );
    entries.truncate(MAX_RECENT);
    let path = recent_daemons_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(&RecentFile { entries })
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw)?;
    fs::rename(tmp, path)?;

    let _ = save_active_connection(&ActiveConnection {
        url,
        label: label.map(str::to_string),
        workshop_id: workshop_id.map(str::to_string),
        updated_at: Some(now),
    });
    Ok(())
}

/// Resolve the daemon URL for TUI startup: CLI/env already applied by caller as
/// `explicit`; otherwise last active connection; else default local.
pub fn resolve_tui_daemon_url(explicit: Option<&str>) -> String {
    if let Some(url) = explicit.map(str::trim).filter(|u| !u.is_empty()) {
        return normalize_daemon_url(url);
    }
    if let Ok(env) = std::env::var("MEDOUSA_DAEMON_URL") {
        let env = env.trim();
        if !env.is_empty() {
            return normalize_daemon_url(env);
        }
    }
    if let Ok(env) = std::env::var("STASIS_DAEMON_URL") {
        let env = env.trim();
        if !env.is_empty() {
            return normalize_daemon_url(env);
        }
    }
    if let Some(active) = load_active_connection() {
        return active.url;
    }
    DEFAULT_DAEMON_URL.to_string()
}

pub fn label_for_url(url: &str, registry: Option<&WorkshopRegistryView>) -> String {
    let normalized = normalize_daemon_url(url);
    if let Some(reg) = registry
        && let Some(w) = reg
            .workshops
            .iter()
            .find(|w| normalize_daemon_url(&w.url) == normalized)
    {
        return w.label.clone();
    }
    if is_default_local_url(&normalized) {
        return "Local".to_string();
    }
    normalized
}

/// Build picker rows: Local, Home registry workshops, then recent (deduped).
pub fn connection_choices() -> Vec<ConnectionChoice> {
    let registry = load_workshop_registry();
    let mut choices = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let local_url = DEFAULT_DAEMON_URL.to_string();
    seen.insert(normalize_daemon_url(&local_url));
    choices.push(ConnectionChoice {
        url: local_url,
        label: registry
            .as_ref()
            .and_then(|r| {
                r.workshops
                    .iter()
                    .find(|w| w.id == PERSONAL_WORKSHOP_ID)
                    .map(|w| w.label.clone())
            })
            .unwrap_or_else(|| "Local".to_string()),
        workshop_id: Some(PERSONAL_WORKSHOP_ID.to_string()),
        source: ConnectionSource::Local,
    });

    if let Some(reg) = registry.as_ref() {
        for workshop in &reg.workshops {
            let url = normalize_daemon_url(&workshop.url);
            if !seen.insert(url.clone()) {
                continue;
            }
            // Skip peer-only inbox rows without a usable engine URL shape — still
            // list portal/local/paired/peer if they have http(s).
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                continue;
            }
            choices.push(ConnectionChoice {
                url,
                label: workshop.label.clone(),
                workshop_id: Some(workshop.id.clone()),
                source: ConnectionSource::HomeRegistry,
            });
        }
    }

    for recent in load_recent_daemons() {
        let url = normalize_daemon_url(&recent.url);
        if !seen.insert(url.clone()) {
            continue;
        }
        choices.push(ConnectionChoice {
            url,
            label: recent
                .label
                .clone()
                .unwrap_or_else(|| label_for_url(&recent.url, registry.as_ref())),
            workshop_id: recent.workshop_id.clone(),
            source: ConnectionSource::Recent,
        });
    }

    choices
}

pub fn source_label(source: ConnectionSource) -> &'static str {
    match source {
        ConnectionSource::Local => "local",
        ConnectionSource::HomeRegistry => "home",
        ConnectionSource::Recent => "recent",
        ConnectionSource::Lan => "lan",
        ConnectionSource::Custom => "url",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_personal_for_default_local() {
        assert_eq!(
            workshop_scope_key(DEFAULT_DAEMON_URL, None),
            PERSONAL_WORKSHOP_ID
        );
        assert_eq!(
            workshop_scope_key("http://127.0.0.1:7419/", None),
            PERSONAL_WORKSHOP_ID
        );
    }

    #[test]
    fn scope_uses_registry_id_when_url_matches() {
        let reg = WorkshopRegistryView {
            active_workshop_id: "paired-abc".into(),
            workshops: vec![WorkshopServerView {
                id: "paired-abc".into(),
                label: "Studio".into(),
                kind: "portal".into(),
                url: "http://192.168.1.10:7419".into(),
            }],
        };
        assert_eq!(
            workshop_scope_key("http://192.168.1.10:7419/", Some(&reg)),
            "paired-abc"
        );
    }

    #[test]
    fn scope_hashes_unknown_url() {
        let a = workshop_scope_key("http://example.test:9000", None);
        let b = workshop_scope_key("http://example.test:9000/", None);
        assert_eq!(a, b);
        assert!(a.starts_with("url-"));
        assert_ne!(a, PERSONAL_WORKSHOP_ID);
    }

    #[test]
    fn remember_and_active_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _data_dir = crate::paths::scoped_test_data_dir(dir.path());

        remember_daemon("http://10.0.0.2:7419", Some("Lab"), Some("paired-lab"))
            .expect("remember");
        let recent = load_recent_daemons();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].label.as_deref(), Some("Lab"));
        let active = load_active_connection().expect("active");
        assert_eq!(active.url, "http://10.0.0.2:7419");
        assert_eq!(active.workshop_id.as_deref(), Some("paired-lab"));

    }
}
