//! Bind recurring definition ids to environment feed ids for FeedSink publish.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_types::feed::is_valid_feed_id;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::sync::RwLock as AsyncRwLock;

const TABLE: &str = "recurring_feed_binding";

const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE recurring_feed_binding SCHEMAFULL",
    "DEFINE FIELD recurring_id ON TABLE recurring_feed_binding TYPE string",
    "DEFINE FIELD feed_ids ON TABLE recurring_feed_binding TYPE array<string>",
    "DEFINE FIELD payload_mode ON TABLE recurring_feed_binding TYPE string",
    "DEFINE FIELD created_at ON TABLE recurring_feed_binding TYPE datetime",
    "DEFINE FIELD updated_at ON TABLE recurring_feed_binding TYPE datetime",
    "DEFINE INDEX idx_recurring_feed_id ON TABLE recurring_feed_binding COLUMNS recurring_id UNIQUE",
];

static RECURRING_FEED_STORE: Lazy<RwLock<Arc<dyn RecurringFeedStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(InMemoryRecurringFeedStore::default())));

pub fn recurring_feed_store() -> Arc<dyn RecurringFeedStore> {
    RECURRING_FEED_STORE.read().unwrap().clone()
}

pub fn set_recurring_feed_store(store: Arc<dyn RecurringFeedStore>) {
    let mut guard = RECURRING_FEED_STORE.write().unwrap();
    *guard = store;
}

pub async fn init_recurring_feed_store_with_runtime(runtime: &RuntimeComposition) {
    if let RuntimeComposition::Surreal(rt) = runtime {
        let store = SurrealRecurringFeedStore::new(rt.job_store.db());
        if let Err(err) = store.ensure_schema().await {
            eprintln!(
                "Surreal recurring feed store schema init error: {err}; keeping in-memory store"
            );
            return;
        }
        set_recurring_feed_store(Arc::new(store));
        eprintln!("Surreal runtime detected; recurring feed store switched to SurrealDB backend");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum FeedPayloadMode {
    Summary,
    #[default]
    ParsedPoll,
    RawExcerpt,
}


impl FeedPayloadMode {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some("summary") => Self::Summary,
            Some("parsed_poll") => Self::ParsedPoll,
            Some("raw_excerpt") => Self::RawExcerpt,
            _ => Self::ParsedPoll,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::ParsedPoll => "parsed_poll",
            Self::RawExcerpt => "raw_excerpt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecurringFeedBinding {
    pub feed_ids: Vec<String>,
    pub payload_mode: FeedPayloadMode,
}

pub async fn bind_recurring_feed_for_registration(
    recurring_id: &str,
    input: &Value,
) -> StasisResult<(bool, Option<RecurringFeedBinding>)> {
    let bound = persist_recurring_feed_binding(recurring_id, input).await?;
    Ok((bound.is_some(), bound))
}

pub async fn remove_recurring_feed_binding(recurring_id: &str) -> anyhow::Result<()> {
    recurring_feed_store().remove(recurring_id).await
}

pub async fn persist_recurring_feed_binding(
    recurring_id: &str,
    input: &Value,
) -> StasisResult<Option<RecurringFeedBinding>> {
    let Some(feeds_value) = input.get("feeds").filter(|value| !value.is_null()) else {
        return Ok(None);
    };

    let binding = parse_feeds_spec(feeds_value)?;
    recurring_feed_store()
        .upsert(recurring_id, &binding)
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

    Ok(Some(binding))
}

pub fn parse_feeds_spec(value: &Value) -> StasisResult<RecurringFeedBinding> {
    let feed_ids_raw = value
        .get("feed_ids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            StasisError::PortFailure("feeds.feed_ids is required and must be a non-empty array".to_string())
        })?;

    if feed_ids_raw.is_empty() {
        return Err(StasisError::PortFailure(
            "feeds.feed_ids must contain at least one feed id".to_string(),
        ));
    }

    let mut feed_ids = Vec::with_capacity(feed_ids_raw.len());
    for entry in feed_ids_raw {
        let feed_id = entry
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure("feeds.feed_ids entries must be strings".to_string())
            })?;
        if !is_valid_feed_id(feed_id) {
            return Err(StasisError::PortFailure(format!(
                "invalid feed_id={feed_id}; use lowercase dotted ids like trip.london.trains"
            )));
        }
        feed_ids.push(feed_id.to_string());
    }

    let payload_mode = FeedPayloadMode::parse(
        value
            .get("payload_mode")
            .and_then(|v| v.as_str()),
    );

    Ok(RecurringFeedBinding {
        feed_ids,
        payload_mode,
    })
}

pub async fn feed_binding_for_recurring(recurring_id: &str) -> Option<RecurringFeedBinding> {
    recurring_feed_store()
        .get(recurring_id)
        .await
        .ok()
        .flatten()
}

pub fn feeds_spec_schema_fragment() -> Value {
    serde_json::json!({
        "feeds": {
            "type": "object",
            "description": "Environment feed ids to publish each materialized run terminal event.",
            "properties": {
                "feed_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Bounded feed bus ids, e.g. trip.london.trains"
                },
                "payload_mode": {
                    "type": "string",
                    "enum": ["summary", "parsed_poll", "raw_excerpt"],
                    "default": "parsed_poll"
                }
            },
            "required": ["feed_ids"]
        }
    })
}

pub fn feeds_binding_to_json(binding: &RecurringFeedBinding) -> Value {
    serde_json::json!({
        "feed_ids": binding.feed_ids,
        "payload_mode": binding.payload_mode.as_str(),
    })
}

#[async_trait]
pub trait RecurringFeedStore: Send + Sync {
    async fn upsert(&self, recurring_id: &str, binding: &RecurringFeedBinding) -> anyhow::Result<()>;
    async fn get(&self, recurring_id: &str) -> anyhow::Result<Option<RecurringFeedBinding>>;
    async fn remove(&self, recurring_id: &str) -> anyhow::Result<()>;
    async fn count(&self) -> anyhow::Result<usize>;
}

#[derive(Default)]
struct InMemoryRecurringFeedStore {
    bindings: AsyncRwLock<std::collections::HashMap<String, RecurringFeedBinding>>,
}

#[async_trait]
impl RecurringFeedStore for InMemoryRecurringFeedStore {
    async fn upsert(&self, recurring_id: &str, binding: &RecurringFeedBinding) -> anyhow::Result<()> {
        self.bindings
            .write()
            .await
            .insert(recurring_id.to_string(), binding.clone());
        Ok(())
    }

    async fn get(&self, recurring_id: &str) -> anyhow::Result<Option<RecurringFeedBinding>> {
        Ok(self.bindings.read().await.get(recurring_id).cloned())
    }

    async fn remove(&self, recurring_id: &str) -> anyhow::Result<()> {
        self.bindings.write().await.remove(recurring_id);
        Ok(())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.bindings.read().await.len())
    }
}

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
struct RecurringFeedRecord {
    recurring_id: String,
    feed_ids: Vec<String>,
    payload_mode: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
struct SurrealRecurringFeedStore {
    db: Surreal<Any>,
}

impl SurrealRecurringFeedStore {
    fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    fn record_id(recurring_id: &str) -> String {
        recurring_id.replace(':', "_")
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

#[async_trait]
impl RecurringFeedStore for SurrealRecurringFeedStore {
    async fn upsert(&self, recurring_id: &str, binding: &RecurringFeedBinding) -> anyhow::Result<()> {
        let now = Utc::now();
        let record = RecurringFeedRecord {
            recurring_id: recurring_id.to_string(),
            feed_ids: binding.feed_ids.clone(),
            payload_mode: binding.payload_mode.as_str().to_string(),
            created_at: now,
            updated_at: now,
        };
        let id = Self::record_id(recurring_id);
        self.db
            .query("UPSERT type::record($table, $id) CONTENT $data")
            .bind(("table", TABLE))
            .bind(("id", id))
            .bind(("data", record))
            .await?;
        Ok(())
    }

    async fn get(&self, recurring_id: &str) -> anyhow::Result<Option<RecurringFeedBinding>> {
        let id = Self::record_id(recurring_id);
        let mut response = self
            .db
            .query("SELECT * FROM type::record($table, $id)")
            .bind(("table", TABLE))
            .bind(("id", id))
            .await?;

        let record: Option<RecurringFeedRecord> = response.take(0)?;
        Ok(record.map(|row| RecurringFeedBinding {
            feed_ids: row.feed_ids,
            payload_mode: FeedPayloadMode::parse(Some(&row.payload_mode)),
        }))
    }

    async fn remove(&self, recurring_id: &str) -> anyhow::Result<()> {
        let id = Self::record_id(recurring_id);
        self.db
            .query("DELETE type::record($table, $id)")
            .bind(("table", TABLE))
            .bind(("id", id))
            .await?;
        Ok(())
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let mut response = self
            .db
            .query("SELECT count() FROM type::table($table) GROUP ALL")
            .bind(("table", TABLE))
            .await?;
        let rows: Vec<Value> = response.take(0)?;
        let count = rows
            .first()
            .and_then(|row| row.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_feeds_spec_validates_ids() {
        let binding = parse_feeds_spec(&json!({
            "feed_ids": ["trip.london.trains"],
            "payload_mode": "parsed_poll"
        }))
        .expect("valid feeds");

        assert_eq!(binding.feed_ids, vec!["trip.london.trains"]);
        assert_eq!(binding.payload_mode, FeedPayloadMode::ParsedPoll);
    }

    #[test]
    fn parse_feeds_spec_rejects_invalid_id() {
        let err = parse_feeds_spec(&json!({
            "feed_ids": ["Bad-ID"]
        }))
        .unwrap_err();
        assert!(err.to_string().contains("invalid feed_id"));
    }
}
