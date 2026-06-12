//! Read-optimized session index (`session_catalog`) for `GET /v1/sessions`.
//!
//! Maintained at write time so list queries never load full transcripts.

use std::collections::HashMap;
use std::future::IntoFuture;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::session::{
    atomic_write, medousa_data_dir, ConversationTurn, SessionHistorySummary,
};
use crate::verification_store::VerificationRunRecord;

pub const PREVIEW_MAX_CHARS: usize = 72;

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
    "DEFINE INDEX idx_session_catalog_session_id ON TABLE session_catalog COLUMNS session_id UNIQUE",
    "DEFINE INDEX idx_session_catalog_last_activity ON TABLE session_catalog COLUMNS last_activity_at",
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
        }
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
    preview_line_from_content(&turn.content)
}

trait SessionCatalogStore: Send + Sync {
    fn upsert_row(&self, row: &SessionCatalogRow);
    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow>;
    fn list_rows(&self, limit: usize) -> Vec<SessionCatalogRow>;
    fn row_count(&self) -> usize;
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
    let mut guard = SESSION_CATALOG_STORE.write().unwrap();
    *guard = store;
}

fn catalog_store() -> Arc<dyn SessionCatalogStore> {
    SESSION_CATALOG_STORE.read().unwrap().clone()
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

    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow> {
        let path = catalog_path(session_id);
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn list_rows(&self, limit: usize) -> Vec<SessionCatalogRow> {
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
            .collect::<Vec<_>>();

        rows.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
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
        Ok(())
    }
}

impl SessionCatalogStore for SurrealSessionCatalogStore {
    fn upsert_row(&self, row: &SessionCatalogRow) {
        let sql = "UPSERT type::record($table, $id) CONTENT $data";
        if let Err(err) = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("id", row.session_id.clone()))
                .bind(("data", row.clone())),
        ) {
            eprintln!("SurrealSessionCatalogStore::upsert_row error: {err}");
        }
    }

    fn get_row(&self, session_id: &str) -> Option<SessionCatalogRow> {
        let sql = "SELECT * FROM type::record($table, $id)";
        let mut response = block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("id", session_id.trim().to_string())),
        )
        .ok()?;

        response.take::<Option<SessionCatalogRow>>(0).ok().flatten()
    }

    fn list_rows(&self, limit: usize) -> Vec<SessionCatalogRow> {
        let sql = "SELECT * FROM type::table($table) \
                   ORDER BY last_activity_at DESC \
                   LIMIT $limit";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", SESSION_CATALOG_TABLE))
                .bind(("limit", limit.max(1) as i64)),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealSessionCatalogStore::list_rows error: {err}");
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
}

pub async fn init_session_catalog_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            if let Err(err) = init_surreal_catalog_for_db(rt.job_store.db()).await {
                eprintln!(
                    "Surreal session catalog schema init error: {err}; keeping file-backed catalog"
                );
                return;
            }
            eprintln!("Surreal runtime detected; session catalog switched to SurrealDB backend");
        }
        _ => {}
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

    catalog_store().upsert_row(&SessionCatalogRow::named_session(
        session_id,
        display_name,
    ));
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

pub fn list_sessions(limit: usize) -> Vec<SessionHistorySummary> {
    catalog_store()
        .list_rows(limit.max(1))
        .into_iter()
        .map(SessionHistorySummary::from)
        .collect()
}

fn backfill_if_needed() {
    if catalog_store().row_count() > 0 {
        return;
    }

    if !legacy_sessions_detected() {
        return;
    }

    eprintln!("session catalog empty — backfilling from existing session history…");
    match backfill_from_legacy_stores() {
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

fn backfill_from_legacy_stores() -> Result<usize, String> {
    let (verification_by_session, verification_counts) = group_latest_verifications();
    let mut count = 0usize;

    for summary in crate::session_store::build_backfill_summaries(usize::MAX) {
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
        records.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
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
    }
}
