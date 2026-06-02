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
    MemoryAvecState, MemoryFallbackPolicy, MemoryFindRequest, MemorySortDirection,
    MemorySortField, MemoryStrictnessMode,
};
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::locus_memory::{
    CANONICAL_STTP_SCHEMA_EXAMPLE, filter_nodes_by_context_keywords,
    ingest_profile_name, normalize_context_keywords, normalize_tiers, resolve_locus_ingest_profile,
    schema_first_guidance, sttp_node_to_json, store_failure_payload, validate_limit, avec_to_json,
};

const DEFAULT_RECALL_AVEC: (f32, f32, f32, f32) = (0.82, 0.31, 0.88, 0.74);

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
            "workflow": [
                "call cognition_memory_schema",
                "optionally cognition_memory_calibrate and cognition_memory_moods",
                "cognition_memory_store with full STTP node string",
                "cognition_memory_context for AVEC-ranked retrieval"
            ],
            "model_guidance": schema_first_guidance(
                "Build a complete four-layer STTP node before store.",
                ingest_profile_name(profile),
            ),
        }))
    }
}

// ── cognition_memory_store ────────────────────────────────────────────────────

pub struct CognitionMemoryStoreTool {
    writer: Arc<dyn MemoryContextWriter>,
    profile_name: &'static str,
    default_session_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryStoreTool {
    pub fn new(
        writer: Arc<dyn MemoryContextWriter>,
        default_session_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        let profile_name = ingest_profile_name(resolve_locus_ingest_profile());
        Self {
            writer,
            profile_name,
            default_session_id,
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

        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(self.default_session_id.as_str())
            .to_string();

        emit_invoked(&self.event_tx, self.name(), &session_id).await;

        let response = self
            .writer
            .store_context(&MemoryStoreRequest {
                session_id,
                raw_node: node.to_string(),
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
    default_session_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryCalibrateTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        default_session_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            calibration: Arc::new(CalibrationService::new(locus_store)),
            default_session_id,
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
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(self.default_session_id.as_str())
            .to_string();
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
    default_session_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryContextTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        default_session_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            context_query: Arc::new(ContextQueryService::new(locus_store)),
            memory_reader,
            default_session_id,
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
             context_keywords; set session_id to null for global retrieval across sessions.",
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
            .map_err(|e| StasisError::PortFailure(e))?;

        let global = input.get("session_id").map(|v| v.is_null()).unwrap_or(false);
        let session_scope = if global {
            None
        } else {
            input
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| Some(self.default_session_id.clone()))
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

        if keywords.is_empty() {
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
        let query_text = keywords.join(" ");

        let recall = MemoryRecallRequest {
            scope: MemoryScope {
                session_ids: session_scope.map(|s| vec![s.to_string()]),
                tiers: tiers_ref.map(|t| t.to_vec()),
                from_utc,
                to_utc,
            },
            current_avec: Some(MemoryAvecState {
                stability,
                friction,
                logic,
                autonomy,
            }),
            query_text: Some(query_text),
            limit,
            alpha,
            beta,
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
    default_session_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryListTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        default_session_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            context_query: Arc::new(ContextQueryService::new(locus_store)),
            memory_reader,
            default_session_id,
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
             Omit session_id or pass null for global listing.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": ["string", "null"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                "context_keywords": { "type": "array", "items": { "type": "string" } }
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
            input
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| Some(self.default_session_id.clone()))
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

        if keywords.is_empty() {
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
        find.filter.text_contains = Some(keywords.join(" "));
        if let Some(ref sid) = session_id {
            find.scope.session_ids = Some(vec![sid.clone()]);
        }

        let found = self
            .memory_reader
            .find(&find)
            .await
            .map_err(|e| StasisError::PortFailure(format!("cognition_memory_list: {e}")))?;

        let nodes = self
            .context_query
            .list_nodes_async(query_limit, session_id.as_deref())
            .await
            .map(|r| r.nodes)
            .unwrap_or_default();
        let filtered = filter_nodes_by_context_keywords(&nodes, &keywords, limit);

        Ok(json!({
            "retrieved": filtered.len(),
            "nodes": filtered.iter().map(sttp_node_to_json).collect::<Vec<_>>(),
            "find_sync_keys": found.node_sync_keys,
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
        default_session_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            context_tool: CognitionMemoryContextTool::new(
                locus_store,
                memory_reader,
                default_session_id,
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
             with explicit AVEC when possible. Maps query to context_keywords with default AVEC posture.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
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
        let wrapped = json!({
            "stability": s,
            "friction": f,
            "logic": l,
            "autonomy": a,
            "context_keywords": [query],
            "limit": limit,
        });
        self.context_tool.invoke(wrapped).await
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
