use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const API_KEY_SERVICE: &str = "medousa.tui";
const API_KEY_ACCOUNT: &str = "api_key";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TuiDefaults {
    pub backend: Option<String>,
    pub theme_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub env_overrides: Option<String>,
    pub allowed_modules: Option<Vec<String>>,
    pub tool_call_mode: Option<String>,
    pub max_tool_rounds: Option<usize>,
    pub thinking_capture: Option<bool>,
    pub thinking_max_lines: Option<usize>,
    pub activation_direct_answer_max_prompt_chars: Option<usize>,
    pub activation_long_session_turn_threshold: Option<usize>,
    pub activation_long_session_max_prompt_chars: Option<usize>,
    pub slice_hot_window_turns: Option<usize>,
    pub slice_cold_window_turns: Option<usize>,
    pub retry_runtime_max_retries: Option<usize>,
    pub retry_runtime_max_rounds: Option<usize>,
    pub verifier_min_citation_coverage: Option<f32>,
    pub verifier_min_avg_support_strength: Option<f32>,
    pub verifier_min_supported_claim_ratio: Option<f32>,
    pub verifier_min_claim_support_strength: Option<f32>,
    pub response_depth_mode: Option<String>,
    pub stage_routing: Option<crate::stage_routing::StageRoutingMatrix>,
    pub command_usage_counts: Option<std::collections::HashMap<String, u64>>,
}

#[derive(Debug, Clone)]
pub struct SessionHistorySummary {
    pub session_id: String,
    pub turns: usize,
    pub verification_runs: usize,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub last_verification_timestamp: Option<DateTime<Utc>>,
    pub last_verification_confidence: Option<f32>,
    pub last_verification_coverage: Option<f32>,
    pub last_verification_verified: Option<bool>,
    pub preview: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStorageBackend {
    KeychainActive,
    KeychainReady,
    FileFallbackActive,
    FileFallbackReady,
}

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

pub fn history_path(session_id: &str) -> PathBuf {
    medousa_data_dir()
        .join("history")
        .join(format!("{session_id}.jsonl"))
}

fn last_session_path() -> PathBuf {
    medousa_data_dir().join("last_session")
}

fn tui_defaults_path() -> PathBuf {
    medousa_data_dir().join("tui_defaults.json")
}

fn api_key_secret_path() -> PathBuf {
    medousa_data_dir().join("secrets").join("api_key")
}

fn api_key_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(API_KEY_SERVICE, API_KEY_ACCOUNT)
}

fn file_api_key() -> Option<String> {
    std::fs::read_to_string(api_key_secret_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn detect_tui_api_key_storage_backend() -> ApiKeyStorageBackend {
    if let Ok(entry) = api_key_keyring_entry() {
        if entry
            .get_password()
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
        {
            return ApiKeyStorageBackend::KeychainActive;
        }
        if file_api_key().is_some() {
            return ApiKeyStorageBackend::FileFallbackActive;
        }
        return ApiKeyStorageBackend::KeychainReady;
    }

    if file_api_key().is_some() {
        ApiKeyStorageBackend::FileFallbackActive
    } else {
        ApiKeyStorageBackend::FileFallbackReady
    }
}

fn atomic_write(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
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
        if path.ends_with("api_key") {
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
        .ok()
        .and_then(|raw| serde_json::from_str::<TuiDefaults>(&raw).ok())
        .unwrap_or_default()
}

pub fn save_tui_defaults(defaults: &TuiDefaults) {
    let path = tui_defaults_path();
    if let Ok(json) = serde_json::to_string_pretty(defaults) {
        let _ = atomic_write(&path, json.as_bytes());
    }
}

pub fn load_tui_api_key() -> Option<String> {
    if let Ok(entry) = api_key_keyring_entry() {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    file_api_key()
}

pub fn save_tui_api_key(api_key: Option<&str>) {
    let path = api_key_secret_path();

    match api_key.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = api_key_keyring_entry() {
                persisted = entry.set_password(value).is_ok();
            }

            if persisted {
                let _ = std::fs::remove_file(path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = api_key_keyring_entry() {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn load_history(session_id: &str) -> Vec<ConversationTurn> {
    let path = history_path(session_id);
    let Ok(file) = std::fs::File::open(&path) else {
        return Vec::new();
    };
    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

pub fn append_turn(session_id: &str, turn: &ConversationTurn) {
    let path = history_path(session_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    if let Ok(line) = serde_json::to_string(turn) {
        let _ = writeln!(file, "{line}");
    }
}

pub fn list_history_sessions(limit: usize) -> Vec<SessionHistorySummary> {
    let history_dir = medousa_data_dir().join("history");
    let Ok(entries) = std::fs::read_dir(history_dir) else {
        return Vec::new();
    };

    let mut sessions = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if ext != Some("jsonl") {
                return None;
            }

            let session_id = path.file_stem()?.to_string_lossy().to_string();
            let metadata = entry.metadata().ok();
            let modified = metadata.and_then(|m| m.modified().ok());
            Some((session_id, modified))
        })
        .collect::<Vec<_>>();

    sessions.sort_by(|a, b| b.1.cmp(&a.1));

    sessions
        .into_iter()
        .take(limit)
        .map(|(session_id, _)| {
            let turns = load_history(&session_id);
            let verifications =
                crate::verification_store::list_verifications(&session_id, usize::MAX);
            let last_timestamp = turns.last().map(|t| t.timestamp);
            let last_verification_timestamp = verifications.first().map(|v| v.created_at_utc);
            let latest_verification =
                crate::verification_store::find_verification(&session_id, None);
            let last_verification_confidence = latest_verification
                .as_ref()
                .map(|run| run.record.confidence_score);
            let last_verification_coverage = latest_verification
                .as_ref()
                .map(|run| run.report.citation_coverage);
            let last_verification_verified = latest_verification
                .as_ref()
                .map(|run| run.record.is_verified);
            let preview = turns
                .iter()
                .rev()
                .find(|t| !t.content.trim().is_empty())
                .and_then(|t| t.content.lines().next())
                .unwrap_or("(empty session)")
                .chars()
                .take(72)
                .collect::<String>();

            SessionHistorySummary {
                session_id,
                turns: turns.len(),
                verification_runs: verifications.len(),
                last_timestamp,
                last_verification_timestamp,
                last_verification_confidence,
                last_verification_coverage,
                last_verification_verified,
                preview,
            }
        })
        .collect()
}
