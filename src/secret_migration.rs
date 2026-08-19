//! Copy legacy keyring/file secrets onto typed daemon paths, then delete the
//! old services so macOS stops prompting for them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use medousa_secrets::{self, SecretStore};
use medousa_types::{
    DaemonSecretPath, InstallationId, IntegrationSecretSlot, KIND_APNS, KIND_CHATGPT, KIND_DISCORD,
    KIND_SLACK, KIND_TELEGRAM, ProviderId,
};

use crate::integration_store;
use crate::paths::medousa_data_dir;

static MIGRATED: AtomicBool = AtomicBool::new(false);
static IN_PROGRESS: AtomicBool = AtomicBool::new(false);

const KNOWN_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "google",
    "gemini",
    "groq",
    "xai",
    "mistral",
    "cohere",
    "perplexity",
    "together",
    "fireworks",
    "openrouter",
    "deepseek",
    "openai.compatible",
];

pub fn migrate_legacy_secrets() -> Result<(), String> {
    if MIGRATED.load(Ordering::SeqCst) || IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = migrate_legacy_secrets_inner();
    if result.is_ok() {
        MIGRATED.store(true, Ordering::SeqCst);
    }
    IN_PROGRESS.store(false, Ordering::SeqCst);
    result
}

fn migrate_legacy_secrets_inner() -> Result<(), String> {
    let store = SecretStore::new(medousa_data_dir());
    let installation = store
        .ensure_installation_id()
        .map_err(|err| err.to_string())?;
    migrate_surreal_password(&store, &installation)?;
    migrate_channel_tokens(&store, &installation)?;
    migrate_provider_keys(&store, &installation)?;
    migrate_chatgpt_oauth(&store, &installation)?;
    migrate_apns(&store, &installation)?;
    Ok(())
}

fn data_dir() -> PathBuf {
    medousa_data_dir()
}

fn secrets_dir() -> PathBuf {
    data_dir().join("secrets")
}

fn migrate_value(
    store: &SecretStore,
    installation: &InstallationId,
    kind: &str,
    slot: IntegrationSecretSlot,
    value: Option<String>,
    legacy: &[(&str, &str, Option<PathBuf>)],
) -> Result<(), String> {
    let Some(value) = value.filter(|token| !token.trim().is_empty()) else {
        for (service, account, path) in legacy {
            let _ = store.delete_legacy_entry(service, account);
            if let Some(path) = path {
                let _ = medousa_secrets::delete_legacy(service, account, Some(path));
            }
        }
        return Ok(());
    };
    let connection = integration_store::upsert_kind(kind, None, None, None)?;
    let path = DaemonSecretPath::integration(installation.clone(), connection, slot);
    if store.get_daemon(&path).is_none() {
        store
            .set_daemon(&path, Some(value.trim()))
            .map_err(|err| err.to_string())?;
    }
    for (service, account, file) in legacy {
        let _ = medousa_secrets::delete_legacy(service, account, file.as_deref());
    }
    Ok(())
}

fn migrate_surreal_password(
    store: &SecretStore,
    installation: &InstallationId,
) -> Result<(), String> {
    let path = DaemonSecretPath::surreal_password(installation.clone());
    let file = data_dir().join("surreal_password");
    let value = store.get_daemon(&path).or_else(|| {
        medousa_secrets::read_legacy("medousa.surreal", "password", &file)
    });
    if let Some(value) = value.filter(|token| !token.trim().is_empty())
        && store.get_daemon(&path).is_none()
    {
        store
            .set_daemon(&path, Some(value.trim()))
            .map_err(|err| err.to_string())?;
    }
    let _ = medousa_secrets::delete_legacy("medousa.surreal", "password", Some(&file));
    Ok(())
}

fn migrate_channel_tokens(
    store: &SecretStore,
    installation: &InstallationId,
) -> Result<(), String> {
    migrate_value(
        store,
        installation,
        KIND_DISCORD,
        IntegrationSecretSlot::BotToken,
        medousa_secrets::read_legacy(
            "medousa.discord",
            "bot_token",
            &secrets_dir().join("discord_bot_token"),
        ),
        &[(
            "medousa.discord",
            "bot_token",
            Some(secrets_dir().join("discord_bot_token")),
        )],
    )?;
    migrate_value(
        store,
        installation,
        KIND_TELEGRAM,
        IntegrationSecretSlot::BotToken,
        medousa_secrets::read_legacy(
            "medousa.telegram",
            "bot_token",
            &secrets_dir().join("telegram_bot_token"),
        ),
        &[(
            "medousa.telegram",
            "bot_token",
            Some(secrets_dir().join("telegram_bot_token")),
        )],
    )?;
    migrate_value(
        store,
        installation,
        KIND_SLACK,
        IntegrationSecretSlot::BotToken,
        medousa_secrets::read_legacy(
            "medousa.slack",
            "bot_token",
            &secrets_dir().join("slack_bot_token"),
        ),
        &[(
            "medousa.slack",
            "bot_token",
            Some(secrets_dir().join("slack_bot_token")),
        )],
    )?;
    migrate_value(
        store,
        installation,
        KIND_SLACK,
        IntegrationSecretSlot::AppToken,
        medousa_secrets::read_legacy(
            "medousa.slack",
            "app_token",
            &secrets_dir().join("slack_app_token"),
        ),
        &[(
            "medousa.slack",
            "app_token",
            Some(secrets_dir().join("slack_app_token")),
        )],
    )?;
    Ok(())
}

fn migrate_provider_keys(
    store: &SecretStore,
    installation: &InstallationId,
) -> Result<(), String> {
    let workshop_key = medousa_secrets::read_legacy(
        "medousa.tui",
        "api_key",
        &secrets_dir().join("api_key"),
    );
    let mut kinds = Vec::new();
    for provider in KNOWN_PROVIDERS {
        kinds.push((*provider).to_string());
    }
    if let Some(custom) = medousa_secrets::read_legacy(
        "medousa.providers",
        "custom_provider_id",
        &secrets_dir().join("custom_provider_id"),
    ) {
        let custom = custom.trim().to_ascii_lowercase();
        if !custom.is_empty() {
            kinds.push(custom);
        }
    }
    if let Ok(entries) = std::fs::read_dir(secrets_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix("api_key_")
                && !rest.starts_with("pv1-")
            {
                kinds.push(rest.to_ascii_lowercase());
            }
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    for kind in kinds {
        if !seen.insert(kind.clone()) {
            continue;
        }
        let Ok(provider) = ProviderId::parse(&kind) else {
            continue;
        };
        let opaque = provider.storage_key();
        let value = medousa_secrets::read_legacy(
            "medousa.providers",
            opaque.as_str(),
            &secrets_dir().join(format!("api_key_{}", opaque.as_str())),
        )
        .or_else(|| {
            medousa_secrets::read_legacy(
                "medousa.providers",
                &format!("api_key.{}", opaque.as_str()),
                &secrets_dir().join(format!("api_key_{}", opaque.as_str())),
            )
        })
        .or_else(|| {
            medousa_secrets::read_legacy(
                "medousa.providers",
                provider.as_str(),
                &secrets_dir().join(format!("api_key_{}", provider.as_str())),
            )
        })
        .or_else(|| workshop_key.clone().filter(|_| workshop_matches(&kind)));
        let base_url = medousa_secrets::read_legacy(
            "medousa.providers",
            &format!("base_url.{}", opaque.as_str()),
            &secrets_dir().join(format!("base_url_{}", opaque.as_str())),
        )
        .or_else(|| {
            medousa_secrets::read_legacy(
                "medousa.providers",
                &format!("base_url_{}", provider.as_str()),
                &secrets_dir().join(format!("base_url_{}", provider.as_str())),
            )
        });
        if value.is_none() && base_url.is_none() {
            continue;
        }
        let catalog_id = None;
        let connection = integration_store::upsert_kind(
            provider.as_str(),
            None,
            catalog_id,
            base_url.as_deref(),
        )?;
        if let Some(value) = value {
            let path = DaemonSecretPath::integration(
                installation.clone(),
                connection,
                IntegrationSecretSlot::ApiKey,
            );
            if store.get_daemon(&path).is_none() {
                let _ = store.set_daemon(&path, Some(value.trim()));
            }
        }
        let _ = medousa_secrets::delete_legacy(
            "medousa.providers",
            opaque.as_str(),
            Some(&secrets_dir().join(format!("api_key_{}", opaque.as_str()))),
        );
        let _ = medousa_secrets::delete_legacy(
            "medousa.providers",
            &format!("api_key.{}", opaque.as_str()),
            Some(&secrets_dir().join(format!("api_key_{}", provider.as_str()))),
        );
        let _ = medousa_secrets::delete_legacy(
            "medousa.providers",
            provider.as_str(),
            Some(&secrets_dir().join(format!("api_key_{}", provider.as_str()))),
        );
        let _ = medousa_secrets::delete_legacy(
            "medousa.providers",
            &format!("base_url.{}", opaque.as_str()),
            Some(&secrets_dir().join(format!("base_url_{}", opaque.as_str()))),
        );
        let _ = medousa_secrets::delete_legacy(
            "medousa.providers",
            &format!("base_url_{}", provider.as_str()),
            Some(&secrets_dir().join(format!("base_url_{}", provider.as_str()))),
        );
    }

    let stt = medousa_secrets::read_legacy(
        "medousa.stt",
        "api_key",
        &secrets_dir().join("stt_api_key"),
    );
    if let Some(value) = stt {
        let kind = crate::session::load_tui_defaults()
            .stt_provider
            .unwrap_or_else(|| "openai".to_string());
        migrate_value(
            store,
            installation,
            &kind,
            IntegrationSecretSlot::ApiKey,
            Some(value),
            &[(
                "medousa.stt",
                "api_key",
                Some(secrets_dir().join("stt_api_key")),
            )],
        )?;
    }

    let _ = medousa_secrets::delete_legacy(
        "medousa.tui",
        "api_key",
        Some(&secrets_dir().join("api_key")),
    );
    let _ = medousa_secrets::delete_legacy(
        "medousa.providers",
        "custom_provider_id",
        Some(&secrets_dir().join("custom_provider_id")),
    );
    Ok(())
}

fn workshop_matches(kind: &str) -> bool {
    crate::session::load_tui_defaults()
        .provider
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .is_some_and(|provider| provider == kind)
}

fn migrate_chatgpt_oauth(
    store: &SecretStore,
    installation: &InstallationId,
) -> Result<(), String> {
    let file = secrets_dir().join("chatgpt_oauth.json");
    let value = medousa_secrets::read_legacy("medousa.chatgpt", "native_oauth", &file);
    migrate_value(
        store,
        installation,
        KIND_CHATGPT,
        IntegrationSecretSlot::OauthBundle,
        value,
        &[("medousa.chatgpt", "native_oauth", Some(file))],
    )
}

fn migrate_apns(store: &SecretStore, installation: &InstallationId) -> Result<(), String> {
    let value = medousa_secrets::read_legacy_keyring("medousa.apns", "auth_key");
    migrate_value(
        store,
        installation,
        KIND_APNS,
        IntegrationSecretSlot::AuthKey,
        value,
        &[("medousa.apns", "auth_key", None)],
    )
}
