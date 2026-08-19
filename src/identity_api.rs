//! Public identity primitives: one query tool and one mutate tool.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::identity_tools::{
    CognitionIdentityCommitTool, CognitionIdentityContextTool, CognitionIdentityProposeTool,
    CognitionIdentityRecallTool, CognitionIdentityRememberTool, CompatibleObject,
    IdentityCommitInput, IdentityContextInput, IdentityProposeInput, IdentityRecallInput,
    IdentityRememberInput,
};
use crate::public_api::{COGNITION_IDENTITY_MUTATE, COGNITION_IDENTITY_QUERY};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::typed_tools::{
    CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};

const IDENTITY_QUERY_ID: ToolId = ToolId::new(COGNITION_IDENTITY_QUERY);
const IDENTITY_MUTATE_ID: ToolId = ToolId::new(COGNITION_IDENTITY_MUTATE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum IdentityQueryAction {
    #[serde(rename = "identity.recall")]
    Recall(IdentityRecall),
    #[serde(rename = "identity.context")]
    Context(IdentityContext),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum IdentityMutateAction {
    #[serde(rename = "identity.remember")]
    Remember(IdentityRemember),
    #[serde(rename = "identity.propose")]
    Propose(IdentityPropose),
    #[serde(rename = "identity.commit")]
    Commit(IdentityCommit),
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdentityRecall {
    query: String,
    /// Optional filter; defaults to any (preference | person | note | any)
    #[serde(default)]
    fact_kind: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct IdentityContext {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    persona_id: Option<String>,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    relationship_limit: Option<usize>,
    /// Identity context slice (default: cognitive)
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdentityRemember {
    /// preference | person | note
    fact_kind: String,
    /// Preference key, person display name, or note subject
    subject: String,
    /// Human-readable fact
    statement: String,
    #[serde(default)]
    attributes: Option<Value>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    /// Defaults to user_direct when the operator stated the fact
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdentityPropose {
    /// persona | user | contact | relationship | channel | policy
    entity_type: String,
    entity_id: String,
    /// Flat or nested JSON patch object
    patch: Value,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    actor: Option<String>,
    /// RFC3339 UTC
    #[serde(default)]
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdentityCommit {
    proposal_id: String,
    expected_version: i64,
    #[serde(default)]
    approver: Option<String>,
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    patch: Option<Value>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    tier: Option<String>,
}

impl JsonSchema for IdentityQueryAction {
    fn schema_name() -> String {
        "IdentityQueryAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["identity.recall", "identity.context"]),
            true,
        )])
    }
}

impl JsonSchema for IdentityMutateAction {
    fn schema_name() -> String {
        "IdentityMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["identity.remember", "identity.propose", "identity.commit"]),
            true,
        )])
    }
}

pub fn identity_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<IdentityRecall>(
            IDENTITY_QUERY_ID,
            "identity.recall",
            "Look up preferences, people, and identity facts",
        ),
        typed_action_schema::<IdentityContext>(
            IDENTITY_QUERY_ID,
            "identity.context",
            "Read identity graph context (persona, user, channels, relationships)",
        ),
        typed_action_schema::<IdentityRemember>(
            IDENTITY_MUTATE_ID,
            "identity.remember",
            "Remember a durable personal fact (preferences, people, notes)",
        ),
        typed_action_schema::<IdentityPropose>(
            IDENTITY_MUTATE_ID,
            "identity.propose",
            "Propose a durable identity patch; commit when policy allows",
        ),
        typed_action_schema::<IdentityCommit>(
            IDENTITY_MUTATE_ID,
            "identity.commit",
            "Commit a proposed identity patch when tier and policy allow",
        ),
    ]
}

pub struct CognitionIdentityQueryTool {
    service: Arc<IdentityMemoryService>,
    store: Arc<MedousaIdentityMemoryStore>,
    default_user_id: String,
    default_persona_id: String,
    default_channel_id: String,
    workshop_dynamic: bool,
    event_tx: mpsc::Sender<TuiEvent>,
}

pub struct CognitionIdentityMutateTool {
    service: Arc<IdentityMemoryService>,
    store: Arc<MedousaIdentityMemoryStore>,
    memory_writer: Option<Arc<dyn MemoryContextWriter>>,
    default_user_id: String,
    workshop_dynamic: bool,
    event_tx: mpsc::Sender<TuiEvent>,
}

#[allow(clippy::too_many_arguments)]
pub fn register_identity_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    identity_service: Arc<IdentityMemoryService>,
    identity_store: Arc<MedousaIdentityMemoryStore>,
    memory_writer: Option<Arc<dyn MemoryContextWriter>>,
    default_user_id: String,
    default_persona_id: String,
    default_channel_id: String,
    workshop_dynamic: bool,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionIdentityQueryTool {
        service: identity_service.clone(),
        store: identity_store.clone(),
        default_user_id: default_user_id.clone(),
        default_persona_id,
        default_channel_id,
        workshop_dynamic,
        event_tx: event_tx.clone(),
    })?;
    registry.register_typed_tool(CognitionIdentityMutateTool {
        service: identity_service,
        store: identity_store,
        memory_writer,
        default_user_id,
        workshop_dynamic,
        event_tx,
    })?;
    Ok(())
}

#[medousa_tool(id = IDENTITY_QUERY_ID)]
impl CognitionIdentityQueryTool {
    /// Read identity: recall facts or load graph context. action is a typed name (identity.recall, identity.context). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: IdentityQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = IDENTITY_MUTATE_ID)]
impl CognitionIdentityMutateTool {
    /// Write identity: remember a fact, propose a patch, or commit. action is a typed name (identity.remember, identity.propose, identity.commit). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: IdentityMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionIdentityQueryTool,
    action: IdentityQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        IdentityQueryAction::Recall(params) => params.execute(tool).await,
        IdentityQueryAction::Context(params) => params.execute(tool).await,
    }
}

async fn dispatch_mutate(
    tool: &CognitionIdentityMutateTool,
    action: IdentityMutateAction,
) -> stasis::prelude::Result<Value> {
    match action {
        IdentityMutateAction::Remember(params) => params.execute(tool).await,
        IdentityMutateAction::Propose(params) => params.execute(tool).await,
        IdentityMutateAction::Commit(params) => params.execute(tool).await,
    }
}

impl IdentityRecall {
    async fn execute(self, tool: &CognitionIdentityQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionIdentityRecallTool::new(
            tool.store.clone(),
            tool.default_user_id.clone(),
            tool.workshop_dynamic,
            tool.event_tx.clone(),
        )
        .invoke_typed(IdentityRecallInput {
            query: Some(self.query),
            fact_kind: self.fact_kind,
            limit: self.limit,
            user_id: self.user_id,
        })
        .await?;
        serialize_output(CognitionIdentityRecallTool::tool_id(), output)
    }
}

impl IdentityContext {
    async fn execute(self, tool: &CognitionIdentityQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionIdentityContextTool::new(
            tool.service.clone(),
            tool.default_user_id.clone(),
            tool.default_persona_id.clone(),
            tool.default_channel_id.clone(),
            tool.workshop_dynamic,
            tool.event_tx.clone(),
        )
        .invoke_typed(IdentityContextInput {
            user_id: CompatOption::from(self.user_id),
            persona_id: CompatOption::from(self.persona_id),
            channel_id: CompatOption::from(self.channel_id),
            relationship_limit: CompatOption::from(self.relationship_limit),
            mode: CompatOption::from(self.mode),
        })
        .await?;
        serialize_output(CognitionIdentityContextTool::tool_id(), output)
    }
}

impl IdentityRemember {
    async fn execute(self, tool: &CognitionIdentityMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionIdentityRememberTool::new(
            tool.store.clone(),
            tool.memory_writer.clone(),
            tool.default_user_id.clone(),
            tool.workshop_dynamic,
            tool.event_tx.clone(),
        )
        .invoke_typed(IdentityRememberInput {
            fact_kind: Some(self.fact_kind),
            subject: Some(self.subject),
            statement: Some(self.statement),
            attributes: self.attributes.map(CompatibleObject::from),
            aliases: self.aliases,
            source: self.source,
            confidence: self.confidence,
            reason: self.reason,
            user_id: self.user_id,
        })
        .await?;
        serialize_output(CognitionIdentityRememberTool::tool_id(), output)
    }
}

impl IdentityPropose {
    async fn execute(self, tool: &CognitionIdentityMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionIdentityProposeTool::new(tool.service.clone(), tool.event_tx.clone())
            .invoke_typed(IdentityProposeInput {
                entity_type: Some(self.entity_type),
                entity_id: Some(self.entity_id),
                patch: Some(CompatibleObject::from(self.patch)),
                source: self.source,
                confidence: self.confidence,
                reason: self.reason,
                actor: self.actor,
                expires_at: self.expires_at,
            })
            .await?;
        serialize_output(CognitionIdentityProposeTool::tool_id(), output)
    }
}

impl IdentityCommit {
    async fn execute(self, tool: &CognitionIdentityMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionIdentityCommitTool::new(
            tool.service.clone(),
            tool.memory_writer.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(IdentityCommitInput {
            proposal_id: Some(self.proposal_id),
            expected_version: Some(self.expected_version),
            approver: self.approver,
            entity_type: self.entity_type,
            entity_id: self.entity_id,
            patch: self.patch.map(CompatibleObject::from),
            source: self.source,
            confidence: self.confidence,
            tier: self.tier,
        })
        .await?;
        serialize_output(CognitionIdentityCommitTool::tool_id(), output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identity_actions_carry_their_params() {
        let query: IdentityQueryAction = serde_json::from_value(json!({
            "action": "identity.recall",
            "query": "Mario"
        }))
        .expect("recall");
        match query {
            IdentityQueryAction::Recall(IdentityRecall { query, .. }) => {
                assert_eq!(query, "Mario");
            }
            other => panic!("expected identity.recall, got {other:?}"),
        }
        let mutate: IdentityMutateAction = serde_json::from_value(json!({
            "action": "identity.remember",
            "fact_kind": "preference",
            "subject": "beverage",
            "statement": "matcha"
        }))
        .expect("remember");
        match mutate {
            IdentityMutateAction::Remember(IdentityRemember {
                fact_kind,
                subject,
                statement,
                ..
            }) => {
                assert_eq!(fact_kind, "preference");
                assert_eq!(subject, "beverage");
                assert_eq!(statement, "matcha");
            }
            other => panic!("expected identity.remember, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let query =
            serde_json::to_value(schemars::schema_for!(IdentityQueryAction)).expect("query");
        let mutate =
            serde_json::to_value(schemars::schema_for!(IdentityMutateAction)).expect("mutate");
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
