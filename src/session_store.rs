use std::fmt;
use std::future::IntoFuture;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use medousa_types::SessionId;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::session::{ConversationTurn, SessionHistorySummary};
use crate::turn_parts::TurnPart;
use crate::turn_slice::TurnSliceSummary;

const SESSION_TURN_TABLE: &str = "session_turn";
pub const MAX_TRANSCRIPT_SEARCH_QUERY_CHARS: usize = 512;

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
    "DEFINE FIELD speaker_profile_id ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD search_text ON TABLE session_turn TYPE option<string>",
    "DEFINE INDEX idx_session_turn_session_id ON TABLE session_turn COLUMNS session_id",
    "DEFINE INDEX idx_session_turn_timestamp ON TABLE session_turn COLUMNS timestamp",
    "DEFINE ANALYZER medousa_transcript TOKENIZERS BLANK,CLASS,PUNCT FILTERS LOWERCASE",
    "DEFINE INDEX idx_session_turn_search ON TABLE session_turn FIELDS search_text FULLTEXT ANALYZER medousa_transcript BM25 HIGHLIGHTS",
];

const SESSION_SCHEMA_MIGRATIONS: &[&str] = &[
    "DEFINE FIELD OVERWRITE parts ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE slice_summary ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE speaker_profile_id ON TABLE session_turn TYPE option<string>",
    "DEFINE FIELD OVERWRITE search_text ON TABLE session_turn TYPE option<string>",
    "UPDATE session_turn SET search_text = content WHERE search_text = NONE OR search_text = NULL",
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
                eprintln!(
                    "Surreal session store schema init error: {err}; falling back to file-backed store"
                );
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speaker_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TranscriptSearchMatch {
    pub session_id: String,
    pub role: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub excerpt: String,
}

fn turn_search_text(turn: &ConversationTurn) -> Option<String> {
    if !matches!(turn.role.as_str(), "user" | "assistant" | "agent") {
        return None;
    }
    let content = turn.content.trim();
    if !content.is_empty() {
        return Some(content.to_string());
    }
    let text = turn
        .parts
        .as_deref()?
        .iter()
        .filter_map(|part| match part {
            TurnPart::Text { markdown } | TurnPart::Progress { markdown } => Some(markdown.trim()),
            TurnPart::Handoff { text, .. } => Some(text.trim()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

fn transcript_excerpt(text: &str, query: &str) -> String {
    const EXCERPT_CHARS: usize = 420;
    const LEADING_CONTEXT_CHARS: usize = 90;

    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = collapsed.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let start = lower
        .find(&needle)
        .map(|byte| {
            collapsed[..byte]
                .chars()
                .count()
                .saturating_sub(LEADING_CONTEXT_CHARS)
        })
        .unwrap_or(0);
    let excerpt = collapsed
        .chars()
        .skip(start)
        .take(EXCERPT_CHARS)
        .collect::<String>();
    if start > 0 {
        format!("…{excerpt}")
    } else {
        excerpt
    }
}

fn validate_transcript_search(query: &str) -> Result<(), StoreError> {
    if query.trim().is_empty() {
        return Err(StoreError::InvalidInput(
            "transcript search query cannot be empty".to_string(),
        ));
    }
    if query.chars().count() > MAX_TRANSCRIPT_SEARCH_QUERY_CHARS {
        return Err(StoreError::InvalidInput(format!(
            "transcript search query exceeds {MAX_TRANSCRIPT_SEARCH_QUERY_CHARS} characters"
        )));
    }
    Ok(())
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
            speaker_profile_id: record.speaker_profile_id,
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
            speaker_profile_id: turn.speaker_profile_id.clone(),
            search_text: turn_search_text(turn),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitDurability {
    /// The bytes were accepted by the capability-owned filesystem handle.
    FilesystemWrite,
    /// The database accepted the complete batch as one statement.
    DatabaseCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitReceipt {
    pub turns: usize,
    pub bytes: usize,
    pub durability: CommitDurability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidInput(String),
    Serialization(String),
    Backend(String),
    Worker(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::Serialization(message) => write!(formatter, "serialization failed: {message}"),
            Self::Backend(message) => write!(formatter, "store backend failed: {message}"),
            Self::Worker(message) => write!(formatter, "store worker failed: {message}"),
        }
    }
}

impl std::error::Error for StoreError {}

#[async_trait]
pub trait SessionStore: Send + Sync + 'static {
    fn load_history(&self, session_id: &SessionId) -> Vec<ConversationTurn>;
    async fn append_turn_batch(
        &self,
        session_id: &SessionId,
        turns: &[ConversationTurn],
    ) -> Result<CommitReceipt, StoreError>;

    async fn append_turn(
        &self,
        session_id: &SessionId,
        turn: &ConversationTurn,
    ) -> Result<CommitReceipt, StoreError> {
        self.append_turn_batch(session_id, std::slice::from_ref(turn))
            .await
    }
    fn delete_session(&self, session_id: &SessionId) -> Result<(), String>;
    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary>;
    fn search_transcripts(
        &self,
        session_ids: &[String],
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptSearchMatch>, StoreError>;
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

struct FileSessionStore {
    files: Arc<crate::session_storage::SessionFileStore>,
}

impl FileSessionStore {
    fn new() -> Self {
        Self::at(crate::session::medousa_data_dir().join("history"))
    }

    fn at(root: std::path::PathBuf) -> Self {
        Self {
            files: Arc::new(crate::session_storage::SessionFileStore::new(root, "jsonl")),
        }
    }
}

#[async_trait]
impl SessionStore for FileSessionStore {
    fn load_history(&self, session_id: &SessionId) -> Vec<ConversationTurn> {
        crate::session::file_load_history(&self.files, session_id)
    }

    async fn append_turn_batch(
        &self,
        session_id: &SessionId,
        turns: &[ConversationTurn],
    ) -> Result<CommitReceipt, StoreError> {
        if turns.is_empty() {
            return Ok(CommitReceipt {
                turns: 0,
                bytes: 0,
                durability: CommitDurability::FilesystemWrite,
            });
        }

        let files = Arc::clone(&self.files);
        let session_id = session_id.clone();
        let turns = turns.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut bytes = Vec::new();
            for turn in &turns {
                serde_json::to_writer(&mut bytes, turn)
                    .map_err(|error| StoreError::Serialization(error.to_string()))?;
                bytes.push(b'\n');
            }
            files
                .append(&session_id, &bytes)
                .map_err(|error| StoreError::Backend(error.to_string()))?;
            for turn in &turns {
                crate::session_catalog::record_turn_appended_for_id(&session_id, turn);
            }
            Ok(CommitReceipt {
                turns: turns.len(),
                bytes: bytes.len(),
                durability: CommitDurability::FilesystemWrite,
            })
        })
        .await
        .map_err(|error| StoreError::Worker(error.to_string()))?
    }

    fn delete_session(&self, session_id: &SessionId) -> Result<(), String> {
        self.files
            .remove(session_id)
            .map_err(|error| error.to_string())?;
        if self
            .files
            .contains(session_id)
            .map_err(|error| error.to_string())?
        {
            return Err("session transcript remains after deletion".to_string());
        }
        Ok(())
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session_catalog::list_sessions(limit)
    }

    fn search_transcripts(
        &self,
        session_ids: &[String],
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptSearchMatch>, StoreError> {
        validate_transcript_search(query)?;
        let needle = query.to_ascii_lowercase();
        let mut hits = session_ids
            .iter()
            .flat_map(|session_id| {
                let needle = needle.clone();
                let parsed = SessionId::parse(session_id).ok();
                parsed
                    .map(|id| self.load_history(&id))
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(move |turn| {
                        let text = turn_search_text(&turn)?;
                        text.to_ascii_lowercase()
                            .contains(&needle)
                            .then(|| TranscriptSearchMatch {
                                session_id: session_id.clone(),
                                role: if turn.role == "agent" {
                                    "assistant".to_string()
                                } else {
                                    turn.role
                                },
                                timestamp: turn.timestamp,
                                excerpt: transcript_excerpt(&text, query),
                            })
                    })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
        hits.truncate(limit);
        Ok(hits)
    }

    fn build_backfill_summaries(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session::file_build_history_summaries_from_files(&self.files, limit)
    }

    fn has_persisted_sessions(&self) -> bool {
        self.files.list().is_ok_and(|entries| !entries.is_empty())
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

#[async_trait]
impl SessionStore for SurrealSessionStore {
    fn load_history(&self, session_id: &SessionId) -> Vec<ConversationTurn> {
        let sql = "SELECT session_id, role, content, timestamp, tool_names, answer_state, parts, \
                    slice_summary, speaker_profile_id \
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

    async fn append_turn_batch(
        &self,
        session_id: &SessionId,
        turns: &[ConversationTurn],
    ) -> Result<CommitReceipt, StoreError> {
        if turns.is_empty() {
            return Ok(CommitReceipt {
                turns: 0,
                bytes: 0,
                durability: CommitDurability::DatabaseCommit,
            });
        }

        let records = turns
            .iter()
            .map(|turn| {
                let mut record = SessionTurnRecord::from(turn);
                record.session_id = session_id.to_string();
                record
            })
            .collect::<Vec<_>>();
        let bytes = records
            .iter()
            .map(|record| serde_json::to_vec(record).map(|value| value.len()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| StoreError::Serialization(error.to_string()))?
            .into_iter()
            .sum();

        let response = self
            .db
            .query("INSERT INTO session_turn $data")
            .bind(("data", records))
            .await
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        response
            .check()
            .map_err(|error| StoreError::Backend(error.to_string()))?;

        for turn in turns {
            crate::session_catalog::record_turn_appended_for_id(session_id, turn);
        }
        Ok(CommitReceipt {
            turns: turns.len(),
            bytes,
            durability: CommitDurability::DatabaseCommit,
        })
    }

    fn delete_session(&self, session_id: &SessionId) -> Result<(), String> {
        let sql = "DELETE type::table($table) WHERE session_id = $session_id";
        let response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("session_id", session_id.to_string())),
        )
        .map_err(|error| error.to_string())?;
        response.check().map_err(|error| error.to_string())?;
        let mut verify = block_on(
            self.db
                .query("SELECT * FROM type::table($table) WHERE session_id = $session_id LIMIT 1")
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("session_id", session_id.to_string())),
        )
        .map_err(|error| error.to_string())?;
        if !verify
            .take::<Vec<SessionTurnRecord>>(0)
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            return Err("session transcript rows remain after deletion".to_string());
        }
        Ok(())
    }

    fn list_history_sessions(&self, limit: usize) -> Vec<SessionHistorySummary> {
        crate::session_catalog::list_sessions(limit)
    }

    fn search_transcripts(
        &self,
        session_ids: &[String],
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptSearchMatch>, StoreError> {
        validate_transcript_search(query)?;
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
        struct SearchRow {
            session_id: String,
            role: String,
            search_text: Option<String>,
            timestamp: chrono::DateTime<chrono::Utc>,
        }

        let sql = "SELECT session_id, role, search_text, timestamp \
                   FROM type::table($table) \
                   WHERE session_id IN $session_ids \
                     AND role IN ['user', 'assistant', 'agent'] \
                     AND search_text @@ $query \
                   ORDER BY timestamp DESC \
                   LIMIT $limit";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_TURN_TABLE))
                .bind(("session_ids", session_ids.to_vec()))
                .bind(("query", query.to_string()))
                .bind(("limit", limit as i64)),
        )
        .map_err(|error| StoreError::Backend(error.to_string()))?;
        let rows = response
            .take::<Vec<SearchRow>>(0)
            .map_err(|error| StoreError::Serialization(error.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let text = row.search_text?;
                Some(TranscriptSearchMatch {
                    session_id: row.session_id,
                    role: if row.role == "agent" {
                        "assistant".to_string()
                    } else {
                        row.role
                    },
                    timestamp: row.timestamp,
                    excerpt: transcript_excerpt(&text, query),
                })
            })
            .collect())
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
                eprintln!("SurrealSessionStore::build_backfill_summaries deserialize error: {err}");
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
                    catalog: None,
                    origin_surface: None,
                    has_code_work: false,
                }
            })
            .collect()
    }

    fn has_persisted_sessions(&self) -> bool {
        let sql = "SELECT count() AS total FROM type::table($table) GROUP ALL";
        let mut response = match block_on(self.db.query(sql).bind(("table", SESSION_TURN_TABLE))) {
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
                speaker_profile_id: None,
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

static SESSION_STORE: Lazy<RwLock<Arc<dyn SessionStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(FileSessionStore::new())));

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

pub fn delete_session_transcript(session_id: &SessionId) -> Result<(), String> {
    get_session_store().delete_session(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn turn(content: &str) -> ConversationTurn {
        ConversationTurn {
            role: "assistant".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts: None,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_receipt_is_visible_to_a_fresh_store_instance() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-receipt-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let session_id = SessionId::parse("receipt-reload").unwrap();
        let store = FileSessionStore::at(root.clone());
        let receipt = store
            .append_turn_batch(&session_id, &[turn("one"), turn("two")])
            .await
            .unwrap();
        assert_eq!(receipt.turns, 2);
        assert!(receipt.bytes > 0);

        let reopened = FileSessionStore::at(root.clone());
        let history = reopened.load_history(&session_id);
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].content, "two");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_backend_failure_never_produces_a_receipt() {
        let parent = std::env::temp_dir().join(format!(
            "medousa-session-failure-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&parent, b"not a directory").unwrap();
        let store = FileSessionStore::at(parent.join("history"));
        let session_id = SessionId::parse("receipt-failure").unwrap();
        let error = store
            .append_turn(&session_id, &turn("must fail"))
            .await
            .unwrap_err();
        assert!(matches!(error, StoreError::Backend(_)));
        std::fs::remove_file(parent).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn file_search_returns_visible_matching_turns_newest_first() {
        let root = std::env::temp_dir().join(format!(
            "medousa-session-search-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let first_id = SessionId::parse("search-first").unwrap();
        let second_id = SessionId::parse("search-second").unwrap();
        let store = FileSessionStore::at(root.clone());
        let mut older = turn("The phoenix project ships tomorrow");
        older.timestamp = Utc::now() - chrono::Duration::minutes(1);
        let newer = turn("A newer Phoenix project update");
        store.append_turn(&first_id, &older).await.unwrap();
        store.append_turn(&second_id, &newer).await.unwrap();

        let hits = store
            .search_transcripts(
                &[first_id.to_string(), second_id.to_string()],
                "phoenix project",
                10,
            )
            .unwrap();

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].session_id, second_id.as_str());
        assert_eq!(hits[0].role, "assistant");
        assert!(hits[0].excerpt.contains("Phoenix project"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transcript_search_rejects_unbounded_queries() {
        let query = "x".repeat(MAX_TRANSCRIPT_SEARCH_QUERY_CHARS + 1);
        assert!(matches!(
            validate_transcript_search(&query),
            Err(StoreError::InvalidInput(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn surreal_search_uses_full_text_index_and_session_scope() {
        let db = surrealdb::engine::any::connect("mem://").await.unwrap();
        db.use_ns("session-search-tests")
            .use_db(uuid::Uuid::new_v4().simple().to_string())
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        SurrealSessionStore::ensure_schema_for_db(&db)
            .await
            .unwrap();
        let store = SurrealSessionStore::new(db);
        let visible_id = SessionId::parse("visible-search").unwrap();
        let hidden_id = SessionId::parse("hidden-search").unwrap();
        store
            .append_turn(&visible_id, &turn("Phoenix transcript sentinel"))
            .await
            .unwrap();
        store
            .append_turn(&hidden_id, &turn("Phoenix hidden sentinel"))
            .await
            .unwrap();

        let hits = store
            .search_transcripts(&[visible_id.to_string()], "phoenix", 10)
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, visible_id.as_str());
        assert!(hits[0].excerpt.contains("Phoenix transcript sentinel"));
    }
}
