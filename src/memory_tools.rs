//! Locus STTP memory tools (same capabilities as the Locus store; Medousa cognition_* naming).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use locus_core_rs::{CalibrationService, ContextQueryService, MoodCatalogService, NodeStore};
use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::domain::errors::StasisError;
use stasis::memory_prelude::{MemoryRecallRequest, MemoryScope, MemoryStoreRequest};
use stasis::memory_prelude_ext::MemoryContextReader;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use stasis::ports::outbound::memory::memory_models::{
    MemoryAvecState, MemoryEvictMode, MemoryEvictRequest, MemoryFallbackPolicy, MemoryFilter,
    MemoryFindRequest, MemoryNode, MemorySortDirection, MemorySortField, MemoryStrictnessMode,
};
use stasis::ports::outbound::memory::memory_operations::MemoryOperations;
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::locus_memory::{
    CANONICAL_STTP_SCHEMA_EXAMPLE, LOCUS_DEFAULT_TENANT, derive_locus_tenant_id,
    infer_store_error_code, ingest_profile_name, normalize_context_keywords, normalize_tiers,
    resolve_locus_ingest_profile, resolve_memory_tool_session_id_typed, store_failure_guidance,
    typed_schema_first_guidance, typed_semantic_index_schema_guidance, validate_limit,
};
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::typed_tools::{CompatList, CompatOption, ToolId, medousa_tool};

const COGNITION_MEMORY_SCHEMA_ID: ToolId = ToolId::new("cognition_memory_schema");
const COGNITION_MEMORY_STORE_ID: ToolId = ToolId::new("cognition_memory_store");
const COGNITION_MEMORY_CALIBRATE_ID: ToolId = ToolId::new("cognition_memory_calibrate");
const COGNITION_MEMORY_CONTEXT_ID: ToolId = ToolId::new("cognition_memory_context");
const COGNITION_MEMORY_LIST_ID: ToolId = ToolId::new("cognition_memory_list");
const COGNITION_MEMORY_RECALL_ID: ToolId = ToolId::new("cognition_memory_recall");
const COGNITION_MEMORY_TAGS_ID: ToolId = ToolId::new("cognition_memory_tags");
const COGNITION_MEMORY_MOODS_ID: ToolId = ToolId::new("cognition_memory_moods");
const COGNITION_MEMORY_EVICT_ID: ToolId = ToolId::new("cognition_memory_evict");

const DEFAULT_RECALL_AVEC: (f32, f32, f32, f32) = (0.82, 0.31, 0.88, 0.74);

#[derive(Debug, Default)]
pub(crate) enum MemorySessionScopeInput {
    #[default]
    Missing,
    Global,
    Explicit(String),
    Invalid,
}

impl MemorySessionScopeInput {
    fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    fn is_global(&self) -> bool {
        matches!(self, Self::Global)
    }

    fn was_present(&self) -> bool {
        !matches!(self, Self::Missing)
    }

    fn explicit(&self) -> Option<&str> {
        match self {
            Self::Explicit(value) => Some(value),
            Self::Missing | Self::Global | Self::Invalid => None,
        }
    }
}

impl<'de> Deserialize<'de> for MemorySessionScopeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(match value {
            Value::Null => Self::Global,
            Value::String(value) => Self::Explicit(value),
            _ => Self::Invalid,
        })
    }
}

impl JsonSchema for MemorySessionScopeInput {
    fn schema_name() -> String {
        "MemorySessionScopeInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        serde_json::from_value(json!({ "type": ["string", "null"] }))
            .expect("valid nullable memory session schema")
    }
}

#[derive(Debug, Default)]
pub(crate) struct CompatibleSemanticTags(Option<Vec<String>>);

impl CompatibleSemanticTags {
    fn as_deref(&self) -> Option<&[String]> {
        self.0.as_deref()
    }

    pub(crate) fn from_vec(tags: Option<Vec<String>>) -> Option<Self> {
        tags.map(|tags| Self(Some(tags)))
    }
}

impl<'de> Deserialize<'de> for CompatibleSemanticTags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let tags = if let Some(items) = value.as_array() {
            crate::locus_semantic_tags::normalize_semantic_tags(
                items.iter().filter_map(Value::as_str),
            )
        } else if let Some(raw) = value.as_str() {
            crate::locus_semantic_tags::normalize_semantic_tags(raw.split(',').map(str::trim))
        } else {
            Vec::new()
        };
        Ok(Self((!tags.is_empty()).then_some(tags)))
    }
}

impl JsonSchema for CompatibleSemanticTags {
    fn schema_name() -> String {
        "CompatibleSemanticTags".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Vec::<String>::json_schema(generator)
    }
}

async fn resolve_optional_locus_session_scope(
    session: &MemorySessionScopeInput,
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    fallback_chat_session_id: &str,
    workshop_dynamic: bool,
) -> stasis::prelude::Result<Option<String>> {
    if session.is_global() {
        return Ok(None);
    }
    Ok(Some(
        resolve_memory_tool_session_id_typed(
            session.explicit(),
            turn_scope,
            fallback_chat_session_id,
            workshop_dynamic,
        )
        .await?,
    ))
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

fn optional_nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn memory_filter(
    semantic_tags: Option<&CompatibleSemanticTags>,
    tag_prefix: Option<&str>,
) -> MemoryFilter {
    let mut filter = MemoryFilter::default();
    filter.indexed_tags = semantic_tags
        .and_then(CompatibleSemanticTags::as_deref)
        .map(<[String]>::to_vec);
    filter.tag_prefix = tag_prefix
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    filter
}

fn has_tag_filters(filter: &MemoryFilter) -> bool {
    filter.indexed_tags.is_some() || filter.tag_prefix.is_some()
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryAvecOutput {
    stability: f32,
    friction: f32,
    logic: f32,
    autonomy: f32,
    psi: f32,
}

impl MemoryAvecOutput {
    fn from_locus(value: locus_core_rs::AvecState) -> Self {
        Self {
            stability: value.stability,
            friction: value.friction,
            logic: value.logic,
            autonomy: value.autonomy,
            psi: value.psi(),
        }
    }

    fn from_stasis(value: MemoryAvecState) -> Self {
        Self {
            stability: value.stability,
            friction: value.friction,
            logic: value.logic,
            autonomy: value.autonomy,
            psi: value.stability + value.friction + value.logic + value.autonomy,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SttpNodeOutput {
    raw: String,
    session_id: String,
    tier: String,
    timestamp: String,
    context_summary: Option<String>,
    semantic_tags: Option<Vec<String>>,
    psi: f32,
    rho: f32,
    kappa: f32,
    sync_key: String,
    user_avec: MemoryAvecOutput,
    model_avec: MemoryAvecOutput,
}

impl From<&locus_core_rs::SttpNode> for SttpNodeOutput {
    fn from(value: &locus_core_rs::SttpNode) -> Self {
        Self {
            raw: value.raw.clone(),
            session_id: value.session_id.clone(),
            tier: value.tier.clone(),
            timestamp: value.timestamp.to_rfc3339(),
            context_summary: value.context_summary.clone(),
            semantic_tags: value.semantic_tags.clone(),
            psi: value.psi,
            rho: value.rho,
            kappa: value.kappa,
            sync_key: value.sync_key.clone(),
            user_avec: MemoryAvecOutput::from_locus(value.user_avec),
            model_avec: MemoryAvecOutput::from_locus(value.model_avec),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryNodeOutput {
    raw: String,
    session_id: String,
    tier: String,
    timestamp: String,
    context_summary: Option<String>,
    compression_depth: i32,
    parent_node_id: Option<String>,
    sync_key: String,
    semantic_tags: Option<Vec<String>>,
    psi: f32,
    rho: f32,
    kappa: f32,
    user_avec: MemoryAvecOutput,
    model_avec: MemoryAvecOutput,
    compression_avec: Option<MemoryAvecOutput>,
    updated_at: String,
}

impl From<&MemoryNode> for MemoryNodeOutput {
    fn from(value: &MemoryNode) -> Self {
        Self {
            raw: value.raw.clone(),
            session_id: value.session_id.clone(),
            tier: value.tier.clone(),
            timestamp: value.timestamp.to_rfc3339(),
            context_summary: value.context_summary.clone(),
            compression_depth: value.compression_depth,
            parent_node_id: value.parent_node_id.clone(),
            sync_key: value.sync_key.clone(),
            semantic_tags: value.semantic_tags.clone(),
            psi: value.psi,
            rho: value.rho,
            kappa: value.kappa,
            user_avec: MemoryAvecOutput::from_stasis(value.user_avec),
            model_avec: MemoryAvecOutput::from_stasis(value.model_avec),
            compression_avec: value.compression_avec.map(MemoryAvecOutput::from_stasis),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemorySchemaInput {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemorySchemaOutput {
    pub canonical_example: String,
    pub ingest_profile_policy: String,
    pub semantic_index: crate::locus_memory::SemanticIndexSchemaGuidance,
    pub workflow: Vec<String>,
    pub model_guidance: crate::locus_memory::SchemaFirstGuidance,
}

#[medousa_tool(id = COGNITION_MEMORY_SCHEMA_ID)]
impl CognitionMemorySchemaTool {
    /// Return a canonical STTP node example and the active ingest profile before storing memory.
    pub(crate) async fn invoke_typed(
        &self,
        _input: MemorySchemaInput,
    ) -> stasis::prelude::Result<MemorySchemaOutput> {
        let profile = resolve_locus_ingest_profile();
        Ok(MemorySchemaOutput {
            canonical_example: CANONICAL_STTP_SCHEMA_EXAMPLE.to_string(),
            ingest_profile_policy: ingest_profile_name(profile).to_string(),
            semantic_index: typed_semantic_index_schema_guidance(),
            workflow: [
                "call cognition_memory_query action=memory.schema",
                "optionally cognition_memory_mutate action=memory.calibrate and cognition_memory_query action=memory.moods",
                "cognition_memory_mutate action=memory.store with full STTP node string — put semantic_tags in provenance.prime (or pass semantic_tags on memory.store to merge workshop tags)",
                "optional provenance.semantic_links for typed cross-node relations",
                "cognition_memory_query action=memory.context or action=memory.list with semantic_tags for indexed recall",
                "cognition_memory_query action=memory.tags to browse the tag vocabulary",
                "cognition_memory_query action=memory.context for AVEC-ranked retrieval",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            model_guidance: typed_schema_first_guidance(
                "Build a complete four-layer STTP node before store; include semantic_tags in prime when you want indexed recall.",
                ingest_profile_name(profile),
            ),
        })
    }
}

// ── cognition_memory_store ────────────────────────────────────────────────────

pub struct CognitionMemoryStoreTool {
    writer: Arc<dyn MemoryContextWriter>,
    profile_name: &'static str,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryStoreTool {
    pub fn new(
        writer: Arc<dyn MemoryContextWriter>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug, JsonSchema)]
pub struct MemoryStoreInput {
    /// Full STTP node payload with ⊕ ⦿ ◈ ⍉ layers
    #[schemars(required, with = "String")]
    pub(crate) node: Option<String>,
    /// Locus session id (defaults to current turn session)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    /// Optional Locus semantic tags merged into the STTP prime block
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub(crate) semantic_tags: Option<Vec<String>>,
    /// Deprecated: use `node` with full STTP instead
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,
    #[schemars(skip)]
    pub(crate) vibe_signature: Option<String>,
}

impl<'de> Deserialize<'de> for MemoryStoreInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            node: CompatOption<String>,
            #[serde(default)]
            session_id: CompatOption<String>,
            #[serde(default)]
            semantic_tags: CompatList<String>,
            #[serde(default)]
            content: CompatOption<String>,
            #[serde(default)]
            vibe_signature: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            node: input.node.into_option(),
            session_id: input.session_id.into_option(),
            semantic_tags: input.semantic_tags.into_option(),
            content: input.content.into_option(),
            vibe_signature: input.vibe_signature.into_option(),
        })
    }
}

#[derive(Debug)]
struct MemoryStoreCommand {
    node: RequiredContent,
    session_id: Option<TrimmedText>,
    semantic_tags: Vec<TrimmedText>,
    vibe_signature: Option<TrimmedText>,
}

impl TryFrom<MemoryStoreInput> for MemoryStoreCommand {
    type Error = StasisError;

    fn try_from(input: MemoryStoreInput) -> Result<Self, Self::Error> {
        let node = input
            .node
            .and_then(|value| RequiredContent::new(value).ok())
            .or_else(|| {
                input
                    .content
                    .and_then(|value| RequiredContent::new(value).ok())
            })
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_memory_mutate action=memory.store: `node` (full STTP string) is required. \
                     Call cognition_memory_query action=memory.schema first."
                        .to_string(),
                )
            })?;
        let semantic_tags = input
            .semantic_tags
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| TrimmedText::new(value).ok())
            .collect();

        Ok(Self {
            node,
            session_id: input
                .session_id
                .and_then(|value| TrimmedText::new(value).ok()),
            semantic_tags,
            vibe_signature: input
                .vibe_signature
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryStoreErrorOutput {
    code: String,
    message: String,
    model_guidance: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum MemoryStoreOutput {
    Stored {
        node_id: String,
        psi: f32,
        valid: bool,
        stored: bool,
        validation_error: Option<String>,
        profile_policy: String,
    },
    Rejected {
        node_id: String,
        psi: f32,
        valid: bool,
        stored: bool,
        validation_error: String,
        profile_policy: String,
        error: MemoryStoreErrorOutput,
    },
}

#[medousa_tool(id = COGNITION_MEMORY_STORE_ID)]
impl CognitionMemoryStoreTool {
    /// Store a complete STTP node in Locus memory. Requires `node` (full STTP string). Optional `session_id` defaults to the current turn session.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryStoreInput,
    ) -> stasis::prelude::Result<MemoryStoreOutput> {
        let command = MemoryStoreCommand::try_from(input)?;
        let node = command.node.into_string();

        let session_id = resolve_memory_tool_session_id_typed(
            command.session_id.as_ref().map(TrimmedText::as_str),
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await?;

        emit_invoked(
            &self.event_tx,
            COGNITION_MEMORY_STORE_ID.as_str(),
            &session_id,
        )
        .await;

        let vibe_signature = command
            .vibe_signature
            .map(TrimmedText::into_string)
            .unwrap_or_else(|| {
                crate::agent_runtime::derive_vibe_signature(
                    &session_id,
                    None,
                    None,
                    &crate::agent_runtime::default_handoff_model_avec(),
                )
            });
        let mut tags = crate::locus_semantic_tags::default_workshop_semantic_tags(&session_id);
        tags.extend(
            command
                .semantic_tags
                .into_iter()
                .map(TrimmedText::into_string),
        );
        let tagged_node = crate::locus_semantic_tags::inject_semantic_tags(&node, &tags);
        let raw_node = crate::locus_memory::enrich_sttp_node_with_vibe_signature(
            &tagged_node,
            &vibe_signature,
        );

        let response = self
            .writer
            .store_context(&MemoryStoreRequest {
                session_id,
                raw_node,
            })
            .await?;

        if response.valid {
            Ok(MemoryStoreOutput::Stored {
                node_id: response.node_id,
                psi: response.psi,
                valid: true,
                stored: true,
                validation_error: response.validation_error,
                profile_policy: self.profile_name.to_string(),
            })
        } else {
            let message = response
                .validation_error
                .unwrap_or_else(|| "store rejected context".to_string());
            Ok(MemoryStoreOutput::Rejected {
                node_id: response.node_id,
                psi: response.psi,
                valid: false,
                stored: false,
                validation_error: message.clone(),
                profile_policy: self.profile_name.to_string(),
                error: MemoryStoreErrorOutput {
                    code: infer_store_error_code(&message).to_string(),
                    message: message.clone(),
                    model_guidance: store_failure_guidance(&message, self.profile_name),
                },
            })
        }
    }
}

// ── cognition_memory_calibrate ────────────────────────────────────────────────

pub struct CognitionMemoryCalibrateTool {
    calibration: Arc<CalibrationService>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryCalibrateTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug, JsonSchema)]
pub struct MemoryCalibrateInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    #[schemars(required, with = "f64")]
    pub(crate) stability: Option<f64>,
    #[schemars(required, with = "f64")]
    pub(crate) friction: Option<f64>,
    #[schemars(required, with = "f64")]
    pub(crate) logic: Option<f64>,
    #[schemars(required, with = "f64")]
    pub(crate) autonomy: Option<f64>,
    /// e.g. manual, session_start
    #[schemars(required, with = "String")]
    pub(crate) trigger: Option<String>,
}

impl<'de> Deserialize<'de> for MemoryCalibrateInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            session_id: CompatOption<String>,
            #[serde(default)]
            stability: CompatOption<f64>,
            #[serde(default)]
            friction: CompatOption<f64>,
            #[serde(default)]
            logic: CompatOption<f64>,
            #[serde(default)]
            autonomy: CompatOption<f64>,
            #[serde(default)]
            trigger: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            session_id: input.session_id.into_option(),
            stability: input.stability.into_option(),
            friction: input.friction.into_option(),
            logic: input.logic.into_option(),
            autonomy: input.autonomy.into_option(),
            trigger: input.trigger.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryCalibrateOutput {
    previous_avec: MemoryAvecOutput,
    delta: f32,
    drift_classification: String,
    trigger: String,
    trigger_history: Vec<String>,
    is_first_calibration: bool,
}

#[medousa_tool(id = COGNITION_MEMORY_CALIBRATE_ID)]
impl CognitionMemoryCalibrateTool {
    /// Measure AVEC drift for a session. Call at session start and after heavy reasoning before store/retrieve.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryCalibrateInput,
    ) -> stasis::prelude::Result<MemoryCalibrateOutput> {
        let session_id = resolve_memory_tool_session_id_typed(
            input.session_id.as_deref(),
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await?;
        let stability = input
            .stability
            .ok_or_else(|| StasisError::PortFailure("stability required".into()))?
            as f32;
        let friction = input
            .friction
            .ok_or_else(|| StasisError::PortFailure("friction required".into()))?
            as f32;
        let logic = input
            .logic
            .ok_or_else(|| StasisError::PortFailure("logic required".into()))?
            as f32;
        let autonomy = input
            .autonomy
            .ok_or_else(|| StasisError::PortFailure("autonomy required".into()))?
            as f32;
        let trigger = input.trigger.as_deref().unwrap_or("manual");

        emit_invoked(
            &self.event_tx,
            COGNITION_MEMORY_CALIBRATE_ID.as_str(),
            &session_id,
        )
        .await;

        let result = self
            .calibration
            .calibrate_async(&session_id, stability, friction, logic, autonomy, trigger)
            .await
            .map_err(|e| {
                StasisError::PortFailure(format!(
                    "cognition_memory_mutate action=memory.calibrate: {e}"
                ))
            })?;

        Ok(MemoryCalibrateOutput {
            previous_avec: MemoryAvecOutput::from_locus(result.previous_avec),
            delta: result.delta,
            drift_classification: format!("{:?}", result.drift_classification),
            trigger: result.trigger,
            trigger_history: result.trigger_history,
            is_first_calibration: result.is_first_calibration,
        })
    }
}

// ── cognition_memory_context ────────────────────────────────────────────────────

pub struct CognitionMemoryContextTool {
    context_query: Arc<ContextQueryService>,
    memory_reader: Arc<dyn MemoryContextReader>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryContextTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug, JsonSchema)]
pub struct MemoryContextInput {
    #[serde(default, skip_serializing_if = "MemorySessionScopeInput::is_missing")]
    #[schemars(
        with = "MemorySessionScopeInput",
        skip_serializing_if = "MemorySessionScopeInput::is_missing"
    )]
    pub(crate) session_id: MemorySessionScopeInput,
    #[schemars(required, with = "f64")]
    pub(crate) stability: Option<f64>,
    #[schemars(required, with = "f64")]
    pub(crate) friction: Option<f64>,
    #[schemars(required, with = "f64")]
    pub(crate) logic: Option<f64>,
    #[schemars(required, with = "f64")]
    pub(crate) autonomy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub(crate) context_keywords: Option<Vec<String>>,
    /// Indexed Locus tags (match-all). Example: ["session", "profile:work"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CompatibleSemanticTags",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) semantic_tags: Option<CompatibleSemanticTags>,
    /// Match nodes whose indexed tags share this prefix (e.g. profile:)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) tag_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64", skip_serializing_if = "Option::is_none")]
    pub(crate) alpha: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64", skip_serializing_if = "Option::is_none")]
    pub(crate) beta: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) from_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) to_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub(crate) tiers: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for MemoryContextInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            session_id: MemorySessionScopeInput,
            #[serde(default)]
            stability: CompatOption<f64>,
            #[serde(default)]
            friction: CompatOption<f64>,
            #[serde(default)]
            logic: CompatOption<f64>,
            #[serde(default)]
            autonomy: CompatOption<f64>,
            #[serde(default)]
            context_keywords: CompatList<String>,
            #[serde(default)]
            semantic_tags: Option<CompatibleSemanticTags>,
            #[serde(default)]
            tag_prefix: CompatOption<String>,
            #[serde(default)]
            limit: CompatOption<usize>,
            #[serde(default)]
            alpha: CompatOption<f64>,
            #[serde(default)]
            beta: CompatOption<f64>,
            #[serde(default)]
            from_utc: CompatOption<String>,
            #[serde(default)]
            to_utc: CompatOption<String>,
            #[serde(default)]
            tiers: CompatList<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            session_id: input.session_id,
            stability: input.stability.into_option(),
            friction: input.friction.into_option(),
            logic: input.logic.into_option(),
            autonomy: input.autonomy.into_option(),
            context_keywords: input.context_keywords.into_option(),
            semantic_tags: input.semantic_tags,
            tag_prefix: input.tag_prefix.into_option(),
            limit: input.limit.into_option(),
            alpha: input.alpha.into_option(),
            beta: input.beta.into_option(),
            from_utc: input.from_utc.into_option(),
            to_utc: input.to_utc.into_option(),
            tiers: input.tiers.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryPsiRangeOutput {
    min: f32,
    max: f32,
    average: f32,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum MemoryContextOutput {
    Retrieved {
        retrieved: usize,
        psi_range: MemoryPsiRangeOutput,
        nodes: Vec<SttpNodeOutput>,
    },
    Recalled {
        retrieved: usize,
        nodes: Vec<MemoryNodeOutput>,
        retrieval_path: Option<String>,
        fallback_triggered: bool,
        fallback_reason: Option<String>,
        node_sync_keys: Vec<String>,
        has_more: bool,
    },
}

#[medousa_tool(id = COGNITION_MEMORY_CONTEXT_ID)]
impl CognitionMemoryContextTool {
    /// Primary memory retrieval by AVEC resonance. Requires stability/friction/logic/autonomy. Optional context_keywords and semantic_tags (indexed, match-all). Use tag_prefix for prefix vocabulary search. Set session_id to null for global retrieval across sessions.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryContextInput,
    ) -> stasis::prelude::Result<MemoryContextOutput> {
        let stability = input
            .stability
            .ok_or_else(|| StasisError::PortFailure("stability required".into()))?
            as f32;
        let friction = input
            .friction
            .ok_or_else(|| StasisError::PortFailure("friction required".into()))?
            as f32;
        let logic = input
            .logic
            .ok_or_else(|| StasisError::PortFailure("logic required".into()))?
            as f32;
        let autonomy = input
            .autonomy
            .ok_or_else(|| StasisError::PortFailure("autonomy required".into()))?
            as f32;

        let limit = input.limit.unwrap_or(5);
        let limit = validate_limit(limit, "limit").map_err(StasisError::PortFailure)?;

        let session_scope = resolve_optional_locus_session_scope(
            &input.session_id,
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await?;
        let session_scope = session_scope.as_deref();

        let keywords = normalize_context_keywords(input.context_keywords.as_deref());

        let from_utc = parse_utc_optional(input.from_utc.as_deref(), "from_utc")
            .map_err(StasisError::PortFailure)?;
        let to_utc = parse_utc_optional(input.to_utc.as_deref(), "to_utc")
            .map_err(StasisError::PortFailure)?;
        let tiers_norm = normalize_tiers(input.tiers.as_deref().unwrap_or_default());
        let tiers_ref = if tiers_norm.is_empty() {
            None
        } else {
            Some(tiers_norm.as_slice())
        };

        emit_invoked(
            &self.event_tx,
            COGNITION_MEMORY_CONTEXT_ID.as_str(),
            "context",
        )
        .await;

        let tag_filter = memory_filter(input.semantic_tags.as_ref(), input.tag_prefix.as_deref());
        let has_tag_filters = has_tag_filters(&tag_filter);

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
            return Ok(MemoryContextOutput::Retrieved {
                retrieved: result.retrieved,
                psi_range: MemoryPsiRangeOutput {
                    min: result.psi_range.min,
                    max: result.psi_range.max,
                    average: result.psi_range.average,
                },
                nodes: result.nodes.iter().map(Into::into).collect(),
            });
        }

        let alpha = input.alpha.unwrap_or(0.7) as f32;
        let beta = input.beta.unwrap_or(0.3) as f32;
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

        let response = self.memory_reader.recall(&recall).await.map_err(|e| {
            StasisError::PortFailure(format!("cognition_memory_query action=memory.context: {e}"))
        })?;

        Ok(MemoryContextOutput::Recalled {
            retrieved: response.retrieved,
            nodes: response.nodes.iter().map(Into::into).collect(),
            retrieval_path: response.retrieval_path,
            fallback_triggered: response.fallback_triggered,
            fallback_reason: response.fallback_reason,
            node_sync_keys: response.node_sync_keys,
            has_more: response.has_more,
        })
    }
}

// ── cognition_memory_list ─────────────────────────────────────────────────────

pub struct CognitionMemoryListTool {
    context_query: Arc<ContextQueryService>,
    memory_reader: Arc<dyn MemoryContextReader>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryListTool {
    pub fn new(
        locus_store: Arc<dyn NodeStore>,
        memory_reader: Arc<dyn MemoryContextReader>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryListInput {
    #[serde(default)]
    #[schemars(
        with = "MemorySessionScopeInput",
        skip_serializing_if = "MemorySessionScopeInput::is_missing"
    )]
    pub(crate) session_id: MemorySessionScopeInput,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) limit: CompatOption<usize>,
    #[serde(default)]
    #[schemars(
        with = "Vec<String>",
        skip_serializing_if = "crate::typed_tools::CompatList::is_none"
    )]
    pub(crate) context_keywords: CompatList<String>,
    /// Indexed Locus tags (match-all)
    #[serde(default)]
    #[schemars(
        with = "CompatibleSemanticTags",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) semantic_tags: Option<CompatibleSemanticTags>,
    /// Match nodes whose indexed tags share this prefix
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) tag_prefix: CompatOption<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum MemoryListOutput {
    Listed {
        retrieved: usize,
        nodes: Vec<SttpNodeOutput>,
    },
    Found {
        retrieved: usize,
        nodes: Vec<MemoryNodeOutput>,
        find_sync_keys: Vec<String>,
        has_more: bool,
    },
}

#[medousa_tool(id = COGNITION_MEMORY_LIST_ID)]
impl CognitionMemoryListTool {
    /// Memory inventory, newest-first. Optional context_keywords filter on context_summary. Optional semantic_tags (indexed, match-all) or tag_prefix. Omit session_id or pass null for global listing.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryListInput,
    ) -> stasis::prelude::Result<MemoryListOutput> {
        let limit = input.limit.into_option().unwrap_or(50);
        let limit = validate_limit(limit, "limit").map_err(StasisError::PortFailure)?;

        let session_id = resolve_optional_locus_session_scope(
            &input.session_id,
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await?;

        let context_keywords = input.context_keywords.into_option();
        let tag_prefix = input.tag_prefix.into_option();
        let keywords = normalize_context_keywords(context_keywords.as_deref());

        emit_invoked(&self.event_tx, COGNITION_MEMORY_LIST_ID.as_str(), "list").await;

        let tag_filter = memory_filter(input.semantic_tags.as_ref(), tag_prefix.as_deref());
        let has_tag_filters = has_tag_filters(&tag_filter);

        if keywords.is_empty() && !has_tag_filters {
            let listed = self
                .context_query
                .list_nodes_async(limit, session_id.as_deref())
                .await
                .map_err(|e| {
                    StasisError::PortFailure(format!(
                        "cognition_memory_query action=memory.list: {e}"
                    ))
                })?;
            return Ok(MemoryListOutput::Listed {
                retrieved: listed.retrieved,
                nodes: listed.nodes.iter().map(Into::into).collect(),
            });
        }

        let query_limit = limit.saturating_mul(5).clamp(1, 200);
        let mut find = MemoryFindRequest {
            limit: query_limit,
            sort_field: MemorySortField::Timestamp,
            sort_direction: MemorySortDirection::Desc,
            filter: tag_filter,
            ..Default::default()
        };
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

        let found = self.memory_reader.find(&find).await.map_err(|e| {
            StasisError::PortFailure(format!("cognition_memory_query action=memory.list: {e}"))
        })?;

        let nodes = found
            .nodes
            .iter()
            .take(limit)
            .map(Into::into)
            .collect::<Vec<_>>();

        Ok(MemoryListOutput::Found {
            retrieved: nodes.len(),
            nodes,
            find_sync_keys: found.node_sync_keys,
            has_more: found.has_more,
        })
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
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug, JsonSchema)]
pub struct MemoryRecallInput {
    #[schemars(required, with = "String")]
    pub(crate) query: Option<String>,
    #[serde(default, skip_serializing_if = "MemorySessionScopeInput::is_missing")]
    #[schemars(
        with = "MemorySessionScopeInput",
        skip_serializing_if = "MemorySessionScopeInput::is_missing"
    )]
    pub(crate) session_id: MemorySessionScopeInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1, max = 20),
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CompatibleSemanticTags",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) semantic_tags: Option<CompatibleSemanticTags>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) tag_prefix: Option<String>,
}

impl<'de> Deserialize<'de> for MemoryRecallInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            query: CompatOption<String>,
            #[serde(default)]
            session_id: MemorySessionScopeInput,
            #[serde(default)]
            limit: CompatOption<usize>,
            #[serde(default)]
            semantic_tags: Option<CompatibleSemanticTags>,
            #[serde(default)]
            tag_prefix: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            query: input.query.into_option(),
            session_id: input.session_id,
            limit: input.limit.into_option(),
            semantic_tags: input.semantic_tags,
            tag_prefix: input.tag_prefix.into_option(),
        })
    }
}

#[derive(Debug)]
struct MemoryRecallCommand {
    query: TrimmedText,
    session_id: MemorySessionScopeInput,
    limit: usize,
    semantic_tags: Option<CompatibleSemanticTags>,
    tag_prefix: Option<TrimmedText>,
}

impl TryFrom<MemoryRecallInput> for MemoryRecallCommand {
    type Error = StasisError;

    fn try_from(input: MemoryRecallInput) -> Result<Self, Self::Error> {
        let query = input.query.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_memory_query action=memory.recall: query is required".to_string(),
            )
        })?;
        Ok(Self {
            query: TrimmedText::new(query).map_err(|_| {
                StasisError::PortFailure(
                    "cognition_memory_query action=memory.recall: query is required".to_string(),
                )
            })?,
            session_id: input.session_id,
            limit: input.limit.unwrap_or(5).min(20),
            semantic_tags: input.semantic_tags,
            tag_prefix: input
                .tag_prefix
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[medousa_tool(id = COGNITION_MEMORY_RECALL_ID)]
impl CognitionMemoryRecallTool {
    /// Retrieve memory by natural-language keywords (legacy). Prefer cognition_memory_context with explicit AVEC when possible. Optional semantic_tags or tag_prefix for indexed filtering. Pass session_id to scope to one session, or null to search across all sessions.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryRecallInput,
    ) -> stasis::prelude::Result<MemoryContextOutput> {
        let command = MemoryRecallCommand::try_from(input)?;
        let query = command.query.into_string();
        let limit = command.limit;

        emit_invoked(&self.event_tx, COGNITION_MEMORY_RECALL_ID.as_str(), &query).await;

        let (s, f, l, a) = DEFAULT_RECALL_AVEC;
        self.context_tool
            .invoke_typed(MemoryContextInput {
                session_id: command.session_id,
                stability: Some(s as f64),
                friction: Some(f as f64),
                logic: Some(l as f64),
                autonomy: Some(a as f64),
                context_keywords: Some(vec![query]),
                semantic_tags: command.semantic_tags,
                tag_prefix: command.tag_prefix.map(TrimmedText::into_string),
                limit: Some(limit),
                alpha: None,
                beta: None,
                from_utc: None,
                to_utc: None,
                tiers: None,
            })
            .await
    }
}

// ── cognition_memory_tags ─────────────────────────────────────────────────────

pub struct CognitionMemoryTagsTool {
    semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryTagsTool {
    pub fn new(
        semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryTagsInput {
    /// Optional session to derive tenant scope
    #[serde(default)]
    #[schemars(
        with = "MemorySessionScopeInput",
        skip_serializing_if = "MemorySessionScopeInput::is_missing"
    )]
    pub(crate) session_id: MemorySessionScopeInput,
    /// Filter tags by prefix (case-insensitive)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) prefix: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 500),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) limit: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryTagsOutput {
    tenant_id: String,
    prefix: Option<String>,
    tags: Vec<String>,
    count: usize,
    usage: String,
}

#[medousa_tool(id = COGNITION_MEMORY_TAGS_ID)]
impl CognitionMemoryTagsTool {
    /// List indexed Locus semantic tags for the active profile tenant. Optional prefix narrows vocabulary (e.g. profile:, chat:, medousa). Use before recall/list to pick tag filters.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryTagsInput,
    ) -> stasis::prelude::Result<MemoryTagsOutput> {
        let limit = input.limit.into_option().unwrap_or(100).clamp(1, 500);
        let prefix = input
            .prefix
            .into_option()
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase);

        let session_scope = if input.session_id.is_global() {
            None
        } else if input.session_id.was_present() {
            Some(
                resolve_memory_tool_session_id_typed(
                    input.session_id.explicit(),
                    &self.turn_scope,
                    &self.fallback_chat_session_id,
                    self.workshop_dynamic,
                )
                .await?,
            )
        } else {
            None
        };
        let tenant =
            crate::locus_semantic_tags::resolve_workshop_tag_tenant_id(session_scope.as_deref());

        emit_invoked(
            &self.event_tx,
            COGNITION_MEMORY_TAGS_ID.as_str(),
            prefix.as_deref().unwrap_or("all"),
        )
        .await;

        let tags = self
            .semantic_index
            .find_tags_async(&tenant, prefix.as_deref(), limit)
            .await
            .map_err(|err| {
                StasisError::PortFailure(format!(
                    "cognition_memory_query action=memory.tags: {err}"
                ))
            })?;

        Ok(MemoryTagsOutput {
            tenant_id: tenant,
            prefix,
            count: tags.len(),
            tags,
            usage: "Pass tags to cognition_memory_query action=memory.context|memory.list|memory.recall via semantic_tags (match-all).".to_string(),
        })
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryMoodsInput {
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) target_mood: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "f64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) blend: CompatOption<f64>,
    #[serde(default)]
    #[schemars(
        with = "f64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) current_stability: CompatOption<f64>,
    #[serde(default)]
    #[schemars(
        with = "f64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) current_friction: CompatOption<f64>,
    #[serde(default)]
    #[schemars(
        with = "f64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) current_logic: CompatOption<f64>,
    #[serde(default)]
    #[schemars(
        with = "f64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) current_autonomy: CompatOption<f64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryMoodPresetOutput {
    name: String,
    description: String,
    avec: MemoryAvecOutput,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryMoodSwapOutput {
    target_mood: String,
    blend: f32,
    current: MemoryAvecOutput,
    target: MemoryAvecOutput,
    blended: MemoryAvecOutput,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryMoodsOutput {
    presets: Vec<MemoryMoodPresetOutput>,
    apply_guide: String,
    swap_preview: Option<MemoryMoodSwapOutput>,
}

#[medousa_tool(id = COGNITION_MEMORY_MOODS_ID)]
impl CognitionMemoryMoodsTool {
    /// AVEC mood presets and blend preview. Use before store/retrieve when reasoning posture is unset.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryMoodsInput,
    ) -> stasis::prelude::Result<MemoryMoodsOutput> {
        let target_mood = input.target_mood.into_option();
        let blend = input.blend.into_option().unwrap_or(1.0) as f32;
        let current_stability = input
            .current_stability
            .into_option()
            .map(|value| value as f32);
        let current_friction = input
            .current_friction
            .into_option()
            .map(|value| value as f32);
        let current_logic = input.current_logic.into_option().map(|value| value as f32);
        let current_autonomy = input
            .current_autonomy
            .into_option()
            .map(|value| value as f32);

        emit_invoked(&self.event_tx, COGNITION_MEMORY_MOODS_ID.as_str(), "moods").await;

        let result = self.moods.get(
            target_mood.as_deref(),
            blend,
            current_stability,
            current_friction,
            current_logic,
            current_autonomy,
        );

        let swap_preview = result.swap_preview.map(|preview| MemoryMoodSwapOutput {
            target_mood: preview.target_mood,
            blend: preview.blend,
            current: MemoryAvecOutput::from_locus(preview.current),
            target: MemoryAvecOutput::from_locus(preview.target),
            blended: MemoryAvecOutput::from_locus(preview.blended),
        });

        Ok(MemoryMoodsOutput {
            presets: result
                .presets
                .into_iter()
                .map(|preset| MemoryMoodPresetOutput {
                    name: preset.name,
                    description: preset.description,
                    avec: MemoryAvecOutput::from_locus(preset.avec),
                })
                .collect(),
            apply_guide: result.apply_guide,
            swap_preview,
        })
    }
}

// ── cognition_memory_evict ────────────────────────────────────────────────────

pub struct CognitionMemoryEvictTool {
    operations: Arc<dyn MemoryOperations>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionMemoryEvictTool {
    pub fn new(
        operations: Arc<dyn MemoryOperations>,
        fallback_chat_session_id: String,
        workshop_dynamic: bool,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn typed_memory_node_output_preserves_legacy_wire_shape() {
        let node = MemoryNode::default();
        assert_eq!(
            serde_json::to_value(MemoryNodeOutput::from(&node)).expect("typed node"),
            crate::locus_memory::memory_node_to_json(&node)
        );
    }

    #[test]
    fn typed_store_rejection_preserves_legacy_wire_shape() {
        let node_id = "node-a".to_string();
        let message = "Strict profile rejected node".to_string();
        let profile = "strict";
        let typed = MemoryStoreOutput::Rejected {
            node_id: node_id.clone(),
            psi: 1.25,
            valid: false,
            stored: false,
            validation_error: message.clone(),
            profile_policy: profile.to_string(),
            error: MemoryStoreErrorOutput {
                code: infer_store_error_code(&message).to_string(),
                message: message.clone(),
                model_guidance: store_failure_guidance(&message, profile),
            },
        };

        assert_eq!(
            serde_json::to_value(typed).expect("typed rejection"),
            crate::locus_memory::store_failure_payload(node_id, 1.25, false, message, profile)
        );
    }

    #[test]
    fn store_input_keeps_deprecated_content_and_hidden_vibe_aliases() {
        let input: MemoryStoreInput = serde_json::from_value(json!({
            "content": "legacy STTP",
            "vibe_signature": "steady",
            "semantic_tags": ["One", 7, "Two"]
        }))
        .expect("compatible store input");

        assert!(input.node.is_none());
        assert_eq!(input.content.as_deref(), Some("legacy STTP"));
        assert_eq!(input.vibe_signature.as_deref(), Some("steady"));
        assert_eq!(
            input.semantic_tags,
            Some(vec!["One".to_string(), "Two".to_string()])
        );
    }

    #[test]
    fn session_scope_distinguishes_missing_null_explicit_and_invalid() {
        let missing: MemoryListInput = serde_json::from_value(json!({})).expect("missing");
        let global: MemoryListInput =
            serde_json::from_value(json!({ "session_id": null })).expect("global");
        let explicit: MemoryListInput =
            serde_json::from_value(json!({ "session_id": "session-a" })).expect("explicit");
        let invalid: MemoryListInput =
            serde_json::from_value(json!({ "session_id": 7 })).expect("invalid");

        assert!(matches!(
            missing.session_id,
            MemorySessionScopeInput::Missing
        ));
        assert!(matches!(global.session_id, MemorySessionScopeInput::Global));
        assert!(matches!(
            explicit.session_id,
            MemorySessionScopeInput::Explicit(ref value) if value == "session-a"
        ));
        assert!(matches!(
            invalid.session_id,
            MemorySessionScopeInput::Invalid
        ));
    }

    #[test]
    fn recall_input_keeps_legacy_comma_separated_semantic_tags() {
        let input: MemoryRecallInput = serde_json::from_value(json!({
            "query": "decision",
            "semantic_tags": "Session, profile:Work, session"
        }))
        .expect("compatible recall input");

        assert_eq!(
            input
                .semantic_tags
                .as_ref()
                .and_then(CompatibleSemanticTags::as_deref),
            Some(["profile:work".to_string(), "session".to_string()].as_slice())
        );
    }

    #[test]
    fn memory_commands_preserve_sttp_boundaries_and_normalize_filters() {
        let raw_node = "  ⊕⟨ prime: { context_summary: \"hello\" } ⟩  \n";
        let store = MemoryStoreCommand::try_from(MemoryStoreInput {
            node: Some(raw_node.to_string()),
            session_id: Some(" session-a ".to_string()),
            semantic_tags: Some(vec![" profile:work ".to_string(), " \n".to_string()]),
            content: Some("fallback".to_string()),
            vibe_signature: Some(" steady ".to_string()),
        })
        .expect("store command");
        assert_eq!(store.node.as_str(), raw_node);
        assert_eq!(
            store.session_id.as_ref().map(TrimmedText::as_str),
            Some("session-a")
        );
        assert_eq!(store.semantic_tags.len(), 1);
        assert_eq!(store.semantic_tags[0].as_str(), "profile:work");
        assert_eq!(
            store.vibe_signature.as_ref().map(TrimmedText::as_str),
            Some("steady")
        );

        let recall = MemoryRecallCommand::try_from(MemoryRecallInput {
            query: Some("  decision  ".to_string()),
            session_id: MemorySessionScopeInput::Global,
            limit: Some(999),
            semantic_tags: None,
            tag_prefix: Some(" profile: ".to_string()),
        })
        .expect("recall command");
        assert_eq!(recall.query.as_str(), "decision");
        assert_eq!(recall.limit, 20);
        assert_eq!(
            recall.tag_prefix.as_ref().map(TrimmedText::as_str),
            Some("profile:")
        );
    }

    #[test]
    fn memory_store_command_uses_legacy_content_without_trimming_bytes() {
        let raw_content = "  legacy STTP  \n";
        let store = MemoryStoreCommand::try_from(MemoryStoreInput {
            node: Some(" \n\t".to_string()),
            session_id: None,
            semantic_tags: None,
            content: Some(raw_content.to_string()),
            vibe_signature: None,
        })
        .expect("legacy content fallback");
        assert_eq!(store.node.as_str(), raw_content);
    }

    #[test]
    fn memory_commands_reject_blank_required_content() {
        let store_error = MemoryStoreCommand::try_from(MemoryStoreInput {
            node: Some(" \n\t".to_string()),
            session_id: None,
            semantic_tags: None,
            content: None,
            vibe_signature: None,
        })
        .expect_err("blank memory node should fail");
        assert!(store_error.to_string().contains("full STTP string"));

        let recall_error = MemoryRecallCommand::try_from(MemoryRecallInput {
            query: Some(" \n\t".to_string()),
            session_id: MemorySessionScopeInput::Missing,
            limit: None,
            semantic_tags: None,
            tag_prefix: None,
        })
        .expect_err("blank recall query should fail");
        assert!(recall_error.to_string().contains("query is required"));
    }

    #[test]
    fn evict_input_keeps_legacy_lenient_optional_values_at_the_wire_boundary() {
        let input: MemoryEvictInput = serde_json::from_value(json!({
            "mode": 7,
            "dry_run": "preview",
            "force": null,
            "session_id": 42,
            "tiers": [" working ", 9, "archive"],
            "node_ids": "node-a",
            "sync_keys": ["sync-a", null, "sync-b"],
            "max_nodes": -1
        }))
        .expect("compatible eviction input");

        assert!(input.mode.is_none());
        assert!(input.dry_run.is_none());
        assert!(input.force.is_none());
        assert!(input.session_id.is_none());
        assert_eq!(
            input.tiers.into_option(),
            Some(vec![" working ".to_string(), "archive".to_string()])
        );
        assert!(input.node_ids.is_none());
        assert_eq!(
            input.sync_keys.into_option(),
            Some(vec!["sync-a".to_string(), "sync-b".to_string()])
        );
        assert!(input.max_nodes.is_none());
    }
}

fn parse_evict_mode(value: Option<&str>) -> MemoryEvictMode {
    match value
        .unwrap_or("by_filter")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "purge_session" => MemoryEvictMode::PurgeSession,
        "by_node_ids" => MemoryEvictMode::ByNodeIds,
        "by_sync_keys" => MemoryEvictMode::BySyncKeys,
        _ => MemoryEvictMode::ByFilter,
    }
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MemoryEvictModeSchema {
    ByFilter,
    PurgeSession,
    ByNodeIds,
    BySyncKeys,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryEvictInput {
    /// Eviction strategy (default: by_filter)
    #[serde(default)]
    #[schemars(
        with = "MemoryEvictModeSchema",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) mode: CompatOption<String>,
    /// Preview deletions without applying (default: true)
    #[serde(default)]
    #[schemars(
        with = "bool",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) dry_run: CompatOption<bool>,
    /// Bypass inbound-reference blocks (default: false)
    #[serde(default)]
    #[schemars(
        with = "bool",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) force: CompatOption<bool>,
    /// Locus session scope (defaults to current turn session)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) session_id: CompatOption<String>,
    /// Filter tiers for by_filter mode
    #[serde(default)]
    #[schemars(
        with = "Vec<String>",
        skip_serializing_if = "crate::typed_tools::CompatList::is_none"
    )]
    pub(crate) tiers: CompatList<String>,
    /// Node ids for by_node_ids mode
    #[serde(default)]
    #[schemars(
        with = "Vec<String>",
        skip_serializing_if = "crate::typed_tools::CompatList::is_none"
    )]
    pub(crate) node_ids: CompatList<String>,
    /// Sync keys for by_sync_keys mode
    #[serde(default)]
    #[schemars(
        with = "Vec<String>",
        skip_serializing_if = "crate::typed_tools::CompatList::is_none"
    )]
    pub(crate) sync_keys: CompatList<String>,
    /// Safety cap on nodes touched (default: 5000)
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) max_nodes: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoryEvictOutput {
    dry_run: bool,
    deleted: usize,
    blocked: usize,
    not_found: usize,
    skipped: usize,
    would_delete: Vec<String>,
    session_id: String,
}

#[medousa_tool(id = COGNITION_MEMORY_EVICT_ID)]
impl CognitionMemoryEvictTool {
    /// Evict Locus memory nodes (dry-run by default). Supports by_filter, purge_session, by_node_ids, and by_sync_keys modes.
    pub(crate) async fn invoke_typed(
        &self,
        input: MemoryEvictInput,
    ) -> stasis::prelude::Result<MemoryEvictOutput> {
        let mode_value = input.mode.into_option();
        let session_id = input.session_id.into_option();
        let tiers = input.tiers.into_option();
        let node_ids = input.node_ids.into_option();
        let sync_keys = input.sync_keys.into_option();
        let mode = parse_evict_mode(mode_value.as_deref());
        let dry_run = input.dry_run.into_option().unwrap_or(true);
        let force = input.force.into_option().unwrap_or(false);
        let max_nodes = input
            .max_nodes
            .into_option()
            .unwrap_or(5000)
            .clamp(1, 50_000);

        let locus_session = resolve_memory_tool_session_id_typed(
            session_id.as_deref(),
            &self.turn_scope,
            &self.fallback_chat_session_id,
            self.workshop_dynamic,
        )
        .await?;
        let tenant = derive_locus_tenant_id(&locus_session);
        let mut scope = MemoryScope {
            session_ids: Some(vec![locus_session.clone()]),
            ..Default::default()
        };
        if tenant != LOCUS_DEFAULT_TENANT {
            scope.tenant_id = Some(tenant);
        }
        if let Some(tiers) = tiers {
            scope.tiers = Some(
                tiers
                    .into_iter()
                    .filter_map(|value| optional_nonempty(&value).map(str::to_string))
                    .collect(),
            );
        }

        let node_ids = node_ids.map(|items| {
            items
                .into_iter()
                .filter_map(|value| optional_nonempty(&value).map(str::to_string))
                .collect::<Vec<_>>()
        });
        let sync_keys = sync_keys.map(|items| {
            items
                .into_iter()
                .filter_map(|value| optional_nonempty(&value).map(str::to_string))
                .collect::<Vec<_>>()
        });

        emit_invoked(
            &self.event_tx,
            COGNITION_MEMORY_EVICT_ID.as_str(),
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

        Ok(MemoryEvictOutput {
            dry_run: response.dry_run,
            deleted: response.deleted,
            blocked: response.blocked,
            not_found: response.not_found,
            skipped: response.skipped,
            would_delete: response.would_delete,
            session_id: locus_session,
        })
    }
}
