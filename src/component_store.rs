//! Engine-owned component key/value store (MedousaStore).
//!
//! Primary backend: SurrealDB (`component_kv` table). Falls back to profile-scoped JSON
//! files when the runtime is in-memory (tests / bare loops).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use medousa_types::component_store::{
    is_valid_component_store_key, is_valid_component_store_scope,
    ComponentStoreDeleteResponse, ComponentStoreGetResponse, ComponentStoreListResponse,
    ComponentStoreSetResponse, DEFAULT_COMPONENT_STORE_MAX_KEYS,
    DEFAULT_COMPONENT_STORE_MAX_VALUE_BYTES,
};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;

const COMPONENT_KV_TABLE: &str = "component_kv";
const FILE_STORE_DIR: &str = "component_store";

const COMPONENT_KV_SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE component_kv SCHEMAFULL",
    "DEFINE FIELD profile_id ON TABLE component_kv TYPE string",
    "DEFINE FIELD component_id ON TABLE component_kv TYPE string",
    "DEFINE FIELD store_key ON TABLE component_kv TYPE string",
    "DEFINE FIELD value_json ON TABLE component_kv TYPE string",
    "DEFINE FIELD updated_at ON TABLE component_kv TYPE datetime",
    "DEFINE INDEX idx_component_kv_scope ON TABLE component_kv COLUMNS profile_id, component_id, store_key UNIQUE",
];

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct ComponentKvRecord {
    profile_id: String,
    component_id: String,
    store_key: String,
    value_json: String,
    updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileComponentStoreDocument {
    #[serde(default)]
    entries: BTreeMap<String, Value>,
}

#[derive(Clone)]
pub struct ComponentStoreService {
    db: Option<Surreal<Any>>,
}

static COMPONENT_STORE: OnceCell<Arc<ComponentStoreService>> = OnceCell::new();

pub async fn init_component_store_with_runtime(runtime: &RuntimeComposition) {
    let service = Arc::new(ComponentStoreService::from_runtime(runtime));
    if let Err(err) = service.ensure_schema().await {
        eprintln!("Component store schema init error: {err}");
    }
    let _ = COMPONENT_STORE.set(service);
    eprintln!("Component store (MedousaStore) initialized");
}

pub fn component_store_service() -> Arc<ComponentStoreService> {
    COMPONENT_STORE
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(ComponentStoreService { db: None }))
}

impl ComponentStoreService {
    pub fn from_runtime(runtime: &RuntimeComposition) -> Self {
        match runtime {
            RuntimeComposition::Surreal(rt) => Self {
                db: Some(rt.job_store.db()),
            },
            _ => Self { db: None },
        }
    }

    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        for statement in COMPONENT_KV_SCHEMA_STATEMENTS {
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

    pub async fn get(
        &self,
        profile_id: &str,
        component_id: &str,
        key: Option<&str>,
    ) -> Result<ComponentStoreGetResponse, String> {
        validate_scope(component_id)?;
        if let Some(key) = key {
            validate_key(key)?;
        }

        let mut entries = if let Some(db) = &self.db {
            self.surreal_get(db, profile_id, component_id, key).await?
        } else {
            self.file_get(profile_id, component_id, key).await?
        };

        if let Some(key) = key {
            entries = entries
                .into_iter()
                .filter(|(entry_key, _)| entry_key == key)
                .collect();
        }

        Ok(ComponentStoreGetResponse {
            component_id: component_id.to_string(),
            entries,
        })
    }

    pub async fn set(
        &self,
        profile_id: &str,
        component_id: &str,
        key: &str,
        value: Value,
    ) -> Result<ComponentStoreSetResponse, String> {
        validate_scope(component_id)?;
        validate_key(key)?;
        validate_value(&value)?;

        let updated_at = Utc::now();
        if let Some(db) = &self.db {
            self.surreal_set(db, profile_id, component_id, key, &value, updated_at)
                .await?;
        } else {
            self.file_set(profile_id, component_id, key, value, updated_at)
                .await?;
        }

        Ok(ComponentStoreSetResponse {
            ok: true,
            component_id: component_id.to_string(),
            key: key.to_string(),
            updated_at_utc: updated_at,
        })
    }

    pub async fn delete(
        &self,
        profile_id: &str,
        component_id: &str,
        key: &str,
    ) -> Result<ComponentStoreDeleteResponse, String> {
        validate_scope(component_id)?;
        validate_key(key)?;

        let deleted = if let Some(db) = &self.db {
            self.surreal_delete(db, profile_id, component_id, key).await?
        } else {
            self.file_delete(profile_id, component_id, key).await?
        };

        Ok(ComponentStoreDeleteResponse {
            ok: true,
            component_id: component_id.to_string(),
            key: key.to_string(),
            deleted,
        })
    }

    pub async fn list_keys(
        &self,
        profile_id: &str,
        component_id: &str,
    ) -> Result<ComponentStoreListResponse, String> {
        validate_scope(component_id)?;
        let entries = self.get(profile_id, component_id, None).await?;
        Ok(ComponentStoreListResponse {
            component_id: component_id.to_string(),
            keys: entries.entries.keys().cloned().collect(),
        })
    }

    async fn surreal_get(
        &self,
        db: &Surreal<Any>,
        profile_id: &str,
        component_id: &str,
        key: Option<&str>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let sql = if key.is_some() {
            "SELECT store_key, value_json FROM type::table($table) \
             WHERE profile_id = $profile_id AND component_id = $component_id AND store_key = $store_key"
        } else {
            "SELECT store_key, value_json FROM type::table($table) \
             WHERE profile_id = $profile_id AND component_id = $component_id"
        };

        let mut query = db
            .query(sql)
            .bind(("table", COMPONENT_KV_TABLE))
            .bind(("profile_id", profile_id.to_string()))
            .bind(("component_id", component_id.to_string()));
        if let Some(key) = key {
            query = query.bind(("store_key", key.to_string()));
        }

        let mut response = query.await.map_err(|err| err.to_string())?;
        let rows: Vec<ComponentKvRecord> = response.take(0).map_err(|err| err.to_string())?;
        let mut entries = BTreeMap::new();
        for row in rows {
            if let Ok(value) = serde_json::from_str::<Value>(&row.value_json) {
                entries.insert(row.store_key, value);
            }
        }
        Ok(entries)
    }

    async fn surreal_set(
        &self,
        db: &Surreal<Any>,
        profile_id: &str,
        component_id: &str,
        key: &str,
        value: &Value,
        updated_at: chrono::DateTime<Utc>,
    ) -> Result<(), String> {
        let existing = self
            .surreal_get(db, profile_id, component_id, None)
            .await?;
        if !existing.contains_key(key) && existing.len() >= DEFAULT_COMPONENT_STORE_MAX_KEYS {
            return Err(format!(
                "component store key limit reached ({DEFAULT_COMPONENT_STORE_MAX_KEYS})"
            ));
        }

        let value_json = serde_json::to_string(value).map_err(|err| err.to_string())?;
        let record = ComponentKvRecord {
            profile_id: profile_id.to_string(),
            component_id: component_id.to_string(),
            store_key: key.to_string(),
            value_json,
            updated_at,
        };
        let record_id = store_record_id(profile_id, component_id, key);
        db.query("UPSERT type::record($table, $id) CONTENT $data")
            .bind(("table", COMPONENT_KV_TABLE))
            .bind(("id", record_id))
            .bind(("data", record))
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn surreal_delete(
        &self,
        db: &Surreal<Any>,
        profile_id: &str,
        component_id: &str,
        key: &str,
    ) -> Result<bool, String> {
        db.query(
            "DELETE FROM type::table($table) \
             WHERE profile_id = $profile_id AND component_id = $component_id AND store_key = $store_key",
        )
        .bind(("table", COMPONENT_KV_TABLE))
        .bind(("profile_id", profile_id.to_string()))
        .bind(("component_id", component_id.to_string()))
        .bind(("store_key", key.to_string()))
        .await
        .map_err(|err| err.to_string())?;
        Ok(true)
    }

    async fn file_get(
        &self,
        profile_id: &str,
        component_id: &str,
        key: Option<&str>,
    ) -> Result<BTreeMap<String, Value>, String> {
        let doc = self.read_file_doc(profile_id, component_id).await?;
        if let Some(key) = key {
            let mut entries = BTreeMap::new();
            if let Some(value) = doc.entries.get(key) {
                entries.insert(key.to_string(), value.clone());
            }
            return Ok(entries);
        }
        Ok(doc.entries)
    }

    async fn file_set(
        &self,
        profile_id: &str,
        component_id: &str,
        key: &str,
        value: Value,
        _updated_at: chrono::DateTime<Utc>,
    ) -> Result<(), String> {
        let mut doc = self.read_file_doc(profile_id, component_id).await?;
        if !doc.entries.contains_key(key) && doc.entries.len() >= DEFAULT_COMPONENT_STORE_MAX_KEYS {
            return Err(format!(
                "component store key limit reached ({DEFAULT_COMPONENT_STORE_MAX_KEYS})"
            ));
        }
        doc.entries.insert(key.to_string(), value);
        self.write_file_doc(profile_id, component_id, &doc).await
    }

    async fn file_delete(
        &self,
        profile_id: &str,
        component_id: &str,
        key: &str,
    ) -> Result<bool, String> {
        let mut doc = self.read_file_doc(profile_id, component_id).await?;
        let deleted = doc.entries.remove(key).is_some();
        if deleted {
            self.write_file_doc(profile_id, component_id, &doc).await?;
        }
        Ok(deleted)
    }

    async fn read_file_doc(
        &self,
        profile_id: &str,
        component_id: &str,
    ) -> Result<FileComponentStoreDocument, String> {
        let path = file_doc_path(profile_id, component_id);
        if !path.exists() {
            return Ok(FileComponentStoreDocument::default());
        }
        let raw = tokio::fs::read_to_string(&path)
            .await
            .map_err(|err| err.to_string())?;
        Ok(serde_json::from_str(&raw).unwrap_or_else(|_| FileComponentStoreDocument::default()))
    }

    async fn write_file_doc(
        &self,
        profile_id: &str,
        component_id: &str,
        doc: &FileComponentStoreDocument,
    ) -> Result<(), String> {
        let path = file_doc_path(profile_id, component_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|err| err.to_string())?;
        }
        let raw = serde_json::to_string_pretty(doc).map_err(|err| err.to_string())?;
        tokio::fs::write(path, raw)
            .await
            .map_err(|err| err.to_string())
    }
}

fn validate_scope(component_id: &str) -> Result<(), String> {
    if is_valid_component_store_scope(component_id) {
        Ok(())
    } else {
        Err(format!("invalid component_id '{component_id}'"))
    }
}

fn validate_key(key: &str) -> Result<(), String> {
    if is_valid_component_store_key(key) {
        Ok(())
    } else {
        Err(format!("invalid store key '{key}'"))
    }
}

fn validate_value(value: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(value).map_err(|err| err.to_string())?;
    if encoded.len() > DEFAULT_COMPONENT_STORE_MAX_VALUE_BYTES {
        return Err(format!(
            "value exceeds max bytes ({}/{DEFAULT_COMPONENT_STORE_MAX_VALUE_BYTES})",
            encoded.len()
        ));
    }
    Ok(())
}

fn store_record_id(profile_id: &str, component_id: &str, key: &str) -> String {
    format!(
        "{}__{}__{}",
        sanitize_record_segment(profile_id),
        sanitize_record_segment(component_id),
        sanitize_record_segment(key),
    )
}

fn sanitize_record_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn file_store_root() -> PathBuf {
    if let Ok(raw) = std::env::var("MEDOUSA_COMPONENT_STORE_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    crate::paths::medousa_data_dir().join(FILE_STORE_DIR)
}

fn file_doc_path(profile_id: &str, component_id: &str) -> PathBuf {
    file_store_root()
        .join(sanitize_record_segment(profile_id))
        .join(format!("{}.json", sanitize_record_segment(component_id)))
}

pub async fn component_exists_in_profile(profile_id: &str, component_id: &str) -> bool {
    let Ok(record) = crate::environment_store::environment_hub()
        .get(profile_id)
        .await
    else {
        return false;
    };
    record
        .spec
        .components
        .iter()
        .any(|component| component.id == component_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_scope_and_key() {
        assert!(is_valid_component_store_scope("braindump-capture"));
        assert!(!is_valid_component_store_scope("Braindump"));
        assert!(is_valid_component_store_key("thoughts"));
        assert!(!is_valid_component_store_key(""));
    }

    #[test]
    fn record_id_sanitizes_colons() {
        let id = store_record_id("default", "braindump", "thoughts:v1");
        assert!(!id.contains(':'));
    }

    #[tokio::test]
    async fn file_backed_round_trip_when_no_surreal() {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("target")
            .join(format!("component-store-test-{}", uuid::Uuid::new_v4().simple()));
        unsafe {
            std::env::set_var("MEDOUSA_COMPONENT_STORE_ROOT", &root);
        }
        let service = ComponentStoreService { db: None };
        let profile = "default";
        let component = "braindump-capture";
        service
            .set(profile, component, "thoughts", serde_json::json!(["one"]))
            .await
            .expect("set");
        let got = service
            .get(profile, component, Some("thoughts"))
            .await
            .expect("get");
        assert_eq!(
            got.entries.get("thoughts"),
            Some(&serde_json::json!(["one"]))
        );
        let keys = service.list_keys(profile, component).await.expect("keys");
        assert!(keys.keys.contains(&"thoughts".to_string()));
        service
            .delete(profile, component, "thoughts")
            .await
            .expect("delete");
        let empty = service
            .get(profile, component, Some("thoughts"))
            .await
            .expect("get after delete");
        assert!(empty.entries.is_empty());
        let _ = tokio::fs::remove_dir_all(root).await;
        unsafe {
            std::env::remove_var("MEDOUSA_COMPONENT_STORE_ROOT");
        }
    }
}
