use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::artifact_chunking::SttpChunkNodeRef;
use crate::artifact_extraction::EvidenceClaim;
use crate::store_root::StorePath;

const CONTEXT_PACK_OBJECT_DOMAIN: &[u8] = b"context-pack";

static CONTEXT_PACK_STORE: Lazy<crate::session_storage::SessionDirectoryStore> = Lazy::new(|| {
    crate::session_storage::SessionDirectoryStore::new(
        crate::paths::medousa_data_dir().join("context_packs"),
    )
});

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
    let (session_id, _mutation) =
        crate::session_deletion::acquire_mutation_for_str(&pack.session_id)?;
    let output_path = context_pack_path(&pack.pack_id);
    let raw = serde_json::to_vec_pretty(pack).map_err(|err| err.to_string())?;
    CONTEXT_PACK_STORE
        .atomic_write(&session_id, &output_path, &raw)
        .map_err(|err| err.to_string())?;
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
    records.sort_by_key(|b| std::cmp::Reverse(b.created_at_utc));
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
    records.sort_by_key(|b| std::cmp::Reverse(b.created_at_utc));
    let query = query.map(str::trim).unwrap_or("");
    let record = if query.is_empty() || query.eq_ignore_ascii_case("last") {
        records.into_iter().next()
    } else {
        records.into_iter().find(|record| {
            record.pack_id.starts_with(query) || record.artifact_id.starts_with(query)
        })
    }?;

    let session_id = crate::session_storage::SessionId::parse(&record.session_id).ok()?;
    CONTEXT_PACK_STORE
        .read(&session_id, &context_pack_path(&record.pack_id))
        .ok()
        .and_then(|raw| serde_json::from_slice::<ContextPack>(&raw).ok())
}

pub fn delete_context_packs_for_session(session_id: &str) -> Result<(), String> {
    let session_id =
        crate::session_storage::SessionId::parse(session_id).map_err(|error| error.to_string())?;
    let remaining = read_index_records()
        .into_iter()
        .filter(|record| record.session_id != session_id.as_str())
        .collect::<Vec<_>>();
    overwrite_index_records(&remaining)?;
    CONTEXT_PACK_STORE
        .remove_session(&session_id)
        .map_err(|error| error.to_string())?;
    if read_index_records()
        .iter()
        .any(|record| record.session_id == session_id.as_str())
        || CONTEXT_PACK_STORE
            .contains_session(&session_id)
            .map_err(|error| error.to_string())?
    {
        return Err("context-pack session data remains after deletion".to_string());
    }
    Ok(())
}

fn append_index_record(
    pack: &ContextPack,
    output_path: &StorePath,
) -> std::result::Result<(), String> {
    let record = ContextPackIndexRecord {
        pack_id: pack.pack_id.clone(),
        session_id: pack.session_id.clone(),
        artifact_id: pack.artifact_id.clone(),
        created_at_utc: pack.created_at_utc,
        total_token_estimate: pack.total_token_estimate,
        output_path: output_path.file_name().to_string(),
    };

    let mut line = serde_json::to_vec(&record).map_err(|err| err.to_string())?;
    line.push(b'\n');
    CONTEXT_PACK_STORE
        .append_root(&index_path(), &line)
        .map_err(|err| err.to_string())
}

fn read_index_records() -> Vec<ContextPackIndexRecord> {
    let Ok(bytes) = CONTEXT_PACK_STORE.read_root(&index_path()) else {
        return Vec::new();
    };
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .filter_map(|line| serde_json::from_slice::<ContextPackIndexRecord>(line).ok())
        .collect()
}

fn overwrite_index_records(records: &[ContextPackIndexRecord]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for record in records {
        serde_json::to_writer(&mut bytes, record).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
    }
    CONTEXT_PACK_STORE
        .atomic_write_root(&index_path(), &bytes)
        .map_err(|error| error.to_string())
}

fn context_pack_path(pack_id: &str) -> StorePath {
    crate::session_storage::session_object_path(CONTEXT_PACK_OBJECT_DOMAIN, pack_id, "json")
}

fn index_path() -> StorePath {
    StorePath::parse("index.jsonl").expect("static context-pack index path must be valid")
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
