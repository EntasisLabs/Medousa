use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::turn_parts::TurnPart;

const API_KEY_SERVICE: &str = "medousa.tui";
const API_KEY_ACCOUNT: &str = "api_key";
const DISCORD_BOT_TOKEN_SERVICE: &str = "medousa.discord";
const DISCORD_BOT_TOKEN_ACCOUNT: &str = "bot_token";
const TELEGRAM_BOT_TOKEN_SERVICE: &str = "medousa.telegram";
const TELEGRAM_BOT_TOKEN_ACCOUNT: &str = "bot_token";
const SLACK_BOT_TOKEN_SERVICE: &str = "medousa.slack";
const SLACK_BOT_TOKEN_ACCOUNT: &str = "bot_token";
const SLACK_APP_TOKEN_SERVICE: &str = "medousa.slack";
const SLACK_APP_TOKEN_ACCOUNT: &str = "app_token";
const SURREAL_PASSWORD_SERVICE: &str = "medousa.surreal";
const SURREAL_PASSWORD_ACCOUNT: &str = "password";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_state: Option<String>,
    /// Ordered timeline (P3). Surfaces prefer parts; content + tool_names remain for compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<TurnPart>>,
    /// Compact tool-history slice for cross-turn continuity (Phase 8A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_summary: Option<crate::turn_slice::TurnSliceSummary>,
}

impl ConversationTurn {
    /// Legacy constructor without structured timeline parts (TUI, tests, channel adapters).
    pub fn plain(
        role: impl Into<String>,
        content: String,
        timestamp: DateTime<Utc>,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) -> Self {
        Self {
            role: role.into(),
            content,
            timestamp,
            tool_names,
            answer_state,
            parts: None,
            slice_summary: None,
        }
    }
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
    pub host_bus_max_tool_rounds: Option<usize>,
    pub host_turn_bus_mode: Option<String>,
    pub activation_tool_intent_max_rounds: Option<usize>,
    pub activation_short_turn_max_tool_rounds: Option<usize>,
    pub continuation_max_tool_rounds: Option<usize>,
    pub max_text_only_stuck_continues: Option<usize>,
    pub classifier_restricted_max_tool_rounds: Option<usize>,
    pub thinking_capture: Option<bool>,
    #[serde(default)]
    pub stasis_otel_enabled: Option<bool>,
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
    /// Preferred web search provider for `cognition_web_search` (e.g. duckduckgo, tavily).
    pub web_search_preferred_provider: Option<String>,
    /// When false, only the preferred provider binding is tried.
    pub web_search_try_fallbacks: Option<bool>,
    pub surreal_endpoint: Option<String>,
    pub surreal_username: Option<String>,
    pub surreal_password: Option<String>,
    pub surreal_namespace: Option<String>,
    pub surreal_database: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistorySummary {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub turns: usize,
    pub verification_runs: usize,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub last_verification_timestamp: Option<DateTime<Utc>>,
    pub last_verification_confidence: Option<f32>,
    pub last_verification_coverage: Option<f32>,
    pub last_verification_verified: Option<bool>,
    pub preview: String,
}

impl SessionHistorySummary {
    pub fn without_verification_fields(mut self) -> Self {
        self.verification_runs = 0;
        self.last_verification_timestamp = None;
        self.last_verification_confidence = None;
        self.last_verification_coverage = None;
        self.last_verification_verified = None;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStorageBackend {
    KeychainActive,
    KeychainReady,
    FileFallbackActive,
    FileFallbackReady,
}

pub(crate) fn medousa_data_dir() -> PathBuf {
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

fn discord_bot_token_secret_path() -> PathBuf {
    medousa_data_dir().join("secrets").join("discord_bot_token")
}

fn telegram_bot_token_secret_path() -> PathBuf {
    medousa_data_dir().join("secrets").join("telegram_bot_token")
}

fn slack_bot_token_secret_path() -> PathBuf {
    medousa_data_dir().join("secrets").join("slack_bot_token")
}

fn slack_app_token_secret_path() -> PathBuf {
    medousa_data_dir().join("secrets").join("slack_app_token")
}

fn api_key_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(API_KEY_SERVICE, API_KEY_ACCOUNT)
}

fn discord_bot_token_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(DISCORD_BOT_TOKEN_SERVICE, DISCORD_BOT_TOKEN_ACCOUNT)
}

fn telegram_bot_token_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(TELEGRAM_BOT_TOKEN_SERVICE, TELEGRAM_BOT_TOKEN_ACCOUNT)
}

fn slack_bot_token_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(SLACK_BOT_TOKEN_SERVICE, SLACK_BOT_TOKEN_ACCOUNT)
}

fn slack_app_token_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(SLACK_APP_TOKEN_SERVICE, SLACK_APP_TOKEN_ACCOUNT)
}

fn file_api_key() -> Option<String> {
    std::fs::read_to_string(api_key_secret_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn file_discord_bot_token() -> Option<String> {
    std::fs::read_to_string(discord_bot_token_secret_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn file_telegram_bot_token() -> Option<String> {
    std::fs::read_to_string(telegram_bot_token_secret_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn file_slack_bot_token() -> Option<String> {
    std::fs::read_to_string(slack_bot_token_secret_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn file_slack_app_token() -> Option<String> {
    std::fs::read_to_string(slack_app_token_secret_path())
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
        if path.ends_with("api_key")
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
        .ok()
        .and_then(|raw| serde_json::from_str::<TuiDefaults>(&raw).ok())
        .unwrap_or_default()
}

fn surreal_password_secret_path() -> PathBuf {
    medousa_data_dir().join("surreal_password")
}

fn surreal_password_keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(SURREAL_PASSWORD_SERVICE, SURREAL_PASSWORD_ACCOUNT)
}

fn file_surreal_password() -> Option<String> {
    std::fs::read_to_string(surreal_password_secret_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn load_surreal_password() -> Option<String> {
    if let Ok(entry) = surreal_password_keyring_entry() {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    file_surreal_password()
}

pub fn save_surreal_password(password: Option<&str>) {
    let path = surreal_password_secret_path();

    match password.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = surreal_password_keyring_entry() {
                persisted = entry.set_password(value).is_ok();
            }
            if persisted {
                let _ = std::fs::remove_file(&path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = surreal_password_keyring_entry() {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
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

pub fn load_discord_bot_token() -> Option<String> {
    if let Ok(entry) = discord_bot_token_keyring_entry() {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    file_discord_bot_token()
}

pub fn save_discord_bot_token(token: Option<&str>) {
    let path = discord_bot_token_secret_path();

    match token.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = discord_bot_token_keyring_entry() {
                persisted = entry.set_password(value).is_ok();
            }

            if persisted {
                let _ = std::fs::remove_file(path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = discord_bot_token_keyring_entry() {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn load_telegram_bot_token() -> Option<String> {
    if let Ok(entry) = telegram_bot_token_keyring_entry() {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    file_telegram_bot_token()
}

pub fn save_telegram_bot_token(token: Option<&str>) {
    let path = telegram_bot_token_secret_path();

    match token.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = telegram_bot_token_keyring_entry() {
                persisted = entry.set_password(value).is_ok();
            }

            if persisted {
                let _ = std::fs::remove_file(path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = telegram_bot_token_keyring_entry() {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn load_slack_bot_token() -> Option<String> {
    if let Ok(entry) = slack_bot_token_keyring_entry() {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    file_slack_bot_token()
}

pub fn save_slack_bot_token(token: Option<&str>) {
    let path = slack_bot_token_secret_path();

    match token.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = slack_bot_token_keyring_entry() {
                persisted = entry.set_password(value).is_ok();
            }

            if persisted {
                let _ = std::fs::remove_file(path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = slack_bot_token_keyring_entry() {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn load_slack_app_token() -> Option<String> {
    if let Ok(entry) = slack_app_token_keyring_entry() {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    file_slack_app_token()
}

pub fn save_slack_app_token(token: Option<&str>) {
    let path = slack_app_token_secret_path();

    match token.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = slack_app_token_keyring_entry() {
                persisted = entry.set_password(value).is_ok();
            }

            if persisted {
                let _ = std::fs::remove_file(path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = slack_app_token_keyring_entry() {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(crate) fn file_load_history(session_id: &str) -> Vec<ConversationTurn> {
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

pub(crate) fn file_append_turn(session_id: &str, turn: &ConversationTurn) {
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

/// One-time backfill helper — loads full history per session. Not for list API hot path.
pub(crate) fn file_build_history_summaries_from_files(limit: usize) -> Vec<SessionHistorySummary> {
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
            let turns = file_load_history(&session_id);
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
                display_name: None,
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

// Public API delegating to the configured session store.
pub fn load_history(session_id: &str) -> Vec<ConversationTurn> {
    crate::session_store::get_session_store().load_history(session_id)
}

pub fn append_turn(session_id: &str, turn: &ConversationTurn) {
    append_turn_with_scratch(session_id, turn, None)
}

pub fn append_turn_with_scratch(
    session_id: &str,
    turn: &ConversationTurn,
    scratch: Option<&crate::agent_runtime::turn_context::TurnScratchpad>,
) {
    let enriched = crate::turn_slice::ensure_turn_slice_summary(turn, scratch);
    crate::session_store::get_session_store().append_turn(session_id, &enriched);
}

pub fn list_history_sessions(limit: usize) -> Vec<SessionHistorySummary> {
    let limit = limit.max(1);
    let mut sessions = crate::session_catalog::list_sessions(limit);

    if sessions.is_empty() {
        crate::session_catalog::ensure_catalog_populated(limit);
        sessions = crate::session_catalog::list_sessions(limit);
    }

    if sessions.is_empty() && crate::session_store::has_persisted_sessions() {
        sessions = crate::session_store::build_backfill_summaries(limit);
    }

    let mut seen: std::collections::HashSet<String> = sessions
        .iter()
        .map(|item| item.session_id.clone())
        .collect();

    for (session_id, display_name) in crate::session_meta_store::list_session_display_names(limit) {
        if !seen.insert(session_id.clone()) {
            continue;
        }
        if crate::session_catalog::get_summary(&session_id).is_some() {
            continue;
        }
        crate::session_catalog::ensure_named_session(&session_id, Some(display_name.clone()));
        sessions.push(SessionHistorySummary {
            session_id,
            display_name: Some(display_name),
            turns: 0,
            verification_runs: 0,
            last_timestamp: None,
            last_verification_timestamp: None,
            last_verification_confidence: None,
            last_verification_coverage: None,
            last_verification_verified: None,
            preview: "(named session)".to_string(),
        });
    }

    enrich_session_summaries(&mut sessions);
    sessions.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
    sessions.truncate(limit);
    sessions
}

pub fn set_session_display_name(session_id: &str, display_name: &str) -> Result<(), String> {
    let result = crate::session_meta_store::set_session_display_name(session_id, display_name);
    if result.is_ok() {
        crate::session_catalog::set_display_name(session_id, display_name);
    }
    result
}

pub fn get_session_display_name(session_id: &str) -> Option<String> {
    crate::session_meta_store::get_session_display_name(session_id)
}

pub fn enrich_session_summaries(sessions: &mut [SessionHistorySummary]) {
    let ids: Vec<String> = sessions.iter().map(|s| s.session_id.clone()).collect();
    let names = crate::session_meta_store::load_session_display_names(&ids);
    for session in sessions.iter_mut() {
        if session.display_name.is_none() {
            session.display_name = names.get(&session.session_id).cloned();
        }
    }
}

pub fn session_turn_count(session_id: &str) -> usize {
    crate::session_catalog::turn_count(session_id)
        .unwrap_or_else(|| load_history(session_id).len())
}

/// Resolve `/history <target>`: full id, id prefix, or global display name (unique).
pub fn resolve_history_resume_target(target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    if let Some(session_id) = crate::session_meta_store::find_session_id_by_display_name(target) {
        return Some(session_id);
    }

    if crate::session_catalog::get_summary(target).is_some() {
        return Some(target.to_string());
    }
    if !load_history(target).is_empty() {
        return Some(target.to_string());
    }

    if let Some(session_id) = crate::session_catalog::find_unique_session_id_by_prefix(target) {
        return Some(session_id);
    }

    if let Some(session_id) =
        crate::session_catalog::find_unique_session_id_by_display_name_case_insensitive(target)
    {
        return Some(session_id);
    }

    None
}

pub fn format_session_history_label(session_id: &str, display_name: Option<&str>) -> String {
    let id_short: String = session_id.chars().take(8).collect();
    match display_name.filter(|name| !name.trim().is_empty()) {
        Some(name) => format!("{name} ({id_short})"),
        None => format!("{id_short}…"),
    }
}
