use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub direction: String,
    pub hash64: String,
    pub byte_size: usize,
    pub stored_at_utc: DateTime<Utc>,
    pub payload_path: String,
}

#[derive(Debug, Clone)]
pub struct StoredArtifact {
    pub record: ArtifactRecord,
    pub payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactMaintenanceReport {
    pub records_before: usize,
    pub records_after: usize,
    pub missing_payload_pruned: usize,
    pub deduped_records_pruned: usize,
    pub retention_pruned: usize,
    pub payload_files_deleted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactIndexStats {
    pub records: usize,
    pub unique_hashes: usize,
    pub total_bytes: usize,
}

pub fn persist_tool_artifact(
    session_id: &str,
    tool_name: &str,
    direction: &str,
    hash64: &str,
    byte_size: usize,
    payload: &Value,
) -> std::result::Result<ArtifactRecord, String> {
    let now = Utc::now();
    let tool_slug = slugify_tool_name(tool_name);
    let hash_short = hash64.chars().take(12).collect::<String>();
    let artifact_id = format!(
        "art:{}:{}:{}:{}",
        short_session(session_id),
        tool_slug,
        direction,
        hash_short
    );

    let payload_dir = artifacts_root()
        .join(session_id)
        .join(&tool_slug)
        .join(direction);
    std::fs::create_dir_all(&payload_dir).map_err(|err| err.to_string())?;

    let payload_path = payload_dir.join(format!("{}.json", hash64));
    if !payload_path.exists() {
        let raw = serde_json::to_vec_pretty(payload).map_err(|err| err.to_string())?;
        std::fs::write(&payload_path, raw).map_err(|err| err.to_string())?;
    }

    let record = ArtifactRecord {
        artifact_id,
        session_id: session_id.to_string(),
        tool_name: tool_name.to_string(),
        direction: direction.to_string(),
        hash64: hash64.to_string(),
        byte_size,
        stored_at_utc: now,
        payload_path: payload_path.to_string_lossy().to_string(),
    };

    append_index_record(&record)?;
    Ok(record)
}

pub fn find_artifact(session_id: &str, query: Option<&str>) -> Option<StoredArtifact> {
    let records = read_index_records();
    if records.is_empty() {
        return None;
    }

    let query = query.map(str::trim).unwrap_or("");
    let mut candidates: Vec<ArtifactRecord> = records
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));

    let record = if query.is_empty() || query.eq_ignore_ascii_case("last") {
        candidates.into_iter().next()
    } else {
        candidates.into_iter().find(|record| {
            record.artifact_id.starts_with(query)
                || record.hash64.starts_with(query)
                || record.tool_name.contains(query)
        })
    }?;

    let payload = std::fs::read_to_string(&record.payload_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())?;

    Some(StoredArtifact { record, payload })
}

pub fn list_artifact_records(session_id: &str, limit: usize) -> Vec<ArtifactRecord> {
    let mut records: Vec<ArtifactRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
    records.into_iter().take(limit.max(1)).collect()
}

pub fn artifact_index_stats(session_id: &str) -> ArtifactIndexStats {
    let records = list_artifact_records(session_id, usize::MAX);
    let mut hashes = HashSet::new();
    let mut total_bytes = 0usize;
    for record in &records {
        hashes.insert(record.hash64.clone());
        total_bytes = total_bytes.saturating_add(record.byte_size);
    }
    ArtifactIndexStats {
        records: records.len(),
        unique_hashes: hashes.len(),
        total_bytes,
    }
}

pub fn run_artifact_maintenance(
    max_per_session: usize,
    max_age_days: i64,
) -> std::result::Result<ArtifactMaintenanceReport, String> {
    let max_per_session = max_per_session.max(1);
    let max_age_days = max_age_days.max(1);

    let mut report = ArtifactMaintenanceReport::default();
    let now = Utc::now();
    let age_cutoff = now - Duration::days(max_age_days);

    let mut records = read_index_records();
    report.records_before = records.len();

    let before_missing = records.len();
    records.retain(|record| Path::new(&record.payload_path).exists());
    report.missing_payload_pruned = before_missing.saturating_sub(records.len());

    let before_dedupe = records.len();
    let mut deduped: HashMap<(String, String, String, String), ArtifactRecord> = HashMap::new();
    for record in records {
        let key = (
            record.session_id.clone(),
            record.tool_name.clone(),
            record.direction.clone(),
            record.hash64.clone(),
        );
        match deduped.get(&key) {
            Some(existing) if existing.stored_at_utc >= record.stored_at_utc => {}
            _ => {
                deduped.insert(key, record);
            }
        }
    }
    let records: Vec<ArtifactRecord> = deduped.into_values().collect();
    report.deduped_records_pruned = before_dedupe.saturating_sub(records.len());

    let mut by_session: HashMap<String, Vec<ArtifactRecord>> = HashMap::new();
    for record in records {
        by_session
            .entry(record.session_id.clone())
            .or_default()
            .push(record);
    }

    let mut kept_records = Vec::new();
    let mut pruned_records = Vec::new();
    for (_session_id, mut group) in by_session {
        group.sort_by(|a, b| b.stored_at_utc.cmp(&a.stored_at_utc));
        for (idx, record) in group.into_iter().enumerate() {
            let too_old = record.stored_at_utc < age_cutoff;
            let over_limit = idx >= max_per_session;
            if too_old || over_limit {
                pruned_records.push(record);
            } else {
                kept_records.push(record);
            }
        }
    }
    report.retention_pruned = pruned_records.len();

    let referenced_payloads: HashSet<String> = kept_records
        .iter()
        .map(|record| record.payload_path.clone())
        .collect();

    let mut payload_files_deleted = 0usize;
    for record in pruned_records {
        if !referenced_payloads.contains(&record.payload_path)
            && std::fs::remove_file(&record.payload_path).is_ok()
        {
            payload_files_deleted = payload_files_deleted.saturating_add(1);
        }
    }
    report.payload_files_deleted = payload_files_deleted;

    kept_records.sort_by(|a, b| a.stored_at_utc.cmp(&b.stored_at_utc));
    overwrite_index_records(&kept_records)?;

    report.records_after = kept_records.len();
    Ok(report)
}

fn append_index_record(record: &ArtifactRecord) -> std::result::Result<(), String> {
    let index_path = artifacts_root().join("index.jsonl");
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

fn overwrite_index_records(records: &[ArtifactRecord]) -> std::result::Result<(), String> {
    let index_path = artifacts_root().join("index.jsonl");
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let temp_path = index_path.with_extension("jsonl.tmp");
    let mut file = std::fs::File::create(&temp_path).map_err(|err| err.to_string())?;
    for record in records {
        let line = serde_json::to_string(record).map_err(|err| err.to_string())?;
        writeln!(file, "{line}").map_err(|err| err.to_string())?;
    }
    std::fs::rename(temp_path, index_path).map_err(|err| err.to_string())?;
    Ok(())
}

fn read_index_records() -> Vec<ArtifactRecord> {
    let index_path = artifacts_root().join("index.jsonl");
    let Ok(file) = std::fs::File::open(index_path) else {
        return Vec::new();
    };

    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ArtifactRecord>(&line).ok())
        .collect()
}

fn artifacts_root() -> PathBuf {
    data_local_medousa_dir().join("artifacts")
}

fn data_local_medousa_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("medousa")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

fn slugify_tool_name(tool_name: &str) -> String {
    let mut out = String::new();
    for ch in tool_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}
