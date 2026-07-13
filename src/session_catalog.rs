//! Read-optimized session index (`session_catalog`) for `GET /v1/sessions`.
//!
//! Maintained at write time so list queries never load full transcripts.

use std::collections::HashMap;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::identity_memory::DEFAULT_USER_ID;
use crate::session::{
    atomic_write, medousa_data_dir, ConversationTurn, SessionHistorySummary,
};
use crate::turn_parts::TurnPart;
use crate::verification_store::VerificationRunRecord;

pub const PREVIEW_MAX_CHARS: usize = 72;
pub const AUTO_TITLE_MAX_CHARS: usize = 48;

const SESSION_CATALOG_TABLE: &str = "session_catalog";

const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE session_catalog SCHEMAFULL",
    "DEFINE FIELD session_id ON TABLE session_catalog TYPE string",
    "DEFINE FIELD preview ON TABLE session_catalog TYPE string",
    "DEFINE FIELD turn_count ON TABLE session_catalog TYPE int",
    "DEFINE FIELD last_activity_at ON TABLE session_catalog TYPE option<datetime>",
    "DEFINE FIELD display_name ON TABLE session_catalog TYPE option<string>",
    "DEFINE FIELD verification_run_count ON TABLE session_catalog TYPE int",
    "DEFINE FIELD last_verification_at ON TABLE session_catalog TYPE option<datetime>",
    "DEFINE FIELD last_verification_confidence ON TABLE session_catalog TYPE option<float>",
    "DEFINE FIELD last_verification_coverage ON TABLE session_catalog TYPE option<float>",
    "DEFINE FIELD last_verification_verified ON TABLE session_catalog TYPE option<bool>",
    "DEFINE FIELD profile_id ON TABLE session_catalog TYPE option<string>",
    "DEFINE INDEX idx_session_catalog_session_id ON TABLE session_catalog COLUMNS session_id UNIQUE",
];

const SCHEMA_MIGRATIONS: &[&str] = &[
    "REMOVE INDEX IF EXISTS idx_session_catalog_last_activity ON TABLE session_catalog",
];

static SESSION_CATALOG_STORE: Lazy<RwLock<Arc<dyn SessionCatalogStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(FileSessionCatalogStore)));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, SurrealValue)]
pub struct SessionCatalogRow {
    pub session_id: String,
    pub preview: String,
    pub turn_count: usize,
    pub last_activity_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub verification_run_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verification_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verification_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verification_coverage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verification_verified: Option<bool>,
    /// Active profile when the session was created or last written (`user:work`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

impl SessionCatalogRow {
    fn empty_session(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            preview: "(empty session)".to_string(),
            turn_count: 0,
            last_activity_at: None,
            display_name: None,
            verification_run_count: 0,
            last_verification_at: None,
            last_verification_confidence: None,
            last_verification_coverage: None,
            last_verification_verified: None,
            profile_id: None,
        }
    }

    fn named_session(session_id: impl Into<String>, display_name: Option<String>) -> Self {
        Self {
            session_id: session_id.into(),
            preview: "(named session)".to_string(),
            turn_count: 0,
            last_activity_at: None,
            display_name,
            verification_run_count: 0,
            last_verification_at: None,
            last_verification_confidence: None,
            last_verification_coverage: None,
            last_verification_verified: None,
            profile_id: None,
        }
    }
}

fn active_workshop_profile_id() -> String {
    crate::user_profiles::resolve_workshop_identity_user_id()
}

fn stamp_profile_id(row: &mut SessionCatalogRow) {
    if row.profile_id.is_none() {
        row.profile_id = Some(active_workshop_profile_id());
    }
}

pub fn row_matches_profile(row: &SessionCatalogRow, active_profile_id: &str) -> bool {
    match row.profile_id.as_deref() {
        None => active_profile_id == DEFAULT_USER_ID,
        Some(stored) => stored == active_profile_id,
    }
}

impl From<SessionCatalogRow> for SessionHistorySummary {
    fn from(row: SessionCatalogRow) -> Self {
        SessionHistorySummary {
            session_id: row.session_id,
            display_name: row.display_name,
            turns: row.turn_count,
            verification_runs: row.verification_run_count,
            last_timestamp: row.last_activity_at,
            last_verification_timestamp: row.last_verification_at,
            last_verification_confidence: row.last_verification_confidence,
            last_verification_coverage: row.last_verification_coverage,
            last_verification_verified: row.last_verification_verified,
            preview: row.preview,
        }
    }
}

pub fn preview_line_from_content(content: &str) -> Option<String> {
    if content.trim().is_empty() {
        return None;
    }
    Some(
        content
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(PREVIEW_MAX_CHARS)
            .collect(),
    )
}

pub fn preview_from_turn(turn: &ConversationTurn) -> Option<String> {
    turn_text_line(turn, PREVIEW_MAX_CHARS)
}

pub fn auto_title_from_turn(turn: &ConversationTurn) -> Option<String> {
    if turn.role != "user" {
        return None;
    }
    turn_text_line(turn, AUTO_TITLE_MAX_CHARS)
}

pub fn auto_title_from_preview(preview: &str) -> Option<String> {
    preview_line_from_content(preview).map(|line| truncate_chars(&line, AUTO_TITLE_MAX_CHARS))
}

fn turn_text_line(turn: &ConversationTurn, max_chars: usize) -> Option<String> {
    if let Some(line) = preview_line_from_content(&turn.content) {
        return Some(truncate_chars(&line, max_chars));
    }

    turn.parts.as_ref().and_then(|parts| {
        for part in parts {
            let text = match part {
                TurnPart::Text { markdown } | TurnPart::Reasoning { markdown } => markdown,
                TurnPart::Progress { markdown } => markdown,
                TurnPart::Handoff { text, .. } => text,
                TurnPart::UserMedia { label, media_id, .. } => {
                    label.as_deref().unwrap_or(media_id.as_str())
                }
                TurnPart::AttachmentRef { label, .. } => label.as_str(),
                TurnPart::ToolRun { .. } | TurnPart::Unknown => continue,
            };
            if let Some(line) = preview_line_from_content(text) {
                return Some(truncate_chars(&line, max_chars));
            }
        }
        None
    })
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

trait SessionCatalogStore: Send + Sync {
    fn upsert_row(&self, row: &SessionCatalogRow);
    fn delete_row(&self, session_id: &str);
    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow>;
    fn list_rows_page(
        &self,
        limit: usize,
        query: Option<&str>,
        cursor: Option<&SessionListCursor>,
    ) -> Vec<SessionCatalogRow>;
    fn row_count(&self) -> usize;
    fn find_session_ids_by_prefix(&self, prefix: &str, max: usize) -> Vec<String>;
    fn find_session_ids_by_display_name_lower(&self, lower: &str, max: usize) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListCursor {
    pub last_activity_at: DateTime<Utc>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListPage {
    pub sessions: Vec<SessionHistorySummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

pub fn encode_list_cursor(row: &SessionCatalogRow) -> String {
    let at = row
        .last_activity_at
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now));
    format!("{}|{}", at.to_rfc3339(), row.session_id)
}

pub fn decode_list_cursor(raw: &str) -> Option<SessionListCursor> {
    let raw = raw.trim();
    let (at_raw, session_id) = raw.rsplit_once('|')?;
    let last_activity_at = chrono::DateTime::parse_from_rfc3339(at_raw)
        .ok()?
        .with_timezone(&Utc);
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return None;
    }
    Some(SessionListCursor {
        last_activity_at,
        session_id: session_id.to_string(),
    })
}

fn row_matches_query(row: &SessionCatalogRow, query: &str) -> bool {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return true;
    }
    row.session_id.to_ascii_lowercase().contains(&needle)
        || row.preview.to_ascii_lowercase().contains(&needle)
        || row
            .display_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(&needle))
}

fn row_is_older_than_cursor(row: &SessionCatalogRow, cursor: &SessionListCursor) -> bool {
    let row_at = row
        .last_activity_at
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now));
    row_at < cursor.last_activity_at
        || (row_at == cursor.last_activity_at && row.session_id < cursor.session_id)
}

fn sort_rows_by_recency(rows: &mut [SessionCatalogRow]) {
    rows.sort_by(|a, b| {
        b.last_activity_at
            .cmp(&a.last_activity_at)
            .then_with(|| b.session_id.cmp(&a.session_id))
    });
}

fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

fn catalog_dir() -> PathBuf {
    medousa_data_dir().join("catalog")
}

fn catalog_path(session_id: &str) -> PathBuf {
    catalog_dir().join(format!("{session_id}.json"))
}

fn set_catalog_store(store: Arc<dyn SessionCatalogStore>) {
    // Wrap every configured store in a write-through row cache so the per-append
    // `get_row` read (a `block_on` SurrealKV SELECT) is served from memory.
    let cached: Arc<dyn SessionCatalogStore> = Arc::new(CachingSessionCatalogStore::new(store));
    let mut guard = SESSION_CATALOG_STORE.write().unwrap();
    *guard = cached;
}

fn catalog_store() -> Arc<dyn SessionCatalogStore> {
    SESSION_CATALOG_STORE.read().unwrap().clone()
}

/// Write-through row cache layered over any `SessionCatalogStore`.
///
/// `record_turn_appended` does a `get_row` before every persisted turn; against
/// SurrealKV that is a blocking SELECT. The daemon is the single writer and every
/// catalog mutation funnels through `upsert_row`/`delete_row`, so caching `get_row`
/// here stays coherent (no stale write-back of e.g. verification fields) while
/// removing one DB round-trip per append. List/count/find queries pass through to
/// the backing store, which is always kept fresh by the write-through.
struct CachingSessionCatalogStore {
    inner: Arc<dyn SessionCatalogStore>,
    cache: RwLock<HashMap<String, SessionCatalogRow>>,
}

impl CachingSessionCatalogStore {
    fn new(inner: Arc<dyn SessionCatalogStore>) -> Self {
        Self {
            inner,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

impl SessionCatalogStore for CachingSessionCatalogStore {
    fn upsert_row(&self, row: &SessionCatalogRow) {
        self.inner.upsert_row(row);
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(row.session_id.clone(), row.clone());
        }
    }

    fn delete_row(&self, session_id: &str) {
        self.inner.delete_row(session_id);
        if let Ok(mut cache) = self.cache.write() {
            cache.remove(session_id);
        }
    }

    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow> {
        if let Ok(cache) = self.cache.read()
            && let Some(row) = cache.get(session_id) {
                return Some(row.clone());
            }
        let row = self.inner.get_row(session_id)?;
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(session_id.to_string(), row.clone());
        }
        Some(row)
    }

    fn list_rows_page(
        &self,
        limit: usize,
        query: Option<&str>,
        cursor: Option<&SessionListCursor>,
    ) -> Vec<SessionCatalogRow> {
        self.inner.list_rows_page(limit, query, cursor)
    }

    fn row_count(&self) -> usize {
        self.inner.row_count()
    }

    fn find_session_ids_by_prefix(&self, prefix: &str, max: usize) -> Vec<String> {
        self.inner.find_session_ids_by_prefix(prefix, max)
    }

    fn find_session_ids_by_display_name_lower(&self, lower: &str, max: usize) -> Vec<String> {
        self.inner
            .find_session_ids_by_display_name_lower(lower, max)
    }
}

struct FileSessionCatalogStore;

impl SessionCatalogStore for FileSessionCatalogStore {
    fn upsert_row(&self, row: &SessionCatalogRow) {
        let path = catalog_path(&row.session_id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(bytes) = serde_json::to_vec_pretty(row) else {
            return;
        };
        let _ = atomic_write(&path, &bytes);
    }

    fn delete_row(&self, session_id: &str) {
        let path = catalog_path(session_id);
        let _ = std::fs::remove_file(path);
    }

    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow> {
        let path = catalog_path(session_id);
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn list_rows_page(
        &self,
        limit: usize,
        query: Option<&str>,
        cursor: Option<&SessionListCursor>,
    ) -> Vec<SessionCatalogRow> {
        let dir = catalog_dir();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut rows = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    == Some("json")
            })
            .filter_map(|entry| {
                let raw = std::fs::read_to_string(entry.path()).ok()?;
                serde_json::from_str::<SessionCatalogRow>(&raw).ok()
            })
            .filter(|row| query.is_none_or(|needle| row_matches_query(row, needle)))
            .filter(|row| cursor.is_none_or(|cursor| row_is_older_than_cursor(row, cursor)))
            .collect::<Vec<_>>();

        sort_rows_by_recency(&mut rows);
        rows.truncate(limit.max(1));
        rows
    }

    fn row_count(&self) -> usize {
        let dir = catalog_dir();
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        entry
                            .path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            == Some("json")
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    fn find_session_ids_by_prefix(&self, prefix: &str, max: usize) -> Vec<String> {
        let prefix = prefix.trim();
        if prefix.is_empty() {
            return Vec::new();
        }

        let dir = catalog_dir();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    return None;
                }
                let stem = path.file_stem()?.to_string_lossy();
                if stem.starts_with(prefix) {
                    Some(stem.to_string())
                } else {
                    None
                }
            })
            .take(max.max(1))
            .collect()
    }

    fn find_session_ids_by_display_name_lower(&self, lower: &str, max: usize) -> Vec<String> {
        if lower.is_empty() {
            return Vec::new();
        }

        let dir = catalog_dir();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut matches = Vec::new();
        for entry in entries.filter_map(|entry| entry.ok()) {
            if matches.len() >= max.max(1) {
                break;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(row) = serde_json::from_str::<SessionCatalogRow>(&raw) else {
                continue;
            };
            if row
                .display_name
                .as_deref()
                .is_some_and(|name| name.to_ascii_lowercase() == lower)
            {
                matches.push(row.session_id);
            }
        }
        matches
    }
}

struct SurrealSessionCatalogStore {
    db: Surreal<Any>,
}

impl SurrealSessionCatalogStore {
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
        for statement in SCHEMA_MIGRATIONS {
            let _ = self.db.query(*statement).await;
        }
        Ok(())
    }
}

impl SessionCatalogStore for SurrealSessionCatalogStore {
    fn upsert_row(&self, row: &SessionCatalogRow) {
        let session_id = row.session_id.clone();
        let update_sql = "UPDATE type::table($table) MERGE $data WHERE session_id = $session_id";
        let update = block_on(
            self.db
                .query(update_sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("session_id", session_id.clone()))
                .bind(("data", row.clone())),
        );

        match update {
            Ok(mut response) => {
                #[derive(Debug, Deserialize, SurrealValue)]
                struct UpdatedRow {
                    session_id: String,
                }
                let updated: Vec<UpdatedRow> = response.take(0).unwrap_or_default();
                if !updated.is_empty() {
                    return;
                }
            }
            Err(err) => {
                eprintln!("SurrealSessionCatalogStore::upsert_row update error: {err}");
            }
        }

        let create_sql = "CREATE type::table($table) CONTENT $data";
        if let Err(err) = block_on(
            self.db
                .query(create_sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("data", row.clone())),
        ) {
            eprintln!("SurrealSessionCatalogStore::upsert_row create error: {err}");
        }
    }

    fn delete_row(&self, session_id: &str) {
        let sql = "DELETE type::table($table) WHERE session_id = $session_id";
        let _ = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("session_id", session_id.trim().to_string())),
        );
    }

    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow> {
        let sql = "SELECT * FROM type::table($table) WHERE session_id = $session_id LIMIT 1";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("session_id", session_id.trim().to_string())),
        )
        .ok()?;

        response
            .take::<Vec<SessionCatalogRow>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
    }

    fn list_rows_page(
        &self,
        limit: usize,
        query: Option<&str>,
        cursor: Option<&SessionListCursor>,
    ) -> Vec<SessionCatalogRow> {
        let q_lower = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase);

        let sql = if q_lower.is_some() {
            "SELECT * FROM type::table($table) \
             WHERE ($cursor_at IS NONE OR last_activity_at < $cursor_at \
                    OR (last_activity_at = $cursor_at AND session_id < $cursor_id)) \
               AND (string::contains(string::lowercase(session_id), $q_lower) \
                    OR string::contains(string::lowercase(preview), $q_lower) \
                    OR (display_name != NONE \
                        AND string::contains(string::lowercase(display_name), $q_lower))) \
             ORDER BY last_activity_at DESC, session_id DESC \
             LIMIT $limit"
        } else {
            "SELECT * FROM type::table($table) \
             WHERE ($cursor_at IS NONE OR last_activity_at < $cursor_at \
                    OR (last_activity_at = $cursor_at AND session_id < $cursor_id)) \
             ORDER BY last_activity_at DESC, session_id DESC \
             LIMIT $limit"
        };

        let mut query_builder = self
            .db
            .query(sql)
            .bind(("table", SESSION_CATALOG_TABLE))
            .bind(("limit", limit.max(1) as i64));

        if let Some(cursor) = cursor {
            query_builder = query_builder
                .bind(("cursor_at", cursor.last_activity_at))
                .bind(("cursor_id", cursor.session_id.clone()));
        } else {
            query_builder = query_builder.bind(("cursor_at", None::<DateTime<Utc>>));
            query_builder = query_builder.bind(("cursor_id", None::<String>));
        }

        if let Some(q_lower) = q_lower {
            query_builder = query_builder.bind(("q_lower", q_lower));
        }

        let mut response = match block_on(query_builder) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealSessionCatalogStore::list_rows_page error: {err}");
                return Vec::new();
            }
        };

        response.take(0).unwrap_or_default()
    }

    fn row_count(&self) -> usize {
        let sql = "SELECT count() AS total FROM type::table($table) GROUP ALL";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE)),
        ) {
            Ok(response) => response,
            Err(_) => return 0,
        };

        #[derive(Debug, Deserialize, SurrealValue)]
        struct CountRow {
            total: usize,
        }

        response
            .take::<Vec<CountRow>>(0)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .map(|row| row.total)
            .unwrap_or(0)
    }

    fn find_session_ids_by_prefix(&self, prefix: &str, max: usize) -> Vec<String> {
        let prefix = prefix.trim().to_string();
        if prefix.is_empty() {
            return Vec::new();
        }

        let sql = "SELECT session_id FROM type::table($table) \
                   WHERE string::starts_with(session_id, $prefix) \
                   LIMIT $limit";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("prefix", prefix))
                .bind(("limit", max.max(1) as i64)),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealSessionCatalogStore::find_session_ids_by_prefix error: {err}");
                return Vec::new();
            }
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

    fn find_session_ids_by_display_name_lower(&self, lower: &str, max: usize) -> Vec<String> {
        if lower.is_empty() {
            return Vec::new();
        }

        let sql = "SELECT session_id FROM type::table($table) \
                   WHERE display_name != NONE \
                     AND string::lowercase(display_name) = $lower \
                   LIMIT $limit";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("lower", lower.to_string()))
                .bind(("limit", max.max(1) as i64)),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!(
                    "SurrealSessionCatalogStore::find_session_ids_by_display_name_lower error: {err}"
                );
                return Vec::new();
            }
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

pub async fn init_session_catalog_with_runtime(runtime: &RuntimeComposition) {
    if let RuntimeComposition::Surreal(rt) = runtime {
        if let Err(err) = init_surreal_catalog_for_db(rt.job_store.db()).await {
            eprintln!(
                "Surreal session catalog schema init error: {err}; keeping file-backed catalog"
            );
        } else {
            eprintln!(
                "Surreal runtime detected; session catalog switched to SurrealDB backend"
            );
        }
    }

    backfill_if_needed();
}

pub async fn init_surreal_catalog_for_db(db: Surreal<Any>) -> Result<(), surrealdb::Error> {
    let store = SurrealSessionCatalogStore::new(db);
    store.ensure_schema().await?;
    set_catalog_store(Arc::new(store));
    Ok(())
}

pub fn record_turn_appended(session_id: &str, turn: &ConversationTurn) {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return;
    }

    let mut row = catalog_store()
        .get_row(session_id)
        .unwrap_or_else(|| SessionCatalogRow::empty_session(session_id));

    row.turn_count = row.turn_count.saturating_add(1);
    row.last_activity_at = Some(turn.timestamp);
    if let Some(preview) = preview_from_turn(turn) {
        row.preview = preview;
    } else if row.preview.is_empty() {
        row.preview = "(empty session)".to_string();
    }

    if row.display_name.is_none()
        && let Some(title) = auto_title_from_turn(turn) {
            row.display_name = Some(title.clone());
            let _ = crate::session_meta_store::set_session_display_name(session_id, &title);
        }

    stamp_profile_id(&mut row);
    catalog_store().upsert_row(&row);
}

pub fn set_display_name(session_id: &str, display_name: &str) {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return;
    }

    let mut row = catalog_store()
        .get_row(session_id)
        .unwrap_or_else(|| SessionCatalogRow::named_session(session_id, None));

    row.display_name = Some(display_name.to_string());
    stamp_profile_id(&mut row);
    catalog_store().upsert_row(&row);
}

pub fn ensure_named_session(session_id: &str, display_name: Option<String>) {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return;
    }

    if catalog_store().get_row(session_id).is_some() {
        if let Some(name) = display_name {
            set_display_name(session_id, &name);
        }
        return;
    }

    catalog_store().upsert_row(&SessionCatalogRow {
        session_id: session_id.to_string(),
        preview: "(named session)".to_string(),
        turn_count: 0,
        last_activity_at: None,
        display_name,
        verification_run_count: 0,
        last_verification_at: None,
        last_verification_confidence: None,
        last_verification_coverage: None,
        last_verification_verified: None,
        profile_id: Some(active_workshop_profile_id()),
    });
}

pub fn record_verification(
    session_id: &str,
    record: &VerificationRunRecord,
    citation_coverage: f32,
) {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return;
    }

    let mut row = catalog_store()
        .get_row(session_id)
        .unwrap_or_else(|| SessionCatalogRow::empty_session(session_id));

    row.verification_run_count = row.verification_run_count.saturating_add(1);
    row.last_verification_at = Some(record.created_at_utc);
    row.last_verification_confidence = Some(record.confidence_score);
    row.last_verification_coverage = Some(citation_coverage);
    row.last_verification_verified = Some(record.is_verified);

    catalog_store().upsert_row(&row);
}

pub fn get_summary(session_id: &str) -> Option<SessionHistorySummary> {
    catalog_store()
        .get_row(session_id.trim())
        .map(SessionHistorySummary::from)
}

pub fn session_has_activity(session_id: &str) -> bool {
    catalog_store()
        .get_row(session_id.trim())
        .is_some_and(|row| row.turn_count > 0)
}

pub fn delete_catalog_row(session_id: &str) {
    catalog_store().delete_row(session_id.trim());
}

static CATALOG_SYNC_ATTEMPTED: AtomicBool = AtomicBool::new(false);

pub fn list_sessions(limit: usize) -> Vec<SessionHistorySummary> {
    list_sessions_page(limit, None, None, None).sessions
}

/// Chat session ids belonging to a specific profile (export/import).
pub fn list_chat_session_ids_for_profile(profile_id: &str, limit: usize) -> Vec<String> {
    let limit = limit.max(1);
    catalog_store()
        .list_rows_page(limit, None, None)
        .into_iter()
        .filter(|row| row_matches_profile(row, profile_id))
        .map(|row| row.session_id)
        .collect()
}

pub fn list_sessions_page(
    limit: usize,
    query: Option<&str>,
    cursor: Option<&str>,
    active_profile_id: Option<&str>,
) -> SessionListPage {
    let limit = limit.max(1);
    let decoded_cursor = cursor.and_then(decode_list_cursor);
    let fetch_limit = limit.saturating_add(1);
    let rows = catalog_store().list_rows_page(
        fetch_limit,
        query,
        decoded_cursor.as_ref(),
    );
    let has_more = rows.len() > limit;
    let page_rows: Vec<_> = rows
        .into_iter()
        .filter(|row| {
            active_profile_id.is_none_or(|profile_id| row_matches_profile(row, profile_id))
        })
        .take(limit)
        .collect();
    let next_cursor = if has_more {
        page_rows.last().map(encode_list_cursor)
    } else {
        None
    };
    SessionListPage {
        sessions: page_rows
            .into_iter()
            .map(SessionHistorySummary::from)
            .collect(),
        next_cursor,
    }
}

/// One-shot repair when the catalog is empty but legacy session data exists.
pub fn ensure_catalog_populated(limit: usize) {
    if catalog_store().row_count() > 0 {
        return;
    }
    if CATALOG_SYNC_ATTEMPTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if !legacy_sessions_detected() {
        return;
    }
    eprintln!("session catalog empty — syncing from session store…");
    match sync_catalog_from_session_store(limit.max(500)) {
        Ok(count) => eprintln!("session catalog sync complete ({count} sessions)"),
        Err(err) => eprintln!("session catalog sync error: {err}"),
    }
}

fn sync_catalog_from_session_store(limit: usize) -> Result<usize, String> {
    backfill_from_legacy_stores(limit)
}

pub fn turn_count(session_id: &str) -> Option<usize> {
    catalog_store()
        .get_row(session_id.trim())
        .map(|row| row.turn_count)
}

pub fn find_unique_session_id_by_prefix(prefix: &str) -> Option<String> {
    let matches = catalog_store().find_session_ids_by_prefix(prefix, 2);
    if matches.len() == 1 {
        Some(matches[0].clone())
    } else {
        None
    }
}

pub fn find_unique_session_id_by_display_name_case_insensitive(name: &str) -> Option<String> {
    let lower = name.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    let matches = catalog_store()
        .find_session_ids_by_display_name_lower(&lower, 2);
    if matches.len() == 1 {
        Some(matches[0].clone())
    } else {
        None
    }
}

fn backfill_if_needed() {
    if catalog_store().row_count() > 0 {
        CATALOG_SYNC_ATTEMPTED.store(true, Ordering::SeqCst);
        return;
    }
    if CATALOG_SYNC_ATTEMPTED.load(Ordering::SeqCst) {
        return;
    }
    if !legacy_sessions_detected() {
        return;
    }
    CATALOG_SYNC_ATTEMPTED.store(true, Ordering::SeqCst);
    eprintln!("session catalog empty — backfilling from existing session history…");
    match sync_catalog_from_session_store(500) {
        Ok(count) => eprintln!("session catalog backfill complete ({count} sessions)"),
        Err(err) => eprintln!("session catalog backfill error: {err}"),
    }
}

fn legacy_sessions_detected() -> bool {
    if crate::session_store::has_persisted_sessions() {
        return true;
    }

    !crate::session_meta_store::list_session_display_names(1).is_empty()
}

fn backfill_from_legacy_stores(limit: usize) -> Result<usize, String> {
    let (verification_by_session, verification_counts) = group_latest_verifications();
    let mut count = 0usize;

    for summary in crate::session_store::build_backfill_summaries(limit) {
        let mut row = SessionCatalogRow {
            session_id: summary.session_id.clone(),
            preview: summary.preview,
            turn_count: summary.turns,
            last_activity_at: summary.last_timestamp,
            display_name: summary.display_name,
            verification_run_count: summary.verification_runs,
            last_verification_at: summary.last_verification_timestamp,
            last_verification_confidence: summary.last_verification_confidence,
            last_verification_coverage: summary.last_verification_coverage,
            last_verification_verified: summary.last_verification_verified,
            profile_id: None,
        };

        if let Some((record, coverage)) = verification_by_session.get(&summary.session_id) {
            row.verification_run_count = verification_counts
                .get(&summary.session_id)
                .copied()
                .unwrap_or(summary.verification_runs)
                .max(summary.verification_runs);
            row.last_verification_at = Some(record.created_at_utc);
            row.last_verification_confidence = Some(record.confidence_score);
            row.last_verification_coverage = Some(*coverage);
            row.last_verification_verified = Some(record.is_verified);
        }

        if row.display_name.is_none() {
            row.display_name = auto_title_from_preview(&row.preview);
        }

        catalog_store().upsert_row(&row);
        count += 1;
    }

    for (session_id, display_name) in
        crate::session_meta_store::list_session_display_names(usize::MAX)
    {
        if catalog_store().get_row(&session_id).is_some() {
            continue;
        }
        catalog_store().upsert_row(&SessionCatalogRow::named_session(
            session_id,
            Some(display_name),
        ));
        count += 1;
    }

    Ok(count)
}

fn group_latest_verifications(
) -> (
    HashMap<String, (VerificationRunRecord, f32)>,
    HashMap<String, usize>,
) {
    let mut grouped: HashMap<String, Vec<VerificationRunRecord>> = HashMap::new();
    for record in crate::verification_store::read_all_index_records_for_backfill() {
        grouped
            .entry(record.session_id.clone())
            .or_default()
            .push(record);
    }

    let mut latest = HashMap::new();
    let mut counts = HashMap::new();
    for (session_id, mut records) in grouped {
        counts.insert(session_id.clone(), records.len());
        records.sort_by_key(|b| std::cmp::Reverse(b.created_at_utc));
        let Some(record) = records.into_iter().next() else {
            continue;
        };
        let coverage = crate::verification_store::read_verification_coverage(&record);
        latest.insert(session_id, (record, coverage));
    }
    (latest, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn preview_truncates_first_line() {
        let long = "a".repeat(120);
        let preview = preview_line_from_content(&long).expect("preview");
        assert_eq!(preview.chars().count(), PREVIEW_MAX_CHARS);
    }

    #[test]
    fn record_turn_appended_increments_count() {
        let tmp = std::env::temp_dir().join(format!(
            "medousa-catalog-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tempdir");
        unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };
        set_catalog_store(Arc::new(FileSessionCatalogStore));

        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let turn = ConversationTurn::plain(
            "user",
            "hello world".to_string(),
            at,
            vec![],
            None,
        );
        record_turn_appended("sess-a", &turn);
        record_turn_appended("sess-a", &turn);

        let summary = get_summary("sess-a").expect("summary");
        assert_eq!(summary.turns, 2);
        assert_eq!(summary.preview, "hello world");
        assert_eq!(summary.display_name.as_deref(), Some("hello world"));
    }

    #[test]
    fn preview_from_turn_reads_text_parts() {
        use crate::turn_parts::TurnPart;

        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let turn = ConversationTurn {
            role: "assistant".into(),
            content: String::new(),
            timestamp: at,
            tool_names: vec![],
            answer_state: None,
            parts: Some(vec![TurnPart::Text {
                markdown: "From parts timeline".into(),
            }]),
            slice_summary: None,
        };
        assert_eq!(
            preview_from_turn(&turn).as_deref(),
            Some("From parts timeline")
        );
    }

    #[test]
    fn auto_title_skips_assistant_turns() {
        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let turn = ConversationTurn::plain(
            "assistant",
            "I can help".to_string(),
            at,
            vec![],
            None,
        );
        assert!(auto_title_from_turn(&turn).is_none());
    }

    #[test]
    fn row_matches_profile_legacy_visible_under_default_only() {
        let legacy = SessionCatalogRow::empty_session("legacy-sess");
        assert!(row_matches_profile(&legacy, DEFAULT_USER_ID));
        assert!(!row_matches_profile(&legacy, "user:work"));

        let work = SessionCatalogRow {
            profile_id: Some("user:work".to_string()),
            ..SessionCatalogRow::empty_session("work-sess")
        };
        assert!(row_matches_profile(&work, "user:work"));
        assert!(!row_matches_profile(&work, DEFAULT_USER_ID));
    }

    #[test]
    fn list_sessions_page_filters_query_and_cursor() {
        let tmp = std::env::temp_dir().join(format!(
            "medousa-catalog-page-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tempdir");
        unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };
        set_catalog_store(Arc::new(FileSessionCatalogStore));

        let needle = format!("budget-unique-{}", std::process::id());
        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        for (session_id, preview) in [
            ("sess-alpha", format!("{needle} planning notes")),
            ("sess-beta", "Morning brief draft".to_string()),
        ] {
            let turn = ConversationTurn::plain(
                "user",
                preview,
                at,
                vec![],
                None,
            );
            record_turn_appended(session_id, &turn);
        }

        let page = list_sessions_page(10, Some(&needle), None, None);
        assert_eq!(page.sessions.len(), 1);
        assert_eq!(page.sessions[0].session_id, "sess-alpha");

        let first = list_sessions_page(1, None, None, None);
        assert_eq!(first.sessions.len(), 1);
        assert!(first.next_cursor.is_some());

        let second = list_sessions_page(1, None, first.next_cursor.as_deref(), None);
        assert_eq!(second.sessions.len(), 1);
        assert_ne!(second.sessions[0].session_id, first.sessions[0].session_id);
    }
}
