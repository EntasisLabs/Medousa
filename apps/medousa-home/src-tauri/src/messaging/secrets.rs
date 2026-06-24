use std::path::PathBuf;

const DISCORD_BOT_TOKEN_SERVICE: &str = "medousa.discord";
const DISCORD_BOT_TOKEN_ACCOUNT: &str = "bot_token";
const TELEGRAM_BOT_TOKEN_SERVICE: &str = "medousa.telegram";
const TELEGRAM_BOT_TOKEN_ACCOUNT: &str = "bot_token";
const SLACK_BOT_TOKEN_SERVICE: &str = "medousa.slack";
const SLACK_BOT_TOKEN_ACCOUNT: &str = "bot_token";
const SLACK_APP_TOKEN_SERVICE: &str = "medousa.slack";
const SLACK_APP_TOKEN_ACCOUNT: &str = "app_token";
const API_KEY_SERVICE: &str = "medousa.tui";
const API_KEY_ACCOUNT: &str = "api_key";
const STT_API_KEY_SERVICE: &str = "medousa.stt";
const STT_API_KEY_ACCOUNT: &str = "api_key";

fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn secret_file_path(name: &str) -> PathBuf {
    medousa_data_dir().join("secrets").join(name)
}

fn keyring_entry(service: &str, account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, account).map_err(|err| err.to_string())
}

fn file_secret(name: &str) -> Option<String> {
    let value = std::fs::read_to_string(secret_file_path(name)).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn load_secret(service: &str, account: &str, file_name: &str) -> bool {
    read_secret_value(service, account, file_name).is_some()
}

fn read_secret_value(service: &str, account: &str, file_name: &str) -> Option<String> {
    if let Ok(entry) = keyring_entry(service, account) {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    file_secret(file_name)
}

pub fn load_secret_value(secret_id: &str) -> Result<Option<String>, String> {
    Ok(match secret_id {
        "telegram_bot_token" => read_secret_value(
            TELEGRAM_BOT_TOKEN_SERVICE,
            TELEGRAM_BOT_TOKEN_ACCOUNT,
            "telegram_bot_token",
        ),
        "discord_bot_token" => read_secret_value(
            DISCORD_BOT_TOKEN_SERVICE,
            DISCORD_BOT_TOKEN_ACCOUNT,
            "discord_bot_token",
        ),
        "slack_bot_token" => read_secret_value(
            SLACK_BOT_TOKEN_SERVICE,
            SLACK_BOT_TOKEN_ACCOUNT,
            "slack_bot_token",
        ),
        "slack_app_token" => read_secret_value(
            SLACK_APP_TOKEN_SERVICE,
            SLACK_APP_TOKEN_ACCOUNT,
            "slack_app_token",
        ),
        "api_key" => read_secret_value(API_KEY_SERVICE, API_KEY_ACCOUNT, "api_key"),
        "stt_api_key" => read_secret_value(STT_API_KEY_SERVICE, STT_API_KEY_ACCOUNT, "stt_api_key"),
        other if other.starts_with("api_key_") => {
            let provider = other.trim_start_matches("api_key_");
            read_secret_value("medousa.providers", provider, other)
        }
        "custom_provider_id" => {
            read_secret_value("medousa.providers", "custom_provider_id", "custom_provider_id")
        }
        other if other.starts_with("base_url_") => {
            read_secret_value("medousa.providers", other, other)
        }
        other => return Err(format!("unknown secret_id '{other}'")),
    })
}

fn save_secret_value(
    service: &str,
    account: &str,
    file_name: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let path = secret_file_path(file_name);
    match value.map(str::trim).filter(|token| !token.is_empty()) {
        Some(token) => {
            let mut persisted = false;
            if let Ok(entry) = keyring_entry(service, account) {
                persisted = entry.set_password(token).is_ok();
            }
            if persisted {
                let _ = std::fs::remove_file(&path);
            } else if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                std::fs::write(&path, token).map_err(|err| err.to_string())?;
            }
        }
        None => {
            if let Ok(entry) = keyring_entry(service, account) {
                let _ = entry.delete_password();
            }
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

pub fn secret_is_set(secret_id: &str) -> Result<bool, String> {
    Ok(match secret_id {
        "telegram_bot_token" => load_secret(
            TELEGRAM_BOT_TOKEN_SERVICE,
            TELEGRAM_BOT_TOKEN_ACCOUNT,
            "telegram_bot_token",
        ),
        "discord_bot_token" => load_secret(
            DISCORD_BOT_TOKEN_SERVICE,
            DISCORD_BOT_TOKEN_ACCOUNT,
            "discord_bot_token",
        ),
        "slack_bot_token" => load_secret(
            SLACK_BOT_TOKEN_SERVICE,
            SLACK_BOT_TOKEN_ACCOUNT,
            "slack_bot_token",
        ),
        "slack_app_token" => load_secret(
            SLACK_APP_TOKEN_SERVICE,
            SLACK_APP_TOKEN_ACCOUNT,
            "slack_app_token",
        ),
        "api_key" => load_secret(API_KEY_SERVICE, API_KEY_ACCOUNT, "api_key"),
        "stt_api_key" => load_secret(STT_API_KEY_SERVICE, STT_API_KEY_ACCOUNT, "stt_api_key"),
        other if other.starts_with("api_key_") => {
            let provider = other.trim_start_matches("api_key_");
            load_secret("medousa.providers", provider, other)
        }
        "custom_provider_id" => load_secret(
            "medousa.providers",
            "custom_provider_id",
            "custom_provider_id",
        ),
        other if other.starts_with("base_url_") => {
            load_secret("medousa.providers", other, other)
        }
        other => return Err(format!("unknown secret_id '{other}'")),
    })
}

pub fn save_secret(secret_id: &str, value: Option<String>) -> Result<(), String> {
    match secret_id {
        "telegram_bot_token" => save_secret_value(
            TELEGRAM_BOT_TOKEN_SERVICE,
            TELEGRAM_BOT_TOKEN_ACCOUNT,
            "telegram_bot_token",
            value.as_deref(),
        ),
        "discord_bot_token" => save_secret_value(
            DISCORD_BOT_TOKEN_SERVICE,
            DISCORD_BOT_TOKEN_ACCOUNT,
            "discord_bot_token",
            value.as_deref(),
        ),
        "slack_bot_token" => save_secret_value(
            SLACK_BOT_TOKEN_SERVICE,
            SLACK_BOT_TOKEN_ACCOUNT,
            "slack_bot_token",
            value.as_deref(),
        ),
        "slack_app_token" => save_secret_value(
            SLACK_APP_TOKEN_SERVICE,
            SLACK_APP_TOKEN_ACCOUNT,
            "slack_app_token",
            value.as_deref(),
        ),
        "api_key" => save_secret_value(
            API_KEY_SERVICE,
            API_KEY_ACCOUNT,
            "api_key",
            value.as_deref(),
        ),
        "stt_api_key" => save_secret_value(
            STT_API_KEY_SERVICE,
            STT_API_KEY_ACCOUNT,
            "stt_api_key",
            value.as_deref(),
        ),
        other if other.starts_with("api_key_") => {
            let provider = other.trim_start_matches("api_key_");
            save_secret_value("medousa.providers", provider, other, value.as_deref())
        }
        "custom_provider_id" => save_secret_value(
            "medousa.providers",
            "custom_provider_id",
            "custom_provider_id",
            value.as_deref(),
        ),
        other if other.starts_with("base_url_") => {
            save_secret_value("medousa.providers", other, other, value.as_deref())
        }
        other => Err(format!("unknown secret_id '{other}'")),
    }
}

pub fn clear_secret(secret_id: &str) -> Result<(), String> {
    save_secret(secret_id, None)
}
