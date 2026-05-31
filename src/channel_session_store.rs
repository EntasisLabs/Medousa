use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::sync::RwLock as AsyncRwLock;

const MAPPING_TABLE: &str = "channel_session_mapping";
const HISTORY_TABLE: &str = "channel_session_history";

const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE channel_session_mapping SCHEMAFULL",
    "DEFINE FIELD mapping_key ON TABLE channel_session_mapping TYPE string",
    "DEFINE FIELD session_id ON TABLE channel_session_mapping TYPE string",
    "DEFINE FIELD updated_at ON TABLE channel_session_mapping TYPE datetime",
    "DEFINE INDEX idx_channel_session_mapping_key ON TABLE channel_session_mapping COLUMNS mapping_key UNIQUE",
    "DEFINE TABLE channel_session_history SCHEMAFULL",
    "DEFINE FIELD mapping_key ON TABLE channel_session_history TYPE string",
    "DEFINE FIELD session_id ON TABLE channel_session_history TYPE string",
    "DEFINE FIELD recorded_at ON TABLE channel_session_history TYPE datetime",
    "DEFINE INDEX idx_channel_session_history_mapping ON TABLE channel_session_history COLUMNS mapping_key",
];

static CHANNEL_SESSION_STORE: Lazy<RwLock<Arc<dyn ChannelSessionStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(InMemoryChannelSessionStore::default())));

pub fn channel_session_store() -> Arc<dyn ChannelSessionStore> {
    CHANNEL_SESSION_STORE.read().unwrap().clone()
}

pub fn set_channel_session_store(store: Arc<dyn ChannelSessionStore>) {
    let mut guard = CHANNEL_SESSION_STORE.write().unwrap();
    *guard = store;
}

pub async fn init_channel_session_store_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let store = SurrealChannelSessionStore::new(rt.job_store.db());
            if let Err(err) = store.ensure_schema().await {
                eprintln!(
                    "Surreal channel session store schema init error: {err}; keeping in-memory store"
                );
                return;
            }
            set_channel_session_store(Arc::new(store));
            eprintln!("Surreal runtime detected; channel session store switched to SurrealDB backend");
        }
        _ => {}
    }
}

#[async_trait]
pub trait ChannelSessionStore: Send + Sync {
    async fn get_session_id(&self, mapping_key: &str) -> Option<String>;
    async fn set_session_id(&self, mapping_key: &str, session_id: String);
    async fn push_session_history(&self, mapping_key: &str, session_id: String);
    async fn list_session_history(&self, mapping_key: &str, limit: usize) -> Vec<String>;
}

#[derive(Default)]
struct InMemoryChannelSessionStore {
    mappings: AsyncRwLock<HashMap<String, String>>,
    history: AsyncRwLock<HashMap<String, Vec<String>>>,
}

#[async_trait]
impl ChannelSessionStore for InMemoryChannelSessionStore {
    async fn get_session_id(&self, mapping_key: &str) -> Option<String> {
        self.mappings.read().await.get(mapping_key).cloned()
    }

    async fn set_session_id(&self, mapping_key: &str, session_id: String) {
        self.mappings
            .write()
            .await
            .insert(mapping_key.to_string(), session_id);
    }

    async fn push_session_history(&self, mapping_key: &str, session_id: String) {
        let mut history = self.history.write().await;
        let entries = history.entry(mapping_key.to_string()).or_default();
        entries.retain(|existing| existing != &session_id);
        entries.insert(0, session_id);
        entries.truncate(20);
    }

    async fn list_session_history(&self, mapping_key: &str, limit: usize) -> Vec<String> {
        let history = self.history.read().await;
        history
            .get(mapping_key)
            .map(|entries| entries.iter().take(limit.max(1)).cloned().collect())
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct SurrealChannelSessionStore {
    db: Surreal<Any>,
}

impl SurrealChannelSessionStore {
    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
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

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct ChannelSessionMappingRecord {
    mapping_key: String,
    session_id: String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct ChannelSessionHistoryRecord {
    mapping_key: String,
    session_id: String,
    recorded_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl ChannelSessionStore for SurrealChannelSessionStore {
    async fn get_session_id(&self, mapping_key: &str) -> Option<String> {
        let sql = "SELECT session_id FROM type::table($table) WHERE mapping_key = $mapping_key LIMIT 1";
        let mut response = self
            .db
            .query(sql)
            .bind(("table", MAPPING_TABLE))
            .bind(("mapping_key", mapping_key.to_string()))
            .await
            .ok()?;

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            session_id: String,
        }

        response.take::<Vec<Row>>(0).ok()?.into_iter().next().map(|row| row.session_id)
    }

    async fn set_session_id(&self, mapping_key: &str, session_id: String) {
        let record = ChannelSessionMappingRecord {
            mapping_key: mapping_key.to_string(),
            session_id: session_id.clone(),
            updated_at: Utc::now(),
        };

        let sql = "UPSERT type::record($table, $id) CONTENT $data";
        let _ = self
            .db
            .query(sql)
            .bind(("table", MAPPING_TABLE))
            .bind(("id", mapping_key.to_string()))
            .bind(("data", record))
            .await;
    }

    async fn push_session_history(&self, mapping_key: &str, session_id: String) {
        let delete_sql = "DELETE type::table($table) \
                          WHERE mapping_key = $mapping_key AND session_id = $session_id";
        let _ = self
            .db
            .query(delete_sql)
            .bind(("table", HISTORY_TABLE))
            .bind(("mapping_key", mapping_key.to_string()))
            .bind(("session_id", session_id.clone()))
            .await;

        let record = ChannelSessionHistoryRecord {
            mapping_key: mapping_key.to_string(),
            session_id: session_id.clone(),
            recorded_at: Utc::now(),
        };

        let create_sql = "CREATE type::table($table) CONTENT $data";
        let _ = self
            .db
            .query(create_sql)
            .bind(("table", HISTORY_TABLE))
            .bind(("data", record))
            .await;

        let prune_sql = "DELETE type::table($table) \
                         WHERE mapping_key = $mapping_key \
                         AND id NOT IN (
                             SELECT VALUE id FROM type::table($table)
                             WHERE mapping_key = $mapping_key
                             ORDER BY recorded_at DESC
                             LIMIT 20
                         )";
        let _ = self
            .db
            .query(prune_sql)
            .bind(("table", HISTORY_TABLE))
            .bind(("mapping_key", mapping_key.to_string()))
            .await;
    }

    async fn list_session_history(&self, mapping_key: &str, limit: usize) -> Vec<String> {
        let sql = "SELECT session_id FROM type::table($table) \
                   WHERE mapping_key = $mapping_key \
                   ORDER BY recorded_at DESC \
                   LIMIT $limit";
        let mut response = match self
            .db
            .query(sql)
            .bind(("table", HISTORY_TABLE))
            .bind(("mapping_key", mapping_key.to_string()))
            .bind(("limit", limit.max(1) as i64))
            .await
        {
            Ok(response) => response,
            Err(_) => return Vec::new(),
        };

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            session_id: String,
        }

        response
            .take::<Vec<Row>>(0)
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.session_id)
            .collect()
    }
}
