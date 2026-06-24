use std::collections::{HashMap, HashSet};
use std::future::IntoFuture;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;
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

/// Canonical ingest mapping key: `{channel}:{channel_id}:{user_id}`.
pub fn channel_mapping_key(channel: &str, channel_id: &str, user_id: &str) -> String {
    format!("{channel}:{channel_id}:{user_id}")
}

/// Parse a mapping key produced by [`channel_mapping_key`].
pub fn parse_channel_mapping_key(mapping_key: &str) -> Option<(String, String, String)> {
    let channel = mapping_key.split(':').next()?.to_string();
    let rest = mapping_key.strip_prefix(&format!("{channel}:"))?;
    let marker = format!(":{channel}:user:");
    let idx = rest.find(&marker)?;
    let channel_id = rest[..idx].to_string();
    let user_id = rest[idx + 1..].to_string();
    Some((channel, channel_id, user_id))
}

#[async_trait]
pub trait ChannelSessionStore: Send + Sync {
    async fn get_session_id(&self, mapping_key: &str) -> Option<String>;
    async fn set_session_id(&self, mapping_key: &str, session_id: String);
    async fn push_session_history(&self, mapping_key: &str, session_id: String);
    async fn list_session_history(&self, mapping_key: &str, limit: usize) -> Vec<String>;
    /// Distinct session ids known to channel mappings (active + history).
    async fn list_distinct_session_ids(&self, limit: usize) -> Vec<String>;
    /// Most recently updated mapping for `channel` whose session id matches (e.g. TUI session linked via prior Telegram ingest).
    async fn find_mapping_key_for_session(&self, channel: &str, session_id: &str) -> Option<String>;
    async fn purge_session_references(&self, session_id: &str);
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

    async fn list_distinct_session_ids(&self, limit: usize) -> Vec<String> {
        let mut ids = HashSet::new();
        let mappings = self.mappings.read().await;
        for session_id in mappings.values() {
            ids.insert(session_id.clone());
        }
        drop(mappings);
        let history = self.history.read().await;
        for entries in history.values() {
            for session_id in entries {
                ids.insert(session_id.clone());
            }
        }
        let mut out: Vec<String> = ids.into_iter().collect();
        out.sort();
        out.truncate(limit.max(1));
        out
    }

    async fn find_mapping_key_for_session(&self, channel: &str, session_id: &str) -> Option<String> {
        let prefix = format!("{channel}:");
        self.mappings
            .read()
            .await
            .iter()
            .find(|(key, sid)| key.starts_with(&prefix) && sid.as_str() == session_id)
            .map(|(key, _)| key.clone())
    }

    async fn purge_session_references(&self, session_id: &str) {
        let mut mappings = self.mappings.write().await;
        mappings.retain(|_, sid| sid != session_id);
        drop(mappings);
        let mut history = self.history.write().await;
        for entries in history.values_mut() {
            entries.retain(|sid| sid != session_id);
        }
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

    async fn list_distinct_session_ids(&self, limit: usize) -> Vec<String> {
        let mut ids = HashSet::new();
        for table in [MAPPING_TABLE, HISTORY_TABLE] {
            let sql = "SELECT session_id FROM type::table($table) GROUP BY session_id LIMIT $limit";
            let Ok(mut response) = self
                .db
                .query(sql)
                .bind(("table", table))
                .bind(("limit", limit.max(1) as i64))
                .await
            else {
                continue;
            };
            #[derive(Debug, Deserialize, SurrealValue)]
            struct Row {
                session_id: String,
            }
            if let Ok(rows) = response.take::<Vec<Row>>(0) {
                for row in rows {
                    ids.insert(row.session_id);
                }
            }
        }
        let mut out: Vec<String> = ids.into_iter().collect();
        out.sort();
        out.truncate(limit.max(1));
        out
    }

    async fn find_mapping_key_for_session(&self, channel: &str, session_id: &str) -> Option<String> {
        let prefix = format!("{channel}:");
        let sql = "SELECT mapping_key FROM type::table($table) \
                   WHERE session_id = $session_id \
                   AND string::starts_with(mapping_key, $prefix) \
                   ORDER BY updated_at DESC \
                   LIMIT 1";
        let mut response = self
            .db
            .query(sql)
            .bind(("table", MAPPING_TABLE))
            .bind(("session_id", session_id.to_string()))
            .bind(("prefix", prefix))
            .await
            .ok()?;

        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            mapping_key: String,
        }

        response
            .take::<Vec<Row>>(0)
            .ok()?
            .into_iter()
            .next()
            .map(|row| row.mapping_key)
    }

    async fn purge_session_references(&self, session_id: &str) {
        let _ = self
            .db
            .query("DELETE type::table($mapping) WHERE session_id = $session_id")
            .bind(("mapping", MAPPING_TABLE))
            .bind(("session_id", session_id.to_string()))
            .await;
        let _ = self
            .db
            .query(
                "UPDATE type::table($history) SET session_ids = array::filter(session_ids, |sid| sid != $session_id)",
            )
            .bind(("history", HISTORY_TABLE))
            .bind(("session_id", session_id.to_string()))
            .await;
    }
}

fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

/// Distinct session ids from channel mappings (Telegram/Discord/etc.), for global history lists.
pub fn list_distinct_channel_session_ids(limit: usize) -> Vec<String> {
    let store = channel_session_store();
    block_on(store.list_distinct_session_ids(limit))
}

/// Remove channel mapping/history rows that reference a deleted session.
pub fn purge_session_references(session_id: &str) {
    let store = channel_session_store();
    block_on(store.purge_session_references(session_id));
}
