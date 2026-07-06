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
use tokio::sync::RwLock;

use crate::turn_continuation::TurnContinuationScope;

pub const LOCUS_TENANT_SCOPE_PREFIX: &str = "tenant:";
pub const LOCUS_TENANT_SCOPE_SEPARATOR: &str = "::session:";
pub const LOCUS_DEFAULT_TENANT: &str = "default";
/// Chat session id for identity→Locus STTP bridge nodes (scoped per active profile).
pub const IDENTITY_BRIDGE_CHAT_SESSION: &str = "medousa-identity";

/// Build a Locus session key for a profile + chat session.
///
/// Default profile keeps legacy plain `chat_session_id` (tenant `default` in locus-core).
/// Other profiles use `tenant:{slug}::session:{chatSessionId}`.
pub fn scoped_locus_session(profile_slug: &str, chat_session_id: &str) -> String {
    let chat = chat_session_id.trim();
    let slug = profile_slug.trim();
    if slug.is_empty() || slug == LOCUS_DEFAULT_TENANT {
        chat.to_string()
    } else {
        format!("{LOCUS_TENANT_SCOPE_PREFIX}{slug}{LOCUS_TENANT_SCOPE_SEPARATOR}{chat}")
    }
}

/// Mirror of `locus-core-rs` tenant derivation for tests and diagnostics.
pub fn derive_locus_tenant_id(session_id: &str) -> String {
    session_id
        .strip_prefix(LOCUS_TENANT_SCOPE_PREFIX)
        .and_then(|remainder| remainder.split_once(LOCUS_TENANT_SCOPE_SEPARATOR))
        .map(|(tenant, _)| tenant)
        .filter(|tenant| !tenant.trim().is_empty())
        .unwrap_or(LOCUS_DEFAULT_TENANT)
        .to_string()
}

pub fn parse_scoped_locus_session(session_id: &str) -> Option<(String, String)> {
    session_id
        .strip_prefix(LOCUS_TENANT_SCOPE_PREFIX)
        .and_then(|remainder| remainder.split_once(LOCUS_TENANT_SCOPE_SEPARATOR))
        .map(|(tenant, chat)| (tenant.to_string(), chat.to_string()))
}

/// Scope the current chat session under the active workshop profile.
pub fn resolve_workshop_locus_session(chat_session_id: &str) -> String {
    let profile_id = crate::user_profiles::resolve_workshop_identity_user_id();
    let slug = crate::user_profiles::profile_slug_from_id(&profile_id).unwrap_or(LOCUS_DEFAULT_TENANT);
    scoped_locus_session(slug, chat_session_id)
}

pub fn identity_bridge_locus_session() -> String {
    resolve_workshop_locus_session(IDENTITY_BRIDGE_CHAT_SESSION)
}

/// Resolve Locus session for memory tools: explicit arg → turn scope chat id → fallback.
pub async fn resolve_memory_tool_session_id(
    input: &Value,
    turn_scope: &RwLock<Option<TurnContinuationScope>>,
    bootstrap_fallback: &str,
    workshop_dynamic: bool,
) -> String {
    if let Some(explicit) = crate::runtime_session::explicit_chat_session_id_from_input(input) {
        return explicit;
    }

    let scoped_chat_session_id = crate::runtime_session::resolve_active_chat_session_id_async(
        turn_scope,
        bootstrap_fallback,
    )
    .await;

    if workshop_dynamic {
        resolve_workshop_locus_session(&scoped_chat_session_id)
    } else {
        scoped_chat_session_id
    }
}

pub const CANONICAL_STTP_SCHEMA_EXAMPLE: &str = r#"Canonical STTP node example (call cognition_memory_schema for this text):

⊕⟨ ⏣0{ trigger: manual, response_format: temporal_node, origin_session: "session-abc", compression_depth: 1, parent_node: null, semantic_links: [{ rel: "related_to", target: "concept:memory-schema", confidence: 0.88 }], prime: { attractor_config: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85 }, context_summary: "parser hardening session", relevant_tier: raw, retrieval_budget: 8, semantic_tags: ["medousa", "session", "grammar-update"] } } ⟩
⦿⟨ ⏣0{ timestamp: "2026-04-25T00:00:00Z", tier: raw, session_id: "session-abc", schema_version: "sttp-1.0", user_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 }, model_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 } } ⟩
◈⟨ ⏣0{ focus(.99): "grammar update", decision(.96): { parser_mode(.95): "strict_and_tolerant" } } ⟩
⍉⟨ ⏣0{ rho: 0.95, kappa: 0.94, psi: 2.93, compression_avec: { stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 } } ⟩"#;

pub fn semantic_index_schema_guidance() -> Value {
    json!({
        "semantic_tags": {
            "sttp_location": "provenance.prime.semantic_tags",
            "format": "array<string> — lowercase strings, deduped at index time",
            "omit_when_absent": "omit the field or use an empty array; do not use null (locus-core 0.4.2+ treats null as absent)",
            "medousa_store_tool": "cognition_memory_store.semantic_tags merges workshop defaults + extras into prime when the node string omits semantic_tags",
            "recall": "cognition_memory_context / cognition_memory_list / cognition_memory_recall with semantic_tags (match-all) or cognition_memory_tags for vocabulary browse"
        },
        "semantic_links": {
            "sttp_location": "provenance.semantic_links",
            "format": "[{ rel: string, target: string, confidence: float }]",
            "optional": true,
            "omit_when_absent": "omit the field when unused; null is treated as absent"
        }
    })
}

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
    } else if normalized.contains("surreal query rejected")
        || normalized.contains("surreal query transport failed")
        || normalized.contains("surreal query result decode failed")
    {
        "SurrealPersistenceFailure"
    } else {
        "StoreContextFailure"
    }
}

fn is_persistence_failure(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    normalized.starts_with("storefailure:")
        && (normalized.contains("surreal query rejected")
            || normalized.contains("surreal query transport failed")
            || normalized.contains("surreal query result decode failed")
            || normalized.contains("schemafull")
            || normalized.contains("found none for field")
            || normalized.contains("found field"))
}

pub fn persistence_failure_guidance(summary: &str) -> Value {
    json!({
        "summary": summary,
        "recommended_first_tool": "cognition_memory_schema",
        "recommended_next_steps": [
            "read the SurrealDB error in validation_error — it is the root cause, not a decode wrapper",
            "if the error mentions a missing or mismatched field, restart with MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON=1 so temporal_node schema is upgraded",
            "if schema is current, fix the STTP payload and retry cognition_memory_store",
            "check daemon logs for [sttp_ingest_trace] stage=store reason=store_failure for retry/cooldown state"
        ],
        "operator_hints": [
            "MEDOUSA_FORCE_LOCUS_INIT_ON_DAEMON=1 runs Locus initialize_async (adds new temporal_node fields such as semantic_links)",
            "MEDOUSA_SKIP_LOCUS_INIT_ON_DAEMON=1 skips init on existing graphs — can leave schema stale"
        ]
    })
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

pub fn store_failure_guidance(message: &str, profile_name: &str) -> Value {
    if is_persistence_failure(message) {
        persistence_failure_guidance(
            "Locus parsed the node but SurrealDB rejected persistence. Fix the DB/schema issue before retrying cognition_memory_store.",
        )
    } else {
        schema_first_guidance(
            "Inspect schema and ingest policy before retrying cognition_memory_store.",
            profile_name,
        )
    }
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
            "model_guidance": store_failure_guidance(&message, profile_name)
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
        "semantic_tags": node.semantic_tags,
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
        "semantic_tags": node.semantic_tags,
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

    #[test]
    fn scoped_session_encodes_tenant_slug() {
        assert_eq!(
            scoped_locus_session("work", "abc-123"),
            "tenant:work::session:abc-123"
        );
        assert_eq!(derive_locus_tenant_id("tenant:work::session:abc-123"), "work");
    }

    #[test]
    fn default_profile_keeps_legacy_plain_session_key() {
        assert_eq!(scoped_locus_session("default", "abc-123"), "abc-123");
        assert_eq!(derive_locus_tenant_id("abc-123"), "default");
    }

    #[test]
    fn parse_scoped_session_round_trips() {
        let scoped = scoped_locus_session("home", "sess-1");
        assert_eq!(
            parse_scoped_locus_session(&scoped),
            Some(("home".to_string(), "sess-1".to_string()))
        );
    }

    #[test]
    fn persistence_guidance_used_for_surreal_store_failures() {
        let message = "StoreFailure: surreal query rejected: `CREATE temporal_node`";
        let guidance = store_failure_guidance(message, "tolerant");
        assert_eq!(
            guidance
                .get("recommended_next_steps")
                .and_then(|v| v.as_array())
                .and_then(|steps| steps.first())
                .and_then(|v| v.as_str()),
            Some("read the SurrealDB error in validation_error — it is the root cause, not a decode wrapper")
        );
    }

    #[test]
    fn schema_guidance_used_for_parse_failures() {
        let message = "ParseFailure: missing content layer";
        let guidance = store_failure_guidance(message, "tolerant");
        assert_eq!(
            guidance.get("recommended_first_tool").and_then(|v| v.as_str()),
            Some("cognition_memory_schema")
        );
    }

    #[test]
    fn canonical_schema_example_includes_semantic_tags() {
        assert!(CANONICAL_STTP_SCHEMA_EXAMPLE.contains("semantic_tags:"));
        assert!(CANONICAL_STTP_SCHEMA_EXAMPLE.contains("semantic_links:"));
    }

    #[test]
    fn semantic_index_guidance_documents_store_and_recall() {
        let guidance = semantic_index_schema_guidance();
        assert!(guidance.get("semantic_tags").is_some());
        assert!(guidance["semantic_tags"]
            .get("medousa_store_tool")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("cognition_memory_store"));
    }
}
