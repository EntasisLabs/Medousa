//! Public memory primitives: one query tool and one mutate tool.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use locus_core_rs::NodeStore;
use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::memory_tools::{
    CognitionMemoryCalibrateTool, CognitionMemoryContextTool, CognitionMemoryEvictTool,
    CognitionMemoryListTool, CognitionMemoryMoodsTool, CognitionMemoryRecallTool,
    CognitionMemorySchemaTool, CognitionMemoryStoreTool, CognitionMemoryTagsTool,
    CompatibleSemanticTags, MemoryCalibrateInput, MemoryContextInput, MemoryEvictInput,
    MemoryListInput, MemoryMoodsInput, MemoryRecallInput, MemorySchemaInput,
    MemorySessionScopeInput, MemoryStoreInput, MemoryTagsInput,
};
use crate::public_api::{COGNITION_MEMORY_MUTATE, COGNITION_MEMORY_QUERY};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::typed_tools::{
    CompatList, CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};

const MEMORY_QUERY_ID: ToolId = ToolId::new(COGNITION_MEMORY_QUERY);
const MEMORY_MUTATE_ID: ToolId = ToolId::new(COGNITION_MEMORY_MUTATE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum MemoryQueryAction {
    #[serde(rename = "memory.schema")]
    Schema(MemorySchema),
    #[serde(rename = "memory.context")]
    Context(MemoryContext),
    #[serde(rename = "memory.list")]
    List(MemoryList),
    #[serde(rename = "memory.recall")]
    Recall(MemoryRecall),
    #[serde(rename = "memory.tags")]
    Tags(MemoryTags),
    #[serde(rename = "memory.moods")]
    Moods(MemoryMoods),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum MemoryMutateAction {
    #[serde(rename = "memory.store")]
    Store(MemoryStore),
    #[serde(rename = "memory.calibrate")]
    Calibrate(MemoryCalibrate),
    #[serde(rename = "memory.evict")]
    Evict(MemoryEvict),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct MemorySchema {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryContext {
    /// Omit for the current turn; JSON null searches globally
    #[serde(default)]
    session_id: MemorySessionScopeInput,
    stability: f64,
    friction: f64,
    logic: f64,
    autonomy: f64,
    #[serde(default)]
    context_keywords: Option<Vec<String>>,
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    tag_prefix: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    alpha: Option<f64>,
    #[serde(default)]
    beta: Option<f64>,
    #[serde(default)]
    from_utc: Option<String>,
    #[serde(default)]
    to_utc: Option<String>,
    #[serde(default)]
    tiers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryList {
    /// Omit for the current turn; JSON null lists globally
    #[serde(default)]
    session_id: MemorySessionScopeInput,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    context_keywords: Option<Vec<String>>,
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    tag_prefix: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryRecall {
    query: String,
    /// Omit for the current turn; JSON null searches globally
    #[serde(default)]
    session_id: MemorySessionScopeInput,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    tag_prefix: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryTags {
    /// Optional session used to derive tenant scope
    #[serde(default)]
    session_id: MemorySessionScopeInput,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct MemoryMoods {
    #[serde(default)]
    target_mood: Option<String>,
    #[serde(default)]
    blend: Option<f64>,
    #[serde(default)]
    current_stability: Option<f64>,
    #[serde(default)]
    current_friction: Option<f64>,
    #[serde(default)]
    current_logic: Option<f64>,
    #[serde(default)]
    current_autonomy: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryStore {
    /// Full STTP node payload
    node: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryCalibrate {
    stability: f64,
    friction: f64,
    logic: f64,
    autonomy: f64,
    /// e.g. manual, session_start
    trigger: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryEvict {
    /// by_filter | purge_session | by_node_ids | by_sync_keys
    #[serde(default)]
    mode: Option<String>,
    /// Preview deletions without applying (default: true)
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    force: Option<bool>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    tiers: Option<Vec<String>>,
    #[serde(default)]
    node_ids: Option<Vec<String>>,
    #[serde(default)]
    sync_keys: Option<Vec<String>>,
    #[serde(default)]
    max_nodes: Option<usize>,
}

impl JsonSchema for MemoryQueryAction {
    fn schema_name() -> String {
        "MemoryQueryAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "memory.schema",
                "memory.context",
                "memory.list",
                "memory.recall",
                "memory.tags",
                "memory.moods",
            ]),
            true,
        )])
    }
}

impl JsonSchema for MemoryMutateAction {
    fn schema_name() -> String {
        "MemoryMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["memory.store", "memory.calibrate", "memory.evict"]),
            true,
        )])
    }
}

pub fn memory_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<MemorySchema>(
            MEMORY_QUERY_ID,
            "memory.schema",
            "Canonical STTP example and ingest policy",
        ),
        typed_action_schema::<MemoryContext>(
            MEMORY_QUERY_ID,
            "memory.context",
            "AVEC-ranked session memory retrieval",
        ),
        typed_action_schema::<MemoryList>(
            MEMORY_QUERY_ID,
            "memory.list",
            "Newest-first memory inventory",
        ),
        typed_action_schema::<MemoryRecall>(
            MEMORY_QUERY_ID,
            "memory.recall",
            "Keyword recall (prefer memory.context with explicit AVEC)",
        ),
        typed_action_schema::<MemoryTags>(
            MEMORY_QUERY_ID,
            "memory.tags",
            "Browse indexed Locus semantic tags",
        ),
        typed_action_schema::<MemoryMoods>(
            MEMORY_QUERY_ID,
            "memory.moods",
            "AVEC mood presets and blend preview",
        ),
        typed_action_schema::<MemoryStore>(
            MEMORY_MUTATE_ID,
            "memory.store",
            "Store a complete STTP node",
        ),
        typed_action_schema::<MemoryCalibrate>(
            MEMORY_MUTATE_ID,
            "memory.calibrate",
            "Write AVEC calibration for a session",
        ),
        typed_action_schema::<MemoryEvict>(
            MEMORY_MUTATE_ID,
            "memory.evict",
            "Evict Locus nodes (dry-run by default)",
        ),
    ]
}

pub struct CognitionMemoryQueryTool {
    locus_store: Arc<dyn NodeStore>,
    memory_reader: Arc<dyn MemoryContextReader>,
    semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

pub struct CognitionMemoryMutateTool {
    memory_writer: Arc<dyn MemoryContextWriter>,
    locus_store: Arc<dyn NodeStore>,
    memory_operations:
        Arc<dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations>,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

#[allow(clippy::too_many_arguments)]
pub fn register_memory_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    locus_store: Arc<dyn NodeStore>,
    memory_reader: Arc<dyn MemoryContextReader>,
    memory_writer: Arc<dyn MemoryContextWriter>,
    semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    memory_operations: Arc<
        dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations,
    >,
    fallback_chat_session_id: String,
    workshop_dynamic: bool,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionMemoryQueryTool {
        locus_store: locus_store.clone(),
        memory_reader,
        semantic_index,
        fallback_chat_session_id: fallback_chat_session_id.clone(),
        workshop_dynamic,
        turn_scope: turn_scope.clone(),
        event_tx: event_tx.clone(),
    })?;
    registry.register_typed_tool(CognitionMemoryMutateTool {
        memory_writer,
        locus_store,
        memory_operations,
        fallback_chat_session_id,
        workshop_dynamic,
        turn_scope,
        event_tx,
    })?;
    Ok(())
}

#[medousa_tool(id = MEMORY_QUERY_ID)]
impl CognitionMemoryQueryTool {
    /// Read Locus memory: schema, AVEC context, list, recall, tags, or mood presets. action is a typed name (memory.context, memory.recall, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: MemoryQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = MEMORY_MUTATE_ID)]
impl CognitionMemoryMutateTool {
    /// Write Locus memory: store an STTP node, calibrate AVEC, or evict nodes. action is a typed name (memory.store, memory.calibrate, memory.evict). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: MemoryMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionMemoryQueryTool,
    action: MemoryQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        MemoryQueryAction::Schema(params) => params.execute().await,
        MemoryQueryAction::Context(params) => params.execute(tool).await,
        MemoryQueryAction::List(params) => params.execute(tool).await,
        MemoryQueryAction::Recall(params) => params.execute(tool).await,
        MemoryQueryAction::Tags(params) => params.execute(tool).await,
        MemoryQueryAction::Moods(params) => params.execute(tool).await,
    }
}

async fn dispatch_mutate(
    tool: &CognitionMemoryMutateTool,
    action: MemoryMutateAction,
) -> stasis::prelude::Result<Value> {
    match action {
        MemoryMutateAction::Store(params) => params.execute(tool).await,
        MemoryMutateAction::Calibrate(params) => params.execute(tool).await,
        MemoryMutateAction::Evict(params) => params.execute(tool).await,
    }
}

fn tags(tags: Option<Vec<String>>) -> Option<CompatibleSemanticTags> {
    CompatibleSemanticTags::from_vec(tags)
}

impl MemorySchema {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        let output = CognitionMemorySchemaTool
            .invoke_typed(MemorySchemaInput {})
            .await?;
        serialize_output(CognitionMemorySchemaTool::tool_id(), output)
    }
}

impl MemoryContext {
    async fn execute(self, tool: &CognitionMemoryQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryContextTool::new(
            tool.locus_store.clone(),
            tool.memory_reader.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryContextInput {
            session_id: self.session_id,
            stability: Some(self.stability),
            friction: Some(self.friction),
            logic: Some(self.logic),
            autonomy: Some(self.autonomy),
            context_keywords: self.context_keywords,
            semantic_tags: tags(self.semantic_tags),
            tag_prefix: self.tag_prefix,
            limit: self.limit,
            alpha: self.alpha,
            beta: self.beta,
            from_utc: self.from_utc,
            to_utc: self.to_utc,
            tiers: self.tiers,
        })
        .await?;
        serialize_output(CognitionMemoryContextTool::tool_id(), output)
    }
}

impl MemoryList {
    async fn execute(self, tool: &CognitionMemoryQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryListTool::new(
            tool.locus_store.clone(),
            tool.memory_reader.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryListInput {
            session_id: self.session_id,
            limit: CompatOption::from(self.limit),
            context_keywords: CompatList::from(self.context_keywords),
            semantic_tags: tags(self.semantic_tags),
            tag_prefix: CompatOption::from(self.tag_prefix),
        })
        .await?;
        serialize_output(CognitionMemoryListTool::tool_id(), output)
    }
}

impl MemoryRecall {
    async fn execute(self, tool: &CognitionMemoryQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryRecallTool::new(
            tool.locus_store.clone(),
            tool.memory_reader.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryRecallInput {
            query: Some(self.query),
            session_id: self.session_id,
            limit: self.limit,
            semantic_tags: tags(self.semantic_tags),
            tag_prefix: self.tag_prefix,
        })
        .await?;
        serialize_output(CognitionMemoryRecallTool::tool_id(), output)
    }
}

impl MemoryTags {
    async fn execute(self, tool: &CognitionMemoryQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryTagsTool::new(
            tool.semantic_index.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryTagsInput {
            session_id: self.session_id,
            prefix: CompatOption::from(self.prefix),
            limit: CompatOption::from(self.limit),
        })
        .await?;
        serialize_output(CognitionMemoryTagsTool::tool_id(), output)
    }
}

impl MemoryMoods {
    async fn execute(self, tool: &CognitionMemoryQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryMoodsTool::new(tool.event_tx.clone())
            .invoke_typed(MemoryMoodsInput {
                target_mood: CompatOption::from(self.target_mood),
                blend: CompatOption::from(self.blend),
                current_stability: CompatOption::from(self.current_stability),
                current_friction: CompatOption::from(self.current_friction),
                current_logic: CompatOption::from(self.current_logic),
                current_autonomy: CompatOption::from(self.current_autonomy),
            })
            .await?;
        serialize_output(CognitionMemoryMoodsTool::tool_id(), output)
    }
}

impl MemoryStore {
    async fn execute(self, tool: &CognitionMemoryMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryStoreTool::new(
            tool.memory_writer.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryStoreInput {
            node: Some(self.node),
            session_id: self.session_id,
            semantic_tags: self.semantic_tags,
            content: None,
            vibe_signature: None,
        })
        .await?;
        serialize_output(CognitionMemoryStoreTool::tool_id(), output)
    }
}

impl MemoryCalibrate {
    async fn execute(self, tool: &CognitionMemoryMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryCalibrateTool::new(
            tool.locus_store.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryCalibrateInput {
            session_id: self.session_id,
            stability: Some(self.stability),
            friction: Some(self.friction),
            logic: Some(self.logic),
            autonomy: Some(self.autonomy),
            trigger: Some(self.trigger),
        })
        .await?;
        serialize_output(CognitionMemoryCalibrateTool::tool_id(), output)
    }
}

impl MemoryEvict {
    async fn execute(self, tool: &CognitionMemoryMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMemoryEvictTool::new(
            tool.memory_operations.clone(),
            tool.fallback_chat_session_id.clone(),
            tool.workshop_dynamic,
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(MemoryEvictInput {
            mode: CompatOption::from(self.mode),
            dry_run: CompatOption::from(self.dry_run),
            force: CompatOption::from(self.force),
            session_id: CompatOption::from(self.session_id),
            tiers: CompatList::from(self.tiers),
            node_ids: CompatList::from(self.node_ids),
            sync_keys: CompatList::from(self.sync_keys),
            max_nodes: CompatOption::from(self.max_nodes),
        })
        .await?;
        serialize_output(CognitionMemoryEvictTool::tool_id(), output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_actions_carry_their_params() {
        let query: MemoryQueryAction = serde_json::from_value(json!({
            "action": "memory.recall",
            "query": "decision",
            "session_id": null
        }))
        .expect("recall");
        match query {
            MemoryQueryAction::Recall(MemoryRecall {
                query, session_id, ..
            }) => {
                assert_eq!(query, "decision");
                assert!(matches!(session_id, MemorySessionScopeInput::Global));
            }
            other => panic!("expected memory.recall, got {other:?}"),
        }
        let mutate: MemoryMutateAction = serde_json::from_value(json!({
            "action": "memory.store",
            "node": "⊕⟨ prime ⟩"
        }))
        .expect("store");
        match mutate {
            MemoryMutateAction::Store(MemoryStore { node, .. }) => {
                assert_eq!(node, "⊕⟨ prime ⟩");
            }
            other => panic!("expected memory.store, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let query = serde_json::to_value(schemars::schema_for!(MemoryQueryAction)).expect("query");
        let mutate =
            serde_json::to_value(schemars::schema_for!(MemoryMutateAction)).expect("mutate");
        for schema in [&query, &mutate] {
            let props = schema["properties"].as_object().expect("properties");
            assert_eq!(props.len(), 1);
            assert!(
                props["action"]["enum"]
                    .as_array()
                    .is_some_and(|values| !values.is_empty())
            );
            assert_eq!(schema["additionalProperties"], true);
        }
    }
}
