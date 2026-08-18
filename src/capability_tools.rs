//! Public capability primitive: find or invoke, backend selected by source enum.

use std::sync::Arc;

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::bridge_tools::{CognitionCapabilityInvokeTool, CognitionGraphemeTemplateRunTool};
use crate::capability_catalog::CapabilityRegistry;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::mcp_gateway_client::McpGatewayClient;
use crate::public_api::COGNITION_CAPABILITY;
use crate::schema_api::{advertised_object_schema, string_enum_schema};
use crate::tools::{
    CognitionCapabilityListTool, CognitionCapabilityResolveTool, CognitionCapabilitySearchTool,
    CognitionGraphemeExamplesTool, CognitionGraphemeModulesInfoTool,
    CognitionGraphemeModulesOpsTool, CognitionGraphemeModulesSearchTool, CognitionGraphemeRunTool,
    CognitionMcpDiscoverTool, CognitionMcpInvokeTool, CognitionMcpServersTool,
};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

const CAPABILITY_ID: ToolId = ToolId::new(COGNITION_CAPABILITY);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityOp {
    Find,
    Invoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapabilitySourceKind {
    #[default]
    Auto,
    Mcp,
    Grapheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityDetail {
    Summary,
    #[default]
    Full,
}

#[derive(Debug, Deserialize)]
pub struct CapabilityInput {
    op: CapabilityOp,
    #[serde(default)]
    source: CapabilitySourceKind,
    #[serde(default)]
    detail: CapabilityDetail,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    capability: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    server_id: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    effect_class: Option<String>,
    #[serde(default)]
    try_fallbacks: Option<bool>,
    #[serde(default)]
    approval_granted: Option<bool>,
    #[serde(default)]
    turn_token: Option<String>,
    #[serde(default)]
    input: Option<Value>,
    #[serde(default)]
    params: Option<Value>,
}

impl JsonSchema for CapabilityInput {
    fn schema_name() -> String {
        "CapabilityInput".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[
            ("op", string_enum_schema(&["find", "invoke"]), true),
            (
                "source",
                string_enum_schema(&["auto", "mcp", "grapheme"]),
                false,
            ),
        ])
    }
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
    /// Find or run a capability, MCP tool, or Grapheme module. op=find|invoke. source=auto|mcp|grapheme. Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, input: CapabilityInput) -> stasis::prelude::Result<ExternalJson> {
        let value = dispatch(self, input).await?;
        Ok(ExternalJson::new(value))
    }
}

async fn dispatch(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    match (input.op, input.source) {
        (CapabilityOp::Find, CapabilitySourceKind::Auto) => find_auto(tool, input).await,
        (CapabilityOp::Find, CapabilitySourceKind::Mcp) => find_mcp(tool, input).await,
        (CapabilityOp::Find, CapabilitySourceKind::Grapheme) => find_grapheme(tool, input).await,
        (CapabilityOp::Invoke, CapabilitySourceKind::Auto) => invoke_auto(tool, input).await,
        (CapabilityOp::Invoke, CapabilitySourceKind::Mcp) => invoke_mcp(tool, input).await,
        (CapabilityOp::Invoke, CapabilitySourceKind::Grapheme) => {
            invoke_grapheme(tool, input).await
        }
    }
}

async fn find_auto(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    if input
        .capability
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionCapabilityResolveTool::new(tool.capability_registry.clone(), tool.event_tx.clone())
            .invoke(json_obj([
                ("capability", opt_str(input.capability)),
                ("query", opt_str(input.query)),
            ]))
            .await
    } else if input
        .query
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionCapabilitySearchTool::new(tool.capability_registry.clone(), tool.event_tx.clone())
            .invoke(json_obj([
                ("query", opt_str(input.query)),
                ("limit", opt_usize(input.limit)),
            ]))
            .await
    } else {
        CognitionCapabilityListTool::new(tool.capability_registry.clone())
            .invoke(json_obj([
                ("prefix", opt_str(input.prefix)),
                ("limit", opt_usize(input.limit)),
            ]))
            .await
    }
}

async fn find_mcp(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    if input
        .query
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionMcpDiscoverTool::new(
            tool.mcp_gateway_client.clone(),
            tool.session_id.clone(),
            tool.turn_scope.clone(),
            tool.event_tx.clone(),
        )
        .invoke(json_obj([
            ("query", opt_str(input.query)),
            ("server_id", opt_str(input.server_id)),
            ("limit", opt_usize(input.limit)),
        ]))
        .await
    } else {
        CognitionMcpServersTool::new(tool.mcp_gateway_client.clone())
            .invoke(json!({}))
            .await
    }
}

async fn find_grapheme(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    if let Some(module) = opt_str(input.module.clone()) {
        let info = CognitionGraphemeModulesInfoTool::new(tool.event_tx.clone())
            .invoke(json_obj([("module", Some(module.clone()))]))
            .await?;
        if matches!(input.detail, CapabilityDetail::Summary) {
            return Ok(info);
        }
        let ops = CognitionGraphemeModulesOpsTool::new(tool.event_tx.clone())
            .invoke(json_obj([("query", Some(module.clone()))]))
            .await?;
        Ok(json!({
            "module": module,
            "detail": "full",
            "info": info,
            "ops": ops,
        }))
    } else if input
        .name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionGraphemeExamplesTool::new(tool.event_tx.clone())
            .invoke(json_obj([
                ("action", Some(Value::String("show".into()))),
                ("name", opt_str(input.name)),
            ]))
            .await
    } else if input
        .query
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionGraphemeModulesSearchTool::new(tool.event_tx.clone())
            .invoke(json_obj([("query", opt_str(input.query))]))
            .await
    } else {
        CognitionGraphemeExamplesTool::new(tool.event_tx.clone())
            .invoke(json_obj([("action", Some(Value::String("list".into())))]))
            .await
    }
}

async fn invoke_auto(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    CognitionCapabilityInvokeTool::new(
        tool.capability_registry.clone(),
        tool.runtime.clone(),
        tool.mcp_gateway_client.clone(),
        tool.session_id.clone(),
        tool.turn_scope.clone(),
        tool.event_tx.clone(),
    )
    .invoke(json_obj([
        ("capability", opt_str(input.capability)),
        ("query", opt_str(input.query)),
        ("source", opt_str(input.script)),
        ("input", input.input),
        ("try_fallbacks", opt_bool(input.try_fallbacks)),
    ]))
    .await
}

async fn invoke_mcp(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    require(
        input.server_id.as_deref(),
        "cognition_capability: mcp invoke needs server_id and tool_name",
    )?;
    require(
        input.tool_name.as_deref(),
        "cognition_capability: mcp invoke needs server_id and tool_name",
    )?;
    CognitionMcpInvokeTool::new(
        tool.mcp_gateway_client.clone(),
        tool.session_id.clone(),
        tool.turn_scope.clone(),
        tool.event_tx.clone(),
    )
    .invoke(json_obj([
        ("server_id", opt_str(input.server_id)),
        ("tool_name", opt_str(input.tool_name)),
        ("input", input.input),
        ("turn_token", opt_str(input.turn_token)),
        ("approval_granted", opt_bool(input.approval_granted)),
        ("effect_class", opt_str(input.effect_class)),
    ]))
    .await
}

async fn invoke_grapheme(
    tool: &CognitionCapabilityTool,
    input: CapabilityInput,
) -> stasis::prelude::Result<Value> {
    if input
        .template
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionGraphemeTemplateRunTool::new(tool.runtime.clone(), tool.event_tx.clone())
            .invoke(json_obj([
                ("template", opt_str(input.template)),
                ("params", input.params),
            ]))
            .await
    } else if input
        .script
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        CognitionGraphemeRunTool::new(
            tool.runtime.clone(),
            tool.event_tx.clone(),
            tool.session_id.clone(),
            tool.model_target.clone(),
            tool.turn_scope.clone(),
        )
        .invoke(json_obj([("source", opt_str(input.script))]))
        .await
    } else {
        Err(StasisError::PortFailure(
            "cognition_capability: grapheme invoke needs template or script".to_string(),
        ))
    }
}

fn require(value: Option<&str>, message: &str) -> stasis::prelude::Result<()> {
    if value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        Ok(())
    } else {
        Err(StasisError::PortFailure(message.to_string()))
    }
}

fn json_obj(fields: impl IntoIterator<Item = (&'static str, Option<Value>)>) -> Value {
    let mut map = Map::new();
    for (key, value) in fields {
        if let Some(value) = value {
            map.insert(key.to_string(), value);
        }
    }
    Value::Object(map)
}

fn opt_str(value: Option<String>) -> Option<Value> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(Value::String)
}

fn opt_usize(value: Option<usize>) -> Option<Value> {
    value.map(|value| json!(value))
}

fn opt_bool(value: Option<bool>) -> Option<Value> {
    value.map(Value::Bool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_enums_are_snake_case() {
        let find: CapabilityInput = serde_json::from_value(json!({
            "op": "find",
            "source": "grapheme",
            "module": "web"
        }))
        .expect("grapheme find");
        assert_eq!(find.op, CapabilityOp::Find);
        assert_eq!(find.source, CapabilitySourceKind::Grapheme);
        assert_eq!(find.detail, CapabilityDetail::Full);
        let invoke: CapabilityInput = serde_json::from_value(json!({
            "op": "invoke",
            "source": "mcp",
            "server_id": "web",
            "tool_name": "search"
        }))
        .expect("mcp invoke");
        assert_eq!(invoke.op, CapabilityOp::Invoke);
        assert_eq!(invoke.source, CapabilitySourceKind::Mcp);
    }

    #[test]
    fn advertised_schema_is_op_and_source_only() {
        let schema = serde_json::to_value(schemars::schema_for!(CapabilityInput)).expect("schema");
        let props = schema["properties"].as_object().expect("properties");
        assert_eq!(props.len(), 2);
        assert!(props.contains_key("op"));
        assert!(props.contains_key("source"));
        assert_eq!(schema["additionalProperties"], true);
    }
}
