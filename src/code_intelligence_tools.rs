//! Cognition tools that query the LSP Interoperability Orchestrator (medousa-code).

use async_trait::async_trait;
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

pub const COGNITION_CODE_HOVER: &str = "cognition_code_hover";
pub const COGNITION_CODE_DEFINITION: &str = "cognition_code_definition";
pub const COGNITION_CODE_DIAGNOSTICS: &str = "cognition_code_diagnostics";
pub const COGNITION_CODE_SYMBOLS: &str = "cognition_code_symbols";

pub const CODE_COGNITION_TOOLS: &[&str] = &[
    COGNITION_CODE_HOVER,
    COGNITION_CODE_DEFINITION,
    COGNITION_CODE_DIAGNOSTICS,
    COGNITION_CODE_SYMBOLS,
];

pub fn is_code_cognition_tool(name: &str) -> bool {
    name.starts_with("cognition_code_")
}

fn daemon_base() -> String {
    crate::daemon_self_url::daemon_self_base_url()
}

async fn proxy(path: &str, uri: &str, line: Option<u32>, character: Option<u32>) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let mut url = reqwest::Url::parse(&format!("{}{path}", daemon_base().trim_end_matches('/')))
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("uri", uri);
        if let Some(line) = line {
            q.append_pair("line", &line.to_string());
        }
        if let Some(character) = character {
            q.append_pair("character", &character.to_string());
        }
    }
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("coding engine proxy: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(StasisError::PortFailure(format!(
            "coding engine {status}: {body}"
        )));
    }
    resp.json()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))
}

fn require_uri(input: &Value) -> StasisResult<String> {
    input
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| StasisError::PortFailure("uri is required (file://…)".into()))
}

fn line_char(input: &Value) -> (Option<u32>, Option<u32>) {
    let line = input.get("line").and_then(|v| v.as_u64()).map(|n| n as u32);
    let character = input
        .get("character")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    (line, character)
}

fn uri_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "uri": { "type": "string", "description": "file:// document URI" },
            "line": { "type": "integer", "description": "0-based line" },
            "character": { "type": "integer", "description": "0-based character" }
        },
        "required": ["uri"]
    })
}

pub struct CognitionCodeHoverTool;
pub struct CognitionCodeDefinitionTool;
pub struct CognitionCodeDiagnosticsTool;
pub struct CognitionCodeSymbolsTool;

#[async_trait]
impl StasisTool for CognitionCodeHoverTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_HOVER
    }
    fn description(&self) -> Option<&'static str> {
        Some("Hover info from the LSP Interoperability Orchestrator (medousa-code).")
    }
    fn input_schema(&self) -> Option<Value> {
        Some(uri_schema())
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let uri = require_uri(&input)?;
        let (line, character) = line_char(&input);
        proxy("/v1/code/hover", &uri, line, character).await
    }
}

#[async_trait]
impl StasisTool for CognitionCodeDefinitionTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_DEFINITION
    }
    fn description(&self) -> Option<&'static str> {
        Some("Go-to-definition via the coding engine Orchestrator.")
    }
    fn input_schema(&self) -> Option<Value> {
        Some(uri_schema())
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let uri = require_uri(&input)?;
        let (line, character) = line_char(&input);
        proxy("/v1/code/definition", &uri, line, character).await
    }
}

#[async_trait]
impl StasisTool for CognitionCodeDiagnosticsTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_DIAGNOSTICS
    }
    fn description(&self) -> Option<&'static str> {
        Some("Open-document diagnostics status from the coding engine.")
    }
    fn input_schema(&self) -> Option<Value> {
        Some(uri_schema())
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let uri = require_uri(&input)?;
        let (line, character) = line_char(&input);
        proxy("/v1/code/diagnostics", &uri, line, character).await
    }
}

#[async_trait]
impl StasisTool for CognitionCodeSymbolsTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_SYMBOLS
    }
    fn description(&self) -> Option<&'static str> {
        Some("Document symbols via the coding engine Orchestrator.")
    }
    fn input_schema(&self) -> Option<Value> {
        Some(uri_schema())
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let uri = require_uri(&input)?;
        let (line, character) = line_char(&input);
        proxy("/v1/code/symbols", &uri, line, character).await
    }
}

pub fn register_code_intelligence_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionCodeHoverTool)?;
    registry.register_tool(CognitionCodeDefinitionTool)?;
    registry.register_tool(CognitionCodeDiagnosticsTool)?;
    registry.register_tool(CognitionCodeSymbolsTool)?;
    Ok(())
}
