//! Public capability primitive: find or invoke.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use std::sync::Arc;

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::{Value, json};
use stasis::prelude::{RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::bridge_tools::{
    BridgeObject, CapabilityInvokeInput, CognitionCapabilityInvokeTool,
    CognitionGraphemeTemplateRunTool, GraphemeTemplateInput, GraphemeTemplateRunInput,
};
use crate::capability_catalog::CapabilityRegistry;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::mcp_gateway_client::McpGatewayClient;
use crate::public_api::COGNITION_CAPABILITY;
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::tools::{
    CapabilityListInput, CapabilityResolveInput, CapabilitySearchInput, CognitionCapabilityListTool,
    CognitionCapabilityResolveTool, CognitionCapabilitySearchTool, CognitionGraphemeExamplesTool,
    CognitionGraphemeModulesInfoTool, CognitionGraphemeModulesOpsTool,
    CognitionGraphemeModulesSearchTool, CognitionGraphemeRunTool, CognitionMcpDiscoverTool,
    CognitionMcpInvokeTool, CognitionMcpServersTool, GraphemeExamplesActionInput,
    GraphemeExamplesInput, GraphemeModulesInfoInput, GraphemeModulesOpsInput,
    GraphemeModulesSearchInput, GraphemeRunInput, McpDiscoverInput, McpInvokeInput,
    McpInvokeObject, McpServersInput,
};
use crate::typed_tools::{
    CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};

const CAPABILITY_ID: ToolId = ToolId::new(COGNITION_CAPABILITY);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CapabilityDetail {
    Summary,
    #[default]
    Full,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum CapabilityAction {
    #[serde(rename = "capability.find")]
    CapabilityFind(CapabilityFind),
    #[serde(rename = "mcp.find")]
    McpFind(McpFind),
    #[serde(rename = "grapheme.find")]
    GraphemeFind(GraphemeFind),
    #[serde(rename = "capability.invoke")]
    CapabilityInvoke(CapabilityInvoke),
    #[serde(rename = "mcp.invoke")]
    McpInvoke(McpInvoke),
    #[serde(rename = "grapheme.invoke")]
    GraphemeInvoke(GraphemeInvoke),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CapabilityFind {
    /// Resolve this catalog id
    #[serde(default)]
    capability: Option<String>,
    /// Search text if capability is omitted
    #[serde(default)]
    query: Option<String>,
    /// Search hit cap
    #[serde(default)]
    limit: Option<usize>,
    /// Prefix filter when listing the catalog
    #[serde(default)]
    prefix: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct McpFind {
    /// Tool search; omit to list servers
    #[serde(default)]
    query: Option<String>,
    /// Limit discover to this server
    #[serde(default)]
    server_id: Option<String>,
    /// Hit cap
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GraphemeFind {
    /// Module id — returns info and ops when detail=full
    #[serde(default)]
    module: Option<String>,
    /// Example name
    #[serde(default)]
    name: Option<String>,
    /// Search modules
    #[serde(default)]
    query: Option<String>,
    /// full (default) includes ops; summary is metadata only
    #[serde(default)]
    detail: CapabilityDetail,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CapabilityInvoke {
    /// Catalog id
    #[serde(default)]
    capability: Option<String>,
    /// Resolve by search if capability omitted
    #[serde(default)]
    query: Option<String>,
    /// Inline Grapheme if the binding is a script
    #[serde(default)]
    script: Option<String>,
    /// MCP arguments when the binding is MCP
    #[serde(default)]
    input: Option<Value>,
    /// Try fallback bindings
    #[serde(default)]
    try_fallbacks: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct McpInvoke {
    /// MCP server id
    server_id: String,
    /// MCP tool name
    tool_name: String,
    /// Tool arguments
    #[serde(default)]
    input: Option<Value>,
    /// external_read is parallel-safe
    #[serde(default)]
    #[allow(dead_code)]
    effect_class: Option<String>,
    #[serde(default)]
    turn_token: Option<String>,
    #[serde(default)]
    approval_granted: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemeInvoke {
    /// Named template (e.g. http_poll, web_research)
    #[serde(default)]
    template: Option<GraphemeTemplateInput>,
    /// Template params
    #[serde(default)]
    params: Option<Value>,
    /// Inline Grapheme source if no template
    #[serde(default)]
    script: Option<String>,
}

impl JsonSchema for CapabilityAction {
    fn schema_name() -> String {
        "CapabilityAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "capability.find",
                "mcp.find",
                "grapheme.find",
                "capability.invoke",
                "mcp.invoke",
                "grapheme.invoke",
            ]),
            true,
        )])
    }
}

pub fn capability_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<CapabilityFind>(
            CAPABILITY_ID,
            "capability.find",
            "Search or resolve the capability catalog",
        ),
        typed_action_schema::<McpFind>(
            CAPABILITY_ID,
            "mcp.find",
            "List MCP servers or discover tools",
        ),
        typed_action_schema::<GraphemeFind>(
            CAPABILITY_ID,
            "grapheme.find",
            "Grapheme modules, ops, or examples",
        ),
        typed_action_schema::<CapabilityInvoke>(
            CAPABILITY_ID,
            "capability.invoke",
            "Run a catalog capability (auto-picks Grapheme or MCP binding)",
        ),
        typed_action_schema::<McpInvoke>(CAPABILITY_ID, "mcp.invoke", "Invoke one MCP tool"),
        typed_action_schema::<GraphemeInvoke>(
            CAPABILITY_ID,
            "grapheme.invoke",
            "Run a Grapheme template or inline script",
        ),
    ]
}

pub struct CognitionCapabilityTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    model_target: GraphemeCompactionModelTarget,
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    mcp_gateway_client: Arc<McpGatewayClient>,
}

#[allow(clippy::too_many_arguments)]
pub fn register_capability_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    model_target: GraphemeCompactionModelTarget,
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    mcp_gateway_client: Arc<McpGatewayClient>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionCapabilityTool {
        runtime,
        event_tx,
        session_id,
        turn_scope,
        model_target,
        capability_registry,
        mcp_gateway_client,
    })?;
    Ok(())
}

#[medousa_tool(id = CAPABILITY_ID)]
impl CognitionCapabilityTool {
    /// Find or run a capability, MCP tool, or Grapheme module. action is a typed name (capability.find, grapheme.invoke, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, action: CapabilityAction) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch(self, action).await?))
    }
}

async fn dispatch(
    tool: &CognitionCapabilityTool,
    action: CapabilityAction,
) -> stasis::prelude::Result<Value> {
    match action {
        CapabilityAction::CapabilityFind(params) => params.execute(tool).await,
        CapabilityAction::McpFind(params) => params.execute(tool).await,
        CapabilityAction::GraphemeFind(params) => params.execute(tool).await,
        CapabilityAction::CapabilityInvoke(params) => params.execute(tool).await,
        CapabilityAction::McpInvoke(params) => params.execute(tool).await,
        CapabilityAction::GraphemeInvoke(params) => params.execute(tool).await,
    }
}

impl CapabilityFind {
    async fn execute(self, tool: &CognitionCapabilityTool) -> stasis::prelude::Result<Value> {
        if present(self.capability.as_deref()) {
            let output = CognitionCapabilityResolveTool::new(
                tool.capability_registry.clone(),
                tool.event_tx.clone(),
            )
            .invoke_typed(CapabilityResolveInput {
                capability: CompatOption::from(self.capability),
                query: CompatOption::from(self.query),
            })
            .await?;
            return serialize_output(CognitionCapabilityResolveTool::tool_id(), output);
        }
        if present(self.query.as_deref()) {
            let output = CognitionCapabilitySearchTool::new(
                tool.capability_registry.clone(),
                tool.event_tx.clone(),
            )
            .invoke_typed(CapabilitySearchInput {
                query: self.query,
                limit: self.limit,
            })
            .await?;
            return serialize_output(CognitionCapabilitySearchTool::tool_id(), output);
        }
        let output = CognitionCapabilityListTool::new(tool.capability_registry.clone())
            .invoke_typed(CapabilityListInput {
                prefix: CompatOption::from(self.prefix),
                limit: CompatOption::from(self.limit),
            })
            .await?;
        serialize_output(CognitionCapabilityListTool::tool_id(), output)
    }
}

impl McpFind {
    async fn execute(self, tool: &CognitionCapabilityTool) -> stasis::prelude::Result<Value> {
        if present(self.query.as_deref()) {
            let output = CognitionMcpDiscoverTool::new(
                tool.mcp_gateway_client.clone(),
                tool.session_id.clone(),
                tool.turn_scope.clone(),
                tool.event_tx.clone(),
            )
            .invoke_typed(McpDiscoverInput {
                query: self.query,
                server_id: self.server_id,
                limit: self.limit,
            })
            .await?;
            return Ok(output.into_value());
        }
        let output = CognitionMcpServersTool::new(tool.mcp_gateway_client.clone())
            .invoke_typed(McpServersInput {})
            .await?;
        Ok(output.into_value())
    }
}

impl GraphemeFind {
    async fn execute(self, tool: &CognitionCapabilityTool) -> stasis::prelude::Result<Value> {
        if present(self.module.as_deref()) {
            let module = self.module.expect("present");
            let info = CognitionGraphemeModulesInfoTool::new(tool.event_tx.clone())
                .invoke_typed(GraphemeModulesInfoInput {
                    module: Some(module.clone()),
                })
                .await?
                .into_value();
            if matches!(self.detail, CapabilityDetail::Summary) {
                return Ok(info);
            }
            let ops = CognitionGraphemeModulesOpsTool::new(tool.event_tx.clone())
                .invoke_typed(GraphemeModulesOpsInput {
                    query: Some(module.clone()),
                })
                .await?
                .into_value();
            return Ok(json!({
                "module": module,
                "detail": "full",
                "info": info,
                "ops": ops,
            }));
        }
        if present(self.name.as_deref()) {
            let output = CognitionGraphemeExamplesTool::new(tool.event_tx.clone())
                .invoke_typed(GraphemeExamplesInput {
                    action: GraphemeExamplesActionInput::Show,
                    name: self.name,
                })
                .await?;
            return Ok(output.into_value());
        }
        if present(self.query.as_deref()) {
            let output = CognitionGraphemeModulesSearchTool::new(tool.event_tx.clone())
                .invoke_typed(GraphemeModulesSearchInput { query: self.query })
                .await?;
            return Ok(output.into_value());
        }
        let output = CognitionGraphemeExamplesTool::new(tool.event_tx.clone())
            .invoke_typed(GraphemeExamplesInput {
                action: GraphemeExamplesActionInput::List,
                name: None,
            })
            .await?;
        Ok(output.into_value())
    }
}

impl CapabilityInvoke {
    async fn execute(self, tool: &CognitionCapabilityTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCapabilityInvokeTool::new(
            tool.capability_registry.clone(),
            tool.runtime.clone(),
            tool.mcp_gateway_client.clone(),
            tool.session_id.clone(),
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(CapabilityInvokeInput {
            capability: self.capability,
            query: self.query,
            input: self.input.map(BridgeObject::from_value),
            source: self.script,
            binding: None,
            preferred_source: None,
            try_fallbacks: self.try_fallbacks.unwrap_or(true),
            extra: Default::default(),
        })
        .await?;
        serialize_output(CognitionCapabilityInvokeTool::tool_id(), output)
    }
}

impl McpInvoke {
    async fn execute(self, tool: &CognitionCapabilityTool) -> stasis::prelude::Result<Value> {
        let output = CognitionMcpInvokeTool::new(
            tool.mcp_gateway_client.clone(),
            tool.session_id.clone(),
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke_typed(McpInvokeInput {
            server_id: Some(self.server_id),
            tool_name: Some(self.tool_name),
            input: self.input.map(McpInvokeObject::from_value),
            turn_token: self.turn_token,
            approval_granted: self.approval_granted,
        })
        .await?;
        Ok(output.into_value())
    }
}

impl GraphemeInvoke {
    async fn execute(self, tool: &CognitionCapabilityTool) -> stasis::prelude::Result<Value> {
        if let Some(template) = self.template {
            let output = CognitionGraphemeTemplateRunTool::new(
                tool.runtime.clone(),
                tool.event_tx.clone(),
            )
            .invoke_typed(GraphemeTemplateRunInput {
                template: Some(template),
                params: self.params.map(BridgeObject::from_value),
            })
            .await?;
            return serialize_output(CognitionGraphemeTemplateRunTool::tool_id(), output);
        }
        if present(self.script.as_deref()) {
            let output = CognitionGraphemeRunTool::new(
                tool.runtime.clone(),
                tool.event_tx.clone(),
                tool.session_id.clone(),
                tool.model_target.clone(),
                tool.turn_scope.clone(),
            )
            .invoke_typed(GraphemeRunInput {
                source: self.script,
            })
            .await?;
            return Ok(output.into_value());
        }
        Err(StasisError::PortFailure(
            "cognition_capability: grapheme.invoke needs template or script".to_string(),
        ))
    }
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_actions_carry_their_params() {
        let find: CapabilityAction = serde_json::from_value(json!({
            "action": "grapheme.find",
            "module": "web"
        }))
        .expect("grapheme find");
        match find {
            CapabilityAction::GraphemeFind(GraphemeFind { module, detail, .. }) => {
                assert_eq!(module.as_deref(), Some("web"));
                assert_eq!(detail, CapabilityDetail::Full);
            }
            other => panic!("expected grapheme.find, got {other:?}"),
        }
        let invoke: CapabilityAction = serde_json::from_value(json!({
            "action": "mcp.invoke",
            "server_id": "web",
            "tool_name": "search"
        }))
        .expect("mcp invoke");
        match invoke {
            CapabilityAction::McpInvoke(McpInvoke {
                server_id,
                tool_name,
                ..
            }) => {
                assert_eq!(server_id, "web");
                assert_eq!(tool_name, "search");
            }
            other => panic!("expected mcp.invoke, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schema_is_action_enum_only() {
        let schema =
            serde_json::to_value(schemars::schema_for!(CapabilityAction)).expect("schema");
        let props = schema["properties"].as_object().expect("properties");
        assert_eq!(props.len(), 1);
        assert!(props.contains_key("action"));
        assert_eq!(schema["additionalProperties"], true);
    }
}
