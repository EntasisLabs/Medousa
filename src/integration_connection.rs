//! Integration connection records + daemon-owned secret coordinates.
//!
//! Surreal `integration_connection` table with owner-only JSON file fallback.
//! Secrets live in `medousa-secrets` under typed `DaemonSecretPath` accounts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

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
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;

use crate::paths::medousa_data_dir;

const TABLE: &str = "integration_connection";
const FILE_STORE: &str = "integration_connections.json";

const SCHEMA: &[&str] = &[
    "DEFINE TABLE integration_connection SCHEMAFULL",
    "DEFINE FIELD connection_id ON TABLE integration_connection TYPE string",
    "DEFINE FIELD kind ON TABLE integration_connection TYPE string",
    "DEFINE FIELD label ON TABLE integration_connection TYPE option<string>",
    "DEFINE FIELD base_url ON TABLE integration_connection TYPE option<string>",
    "DEFINE FIELD api_key ON TABLE integration_connection TYPE bool",
    "DEFINE FIELD oauth_bundle ON TABLE integration_connection TYPE bool",
    "DEFINE FIELD bot_token ON TABLE integration_connection TYPE bool",
    "DEFINE FIELD app_token ON TABLE integration_connection TYPE bool",
    "DEFINE FIELD auth_key ON TABLE integration_connection TYPE bool",
    "DEFINE FIELD created_at ON TABLE integration_connection TYPE datetime",
    "DEFINE FIELD updated_at ON TABLE integration_connection TYPE datetime",
    "DEFINE INDEX idx_integration_connection_id ON TABLE integration_connection COLUMNS connection_id UNIQUE",
];

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct IntegrationConnectionRow {
    connection_id: String,
    kind: String,
    label: Option<String>,
    base_url: Option<String>,
    api_key: bool,
    oauth_bundle: bool,
    bot_token: bool,
    app_token: bool,
    auth_key: bool,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileStoreDocument {
    #[serde(default)]
    connections: BTreeMap<String, IntegrationConnection>,
}

#[derive(Clone)]
pub struct IntegrationConnectionService {
    db: Option<Surreal<Any>>,
    file_lock: Arc<Mutex<()>>,
}

static SERVICE: OnceCell<Arc<IntegrationConnectionService>> = OnceCell::new();
static MIGRATED: OnceLock<()> = OnceLock::new();
const LEGACY_SECRET_MIGRATION: &str = "daemon-legacy-secrets-v1";

pub fn init_integration_connection_service(db: Option<Surreal<Any>>) {
    let service = Arc::new(IntegrationConnectionService {
        db,
        file_lock: Arc::new(Mutex::new(())),
    });
    let _ = SERVICE.set(service);
}

pub async fn init_integration_connection_from_runtime(
    runtime: &stasis::prelude::RuntimeComposition,
) {
    let db = match runtime {
        stasis::prelude::RuntimeComposition::Surreal(rt) => Some(rt.job_store.db()),
        _ => None,
    };
    init_integration_connection_service(db);
    let service = integration_connection_service();
    if let Err(err) = service.ensure_schema().await {
        eprintln!("integration_connection schema init error: {err}");
    }
    let _ = ensure_secrets_bootstrapped();
}

pub fn integration_connection_service() -> Arc<IntegrationConnectionService> {
    SERVICE.get().cloned().unwrap_or_else(|| {
        Arc::new(IntegrationConnectionService {
            db: None,
            file_lock: Arc::new(Mutex::new(())),
        })
    })
}

pub fn ensure_secrets_bootstrapped() -> anyhow::Result<InstallationId> {
    let data_dir = medousa_data_dir();
    let installation_id = ensure_installation_id(&data_dir)?;
    let _ = MIGRATED.get_or_init(|| {
        if let Err(err) = migrate_legacy_secrets_once(&data_dir, &installation_id) {
            eprintln!("integration secret migration warning: {err:#}");
        }
    });
    Ok(installation_id)
}

fn migrate_legacy_secrets_once(
    data_dir: &Path,
    installation_id: &InstallationId,
) -> anyhow::Result<()> {
    if secret_migration_completed(data_dir, LEGACY_SECRET_MIGRATION)? {
        return Ok(());
    }
    migrate_legacy_secrets(data_dir, installation_id)?;
    mark_secret_migration_completed(data_dir, LEGACY_SECRET_MIGRATION)?;
    Ok(())
}

impl IntegrationConnectionService {
    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        for statement in SCHEMA {
            if let Err(err) = db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    pub async fn list(&self) -> anyhow::Result<Vec<IntegrationConnection>> {
        if let Some(db) = &self.db {
            let mut response = db
                .query(format!(
                    "SELECT * FROM {TABLE} ORDER BY kind ASC, connection_id ASC"
                ))
                .await?;
            let rows: Vec<IntegrationConnectionRow> = response.take(0)?;
            if !rows.is_empty() {
                return Ok(rows.into_iter().map(row_to_connection).collect());
            }
        }
        Ok(self.list_file())
    }

    pub async fn get(
        &self,
        connection_id: &ConnectionId,
    ) -> anyhow::Result<Option<IntegrationConnection>> {
        if let Some(db) = &self.db {
            let mut response = db
                .query(format!(
                    "SELECT * FROM {TABLE} WHERE connection_id = $id LIMIT 1"
                ))
                .bind(("id", connection_id.as_str().to_string()))
                .await?;
            let rows: Vec<IntegrationConnectionRow> = response.take(0)?;
            if let Some(row) = rows.into_iter().next() {
                return Ok(Some(row_to_connection(row)));
            }
        }
        Ok(self.get_file(connection_id))
    }

    pub async fn find_by_kind(&self, kind: &str) -> anyhow::Result<Vec<IntegrationConnection>> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .filter(|c| c.kind == kind)
            .collect())
    }

    pub async fn upsert_record(&self, record: IntegrationConnection) -> anyhow::Result<()> {
        self.upsert_file(&record)?;
        if let Some(db) = &self.db {
            let row = connection_to_row(&record);
            db.query(format!(
                "DELETE {TABLE} WHERE connection_id = $id; CREATE {TABLE} CONTENT $row"
            ))
            .bind(("id", record.connection_id.as_str().to_string()))
            .bind(("row", row))
            .await?;
        }
        Ok(())
    }

    pub async fn delete_record(&self, connection_id: &ConnectionId) -> anyhow::Result<bool> {
        let Some(existing) = self.get(connection_id).await? else {
            return Ok(false);
        };
        let installation_id = ensure_secrets_bootstrapped()?;
        for slot in [
            IntegrationSecretSlot::ApiKey,
            IntegrationSecretSlot::OauthBundle,
            IntegrationSecretSlot::BotToken,
            IntegrationSecretSlot::AppToken,
            IntegrationSecretSlot::AuthKey,
        ] {
            if existing.secrets.slot(slot) {
                let path = integration_path(&installation_id, connection_id, slot);
                let _ = delete_daemon_secret(&medousa_data_dir(), &path);
            }
        }
        self.delete_file(connection_id);
        if let Some(db) = &self.db {
            db.query(format!("DELETE {TABLE} WHERE connection_id = $id"))
                .bind(("id", connection_id.as_str().to_string()))
                .await?;
        }
        Ok(true)
    }

    pub async fn ensure_kind(
        &self,
        kind: &str,
        label: Option<&str>,
        base_url: Option<&str>,
    ) -> anyhow::Result<IntegrationConnection> {
        ensure_kind_sync(kind, label, base_url)
    }

    pub async fn set_slot_presence(
        &self,
        connection_id: &ConnectionId,
        slot: IntegrationSecretSlot,
        present: bool,
    ) -> anyhow::Result<Option<IntegrationConnection>> {
        set_slot_presence_sync(connection_id, slot, present)
    }

    fn list_file(&self) -> Vec<IntegrationConnection> {
        load_file_doc().connections.into_values().collect()
    }

    fn get_file(&self, connection_id: &ConnectionId) -> Option<IntegrationConnection> {
        load_file_doc()
            .connections
            .get(connection_id.as_str())
            .cloned()
    }

    fn upsert_file(&self, record: &IntegrationConnection) -> anyhow::Result<()> {
        let _guard = self.file_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut doc = load_file_doc();
        doc.connections
            .insert(record.connection_id.as_str().to_string(), record.clone());
        save_file_doc(&doc)
    }

    fn delete_file(&self, connection_id: &ConnectionId) {
        let _guard = self.file_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut doc = load_file_doc();
        if doc.connections.remove(connection_id.as_str()).is_some() {
            let _ = save_file_doc(&doc);
        }
    }
}

fn file_store_path() -> PathBuf {
    medousa_data_dir().join(FILE_STORE)
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

fn save_file_doc(doc: &FileStoreDocument) -> anyhow::Result<()> {
    let path = file_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(doc)?;
    crate::session::atomic_write(&path, &bytes)?;
    Ok(())
}

fn row_to_connection(row: IntegrationConnectionRow) -> IntegrationConnection {
    let connection_id = ConnectionId::parse(&row.connection_id).unwrap_or_else(|_| {
        ConnectionId::parse("00000000-0000-0000-0000-000000000000").expect("nil uuid")
    });
    IntegrationConnection {
        connection_id,
        kind: row.kind,
        label: row.label,
        base_url: row.base_url,
        secrets: IntegrationSecretStatus {
            api_key: row.api_key,
            oauth_bundle: row.oauth_bundle,
            bot_token: row.bot_token,
            app_token: row.app_token,
            auth_key: row.auth_key,
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn connection_to_row(record: &IntegrationConnection) -> IntegrationConnectionRow {
    IntegrationConnectionRow {
        connection_id: record.connection_id.as_str().to_string(),
        kind: record.kind.clone(),
        label: record.label.clone(),
        base_url: record.base_url.clone(),
        api_key: record.secrets.api_key,
        oauth_bundle: record.secrets.oauth_bundle,
        bot_token: record.secrets.bot_token,
        app_token: record.secrets.app_token,
        auth_key: record.secrets.auth_key,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
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
) -> anyhow::Result<IntegrationConnection> {
    if let Some(first) = find_by_kind_sync(kind).into_iter().next() {
        return Ok(first);
    }
    let now = Utc::now();
    let record = IntegrationConnection {
        connection_id: ConnectionId::parse(&uuid::Uuid::new_v4().to_string())
            .map_err(|e| anyhow::anyhow!("{e}"))?,
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
) -> anyhow::Result<Option<IntegrationConnection>> {
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

/// Load a secret for a catalog kind + slot (exactly-one connection semantics).
pub fn load_kind_secret(kind: &str, slot: IntegrationSecretSlot) -> Option<String> {
    let installation_id = ensure_secrets_bootstrapped().ok()?;
    let matches = find_by_kind_sync(kind);
    let connection = matches
        .iter()
        .find(|c| c.secrets.slot(slot))
        .cloned()
        .or_else(|| matches.into_iter().next())?;
    let path = integration_path(&installation_id, &connection.connection_id, slot);
    load_daemon_secret(&medousa_data_dir(), &path)
        .ok()
        .flatten()
        .map(|r| r.value)
}

/// Report whether a secret slot is configured without opening the secret store.
///
/// Status and routing checks must use the durable presence metadata instead of
/// reading a Keychain value they do not consume. The value is still verified
/// when the integration actually loads it.
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
            let _ = save_daemon_secret(&medousa_data_dir(), &path, v);
            let _ = set_slot_presence_sync(&connection.connection_id, slot, true);
        }
        None => {
            let _ = delete_daemon_secret(&medousa_data_dir(), &path);
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
    let mut doc = load_file_doc();
    doc.connections
        .insert(connection.connection_id.as_str().to_string(), connection);
    let _ = save_file_doc(&doc);
}

pub fn load_surreal_password_secret() -> Option<String> {
    let installation_id = ensure_secrets_bootstrapped().ok()?;
    let path = DaemonSecretPath::SurrealPassword { installation_id };
    load_daemon_secret(&medousa_data_dir(), &path)
        .ok()
        .flatten()
        .map(|r| r.value)
}

pub fn save_surreal_password_secret(password: Option<&str>) {
    let Ok(installation_id) = ensure_secrets_bootstrapped() else {
        return;
    };
    let path = DaemonSecretPath::SurrealPassword { installation_id };
    match password.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let _ = save_daemon_secret(&medousa_data_dir(), &path, value);
        }
        None => {
            let _ = delete_daemon_secret(&medousa_data_dir(), &path);
        }
    }
}

fn migrate_legacy_secrets(data_dir: &Path, installation_id: &InstallationId) -> anyhow::Result<()> {
    if load_daemon_secret(
        data_dir,
        &DaemonSecretPath::SurrealPassword {
            installation_id: installation_id.clone(),
        },
    )?
    .is_none()
    {
        let legacy = load_legacy_keyring("medousa.surreal", "password")?.or_else(|| {
            load_legacy_file(&data_dir.join("surreal_password"))
                .ok()
                .flatten()
        });
        if let Some(value) = legacy {
            save_daemon_secret(
                data_dir,
                &DaemonSecretPath::SurrealPassword {
                    installation_id: installation_id.clone(),
                },
                &value,
            )?;
            let _ = delete_legacy_keyring("medousa.surreal", "password");
            let _ = delete_legacy_file(&data_dir.join("surreal_password"));
        }
    }

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

    if let Some(value) = load_legacy_keyring("medousa.tui", "api_key")?.or_else(|| {
        load_legacy_file(&data_dir.join("secrets").join("api_key"))
            .ok()
            .flatten()
    }) {
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

    if let Some(value) = load_legacy_keyring("medousa.chatgpt", "native_oauth")?.or_else(|| {
        load_legacy_file(&data_dir.join("secrets").join("chatgpt_oauth.json"))
            .ok()
            .flatten()
    }) {
        seed_secret(
            data_dir,
            installation_id,
            "chatgpt",
            IntegrationSecretSlot::OauthBundle,
            &value,
        )?;
        let _ = delete_legacy_keyring("medousa.chatgpt", "native_oauth");
        let _ = delete_legacy_file(&data_dir.join("secrets").join("chatgpt_oauth.json"));
    }

    if let Some(value) = load_legacy_keyring("medousa.apns", "auth_key")?.or_else(|| {
        load_legacy_file(&data_dir.join("secrets").join("apns_auth_key"))
            .ok()
            .flatten()
    }) {
        seed_secret(
            data_dir,
            installation_id,
            "apns",
            IntegrationSecretSlot::AuthKey,
            &value,
        )?;
        let _ = delete_legacy_keyring("medousa.apns", "auth_key");
        let _ = delete_legacy_file(&data_dir.join("secrets").join("apns_auth_key"));
    }

    if let Some(value) = load_legacy_keyring("medousa.stt", "api_key")?.or_else(|| {
        load_legacy_file(&data_dir.join("secrets").join("stt_api_key"))
            .ok()
            .flatten()
    }) {
        let stt_kind = crate::session::load_tui_defaults()
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

fn migrate_bot(
    data_dir: &Path,
    installation_id: &InstallationId,
    kind: &str,
    legacy_service: &str,
    legacy_account: &str,
    file_name: &str,
    slot: IntegrationSecretSlot,
) -> anyhow::Result<()> {
    let Some(value) = load_legacy_keyring(legacy_service, legacy_account)?.or_else(|| {
        load_legacy_file(&data_dir.join("secrets").join(file_name))
            .ok()
            .flatten()
    }) else {
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
) -> anyhow::Result<()> {
    let Ok(provider_id) = ProviderId::parse(provider) else {
        return Ok(());
    };
    let opaque = provider_id.storage_key();
    let value = load_legacy_keyring("medousa.providers", opaque.as_str())?
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
    )?
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

fn seed_secret(
    data_dir: &Path,
    installation_id: &InstallationId,
    kind: &str,
    slot: IntegrationSecretSlot,
    value: &str,
) -> anyhow::Result<()> {
    let connection = ensure_kind_sync(kind, None, None)?;
    let path = integration_path(installation_id, &connection.connection_id, slot);
    if load_daemon_secret(data_dir, &path)?.is_some() {
        return Ok(());
    }
    save_daemon_secret(data_dir, &path, value)?;
    set_slot_presence_sync(&connection.connection_id, slot, true)?;
    Ok(())
}
