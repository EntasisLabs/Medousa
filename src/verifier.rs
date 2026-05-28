use serde::{Deserialize, Serialize};

use crate::context_pack::ContextPack;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationPolicy {
    pub min_citation_coverage: f32,
    pub min_avg_support_strength: f32,
    pub min_supported_claim_ratio: f32,
    pub min_claim_support_strength: f32,
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self {
            min_citation_coverage: 0.60,
            min_avg_support_strength: 0.70,
            min_supported_claim_ratio: 0.60,
            min_claim_support_strength: 0.65,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub pack_id: String,
    pub artifact_id: String,
    pub total_claims: usize,
    pub supported_claims: usize,
    pub unsupported_claim_ids: Vec<String>,
    pub citation_coverage: f32,
    pub avg_support_strength: f32,
    pub supported_claim_ratio: f32,
    pub confidence_score: f32,
    pub is_verified: bool,
}

pub fn verify_context_pack(pack: &ContextPack, policy: &VerificationPolicy) -> VerificationReport {
    let total_claims = pack.selected_claims.len();
    let claims_with_refs = pack
        .selected_claims
        .iter()
        .filter(|claim| !claim.supporting_chunk_node_ids.is_empty())
        .count();
    let citation_coverage = if total_claims == 0 {
        0.0
    } else {
        claims_with_refs as f32 / total_claims as f32
    };

    let avg_support_strength = if total_claims == 0 {
        0.0
    } else {
        pack.selected_claims
            .iter()
            .map(|claim| claim.support_strength)
            .sum::<f32>()
            / total_claims as f32
    };

    let mut supported_claims = 0usize;
    let mut unsupported_claim_ids = Vec::new();
    for claim in &pack.selected_claims {
        let has_refs = !claim.supporting_chunk_node_ids.is_empty();
        let strong_enough = claim.support_strength >= policy.min_claim_support_strength;
        if has_refs && strong_enough {
            supported_claims = supported_claims.saturating_add(1);
        } else {
            unsupported_claim_ids.push(claim.claim_id.clone());
        }
    }

    let supported_claim_ratio = if total_claims == 0 {
        0.0
    } else {
        supported_claims as f32 / total_claims as f32
    };

    let confidence_score = ((citation_coverage * 0.4)
        + (avg_support_strength.clamp(0.0, 1.0) * 0.4)
        + (supported_claim_ratio * 0.2))
        .clamp(0.0, 1.0);

    let is_verified = !pack.selected_chunk_refs.is_empty()
        && citation_coverage >= policy.min_citation_coverage
        && avg_support_strength >= policy.min_avg_support_strength
        && supported_claim_ratio >= policy.min_supported_claim_ratio;

    VerificationReport {
        pack_id: pack.pack_id.clone(),
        artifact_id: pack.artifact_id.clone(),
        total_claims,
        supported_claims,
        unsupported_claim_ids,
        citation_coverage,
        avg_support_strength,
        supported_claim_ratio,
        confidence_score,
        is_verified,
    }
}

#[cfg(test)]
mod tests {
    use super::{VerificationPolicy, verify_context_pack};
    use crate::artifact_chunking::SttpChunkNodeRef;
    use crate::artifact_extraction::EvidenceClaim;
    use crate::context_pack::{ContextPack, ContextPackBudgetProfile};
    use chrono::Utc;

    fn sample_pack() -> ContextPack {
        ContextPack {
            pack_id: "pack:test:verify".to_string(),
            session_id: "session-1".to_string(),
            artifact_id: "artifact-1".to_string(),
            created_at_utc: Utc::now(),
            budget_profile: ContextPackBudgetProfile {
                max_tokens: 3200,
                max_claims: 6,
                max_chunks: 12,
            },
            selected_claims: vec![
                EvidenceClaim {
                    claim_id: "claim-1".to_string(),
                    statement: "verified claim".to_string(),
                    supporting_chunk_node_ids: vec!["sttp:artifact-1:chunk:0".to_string()],
                    support_strength: 0.88,
                },
                EvidenceClaim {
                    claim_id: "claim-2".to_string(),
                    statement: "also verified claim".to_string(),
                    supporting_chunk_node_ids: vec!["sttp:artifact-1:chunk:1".to_string()],
                    support_strength: 0.80,
                },
            ],
            selected_chunk_refs: vec![
                SttpChunkNodeRef {
                    node_id: "sttp:artifact-1:chunk:0".to_string(),
                    chunk_id: "artifact-1:chunk:0".to_string(),
                    sequence: 0,
                    token_estimate: 120,
                    hash64: "abc123".to_string(),
                },
                SttpChunkNodeRef {
                    node_id: "sttp:artifact-1:chunk:1".to_string(),
                    chunk_id: "artifact-1:chunk:1".to_string(),
                    sequence: 1,
                    token_estimate: 98,
                    hash64: "def456".to_string(),
                },
            ],
            total_token_estimate: 218,
        }
    }

    #[test]
    fn verifies_high_quality_pack() {
        let pack = sample_pack();
        let report = verify_context_pack(&pack, &VerificationPolicy::default());
        assert!(report.is_verified);
        assert_eq!(report.supported_claims, 2);
        assert!(report.unsupported_claim_ids.is_empty());
        assert!(report.confidence_score >= 0.75);
    }

    #[test]
    fn rejects_low_quality_pack() {
        let mut pack = sample_pack();
        pack.selected_claims[1].supporting_chunk_node_ids.clear();
        pack.selected_claims[1].support_strength = 0.2;

        let report = verify_context_pack(&pack, &VerificationPolicy::default());
        assert!(!report.is_verified);
        assert_eq!(report.supported_claims, 1);
        assert_eq!(report.unsupported_claim_ids.len(), 1);
    }
}
