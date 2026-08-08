//! Forge-scoped semantic working memory for Coder.
//!
//! Locus stores the temporal nodes, while this module owns the Coder-facing
//! scope, compact schemas, canonical STTP construction, and bounded recall
//! projection. The model never chooses the underlying Locus session.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Component, Path};

use chrono::{SecondsFormat, Utc};
use genai::chat::Tool;
use locus_core_rs::SttpNodeParser;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use stasis::prelude::{Result, StasisError};

use super::coder_activity::CoderAgentIdentity;
use super::coder_mode::CoderEntryContext;

pub const COGNITION_CODER_MEMORY_OVERVIEW: &str = "cognition_coder_memory_overview";
pub const COGNITION_CODER_MEMORY_RECALL: &str = "cognition_coder_memory_recall";
pub const COGNITION_CODER_MEMORY_COMMIT: &str = "cognition_coder_memory_commit";

pub const CODER_MEMORY_TOOL_NAMES: &[&str] = &[
    COGNITION_CODER_MEMORY_OVERVIEW,
    COGNITION_CODER_MEMORY_RECALL,
    COGNITION_CODER_MEMORY_COMMIT,
];

const MEMORY_KINDS: &[&str] = &[
    "goal",
    "discovery",
    "hypothesis",
    "decision",
    "change",
    "verification",
    "open_gap",
    "checkpoint",
    "handoff",
];

const MEMORY_RELATIONS: &[&str] = &[
    "supports",
    "contradicts",
    "supersedes",
    "depends_on",
    "applies_to",
    "verified_by",
    "derived_from",
    "blocks",
    "resolves",
];

const MAX_SUMMARY_CHARS: usize = 2_000;
const MAX_DETAILS_CHARS: usize = 6_000;
const MAX_LIST_ITEMS: usize = 20;
const MAX_ITEM_CHARS: usize = 512;
const MAX_RELATIONS: usize = 16;
const MAX_RECALL_LIMIT: usize = 12;
const MAX_OVERVIEW_LIMIT: usize = 20;
const MAX_RECALLED_RAW_CHARS: usize = 6_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderMemoryScope {
    pub session_id: String,
    pub repo_id: String,
    pub work_id: String,
    pub branch: String,
    pub branch_digest: String,
    pub environment_generation: u32,
}

impl CoderMemoryScope {
    pub fn for_entry(entry: &CoderEntryContext) -> Self {
        let branch_digest = short_digest(&entry.branch);
        let environment_key = environment_memory_key(
            &entry.repo_id,
            &entry.work_id,
            &branch_digest,
            entry.environment_generation,
        );
        Self {
            session_id: crate::locus_memory::resolve_workshop_locus_session(&environment_key),
            repo_id: entry.repo_id.clone(),
            work_id: entry.work_id.clone(),
            branch: entry.branch.clone(),
            branch_digest,
            environment_generation: entry.environment_generation,
        }
    }

    pub fn public_descriptor(&self) -> Value {
        json!({
            "repo_id": self.repo_id,
            "work_id": self.work_id,
            "branch": self.branch,
            "environment_generation": self.environment_generation,
            "environment_id": format!("{}:g{}", self.branch_digest, self.environment_generation),
        })
    }

    pub fn base_tags(&self) -> Vec<String> {
        vec![
            "coder-memory".to_string(),
            format!("repo:{}", self.repo_id),
            format!("work:{}", self.work_id),
            format!(
                "environment:{}:g{}",
                self.branch_digest, self.environment_generation
            ),
        ]
    }
}

fn environment_memory_key(
    repo_id: &str,
    work_id: &str,
    branch_digest: &str,
    generation: u32,
) -> String {
    format!("coder:{repo_id}:{work_id}:{branch_digest}:g{generation}")
}

fn short_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..16].to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoderMemoryRelation {
    pub relation: String,
    pub target: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoderMemoryCommit {
    pub kind: String,
    pub summary: String,
    pub raw_node: String,
    pub semantic_tags: Vec<String>,
    pub dedupe_tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderMemoryRecallQuery {
    pub query: String,
    pub kind: Option<String>,
    pub path: Option<String>,
    pub limit: usize,
}

pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(COGNITION_CODER_MEMORY_OVERVIEW)
            .with_description(
                "Load the compact semantic working state pinned to this governed Coder environment: goals, decisions, touched paths, verification, open gaps, and checkpoints.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_OVERVIEW_LIMIT }
                }
            })),
        Tool::new(COGNITION_CODER_MEMORY_RECALL)
            .with_description(
                "Recall bounded STTP working-memory nodes from this governed Coder environment. The runtime pins scope and labels observations stale when HEAD changed.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "maxLength": MAX_SUMMARY_CHARS },
                    "kind": { "type": "string", "enum": MEMORY_KINDS },
                    "path": { "type": "string", "description": "Optional repository-relative path" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_RECALL_LIMIT }
                },
                "required": ["query"]
            })),
        Tool::new(COGNITION_CODER_MEMORY_COMMIT)
            .with_description(
                "Commit an explicit engineering working-state summary as canonical STTP in this governed environment. Store decisions and evidence, not private reasoning or raw source/log payloads.",
            )
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": MEMORY_KINDS },
                    "summary": { "type": "string", "maxLength": MAX_SUMMARY_CHARS },
                    "details": { "type": "string", "maxLength": MAX_DETAILS_CHARS },
                    "paths": {
                        "type": "array",
                        "maxItems": MAX_LIST_ITEMS,
                        "items": { "type": "string", "maxLength": MAX_ITEM_CHARS }
                    },
                    "symbols": {
                        "type": "array",
                        "maxItems": MAX_LIST_ITEMS,
                        "items": { "type": "string", "maxLength": MAX_ITEM_CHARS }
                    },
                    "evidence_refs": {
                        "type": "array",
                        "maxItems": MAX_LIST_ITEMS,
                        "items": { "type": "string", "maxLength": MAX_ITEM_CHARS }
                    },
                    "relations": {
                        "type": "array",
                        "maxItems": MAX_RELATIONS,
                        "items": {
                            "type": "object",
                            "properties": {
                                "rel": { "type": "string", "enum": MEMORY_RELATIONS },
                                "target": { "type": "string", "maxLength": MAX_ITEM_CHARS },
                                "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                            },
                            "required": ["rel", "target"]
                        }
                    }
                },
                "required": ["kind", "summary"]
            })),
    ]
}

pub fn overview_limit(input: &Value) -> usize {
    input
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(10)
        .clamp(1, MAX_OVERVIEW_LIMIT)
}

pub fn parse_recall_query(input: &Value) -> Result<CoderMemoryRecallQuery> {
    let query = required_text(input, "query", MAX_SUMMARY_CHARS)?;
    let kind = optional_text(input, "kind", 64)?;
    if let Some(kind) = kind.as_deref()
        && !MEMORY_KINDS.contains(&kind)
    {
        return Err(input_error(format!(
            "unknown Coder memory kind '{kind}'; expected one of {}",
            MEMORY_KINDS.join(", ")
        )));
    }
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(normalize_relative_path)
        .transpose()?;
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(6)
        .clamp(1, MAX_RECALL_LIMIT);
    Ok(CoderMemoryRecallQuery {
        query,
        kind,
        path,
        limit,
    })
}

pub fn validate_raw_node_scope(raw_node: &str, expected_session_id: &str) -> Result<()> {
    let parsed = SttpNodeParser::with_profile(crate::locus_memory::resolve_locus_ingest_profile())
        .try_parse(raw_node, expected_session_id);
    if !parsed.success {
        return Err(input_error(format!(
            "advanced raw STTP must parse before it can be stored: {}",
            parsed
                .error
                .unwrap_or_else(|| "unknown STTP validation error".to_string())
        )));
    }
    let envelope_start = unquoted_marker_positions(raw_node, "⦿⟨")
        .first()
        .copied()
        .ok_or_else(|| input_error("advanced raw STTP is missing its envelope block"))?;
    let content_start = unquoted_marker_positions(raw_node, "◈⟨")
        .first()
        .copied()
        .filter(|content_start| *content_start > envelope_start)
        .ok_or_else(|| input_error("advanced raw STTP is missing its content block"))?;
    let envelope = &raw_node[envelope_start..content_start];
    let session_ids = unquoted_string_fields(envelope, "session_id");
    if session_ids.as_slice() != [expected_session_id] {
        return Err(input_error(
            "advanced raw STTP session_id must match the governed Coder environment",
        ));
    }
    Ok(())
}

pub fn build_commit(
    input: &Value,
    scope: &CoderMemoryScope,
    identity: &CoderAgentIdentity,
    current_head: &str,
) -> Result<CoderMemoryCommit> {
    let kind = required_text(input, "kind", 64)?;
    if !MEMORY_KINDS.contains(&kind.as_str()) {
        return Err(input_error(format!(
            "unknown Coder memory kind '{kind}'; expected one of {}",
            MEMORY_KINDS.join(", ")
        )));
    }
    let summary = super::coder_evidence::redact_evidence_text(&required_text(
        input,
        "summary",
        MAX_SUMMARY_CHARS,
    )?);
    let details = optional_text(input, "details", MAX_DETAILS_CHARS)?
        .map(|details| super::coder_evidence::redact_evidence_text(&details));
    let paths = string_list(input, "paths", MAX_LIST_ITEMS, MAX_ITEM_CHARS)?
        .into_iter()
        .map(|path| normalize_relative_path(&path))
        .collect::<Result<Vec<_>>>()?;
    let mut symbols = string_list(input, "symbols", MAX_LIST_ITEMS, MAX_ITEM_CHARS)?
        .into_iter()
        .map(|symbol| super::coder_evidence::redact_evidence_text(&symbol))
        .collect::<Vec<_>>();
    dedupe_preserving_order(&mut symbols);
    let mut evidence_refs = string_list(input, "evidence_refs", MAX_LIST_ITEMS, MAX_ITEM_CHARS)?
        .into_iter()
        .map(|reference| super::coder_evidence::redact_evidence_text(&reference))
        .collect::<Vec<_>>();
    dedupe_preserving_order(&mut evidence_refs);
    let relations = parse_relations(input)?;

    let dedupe_value = json!({
        "session": scope.session_id,
        "kind": kind,
        "summary": summary,
        "details": details,
        "paths": paths,
        "symbols": symbols,
        "evidence_refs": evidence_refs,
        "relations": relations.iter().map(|relation| json!({
            "rel": relation.relation,
            "target": relation.target,
            "confidence": relation.confidence,
        })).collect::<Vec<_>>(),
        "observed_head": current_head,
    });
    let dedupe_hash = Sha256::digest(
        serde_json::to_vec(&dedupe_value)
            .map_err(|error| input_error(format!("cannot fingerprint Coder memory: {error}")))?,
    );
    let dedupe_key = format!("{dedupe_hash:x}");
    let dedupe_tag = format!("coder-dedupe:{}", &dedupe_key[..40]);

    let mut semantic_tags = scope.base_tags();
    semantic_tags.push(format!("kind:{kind}"));
    semantic_tags.push(format!("head:{}", current_head.trim()));
    semantic_tags.push(dedupe_tag.clone());
    semantic_tags.extend(paths.iter().map(|path| indexed_tag("path", path)));
    semantic_tags.extend(symbols.iter().map(|symbol| indexed_tag("symbol", symbol)));
    dedupe_preserving_order(&mut semantic_tags);

    let links = if relations.is_empty() {
        String::new()
    } else {
        let entries = relations
            .iter()
            .map(|relation| {
                format!(
                    "{{ rel: {}, target: {}, confidence: {:.3} }}",
                    json_string(&relation.relation),
                    json_string(&relation.target),
                    relation.confidence
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(", semantic_links: [{entries}]")
    };

    let context_summary = truncate_chars(&format!("{kind}: {summary}"), 1_000);
    let tags_json = escape_protocol_glyphs(
        &serde_json::to_string(&semantic_tags)
            .map_err(|error| input_error(format!("cannot encode Coder memory tags: {error}")))?,
    );
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut content_fields = vec![
        format!("memory_kind(.99): {}", json_string(&kind)),
        format!("summary(.98): {}", json_string(&summary)),
        format!("observed_head(.99): {}", json_string(current_head.trim())),
        format!("dedupe_key(.99): {}", json_string(&dedupe_key)),
        format!("repo_id(.99): {}", json_string(&scope.repo_id)),
        format!("work_id(.99): {}", json_string(&scope.work_id)),
        format!("branch(.99): {}", json_string(&scope.branch)),
        format!(
            "environment_id(.99): {}",
            json_string(&format!(
                "{}:g{}",
                scope.branch_digest, scope.environment_generation
            ))
        ),
        format!(
            "environment_generation(.99): {}",
            scope.environment_generation
        ),
        format!("author_agent(.99): {}", json_string(&identity.agent_id)),
        format!("author_session(.99): {}", json_string(&identity.session_id)),
        format!("author_turn(.99): {}", json_string(&identity.turn_id)),
        format!("author_attempt(.99): {}", json_string(&identity.attempt_id)),
    ];
    if let Some(details) = details.as_deref() {
        content_fields.push(format!("details(.95): {}", json_string(details)));
    }
    if !paths.is_empty() {
        content_fields.push(format!("paths(.98): {}", json_string_array(&paths)));
    }
    if !symbols.is_empty() {
        content_fields.push(format!("symbols(.96): {}", json_string_array(&symbols)));
    }
    if !evidence_refs.is_empty() {
        content_fields.push(format!(
            "evidence_refs(.99): {}",
            json_string_array(&evidence_refs)
        ));
    }

    let raw_node = format!(
        "⊕⟨ ⏣0{{ trigger: manual, response_format: temporal_node, origin_session: {session}, compression_depth: 1, parent_node: null{links}, prime: {{ attractor_config: {{ stability: 0.94, friction: 0.16, logic: 0.99, autonomy: 0.84 }}, context_summary: {context_summary}, relevant_tier: raw, retrieval_budget: 12, semantic_tags: {tags_json} }} }} ⟩\n\
⦿⟨ ⏣0{{ timestamp: {timestamp}, tier: raw, session_id: {session}, schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.96, autonomy: 0.84, psi: 2.90 }}, model_avec: {{ stability: 0.94, friction: 0.16, logic: 0.99, autonomy: 0.84, psi: 2.93 }} }} ⟩\n\
◈⟨ ⏣0{{\n    {content}\n}} ⟩\n\
⍉⟨ ⏣0{{ rho: 0.98, kappa: 0.99, psi: 2.93, compression_avec: {{ stability: 0.94, friction: 0.16, logic: 0.99, autonomy: 0.84, psi: 2.93 }} }} ⟩",
        session = json_string(&scope.session_id),
        context_summary = json_string(&context_summary),
        timestamp = json_string(&timestamp),
        content = content_fields.join(",\n    "),
    );

    super::sttp::validate_canonical_sttp_node(&raw_node).map_err(|error| {
        input_error(format!(
            "Coder memory compiler emitted invalid STTP: {error}"
        ))
    })?;

    Ok(CoderMemoryCommit {
        kind,
        summary,
        raw_node,
        semantic_tags,
        dedupe_tag,
    })
}

pub fn recall_semantic_tags(query: &CoderMemoryRecallQuery) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(kind) = query.kind.as_deref() {
        tags.push(parser_encoded_string(&format!("kind:{kind}")));
    }
    if let Some(path) = query.path.as_deref() {
        tags.push(parser_encoded_string(&indexed_tag("path", path)));
    }
    tags
}

pub fn first_node_id(result: &Value) -> Option<String> {
    result
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
        .and_then(|node| {
            node.get("sync_key")
                .or_else(|| node.get("node_id"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

pub fn project_recall(
    scope: &CoderMemoryScope,
    current_head: &str,
    result: &Value,
    include_raw: bool,
    limit: usize,
) -> Value {
    let nodes = result
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .map(|node| project_node(node, current_head, include_raw))
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "scope": scope.public_descriptor(),
        "current_head": current_head,
        "retrieved": nodes.len(),
        "nodes": nodes,
    })
}

fn project_node(node: &Value, current_head: &str, include_raw: bool) -> Value {
    let tags = node
        .get("semantic_tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let observed_head = tag_value(&tags, "head:").map(|value| decode_parser_string(&value));
    let kind = tag_value(&tags, "kind:").map(|value| decode_parser_string(&value));
    let paths = tags
        .iter()
        .filter_map(|tag| tag.strip_prefix("path:"))
        .map(decode_parser_string)
        .collect::<Vec<_>>();
    let stale = observed_head
        .as_deref()
        .is_some_and(|head| head != current_head.trim());
    let raw = node.get("raw").and_then(Value::as_str).unwrap_or_default();
    let relations = SttpNodeParser::new()
        .try_parse(raw, "")
        .node
        .and_then(|node| node.semantic_links)
        .unwrap_or_default()
        .into_iter()
        .map(|link| {
            json!({
                "rel": decode_parser_string(&link.rel),
                "target": decode_parser_string(&link.target),
                "confidence": link.confidence,
            })
        })
        .collect::<Vec<_>>();
    let summary = node
        .get("context_summary")
        .and_then(Value::as_str)
        .map(decode_parser_string);
    let mut projected = json!({
        "node_id": node.get("sync_key").or_else(|| node.get("node_id")),
        "kind": kind,
        "summary": summary,
        "timestamp": node.get("timestamp"),
        "observed_head": observed_head,
        "stale": stale,
        "paths": paths,
        "relations": relations,
    });
    if include_raw {
        let content = recalled_content(raw);
        projected["content"] = Value::String(truncate_chars(&content, MAX_RECALLED_RAW_CHARS));
        projected["content_truncated"] =
            Value::Bool(content.chars().count() > MAX_RECALLED_RAW_CHARS);
    }
    projected
}

fn recalled_content(raw: &str) -> String {
    let Some(start) = unquoted_marker_positions(raw, "◈⟨").first().copied() else {
        return raw.to_string();
    };
    let end = unquoted_marker_positions(raw, "⍉⟨")
        .into_iter()
        .find(|end| *end > start)
        .unwrap_or(raw.len());
    decode_sttp_display_strings(raw[start..end].trim())
}

fn tag_value(tags: &[&str], prefix: &str) -> Option<String> {
    tags.iter()
        .find_map(|tag| tag.strip_prefix(prefix))
        .map(str::to_string)
}

fn indexed_tag(prefix: &str, value: &str) -> String {
    let tag = format!("{prefix}:{value}");
    if tag.chars().count() <= 64 {
        tag
    } else {
        format!("{prefix}-sha:{}", short_digest(value))
    }
}

fn required_text(input: &Value, field: &str, max_chars: usize) -> Result<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| input_error(format!("{field} is required")))
        .and_then(|value| bounded_text(value, field, max_chars))
}

fn optional_text(input: &Value, field: &str, max_chars: usize) -> Result<Option<String>> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| bounded_text(value, field, max_chars))
        .transpose()
}

fn bounded_text(value: &str, field: &str, max_chars: usize) -> Result<String> {
    if value.chars().count() > max_chars {
        Err(input_error(format!(
            "{field} exceeds the {max_chars}-character Coder memory limit"
        )))
    } else {
        Ok(value.to_string())
    }
}

fn string_list(
    input: &Value,
    field: &str,
    max_items: usize,
    max_chars: usize,
) -> Result<Vec<String>> {
    let Some(values) = input.get(field) else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| input_error(format!("{field} must be an array")))?;
    if values.len() > max_items {
        return Err(input_error(format!(
            "{field} exceeds the {max_items}-item Coder memory limit"
        )));
    }
    let mut out = Vec::new();
    for value in values {
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| input_error(format!("{field} entries must be non-empty strings")))?;
        out.push(bounded_text(value, field, max_chars)?);
    }
    dedupe_preserving_order(&mut out);
    Ok(out)
}

fn parse_relations(input: &Value) -> Result<Vec<CoderMemoryRelation>> {
    let Some(relations) = input.get("relations") else {
        return Ok(Vec::new());
    };
    let relations = relations
        .as_array()
        .ok_or_else(|| input_error("relations must be an array"))?;
    if relations.len() > MAX_RELATIONS {
        return Err(input_error(format!(
            "relations exceeds the {MAX_RELATIONS}-item Coder memory limit"
        )));
    }
    relations
        .iter()
        .map(|relation| {
            let rel = required_text(relation, "rel", 64)?;
            if !MEMORY_RELATIONS.contains(&rel.as_str()) {
                return Err(input_error(format!(
                    "unknown Coder memory relation '{rel}'; expected one of {}",
                    MEMORY_RELATIONS.join(", ")
                )));
            }
            let target = super::coder_evidence::redact_evidence_text(&required_text(
                relation,
                "target",
                MAX_ITEM_CHARS,
            )?);
            let confidence = relation
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.9);
            if !(0.0..=1.0).contains(&confidence) {
                return Err(input_error("relation confidence must be between 0 and 1"));
            }
            Ok(CoderMemoryRelation {
                relation: rel,
                target,
                confidence,
            })
        })
        .collect()
}

fn normalize_relative_path(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(input_error("memory path cannot be empty"));
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(input_error("memory paths must be repository-relative"));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(input_error(
                    "memory paths cannot escape the governed repository",
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(input_error("memory path cannot be empty"));
    }
    Ok(parts.join("/"))
}

fn dedupe_preserving_order(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>() + "…"
    }
}

fn json_string(value: &str) -> String {
    escape_protocol_glyphs(&serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()))
}

fn json_string_array(values: &[String]) -> String {
    escape_protocol_glyphs(&serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string()))
}

fn parser_encoded_string(value: &str) -> String {
    let encoded = json_string(value);
    encoded
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(&encoded)
        .to_string()
}

fn decode_parser_string(value: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{value}\"")).unwrap_or_else(|_| value.to_string())
}

fn decode_sttp_display_strings(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0usize;
    while let Some(relative_start) = value[cursor..].find('"') {
        let start = cursor + relative_start;
        output.push_str(&value[cursor..start]);
        let mut escaped = false;
        let mut end = None;
        for (relative_end, character) in value[start + 1..].char_indices() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                end = Some(start + 1 + relative_end);
                break;
            }
        }
        let Some(end) = end else {
            output.push_str(&value[start..]);
            return output;
        };
        let token = &value[start..=end];
        if let Ok(decoded) = serde_json::from_str::<String>(token) {
            output.push_str(&serde_json::to_string(&decoded).unwrap_or_else(|_| token.to_string()));
        } else {
            output.push_str(token);
        }
        cursor = end + 1;
    }
    output.push_str(&value[cursor..]);
    output
}

fn escape_protocol_glyphs(value: &str) -> String {
    // Locus 0.4.2's structural lexer also counts braces inside quoted data,
    // so encode both protocol glyphs and object delimiters in runtime-owned
    // string values before assembling the canonical blocks.
    const PROTOCOL_GLYPHS: &str = "{}⊕⟨⟩⦿◈⍉⏣";
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if PROTOCOL_GLYPHS.contains(character) {
            let _ = write!(escaped, "\\u{:04x}", character as u32);
        } else {
            escaped.push(character);
        }
    }
    escaped
}

fn unquoted_marker_positions(value: &str, marker: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if in_string && escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && value[index..].starts_with(marker) {
            positions.push(index);
        }
    }
    positions
}

fn unquoted_string_fields(block: &str, field: &str) -> Vec<String> {
    let marker = format!("{field}:");
    let mut values = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in block.char_indices() {
        if in_string && escaped {
            escaped = false;
            continue;
        }
        if in_string && character == '\\' {
            escaped = true;
            continue;
        }
        if character == '"' {
            in_string = !in_string;
            continue;
        }
        let preceded_by_identifier = block[..index]
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_');
        if in_string || preceded_by_identifier || !block[index..].starts_with(&marker) {
            continue;
        }
        let value = block[index + marker.len()..].trim_start();
        if !value.starts_with('"') {
            continue;
        }
        let mut value_escaped = false;
        for (end, value_character) in value.char_indices().skip(1) {
            if value_escaped {
                value_escaped = false;
            } else if value_character == '\\' {
                value_escaped = true;
            } else if value_character == '"' {
                if let Ok(parsed) = serde_json::from_str::<String>(&value[..=end]) {
                    values.push(parsed);
                }
                break;
            }
        }
    }
    values
}

fn input_error(message: impl Into<String>) -> StasisError {
    StasisError::PortFailure(format!("Coder memory: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use locus_core_rs::ParseProfile;

    use super::*;
    use crate::agent_runtime::coder_mode::{CoderEditorContext, RepositoryInstruction};

    fn entry(branch: &str, generation: u32) -> CoderEntryContext {
        CoderEntryContext {
            repo_id: "repo-123".to_string(),
            work_id: "work-456".to_string(),
            title: "Demo".to_string(),
            brief: "Build memory".to_string(),
            worktree: PathBuf::from("/tmp/demo"),
            branch: branch.to_string(),
            environment_generation: generation,
            baseline_oid: "a".repeat(40),
            head_oid: "b".repeat(40),
            changed_paths: Vec::new(),
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            project_markers: Vec::new(),
            repository_instructions: Vec::<RepositoryInstruction>::new(),
            editor: CoderEditorContext::default(),
        }
    }

    #[test]
    fn memory_scope_is_stable_for_one_environment_and_changes_for_forks() {
        let first = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let same = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let fork = CoderMemoryScope::for_entry(&entry("worktree/demo-a2", 1));
        let restart = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 2));

        assert_eq!(first, same);
        assert_ne!(first.session_id, fork.session_id);
        assert_ne!(first.session_id, restart.session_id);
        assert!(first.session_id.contains("coder:repo-123:work-456:"));
        assert!(!first.session_id.contains("/tmp/demo"));
    }

    #[test]
    fn commit_compiles_strict_typed_sttp_with_scope_and_relations() {
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 7, "attempt-1");
        let commit = build_commit(
            &json!({
                "kind": "decision",
                "summary": "Keep exact turn checkpoints separate from semantic memory",
                "details": "Locus records explicit engineering state; the turn ledger owns protocol replay.",
                "paths": ["src/agent_runtime/coder_memory.rs"],
                "symbols": ["CoderMemoryScope"],
                "evidence_refs": ["engineering:call:7"],
                "relations": [{
                    "rel": "supports",
                    "target": "decision:durable-coder",
                    "confidence": 0.97
                }]
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect("commit");

        crate::agent_runtime::sttp::validate_canonical_sttp_node(&commit.raw_node)
            .expect("runtime STTP");
        validate_raw_node_scope(&commit.raw_node, &scope.session_id).expect("matching scope");
        let scope_error = validate_raw_node_scope(&commit.raw_node, "another-locus-session")
            .expect_err("mismatched scope rejected");
        assert!(
            scope_error
                .to_string()
                .contains("governed Coder environment")
        );
        let parsed = SttpNodeParser::with_profile(ParseProfile::StrictTypedIr)
            .try_parse(&commit.raw_node, &scope.session_id);
        assert!(
            parsed.success,
            "strict parse failed: {:?}\ndiagnostics={:#?}\n{}",
            parsed.error, parsed.diagnostics, commit.raw_node
        );
        let parsed = parsed.node.expect("parsed node");
        assert_eq!(parsed.session_id, scope.session_id);
        assert!(parsed.semantic_tags.as_ref().is_some_and(|tags| {
            tags.contains(&"kind:decision".to_string())
                && tags.contains(&format!("head:{}", entry.head_oid))
        }));
        assert!(parsed.semantic_links.as_ref().is_some_and(|links| {
            links
                .iter()
                .any(|link| link.rel == "supports" && link.target == "decision:durable-coder")
        }));
    }

    #[test]
    fn structured_commit_escapes_hostile_text_without_model_authored_sttp() {
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 8, "attempt-1");
        let hostile = "quoted \"value\", slash \\\\, newline\nbrace { }, comma, and protocol markers ⊕⟨ ⦿⟨ ◈⟨ ⍉⟨ ⏣0{";
        let hostile_path = "src/{odd \"quoted\" name}.rs";
        let commit = build_commit(
            &json!({
                "kind": "discovery",
                "summary": hostile,
                "details": hostile,
                "paths": [hostile_path],
                "symbols": [hostile],
                "evidence_refs": ["coder-evidence:sha256:abc\\def"],
                "relations": [{
                    "rel": "derived_from",
                    "target": hostile
                }]
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect("runtime compiles hostile structured input");

        crate::agent_runtime::sttp::validate_canonical_sttp_node(&commit.raw_node)
            .expect("runtime STTP shape");
        let parsed = SttpNodeParser::with_profile(ParseProfile::StrictTypedIr)
            .try_parse(&commit.raw_node, &scope.session_id);
        assert!(
            parsed.success,
            "strict parser rejected runtime-escaped input: {:?}\ndiagnostics={:#?}\n{}",
            parsed.error, parsed.diagnostics, commit.raw_node
        );
        let parsed = parsed.node.expect("strict node");
        let tags = parsed.semantic_tags.expect("semantic tags");
        let query = parse_recall_query(&json!({
            "query": "hostile path",
            "kind": "discovery",
            "path": hostile_path
        }))
        .expect("recall query");
        for tag in recall_semantic_tags(&query) {
            assert!(
                tags.contains(&tag),
                "stored tag does not match recall filter: {tag}"
            );
        }
        assert_eq!(
            parsed.context_summary.as_deref().map(decode_parser_string),
            Some(format!("discovery: {hostile}"))
        );
        assert!(recalled_content(&commit.raw_node).contains("brace { }"));
        assert!(recalled_content(&commit.raw_node).contains("protocol markers ⊕⟨"));
    }

    #[test]
    fn commit_dedupe_key_is_semantic_and_path_escape_is_rejected() {
        let entry = entry("worktree/demo-a1", 1);
        let scope = CoderMemoryScope::for_entry(&entry);
        let identity = CoderAgentIdentity::for_turn("chat-1", 7, "attempt-1");
        let input = json!({
            "kind": "verification",
            "summary": "Focused tests pass",
            "details": "token=must-not-persist verification remained green",
            "paths": ["src/lib.rs"]
        });
        let first = build_commit(&input, &scope, &identity, &entry.head_oid).expect("first");
        let second = build_commit(&input, &scope, &identity, &entry.head_oid).expect("second");
        assert_eq!(first.dedupe_tag, second.dedupe_tag);
        assert!(!first.raw_node.contains("must-not-persist"));
        assert!(first.raw_node.contains("token=[REDACTED]"));

        let escaped = build_commit(
            &json!({
                "kind": "discovery",
                "summary": "Unsafe path",
                "paths": ["../outside"]
            }),
            &scope,
            &identity,
            &entry.head_oid,
        )
        .expect_err("path escape denied");
        assert!(escaped.to_string().contains("cannot escape"));
    }

    #[test]
    fn recall_projection_labels_changed_head_stale_and_hides_locus_session() {
        let scope = CoderMemoryScope::for_entry(&entry("worktree/demo-a1", 1));
        let result = json!({
            "nodes": [{
                "sync_key": "node-1",
                "session_id": scope.session_id,
                "timestamp": "2026-08-08T00:00:00Z",
                "context_summary": "decision: use environment lineage",
                "semantic_tags": [
                    "kind:decision",
                    "head:old-head",
                    "path:src/lib.rs"
                ],
                "raw": "bounded STTP"
            }]
        });
        let projected = project_recall(&scope, "new-head", &result, true, 5);
        assert_eq!(projected["nodes"][0]["stale"], true);
        assert_eq!(projected["nodes"][0]["kind"], "decision");
        assert_eq!(projected["nodes"][0]["paths"][0], "src/lib.rs");
        assert!(projected.to_string().contains("bounded STTP"));
        assert!(!projected.to_string().contains(&scope.session_id));
    }

    #[test]
    fn recalled_content_ignores_protocol_markers_inside_quoted_data() {
        let raw = "◈⟨ ⏣0{ detail(.95): \"quoted ⍉⟨ marker\", next(.99): \"keep me\" } ⟩\n\
⍉⟨ ⏣0{ rho: 0.9 } ⟩";
        let content = recalled_content(raw);
        assert!(content.contains("quoted ⍉⟨ marker"));
        assert!(content.contains("keep me"));
        assert!(!content.contains("rho: 0.9"));
    }
}
