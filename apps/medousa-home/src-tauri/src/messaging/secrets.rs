use std::path::PathBuf;

use medousa_types::authority_id::ProviderId;
use medousa_types::secrets::IntegrationSecretSlot;

use crate::integration_secrets;

fn custom_provider_id_path() -> PathBuf {
    crate::paths::medousa_data_dir().join("custom_provider_id")
}

fn stt_kind() -> String {
    crate::medousa_paths::load_tui_defaults()
        .stt_provider
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "openai".to_string())
}

fn read_custom_provider_id() -> Option<String> {
    let path = custom_provider_id_path();
    if let Ok(value) = std::fs::read_to_string(&path) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    // Legacy keyring / secrets file migration.
    let legacy = medousa_secrets::load_legacy_keyring("medousa.providers", "custom_provider_id")
        .ok()
        .flatten()
        .or_else(|| {
            medousa_secrets::load_legacy_file(
                &crate::paths::medousa_data_dir()
                    .join("secrets")
                    .join("custom_provider_id"),
            )
            .ok()
            .flatten()
        })?;
    let _ = save_custom_provider_id(Some(&legacy));
    Some(legacy)
}

fn save_custom_provider_id(value: Option<&str>) -> Result<(), String> {
    let path = custom_provider_id_path();
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(token) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            std::fs::write(&path, token).map_err(|err| err.to_string())?;
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
    let _ = medousa_secrets::delete_legacy_keyring("medousa.providers", "custom_provider_id");
    let _ = medousa_secrets::delete_legacy_file(
        &crate::paths::medousa_data_dir()
            .join("secrets")
            .join("custom_provider_id"),
    );
    Ok(())
}

pub fn load_secret_value(secret_id: &str) -> Result<Option<String>, String> {
    let _ = integration_secrets::ensure_secrets_bootstrapped();
    Ok(match secret_id {
        "telegram_bot_token" => {
            integration_secrets::load_kind_secret("telegram", IntegrationSecretSlot::BotToken)
        }
        "discord_bot_token" => {
            integration_secrets::load_kind_secret("discord", IntegrationSecretSlot::BotToken)
        }
        "slack_bot_token" => {
            integration_secrets::load_kind_secret("slack", IntegrationSecretSlot::BotToken)
        }
        "slack_app_token" => {
            integration_secrets::load_kind_secret("slack", IntegrationSecretSlot::AppToken)
        }
        "api_key" => integration_secrets::load_provider_secret("openai"),
        "stt_api_key" => {
            integration_secrets::load_kind_secret(&format!("stt.{}", stt_kind()), IntegrationSecretSlot::ApiKey)
        }
        other if other.starts_with("api_key_") => {
            let provider = other.trim_start_matches("api_key_");
            let provider = ProviderId::parse(provider).map_err(|err| err.to_string())?;
            integration_secrets::load_provider_secret(provider.as_str())
        }
        "custom_provider_id" => read_custom_provider_id(),
        other if other.starts_with("base_url_") => {
            let provider = other.trim_start_matches("base_url_");
            let provider = ProviderId::parse(provider).map_err(|err| err.to_string())?;
            integration_secrets::load_connection_base_url(provider.as_str())
        }
        other => return Err(format!("unknown secret_id '{other}'")),
    })
}

pub fn secret_is_set(secret_id: &str) -> Result<bool, String> {
    Ok(load_secret_value(secret_id)?.is_some())
}

pub fn save_secret(secret_id: &str, value: Option<String>) -> Result<(), String> {
    let _ = integration_secrets::ensure_secrets_bootstrapped();
    match secret_id {
        "telegram_bot_token" => {
            integration_secrets::save_kind_secret(
                "telegram",
                IntegrationSecretSlot::BotToken,
                value.as_deref(),
            );
            Ok(())
        }
        "discord_bot_token" => {
            integration_secrets::save_kind_secret(
                "discord",
                IntegrationSecretSlot::BotToken,
                value.as_deref(),
            );
            Ok(())
        }
        "slack_bot_token" => {
            integration_secrets::save_kind_secret(
                "slack",
                IntegrationSecretSlot::BotToken,
                value.as_deref(),
            );
            Ok(())
        }
        "slack_app_token" => {
            integration_secrets::save_kind_secret(
                "slack",
                IntegrationSecretSlot::AppToken,
                value.as_deref(),
            );
            Ok(())
        }
        "api_key" => {
            integration_secrets::save_provider_secret("openai", value.as_deref());
            Ok(())
        }
        "stt_api_key" => {
            integration_secrets::save_kind_secret(
                &format!("stt.{}", stt_kind()),
                IntegrationSecretSlot::ApiKey,
                value.as_deref(),
            );
            Ok(())
        }
        other if other.starts_with("api_key_") => {
            let provider = other.trim_start_matches("api_key_");
            let provider = ProviderId::parse(provider).map_err(|err| err.to_string())?;
            integration_secrets::save_provider_secret(provider.as_str(), value.as_deref());
            Ok(())
        }
        "custom_provider_id" => save_custom_provider_id(value.as_deref()),
        other if other.starts_with("base_url_") => {
            let provider = other.trim_start_matches("base_url_");
            let provider = ProviderId::parse(provider).map_err(|err| err.to_string())?;
            integration_secrets::save_connection_base_url(provider.as_str(), value.as_deref());
            Ok(())
        }
        other => Err(format!("unknown secret_id '{other}'")),
    }
}

pub fn clear_secret(secret_id: &str) -> Result<(), String> {
    save_secret(secret_id, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_secret_ids_parse() {
        assert!(ProviderId::parse("openai.compatible").is_ok());
        assert!(ProviderId::parse("../../outside").is_err());
    }

    #[test]
    fn stt_kind_defaults_when_unset() {
        // Smoke: helper returns a non-empty slug without panicking.
        assert!(!stt_kind().is_empty());
    }
}
