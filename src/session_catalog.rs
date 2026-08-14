//! Read-optimized session index (`session_catalog`) for `GET /v1/sessions`.
//!
//! Maintained at write time so list queries never load full transcripts.

use std::collections::HashMap;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use medousa_types::SessionId;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::identity_memory::DEFAULT_USER_ID;
use crate::session::{ConversationTurn, SessionHistorySummary, atomic_write, medousa_data_dir};
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
    "DEFINE FIELD origin_surface ON TABLE session_catalog TYPE option<string>",
    "DEFINE FIELD has_code_work ON TABLE session_catalog TYPE bool",
    "DEFINE INDEX idx_session_catalog_session_id ON TABLE session_catalog COLUMNS session_id UNIQUE",
];

const SCHEMA_MIGRATIONS: &[&str] =
    &["REMOVE INDEX IF EXISTS idx_session_catalog_last_activity ON TABLE session_catalog"];

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
    /// First sticky non-home host surface for rail channel marks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_surface: Option<String>,
    /// Sticky once a Forge code binding was set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_code_work: bool,
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
            origin_surface: None,
            has_code_work: false,
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
            origin_surface: None,
            has_code_work: false,
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
            catalog: None,
            origin_surface: row.origin_surface,
            has_code_work: row.has_code_work,
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
                TurnPart::UserMedia {
                    label, media_id, ..
                } => label.as_deref().unwrap_or(media_id.as_str()),
                TurnPart::AttachmentRef { label, .. } => label.as_str(),
                TurnPart::HostContext { .. }
                | TurnPart::ModelReceipt { .. }
                | TurnPart::ToolRun { .. }
                | TurnPart::Unknown => continue,
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

/// Supported rail channel surfaces (matches Home `hostContextLabel` keys).
pub fn sticky_origin_surface(source: &str) -> Option<String> {
    match source.trim().to_ascii_lowercase().as_str() {
        "vscode" | "neovim" | "obsidian" | "browser" => Some(source.trim().to_ascii_lowercase()),
        _ => None,
    }
}

fn origin_surface_from_turn(turn: &ConversationTurn) -> Option<String> {
    crate::agent_runtime::host_context::host_context_from_turn(turn)
        .and_then(|context| sticky_origin_surface(&context.source))
}

fn apply_origin_from_turn(row: &mut SessionCatalogRow, turn: &ConversationTurn) {
    if row.origin_surface.is_some() {
        return;
    }
    if let Some(surface) = origin_surface_from_turn(turn) {
        row.origin_surface = Some(surface);
    }
}

/// Sticky mark once a Forge code binding is set (does not clear when unbound).
pub fn mark_has_code_work(session_id: &str) {
    let Ok(session_id) = SessionId::parse(session_id) else {
        return;
    };
    let mut row = catalog_store()
        .get_row(&session_id)
        .unwrap_or_else(|| SessionCatalogRow::empty_session(session_id.as_str()));
    if row.has_code_work {
        return;
    }
    row.has_code_work = true;
    stamp_profile_id(&mut row);
    catalog_store().upsert_row(&session_id, &row);
}

trait SessionCatalogStore: Send + Sync {
    fn upsert_row(&self, session_id: &SessionId, row: &SessionCatalogRow);
    fn delete_row(&self, session_id: &SessionId);
    fn get_row(&self, session_id: &SessionId) -> Option<SessionCatalogRow>;
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

fn catalog_path(session_id: &SessionId) -> PathBuf {
    crate::session_storage::session_file_for_read(&catalog_dir(), session_id, "json")
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
    fn upsert_row(&self, session_id: &SessionId, row: &SessionCatalogRow) {
        assert_eq!(session_id.as_str(), row.session_id);
        self.inner.upsert_row(session_id, row);
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(session_id.to_string(), row.clone());
        }
    }

    fn delete_row(&self, session_id: &SessionId) {
        self.inner.delete_row(session_id);
        if let Ok(mut cache) = self.cache.write() {
            cache.remove(session_id.as_str());
        }
    }

    fn get_row(&self, session_id: &SessionId) -> Option<SessionCatalogRow> {
        if let Ok(cache) = self.cache.read()
            && let Some(row) = cache.get(session_id.as_str())
        {
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
    fn upsert_row(&self, session_id: &SessionId, row: &SessionCatalogRow) {
        assert_eq!(session_id.as_str(), row.session_id);
        let Ok(path) =
            crate::session_storage::session_file_for_write(&catalog_dir(), session_id, "json")
        else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec_pretty(row) else {
            return;
        };
        let _ = atomic_write(&path, &bytes);
    }

    fn delete_row(&self, session_id: &SessionId) {
        let _ = crate::session_storage::remove_session_file(&catalog_dir(), session_id, "json");
    }

    fn get_row(&self, session_id: &SessionId) -> Option<SessionCatalogRow> {
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
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
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
                        entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
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
                let raw = std::fs::read_to_string(path).ok()?;
                let row = serde_json::from_str::<SessionCatalogRow>(&raw).ok()?;
                row.session_id.starts_with(prefix).then_some(row.session_id)
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
    fn upsert_row(&self, session_id: &SessionId, row: &SessionCatalogRow) {
        assert_eq!(session_id.as_str(), row.session_id);
        let session_id = session_id.to_string();
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

    fn delete_row(&self, session_id: &SessionId) {
        let sql = "DELETE type::table($table) WHERE session_id = $session_id";
        let _ = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("session_id", session_id.to_string())),
        );
    }

    fn get_row(&self, session_id: &SessionId) -> Option<SessionCatalogRow> {
        let sql = "SELECT * FROM type::table($table) WHERE session_id = $session_id LIMIT 1";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("session_id", session_id.to_string())),
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
        let mut response = match block_on(self.db.query(sql).bind(("table", SESSION_CATALOG_TABLE)))
        {
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
            eprintln!("Surreal runtime detected; session catalog switched to SurrealDB backend");
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
    let Ok(session_id) = SessionId::parse(session_id) else {
        return;
    };
    record_turn_appended_for_id(&session_id, turn);
}

pub(crate) fn record_turn_appended_for_id(session_id: &SessionId, turn: &ConversationTurn) {
    let session_id_text = session_id.as_str();

    // Shared rooms live in a separate index — never create a conflicting single-catalog row.
    if crate::shared_session_catalog::get_shared_row(session_id_text).is_some() {
        let preview = preview_from_turn(turn);
        let title = if turn.role == "user" {
            auto_title_from_turn(turn)
        } else {
            None
        };
        let _ = crate::shared_session_catalog::touch_shared_session(
            session_id_text,
            preview.as_deref(),
            title.as_deref(),
        );
        if let Some(title) = title.as_deref() {
            let _ = crate::session_meta_store::set_session_display_name(session_id_text, title);
        }
        return;
    }

    let mut row = catalog_store()
        .get_row(session_id)
        .unwrap_or_else(|| SessionCatalogRow::empty_session(session_id_text));

    row.turn_count = row.turn_count.saturating_add(1);
    row.last_activity_at = Some(turn.timestamp);
    if let Some(preview) = preview_from_turn(turn) {
        row.preview = preview;
    } else if row.preview.is_empty() {
        row.preview = "(empty session)".to_string();
    }

    if row.display_name.is_none()
        && let Some(title) = auto_title_from_turn(turn)
    {
        row.display_name = Some(title.clone());
        let _ = crate::session_meta_store::set_session_display_name(session_id_text, &title);
    }

    apply_origin_from_turn(&mut row, turn);

    stamp_profile_id(&mut row);
    catalog_store().upsert_row(session_id, &row);
}

pub fn set_display_name(session_id: &str, display_name: &str) {
    let Ok(session_id) = SessionId::parse(session_id) else {
        return;
    };

    let mut row = catalog_store()
        .get_row(&session_id)
        .unwrap_or_else(|| SessionCatalogRow::named_session(session_id.as_str(), None));

    row.display_name = Some(display_name.to_string());
    stamp_profile_id(&mut row);
    catalog_store().upsert_row(&session_id, &row);
}

pub fn ensure_named_session(session_id: &str, display_name: Option<String>) {
    let Ok(session_id) = SessionId::parse(session_id) else {
        return;
    };

    if catalog_store().get_row(&session_id).is_some() {
        if let Some(name) = display_name {
            set_display_name(session_id.as_str(), &name);
        }
        return;
    }

    let row = SessionCatalogRow {
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
        origin_surface: None,
        has_code_work: false,
    };
    catalog_store().upsert_row(&session_id, &row);
}

pub fn record_verification(
    session_id: &str,
    record: &VerificationRunRecord,
    citation_coverage: f32,
) {
    let Ok(session_id) = SessionId::parse(session_id) else {
        return;
    };

    let mut row = catalog_store()
        .get_row(&session_id)
        .unwrap_or_else(|| SessionCatalogRow::empty_session(session_id.as_str()));

    row.verification_run_count = row.verification_run_count.saturating_add(1);
    row.last_verification_at = Some(record.created_at_utc);
    row.last_verification_confidence = Some(record.confidence_score);
    row.last_verification_coverage = Some(citation_coverage);
    row.last_verification_verified = Some(record.is_verified);

    catalog_store().upsert_row(&session_id, &row);
}

pub fn get_summary(session_id: &str) -> Option<SessionHistorySummary> {
    let session_id = SessionId::parse(session_id).ok()?;
    catalog_store()
        .get_row(&session_id)
        .map(SessionHistorySummary::from)
}

/// Whether `profile_id` may read this session through model-facing history tools.
/// Single-user sessions follow their stamped profile (legacy unstamped rows belong
/// to the default profile); shared rooms require explicit membership.
pub fn session_visible_to_profile(session_id: &str, profile_id: &str) -> bool {
    let Ok(session_id) = SessionId::parse(session_id) else {
        return false;
    };
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return false;
    }
    if let Some(shared) = crate::shared_session_catalog::get_shared_row(session_id.as_str()) {
        return shared.includes_member(profile_id);
    }
    catalog_store()
        .get_row(&session_id)
        .is_some_and(|row| row_matches_profile(&row, profile_id))
}

pub fn session_has_activity(session_id: &str) -> bool {
    let Ok(session_id) = SessionId::parse(session_id) else {
        return false;
    };
    catalog_store()
        .get_row(&session_id)
        .is_some_and(|row| row.turn_count > 0)
}

pub fn delete_catalog_row(session_id: &SessionId) {
    catalog_store().delete_row(session_id);
}

static CATALOG_SYNC_ATTEMPTED: AtomicBool = AtomicBool::new(false);
static RAIL_META_REPAIR_ATTEMPTED: AtomicBool = AtomicBool::new(false);

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
    let rows = catalog_store().list_rows_page(fetch_limit, query, decoded_cursor.as_ref());
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

/// One-shot repair of rail metadata (origin + code marks) without scanning on every list.
pub fn ensure_rail_metadata_repaired() {
    if RAIL_META_REPAIR_ATTEMPTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let code_marks = repair_has_code_work_from_bindings();
    let origins = repair_origin_surfaces_from_history(500);
    if code_marks > 0 || origins > 0 {
        eprintln!(
            "session catalog rail metadata repair: {code_marks} code marks, {origins} origins"
        );
    }
}

fn repair_has_code_work_from_bindings() -> usize {
    let mut count = 0usize;
    for session_id in crate::agent_mode_state::session_ids_with_code_binding() {
        let Ok(session_id) = SessionId::parse(session_id) else {
            continue;
        };
        let Some(mut row) = catalog_store().get_row(&session_id) else {
            continue;
        };
        if row.has_code_work {
            continue;
        }
        row.has_code_work = true;
        catalog_store().upsert_row(&session_id, &row);
        count += 1;
    }
    count
}

/// Scan transcripts once for sessions missing `origin_surface` (capped).
fn repair_origin_surfaces_from_history(limit: usize) -> usize {
    let mut count = 0usize;
    let rows = catalog_store().list_rows_page(limit.max(1), None, None);
    for row in rows {
        if row.origin_surface.is_some() {
            continue;
        }
        let Ok(session_id) = SessionId::parse(&row.session_id) else {
            continue;
        };
        let turns = crate::session_store::get_session_store().load_history(&session_id);
        let Some(surface) = turns.iter().find_map(origin_surface_from_turn) else {
            continue;
        };
        let mut updated = row;
        updated.origin_surface = Some(surface);
        catalog_store().upsert_row(&session_id, &updated);
        count += 1;
    }
    count
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
    let session_id = SessionId::parse(session_id).ok()?;
    catalog_store()
        .get_row(&session_id)
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
    let matches = catalog_store().find_session_ids_by_display_name_lower(&lower, 2);
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
        let Ok(session_id) = SessionId::parse(&summary.session_id) else {
            continue;
        };
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
            origin_surface: summary.origin_surface,
            has_code_work: summary.has_code_work,
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

        catalog_store().upsert_row(&session_id, &row);
        count += 1;
    }

    for (session_id, display_name) in
        crate::session_meta_store::list_session_display_names(usize::MAX)
    {
        let Ok(parsed_session_id) = SessionId::parse(&session_id) else {
            continue;
        };
        if catalog_store().get_row(&parsed_session_id).is_some() {
            continue;
        }
        let row = SessionCatalogRow::named_session(session_id, Some(display_name));
        catalog_store().upsert_row(&parsed_session_id, &row);
        count += 1;
    }

    Ok(count)
}

fn group_latest_verifications() -> (
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
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let tmp = std::env::temp_dir().join(format!("medousa-catalog-test-{suffix}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tempdir");
        unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };
        set_catalog_store(Arc::new(FileSessionCatalogStore));

        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let turn = ConversationTurn::plain("user", "hello world".to_string(), at, vec![], None);
        let session_id = format!("sess-a-{suffix}");
        record_turn_appended(&session_id, &turn);
        record_turn_appended(&session_id, &turn);

        let summary = get_summary(&session_id).expect("summary");
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
            speaker_profile_id: None,
        };
        assert_eq!(
            preview_from_turn(&turn).as_deref(),
            Some("From parts timeline")
        );
    }

    #[test]
    fn auto_title_skips_assistant_turns() {
        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let turn = ConversationTurn::plain("assistant", "I can help".to_string(), at, vec![], None);
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
    fn record_turn_appended_sticky_origin_surface() {
        use crate::turn_parts::TurnPart;
        use medousa_types::HostTurnContext;

        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let tmp = std::env::temp_dir().join(format!("medousa-catalog-origin-{suffix}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tempdir");
        unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };
        set_catalog_store(Arc::new(FileSessionCatalogStore));

        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let session_id = format!("sess-origin-{suffix}");
        let home_turn = ConversationTurn {
            role: "user".into(),
            content: "from home".into(),
            timestamp: at,
            tool_names: vec![],
            answer_state: None,
            parts: Some(vec![TurnPart::HostContext {
                context: HostTurnContext {
                    source: "home".into(),
                    workspace: None,
                    resource_kind: None,
                    resource_path: None,
                    resource_title: None,
                    resource_url: None,
                    language: None,
                    cursor: None,
                    selection: None,
                    document_excerpt: None,
                    diagnostics: vec![],
                    related_resources: vec![],
                },
            }]),
            slice_summary: None,
            speaker_profile_id: None,
        };
        record_turn_appended(&session_id, &home_turn);
        assert!(get_summary(&session_id).unwrap().origin_surface.is_none());

        let vscode_turn = ConversationTurn {
            role: "user".into(),
            content: "from vscode".into(),
            timestamp: at,
            tool_names: vec![],
            answer_state: None,
            parts: Some(vec![TurnPart::HostContext {
                context: HostTurnContext {
                    source: "vscode".into(),
                    workspace: None,
                    resource_kind: Some("file".into()),
                    resource_path: Some("main.rs".into()),
                    resource_title: None,
                    resource_url: None,
                    language: None,
                    cursor: None,
                    selection: None,
                    document_excerpt: None,
                    diagnostics: vec![],
                    related_resources: vec![],
                },
            }]),
            slice_summary: None,
            speaker_profile_id: None,
        };
        record_turn_appended(&session_id, &vscode_turn);
        assert_eq!(
            get_summary(&session_id).unwrap().origin_surface.as_deref(),
            Some("vscode")
        );

        let browser_turn = ConversationTurn {
            role: "user".into(),
            content: "from browser".into(),
            timestamp: at,
            tool_names: vec![],
            answer_state: None,
            parts: Some(vec![TurnPart::HostContext {
                context: HostTurnContext {
                    source: "browser".into(),
                    workspace: None,
                    resource_kind: None,
                    resource_path: None,
                    resource_title: None,
                    resource_url: None,
                    language: None,
                    cursor: None,
                    selection: None,
                    document_excerpt: None,
                    diagnostics: vec![],
                    related_resources: vec![],
                },
            }]),
            slice_summary: None,
            speaker_profile_id: None,
        };
        record_turn_appended(&session_id, &browser_turn);
        assert_eq!(
            get_summary(&session_id).unwrap().origin_surface.as_deref(),
            Some("vscode"),
            "origin stays sticky"
        );
    }

    #[test]
    fn mark_has_code_work_is_sticky() {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let tmp = std::env::temp_dir().join(format!("medousa-catalog-code-{suffix}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tempdir");
        unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };
        set_catalog_store(Arc::new(FileSessionCatalogStore));

        let session_id = format!("sess-code-{suffix}");
        ensure_named_session(&session_id, Some("Code".into()));
        assert!(!get_summary(&session_id).unwrap().has_code_work);
        mark_has_code_work(&session_id);
        assert!(get_summary(&session_id).unwrap().has_code_work);
        mark_has_code_work(&session_id);
        assert!(get_summary(&session_id).unwrap().has_code_work);
    }

    #[test]
    fn sticky_origin_surface_filters_home() {
        assert_eq!(sticky_origin_surface("vscode").as_deref(), Some("vscode"));
        assert_eq!(sticky_origin_surface("HOME"), None);
        assert_eq!(sticky_origin_surface("notion"), None);
    }

    #[test]
    fn list_sessions_page_filters_query_and_cursor() {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let tmp = std::env::temp_dir().join(format!("medousa-catalog-page-test-{suffix}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("tempdir");
        unsafe { std::env::set_var("XDG_DATA_HOME", &tmp) };
        set_catalog_store(Arc::new(FileSessionCatalogStore));

        let needle = format!("budget-unique-{suffix}");
        let at = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap();
        let sess_alpha = format!("sess-alpha-{suffix}");
        let sess_beta = format!("sess-beta-{suffix}");
        for (session_id, preview) in [
            (sess_alpha.as_str(), format!("{needle} planning notes")),
            (sess_beta.as_str(), format!("{needle} morning brief draft")),
        ] {
            let turn = ConversationTurn::plain("user", preview, at, vec![], None);
            record_turn_appended(session_id, &turn);
        }

        let page = list_sessions_page(10, Some(&needle), None, None);
        assert_eq!(page.sessions.len(), 2);
        assert!(
            page.sessions
                .iter()
                .any(|session| session.session_id == sess_alpha)
        );

        let first = list_sessions_page(1, Some(&needle), None, None);
        assert_eq!(first.sessions.len(), 1);
        assert!(first.next_cursor.is_some());

        let second = list_sessions_page(1, Some(&needle), first.next_cursor.as_deref(), None);
        assert_eq!(second.sessions.len(), 1);
        assert_ne!(second.sessions[0].session_id, first.sessions[0].session_id);
    }
}
