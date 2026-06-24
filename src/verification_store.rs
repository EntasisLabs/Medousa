use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::future::IntoFuture;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::runtime::Handle;

use crate::verifier::{VerificationPolicy, VerificationReport};

const VERIFICATION_INDEX_TABLE: &str = "verification_record";

const VERIFICATION_SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE verification_record SCHEMAFULL",
    "DEFINE FIELD verification_id ON TABLE verification_record TYPE string",
    "DEFINE FIELD session_id ON TABLE verification_record TYPE string",
    "DEFINE FIELD pack_id ON TABLE verification_record TYPE string",
    "DEFINE FIELD artifact_id ON TABLE verification_record TYPE string",
    "DEFINE FIELD selector ON TABLE verification_record TYPE string",
    "DEFINE FIELD source ON TABLE verification_record TYPE string",
    "DEFINE FIELD is_verified ON TABLE verification_record TYPE bool",
    "DEFINE FIELD confidence_score ON TABLE verification_record TYPE float",
    "DEFINE FIELD created_at_utc ON TABLE verification_record TYPE datetime",
    "DEFINE FIELD output_path ON TABLE verification_record TYPE string",
    "DEFINE INDEX idx_verification_record_session ON TABLE verification_record COLUMNS session_id",
    "DEFINE INDEX idx_verification_record_id ON TABLE verification_record COLUMNS verification_id UNIQUE",
];

static VERIFICATION_INDEX_STORE: Lazy<RwLock<Arc<dyn VerificationIndexStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(FileVerificationIndexStore)));

pub async fn init_verification_store_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let store = SurrealVerificationIndexStore::new(rt.job_store.db());
            if let Err(err) = store.ensure_schema().await {
                eprintln!(
                    "Surreal verification index schema init error: {err}; keeping file-backed index"
                );
                return;
            }
            set_verification_index_store(Arc::new(store));
            eprintln!(
                "Surreal runtime detected; verification index switched to SurrealDB backend"
            );
        }
        _ => {}
    }
}

fn set_verification_index_store(store: Arc<dyn VerificationIndexStore>) {
    let mut guard = VERIFICATION_INDEX_STORE.write().unwrap();
    *guard = store;
}

trait VerificationIndexStore: Send + Sync {
    fn read_all(&self) -> Vec<VerificationRunRecord>;
    fn append(&self, record: &VerificationRunRecord) -> std::result::Result<(), String>;
    fn delete_for_session(&self, session_id: &str);
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct VerificationRunRecord {
    pub verification_id: String,
    pub session_id: String,
    pub pack_id: String,
    pub artifact_id: String,
    pub selector: String,
    pub source: String,
    pub is_verified: bool,
    pub confidence_score: f32,
    pub created_at_utc: DateTime<Utc>,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRun {
    pub record: VerificationRunRecord,
    pub policy: VerificationPolicy,
    pub report: VerificationReport,
}

pub fn persist_verification(
    session_id: &str,
    selector: &str,
    source: &str,
    policy: &VerificationPolicy,
    report: &VerificationReport,
) -> std::result::Result<VerificationRunRecord, String> {
    let now = Utc::now();
    let verification_id = format!(
        "verify:{}:{}",
        short_session(session_id),
        now.timestamp_millis()
    );

    let output_dir = verifications_root().join(session_id);
    std::fs::create_dir_all(&output_dir).map_err(|err| err.to_string())?;
    let output_path = output_dir.join(format!("{}.json", verification_id));

    let run = VerificationRun {
        record: VerificationRunRecord {
            verification_id: verification_id.clone(),
            session_id: session_id.to_string(),
            pack_id: report.pack_id.clone(),
            artifact_id: report.artifact_id.clone(),
            selector: selector.to_string(),
            source: source.to_string(),
            is_verified: report.is_verified,
            confidence_score: report.confidence_score,
            created_at_utc: now,
            output_path: output_path.to_string_lossy().to_string(),
        },
        policy: policy.clone(),
        report: report.clone(),
    };

    let raw = serde_json::to_vec_pretty(&run).map_err(|err| err.to_string())?;
    std::fs::write(&output_path, raw).map_err(|err| err.to_string())?;
    append_index_record(&run.record)?;
    crate::session_catalog::record_verification(
        session_id,
        &run.record,
        run.report.citation_coverage,
    );
    Ok(run.record)
}

pub fn read_all_index_records_for_backfill() -> Vec<VerificationRunRecord> {
    read_index_records()
}

pub fn read_verification_coverage(record: &VerificationRunRecord) -> f32 {
    std::fs::read_to_string(&record.output_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<VerificationRun>(&raw).ok())
        .map(|run| run.report.citation_coverage)
        .unwrap_or(0.0)
}

pub fn list_verifications(session_id: &str, limit: usize) -> Vec<VerificationRunRecord> {
    let mut records: Vec<VerificationRunRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
    records.into_iter().take(limit.max(1)).collect()
}

pub fn delete_verifications_for_session(session_id: &str) {
    verification_index_store().delete_for_session(session_id.trim());
    let _ = std::fs::remove_dir_all(verifications_root().join(session_id.trim()));
}

pub fn find_verification(session_id: &str, query: Option<&str>) -> Option<VerificationRun> {
    let mut records: Vec<VerificationRunRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    if records.is_empty() {
        return None;
    }

    records.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
    let query = query.map(str::trim).unwrap_or("");
    let record = if query.is_empty() || query.eq_ignore_ascii_case("last") {
        records.into_iter().next()
    } else {
        records.into_iter().find(|record| {
            record.verification_id.starts_with(query)
                || record.pack_id.starts_with(query)
                || record.artifact_id.starts_with(query)
        })
    }?;

    std::fs::read_to_string(&record.output_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<VerificationRun>(&raw).ok())
}

fn append_index_record(record: &VerificationRunRecord) -> std::result::Result<(), String> {
    verification_index_store().append(record)
}

fn read_index_records() -> Vec<VerificationRunRecord> {
    verification_index_store().read_all()
}

fn verification_index_store() -> Arc<dyn VerificationIndexStore> {
    VERIFICATION_INDEX_STORE.read().unwrap().clone()
}

struct FileVerificationIndexStore;

impl VerificationIndexStore for FileVerificationIndexStore {
    fn read_all(&self) -> Vec<VerificationRunRecord> {
        file_read_index_records()
    }

    fn append(&self, record: &VerificationRunRecord) -> std::result::Result<(), String> {
        file_append_index_record(record)
    }

    fn delete_for_session(&self, session_id: &str) {
        let remaining: Vec<_> = file_read_index_records()
            .into_iter()
            .filter(|record| record.session_id != session_id)
            .collect();
        let path = verifications_root().join("index.jsonl");
        if let Ok(mut file) = std::fs::File::create(path) {
            for record in remaining {
                if let Ok(line) = serde_json::to_string(&record) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
    }
}

struct SurrealVerificationIndexStore {
    db: Surreal<Any>,
}

impl SurrealVerificationIndexStore {
    fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn ensure_schema(&self) -> Result<(), surrealdb::Error> {
        for statement in VERIFICATION_SCHEMA_STATEMENTS {
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

impl VerificationIndexStore for SurrealVerificationIndexStore {
    fn read_all(&self) -> Vec<VerificationRunRecord> {
        let sql = "SELECT * FROM type::table($table) ORDER BY created_at_utc ASC";
        let mut response = match block_on(
            self.db
                .query(sql)
                .bind(("table", VERIFICATION_INDEX_TABLE)),
        ) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("SurrealVerificationIndexStore::read_all query error: {err}");
                return Vec::new();
            }
        };

        response.take::<Vec<VerificationRunRecord>>(0).unwrap_or_default()
    }

    fn append(&self, record: &VerificationRunRecord) -> std::result::Result<(), String> {
        let sql = "UPSERT type::record($table, $id) CONTENT $data";
        block_on(
            self.db
                .query(sql)
                .bind(("table", VERIFICATION_INDEX_TABLE))
                .bind(("id", record.verification_id.clone()))
                .bind(("data", record.clone())),
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn delete_for_session(&self, session_id: &str) {
        let sql = "DELETE type::table($table) WHERE session_id = $session_id";
        let _ = block_on(
            self.db
                .query(sql)
                .bind(("table", VERIFICATION_INDEX_TABLE))
                .bind(("session_id", session_id.trim().to_string())),
        );
    }
}

fn block_on<F: IntoFuture>(f: F) -> F::Output {
    tokio::task::block_in_place(move || Handle::current().block_on(f.into_future()))
}

fn file_append_index_record(record: &VerificationRunRecord) -> std::result::Result<(), String> {
    let index_path = verifications_root().join("index.jsonl");
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())?;
    Ok(())
}

fn file_read_index_records() -> Vec<VerificationRunRecord> {
    let index_path = verifications_root().join("index.jsonl");
    let Ok(file) = std::fs::File::open(index_path) else {
        return Vec::new();
    };

    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<VerificationRunRecord>(&line).ok())
        .collect()
}

fn verifications_root() -> PathBuf {
    crate::paths::medousa_data_dir().join("verifications")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}
