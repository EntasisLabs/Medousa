use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::artifact_chunking::SttpChunkNodeRef;
use crate::artifact_extraction::EvidenceClaim;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPackBudgetProfile {
    pub max_tokens: usize,
    pub max_claims: usize,
    pub max_chunks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    pub pack_id: String,
    pub session_id: String,
    pub artifact_id: String,
    pub created_at_utc: DateTime<Utc>,
    pub budget_profile: ContextPackBudgetProfile,
    pub selected_claims: Vec<EvidenceClaim>,
    pub selected_chunk_refs: Vec<SttpChunkNodeRef>,
    pub total_token_estimate: usize,
}

#[derive(Debug, Clone)]
pub struct BuildContextPackInput {
    pub session_id: String,
    pub artifact_id: String,
    pub claims: Vec<EvidenceClaim>,
    pub chunk_refs: Vec<SttpChunkNodeRef>,
    pub budget_profile: ContextPackBudgetProfile,
}

pub fn build_context_pack(input: BuildContextPackInput) -> ContextPack {
    let now = Utc::now();
    let pack_id = format!(
        "pack:{}:{}",
        short_session(&input.session_id),
        now.timestamp_millis()
    );

    let selected_claims = input
        .claims
        .into_iter()
        .take(input.budget_profile.max_claims.max(1))
        .collect::<Vec<_>>();

    let mut selected_chunk_refs = Vec::new();
    let mut token_estimate = 0usize;
    for chunk in input
        .chunk_refs
        .into_iter()
        .take(input.budget_profile.max_chunks.max(1))
    {
        let next = token_estimate.saturating_add(chunk.token_estimate);
        if next > input.budget_profile.max_tokens {
            break;
        }
        token_estimate = next;
        selected_chunk_refs.push(chunk);
    }

    ContextPack {
        pack_id,
        session_id: input.session_id,
        artifact_id: input.artifact_id,
        created_at_utc: now,
        budget_profile: input.budget_profile,
        selected_claims,
        selected_chunk_refs,
        total_token_estimate: token_estimate,
    }
}

pub fn persist_context_pack(pack: &ContextPack) -> std::result::Result<(), String> {
    let session_dir = packs_root().join(&pack.session_id);
    std::fs::create_dir_all(&session_dir).map_err(|err| err.to_string())?;
    let output_path = session_dir.join(format!("{}.json", pack.pack_id));
    let raw = serde_json::to_vec_pretty(pack).map_err(|err| err.to_string())?;
    std::fs::write(&output_path, raw).map_err(|err| err.to_string())?;
    append_index_record(pack, &output_path)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPackIndexRecord {
    pub pack_id: String,
    pub session_id: String,
    pub artifact_id: String,
    pub created_at_utc: DateTime<Utc>,
    pub total_token_estimate: usize,
    pub output_path: String,
}

pub fn list_context_packs(session_id: &str, limit: usize) -> Vec<ContextPackIndexRecord> {
    let mut records: Vec<ContextPackIndexRecord> = read_index_records()
        .into_iter()
        .filter(|record| record.session_id == session_id)
        .collect();
    records.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
    records.into_iter().take(limit.max(1)).collect()
}

pub fn find_context_pack(session_id: &str, query: Option<&str>) -> Option<ContextPack> {
    let mut records: Vec<ContextPackIndexRecord> = read_index_records()
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
            record.pack_id.starts_with(query) || record.artifact_id.starts_with(query)
        })
    }?;

    std::fs::read_to_string(&record.output_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ContextPack>(&raw).ok())
}

fn append_index_record(pack: &ContextPack, output_path: &Path) -> std::result::Result<(), String> {
    let index_path = packs_root().join("index.jsonl");
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let record = ContextPackIndexRecord {
        pack_id: pack.pack_id.clone(),
        session_id: pack.session_id.clone(),
        artifact_id: pack.artifact_id.clone(),
        created_at_utc: pack.created_at_utc,
        total_token_estimate: pack.total_token_estimate,
        output_path: output_path.to_string_lossy().to_string(),
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(index_path)
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(&record).map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())?;
    Ok(())
}

fn read_index_records() -> Vec<ContextPackIndexRecord> {
    let index_path = packs_root().join("index.jsonl");
    let Ok(file) = std::fs::File::open(index_path) else {
        return Vec::new();
    };

    std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ContextPackIndexRecord>(&line).ok())
        .collect()
}

fn packs_root() -> PathBuf {
    data_local_medousa_dir().join("context_packs")
}

fn data_local_medousa_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("medousa")
}

fn short_session(session_id: &str) -> String {
    session_id.chars().take(8).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::{BuildContextPackInput, ContextPackBudgetProfile, build_context_pack};
    use serde_json::json;

    #[test]
    fn pipeline_builds_context_pack_from_chunks_and_claims() {
        let payload = json!({
            "results": [
                {"title": "A", "score": 0.91},
                {"title": "B", "score": 0.88}
            ],
            "meta": {"source": "unit-test"}
        });

        let chunk_refs =
            crate::artifact_chunking::chunk_json_payload("artifact-1", &payload, 320, 40);
        let claims = crate::artifact_extraction::extract_claims_from_chunks(
            "artifact-1",
            &payload,
            &chunk_refs,
        );

        let pack = build_context_pack(BuildContextPackInput {
            session_id: "session-1".to_string(),
            artifact_id: "artifact-1".to_string(),
            claims,
            chunk_refs,
            budget_profile: ContextPackBudgetProfile {
                max_tokens: 5000,
                max_claims: 8,
                max_chunks: 20,
            },
        });

        assert!(!pack.selected_claims.is_empty());
        assert!(!pack.selected_chunk_refs.is_empty());
        assert!(pack.total_token_estimate > 0);
        assert!(pack.total_token_estimate <= pack.budget_profile.max_tokens);
    }

    #[test]
    fn pipeline_respects_budget_overflow_limits() {
        let payload = json!({
            "results": (0..120).map(|idx| json!({"i": idx, "text": format!("item-{idx}")})).collect::<Vec<_>>()
        });

        let chunk_refs =
            crate::artifact_chunking::chunk_json_payload("artifact-2", &payload, 280, 30);
        let claims = crate::artifact_extraction::extract_claims_from_chunks(
            "artifact-2",
            &payload,
            &chunk_refs,
        );

        let pack = build_context_pack(BuildContextPackInput {
            session_id: "session-2".to_string(),
            artifact_id: "artifact-2".to_string(),
            claims,
            chunk_refs,
            budget_profile: ContextPackBudgetProfile {
                max_tokens: 180,
                max_claims: 2,
                max_chunks: 3,
            },
        });

        assert!(pack.selected_claims.len() <= 2);
        assert!(pack.selected_chunk_refs.len() <= 3);
        assert!(pack.total_token_estimate <= 180);
    }
}
