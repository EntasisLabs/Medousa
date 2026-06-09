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
    "DEFINE INDEX idx_session_turn_session_id ON TABLE session_turn COLUMNS session_id",
    "DEFINE INDEX idx_session_turn_timestamp ON TABLE session_turn COLUMNS timestamp",
];

const SESSION_SCHEMA_MIGRATIONS: &[&str] = &[
    "DEFINE FIELD OVERWRITE parts ON TABLE session_turn TYPE option<string>",
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
}

fn parts_to_json(parts: Option<&[TurnPart]>) -> Option<String> {
    parts.and_then(|items| serde_json::to_string(items).ok())
}

fn parts_from_json(value: Option<String>) -> Option<Vec<TurnPart>> {
    value.and_then(|raw| serde_json::from_str(&raw).ok())
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
        }
    }
}

pub trait SessionStore: Send + Sync + 'static {
    fn load_history(&self, session_id: &str) -> Vec<ConversationTurn>;
    fn append_turn(&self, session_id: &str, turn: &ConversationTurn);
    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary>;
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
        crate::session::file_append_turn(session_id, turn)
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session::file_list_history_sessions(limit)
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
        let mut response = match block_on(
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
        }
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        // time::max (not math::max) — Surreal 3 GROUP BY returns -Infinity for math::max(datetime).
        // type::datetime(...) keeps last_timestamp as datetime when the aggregate is numeric.
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
                .bind(("limit", limit as i64)),
        ) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("SurrealSessionStore::list_history_sessions query error: {err}");
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
                eprintln!("SurrealSessionStore::list_history_sessions deserialize error: {err}");
                return Vec::new();
            }
        };

        aggregates
            .into_iter()
            .map(|agg| {
                let turns = self.load_history(&agg.session_id);
                let preview = turns
                    .iter()
                    .rev()
                    .find(|t| !t.content.trim().is_empty())
                    .and_then(|t| t.content.lines().next())
                    .unwrap_or("(empty session)")
                    .chars()
                    .take(72)
                    .collect::<String>();

                let verifications =
                    crate::verification_store::list_verifications(&agg.session_id, usize::MAX);
                let latest_verification =
                    crate::verification_store::find_verification(&agg.session_id, None);

                SessionHistorySummary {
                    session_id: agg.session_id,
                    display_name: None,
                    turns: agg.turns,
                    verification_runs: verifications.len(),
                    last_timestamp: agg.last_timestamp,
                    last_verification_timestamp: latest_verification
                        .as_ref()
                        .map(|run| run.record.created_at_utc),
                    last_verification_confidence: latest_verification
                        .as_ref()
                        .map(|run| run.record.confidence_score),
                    last_verification_coverage: latest_verification
                        .as_ref()
                        .map(|run| run.report.citation_coverage),
                    last_verification_verified: latest_verification
                        .as_ref()
                        .map(|run| run.record.is_verified),
                    preview,
                }
            })
            .collect()
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
