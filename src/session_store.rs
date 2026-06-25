use std::future::IntoFuture;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::session::{ConversationTurn, SessionHistorySummary};
use crate::turn_parts::TurnPart;
use crate::turn_slice::TurnSliceSummary;
use stasis::prelude::RuntimeComposition;

const SESSION_TURN_TABLE: &str = "session_turn";

const SESSION_SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE session_turn SCHEMAFULL",
    "DEFINE FIELD session_id ON TABLE session_turn TYPE string",
    "DEFINE FIELD role ON TABLE session_turn TYPE string",
    "DEFINE FIELD content ON TABLE session_turn TYPE string",
    "DEFINE FIELD timestamp ON TABLE session_turn TYPE datetime",
    "DEFINE FIELD tool_names ON TABLE session_turn TYPE array<string>",
    "DEFINE FIELD answer_state ON TABLE session_turn TYPE option<string>",
    // JSON-serialized TurnPart[] — kept as string so SCHEMAFULL does not reject nested arrays.
    "DEFINE FIELD parts ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD slice_summary ON TABLE session_turn TYPE option<string>",
    "DEFINE INDEX idx_session_turn_session_id ON TABLE session_turn COLUMNS session_id",
    "DEFINE INDEX idx_session_turn_timestamp ON TABLE session_turn COLUMNS timestamp",
];

const SESSION_SCHEMA_MIGRATIONS: &[&str] = &[
    "DEFINE FIELD OVERWRITE parts ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE slice_summary ON TABLE session_turn TYPE option<string>",
];

/// Initialize the session store based on the runtime composition.
/// When a Surreal runtime is active, swaps the file-backed store for a
/// SurrealDB-backed implementation.
pub async fn init_session_store_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let db = rt.job_store.db();
            let store = SurrealSessionStore::new(db);
            if let Err(err) = store.ensure_schema().await {
                eprintln!("Surreal session store schema init error: {err}; falling back to file-backed store");
                return;
            }
            set_session_store(Arc::new(store));
            eprintln!("Surreal runtime detected; session store switched to SurrealDB backend");
        }
        _ => {
            // Keep file-backed store for in-memory runtimes.
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct SessionTurnRecord {
    session_id: String,
    role: String,
    content: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parts: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    slice_summary: Option<String>,
}

fn parts_to_json(parts: Option<&[TurnPart]>) -> Option<String> {
    parts.and_then(|items| serde_json::to_string(items).ok())
}

fn parts_from_json(value: Option<String>) -> Option<Vec<TurnPart>> {
    let raw = value?;
    if let Ok(parts) = serde_json::from_str::<Vec<TurnPart>>(&raw) {
        return (!parts.is_empty()).then_some(parts);
    }
    // Tolerant reload: keep recognized parts if the array mixed in an unknown kind.
    let items: Vec<serde_json::Value> = serde_json::from_str(&raw).ok()?;
    let parts = items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<TurnPart>(item).ok())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then_some(parts)
}

fn slice_summary_from_json(value: Option<String>) -> Option<TurnSliceSummary> {
    value.and_then(|raw| serde_json::from_str(&raw).ok())
}

fn slice_summary_to_json(value: Option<&TurnSliceSummary>) -> Option<String> {
    value.and_then(|summary| serde_json::to_string(summary).ok())
}

impl From<SessionTurnRecord> for ConversationTurn {
    fn from(record: SessionTurnRecord) -> Self {
        ConversationTurn {
            role: record.role,
            content: record.content,
            timestamp: record.timestamp,
            tool_names: record.tool_names,
            answer_state: record.answer_state,
            parts: parts_from_json(record.parts),
            slice_summary: slice_summary_from_json(record.slice_summary),
        }
    }
}

impl From<&ConversationTurn> for SessionTurnRecord {
    fn from(turn: &ConversationTurn) -> Self {
        SessionTurnRecord {
            session_id: String::new(), // filled in by caller
            role: turn.role.clone(),
            content: turn.content.clone(),
            timestamp: turn.timestamp,
            tool_names: turn.tool_names.clone(),
            answer_state: turn.answer_state.clone(),
            parts: parts_to_json(turn.parts.as_deref()),
            slice_summary: slice_summary_to_json(turn.slice_summary.as_ref()),
        }
    }
}

pub trait SessionStore: Send + Sync + 'static {
    fn load_history(&self, session_id: &str) -> Vec<ConversationTurn>;
    fn append_turn(&self, session_id: &str, turn: &ConversationTurn);
    fn delete_session(&self, session_id: &str);
    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary>;
    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary>;
    fn has_persisted_sessions(&self) -> bool;
}

/// Helper: run an `IntoFuture` on the current Tokio runtime from a sync context.
/// SAFETY: must be called from within a Tokio runtime (daemon, TUI, or test).
fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

// ---------------------------------------------------------------------------
// File-backed store (original)
// ---------------------------------------------------------------------------

struct FileSessionStore;

impl FileSessionStore {
    fn new() -> Self {
        FileSessionStore {}
    }
}

impl SessionStore for FileSessionStore {
    fn load_history(&self, session_id: &str) -> Vec<ConversationTurn> {
        crate::session::file_load_history(session_id)
    }

    fn append_turn(&self, session_id: &str, turn: &ConversationTurn) {
        crate::session::file_append_turn(session_id, turn);
        crate::session_catalog::record_turn_appended(session_id, turn);
    }

    fn delete_session(&self, session_id: &str) {
        let path = crate::session::medousa_data_dir()
            .join("history")
            .join(format!("{session_id}.jsonl"));
        let _ = std::fs::remove_file(path);
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session_catalog::list_sessions(limit)
    }

    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session::file_build_history_summaries_from_files(limit)
    }

    fn has_persisted_sessions(&self) -> bool {
        let history_dir = crate::session::medousa_data_dir().join("history");
        std::fs::read_dir(history_dir).ok().is_some_and(|mut entries| {
            entries.any(|entry| {
                entry.ok().is_some_and(|item| {
                    item.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        == Some("jsonl")
                })
            })
        })
    }
}

// ---------------------------------------------------------------------------
// SurrealDB-backed store
// ---------------------------------------------------------------------------

pub struct SurrealSessionStore {
    db: Surreal<Any>,
}

impl SurrealSessionStore {
    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    pub async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        Self::ensure_schema_for_db(&self.db).await
    }

    pub async fn ensure_schema_for_db(db: &Surreal<Any>) -> Result<(), surrealdb::Error> {
        for statement in SESSION_SCHEMA_STATEMENTS {
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
        for statement in SESSION_SCHEMA_MIGRATIONS {
            db.query(*statement).await?;
        }
        Ok(())
    }
}

impl SessionStore for SurrealSessionStore {
    fn load_history(&self, session_id: &str) -> Vec<ConversationTurn> {
        let sql = "SELECT session_id, role, content, timestamp, tool_names, answer_state, parts \
                    FROM type::table($table) \
                    WHERE session_id = $session_id \
                    ORDER BY timestamp ASC";
        let session_id_owned = session_id.to_string();
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("session_id", session_id_owned)),
        ) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("SurrealSessionStore::load_history query error: {err}");
                return Vec::new();
            }
        };

        match response.take::<Vec<SessionTurnRecord>>(0) {
            Ok(records) => records.into_iter().map(ConversationTurn::from).collect(),
            Err(err) => {
                eprintln!("SurrealSessionStore::load_history deserialize error: {err}");
                Vec::new()
            }
        }
    }

    fn append_turn(&self, session_id: &str, turn: &ConversationTurn) {
        let mut record = SessionTurnRecord::from(turn);
        record.session_id = session_id.to_string();

        let sql = "CREATE type::table($table) CONTENT $data";
        let response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("data", record)),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealSessionStore::append_turn query error: {err}");
                return;
            }
        };

        if let Err(err) = response.check() {
            eprintln!("SurrealSessionStore::append_turn error: {err}");
            return;
        }

        crate::session_catalog::record_turn_appended(session_id, turn);
    }

    fn delete_session(&self, session_id: &str) {
        let sql = "DELETE type::table($table) WHERE session_id = $session_id";
        let _ = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("session_id", session_id.trim().to_string())),
        );
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session_catalog::list_sessions(limit)
    }

    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary> {
        let sql = "SELECT session_id, \
                           count() AS turns, \
                           type::datetime(time::max(timestamp)) AS last_timestamp \
                    FROM type::table($table) \
                    GROUP BY session_id \
                    ORDER BY last_timestamp DESC \
                    LIMIT $limit";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("limit", limit.max(1) as i64)),
        ) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("SurrealSessionStore::build_backfill_summaries query error: {err}");
                return Vec::new();
            }
        };

        #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
        struct SessionAggregate {
            session_id: String,
            turns: usize,
            last_timestamp: Option<chrono::DateTime<chrono::Utc>>,
        }

        let aggregates: Vec<SessionAggregate> = match response.take(0) {
            Ok(rows) => rows,
            Err(err) => {
                eprintln!(
                    "SurrealSessionStore::build_backfill_summaries deserialize error: {err}"
                );
                return Vec::new();
            }
        };

        aggregates
            .into_iter()
            .map(|agg| {
                let preview = self
                    .preview_for_session(&agg.session_id)
                    .unwrap_or_else(|| "(empty session)".to_string());
                SessionHistorySummary {
                    session_id: agg.session_id,
                    display_name: None,
                    turns: agg.turns,
                    verification_runs: 0,
                    last_timestamp: agg.last_timestamp,
                    last_verification_timestamp: None,
                    last_verification_confidence: None,
                    last_verification_coverage: None,
                    last_verification_verified: None,
                    preview,
                }
            })
            .collect()
    }

    fn has_persisted_sessions(&self) -> bool {
        let sql = "SELECT count() AS total FROM type::table($table) GROUP ALL";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE)),
        ) {
            Ok(response) => response,
            Err(_) => return false,
        };

        #[derive(Debug, Deserialize, SurrealValue)]
        struct CountRow {
            total: usize,
        }

        response
            .take::<Vec<CountRow>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .is_some_and(|row| row.total > 0)
    }
}

impl SurrealSessionStore {
    fn preview_for_session(&self, session_id: &str) -> Option<String> {
        let sql = "SELECT role, content, parts FROM type::table($table) \
                   WHERE session_id = $session_id \
                   ORDER BY timestamp DESC \
                   LIMIT 8";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("session_id", session_id.to_string())),
        )
        .ok()?;

        #[derive(Debug, Deserialize, SurrealValue)]
        struct TurnPreviewRow {
            role: String,
            content: String,
            parts: Option<String>,
        }

        let rows: Vec<TurnPreviewRow> = response.take(0).ok()?;
        for row in rows {
            let turn = ConversationTurn {
                role: row.role,
                content: row.content,
                timestamp: chrono::Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
                parts: parts_from_json(row.parts),
                slice_summary: None,
            };
            if let Some(preview) = crate::session_catalog::preview_from_turn(&turn) {
                return Some(preview);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Global singleton & public helpers
// ---------------------------------------------------------------------------

static SESSION_STORE: Lazy<RwLock<Arc<dyn SessionStore>>> = Lazy::new(|| {
    RwLock::new(Arc::new(FileSessionStore::new()))
});

pub fn set_session_store(store: Arc<dyn SessionStore>) {
    let mut guard = SESSION_STORE.write().unwrap();
    *guard = store;
}

pub fn get_session_store() -> Arc<dyn SessionStore> {
    SESSION_STORE.read().unwrap().clone()
}

pub fn build_backfill_summaries(limit: usize) -> Vec<SessionHistorySummary> {
    get_session_store().build_backfill_summaries(limit)
}

pub fn has_persisted_sessions() -> bool {
    get_session_store().has_persisted_sessions()
}

pub fn delete_session_transcript(session_id: &str) {
    get_session_store().delete_session(session_id.trim());
}
