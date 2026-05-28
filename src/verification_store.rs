use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::verifier::{VerificationPolicy, VerificationReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    Ok(run.record)
}

pub fn list_verifications(session_id: &str, limit: usize) -> Vec<VerificationRunRecord> {
    let mut records: Vec<VerificationRunRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
    records.into_iter().take(limit.max(1)).collect()
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

fn read_index_records() -> Vec<VerificationRunRecord> {
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
    data_local_medousa_dir().join("verifications")
}

fn data_local_medousa_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("medousa")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}
