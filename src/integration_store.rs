//! Durable integration connection catalog.
//!
//! The JSON file is always available (needed before Surreal). Surreal is an
//! optional replica once the runtime is up. Secret bytes live in `medousa-secrets`.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use medousa_secrets::SecretStore;
use medousa_types::{
    ConnectionId, CreateIntegrationRequest, DaemonSecretPath, InstallationId,
    IntegrationConnection, IntegrationSecretSlot, IntegrationSecretStatus, KIND_APNS,
    KIND_CHATGPT, KIND_DISCORD, KIND_SLACK, KIND_TELEGRAM, PatchIntegrationRequest, ProviderId,
};
use serde::{Deserialize, Serialize};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;

use crate::paths::medousa_data_dir;

const CONNECTIONS_FILE: &str = "integration_connections.json";
const TABLE: &str = "integration_connection";

const SCHEMA: &[&str] = &[
    "DEFINE TABLE integration_connection SCHEMAFULL",
    "DEFINE FIELD connection_id ON TABLE integration_connection TYPE string",
    "DEFINE FIELD kind ON TABLE integration_connection TYPE string",
    "DEFINE FIELD label ON TABLE integration_connection TYPE string",
    "DEFINE FIELD catalog_id ON TABLE integration_connection TYPE option<string>",
    "DEFINE FIELD base_url ON TABLE integration_connection TYPE option<string>",
    "DEFINE FIELD created_at_utc ON TABLE integration_connection TYPE datetime",
    "DEFINE FIELD updated_at_utc ON TABLE integration_connection TYPE datetime",
    "DEFINE INDEX idx_integration_connection_id ON TABLE integration_connection COLUMNS connection_id UNIQUE",
];

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct ConnectionRecord {
    connection_id: String,
    kind: String,
    label: String,
    catalog_id: Option<String>,
    base_url: Option<String>,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConnectionDocument {
    #[serde(default)]
    connections: Vec<ConnectionRecord>,
}

struct IntegrationCatalog {
    records: Vec<ConnectionRecord>,
    db: Option<Surreal<Any>>,
}

static CATALOG: OnceLock<Mutex<IntegrationCatalog>> = OnceLock::new();

fn catalog() -> &'static Mutex<IntegrationCatalog> {
    CATALOG.get_or_init(|| {
        Mutex::new(IntegrationCatalog {
            records: load_file_document().connections,
            db: None,
        })
    })
}

fn data_dir() -> PathBuf {
    medousa_data_dir()
}

fn secret_store() -> SecretStore {
    SecretStore::new(data_dir())
}

pub fn ensure_installation_id() -> Result<InstallationId, String> {
    secret_store()
        .ensure_installation_id()
        .map_err(|err| err.to_string())
}

pub fn seed_from_legacy() -> Result<(), String> {
    crate::secret_migration::migrate_legacy_secrets()
}

fn load_file_document() -> ConnectionDocument {
    let path = data_dir().join(CONNECTIONS_FILE);
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn persist_file(records: &[ConnectionRecord]) -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join(CONNECTIONS_FILE);
    let encoded = serde_json::to_vec_pretty(&ConnectionDocument {
        connections: records.to_vec(),
    })
    .map_err(|err| err.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &encoded).map_err(|err| err.to_string())?;
    fs::rename(&tmp, &path).map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn init_with_runtime(runtime: &stasis::prelude::RuntimeComposition) {
    let db = match runtime {
        stasis::prelude::RuntimeComposition::Surreal(rt) => Some(rt.job_store.db()),
        _ => None,
    };
    if let Some(db) = &db {
        for statement in SCHEMA {
            if let Err(err) = db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    eprintln!("integration connection schema init error: {err}");
                    break;
                }
            }
        }
    }
    let existing = {
        let mut catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        catalog.db = db.clone();
        catalog.records.clone()
    };
    if existing.is_empty() {
        if let Some(db) = &db
            && let Ok(rows) = surreal_list(db).await
        {
            let mut catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            catalog.records = rows;
            let _ = persist_file(&catalog.records);
        }
    } else if let Some(db) = db {
        for record in &existing {
            let _ = surreal_upsert(&db, record).await;
        }
    }
    let _ = seed_from_legacy();
}

async fn surreal_list(db: &Surreal<Any>) -> Result<Vec<ConnectionRecord>, String> {
    let mut response = db
        .query("SELECT * FROM type::table($table)")
        .bind(("table", TABLE))
        .await
        .map_err(|err| err.to_string())?;
    response.take(0).map_err(|err| err.to_string())
}

async fn surreal_upsert(db: &Surreal<Any>, record: &ConnectionRecord) -> Result<(), String> {
    db.query("UPSERT type::record($table, $id) CONTENT $data")
        .bind(("table", TABLE))
        .bind(("id", record.connection_id.clone()))
        .bind(("data", record.clone()))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

async fn surreal_delete(db: &Surreal<Any>, connection_id: &str) -> Result<(), String> {
    db.query("DELETE FROM type::table($table) WHERE connection_id = $id")
        .bind(("table", TABLE))
        .bind(("id", connection_id.to_string()))
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn list_connections() -> Result<Vec<IntegrationConnection>, String> {
    let _ = seed_from_legacy();
    let records = {
        let catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        catalog.records.clone()
    };
    records.iter().map(to_wire).collect::<Result<Vec<_>, _>>()
}

pub fn get_connection(connection_id: &str) -> Result<IntegrationConnection, String> {
    let _ = seed_from_legacy();
    let id = ConnectionId::parse(connection_id).map_err(|err| err.to_string())?;
    let record = {
        let catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        catalog
            .records
            .iter()
            .find(|row| row.connection_id == id.as_str())
            .cloned()
            .ok_or_else(|| "connection not found".to_string())?
    };
    to_wire(&record)
}

pub fn find_by_kind(kind: &str) -> Option<ConnectionId> {
    let _ = seed_from_legacy();
    let kind = kind.trim().to_ascii_lowercase();
    let catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let matches: Vec<_> = catalog
        .records
        .iter()
        .filter(|row| row.kind == kind)
        .collect();
    if matches.len() == 1 {
        ConnectionId::parse(&matches[0].connection_id).ok()
    } else {
        None
    }
}

pub fn find_first_kind(kind: &str) -> Option<ConnectionId> {
    let _ = seed_from_legacy();
    let kind = kind.trim().to_ascii_lowercase();
    let catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    catalog
        .records
        .iter()
        .find(|row| row.kind == kind)
        .and_then(|row| ConnectionId::parse(&row.connection_id).ok())
}

pub fn create_connection(request: CreateIntegrationRequest) -> Result<IntegrationConnection, String> {
    let kind = normalize_kind(&request.kind)?;
    let now = Utc::now();
    let record = ConnectionRecord {
        connection_id: ConnectionId::generate().as_str().to_string(),
        kind: kind.clone(),
        label: request
            .label
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default_label(&kind)),
        catalog_id: request.catalog_id.filter(|value| !value.trim().is_empty()),
        base_url: normalize_optional_url(request.base_url)?,
        created_at_utc: now,
        updated_at_utc: now,
    };
    insert_record(record)
}

pub fn upsert_kind(
    kind: &str,
    label: Option<&str>,
    catalog_id: Option<&str>,
    base_url: Option<&str>,
) -> Result<ConnectionId, String> {
    let kind = normalize_kind(kind)?;
    if let Some(existing) = find_first_kind(&kind) {
        if label.is_some() || catalog_id.is_some() || base_url.is_some() {
            let _ = patch_connection(
                existing.as_str(),
                PatchIntegrationRequest {
                    label: label.map(ToString::to_string),
                    catalog_id: catalog_id.map(ToString::to_string),
                    base_url: base_url.map(ToString::to_string),
                    kind: None,
                },
            );
        }
        return Ok(existing);
    }
    let created = create_connection(CreateIntegrationRequest {
        kind,
        label: label.map(ToString::to_string),
        catalog_id: catalog_id.map(ToString::to_string),
        base_url: base_url.map(ToString::to_string),
    })?;
    ConnectionId::parse(&created.connection_id).map_err(|err| err.to_string())
}

pub fn patch_connection(
    connection_id: &str,
    request: PatchIntegrationRequest,
) -> Result<IntegrationConnection, String> {
    let id = ConnectionId::parse(connection_id).map_err(|err| err.to_string())?;
    let snapshot = {
        let mut catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let record = catalog
            .records
            .iter_mut()
            .find(|row| row.connection_id == id.as_str())
            .ok_or_else(|| "connection not found".to_string())?;
        if let Some(kind) = request.kind {
            record.kind = normalize_kind(&kind)?;
        }
        if let Some(label) = request.label {
            let trimmed = label.trim();
            if !trimmed.is_empty() {
                record.label = trimmed.to_string();
            }
        }
        if let Some(catalog_id) = request.catalog_id {
            record.catalog_id = Some(catalog_id).filter(|value| !value.trim().is_empty());
        }
        if request.base_url.is_some() {
            record.base_url = normalize_optional_url(request.base_url)?;
        }
        record.updated_at_utc = Utc::now();
        let snapshot = record.clone();
        persist_file(&catalog.records)?;
        snapshot
    };
    schedule_surreal_upsert(snapshot.clone());
    to_wire(&snapshot)
}

pub fn delete_connection(connection_id: &str) -> Result<String, String> {
    let id = ConnectionId::parse(connection_id).map_err(|err| err.to_string())?;
    let installation = ensure_installation_id()?;
    let store = secret_store();
    for slot in IntegrationSecretSlot::all() {
        let path = DaemonSecretPath::integration(installation.clone(), id.clone(), *slot);
        let _ = store.set_daemon(&path, None);
    }
    {
        let mut catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = catalog.records.len();
        catalog
            .records
            .retain(|row| row.connection_id != id.as_str());
        if catalog.records.len() == before {
            return Err("connection not found".to_string());
        }
        persist_file(&catalog.records)?;
    }
    schedule_surreal_delete(id.as_str().to_string());
    Ok(id.as_str().to_string())
}

pub fn connection_secret_path(
    connection_id: &ConnectionId,
    slot: IntegrationSecretSlot,
) -> Result<DaemonSecretPath, String> {
    Ok(DaemonSecretPath::integration(
        ensure_installation_id()?,
        connection_id.clone(),
        slot,
    ))
}

pub fn secret_configured(connection_id: &ConnectionId, slot: IntegrationSecretSlot) -> bool {
    connection_secret_path(connection_id, slot)
        .ok()
        .and_then(|path| secret_store().get_daemon(&path))
        .is_some()
}

pub fn load_connection_secret(
    connection_id: &ConnectionId,
    slot: IntegrationSecretSlot,
) -> Option<String> {
    let path = connection_secret_path(connection_id, slot).ok()?;
    secret_store().get_daemon(&path)
}

pub fn save_connection_secret(
    connection_id: &ConnectionId,
    slot: IntegrationSecretSlot,
    value: Option<&str>,
) -> Result<bool, String> {
    let path = connection_secret_path(connection_id, slot)?;
    secret_store()
        .set_daemon(&path, value)
        .map_err(|err| err.to_string())?;
    Ok(value.map(str::trim).is_some_and(|token| !token.is_empty()))
}

pub fn load_kind_secret(kind: &str, slot: IntegrationSecretSlot) -> Option<String> {
    let connection = find_first_kind(kind)?;
    load_connection_secret(&connection, slot)
}

pub fn save_kind_secret(
    kind: &str,
    slot: IntegrationSecretSlot,
    value: Option<&str>,
) -> Result<(), String> {
    let connection = if value.is_some() {
        upsert_kind(kind, None, None, None)?
    } else {
        find_first_kind(kind).ok_or_else(|| format!("no {kind} connection"))?
    };
    save_connection_secret(&connection, slot, value).map(|_| ())
}

fn schedule_surreal_upsert(record: ConnectionRecord) {
    let db = catalog()
        .lock()
        .ok()
        .and_then(|catalog| catalog.db.clone());
    let Some(db) = db else {
        return;
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let _ = surreal_upsert(&db, &record).await;
        });
    }
}

fn schedule_surreal_delete(connection_id: String) {
    let db = catalog()
        .lock()
        .ok()
        .and_then(|catalog| catalog.db.clone());
    let Some(db) = db else {
        return;
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let _ = surreal_delete(&db, &connection_id).await;
        });
    }
}

fn insert_record(record: ConnectionRecord) -> Result<IntegrationConnection, String> {
    {
        let mut catalog = catalog().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        catalog.records.push(record.clone());
        persist_file(&catalog.records)?;
    }
    schedule_surreal_upsert(record.clone());
    to_wire(&record)
}

fn to_wire(record: &ConnectionRecord) -> Result<IntegrationConnection, String> {
    let connection = ConnectionId::parse(&record.connection_id).map_err(|err| err.to_string())?;
    let mut secrets = IntegrationSecretStatus::default();
    for slot in IntegrationSecretSlot::all() {
        secrets.set_slot(*slot, secret_configured(&connection, *slot));
    }
    Ok(IntegrationConnection {
        connection_id: record.connection_id.clone(),
        kind: record.kind.clone(),
        label: record.label.clone(),
        catalog_id: record.catalog_id.clone(),
        base_url: record.base_url.clone(),
        secrets,
        created_at_utc: record.created_at_utc,
        updated_at_utc: record.updated_at_utc,
    })
}

fn normalize_kind(kind: &str) -> Result<String, String> {
    let kind = kind.trim().to_ascii_lowercase();
    if kind.is_empty() {
        return Err("kind is required".to_string());
    }
    if matches!(
        kind.as_str(),
        KIND_DISCORD | KIND_TELEGRAM | KIND_SLACK | KIND_CHATGPT | KIND_APNS
    ) {
        return Ok(kind);
    }
    ProviderId::parse(&kind)
        .map(|id| id.as_str().to_string())
        .map_err(|err| err.to_string())
}

fn default_label(kind: &str) -> String {
    match kind {
        KIND_DISCORD => "Discord".to_string(),
        KIND_TELEGRAM => "Telegram".to_string(),
        KIND_SLACK => "Slack".to_string(),
        KIND_CHATGPT => "ChatGPT".to_string(),
        KIND_APNS => "APNs".to_string(),
        other => other.to_string(),
    }
}

fn normalize_optional_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("base_url must be an http(s) URL".to_string());
    }
    Ok(Some(trimmed.trim_end_matches('/').to_string()))
}
