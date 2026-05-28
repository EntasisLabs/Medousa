use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::artifact_chunking::SttpChunkNodeRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceClaim {
    pub claim_id: String,
    pub statement: String,
    pub supporting_chunk_node_ids: Vec<String>,
    pub support_strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionRunRecord {
    pub extraction_id: String,
    pub session_id: String,
    pub artifact_id: String,
    pub created_at_utc: DateTime<Utc>,
    pub claim_count: usize,
    pub output_path: String,
}

#[derive(Debug, Clone)]
pub struct ExtractionRun {
    pub record: ExtractionRunRecord,
    pub claims: Vec<EvidenceClaim>,
}

pub fn extract_claims_from_chunks(
    artifact_id: &str,
    payload: &Value,
    chunk_refs: &[SttpChunkNodeRef],
) -> Vec<EvidenceClaim> {
    let mut claims = Vec::new();

    if let Some(obj) = payload.as_object() {
        if !obj.is_empty() {
            let keys = obj.keys().take(8).cloned().collect::<Vec<_>>().join(", ");
            claims.push(EvidenceClaim {
                claim_id: format!("{}:claim:0", artifact_id),
                statement: format!("Top-level keys observed: {keys}"),
                supporting_chunk_node_ids: chunk_refs
                    .iter()
                    .take(2)
                    .map(|chunk| chunk.node_id.clone())
                    .collect(),
                support_strength: 0.78,
            });
        }

        if let Some(results) = obj.get("results").and_then(|value| value.as_array()) {
            let count = results.len();
            claims.push(EvidenceClaim {
                claim_id: format!("{}:claim:1", artifact_id),
                statement: format!("Results array contains {count} item(s)"),
                supporting_chunk_node_ids: chunk_refs
                    .iter()
                    .skip(1)
                    .take(2)
                    .map(|chunk| chunk.node_id.clone())
                    .collect(),
                support_strength: 0.84,
            });
        }
    }

    if claims.is_empty() {
        claims.push(EvidenceClaim {
            claim_id: format!("{}:claim:0", artifact_id),
            statement: "Payload captured and chunked for downstream extraction".to_string(),
            supporting_chunk_node_ids: chunk_refs
                .iter()
                .take(2)
                .map(|chunk| chunk.node_id.clone())
                .collect(),
            support_strength: 0.65,
        });
    }

    claims
}

pub fn persist_extraction_run(
    session_id: &str,
    artifact_id: &str,
    claims: &[EvidenceClaim],
) -> std::result::Result<ExtractionRunRecord, String> {
    let now = Utc::now();
    let extraction_id = format!(
        "ext:{}:{}",
        short_session(session_id),
        now.timestamp_millis()
    );

    let output_dir = extractions_root().join(session_id);
    std::fs::create_dir_all(&output_dir).map_err(|err| err.to_string())?;
    let output_path = output_dir.join(format!("{}.json", extraction_id));
    let raw = serde_json::to_vec_pretty(claims).map_err(|err| err.to_string())?;
    std::fs::write(&output_path, raw).map_err(|err| err.to_string())?;

    let record = ExtractionRunRecord {
        extraction_id,
        session_id: session_id.to_string(),
        artifact_id: artifact_id.to_string(),
        created_at_utc: now,
        claim_count: claims.len(),
        output_path: output_path.to_string_lossy().to_string(),
    };

    append_index_record(&record)?;
    Ok(record)
}

pub fn find_extraction(session_id: &str, query: Option<&str>) -> Option<ExtractionRun> {
    let mut records: Vec<ExtractionRunRecord> = read_index_records()
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
            record.extraction_id.starts_with(query) || record.artifact_id.starts_with(query)
        })
    }?;

    let claims = std::fs::read_to_string(&record.output_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<EvidenceClaim>>(&raw).ok())?;

    Some(ExtractionRun { record, claims })
}

pub fn list_extraction_runs(session_id: &str, limit: usize) -> Vec<ExtractionRunRecord> {
    let mut records: Vec<ExtractionRunRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
    records.into_iter().take(limit.max(1)).collect()
}

fn append_index_record(record: &ExtractionRunRecord) -> std::result::Result<(), String> {
    let index_path = extractions_root().join("index.jsonl");
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

fn read_index_records() -> Vec<ExtractionRunRecord> {
    let index_path = extractions_root().join("index.jsonl");
    let Ok(file) = std::fs::File::open(index_path) else {
        return Vec::new();
    };

    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ExtractionRunRecord>(&line).ok())
        .collect()
}

fn extractions_root() -> PathBuf {
    data_local_medousa_dir().join("extractions")
}

fn data_local_medousa_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("medousa")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}
