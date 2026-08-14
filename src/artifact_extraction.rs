use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::artifact_chunking::SttpChunkNodeRef;
use crate::store_root::StorePath;

const EXTRACTION_OBJECT_DOMAIN: &[u8] = b"extraction";

static EXTRACTION_STORE: Lazy<crate::session_storage::SessionDirectoryStore> = Lazy::new(|| {
    crate::session_storage::SessionDirectoryStore::new(
        crate::paths::medousa_data_dir().join("extractions"),
    )
});

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
    let (session_id, _mutation) = crate::session_deletion::acquire_mutation_for_str(session_id)?;
    let now = Utc::now();
    let extraction_id = format!(
        "ext:{}:{}",
        short_session(session_id.as_str()),
        now.timestamp_millis()
    );

    let output_path = extraction_path(&extraction_id);
    let raw = serde_json::to_vec_pretty(claims).map_err(|err| err.to_string())?;
    EXTRACTION_STORE
        .atomic_write(&session_id, &output_path, &raw)
        .map_err(|err| err.to_string())?;

    let record = ExtractionRunRecord {
        extraction_id,
        session_id: session_id.to_string(),
        artifact_id: artifact_id.to_string(),
        created_at_utc: now,
        claim_count: claims.len(),
        output_path: output_path.file_name().to_string(),
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

    records.sort_by_key(|b| std::cmp::Reverse(b.created_at_utc));
    let query = query.map(str::trim).unwrap_or("");
    let record = if query.is_empty() || query.eq_ignore_ascii_case("last") {
        records.into_iter().next()
    } else {
        records.into_iter().find(|record| {
            record.extraction_id.starts_with(query) || record.artifact_id.starts_with(query)
        })
    }?;

    let session_id = crate::session_storage::SessionId::parse(&record.session_id).ok()?;
    let claims = EXTRACTION_STORE
        .read(&session_id, &extraction_path(&record.extraction_id))
        .ok()
        .and_then(|raw| serde_json::from_slice::<Vec<EvidenceClaim>>(&raw).ok())?;

    Some(ExtractionRun { record, claims })
}

pub fn list_extraction_runs(session_id: &str, limit: usize) -> Vec<ExtractionRunRecord> {
    let mut records: Vec<ExtractionRunRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by_key(|b| std::cmp::Reverse(b.created_at_utc));
    records.into_iter().take(limit.max(1)).collect()
}

pub fn delete_extractions_for_session(session_id: &str) -> Result<(), String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    let remaining = read_index_records()
        .into_iter()
        .filter(|record| record.session_id != session_id.as_str())
        .collect::<Vec<_>>();
    overwrite_index_records(&remaining)?;
    EXTRACTION_STORE
        .remove_session(&session_id)
        .map_err(|error| error.to_string())?;
    if read_index_records()
        .iter()
        .any(|record| record.session_id == session_id.as_str())
        || EXTRACTION_STORE
            .contains_session(&session_id)
            .map_err(|error| error.to_string())?
    {
        return Err("extraction session data remains after deletion".to_string());
    }
    Ok(())
}

fn append_index_record(record: &ExtractionRunRecord) -> std::result::Result<(), String> {
    let mut line = serde_json::to_vec(record).map_err(|err| err.to_string())?;
    line.push(b'\n');
    EXTRACTION_STORE
        .append_root(&index_path(), &line)
        .map_err(|err| err.to_string())
}

fn read_index_records() -> Vec<ExtractionRunRecord> {
    let Ok(bytes) = EXTRACTION_STORE.read_root(&index_path()) else {
        return Vec::new();
    };
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .filter_map(|line| serde_json::from_slice::<ExtractionRunRecord>(line).ok())
        .collect()
}

fn overwrite_index_records(records: &[ExtractionRunRecord]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
    }
    EXTRACTION_STORE
        .atomic_write_root(&index_path(), &bytes)
        .map_err(|error| error.to_string())
}

fn extraction_path(extraction_id: &str) -> StorePath {
    crate::session_storage::session_object_path(EXTRACTION_OBJECT_DOMAIN, extraction_id, "json")
}

fn index_path() -> StorePath {
    StorePath::parse("index.jsonl").expect("static extraction index path must be valid")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}
