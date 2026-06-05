//! Medousa Locus memory adapters (ingest profile + MCP-aligned helpers).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Duration;
use locus_core_rs::{
    ParseProfile, StoreContextService, StoreRetryPolicy, SttpNodeParser, TreeSitterValidator,
};
use serde_json::{Value, json};
use stasis::domain::errors::Result as StasisResult;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_models::{
    MemoryAvecState, MemoryNode, MemoryStoreRequest, MemoryStoreResponse,
};

pub const CANONICAL_STTP_SCHEMA_EXAMPLE: &str = r#"Canonical STTP node example (call cognition_memory_schema for this text):

⊕⟨ ⏣0{ trigger: manual, response_format: temporal_node, origin_session: "session-abc", compression_depth: 1, parent_node: null, prime: { attractor_config: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85 }, context_summary: "parser hardening session", relevant_tier: raw, retrieval_budget: 8 } } ⟩
⦿⟨ ⏣0{ timestamp: "2026-04-25T00:00:00Z", tier: raw, session_id: "session-abc", schema_version: "sttp-1.0", user_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 }, model_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 } } ⟩
◈⟨ ⏣0{ focus(.99): "grammar update", decision(.96): { parser_mode(.95): "strict_and_tolerant" } } ⟩
⍉⟨ ⏣0{ rho: 0.95, kappa: 0.94, psi: 2.93, compression_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 } } ⟩"#;

/// Default interactive ingest: tolerant (matches recommended MCP dev profile).
pub fn default_interactive_ingest_profile() -> ParseProfile {
    ParseProfile::Tolerant
}

/// Resolve ingest profile from env (MCP-compatible names).
pub fn resolve_locus_ingest_profile() -> ParseProfile {
    let raw = std::env::var("MEDOUSA_MEMORY_INGEST_PROFILE")
        .ok()
        .or_else(|| std::env::var("LOCUS_MCP_PARSE_PROFILE").ok())
        .unwrap_or_else(|| "tolerant".to_string());
    parse_ingest_profile(raw.trim()).unwrap_or_else(default_interactive_ingest_profile)
}

pub fn parse_ingest_profile(value: &str) -> Option<ParseProfile> {
    match value.trim().to_ascii_lowercase().as_str() {
        "strict_typed_ir" | "strict-typed-ir" | "stricttypedir" | "typed_ir" | "typed-ir" => {
            Some(ParseProfile::StrictTypedIr)
        }
        "strict" => Some(ParseProfile::Strict),
        "tolerant" | "default" => Some(ParseProfile::Tolerant),
        _ => None,
    }
}

pub fn ingest_profile_name(profile: ParseProfile) -> &'static str {
    match profile {
        ParseProfile::StrictTypedIr => "strict_typed_ir",
        ParseProfile::Strict => "strict",
        ParseProfile::Tolerant => "tolerant",
    }
}

pub fn interactive_store_retry_policy() -> StoreRetryPolicy {
    StoreRetryPolicy {
        max_failures_before_cooldown: 5,
        cooldown: Duration::seconds(60),
    }
}

#[derive(Clone)]
pub struct MedousaLocusContextWriter {
    service: Arc<StoreContextService>,
    profile_name: &'static str,
}

impl MedousaLocusContextWriter {
    pub fn new(store: Arc<dyn locus_core_rs::NodeStore>, profile: ParseProfile) -> Self {
        let validator = Arc::new(TreeSitterValidator::new());
        let parser = SttpNodeParser::with_profile(profile);
        let profile_name = ingest_profile_name(profile);
        let service = Arc::new(StoreContextService::new_with_policy(
            store,
            validator,
            interactive_store_retry_policy(),
            parser,
        ));
        Self {
            service,
            profile_name,
        }
    }

    pub fn profile_name(&self) -> &'static str {
        self.profile_name
    }
}

#[async_trait]
impl MemoryContextWriter for MedousaLocusContextWriter {
    async fn store_context(&self, request: &MemoryStoreRequest) -> StasisResult<MemoryStoreResponse> {
        let result = self
            .service
            .store_async(&request.raw_node, &request.session_id)
            .await;
        Ok(MemoryStoreResponse {
            node_id: result.node_id,
            psi: result.psi,
            valid: result.valid,
            validation_error: result.validation_error,
        })
    }
}

pub fn infer_store_error_code(message: &str) -> &'static str {
    let normalized = message.trim().to_ascii_lowercase();
    if normalized.starts_with("parsefailure") {
        "StrictTypedIrParseFailure"
    } else if normalized.starts_with("ratelimited") {
        "StoreRateLimited"
    } else if normalized.starts_with("storefailure") {
        "StoreFailure"
    } else if normalized.contains("strict profile") {
        "StrictTypedIrPolicyViolation"
    } else {
        "StoreContextFailure"
    }
}

pub fn schema_first_guidance(summary: &str, profile_name: &str) -> Value {
    json!({
        "summary": summary,
        "recommended_first_tool": "cognition_memory_schema",
        "recommended_next_steps": [
            "call cognition_memory_schema",
            "verify payload layers provenance->envelope->content->metrics",
            "ensure STTP symbols and AVEC blocks are present before retry"
        ],
        "ingest_profile_policy": profile_name,
    })
}

pub fn store_failure_payload(
    node_id: String,
    psi: f32,
    valid: bool,
    message: String,
    profile_name: &str,
) -> Value {
    let code = infer_store_error_code(&message);
    json!({
        "node_id": node_id,
        "psi": psi,
        "valid": valid,
        "stored": valid,
        "validation_error": message,
        "profile_policy": profile_name,
        "error": {
            "code": code,
            "message": message,
            "model_guidance": schema_first_guidance(
                "Inspect schema and ingest policy before retrying cognition_memory_store.",
                profile_name
            )
        }
    })
}

pub fn avec_to_json(avec: locus_core_rs::AvecState) -> Value {
    json!({
        "stability": avec.stability,
        "friction": avec.friction,
        "logic": avec.logic,
        "autonomy": avec.autonomy,
        "psi": avec.psi(),
    })
}

pub fn sttp_node_to_json(node: &locus_core_rs::SttpNode) -> Value {
    json!({
        "raw": node.raw,
        "session_id": node.session_id,
        "tier": node.tier,
        "timestamp": node.timestamp.to_rfc3339(),
        "context_summary": node.context_summary,
        "psi": node.psi,
        "rho": node.rho,
        "kappa": node.kappa,
        "sync_key": node.sync_key,
        "user_avec": avec_to_json(node.user_avec),
        "model_avec": avec_to_json(node.model_avec),
    })
}

/// JSON for nodes returned by Stasis `MemoryRecallResponse` / `MemoryFindResponse` (0.2.3+).
pub fn memory_node_to_json(node: &MemoryNode) -> Value {
    json!({
        "raw": node.raw,
        "session_id": node.session_id,
        "tier": node.tier,
        "timestamp": node.timestamp.to_rfc3339(),
        "context_summary": node.context_summary,
        "compression_depth": node.compression_depth,
        "parent_node_id": node.parent_node_id,
        "sync_key": node.sync_key,
        "psi": node.psi,
        "rho": node.rho,
        "kappa": node.kappa,
        "user_avec": memory_avec_to_json(node.user_avec),
        "model_avec": memory_avec_to_json(node.model_avec),
        "compression_avec": node.compression_avec.map(memory_avec_to_json),
        "updated_at": node.updated_at.to_rfc3339(),
    })
}

/// Inject `vibe_signature(.97)` into the STTP content layer when absent.
pub fn enrich_sttp_node_with_vibe_signature(raw_node: &str, vibe_signature: &str) -> String {
    let vibe = vibe_signature.trim();
    if vibe.is_empty() || raw_node.contains("vibe_signature") {
        return raw_node.to_string();
    }

    let escaped: String = vibe
        .chars()
        .map(|ch| if ch == '"' { '\'' } else { ch })
        .collect();

    if let Some(marker) = raw_node.find("◈⟨") {
        if let Some(open) = raw_node[marker..].find('{') {
            let insert_at = marker + open + 1;
            let mut out = String::with_capacity(raw_node.len() + escaped.len() + 32);
            out.push_str(&raw_node[..insert_at]);
            out.push_str(&format!(" vibe_signature(.97): \"{escaped}\", "));
            out.push_str(&raw_node[insert_at..]);
            return out;
        }
    }

    raw_node.to_string()
}

pub fn memory_avec_to_json(avec: MemoryAvecState) -> Value {
    json!({
        "stability": avec.stability,
        "friction": avec.friction,
        "logic": avec.logic,
        "autonomy": avec.autonomy,
        "psi": avec.stability + avec.friction + avec.logic + avec.autonomy,
    })
}

pub fn validate_limit(limit: usize, field: &str) -> Result<usize, String> {
    if (1..=200).contains(&limit) {
        Ok(limit)
    } else {
        Err(format!("{field} must be between 1 and 200"))
    }
}

/// `session_id` for `cognition_memory_recall`: JSON `null` or omitted → global (no default session).
/// A non-empty string scopes to that session.
pub fn recall_session_id_for_context(input: &serde_json::Value) -> serde_json::Value {
    match input.get("session_id") {
        Some(value) if value.is_null() => serde_json::Value::Null,
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::Value::String(s.to_string()))
            .unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    }
}

pub fn normalize_context_keywords(keywords: Option<&[String]>) -> Vec<String> {
    keywords
        .unwrap_or(&[])
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn normalize_tiers(tiers: &[String]) -> Vec<String> {
    tiers
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn filter_nodes_by_context_keywords(
    nodes: &[locus_core_rs::SttpNode],
    keywords: &[String],
    limit: usize,
) -> Vec<locus_core_rs::SttpNode> {
    let mut scored = nodes
        .iter()
        .filter_map(|node| {
            let summary = node
                .context_summary
                .as_deref()
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_default();
            let session_id = node.session_id.to_ascii_lowercase();
            let score = keywords
                .iter()
                .filter(|keyword| {
                    let needle = keyword.as_str();
                    summary.contains(needle) || session_id.contains(needle)
                })
                .count();
            if score == 0 {
                None
            } else {
                Some((score, node.timestamp, node.clone()))
            }
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, node)| node)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mcp_compatible_profile_aliases() {
        assert_eq!(
            parse_ingest_profile("strict-typed-ir"),
            Some(ParseProfile::StrictTypedIr)
        );
        assert_eq!(parse_ingest_profile("tolerant"), Some(ParseProfile::Tolerant));
    }

    #[test]
    fn recall_session_id_null_or_omitted_is_global() {
        assert!(recall_session_id_for_context(&serde_json::json!({})).is_null());
        assert!(recall_session_id_for_context(&serde_json::json!({ "session_id": null })).is_null());
    }

    #[test]
    fn recall_session_id_string_is_preserved() {
        assert_eq!(
            recall_session_id_for_context(&serde_json::json!({ "session_id": "abc-123" })),
            serde_json::json!("abc-123")
        );
    }
}
