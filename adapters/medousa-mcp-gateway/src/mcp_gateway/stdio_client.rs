//! MCP client over a child process using the official Rust SDK transport.

use std::collections::BTreeMap;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use rmcp::model::{CallToolRequestParams, ClientInfo, Implementation, Tool};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Map, Value};
use tokio::time::{Duration, timeout};

#[derive(Debug, Clone, Default)]
pub struct McpToolAnnotations {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct McpToolDefinition {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub annotations: Option<McpToolAnnotations>,
    pub icons: Option<Value>,
    pub meta: Option<BTreeMap<String, Value>>,
}

pub struct StdioMcpSession {
    client: RunningService<RoleClient, ClientInfo>,
    request_timeout: Duration,
}

impl StdioMcpSession {
    pub async fn spawn(command: &str, args: &[String], request_timeout: Duration) -> Result<Self> {
        let transport =
            TokioChildProcess::new(tokio::process::Command::new(command).configure(|child| {
                child.args(args).stderr(Stdio::null()).kill_on_drop(true);
            }))
            .with_context(|| format!("failed to spawn MCP server command '{command}'"))?;

        let client = timeout(request_timeout, medousa_client_info().serve(transport))
            .await
            .context("MCP initialize timed out")?
            .context("MCP initialize failed")?;
        Ok(Self {
            client,
            request_timeout,
        })
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        let tools = timeout(self.request_timeout, self.client.list_all_tools())
            .await
            .context("MCP tools/list timed out")?
            .context("MCP tools/list failed")?;
        tools.into_iter().map(tool_definition_from_sdk).collect()
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let arguments = match arguments {
            Value::Object(arguments) => arguments,
            Value::Null => Map::new(),
            _ => bail!("MCP tool arguments must be a JSON object"),
        };
        let result = timeout(
            self.request_timeout,
            self.client
                .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(arguments)),
        )
        .await
        .context("MCP tools/call timed out")?
        .context("MCP tools/call failed")?;
        serde_json::to_value(result).context("failed to encode MCP tool result")
    }
}

pub(super) fn medousa_client_info() -> ClientInfo {
    let mut info = ClientInfo::default();
    info.client_info =
        Implementation::new("medousa-mcp-gateway", env!("CARGO_PKG_VERSION")).with_title("Medousa");
    info
}

pub(super) fn tool_definition_from_sdk(tool: Tool) -> Result<McpToolDefinition> {
    let title = tool
        .title
        .clone()
        .or_else(|| {
            tool.annotations
                .as_ref()
                .and_then(|annotations| annotations.title.clone())
        })
        .unwrap_or_else(|| tool.name.to_string());
    let icons = tool
        .icons
        .map(serde_json::to_value)
        .transpose()
        .context("failed to encode MCP tool icons")?;
    let meta = tool.meta.map(|meta| meta.0.into_iter().collect());
    let annotations = tool.annotations.map(|annotations| McpToolAnnotations {
        read_only_hint: annotations.read_only_hint,
        destructive_hint: annotations.destructive_hint,
        idempotent_hint: annotations.idempotent_hint,
        open_world_hint: annotations.open_world_hint,
    });
    Ok(McpToolDefinition {
        title,
        name: tool.name.to_string(),
        description: tool.description.map(|description| description.to_string()),
        input_schema: Some(Value::Object((*tool.input_schema).clone())),
        output_schema: tool
            .output_schema
            .map(|schema| Value::Object((*schema).clone())),
        annotations,
        icons,
        meta,
    })
}

pub(crate) fn parse_tool_list(response: &Value) -> Result<Vec<McpToolDefinition>> {
    let tools = response
        .get("result")
        .and_then(|value| value.get("tools"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(tools
        .into_iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.to_string();
            let annotations = tool.get("annotations").and_then(Value::as_object);
            let title = tool
                .get("title")
                .or_else(|| annotations.and_then(|value| value.get("title")))
                .or_else(|| tool.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(name.as_str())
                .to_string();
            Some(McpToolDefinition {
                name,
                title,
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                input_schema: tool.get("inputSchema").cloned(),
                output_schema: tool.get("outputSchema").cloned(),
                annotations: annotations.map(|annotations| McpToolAnnotations {
                    read_only_hint: annotations.get("readOnlyHint").and_then(Value::as_bool),
                    destructive_hint: annotations.get("destructiveHint").and_then(Value::as_bool),
                    idempotent_hint: annotations.get("idempotentHint").and_then(Value::as_bool),
                    open_world_hint: annotations.get("openWorldHint").and_then(Value::as_bool),
                }),
                icons: tool.get("icons").cloned(),
                meta: tool
                    .get("_meta")
                    .and_then(Value::as_object)
                    .map(|meta| meta.clone().into_iter().collect()),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::{MetaObject, Tool, ToolAnnotations};
    use serde_json::{Map, Value, json};

    use super::tool_definition_from_sdk;

    #[test]
    fn sdk_tool_conversion_preserves_annotations_schemas_and_app_metadata() {
        let mut meta = Map::new();
        meta.insert(
            "ui".to_string(),
            json!({ "resourceUri": "ui://weather/dashboard" }),
        );
        let tool = Tool::new("weather", "Shows a forecast", Arc::new(Map::new()))
            .with_title("Weather dashboard")
            .with_raw_output_schema(Arc::new(Map::from_iter([(
                "type".to_string(),
                Value::String("object".to_string()),
            )])))
            .with_annotations(ToolAnnotations::new().read_only(true).open_world(true))
            .with_meta(MetaObject(meta));

        let converted = tool_definition_from_sdk(tool).expect("convert tool");
        assert_eq!(converted.title, "Weather dashboard");
        assert_eq!(
            converted
                .meta
                .as_ref()
                .and_then(|meta| meta.get("ui"))
                .and_then(|ui| ui.get("resourceUri"))
                .and_then(Value::as_str),
            Some("ui://weather/dashboard")
        );
        assert_eq!(
            converted
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.read_only_hint),
            Some(true)
        );
        assert_eq!(converted.output_schema, Some(json!({ "type": "object" })));
    }
}
