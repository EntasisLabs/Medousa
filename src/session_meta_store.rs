//! Global session display names (one label per `session_id`, all channels).

use std::collections::HashMap;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

pub const MAX_SESSION_DISPLAY_NAME_CHARS: usize = 80;

const SESSION_META_TABLE: &str = "session_meta";

const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE session_meta SCHEMAFULL",
    "DEFINE FIELD session_id ON TABLE session_meta TYPE string",
    "DEFINE FIELD display_name ON TABLE session_meta TYPE string",
    "DEFINE FIELD updated_at ON TABLE session_meta TYPE datetime",
    "DEFINE INDEX idx_session_meta_session_id ON TABLE session_meta COLUMNS session_id UNIQUE",
];

static SESSION_META_STORE: Lazy<RwLock<Arc<dyn SessionMetaStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(FileSessionMetaStore)));

pub async fn init_session_meta_store_with_runtime(runtime: &RuntimeComposition) {
    if let RuntimeComposition::Surreal(rt) = runtime {
        let store = SurrealSessionMetaStore::new(rt.job_store.db());
        if let Err(err) = store.ensure_schema().await {
            eprintln!(
                "Surreal session meta schema init error: {err}; keeping file-backed session names"
            );
            return;
        }
        set_session_meta_store(Arc::new(store));
        eprintln!("Surreal runtime detected; session display names switched to SurrealDB backend");
    }
}

fn set_session_meta_store(store: Arc<dyn SessionMetaStore>) {
    let mut guard = SESSION_META_STORE.write().unwrap();
    *guard = store;
}

fn session_meta_store() -> Arc<dyn SessionMetaStore> {
    SESSION_META_STORE.read().unwrap().clone()
}

fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

/// Normalize and validate a user-provided session label.
pub fn normalize_session_display_name(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let out: String = trimmed.chars().take(MAX_SESSION_DISPLAY_NAME_CHARS).collect();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

trait SessionMetaStore: Send + Sync {
    fn set_display_name(&self, session_id: &str, display_name: &str) -> Result<(), String>;
    fn delete_session(&self, session_id: &str);
    fn get_display_name(&self, session_id: &str) -> Option<String>;
    fn load_display_names(&self, session_ids: &[String]) -> HashMap<String, String>;
    fn find_session_id_by_display_name(&self, display_name: &str) -> Option<String>;
    fn list_all_display_names(&self, limit: usize) -> Vec<(String, String)>;
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct SessionMetaRecord {
    session_id: String,
    display_name: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

struct SurrealSessionMetaStore {
    db: Surreal<Any>,
}

impl SurrealSessionMetaStore {
    fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        for statement in SCHEMA_STATEMENTS {
            if let Err(err) = self.db.query(*statement).await {
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
}

impl SessionMetaStore for SurrealSessionMetaStore {
    fn set_display_name(&self, session_id: &str, display_name: &str) -> Result<(), String> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Err("session_id is required".to_string());
        }
        let record = SessionMetaRecord {
            session_id: session_id.to_string(),
            display_name: display_name.to_string(),
            updated_at: Utc::now(),
        };
        let update_sql = "UPDATE type::table($table) MERGE $data WHERE session_id = $session_id";
        let mut update = block_on(
            self.db
                .query(update_sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("session_id", session_id.to_string()))
                .bind(("data", record.clone())),
        )
        .map_err(|err| err.to_string())?;

        #[derive(Debug, Deserialize, SurrealValue)]
        struct UpdatedRow {
            session_id: String,
        }

        let updated: Vec<UpdatedRow> = update.take(0).unwrap_or_default();
        if !updated.is_empty() {
            return Ok(());
        }

        let create_sql = "CREATE type::table($table) CONTENT $data";
        block_on(
            self.db
                .query(create_sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("data", record)),
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn delete_session(&self, session_id: &str) {
        let sql = "DELETE type::table($table) WHERE session_id = $session_id";
        let _ = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("session_id", session_id.trim().to_string())),
        );
    }

    fn get_display_name(&self, session_id: &str) -> Option<String> {
        let sql = "SELECT display_name FROM type::table($table) WHERE session_id = $session_id LIMIT 1";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("session_id", session_id.trim().to_string())),
        )
        .ok()?;

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            display_name: String,
        }

        response
            .take::<Vec<Row>>(0)
            .ok()?
            .into_iter()
            .next()
            .map(|row| row.display_name)
    }

    fn load_display_names(&self, session_ids: &[String]) -> HashMap<String, String> {
        if session_ids.is_empty() {
            return HashMap::new();
        }
        let sql = "SELECT session_id, display_name FROM type::table($table) \
                   WHERE session_id IN $session_ids";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("session_ids", session_ids.to_vec())),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealSessionMetaStore::load_display_names error: {err}");
                return HashMap::new();
            }
        };

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            session_id: String,
            display_name: String,
        }

        let rows: Vec<Row> = response.take(0).unwrap_or_default();
        rows.into_iter()
            .map(|row| (row.session_id, row.display_name))
            .collect()
    }

    fn find_session_id_by_display_name(&self, display_name: &str) -> Option<String> {
        let sql = "SELECT session_id FROM type::table($table) \
                   WHERE display_name = $display_name LIMIT 2";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("display_name", display_name.to_string())),
        )
        .ok()?;

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            session_id: String,
        }

        let rows: Vec<Row> = response.take(0).ok()?;
        match rows.len() {
            1 => Some(rows[0].session_id.clone()),
            _ => None,
        }
    }

    fn list_all_display_names(&self, limit: usize) -> Vec<(String, String)> {
        let sql = "SELECT session_id, display_name FROM type::table($table) \
                   ORDER BY updated_at DESC LIMIT $limit";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_META_TABLE))
                .bind(("limit", limit.max(1) as i64)),
        ) {
            Ok(response) => response,
            Err(_) => return Vec::new(),
        };

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            session_id: String,
            display_name: String,
        }

        response
            .take::<Vec<Row>>(0)
            .unwrap_or_default()
            .into_iter()
            .map(|row| (row.session_id, row.display_name))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FileSessionMetaIndex {
    entries: HashMap<String, FileSessionMetaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileSessionMetaEntry {
    display_name: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

struct FileSessionMetaStore;

impl FileSessionMetaStore {
    fn index_path() -> PathBuf {
        crate::session::medousa_data_dir().join("session_meta.json")
    }

    fn read_index(&self) -> FileSessionMetaIndex {
        let path = Self::index_path();
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, index: &FileSessionMetaIndex) -> Result<(), String> {
        let path = Self::index_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let json = serde_json::to_string_pretty(index).map_err(|err| err.to_string())?;
        crate::session::atomic_write(&path, json.as_bytes()).map_err(|err| err.to_string())
    }
}

impl SessionMetaStore for FileSessionMetaStore {
    fn set_display_name(&self, session_id: &str, display_name: &str) -> Result<(), String> {
        let session_id = session_id.trim().to_string();
        if session_id.is_empty() {
            return Err("session_id is required".to_string());
        }
        let mut index = self.read_index();
        index.entries.insert(
            session_id,
            FileSessionMetaEntry {
                display_name: display_name.to_string(),
                updated_at: Utc::now(),
            },
        );
        self.write_index(&index)
    }

    fn delete_session(&self, session_id: &str) {
        let mut index = self.read_index();
        index.entries.remove(session_id.trim());
        let _ = self.write_index(&index);
    }

    fn get_display_name(&self, session_id: &str) -> Option<String> {
        self.read_index()
            .entries
            .get(session_id.trim())?
            .display_name
            .clone()
            .into()
    }

    fn load_display_names(&self, session_ids: &[String]) -> HashMap<String, String> {
        let index = self.read_index();
        session_ids
            .iter()
            .filter_map(|id| {
                index
                    .entries
                    .get(id)
                    .map(|entry| (id.clone(), entry.display_name.clone()))
            })
            .collect()
    }

    fn find_session_id_by_display_name(&self, display_name: &str) -> Option<String> {
        let matches: Vec<_> = self
            .read_index()
            .entries
            .iter()
            .filter(|(_, entry)| entry.display_name == display_name)
            .map(|(id, _)| id.clone())
            .collect();
        if matches.len() == 1 {
            Some(matches[0].clone())
        } else {
            None
        }
    }

    fn list_all_display_names(&self, limit: usize) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .read_index()
            .entries
            .into_iter()
            .map(|(session_id, entry)| (session_id, entry.display_name, entry.updated_at))
            .collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.2));
        entries
            .into_iter()
            .take(limit.max(1))
            .map(|(session_id, display_name, _)| (session_id, display_name))
            .collect()
    }
}

pub fn set_session_display_name(session_id: &str, display_name: &str) -> Result<(), String> {
    let Some(normalized) = normalize_session_display_name(display_name) else {
        return Err("display name must not be empty".to_string());
    };
    session_meta_store().set_display_name(session_id, &normalized)
}

pub fn get_session_display_name(session_id: &str) -> Option<String> {
    session_meta_store().get_display_name(session_id.trim())
}

pub fn delete_session_meta(session_id: &str) {
    session_meta_store().delete_session(session_id.trim());
}

pub fn load_session_display_names(session_ids: &[String]) -> HashMap<String, String> {
    session_meta_store().load_display_names(session_ids)
}

pub fn find_session_id_by_display_name(display_name: &str) -> Option<String> {
    let normalized = normalize_session_display_name(display_name)?;
    session_meta_store().find_session_id_by_display_name(&normalized)
}

pub fn list_session_display_names(limit: usize) -> Vec<(String, String)> {
    session_meta_store().list_all_display_names(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_blank_and_collapses_whitespace() {
        assert_eq!(
            normalize_session_display_name("  my   project  "),
            Some("my project".to_string())
        );
        assert!(normalize_session_display_name("   ").is_none());
    }
}
