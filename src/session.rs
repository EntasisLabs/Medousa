use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub use medousa_types::session::{ConversationTurn, SessionHistorySummary, TuiDefaults};


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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyStorageBackend {
    KeychainActive,
    KeychainReady,
    FileFallbackActive,
    FileFallbackReady,
}

pub(crate) fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
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

fn provider_api_key_keyring_entry(provider: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new("medousa.providers", provider)
}

fn provider_api_key_secret_path(provider: &str) -> PathBuf {
    medousa_data_dir()
        .join("secrets")
        .join(format!("api_key_{}", provider.trim().to_ascii_lowercase()))
}

fn file_provider_api_key(provider: &str) -> Option<String> {
    std::fs::read_to_string(provider_api_key_secret_path(provider))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
    let mut defaults = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<TuiDefaults>(&raw).ok())
        .unwrap_or_default();
    crate::inference_profiles::normalize_tui_defaults(&mut defaults);
    defaults
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
    if let Ok(entry) = surreal_password_keyring_entry()
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
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
    let mut normalized = defaults.clone();
    crate::inference_profiles::normalize_tui_defaults(&mut normalized);
    crate::inference_profiles::sync_top_level_from_main(&mut normalized);
    if let Ok(json) = serde_json::to_string_pretty(&normalized) {
        let _ = atomic_write(&path, json.as_bytes());
    }
}

pub fn load_tui_defaults_value() -> serde_json::Value {
    let path = tui_defaults_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Merge incoming JSON (may include client-only keys) with normalized `TuiDefaults` fields.
pub fn save_tui_defaults_merged(incoming: serde_json::Value) -> Result<TuiDefaults, String> {
    let mut typed: TuiDefaults =
        serde_json::from_value(incoming.clone()).unwrap_or_default();
    crate::inference_profiles::normalize_tui_defaults(&mut typed);
    crate::inference_profiles::sync_top_level_from_main(&mut typed);

    let mut merged = incoming;
    if let serde_json::Value::Object(ref mut obj) = merged {
        let normalized =
            serde_json::to_value(&typed).map_err(|err| format!("encode defaults: {err}"))?;
        if let serde_json::Value::Object(norm_obj) = normalized {
            for (key, value) in norm_obj {
                obj.insert(key, value);
            }
        }
    }

    let path = tui_defaults_path();
    let json =
        serde_json::to_string_pretty(&merged).map_err(|err| format!("serialize defaults: {err}"))?;
    atomic_write(&path, json.as_bytes()).map_err(|err| err.to_string())?;
    Ok(typed)
}

pub fn load_tui_api_key() -> Option<String> {
    if let Ok(entry) = api_key_keyring_entry()
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

    file_api_key()
}

/// Per-provider API key (Phase 3). Falls back to legacy workshop key when provider matches main.
pub fn load_provider_api_key(provider: &str) -> Option<String> {
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return None;
    }
    if provider == "ollama" || provider == "local" || provider == "lmstudio" || provider == "lm-studio" {
        return None;
    }

    if let Ok(entry) = provider_api_key_keyring_entry(&provider)
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    if let Some(value) = file_provider_api_key(&provider) {
        return Some(value);
    }

    let defaults = load_tui_defaults();
    let main_provider = crate::resolve_llm_provider(defaults.provider.as_deref())
        .trim()
        .to_ascii_lowercase();
    if provider == main_provider {
        return load_tui_api_key();
    }

    provider_api_key_from_env(&provider)
}

pub fn provider_api_key_configured(provider: &str) -> bool {
    load_provider_api_key(provider).is_some()
}

pub fn save_provider_api_key(provider: &str, api_key: Option<&str>) {
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return;
    }
    let path = provider_api_key_secret_path(&provider);
    match api_key.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            let mut persisted = false;
            if let Ok(entry) = provider_api_key_keyring_entry(&provider) {
                persisted = entry.set_password(value).is_ok();
            }
            if persisted {
                let _ = std::fs::remove_file(&path);
            } else {
                let _ = atomic_write(&path, value.as_bytes());
            }
        }
        None => {
            if let Ok(entry) = provider_api_key_keyring_entry(&provider) {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

fn provider_api_key_from_env(provider: &str) -> Option<String> {
    let normalized = provider.trim().to_ascii_uppercase().replace('-', "_");
    for key in [
        format!("MEDOUSA_{normalized}_API_KEY"),
        format!("STASIS_{normalized}_API_KEY"),
    ] {
        if let Ok(value) = std::env::var(&key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let canonical = match provider {
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "google" | "gemini" | "google-gemini" => "GEMINI_API_KEY",
        "groq" => "GROQ_API_KEY",
        "xai" => "XAI_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "cohere" => "COHERE_API_KEY",
        "perplexity" => "PERPLEXITY_API_KEY",
        "together" => "TOGETHER_API_KEY",
        "fireworks" => "FIREWORKS_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        _ => return None,
    };
    std::env::var(canonical)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
    if let Ok(entry) = discord_bot_token_keyring_entry()
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
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
    for key in [
        "MEDOUSA_TELEGRAM_BOT_TOKEN",
        "MEDOUSA_TELEGRAM_TOKEN",
        "TELOXIDE_TOKEN",
    ] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Ok(entry) = telegram_bot_token_keyring_entry()
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
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
    if let Ok(entry) = slack_bot_token_keyring_entry()
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
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
    if let Ok(entry) = slack_app_token_keyring_entry()
        && let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
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
        .map_while(Result::ok)
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

    sessions.sort_by_key(|b| std::cmp::Reverse(b.1));

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
    list_history_sessions_page(limit, None, None).sessions
}

pub fn list_history_sessions_page(
    limit: usize,
    query: Option<&str>,
    cursor: Option<&str>,
) -> crate::session_catalog::SessionListPage {
    let limit = limit.max(1);
    let searching = query.is_some() || cursor.is_some();

    crate::session_catalog::ensure_catalog_populated(limit.max(500));
    let active_profile = crate::user_profiles::resolve_workshop_identity_user_id();
    let mut page =
        crate::session_catalog::list_sessions_page(limit, query, cursor, Some(&active_profile));

    if searching {
        enrich_session_summaries(&mut page.sessions);
        return page;
    }

    if page.sessions.is_empty() {
        page.sessions = crate::session_catalog::list_sessions(limit);
    }

    if page.sessions.is_empty() && crate::session_store::has_persisted_sessions() {
        page.sessions = crate::session_store::build_backfill_summaries(limit);
    }

    let mut seen: std::collections::HashSet<String> = page
        .sessions
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
        page.sessions.push(SessionHistorySummary {
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

    enrich_session_summaries(&mut page.sessions);
    page.sessions.sort_by_key(|b| std::cmp::Reverse(b.last_timestamp));
    page.sessions.truncate(limit);
    page
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
