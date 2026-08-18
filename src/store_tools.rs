//! Public store primitives: one read tool and one write tool.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::artifact_tools::{
    ArtifactDeleteInput, ArtifactGrepInput, ArtifactListInput, ArtifactReadInput,
    ArtifactWriteInput, CognitionArtifactDeleteTool, CognitionArtifactGrepTool,
    CognitionArtifactListTool, CognitionArtifactReadTool, CognitionArtifactWriteTool,
};
use crate::coding_tools::{
    CodeApplyPatchInput, CodeReadInput, CodeSearchInput, invoke_code_apply_patch, invoke_code_read,
    invoke_code_search,
};
use crate::events::TuiEvent;
use crate::grapheme_script_tools::{
    CognitionGraphemeScriptListTool, CognitionGraphemeScriptLoadTool,
    CognitionGraphemeScriptSaveTool, CognitionGraphemeScriptSearchTool, GraphemeScriptListInput,
    GraphemeScriptLoadInput, GraphemeScriptSaveInput, GraphemeScriptSearchInput,
};
use crate::public_api::{COGNITION_STORE_READ, COGNITION_STORE_WRITE};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::typed_tools::{CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output};
use crate::vault_tools::{
    CognitionVaultDeleteTool, CognitionVaultGrepTool, CognitionVaultListTool,
    CognitionVaultMoveTool, CognitionVaultReadTool, CognitionVaultSearchTool,
    CognitionVaultTagsTool, CognitionVaultWriteTool, VaultDeleteInput, VaultGrepInput,
    VaultListInput, VaultMoveInput, VaultReadInput, VaultSearchInput, VaultTagsInput,
    VaultWriteInput,
};

const STORE_READ_ID: ToolId = ToolId::new(COGNITION_STORE_READ);
const STORE_WRITE_ID: ToolId = ToolId::new(COGNITION_STORE_WRITE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum StoreReadAction {
    #[serde(rename = "vault.list")]
    VaultList(VaultList),
    #[serde(rename = "vault.read")]
    VaultRead(VaultRead),
    #[serde(rename = "vault.search")]
    VaultSearch(VaultSearch),
    #[serde(rename = "artifacts.list")]
    ArtifactsList(ArtifactsList),
    #[serde(rename = "artifacts.read")]
    ArtifactsRead(ArtifactsRead),
    #[serde(rename = "artifacts.search")]
    ArtifactsSearch(ArtifactsSearch),
    #[serde(rename = "code.read")]
    CodeRead(CodeRead),
    #[serde(rename = "code.search")]
    CodeSearch(CodeSearch),
    #[serde(rename = "scripts.list")]
    ScriptsList(ScriptsList),
    #[serde(rename = "scripts.read")]
    ScriptsRead(ScriptsRead),
    #[serde(rename = "scripts.search")]
    ScriptsSearch(ScriptsSearch),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum StoreWriteAction {
    #[serde(rename = "vault.write")]
    VaultWrite(VaultWrite),
    #[serde(rename = "vault.delete")]
    VaultDelete(VaultDelete),
    #[serde(rename = "vault.move")]
    VaultMove(VaultMove),
    #[serde(rename = "artifacts.write")]
    ArtifactsWrite(ArtifactsWrite),
    #[serde(rename = "artifacts.delete")]
    ArtifactsDelete(ArtifactsDelete),
    #[serde(rename = "code.write")]
    CodeWrite(CodeWrite),
    #[serde(rename = "scripts.write")]
    ScriptsWrite(ScriptsWrite),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct VaultList {
    /// Path prefix
    #[serde(default)]
    prefix: Option<String>,
    /// Tag filter
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    /// Tag prefix; facet=tags lists tags
    #[serde(default)]
    tag_prefix: Option<String>,
    /// Set tags to list tag names
    #[serde(default)]
    facet: Option<String>,
    /// Max rows
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultRead {
    /// Note path
    path: String,
    /// Truncate body
    #[serde(default)]
    max_chars: Option<usize>,
    /// 1-based start line
    #[serde(default)]
    line_start: Option<usize>,
    /// 1-based end line
    #[serde(default)]
    line_end: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultSearch {
    /// Search text or grep pattern
    #[serde(default)]
    query: Option<String>,
    /// If set, grep this file
    #[serde(default)]
    path: Option<String>,
    /// Tag filter for corpus search
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    /// Grep context
    #[serde(default)]
    context_lines: Option<usize>,
    /// Max hits
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ArtifactsList {
    /// Title/id substring
    #[serde(default)]
    query: Option<String>,
    /// Max rows
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactsRead {
    /// Artifact id
    path: String,
    /// Truncate body
    #[serde(default)]
    max_chars: Option<usize>,
    /// 1-based start line
    #[serde(default)]
    line_start: Option<usize>,
    /// 1-based end line
    #[serde(default)]
    line_end: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactsSearch {
    /// Artifact id
    path: String,
    /// Grep pattern
    query: String,
    /// Grep context
    #[serde(default)]
    context_lines: Option<usize>,
    /// Max hits
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CodeRead {
    /// Path relative to root
    path: String,
    /// Worktree root; Coder binds this
    #[serde(default)]
    root: Option<String>,
    /// 1-based start line
    #[serde(default)]
    line_start: Option<usize>,
    /// 1-based end line
    #[serde(default)]
    line_end: Option<usize>,
    #[serde(default)]
    byte_start: Option<u64>,
    #[serde(default)]
    byte_end: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CodeSearch {
    /// Search text
    query: String,
    /// Worktree root; Coder binds this
    #[serde(default)]
    root: Option<String>,
    /// Hit cap
    #[serde(default)]
    max_results: Option<u64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ScriptsList {
    /// Module filter
    #[serde(default)]
    module: Option<String>,
    /// Tag filter
    #[serde(default)]
    tag: Option<String>,
    /// Max rows
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScriptsRead {
    /// Script id
    path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScriptsSearch {
    /// Search text
    #[serde(default)]
    query: Option<String>,
    /// Module filter
    #[serde(default)]
    module: Option<String>,
    /// Tag filter
    #[serde(default)]
    tag: Option<String>,
    /// Max rows
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultWrite {
    /// Note path
    path: String,
    /// Markdown body
    content: String,
    /// Tags
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    /// Optimistic concurrency token
    #[serde(default)]
    if_match: Option<String>,
    /// Merge workshop default tags
    #[serde(default)]
    auto_workshop_tags: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultDelete {
    /// Note path
    path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultMove {
    /// From path
    path: String,
    /// To path
    to_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactsWrite {
    /// HTML body
    content: String,
    /// Artifact title
    #[serde(default)]
    title: Option<String>,
    /// Existing artifact id to revise
    #[serde(default)]
    path: Option<String>,
    /// inline, panel, or fullscreen
    #[serde(default)]
    presentation: Option<String>,
    /// Preferred height
    #[serde(default)]
    height: Option<u64>,
    /// Hash of the artifact being revised
    #[serde(default)]
    if_match: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ArtifactsDelete {
    /// Artifact id
    path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CodeWrite {
    /// Path relative to root
    path: String,
    /// Hash from read, or missing for a new file
    expected_sha256: String,
    /// Full file contents
    #[serde(default)]
    content: Option<String>,
    /// Patch find text
    #[serde(default)]
    find: Option<String>,
    /// Patch replace text
    #[serde(default)]
    replace: Option<String>,
    /// Worktree root; Coder binds this
    #[serde(default)]
    root: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScriptsWrite {
    /// Grapheme source
    content: String,
    /// Script id
    #[serde(default)]
    path: Option<String>,
    /// Display name
    #[serde(default)]
    name: Option<String>,
    /// Module names
    #[serde(default)]
    modules: Option<Vec<String>>,
    /// Tags
    #[serde(default)]
    tags: Option<Vec<String>>,
    /// Why this script exists
    #[serde(default)]
    script_intent: Option<String>,
}

impl JsonSchema for StoreReadAction {
    fn schema_name() -> String {
        "StoreReadAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "vault.list",
                "vault.read",
                "vault.search",
                "artifacts.list",
                "artifacts.read",
                "artifacts.search",
                "code.read",
                "code.search",
                "scripts.list",
                "scripts.read",
                "scripts.search",
            ]),
            true,
        )])
    }
}

impl JsonSchema for StoreWriteAction {
    fn schema_name() -> String {
        "StoreWriteAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "vault.write",
                "vault.delete",
                "vault.move",
                "artifacts.write",
                "artifacts.delete",
                "code.write",
                "scripts.write",
            ]),
            true,
        )])
    }
}

pub fn store_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<VaultList>(STORE_READ_ID, "vault.list", "List vault notes"),
        typed_action_schema::<VaultRead>(STORE_READ_ID, "vault.read", "Read a vault note"),
        typed_action_schema::<VaultSearch>(
            STORE_READ_ID,
            "vault.search",
            "Search vault notes; path set = in-file grep",
        ),
        typed_action_schema::<ArtifactsList>(STORE_READ_ID, "artifacts.list", "List HTML artifacts"),
        typed_action_schema::<ArtifactsRead>(STORE_READ_ID, "artifacts.read", "Read an HTML artifact"),
        typed_action_schema::<ArtifactsSearch>(
            STORE_READ_ID,
            "artifacts.search",
            "Grep one HTML artifact",
        ),
        typed_action_schema::<CodeRead>(
            STORE_READ_ID,
            "code.read",
            "Read a file in the bound worktree",
        ),
        typed_action_schema::<CodeSearch>(
            STORE_READ_ID,
            "code.search",
            "Search the bound worktree",
        ),
        typed_action_schema::<ScriptsList>(
            STORE_READ_ID,
            "scripts.list",
            "List saved Grapheme scripts",
        ),
        typed_action_schema::<ScriptsRead>(
            STORE_READ_ID,
            "scripts.read",
            "Load a saved Grapheme script",
        ),
        typed_action_schema::<ScriptsSearch>(
            STORE_READ_ID,
            "scripts.search",
            "Search saved Grapheme scripts",
        ),
        typed_action_schema::<VaultWrite>(STORE_WRITE_ID, "vault.write", "Write a vault note"),
        typed_action_schema::<VaultDelete>(STORE_WRITE_ID, "vault.delete", "Delete a vault note"),
        typed_action_schema::<VaultMove>(STORE_WRITE_ID, "vault.move", "Move a vault note"),
        typed_action_schema::<ArtifactsWrite>(
            STORE_WRITE_ID,
            "artifacts.write",
            "Create or revise an HTML artifact",
        ),
        typed_action_schema::<ArtifactsDelete>(
            STORE_WRITE_ID,
            "artifacts.delete",
            "Delete an HTML artifact",
        ),
        typed_action_schema::<CodeWrite>(
            STORE_WRITE_ID,
            "code.write",
            "Write or patch a file in the bound worktree",
        ),
        typed_action_schema::<ScriptsWrite>(STORE_WRITE_ID, "scripts.write", "Save a Grapheme script"),
    ]
}

pub struct CognitionStoreReadTool {
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

pub struct CognitionStoreWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    fallback_chat_session_id: String,
}

pub fn register_store_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    fallback_chat_session_id: String,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionStoreReadTool {
        event_tx: event_tx.clone(),
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_typed_tool(CognitionStoreWriteTool {
        event_tx,
        turn_scope,
        fallback_chat_session_id,
    })?;
    Ok(())
}

#[medousa_tool(id = STORE_READ_ID)]
impl CognitionStoreReadTool {
    /// Read or search vault notes, HTML artifacts, code, or saved Grapheme scripts. action is a typed name (vault.read, artifacts.search, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, action: StoreReadAction) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_read(self, action).await?))
    }
}

#[medousa_tool(id = STORE_WRITE_ID)]
impl CognitionStoreWriteTool {
    /// Create, update, delete, or move vault notes, HTML artifacts, code, or saved Grapheme scripts. action is a typed name (vault.write, artifacts.write, …). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, action: StoreWriteAction) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_write(self, action).await?))
    }
}

async fn dispatch_read(
    tool: &CognitionStoreReadTool,
    action: StoreReadAction,
) -> stasis::prelude::Result<Value> {
    match action {
        StoreReadAction::VaultList(params) => params.execute(tool).await,
        StoreReadAction::VaultRead(params) => params.execute(tool).await,
        StoreReadAction::VaultSearch(params) => params.execute(tool).await,
        StoreReadAction::ArtifactsList(params) => params.execute(tool).await,
        StoreReadAction::ArtifactsRead(params) => params.execute(tool).await,
        StoreReadAction::ArtifactsSearch(params) => params.execute(tool).await,
        StoreReadAction::CodeRead(params) => params.execute().await,
        StoreReadAction::CodeSearch(params) => params.execute().await,
        StoreReadAction::ScriptsList(params) => params.execute(tool).await,
        StoreReadAction::ScriptsRead(params) => params.execute(tool).await,
        StoreReadAction::ScriptsSearch(params) => params.execute(tool).await,
    }
}

async fn dispatch_write(
    tool: &CognitionStoreWriteTool,
    action: StoreWriteAction,
) -> stasis::prelude::Result<Value> {
    match action {
        StoreWriteAction::VaultWrite(params) => params.execute(tool).await,
        StoreWriteAction::VaultDelete(params) => params.execute(tool).await,
        StoreWriteAction::VaultMove(params) => params.execute(tool).await,
        StoreWriteAction::ArtifactsWrite(params) => params.execute(tool).await,
        StoreWriteAction::ArtifactsDelete(params) => params.execute(tool).await,
        StoreWriteAction::CodeWrite(params) => params.execute().await,
        StoreWriteAction::ScriptsWrite(params) => params.execute(tool).await,
    }
}

impl VaultList {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        if self
            .facet
            .as_deref()
            .is_some_and(|facet| facet.eq_ignore_ascii_case("tags"))
        {
            let output = CognitionVaultTagsTool::new(tool.event_tx.clone())
                .invoke_typed(VaultTagsInput {
                    prefix: CompatOption::from(self.tag_prefix.or(self.prefix)),
                    limit: CompatOption::from(self.limit),
                })
                .await?;
            return serialize_output(CognitionVaultTagsTool::tool_id(), output);
        }
        let output = CognitionVaultListTool::new(tool.event_tx.clone())
            .invoke_typed(VaultListInput {
                prefix: self.prefix,
                limit: self.limit,
                semantic_tags: self.semantic_tags,
                tag_prefix: self.tag_prefix,
            })
            .await?;
        serialize_output(CognitionVaultListTool::tool_id(), output)
    }
}

impl VaultRead {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionVaultReadTool::new(tool.event_tx.clone())
            .invoke_typed(VaultReadInput {
                path: Some(self.path),
                max_chars: self.max_chars,
                line_start: self.line_start,
                line_end: self.line_end,
            })
            .await?;
        serialize_output(CognitionVaultReadTool::tool_id(), output)
    }
}

impl VaultSearch {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        if present(self.path.as_deref()) {
            let output = CognitionVaultGrepTool::new(tool.event_tx.clone())
                .invoke_typed(VaultGrepInput {
                    path: self.path,
                    pattern: self.query,
                    context_lines: self.context_lines,
                    limit: self.limit,
                })
                .await?;
            return serialize_output(CognitionVaultGrepTool::tool_id(), output);
        }
        let output = CognitionVaultSearchTool::new(tool.event_tx.clone())
            .invoke_typed(VaultSearchInput {
                q: CompatOption::from(self.query),
                semantic_tags: self.semantic_tags,
                limit: CompatOption::from(self.limit),
            })
            .await?;
        serialize_output(CognitionVaultSearchTool::tool_id(), output)
    }
}

impl ArtifactsList {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionArtifactListTool::new(tool.event_tx.clone(), tool.turn_scope.clone())
            .invoke_typed(ArtifactListInput {
                limit: CompatOption::from(self.limit),
                query: CompatOption::from(self.query),
            })
            .await?;
        serialize_output(CognitionArtifactListTool::tool_id(), output)
    }
}

impl ArtifactsRead {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionArtifactReadTool::new(tool.event_tx.clone(), tool.turn_scope.clone())
            .invoke_typed(ArtifactReadInput {
                artifact_id: Some(self.path),
                line_start: self.line_start,
                line_end: self.line_end,
                max_chars: self.max_chars,
            })
            .await?;
        serialize_output(CognitionArtifactReadTool::tool_id(), output)
    }
}

impl ArtifactsSearch {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionArtifactGrepTool::new(tool.event_tx.clone(), tool.turn_scope.clone())
            .invoke_typed(ArtifactGrepInput {
                artifact_id: Some(self.path),
                pattern: Some(self.query),
                context_lines: self.context_lines,
                limit: self.limit,
            })
            .await?;
        serialize_output(CognitionArtifactGrepTool::tool_id(), output)
    }
}

impl CodeRead {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        invoke_code_read(CodeReadInput {
            path: self.path,
            root: CompatOption::from(self.root),
            line_start: CompatOption::from(self.line_start.map(|value| value as u64)),
            line_end: CompatOption::from(self.line_end.map(|value| value as u64)),
            byte_start: CompatOption::from(self.byte_start),
            byte_end: CompatOption::from(self.byte_end),
        })
        .await
    }
}

impl CodeSearch {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        invoke_code_search(CodeSearchInput {
            query: self.query,
            root: CompatOption::from(self.root),
            max_results: CompatOption::from(self.max_results),
        })
        .await
    }
}

impl ScriptsList {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionGraphemeScriptListTool::new(tool.event_tx.clone())
            .invoke_typed(GraphemeScriptListInput {
                module: CompatOption::from(self.module),
                tag: CompatOption::from(self.tag),
                limit: CompatOption::from(self.limit),
            })
            .await?;
        serialize_output(CognitionGraphemeScriptListTool::tool_id(), output)
    }
}

impl ScriptsRead {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionGraphemeScriptLoadTool::new(tool.event_tx.clone())
            .invoke_typed(GraphemeScriptLoadInput {
                id: Some(self.path),
            })
            .await?;
        serialize_output(CognitionGraphemeScriptLoadTool::tool_id(), output)
    }
}

impl ScriptsSearch {
    async fn execute(self, tool: &CognitionStoreReadTool) -> stasis::prelude::Result<Value> {
        let output = CognitionGraphemeScriptSearchTool::new(tool.event_tx.clone())
            .invoke_typed(GraphemeScriptSearchInput {
                q: self.query,
                module: self.module,
                tag: self.tag,
                limit: self.limit,
            })
            .await?;
        serialize_output(CognitionGraphemeScriptSearchTool::tool_id(), output)
    }
}

impl VaultWrite {
    async fn execute(self, tool: &CognitionStoreWriteTool) -> stasis::prelude::Result<Value> {
        let output = CognitionVaultWriteTool::new(
            tool.event_tx.clone(),
            tool.turn_scope.clone(),
            tool.fallback_chat_session_id.clone(),
        )
        .invoke_typed(VaultWriteInput {
            path: Some(self.path),
            content: Some(self.content),
            session_id: None,
            session_id_provided: false,
            semantic_tags: self.semantic_tags,
            auto_workshop_tags: self.auto_workshop_tags,
            if_match: self.if_match,
        })
        .await?;
        serialize_output(CognitionVaultWriteTool::tool_id(), output)
    }
}

impl VaultDelete {
    async fn execute(self, tool: &CognitionStoreWriteTool) -> stasis::prelude::Result<Value> {
        let output = CognitionVaultDeleteTool::new(tool.event_tx.clone())
            .invoke_typed(VaultDeleteInput {
                path: Some(self.path),
            })
            .await?;
        serialize_output(CognitionVaultDeleteTool::tool_id(), output)
    }
}

impl VaultMove {
    async fn execute(self, tool: &CognitionStoreWriteTool) -> stasis::prelude::Result<Value> {
        let output = CognitionVaultMoveTool::new(tool.event_tx.clone())
            .invoke_typed(VaultMoveInput {
                from_path: Some(self.path),
                to_path: Some(self.to_path),
            })
            .await?;
        serialize_output(CognitionVaultMoveTool::tool_id(), output)
    }
}

impl ArtifactsWrite {
    async fn execute(self, tool: &CognitionStoreWriteTool) -> stasis::prelude::Result<Value> {
        let output =
            CognitionArtifactWriteTool::new(tool.event_tx.clone(), tool.turn_scope.clone())
                .invoke_typed(ArtifactWriteInput {
                    title: self.title.or(self.path.clone()),
                    html: Some(self.content),
                    presentation: self.presentation,
                    artifact_id: self.path,
                    if_match_hash64: self.if_match,
                    height: self.height,
                })
                .await?;
        serialize_output(CognitionArtifactWriteTool::tool_id(), output)
    }
}

impl ArtifactsDelete {
    async fn execute(self, tool: &CognitionStoreWriteTool) -> stasis::prelude::Result<Value> {
        let output =
            CognitionArtifactDeleteTool::new(tool.event_tx.clone(), tool.turn_scope.clone())
                .invoke_typed(ArtifactDeleteInput {
                    artifact_id: Some(self.path),
                })
                .await?;
        serialize_output(CognitionArtifactDeleteTool::tool_id(), output)
    }
}

impl CodeWrite {
    async fn execute(self) -> stasis::prelude::Result<Value> {
        invoke_code_apply_patch(CodeApplyPatchInput {
            path: self.path,
            root: CompatOption::from(self.root),
            content: CompatOption::from(self.content),
            find: CompatOption::from(self.find),
            replace: CompatOption::from(self.replace),
            expected_sha256: self.expected_sha256,
        })
        .await
    }
}

impl ScriptsWrite {
    async fn execute(self, tool: &CognitionStoreWriteTool) -> stasis::prelude::Result<Value> {
        let output = CognitionGraphemeScriptSaveTool::new(tool.event_tx.clone())
            .invoke_typed(GraphemeScriptSaveInput {
                id: self.path,
                name: self.name,
                body: Some(self.content),
                modules: self.modules,
                tags: self.tags,
                intent: self.script_intent,
                session_id: None,
            })
            .await?;
        serialize_output(CognitionGraphemeScriptSaveTool::tool_id(), output)
    }
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn store_actions_carry_their_params() {
        let read: StoreReadAction = serde_json::from_value(json!({
            "action": "vault.search",
            "query": "hello"
        }))
        .expect("vault search");
        match read {
            StoreReadAction::VaultSearch(VaultSearch { query, .. }) => {
                assert_eq!(query.as_deref(), Some("hello"));
            }
            other => panic!("expected vault.search, got {other:?}"),
        }
        let write: StoreWriteAction = serde_json::from_value(json!({
            "action": "code.write",
            "path": "src/lib.rs",
            "expected_sha256": "missing",
            "content": "fn main() {}"
        }))
        .expect("code write");
        match write {
            StoreWriteAction::CodeWrite(CodeWrite {
                path,
                expected_sha256,
                ..
            }) => {
                assert_eq!(path, "src/lib.rs");
                assert_eq!(expected_sha256, "missing");
            }
            other => panic!("expected code.write, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let read = serde_json::to_value(schemars::schema_for!(StoreReadAction)).expect("read");
        let write = serde_json::to_value(schemars::schema_for!(StoreWriteAction)).expect("write");
        for schema in [&read, &write] {
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
