use serde::{Deserialize, Serialize};

use crate::daemon_api::GraphemeScriptEntryDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeAllowlistResponse {
    pub allowed_modules: Vec<String>,
    pub enforce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeAllowlistUpdateRequest {
    pub allowed_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeCompileRequest {
    pub source: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeCompileResponse {
    pub mode: String,
    pub validated: bool,
    pub artifact_id: Option<String>,
    pub lint_warnings: Vec<String>,
    pub compile_hints: Vec<String>,
    pub aot_stage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeModuleLoadRequest {
    pub module_id: String,
    pub wasm_path: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub abi: Option<String>,
    #[serde(default)]
    pub compatibility_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeModuleLoadResponse {
    pub module_id: String,
    pub generation_id: u64,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeLifecycleEventDto {
    pub kind: String,
    pub module_id: String,
    #[serde(default)]
    pub generation_id: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeLifecycleResponse {
    pub events: Vec<GraphemeLifecycleEventDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeLspWorkspaceResponse {
    pub root_path: String,
    pub root_uri: String,
    pub scripts_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeScriptSaveRequest {
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub source_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeScriptSaveResponse {
    pub script: GraphemeScriptEntryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeScriptDeleteResponse {
    pub deleted: bool,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GraphemeScriptRenameRequest {
    pub name: String,
}
