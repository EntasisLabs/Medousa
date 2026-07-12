//! Locus STTP memory tools (same capabilities as the Locus store; Medousa cognition_* naming).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use locus_core_rs::{CalibrationService, ContextQueryService, MoodCatalogService, NodeStore};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::memory_prelude::{MemoryRecallRequest, MemoryScope, MemoryStoreRequest};
use stasis::memory_prelude_ext::MemoryContextReader;
use stasis::ports::outbound::memory::memory_models::{
    MemoryAvecState, MemoryEvictMode, MemoryEvictRequest, MemoryFallbackPolicy, MemoryFilter,
    MemoryFindRequest, MemorySortDirection, MemorySortField, MemoryStrictnessMode,
};
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use tokio::sync::{RwLock, mpsc};

use crate::events::TuiEvent;
use crate::locus_memory::{
    CANONICAL_STTP_SCHEMA_EXAMPLE, LOCUS_DEFAULT_TENANT, derive_locus_tenant_id,
    ingest_profile_name, normalize_context_keywords, normalize_tiers,
    resolve_locus_ingest_profile, resolve_memory_tool_session_id,
    memory_node_to_json, schema_first_guidance, semantic_index_schema_guidance,
    sttp_node_to_json, store_failure_payload,
    validate_limit, avec_to_json,
};
use crate::turn_continuation::TurnContinuationScope;

const DEFAULT_RECALL_AVEC: (f32, f32, f32, f32) = (0.82, 0.31, 0.88, 0.74);

async fn resolve_optional_locus_session_scope(
    input: &Value,
    turn_scope: &RwLock<Option<TurnContinuationScope>>,
    fallback_chat_session_id: &str,
    workshop_dynamic: bool,
) -> Option<String> {
    if input.get("session_id").is_some_and(Value::is_null) {
        return None;
    }
    Some(
        resolve_memory_tool_session_id(
            input,
            turn_scope,
            fallback_chat_session_id,
            workshop_dynamic,
        )
        .await,
    )
}

async fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx
        .send(TuiEvent::ToolInvoked {
            tool_name: tool_name.to_string(),
            input_summary: summary.chars().take(80).collect(),
        })
        .await;
}

fn parse_utc_optional(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>, String> {
    match value {
        Some(raw) => DateTime::parse_from_rfc3339(raw)
            .map(|parsed| Some(parsed.with_timezone(&Utc)))
            .map_err(|_| format!("{field} must be an ISO8601 UTC datetime")),
        None => Ok(None),
    }
}

fn retrieve_result_to_json(result: locus_core_rs::RetrieveResult) -> Value {
    json!({
        "retrieved": result.retrieved,
        "psi_range": {
            "min": result.psi_range.min,
            "max": result.psi_range.max,
            "average": result.psi_range.average,
        },
        "nodes": result.nodes.iter().map(sttp_node_to_json).collect::<Vec<_>>(),
    })
}

// ── cognition_memory_schema ───────────────────────────────────────────────────

pub struct CognitionMemorySchemaTool;

impl Default for CognitionMemorySchemaTool {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitionMemorySchemaTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StasisTool for CognitionMemorySchemaTool {
    fn name(&self) -> &'static str {
        "cognition_memory_schema"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Return a canonical STTP node example and the active ingest profile before storing memory."
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn invoke(&self, _input: Value) -> StasisResult<Value> {
        let profile = resolve_locus_ingest_profile();
        Ok(json!({
            "canonical_example": CANONICAL_STTP_SCHEMA_EXAMPLE,
            "ingest_profile_policy": ingest_profile_name(profile),
            "semantic_index": semantic_index_schema_guidance(),
            "workflow": [
                "call cognition_memory_schema",
                "optionally cognition_memory_calibrate and cognition_memory_moods",
                "cognition_memory_store with full STTP node string — put semantic_tags in provenance.prime (or pass cognition_memory_store.semantic_tags to merge workshop tags)",
                "optional provenance.semantic_links for typed cross-node relations",
                "cognition_memory_context or cognition_memory_list with semantic_tags for indexed recall",
                "cognition_memory_tags to browse the tag vocabulary",
                "cognition_memory_context for AVEC-ranked retrieval"
            ],
            "model_guidance": schema_first_guidance(
                "Build a complete four-layer STTP node before store; include semantic_tags in prime when you want indexed recall.",
                ingest_profile_name(profile),
            ),
        }))
    }
}

// ── cognition_memory_store ────────────────────────────────────────────────────

pub struct CognitionMemoryStoreTool {
    writer: Arc<dyn MemoryContextWriter>,
    profile_name: &'static str,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryStoreTool {
    pub fn new(
        writer: Arc<dyn MemoryContextWriter>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        let profile_name = ingest_profile_name(resolve_locus_ingest_profile());
        Self {
            writer,
            profile_name,
            fallback_chat_session_id,
            workshop_dynamic,
            turn_scope,
            event_tx,
        }
    }

}

#[async_trait]
impl StasisTool for CognitionMemoryStoreTool {
    fn name(&self) -> &'static str {
        "cognition_memory_store"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Store a complete STTP node in Locus memory. Requires `node` (full STTP string). \
             Optional `session_id` defaults to the current turn session."
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "node": {
                    "type": "string",
                    "description": "Full STTP node payload with ⊕ ⦿ ◈ ⍉ layers"
                },
                "session_id": {
                    "type": "string",
                    "description": "Locus session id (defaults to current turn session)"
                },
                "semantic_tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional Locus semantic tags merged into the STTP prime block"
                },
                "content": {
                    "type": "string",
                    "description": "Deprecated: use `node` with full STTP instead"
                }
            },
            "required": ["node"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let node = input
            .get("node")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_memory_store: `node` (full STTP string) is required. \
                     Call cognition_memory_schema first."
                        .to_string(),
                )
            })?;

        let session_id = resolve_memory_tool_session_id(
            &input,
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await;

        emit_invoked(&self.event_tx, self.name(), &session_id).await;

        let vibe_signature = input
            .get("vibe_signature")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                crate::agent_runtime::derive_vibe_signature(
                    &session_id,
                    None,
                    None,
                    &crate::agent_runtime::default_handoff_model_avec(),
                )
            });
        let mut tags = crate::locus_semantic_tags::default_workshop_semantic_tags(&session_id);
        if let Some(extra) = input.get("semantic_tags").and_then(|v| v.as_array()) {
            for item in extra {
                if let Some(tag) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    tags.push(tag.to_string());
                }
            }
        }
        let tagged_node = crate::locus_semantic_tags::inject_semantic_tags(node, &tags);
        let raw_node =
            crate::locus_memory::enrich_sttp_node_with_vibe_signature(&tagged_node, &vibe_signature);

        let response = self
            .writer
            .store_context(&MemoryStoreRequest {
                session_id,
                raw_node,
            })
            .await?;

        if response.valid {
            Ok(json!({
                "node_id": response.node_id,
                "psi": response.psi,
                "valid": true,
                "stored": true,
                "validation_error": response.validation_error,
                "profile_policy": self.profile_name,
            }))
        } else {
            let message = response
                .validation_error
                .unwrap_or_else(|| "store rejected context".to_string());
            Ok(store_failure_payload(
                response.node_id,
                response.psi,
                false,
                message,
                self.profile_name,
            ))
        }
    }
}

// ── cognition_memory_calibrate ────────────────────────────────────────────────

pub struct CognitionMemoryCalibrateTool {
    calibration: Arc<CalibrationService>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryCalibrateTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            calibration: Arc::new(CalibrationService::new(locus_store)),
            fallback_chat_session_id,
            workshop_dynamic,
            turn_scope,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryCalibrateTool {
    fn name(&self) -> &'static str {
        "cognition_memory_calibrate"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Measure AVEC drift for a session. Call at session start and after heavy reasoning before store/retrieve.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "stability": { "type": "number" },
                "friction": { "type": "number" },
                "logic": { "type": "number" },
                "autonomy": { "type": "number" },
                "trigger": { "type": "string", "description": "e.g. manual, session_start" }
            },
            "required": ["stability", "friction", "logic", "autonomy", "trigger"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = resolve_memory_tool_session_id(
            &input,
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await;
        let stability = input
            .get("stability")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("stability required".into()))? as f32;
        let friction = input
            .get("friction")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("friction required".into()))? as f32;
        let logic = input
            .get("logic")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("logic required".into()))? as f32;
        let autonomy = input
            .get("autonomy")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("autonomy required".into()))? as f32;
        let trigger = input
            .get("trigger")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");

        emit_invoked(&self.event_tx, self.name(), &session_id).await;

        let result = self
            .calibration
            .calibrate_async(&session_id, stability, friction, logic, autonomy, trigger)
            .await
            .map_err(|e| StasisError::PortFailure(format!("cognition_memory_calibrate: {e}")))?;

        Ok(json!({
            "previous_avec": avec_to_json(result.previous_avec),
            "delta": result.delta,
            "drift_classification": format!("{:?}", result.drift_classification),
            "trigger": result.trigger,
            "trigger_history": result.trigger_history,
            "is_first_calibration": result.is_first_calibration,
        }))
    }
}

// ── cognition_memory_context ────────────────────────────────────────────────────

pub struct CognitionMemoryContextTool {
    context_query: Arc<ContextQueryService>,
    memory_reader: Arc<dyn MemoryContextReader>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryContextTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            context_query: Arc::new(ContextQueryService::new(locus_store)),
            memory_reader,
            fallback_chat_session_id,
            workshop_dynamic,
            turn_scope,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryContextTool {
    fn name(&self) -> &'static str {
        "cognition_memory_context"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Primary memory retrieval by AVEC resonance. Requires stability/friction/logic/autonomy. Optional \
             context_keywords and semantic_tags (indexed, match-all). Use tag_prefix for prefix vocabulary search. \
             Set session_id to null for global retrieval across sessions.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": ["string", "null"] },
                "stability": { "type": "number" },
                "friction": { "type": "number" },
                "logic": { "type": "number" },
                "autonomy": { "type": "number" },
                "context_keywords": { "type": "array", "items": { "type": "string" } },
                "semantic_tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Indexed Locus tags (match-all). Example: [\"session\", \"profile:work\"]"
                },
                "tag_prefix": {
                    "type": "string",
                    "description": "Match nodes whose indexed tags share this prefix (e.g. profile:)"
                },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                "alpha": { "type": "number" },
                "beta": { "type": "number" },
                "from_utc": { "type": "string" },
                "to_utc": { "type": "string" },
                "tiers": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["stability", "friction", "logic", "autonomy"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let stability = input
            .get("stability")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("stability required".into()))? as f32;
        let friction = input
            .get("friction")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("friction required".into()))? as f32;
        let logic = input
            .get("logic")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("logic required".into()))? as f32;
        let autonomy = input
            .get("autonomy")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| StasisError::PortFailure("autonomy required".into()))? as f32;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(5);
        let limit = validate_limit(limit, "limit")
            .map_err(StasisError::PortFailure)?;

        let global = input.get("session_id").map(|v| v.is_null()).unwrap_or(false);
        let session_scope = if global {
            None
        } else {
            Some(
                resolve_optional_locus_session_scope(
                    &input,
                    &self.turn_scope,
                    &self.fallback_chat_session_id,
                    self.workshop_dynamic,
                )
                .await
                .expect("non-global session scope"),
            )
        };
        let session_scope = session_scope.as_deref();

        let keywords: Vec<String> = input
            .get("context_keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let keywords = normalize_context_keywords(Some(&keywords));

        let from_utc = parse_utc_optional(
            input.get("from_utc").and_then(|v| v.as_str()),
            "from_utc",
        )
        .map_err(StasisError::PortFailure)?;
        let to_utc =
            parse_utc_optional(input.get("to_utc").and_then(|v| v.as_str()), "to_utc")
                .map_err(StasisError::PortFailure)?;
        let tiers = input
            .get("tiers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let tiers_norm = normalize_tiers(&tiers);
        let tiers_ref = if tiers_norm.is_empty() {
            None
        } else {
            Some(tiers_norm.as_slice())
        };

        emit_invoked(&self.event_tx, self.name(), "context").await;

        let tag_filter = crate::locus_semantic_tags::memory_filter_from_tag_input(&input);
        let has_tag_filters = crate::locus_semantic_tags::input_has_tag_filters(&input);

        if keywords.is_empty() && !has_tag_filters {
            let result = self
                .context_query
                .get_context_scoped_filtered_async(
                    session_scope,
                    stability,
                    friction,
                    logic,
                    autonomy,
                    from_utc,
                    to_utc,
                    tiers_ref,
                    limit,
                )
                .await;
            return Ok(retrieve_result_to_json(result));
        }

        let alpha = input.get("alpha").and_then(|v| v.as_f64()).unwrap_or(0.7) as f32;
        let beta = input.get("beta").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
        let query_text = if keywords.is_empty() {
            None
        } else {
            Some(keywords.join(" "))
        };

        let recall = MemoryRecallRequest {
            scope: MemoryScope {
                tenant_id: {
                    let tenant =
                        crate::locus_memory::derive_locus_tenant_id(session_scope.unwrap_or(""));
                    if tenant == crate::locus_memory::LOCUS_DEFAULT_TENANT {
                        None
                    } else {
                        Some(tenant)
                    }
                },
                session_ids: session_scope.map(|s| vec![s.to_string()]),
                tiers: tiers_ref.map(|t| t.to_vec()),
                from_utc,
                to_utc,
            },
            filter: tag_filter,
            current_avec: Some(MemoryAvecState {
                stability,
                friction,
                logic,
                autonomy,
            }),
            query_text,
            limit,
            alpha,
            beta,
            gamma: 0.0,
            strictness: MemoryStrictnessMode::Balanced,
            fallback_policy: MemoryFallbackPolicy::OnEmpty,
            include_explain: true,
        };

        let response = self
            .memory_reader
            .recall(&recall)
            .await
            .map_err(|e| StasisError::PortFailure(format!("cognition_memory_context: {e}")))?;

        Ok(json!({
            "retrieved": response.retrieved,
            "nodes": response.nodes.iter().map(memory_node_to_json).collect::<Vec<_>>(),
            "retrieval_path": response.retrieval_path,
            "fallback_triggered": response.fallback_triggered,
            "fallback_reason": response.fallback_reason,
            "node_sync_keys": response.node_sync_keys,
            "has_more": response.has_more,
        }))
    }
}

// ── cognition_memory_list ─────────────────────────────────────────────────────

pub struct CognitionMemoryListTool {
    context_query: Arc<ContextQueryService>,
    memory_reader: Arc<dyn MemoryContextReader>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryListTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            context_query: Arc::new(ContextQueryService::new(locus_store)),
            memory_reader,
            fallback_chat_session_id,
            workshop_dynamic,
            turn_scope,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryListTool {
    fn name(&self) -> &'static str {
        "cognition_memory_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Memory inventory, newest-first. Optional context_keywords filter on context_summary. \
             Optional semantic_tags (indexed, match-all) or tag_prefix. Omit session_id or pass null for global listing.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": ["string", "null"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                "context_keywords": { "type": "array", "items": { "type": "string" } },
                "semantic_tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Indexed Locus tags (match-all)"
                },
                "tag_prefix": {
                    "type": "string",
                    "description": "Match nodes whose indexed tags share this prefix"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(50);
        let limit = validate_limit(limit, "limit").map_err(StasisError::PortFailure)?;

        let global = input.get("session_id").map(|v| v.is_null()).unwrap_or(false);
        let session_id = if global {
            None
        } else {
            resolve_optional_locus_session_scope(
                &input,
                &self.turn_scope,
                &self.fallback_chat_session_id,
                self.workshop_dynamic,
            )
            .await
        };

        let keywords = normalize_context_keywords(
            input
                .get("context_keywords")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .as_deref(),
        );

        emit_invoked(&self.event_tx, self.name(), "list").await;

        let tag_filter = crate::locus_semantic_tags::memory_filter_from_tag_input(&input);
        let has_tag_filters = crate::locus_semantic_tags::input_has_tag_filters(&input);

        if keywords.is_empty() && !has_tag_filters {
            let listed = self
                .context_query
                .list_nodes_async(limit, session_id.as_deref())
                .await
                .map_err(|e| StasisError::PortFailure(format!("cognition_memory_list: {e}")))?;
            return Ok(json!({
                "retrieved": listed.retrieved,
                "nodes": listed.nodes.iter().map(sttp_node_to_json).collect::<Vec<_>>(),
            }));
        }

        let query_limit = limit.saturating_mul(5).clamp(1, 200);
        let mut find = MemoryFindRequest::default();
        find.limit = query_limit;
        find.sort_field = MemorySortField::Timestamp;
        find.sort_direction = MemorySortDirection::Desc;
        find.filter = tag_filter;
        if !keywords.is_empty() {
            find.filter.text_contains = Some(keywords.join(" "));
        }
        if let Some(ref sid) = session_id {
            find.scope.session_ids = Some(vec![sid.clone()]);
            let tenant = crate::locus_memory::derive_locus_tenant_id(sid);
            if tenant != crate::locus_memory::LOCUS_DEFAULT_TENANT {
                find.scope.tenant_id = Some(tenant);
            }
        }

        let found = self
            .memory_reader
            .find(&find)
            .await
            .map_err(|e| StasisError::PortFailure(format!("cognition_memory_list: {e}")))?;

        let nodes = found
            .nodes
            .iter()
            .take(limit)
            .map(memory_node_to_json)
            .collect::<Vec<_>>();

        Ok(json!({
            "retrieved": nodes.len(),
            "nodes": nodes,
            "find_sync_keys": found.node_sync_keys,
            "has_more": found.has_more,
        }))
    }
}

// ── cognition_memory_recall (legacy query → AVEC recall) ──────────────────────

pub struct CognitionMemoryRecallTool {
    context_tool: CognitionMemoryContextTool,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryRecallTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            context_tool: CognitionMemoryContextTool::new(
                locus_store,
                memory_reader,
                fallback_chat_session_id,
                workshop_dynamic,
                turn_scope,
                event_tx.clone(),
            ),
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryRecallTool {
    fn name(&self) -> &'static str {
        "cognition_memory_recall"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Retrieve memory by natural-language keywords (legacy). Prefer cognition_memory_context \
             with explicit AVEC when possible. Optional semantic_tags or tag_prefix for indexed filtering. \
             Pass session_id to scope to one session, or null to search across all sessions.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "session_id": { "type": ["string", "null"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20 },
                "semantic_tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "tag_prefix": { "type": "string" }
            },
            "required": ["query"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let query = input.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure("cognition_memory_recall: query is required".to_string())
        })?;
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5).min(20) as usize;

        emit_invoked(&self.event_tx, self.name(), query).await;

        let (s, f, l, a) = DEFAULT_RECALL_AVEC;
        let session_id = match input.get("session_id") {
            Some(value) if value.is_null() => serde_json::Value::Null,
            Some(value) => value.clone(),
            None => json!(resolve_memory_tool_session_id(
                &input,
                &self.context_tool.turn_scope,
                &self.context_tool.fallback_chat_session_id,
                self.context_tool.workshop_dynamic,
            )
            .await),
        };
        let wrapped = json!({
            "stability": s,
            "friction": f,
            "logic": l,
            "autonomy": a,
            "context_keywords": [query],
            "limit": limit,
            "session_id": session_id,
            "semantic_tags": input.get("semantic_tags").cloned(),
            "tag_prefix": input.get("tag_prefix").cloned(),
        });
        self.context_tool.invoke(wrapped).await
    }
}

// ── cognition_memory_tags ─────────────────────────────────────────────────────

pub struct CognitionMemoryTagsTool {
    semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryTagsTool {
    pub fn new(
        semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            semantic_index,
            fallback_chat_session_id,
            workshop_dynamic,
            turn_scope,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryTagsTool {
    fn name(&self) -> &'static str {
        "cognition_memory_tags"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "List indexed Locus semantic tags for the active profile tenant. Optional prefix narrows \
             vocabulary (e.g. profile:, chat:, medousa). Use before recall/list to pick tag filters.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": ["string", "null"],
                    "description": "Optional session to derive tenant scope"
                },
                "prefix": {
                    "type": "string",
                    "description": "Filter tags by prefix (case-insensitive)"
                },
                "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100)
            .clamp(1, 500) as usize;
        let prefix = crate::locus_semantic_tags::parse_tag_prefix_from_value(input.get("prefix"));

        let session_scope = if input.get("session_id").is_some_and(Value::is_null) {
            None
        } else if input.get("session_id").is_some() {
            Some(
                resolve_memory_tool_session_id(
                    &input,
                    &self.turn_scope,
                    &self.fallback_chat_session_id,
                    self.workshop_dynamic,
                )
                .await,
            )
        } else {
            None
        };
        let tenant =
            crate::locus_semantic_tags::resolve_workshop_tag_tenant_id(session_scope.as_deref());

        emit_invoked(
            &self.event_tx,
            self.name(),
            prefix.as_deref().unwrap_or("all"),
        )
        .await;

        let tags = self
            .semantic_index
            .find_tags_async(&tenant, prefix.as_deref(), limit)
            .await
            .map_err(|err| StasisError::PortFailure(format!("cognition_memory_tags: {err}")))?;

        Ok(json!({
            "tenant_id": tenant,
            "prefix": prefix,
            "tags": tags,
            "count": tags.len(),
            "usage": "Pass tags to cognition_memory_context, cognition_memory_list, or cognition_memory_recall via semantic_tags (match-all).",
        }))
    }
}

// ── cognition_memory_moods ────────────────────────────────────────────────────

pub struct CognitionMemoryMoodsTool {
    moods: MoodCatalogService,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryMoodsTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self {
            moods: MoodCatalogService::new(),
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryMoodsTool {
    fn name(&self) -> &'static str {
        "cognition_memory_moods"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "AVEC mood presets and blend preview. Use before store/retrieve when reasoning posture is unset.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "target_mood": { "type": "string" },
                "blend": { "type": "number" },
                "current_stability": { "type": "number" },
                "current_friction": { "type": "number" },
                "current_logic": { "type": "number" },
                "current_autonomy": { "type": "number" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let target_mood = input.get("target_mood").and_then(|v| v.as_str());
        let blend = input.get("blend").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let current_stability = input.get("current_stability").and_then(|v| v.as_f64()).map(|v| v as f32);
        let current_friction = input.get("current_friction").and_then(|v| v.as_f64()).map(|v| v as f32);
        let current_logic = input.get("current_logic").and_then(|v| v.as_f64()).map(|v| v as f32);
        let current_autonomy = input.get("current_autonomy").and_then(|v| v.as_f64()).map(|v| v as f32);

        emit_invoked(&self.event_tx, self.name(), "moods").await;

        let result = self.moods.get(
            target_mood,
            blend,
            current_stability,
            current_friction,
            current_logic,
            current_autonomy,
        );

        let swap_preview = result.swap_preview.as_ref().map(|preview| {
            json!({
                "target_mood": preview.target_mood,
                "blend": preview.blend,
                "current": avec_to_json(preview.current),
                "target": avec_to_json(preview.target),
                "blended": avec_to_json(preview.blended),
            })
        });

        Ok(json!({
            "presets": result.presets.iter().map(|preset| {
                json!({
                    "name": preset.name,
                    "description": preset.description,
                    "avec": avec_to_json(preset.avec),
                })
            }).collect::<Vec<_>>(),
            "apply_guide": result.apply_guide,
            "swap_preview": swap_preview,
        }))
    }
}

// ── cognition_memory_evict ────────────────────────────────────────────────────

pub struct CognitionMemoryEvictTool {
    operations: Arc<dyn MemoryOperations>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryEvictTool {
    pub fn new(
        operations: Arc<dyn MemoryOperations>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            operations,
            fallback_chat_session_id,
            workshop_dynamic,
            turn_scope,
            event_tx,
        }
    }
}

fn parse_evict_mode(value: Option<&str>) -> MemoryEvictMode {
    match value.unwrap_or("by_filter").trim().to_ascii_lowercase().as_str() {
        "purge_session" => MemoryEvictMode::PurgeSession,
        "by_node_ids" => MemoryEvictMode::ByNodeIds,
        "by_sync_keys" => MemoryEvictMode::BySyncKeys,
        _ => MemoryEvictMode::ByFilter,
    }
}

#[async_trait]
impl StasisTool for CognitionMemoryEvictTool {
    fn name(&self) -> &'static str {
        "cognition_memory_evict"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Evict Locus memory nodes (dry-run by default). Supports by_filter, purge_session, \
             by_node_ids, and by_sync_keys modes.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["by_filter", "purge_session", "by_node_ids", "by_sync_keys"],
                    "description": "Eviction strategy (default: by_filter)"
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Preview deletions without applying (default: true)"
                },
                "force": {
                    "type": "boolean",
                    "description": "Bypass inbound-reference blocks (default: false)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Locus session scope (defaults to current turn session)"
                },
                "tiers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filter tiers for by_filter mode"
                },
                "node_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Node ids for by_node_ids mode"
                },
                "sync_keys": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Sync keys for by_sync_keys mode"
                },
                "max_nodes": {
                    "type": "integer",
                    "description": "Safety cap on nodes touched (default: 5000)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let mode = parse_evict_mode(input.get("mode").and_then(|v| v.as_str()));
        let dry_run = input.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(true);
        let force = input.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
        let max_nodes = input
            .get("max_nodes")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000)
            .clamp(1, 50_000) as usize;

        let locus_session = resolve_memory_tool_session_id(
            &input,
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await;
        let tenant = derive_locus_tenant_id(&locus_session);
        let mut scope = MemoryScope {
            session_ids: Some(vec![locus_session.clone()]),
            ..Default::default()
        };
        if tenant != LOCUS_DEFAULT_TENANT {
            scope.tenant_id = Some(tenant);
        }
        if let Some(tiers) = input.get("tiers").and_then(|v| v.as_array()) {
            scope.tiers = Some(
                tiers
                    .iter()
                    .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
                    .map(str::to_string)
                    .collect(),
            );
        }

        let node_ids = input.get("node_ids").and_then(|v| v.as_array()).map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
        let sync_keys = input.get("sync_keys").and_then(|v| v.as_array()).map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
                .map(str::to_string)
                .collect::<Vec<_>>()
        });

        emit_invoked(
            &self.event_tx,
            self.name(),
            &format!("{mode:?} dry_run={dry_run}"),
        )
        .await;

        let response = self
            .operations
            .evict(&MemoryEvictRequest {
                mode,
                scope,
                filter: MemoryFilter::default(),
                node_ids,
                sync_keys,
                dry_run,
                force,
                max_nodes,
                include_calibration: false,
                include_checkpoints: false,
            })
            .await?;

        Ok(json!({
            "dry_run": response.dry_run,
            "deleted": response.deleted,
            "blocked": response.blocked,
            "not_found": response.not_found,
            "skipped": response.skipped,
            "would_delete": response.would_delete,
            "session_id": locus_session,
        }))
    }
}
