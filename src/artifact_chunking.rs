use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttpChunkNodeRef {
    pub node_id: String,
    pub chunk_id: String,
    pub sequence: usize,
    pub token_estimate: usize,
    pub hash64: String,
}

pub fn chunk_json_payload(
    artifact_id: &str,
    payload: &Value,
    target_chars: usize,
    overlap_chars: usize,
) -> Vec<SttpChunkNodeRef> {
    let normalized = serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string());
    if normalized.trim().is_empty() {
        return Vec::new();
    }

    let target = target_chars.max(256);
    let overlap = overlap_chars.min(target / 2);
    let chars: Vec<char> = normalized.chars().collect();
    let mut refs = Vec::new();
    let mut start = 0usize;
    let mut seq = 0usize;

    while start < chars.len() {
        let end = (start + target).min(chars.len());
        let chunk_text: String = chars[start..end].iter().collect();
        let chunk_hash = hash_text(&chunk_text);
        let chunk_id = format!("{}:chunk:{}", artifact_id, seq);
        refs.push(SttpChunkNodeRef {
            node_id: format!("sttp:{}", chunk_id),
            chunk_id,
            sequence: seq,
            token_estimate: (chunk_text.chars().count() / 4).max(1),
            hash64: chunk_hash,
        });

        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(overlap);
        seq = seq.saturating_add(1);
    }

    refs
}

fn hash_text(text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
