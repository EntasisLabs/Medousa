//! Home-side daemon secret coordinates (file-backed connection index + medousa-secrets).
//!
//! Mirrors `medousa::integration_connection` sync helpers so co-located Home can
//! read/write bot/provider/STT secrets without linking the full engine crate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use medousa_secrets::{
    delete_daemon_secret, delete_legacy_file, delete_legacy_keyring, ensure_installation_id,
    load_daemon_secret, load_legacy_file, load_legacy_keyring, mark_secret_migration_completed,
    save_daemon_secret, secret_migration_completed,
};
use medousa_types::authority_id::ProviderId;
use medousa_types::secrets::{
    ConnectionId, DaemonSecretPath, InstallationId, IntegrationConnection, IntegrationSecretSlot,
    IntegrationSecretStatus,
};
use serde::{Deserialize, Serialize};

const FILE_STORE: &str = "integration_connections.json";

static FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static MIGRATED: OnceLock<()> = OnceLock::new();
const DAEMON_LEGACY_SECRET_MIGRATION: &str = "daemon-legacy-secrets-v1";
const HOME_LEGACY_SECRET_MIGRATION: &str = "home-legacy-secrets-v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileStoreDocument {
    #[serde(default)]
    connections: BTreeMap<String, IntegrationConnection>,
}

fn data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn file_lock() -> &'static Mutex<()> {
    FILE_LOCK.get_or_init(|| Mutex::new(()))
}

fn file_store_path() -> PathBuf {
    data_dir().join(FILE_STORE)
}

fn load_file_doc() -> FileStoreDocument {
    let path = file_store_path();
    if !path.is_file() {
        return FileStoreDocument::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_file_doc(doc: &FileStoreDocument) -> Result<(), String> {
    let path = file_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(doc).map_err(|err| err.to_string())?;
    std::fs::write(&path, bytes).map_err(|err| err.to_string())
}

fn integration_path(
    installation_id: &InstallationId,
    connection_id: &ConnectionId,
    slot: IntegrationSecretSlot,
) -> DaemonSecretPath {
    DaemonSecretPath::Integration {
        installation_id: installation_id.clone(),
        connection_id: connection_id.clone(),
        slot,
    }
}

pub fn ensure_secrets_bootstrapped() -> Result<InstallationId, String> {
    let data_dir = data_dir();
    let installation_id = ensure_installation_id(&data_dir).map_err(|err| err.to_string())?;
    let _ = MIGRATED.get_or_init(|| {
        if let Err(err) = migrate_legacy_secrets_once(&data_dir, &installation_id) {
            eprintln!("home integration secret migration warning: {err}");
        }
    });
    Ok(installation_id)
}

fn migrate_legacy_secrets_once(
    data_dir: &Path,
    installation_id: &InstallationId,
) -> Result<(), String> {
    // On desktop the daemon owns the superset migration and shares this data
    // root. Do not make Home rescan the same legacy Keychain coordinates.
    if secret_migration_completed(data_dir, DAEMON_LEGACY_SECRET_MIGRATION)
        .map_err(|error| error.to_string())?
        || secret_migration_completed(data_dir, HOME_LEGACY_SECRET_MIGRATION)
            .map_err(|error| error.to_string())?
    {
        return Ok(());
    }
    migrate_legacy_secrets(data_dir, installation_id)?;
    mark_secret_migration_completed(data_dir, HOME_LEGACY_SECRET_MIGRATION)
        .map_err(|error| error.to_string())
}

fn find_by_kind_sync(kind: &str) -> Vec<IntegrationConnection> {
    load_file_doc()
        .connections
        .into_values()
        .filter(|c| c.kind == kind)
        .collect()
}

fn ensure_kind_sync(
    kind: &str,
    label: Option<&str>,
    base_url: Option<&str>,
) -> Result<IntegrationConnection, String> {
    let _guard = file_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(first) = find_by_kind_sync(kind).into_iter().next() {
        return Ok(first);
    }
    let now = Utc::now();
    let record = IntegrationConnection {
        connection_id: ConnectionId::parse(&uuid::Uuid::new_v4().to_string())
            .map_err(|e| e.to_string())?,
        kind: kind.to_string(),
        label: label.map(str::to_string),
        base_url: base_url.map(str::to_string),
        secrets: IntegrationSecretStatus::default(),
        created_at: now,
        updated_at: now,
    };
    let mut doc = load_file_doc();
    doc.connections
        .insert(record.connection_id.as_str().to_string(), record.clone());
    save_file_doc(&doc)?;
    Ok(record)
}

fn set_slot_presence_sync(
    connection_id: &ConnectionId,
    slot: IntegrationSecretSlot,
    present: bool,
) -> Result<Option<IntegrationConnection>, String> {
    let _guard = file_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut doc = load_file_doc();
    let Some(record) = doc.connections.get_mut(connection_id.as_str()) else {
        return Ok(None);
    };
    record.secrets.set_slot(slot, present);
    record.updated_at = Utc::now();
    let cloned = record.clone();
    save_file_doc(&doc)?;
    Ok(Some(cloned))
}

pub fn load_kind_secret(kind: &str, slot: IntegrationSecretSlot) -> Option<String> {
    let installation_id = ensure_secrets_bootstrapped().ok()?;
    let matches = find_by_kind_sync(kind);
    let connection = matches
        .iter()
        .find(|c| c.secrets.slot(slot))
        .cloned()
        .or_else(|| matches.into_iter().next())?;
    let path = integration_path(&installation_id, &connection.connection_id, slot);
    load_daemon_secret(&data_dir(), &path)
        .ok()
        .flatten()
        .map(|r| r.value)
}

pub fn kind_secret_configured(kind: &str, slot: IntegrationSecretSlot) -> bool {
    if ensure_secrets_bootstrapped().is_err() {
        return false;
    }
    find_by_kind_sync(kind)
        .into_iter()
        .any(|connection| connection.secrets.slot(slot))
}

pub fn save_kind_secret(kind: &str, slot: IntegrationSecretSlot, value: Option<&str>) {
    let Ok(installation_id) = ensure_secrets_bootstrapped() else {
        return;
    };
    let Ok(connection) = ensure_kind_sync(kind, None, None) else {
        return;
    };
    let path = integration_path(&installation_id, &connection.connection_id, slot);
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => {
            let _ = save_daemon_secret(&data_dir(), &path, v);
            let _ = set_slot_presence_sync(&connection.connection_id, slot, true);
        }
        None => {
            let _ = delete_daemon_secret(&data_dir(), &path);
            let _ = set_slot_presence_sync(&connection.connection_id, slot, false);
        }
    }
}

pub fn load_provider_secret(provider: &str) -> Option<String> {
    load_kind_secret(provider, IntegrationSecretSlot::ApiKey)
}

pub fn save_provider_secret(provider: &str, api_key: Option<&str>) {
    save_kind_secret(provider, IntegrationSecretSlot::ApiKey, api_key);
}

pub fn load_connection_base_url(kind: &str) -> Option<String> {
    find_by_kind_sync(kind).into_iter().find_map(|c| c.base_url)
}

pub fn save_connection_base_url(kind: &str, base_url: Option<&str>) {
    let Ok(mut connection) = ensure_kind_sync(kind, None, None) else {
        return;
    };
    connection.base_url = base_url
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    connection.updated_at = Utc::now();
    let _guard = file_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut doc = load_file_doc();
    doc.connections
        .insert(connection.connection_id.as_str().to_string(), connection);
    let _ = save_file_doc(&doc);
}

pub fn load_bot_token(channel: &str) -> Option<String> {
    load_kind_secret(channel, IntegrationSecretSlot::BotToken)
}

pub fn load_app_token(channel: &str) -> Option<String> {
    load_kind_secret(channel, IntegrationSecretSlot::AppToken)
}

fn seed_secret(
    data_dir: &Path,
    installation_id: &InstallationId,
    kind: &str,
    slot: IntegrationSecretSlot,
    value: &str,
) -> Result<(), String> {
    let connection = ensure_kind_sync(kind, None, None)?;
    let path = integration_path(installation_id, &connection.connection_id, slot);
    if load_daemon_secret(data_dir, &path)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(());
    }
    save_daemon_secret(data_dir, &path, value).map_err(|e| e.to_string())?;
    set_slot_presence_sync(&connection.connection_id, slot, true)?;
    Ok(())
}

fn migrate_bot(
    data_dir: &Path,
    installation_id: &InstallationId,
    kind: &str,
    legacy_service: &str,
    legacy_account: &str,
    file_name: &str,
    slot: IntegrationSecretSlot,
) -> Result<(), String> {
    let Some(value) = load_legacy_keyring(legacy_service, legacy_account)
        .ok()
        .flatten()
        .or_else(|| {
            load_legacy_file(&data_dir.join("secrets").join(file_name))
                .ok()
                .flatten()
        })
    else {
        return Ok(());
    };
    seed_secret(data_dir, installation_id, kind, slot, &value)?;
    let _ = delete_legacy_keyring(legacy_service, legacy_account);
    let _ = delete_legacy_file(&data_dir.join("secrets").join(file_name));
    Ok(())
}

fn migrate_provider(
    data_dir: &Path,
    installation_id: &InstallationId,
    provider: &str,
) -> Result<(), String> {
    let Ok(provider_id) = ProviderId::parse(provider) else {
        return Ok(());
    };
    let opaque = provider_id.storage_key();
    let value = load_legacy_keyring("medousa.providers", opaque.as_str())
        .ok()
        .flatten()
        .or_else(|| {
            load_legacy_keyring("medousa.providers", &format!("api_key.{}", opaque.as_str()))
                .ok()
                .flatten()
        })
        .or_else(|| {
            load_legacy_keyring("medousa.providers", provider)
                .ok()
                .flatten()
        })
        .or_else(|| {
            load_legacy_file(
                &data_dir
                    .join("secrets")
                    .join(format!("api_key_{}", opaque.as_str())),
            )
            .ok()
            .flatten()
        })
        .or_else(|| {
            load_legacy_file(&data_dir.join("secrets").join(format!("api_key_{provider}")))
                .ok()
                .flatten()
        });
    if let Some(value) = value {
        seed_secret(
            data_dir,
            installation_id,
            provider,
            IntegrationSecretSlot::ApiKey,
            &value,
        )?;
        let _ = delete_legacy_keyring("medousa.providers", opaque.as_str());
        let _ = delete_legacy_keyring("medousa.providers", &format!("api_key.{}", opaque.as_str()));
        let _ = delete_legacy_keyring("medousa.providers", provider);
        let _ = delete_legacy_file(
            &data_dir
                .join("secrets")
                .join(format!("api_key_{}", opaque.as_str())),
        );
        let _ = delete_legacy_file(&data_dir.join("secrets").join(format!("api_key_{provider}")));
    }

    let base = load_legacy_keyring(
        "medousa.providers",
        &format!("base_url.{}", opaque.as_str()),
    )
    .ok()
    .flatten()
    .or_else(|| {
        load_legacy_keyring("medousa.providers", &format!("base_url_{provider}"))
            .ok()
            .flatten()
    })
    .or_else(|| {
        load_legacy_file(
            &data_dir
                .join("secrets")
                .join(format!("base_url_{}", opaque.as_str())),
        )
        .ok()
        .flatten()
    })
    .or_else(|| {
        load_legacy_file(
            &data_dir
                .join("secrets")
                .join(format!("base_url_{provider}")),
        )
        .ok()
        .flatten()
    });
    if let Some(base_url) = base {
        let mut connection = ensure_kind_sync(provider, None, Some(&base_url))?;
        connection.base_url = Some(base_url);
        connection.updated_at = Utc::now();
        let _guard = file_lock().lock().unwrap_or_else(|e| e.into_inner());
        let mut doc = load_file_doc();
        doc.connections
            .insert(connection.connection_id.as_str().to_string(), connection);
        save_file_doc(&doc)?;
        let _ = delete_legacy_keyring(
            "medousa.providers",
            &format!("base_url.{}", opaque.as_str()),
        );
        let _ = delete_legacy_keyring("medousa.providers", &format!("base_url_{provider}"));
        let _ = delete_legacy_file(
            &data_dir
                .join("secrets")
                .join(format!("base_url_{}", opaque.as_str())),
        );
        let _ = delete_legacy_file(
            &data_dir
                .join("secrets")
                .join(format!("base_url_{provider}")),
        );
    }
    Ok(())
}

fn migrate_legacy_secrets(data_dir: &Path, installation_id: &InstallationId) -> Result<(), String> {
    migrate_bot(
        data_dir,
        installation_id,
        "discord",
        "medousa.discord",
        "bot_token",
        "discord_bot_token",
        IntegrationSecretSlot::BotToken,
    )?;
    migrate_bot(
        data_dir,
        installation_id,
        "telegram",
        "medousa.telegram",
        "bot_token",
        "telegram_bot_token",
        IntegrationSecretSlot::BotToken,
    )?;
    migrate_bot(
        data_dir,
        installation_id,
        "slack",
        "medousa.slack",
        "bot_token",
        "slack_bot_token",
        IntegrationSecretSlot::BotToken,
    )?;
    migrate_bot(
        data_dir,
        installation_id,
        "slack",
        "medousa.slack",
        "app_token",
        "slack_app_token",
        IntegrationSecretSlot::AppToken,
    )?;

    if let Some(value) = load_legacy_keyring("medousa.tui", "api_key")
        .ok()
        .flatten()
        .or_else(|| {
            load_legacy_file(&data_dir.join("secrets").join("api_key"))
                .ok()
                .flatten()
        })
    {
        seed_secret(
            data_dir,
            installation_id,
            "openai",
            IntegrationSecretSlot::ApiKey,
            &value,
        )?;
        let _ = delete_legacy_keyring("medousa.tui", "api_key");
        let _ = delete_legacy_file(&data_dir.join("secrets").join("api_key"));
    }

    for provider in [
        "openai",
        "anthropic",
        "google",
        "groq",
        "xai",
        "deepseek",
        "mistral",
        "cohere",
        "openrouter",
        "ollama",
        "openai.compatible",
    ] {
        migrate_provider(data_dir, installation_id, provider)?;
    }

    if let Some(value) = load_legacy_keyring("medousa.stt", "api_key")
        .ok()
        .flatten()
        .or_else(|| {
            load_legacy_file(&data_dir.join("secrets").join("stt_api_key"))
                .ok()
                .flatten()
        })
    {
        let stt_kind = crate::medousa_paths::load_tui_defaults()
            .stt_provider
            .unwrap_or_else(|| "openai".to_string());
        seed_secret(
            data_dir,
            installation_id,
            &format!("stt.{stt_kind}"),
            IntegrationSecretSlot::ApiKey,
            &value,
        )?;
        let _ = delete_legacy_keyring("medousa.stt", "api_key");
        let _ = delete_legacy_file(&data_dir.join("secrets").join("stt_api_key"));
    }

    Ok(())
}
