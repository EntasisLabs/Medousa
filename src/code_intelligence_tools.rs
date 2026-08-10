//! Cognition tools that query the LSP Interoperability Orchestrator (medousa-code).

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, StasisError};

use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};

pub const COGNITION_CODE_HOVER: &str = "cognition_code_hover";
pub const COGNITION_CODE_DEFINITION: &str = "cognition_code_definition";
pub const COGNITION_CODE_DIAGNOSTICS: &str = "cognition_code_diagnostics";
pub const COGNITION_CODE_SYMBOLS: &str = "cognition_code_symbols";

const COGNITION_CODE_HOVER_ID: ToolId = ToolId::new(COGNITION_CODE_HOVER);
const COGNITION_CODE_DEFINITION_ID: ToolId = ToolId::new(COGNITION_CODE_DEFINITION);
const COGNITION_CODE_DIAGNOSTICS_ID: ToolId = ToolId::new(COGNITION_CODE_DIAGNOSTICS);
const COGNITION_CODE_SYMBOLS_ID: ToolId = ToolId::new(COGNITION_CODE_SYMBOLS);

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

async fn proxy(
    path: &str,
    uri: &str,
    line: Option<u32>,
    character: Option<u32>,
    work_id: Option<&str>,
    attempt_id: Option<&str>,
) -> StasisResult<Value> {
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
        if let Some(work_id) = work_id {
            q.append_pair("work_id", work_id);
        }
        if let Some(attempt_id) = attempt_id {
            q.append_pair("attempt_id", attempt_id);
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

pub(crate) async fn request_code_action(input: Value) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/code/request", daemon_base().trim_end_matches('/'));
    let response = client
        .post(url)
        .json(&input)
        .send()
        .await
        .map_err(|error| StasisError::PortFailure(format!("coding engine proxy: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(StasisError::PortFailure(format!(
            "coding engine {status}: {body}"
        )));
    }
    response
        .json()
        .await
        .map_err(|error| StasisError::PortFailure(error.to_string()))
}

pub struct CognitionCodeHoverTool;
pub struct CognitionCodeDefinitionTool;
pub struct CognitionCodeDiagnosticsTool;
pub struct CognitionCodeSymbolsTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CodeLocationInput {
    /// file:// document URI
    pub uri: String,
    /// 0-based line
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub line: CompatOption<u64>,
    /// 0-based character
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub character: CompatOption<u64>,
    /// Runtime-bound Forge authority, hidden from the advertised base contract.
    #[serde(default)]
    #[schemars(skip)]
    pub work_id: CompatOption<String>,
    /// Runtime-bound attempt authority, hidden from the advertised base contract.
    #[serde(default)]
    #[schemars(skip)]
    pub attempt_id: CompatOption<String>,
}

impl CodeLocationInput {
    fn validated_uri(&self) -> StasisResult<&str> {
        (!self.uri.trim().is_empty())
            .then_some(self.uri.as_str())
            .ok_or_else(|| StasisError::PortFailure("uri is required (file://…)".into()))
    }

    fn line_char(&self) -> (Option<u32>, Option<u32>) {
        (
            self.line.as_ref().copied().map(|value| value as u32),
            self.character.as_ref().copied().map(|value| value as u32),
        )
    }

    async fn invoke_proxy(self, path: &str) -> StasisResult<ExternalJson> {
        let uri = self.validated_uri()?.to_string();
        let (line, character) = self.line_char();
        let work_id = self.work_id.into_option();
        let attempt_id = self.attempt_id.into_option();
        proxy(
            path,
            &uri,
            line,
            character,
            work_id.as_deref(),
            attempt_id.as_deref(),
        )
        .await
        .map(ExternalJson::new)
    }
}

#[medousa_tool(id = COGNITION_CODE_HOVER_ID)]
impl CognitionCodeHoverTool {
    /// Hover info from the LSP Interoperability Orchestrator (medousa-code).
    async fn invoke_typed(
        &self,
        input: CodeLocationInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        input.invoke_proxy("/v1/code/hover").await
    }
}

#[medousa_tool(id = COGNITION_CODE_DEFINITION_ID)]
impl CognitionCodeDefinitionTool {
    /// Go-to-definition via the coding engine Orchestrator.
    async fn invoke_typed(
        &self,
        input: CodeLocationInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        input.invoke_proxy("/v1/code/definition").await
    }
}

#[medousa_tool(id = COGNITION_CODE_DIAGNOSTICS_ID)]
impl CognitionCodeDiagnosticsTool {
    /// Open-document diagnostics status from the coding engine.
    async fn invoke_typed(
        &self,
        input: CodeLocationInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        input.invoke_proxy("/v1/code/diagnostics").await
    }
}

#[medousa_tool(id = COGNITION_CODE_SYMBOLS_ID)]
impl CognitionCodeSymbolsTool {
    /// Document symbols via the coding engine Orchestrator.
    async fn invoke_typed(
        &self,
        input: CodeLocationInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        input.invoke_proxy("/v1/code/symbols").await
    }
}

pub fn register_code_intelligence_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionCodeHoverTool)?;
    registry.register_typed_tool(CognitionCodeDefinitionTool)?;
    registry.register_typed_tool(CognitionCodeDiagnosticsTool)?;
    registry.register_typed_tool(CognitionCodeSymbolsTool)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_location_wire_optionals_remain_lenient_for_legacy_values() {
        let input: CodeLocationInput = serde_json::from_value(serde_json::json!({
            "uri": "file:///workspace/main.rs",
            "line": "10",
            "character": false,
            "work_id": 42,
            "attempt_id": " attempt-1 ",
        }))
        .expect("code location input");
        assert_eq!(
            input.validated_uri().expect("uri"),
            "file:///workspace/main.rs"
        );
        assert_eq!(input.line_char(), (None, None));
        assert!(input.work_id.into_option().is_none());
        assert_eq!(
            input.attempt_id.into_option().as_deref(),
            Some(" attempt-1 ")
        );
    }
}
