use std::collections::HashMap;

use locus_core_rs::NodeQuery;
use stasis::prelude::MemoryRecallRequest;

use crate::agent_runtime::ambient_context::ChannelAmbientPolicy;
use crate::engine_context::{
    ContextCompilerInput, EngineExecutionLane, RecallReadiness, compile_context_prompt,
    default_policy_profile_for_lane,
};
use crate::cognitive_identity::{
    DigestCompileOptions, cognitive_identity_diagnostics_with_stats,
    compile_relational_memory_digest_with_options, load_cognitive_identity_snapshot,
    DEFAULT_RELATIONAL_DIGEST_BUDGET,
};
use crate::identity_manuscript::{
    ManuscriptContext, digest_options_for_manuscript, format_manuscript_prompt_block,
};
use crate::identity_memory::{
    policy_identity_context_request, resolve_identity_channel_id, resolve_identity_persona_id,
    resolve_identity_user_id,
};
use crate::stage_routing::StageRoute;
use crate::tools::TuiRuntime;
use crate::tui::settings::{RuntimeSettings, parse_f32_with_bounds};

pub const MAX_REQUEST_PROMPT_CHARS: usize = 48_000;

const CHEAP_RECALL_LIMIT: usize = 4;
const CHEAP_RECALL_QUERY_MAX_CHARS: usize = 280;
const CHEAP_RECALL_MAX_KEYS: usize = 6;
const CHEAP_RECALL_SNIPPET_MAX_COUNT: usize = 3;
const CHEAP_RECALL_SNIPPET_MAX_CHARS: usize = 220;
const CHEAP_RECALL_NODE_SCAN_LIMIT: usize = 240;

#[derive(Debug, Clone)]
pub struct ContextPackQuality {
    pub citation_coverage: f32,
    pub avg_support_strength: f32,
    pub supported_claim_ratio: f32,
    pub confidence_score: f32,
    pub is_usable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CheapRecallProbe {
    pub attempted: bool,
    pub retrieved: usize,
    pub retrieval_path: Option<String>,
    pub fallback_triggered: bool,
    pub fallback_reason: Option<String>,
    pub node_sync_keys: Vec<String>,
    pub snippets: Vec<RecallSnippet>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct IdentityContextProbe {
    pub attempted: bool,
    pub summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecallSnippet {
    pub sync_key: String,
    pub context_summary: String,
    pub excerpt: String,
}

pub fn resolve_prompt_with_context_pack(
    session_id: &str,
    prompt: &str,
    pack_query: Option<&str>,
    policy: &crate::verifier::VerificationPolicy,
) -> (String, Option<String>, Option<bool>) {
    let selector = pack_query.unwrap_or("last");
    let Some(pack) = crate::context_pack::find_context_pack(session_id, Some(selector)) else {
        return (prompt.to_string(), None, None);
    };

    let (prompt_with_pack, quality, report) = build_prompt_with_context_pack(prompt, &pack, policy);
    let verification_id = crate::verification_store::persist_verification(
        session_id,
        selector,
        "prompt_injection",
        policy,
        &report,
    )
    .ok()
    .map(|record| record.verification_id);

    let verification_suffix = verification_id
        .map(|id| format!(" verification={id}"))
        .unwrap_or_default();
    let note = if quality.is_usable {
        format!(
            "◈ context pack verified {} selector={} artifact={} claims={} chunks={} coverage={:.2} avg_support={:.2} support_ratio={:.2} confidence={:.2}{}",
            pack.pack_id,
            selector,
            pack.artifact_id,
            pack.selected_claims.len(),
            pack.selected_chunk_refs.len(),
            quality.citation_coverage,
            quality.avg_support_strength,
            quality.supported_claim_ratio,
            quality.confidence_score,
            verification_suffix,
        )
    } else {
        format!(
            "◈ context pack verification failed {} selector={} artifact={} coverage={:.2} avg_support={:.2} support_ratio={:.2} confidence={:.2}{}",
            pack.pack_id,
            selector,
            pack.artifact_id,
            quality.citation_coverage,
            quality.avg_support_strength,
            quality.supported_claim_ratio,
            quality.confidence_score,
            verification_suffix,
        )
    };

    (prompt_with_pack, Some(note), Some(quality.is_usable))
}

pub async fn cheap_memory_recall_probe(
    tui_rt: &TuiRuntime,
    session_id: &str,
    prompt: &str,
) -> CheapRecallProbe {
    let query_text = truncate_text_for_budget(prompt, CHEAP_RECALL_QUERY_MAX_CHARS)
        .trim()
        .to_string();
    if query_text.is_empty() {
        return CheapRecallProbe::default();
    }

    let mut request = MemoryRecallRequest {
        query_text: Some(query_text),
        limit: CHEAP_RECALL_LIMIT,
        ..Default::default()
    };
    request.scope.session_ids = Some(vec![session_id.to_string()]);

    match tui_rt.memory_reader.recall(&request).await {
        Ok(response) => {
            let node_sync_keys = response
                .node_sync_keys
                .into_iter()
                .take(CHEAP_RECALL_MAX_KEYS)
                .collect::<Vec<_>>();
            let snippets = hydrate_recall_snippets(tui_rt, session_id, &node_sync_keys).await;

            CheapRecallProbe {
                attempted: true,
                retrieved: response.retrieved,
                retrieval_path: response.retrieval_path,
                fallback_triggered: response.fallback_triggered,
                fallback_reason: response.fallback_reason,
                node_sync_keys,
                snippets,
                error: None,
            }
        }
        Err(err) => CheapRecallProbe {
            attempted: true,
            error: Some(err.to_string()),
            ..Default::default()
        },
    }
}

pub async fn channel_policy_probe(
    tui_rt: &TuiRuntime,
    policy_profile: Option<&str>,
) -> ChannelAmbientPolicy {
    let effective_policy_profile = policy_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| Some(default_policy_profile_for_lane(EngineExecutionLane::Interactive)));

    let request = policy_identity_context_request(
        resolve_identity_user_id(None),
        resolve_identity_persona_id(),
        resolve_identity_channel_id(effective_policy_profile),
        4,
    );

    match tui_rt
        .identity_memory_store
        .get_identity_context(&request)
        .await
    {
        Ok(context) => ChannelAmbientPolicy {
            proactive_allowed: context.channel.as_ref().map(|channel| channel.proactive_allowed),
            identity_channel_type: context.channel.as_ref().map(|channel| channel.channel_type.clone()),
        },
        Err(_) => ChannelAmbientPolicy::default(),
    }
}

pub async fn identity_context_probe(
    tui_rt: &TuiRuntime,
    policy_profile: Option<&str>,
    query_hints: Option<&str>,
    manuscript: Option<&ManuscriptContext>,
) -> IdentityContextProbe {
    let effective_policy_profile = policy_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| Some(default_policy_profile_for_lane(EngineExecutionLane::Interactive)));
    let identity_user_id = resolve_identity_user_id(None);

    let snapshot = load_cognitive_identity_snapshot(
        Some(&tui_rt.identity_memory_store),
        &identity_user_id,
        effective_policy_profile,
        8,
    )
    .await;
    let mut options = DigestCompileOptions::from_product_config(DEFAULT_RELATIONAL_DIGEST_BUDGET)
        .with_query_hints(query_hints.unwrap_or_default());
    if let Some(manuscript) = manuscript {
        options = digest_options_for_manuscript(options, manuscript);
    }
    let ranked = compile_relational_memory_digest_with_options(&snapshot, options);
    let diagnostics =
        cognitive_identity_diagnostics_with_stats(&snapshot, Some(&ranked.stats));
    let summary = Some(format!("{}\n{diagnostics}", ranked.text));
    let error = snapshot.error;

    IdentityContextProbe {
        attempted: true,
        summary,
        error,
    }
}

pub async fn hydrate_recall_snippets(
    tui_rt: &TuiRuntime,
    session_id: &str,
    node_sync_keys: &[String],
) -> Vec<RecallSnippet> {
    if node_sync_keys.is_empty() {
        return Vec::new();
    }

    let nodes = match tui_rt
        .locus_store
        .query_nodes_async(NodeQuery {
            limit: CHEAP_RECALL_NODE_SCAN_LIMIT,
            session_id: Some(session_id.to_string()),
            ..Default::default()
        })
        .await
    {
        Ok(nodes) => nodes,
        Err(_) => return Vec::new(),
    };

    let by_key = nodes
        .into_iter()
        .map(|node| (node.sync_key.clone(), node))
        .collect::<HashMap<_, _>>();

    node_sync_keys
        .iter()
        .filter_map(|sync_key| by_key.get(sync_key).map(|node| (sync_key, node)))
        .take(CHEAP_RECALL_SNIPPET_MAX_COUNT)
        .map(|(sync_key, node)| {
            let summary = sanitize_prompt_line(
                node.context_summary
                    .as_deref()
                    .unwrap_or("context_summary_unavailable"),
            );
            let excerpt_source = if let Some(summary) = node.context_summary.as_deref() {
                summary
            } else {
                &node.raw
            };

            RecallSnippet {
                sync_key: sync_key.clone(),
                context_summary: truncate_text_for_budget(&summary, 120),
                excerpt: truncate_text_for_budget(
                    &sanitize_prompt_line(excerpt_source),
                    CHEAP_RECALL_SNIPPET_MAX_CHARS,
                ),
            }
        })
        .collect()
}

pub fn append_memory_recall_hint(prompt: &str, recall: &CheapRecallProbe) -> String {
    if !recall.attempted {
        return prompt.to_string();
    }

    let keys = if recall.node_sync_keys.is_empty() {
        "none".to_string()
    } else {
        recall.node_sync_keys.join(",")
    };
    let status = if recall.retrieved > 0 { "hit" } else { "miss" };
    let fallback_reason = sanitize_prompt_line(recall.fallback_reason.as_deref().unwrap_or("none"));
    let snippets_block = if recall.snippets.is_empty() {
        "none".to_string()
    } else {
        recall
            .snippets
            .iter()
            .map(|snippet| {
                format!(
                    "- key={} summary={} excerpt={}",
                    snippet.sync_key, snippet.context_summary, snippet.excerpt
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let miss_guidance = if status == "miss" {
        "\nmiss_fallback_policy=Do not stop at status=miss — try cognition_capability_invoke, cognition_grapheme_run, or reason explicitly from the current request before saying you lack memory.\n"
    } else {
        ""
    };

    format!(
        "{prompt}\n\n[MEDOUSA_MEMORY_RECALL]\nstatus={status}\nretrieved={}\nretrieval_path={}\nfallback_triggered={}\nfallback_reason={}\nnode_sync_keys={}\nrecall_snippets:\n{}{miss_guidance}",
        recall.retrieved,
        recall.retrieval_path.as_deref().unwrap_or("none"),
        recall.fallback_triggered,
        truncate_text_for_budget(&fallback_reason, 200),
        keys,
        snippets_block,
    )
}

pub fn append_suggested_capabilities_hint(prompt: &str, capability_ids: &[String]) -> String {
    if capability_ids.is_empty() {
        return prompt.to_string();
    }
    let ids = capability_ids.join(", ");
    format!(
        "{prompt}\n\n[MEDOUSA_SUGGESTED_CAPABILITIES]\nids={ids}\nPrefer cognition_capability_resolve and cognition_capability_invoke with these capability ids when the task needs them."
    )
}

pub fn append_manuscript_hint(prompt: &str, manuscript: Option<&ManuscriptContext>) -> String {
    let Some(manuscript) = manuscript else {
        return prompt.to_string();
    };
    format!(
        "{prompt}\n\n{}",
        truncate_text_for_budget(&format_manuscript_prompt_block(manuscript), 1_200)
    )
}

pub fn append_identity_context_hint(prompt: &str, identity: &IdentityContextProbe) -> String {
    if !identity.attempted {
        return prompt.to_string();
    }

    let status = if identity.summary.is_some() {
        "ready"
    } else {
        "missing"
    };
    let summary = sanitize_prompt_line(identity.summary.as_deref().unwrap_or("none"));
    let error = sanitize_prompt_line(identity.error.as_deref().unwrap_or("none"));

    format!(
        "{prompt}\n\n[MEDOUSA_IDENTITY_CONTEXT]\nstatus={status}\nsummary={}\nerror={}",
        truncate_text_for_budget(&summary, 260),
        truncate_text_for_budget(&error, 220),
    )
}

pub fn sanitize_prompt_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn build_prompt_with_context_pack(
    prompt: &str,
    pack: &crate::context_pack::ContextPack,
    policy: &crate::verifier::VerificationPolicy,
) -> (
    String,
    ContextPackQuality,
    crate::verifier::VerificationReport,
) {
    let report = crate::verifier::verify_context_pack(pack, policy);
    let quality = ContextPackQuality {
        citation_coverage: report.citation_coverage,
        avg_support_strength: report.avg_support_strength,
        supported_claim_ratio: report.supported_claim_ratio,
        confidence_score: report.confidence_score,
        is_usable: report.is_verified,
    };

    if !quality.is_usable {
        let fallback = format!(
            "{prompt}\n\n[MEDOUSA_CONTEXT_PACK]\nstatus=verification_failed\npack_id={}\nartifact_id={}\ncitation_coverage={:.2}\navg_support={:.2}\nsupported_claim_ratio={:.2}\nconfidence={:.2}\npolicy=Treat context pack claims as non-authoritative. If evidence is needed, call tools or request fresher data.",
            pack.pack_id,
            pack.artifact_id,
            quality.citation_coverage,
            quality.avg_support_strength,
            quality.supported_claim_ratio,
            quality.confidence_score,
        );
        return (fallback, quality, report);
    }

    let claim_lines = pack
        .selected_claims
        .iter()
        .take(8)
        .map(|claim| {
            let refs = if claim.supporting_chunk_node_ids.is_empty() {
                "none".to_string()
            } else {
                claim
                    .supporting_chunk_node_ids
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let statement = truncate_text_for_budget(&claim.statement, 360);
            format!(
                "- [{}] strength={:.2} refs={} {}",
                claim.claim_id, claim.support_strength, refs, statement
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let chunk_lines = pack
        .selected_chunk_refs
        .iter()
        .take(8)
        .map(|chunk| {
            format!(
                "- {} tokens={} hash={}",
                chunk.node_id, chunk.token_estimate, chunk.hash64
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let augmented = format!(
        "{prompt}\n\n[MEDOUSA_CONTEXT_PACK]\nstatus=verified\npack_id={}\nartifact_id={}\ntoken_estimate={}\ncitation_coverage={:.2}\navg_support={:.2}\nsupported_claim_ratio={:.2}\nconfidence={:.2}\nclaims:\n{}\nchunks:\n{}",
        pack.pack_id,
        pack.artifact_id,
        pack.total_token_estimate,
        quality.citation_coverage,
        quality.avg_support_strength,
        quality.supported_claim_ratio,
        quality.confidence_score,
        claim_lines,
        chunk_lines,
    );

    (augmented, quality, report)
}

pub fn truncate_text_for_budget(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    if max_chars <= 12 {
        return text.chars().take(max_chars).collect();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 5);
    let head_part = text.chars().take(head).collect::<String>();
    let tail_part = text
        .chars()
        .skip(total_chars.saturating_sub(tail))
        .collect::<String>();
    format!("{head_part}\n...\n{tail_part}")
}

pub fn verifier_policy_from_settings_and_route(
    settings: &RuntimeSettings,
    verifier_route: Option<&StageRoute>,
) -> crate::verifier::VerificationPolicy {
    let mut policy = crate::verifier::VerificationPolicy {
        min_citation_coverage: parse_f32_with_bounds(
            &settings.verifier_min_citation_coverage,
            0.60,
            0.0,
            1.0,
        ),
        min_avg_support_strength: parse_f32_with_bounds(
            &settings.verifier_min_avg_support_strength,
            0.70,
            0.0,
            1.0,
        ),
        min_supported_claim_ratio: parse_f32_with_bounds(
            &settings.verifier_min_supported_claim_ratio,
            0.60,
            0.0,
            1.0,
        ),
        min_claim_support_strength: parse_f32_with_bounds(
            &settings.verifier_min_claim_support_strength,
            0.65,
            0.0,
            1.0,
        ),
    };

    if let Some(route) = verifier_route {
        apply_verifier_policy_profile(&mut policy, &route.policy_profile);
    }

    policy
}

pub fn apply_verifier_policy_profile(
    policy: &mut crate::verifier::VerificationPolicy,
    policy_profile: &str,
) {
    match policy_profile.trim().to_ascii_lowercase().as_str() {
        "strict" => {
            policy.min_citation_coverage = policy.min_citation_coverage.max(0.70);
            policy.min_avg_support_strength = policy.min_avg_support_strength.max(0.75);
            policy.min_supported_claim_ratio = policy.min_supported_claim_ratio.max(0.70);
            policy.min_claim_support_strength = policy.min_claim_support_strength.max(0.72);
        }
        "analytical" => {
            policy.min_citation_coverage = policy.min_citation_coverage.max(0.65);
            policy.min_avg_support_strength = policy.min_avg_support_strength.max(0.78);
            policy.min_supported_claim_ratio = policy.min_supported_claim_ratio.max(0.62);
            policy.min_claim_support_strength = policy.min_claim_support_strength.max(0.76);
        }
        "fast" => {
            policy.min_citation_coverage = policy.min_citation_coverage.min(0.50);
            policy.min_avg_support_strength = policy.min_avg_support_strength.min(0.55);
            policy.min_supported_claim_ratio = policy.min_supported_claim_ratio.min(0.50);
            policy.min_claim_support_strength = policy.min_claim_support_strength.min(0.52);
        }
        _ => {}
    }
}

pub fn derive_recall_readiness(
    verification_state: Option<bool>,
    recall_attempted: bool,
    recall_retrieved: usize,
    identity_context_ready: bool,
) -> RecallReadiness {
    if verification_state == Some(true) || recall_retrieved > 0 || identity_context_ready {
        RecallReadiness::Verified
    } else if verification_state == Some(false) || recall_attempted {
        RecallReadiness::Unverified
    } else {
        RecallReadiness::Missing
    }
}

pub fn compile_interactive_context_prompt(
    user_prompt: &str,
    response_depth_mode: &str,
    stage_route: Option<&StageRoute>,
    recall_readiness: RecallReadiness,
) -> crate::engine_context::ContextCompilerOutput {
    compile_context_prompt(ContextCompilerInput {
        lane: EngineExecutionLane::Interactive,
        user_prompt,
        response_depth_mode,
        stage_route,
        recall_readiness,
    })
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::artifact_chunking::SttpChunkNodeRef;
    use crate::artifact_extraction::EvidenceClaim;
    use crate::context_pack::{ContextPack, ContextPackBudgetProfile};
    use crate::engine_context::RecallReadiness;
    use crate::stage_routing::StageRoute;
    use crate::verifier::VerificationPolicy;

    use super::{
        CheapRecallProbe, IdentityContextProbe, RecallSnippet, append_identity_context_hint,
        append_manuscript_hint, append_memory_recall_hint, build_prompt_with_context_pack,
        compile_interactive_context_prompt,
        derive_recall_readiness, verifier_policy_from_settings_and_route,
    };
    use crate::tui::settings::RuntimeSettings;

    fn sample_pack() -> ContextPack {
        ContextPack {
            pack_id: "pack:test:1".to_string(),
            session_id: "session-1".to_string(),
            artifact_id: "artifact-1".to_string(),
            created_at_utc: Utc::now(),
            budget_profile: ContextPackBudgetProfile {
                max_tokens: 3200,
                max_claims: 6,
                max_chunks: 12,
            },
            selected_claims: vec![EvidenceClaim {
                claim_id: "claim-1".to_string(),
                statement: "The payload contains two result entries.".to_string(),
                supporting_chunk_node_ids: vec!["sttp:artifact-1:chunk:0".to_string()],
                support_strength: 0.88,
            }],
            selected_chunk_refs: vec![SttpChunkNodeRef {
                node_id: "sttp:artifact-1:chunk:0".to_string(),
                chunk_id: "artifact-1:chunk:0".to_string(),
                sequence: 0,
                token_estimate: 120,
                hash64: "abc123".to_string(),
            }],
            total_token_estimate: 120,
        }
    }

    #[test]
    fn prompt_includes_pack_when_quality_is_usable() {
        let pack = sample_pack();
        let policy = VerificationPolicy::default();
        let (prompt, quality, _) =
            build_prompt_with_context_pack("Summarize latest run", &pack, &policy);
        assert!(quality.is_usable);
        assert!(prompt.contains("[MEDOUSA_CONTEXT_PACK]"));
        assert!(prompt.contains("status=verified"));
        assert!(prompt.contains("claims:"));
    }

    #[test]
    fn quality_rejects_low_coverage_pack() {
        let mut pack = sample_pack();
        pack.selected_claims[0].supporting_chunk_node_ids.clear();
        pack.selected_claims[0].support_strength = 0.40;

        let policy = VerificationPolicy::default();
        let (prompt, quality, _) =
            build_prompt_with_context_pack("Summarize latest run", &pack, &policy);
        assert!(!quality.is_usable);
        assert!(prompt.contains("status=verification_failed"));
    }

    #[test]
    fn derives_policy_from_settings_values() {
        let settings = RuntimeSettings {
            backend: "in-memory".to_string(),
            theme_id: "medousa-default".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: String::new(),
            env_overrides: String::new(),
            api_key: String::new(),
            allowed_modules: String::new(),
            tool_call_mode: "auto".to_string(),
            max_tool_rounds: "10".to_string(),
            host_bus_max_tool_rounds: String::new(),
            host_turn_bus_mode: String::new(),
            activation_tool_intent_max_rounds: String::new(),
            activation_short_turn_max_tool_rounds: String::new(),
            continuation_max_tool_rounds: String::new(),
            max_text_only_stuck_continues: String::new(),
            classifier_restricted_max_tool_rounds: String::new(),
            thinking_capture: "true".to_string(),
            stasis_otel_enabled: "false".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "10".to_string(),
            verifier_min_citation_coverage: "0.55".to_string(),
            verifier_min_avg_support_strength: "0.66".to_string(),
            verifier_min_supported_claim_ratio: "0.77".to_string(),
            verifier_min_claim_support_strength: "0.88".to_string(),
            web_search_preferred_provider: String::new(),
            web_search_try_fallbacks: "true".to_string(),
        };

        let policy = verifier_policy_from_settings_and_route(&settings, None);
        assert!((policy.min_citation_coverage - 0.55).abs() < 0.001);
        assert!((policy.min_avg_support_strength - 0.66).abs() < 0.001);
        assert!((policy.min_supported_claim_ratio - 0.77).abs() < 0.001);
        assert!((policy.min_claim_support_strength - 0.88).abs() < 0.001);
    }

    #[test]
    fn strict_route_profile_tightens_verifier_policy() {
        let settings = RuntimeSettings {
            backend: "in-memory".to_string(),
            theme_id: "medousa-default".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: String::new(),
            env_overrides: String::new(),
            api_key: String::new(),
            allowed_modules: String::new(),
            tool_call_mode: "auto".to_string(),
            max_tool_rounds: "10".to_string(),
            host_bus_max_tool_rounds: String::new(),
            host_turn_bus_mode: String::new(),
            activation_tool_intent_max_rounds: String::new(),
            activation_short_turn_max_tool_rounds: String::new(),
            continuation_max_tool_rounds: String::new(),
            max_text_only_stuck_continues: String::new(),
            classifier_restricted_max_tool_rounds: String::new(),
            thinking_capture: "true".to_string(),
            stasis_otel_enabled: "false".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "10".to_string(),
            verifier_min_citation_coverage: "0.55".to_string(),
            verifier_min_avg_support_strength: "0.66".to_string(),
            verifier_min_supported_claim_ratio: "0.57".to_string(),
            verifier_min_claim_support_strength: "0.61".to_string(),
            web_search_preferred_provider: String::new(),
            web_search_try_fallbacks: "true".to_string(),
        };
        let route = StageRoute {
            role: "verifier".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            policy_profile: "strict".to_string(),
            fallback_chain: vec!["verifier".to_string()],
        };

        let policy = verifier_policy_from_settings_and_route(&settings, Some(&route));
        assert!((policy.min_citation_coverage - 0.70).abs() < 0.001);
        assert!((policy.min_avg_support_strength - 0.75).abs() < 0.001);
        assert!((policy.min_supported_claim_ratio - 0.70).abs() < 0.001);
        assert!((policy.min_claim_support_strength - 0.72).abs() < 0.001);
    }

    #[test]
    fn derive_recall_readiness_marks_verified_for_verified_pack() {
        let readiness = derive_recall_readiness(Some(true), false, 0, false);
        assert_eq!(readiness, RecallReadiness::Verified);
    }

    #[test]
    fn derive_recall_readiness_marks_verified_for_recall_hit() {
        let readiness = derive_recall_readiness(None, true, 1, false);
        assert_eq!(readiness, RecallReadiness::Verified);
    }

    #[test]
    fn derive_recall_readiness_marks_verified_for_identity_context() {
        let readiness = derive_recall_readiness(None, false, 0, true);
        assert_eq!(readiness, RecallReadiness::Verified);
    }

    #[test]
    fn derive_recall_readiness_marks_unverified_for_attempt_without_hit() {
        let readiness = derive_recall_readiness(None, true, 0, false);
        assert_eq!(readiness, RecallReadiness::Unverified);
    }

    #[test]
    fn derive_recall_readiness_marks_missing_when_no_signals_exist() {
        let readiness = derive_recall_readiness(None, false, 0, false);
        assert_eq!(readiness, RecallReadiness::Missing);
    }

    #[test]
    fn interactive_compiler_helper_emits_interactive_metadata() {
        let route = StageRoute {
            role: "final_response".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            policy_profile: "interactive".to_string(),
            fallback_chain: vec!["openai:gpt-4o-mini".to_string()],
        };

        let output = compile_interactive_context_prompt(
            "Summarize the latest run",
            "standard",
            Some(&route),
            RecallReadiness::Verified,
        );

        assert!(output.compiled_prompt.contains("[MEDOUSA_CONTEXT_COMPILER]"));
        assert!(output.compiled_prompt.contains("lane=interactive"));
        assert!(output.compiled_prompt.contains("lane_policy_profile=interactive"));
        assert!(output.allow_no_tools_fallback);
    }

    #[test]
    fn recall_hint_includes_snippet_block_when_available() {
        let hint = append_memory_recall_hint(
            "Explain this",
            &CheapRecallProbe {
                attempted: true,
                retrieved: 1,
                retrieval_path: Some("semantic".to_string()),
                fallback_triggered: false,
                fallback_reason: None,
                node_sync_keys: vec!["sync-1".to_string()],
                snippets: vec![RecallSnippet {
                    sync_key: "sync-1".to_string(),
                    context_summary: "previous architecture decision".to_string(),
                    excerpt: "we chose heartbeat notify threshold 0.65".to_string(),
                }],
                error: None,
            },
        );

        assert!(hint.contains("[MEDOUSA_MEMORY_RECALL]"));
        assert!(hint.contains("recall_snippets:"));
        assert!(hint.contains("previous architecture decision"));
    }

    #[test]
    fn identity_hint_includes_summary_when_available() {
        let hint = append_identity_context_hint(
            "Explain this",
            &IdentityContextProbe {
                attempted: true,
                summary: Some(
                    "[MEDOUSA_RELATIONAL_MEMORY]\nstatus=ready\npreferences: beverage=matcha\nmode=cognitive contacts=0 preferences=1 relationships=0".to_string(),
                ),
                error: None,
            },
        );

        assert!(hint.contains("[MEDOUSA_IDENTITY_CONTEXT]"));
        assert!(hint.contains("status=ready"));
        assert!(hint.contains("beverage=matcha"));
    }

    #[test]
    fn manuscript_hint_includes_specialty_block() {
        let manuscript = crate::identity_manuscript::ManuscriptContext {
            id: "morning-brief".to_string(),
            name: "Morning Brief".to_string(),
            description: Some("Daily summary".to_string()),
            display_name: None,
            voice_appendix: Some("Concise chief-of-staff.".to_string()),
            system_appendix: Some("Lead with overnight deltas.".to_string()),
            task_template: None,
            pinned_preferences: vec!["timezone".to_string()],
            pinned_contact_ids: Vec::new(),
            recall_hints: Vec::new(),
            worker_intent: Some("research".to_string()),
            worker_stage_role: Some("extractor".to_string()),
            worker_model_hint: None,
            max_tool_rounds: None,
            tools_allow: Vec::new(),
            locus_session_id: None,
            delivery_mode: None,
            delivery_on_complete: None,
            schedule_cron: None,
            schedule_execution_mode: None,
            openshell_enabled: false,
            openshell_policy_template: None,
            openshell_sandbox_from: None,
            openshell_allow_scheduled: false,
            extends_from: None,
            source_path: std::path::PathBuf::from("morning-brief.yaml"),
        };
        let hint = append_manuscript_hint("Plan my day", Some(&manuscript));
        assert!(hint.contains("[MEDOUSA_MANUSCRIPT]"));
        assert!(hint.contains("morning-brief"));
        assert!(hint.contains("chief-of-staff"));
        assert!(hint.contains("worker_stage_role=extractor"));
    }
}
