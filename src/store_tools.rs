//! Public store primitives: one read tool and one write tool, store selected by enum.

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::StasisError;
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::public_api::{COGNITION_STORE_READ, COGNITION_STORE_WRITE};
use crate::schema_api::{advertised_object_schema, string_enum_schema};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

const STORE_READ_ID: ToolId = ToolId::new(COGNITION_STORE_READ);
const STORE_WRITE_ID: ToolId = ToolId::new(COGNITION_STORE_WRITE);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoreKind {
    Vault,
    Artifacts,
    Code,
    Scripts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoreReadOp {
    List,
    Read,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoreWriteOp {
    Write,
    Delete,
    Move,
}

#[derive(Debug, Deserialize)]
pub struct StoreReadInput {
    store: StoreKind,
    op: StoreReadOp,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    tag_prefix: Option<String>,
    #[serde(default)]
    facet: Option<String>,
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    line_start: Option<usize>,
    #[serde(default)]
    line_end: Option<usize>,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    context_lines: Option<usize>,
    #[serde(default)]
    byte_start: Option<u64>,
    #[serde(default)]
    byte_end: Option<u64>,
    #[serde(default)]
    max_results: Option<u64>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StoreWriteInput {
    store: StoreKind,
    op: StoreWriteOp,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    to_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    semantic_tags: Option<Vec<String>>,
    #[serde(default)]
    auto_workshop_tags: Option<bool>,
    #[serde(default)]
    if_match: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    presentation: Option<String>,
    #[serde(default)]
    artifact_id: Option<String>,
    #[serde(default)]
    if_match_hash64: Option<String>,
    #[serde(default)]
    height: Option<u64>,
    #[serde(default)]
    root: Option<String>,
    #[serde(default)]
    find: Option<String>,
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    expected_sha256: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    modules: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    script_intent: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

impl JsonSchema for StoreReadInput {
    fn schema_name() -> String {
        "StoreReadInput".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[
            (
                "store",
                string_enum_schema(&["vault", "artifacts", "code", "scripts"]),
                true,
            ),
            ("op", string_enum_schema(&["list", "read", "search"]), true),
        ])
    }
}

impl JsonSchema for StoreWriteInput {
    fn schema_name() -> String {
        "StoreWriteInput".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[
            (
                "store",
                string_enum_schema(&["vault", "artifacts", "code", "scripts"]),
                true,
            ),
            ("op", string_enum_schema(&["write", "delete", "move"]), true),
        ])
    }
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
    /// Read or search vault notes, HTML artifacts, code, or saved Grapheme scripts. store=vault|artifacts|code|scripts. op=list|read|search. Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, input: StoreReadInput) -> stasis::prelude::Result<ExternalJson> {
        let value = dispatch_read(&self.event_tx, self.turn_scope.clone(), input).await?;
        Ok(ExternalJson::new(value))
    }
}

#[medousa_tool(id = STORE_WRITE_ID)]
impl CognitionStoreWriteTool {
    /// Create, update, delete, or move vault notes, HTML artifacts, code, or saved Grapheme scripts. store=vault|artifacts|code|scripts. op=write|delete|move. Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(&self, input: StoreWriteInput) -> stasis::prelude::Result<ExternalJson> {
        let value = dispatch_write(
            &self.event_tx,
            self.turn_scope.clone(),
            &self.fallback_chat_session_id,
            input,
        )
        .await?;
        Ok(ExternalJson::new(value))
    }
}

async fn dispatch_read(
    event_tx: &mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    input: StoreReadInput,
) -> stasis::prelude::Result<Value> {
    match (input.store, input.op) {
        (StoreKind::Vault, StoreReadOp::List) => {
            if input
                .facet
                .as_deref()
                .is_some_and(|facet| facet.eq_ignore_ascii_case("tags"))
            {
                crate::vault_tools::CognitionVaultTagsTool::new(event_tx.clone())
                    .invoke(json_obj([
                        ("prefix", opt_str(input.tag_prefix.or(input.prefix))),
                        ("limit", opt_usize(input.limit)),
                    ]))
                    .await
            } else {
                crate::vault_tools::CognitionVaultListTool::new(event_tx.clone())
                    .invoke(json_obj([
                        ("prefix", opt_str(input.prefix)),
                        ("limit", opt_usize(input.limit)),
                        ("semantic_tags", opt_tags(input.semantic_tags)),
                        ("tag_prefix", opt_str(input.tag_prefix)),
                    ]))
                    .await
            }
        }
        (StoreKind::Vault, StoreReadOp::Read) => {
            crate::vault_tools::CognitionVaultReadTool::new(event_tx.clone())
                .invoke(json_obj([
                    ("path", opt_str(input.path)),
                    ("max_chars", opt_usize(input.max_chars)),
                    ("line_start", opt_usize(input.line_start)),
                    ("line_end", opt_usize(input.line_end)),
                ]))
                .await
        }
        (StoreKind::Vault, StoreReadOp::Search) => {
            if input
                .path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty())
            {
                crate::vault_tools::CognitionVaultGrepTool::new(event_tx.clone())
                    .invoke(json_obj([
                        ("path", opt_str(input.path)),
                        ("pattern", opt_str(input.query)),
                        ("context_lines", opt_usize(input.context_lines)),
                        ("limit", opt_usize(input.limit)),
                    ]))
                    .await
            } else {
                crate::vault_tools::CognitionVaultSearchTool::new(event_tx.clone())
                    .invoke(json_obj([
                        ("query", opt_str(input.query)),
                        ("semantic_tags", opt_tags(input.semantic_tags)),
                        ("limit", opt_usize(input.limit)),
                    ]))
                    .await
            }
        }
        (StoreKind::Artifacts, StoreReadOp::List) => {
            crate::artifact_tools::CognitionArtifactListTool::new(event_tx.clone(), turn_scope)
                .invoke(json_obj([
                    ("limit", opt_usize(input.limit)),
                    ("query", opt_str(input.query.or(input.prefix))),
                ]))
                .await
        }
        (StoreKind::Artifacts, StoreReadOp::Read) => {
            crate::artifact_tools::CognitionArtifactReadTool::new(event_tx.clone(), turn_scope)
                .invoke(json_obj([
                    ("artifact_id", opt_str(input.path)),
                    ("line_start", opt_usize(input.line_start)),
                    ("line_end", opt_usize(input.line_end)),
                    ("max_chars", opt_usize(input.max_chars)),
                ]))
                .await
        }
        (StoreKind::Artifacts, StoreReadOp::Search) => {
            require(
                input.path.as_deref(),
                "cognition_store_read: artifacts search needs path (artifact id) and query",
            )?;
            crate::artifact_tools::CognitionArtifactGrepTool::new(event_tx.clone(), turn_scope)
                .invoke(json_obj([
                    ("artifact_id", opt_str(input.path)),
                    ("pattern", opt_str(input.query)),
                    ("context_lines", opt_usize(input.context_lines)),
                    ("limit", opt_usize(input.limit)),
                ]))
                .await
        }
        (StoreKind::Code, StoreReadOp::List) => Err(StasisError::PortFailure(
            "cognition_store_read: code has no list; use op=search or op=read".to_string(),
        )),
        (StoreKind::Code, StoreReadOp::Read) => {
            crate::coding_tools::invoke_code_read_json(json_obj([
                ("path", opt_str(input.path)),
                ("root", opt_str(input.root)),
                (
                    "line_start",
                    opt_u64(input.line_start.map(|value| value as u64)),
                ),
                (
                    "line_end",
                    opt_u64(input.line_end.map(|value| value as u64)),
                ),
                ("byte_start", opt_u64(input.byte_start)),
                ("byte_end", opt_u64(input.byte_end)),
            ]))
            .await
        }
        (StoreKind::Code, StoreReadOp::Search) => {
            crate::coding_tools::invoke_code_search_json(json_obj([
                ("query", opt_str(input.query)),
                ("root", opt_str(input.root)),
                (
                    "max_results",
                    opt_u64(input.max_results.or(input.limit.map(|value| value as u64))),
                ),
            ]))
            .await
        }
        (StoreKind::Scripts, StoreReadOp::List) => {
            crate::grapheme_script_tools::CognitionGraphemeScriptListTool::new(event_tx.clone())
                .invoke(json_obj([
                    ("module", opt_str(input.module)),
                    ("tag", opt_str(input.tag.or(input.tag_prefix))),
                    ("limit", opt_usize(input.limit)),
                ]))
                .await
        }
        (StoreKind::Scripts, StoreReadOp::Read) => {
            crate::grapheme_script_tools::CognitionGraphemeScriptLoadTool::new(event_tx.clone())
                .invoke(json_obj([("id", opt_str(input.path))]))
                .await
        }
        (StoreKind::Scripts, StoreReadOp::Search) => {
            crate::grapheme_script_tools::CognitionGraphemeScriptSearchTool::new(event_tx.clone())
                .invoke(json_obj([
                    ("q", opt_str(input.query)),
                    ("module", opt_str(input.module)),
                    ("tag", opt_str(input.tag)),
                    ("limit", opt_usize(input.limit)),
                ]))
                .await
        }
    }
}

async fn dispatch_write(
    event_tx: &mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    fallback_chat_session_id: &str,
    input: StoreWriteInput,
) -> stasis::prelude::Result<Value> {
    match (input.store, input.op) {
        (StoreKind::Vault, StoreWriteOp::Write) => {
            crate::vault_tools::CognitionVaultWriteTool::new(
                event_tx.clone(),
                turn_scope,
                fallback_chat_session_id.to_string(),
            )
            .invoke(json_obj([
                ("path", opt_str(input.path)),
                ("content", opt_str(input.content)),
                ("semantic_tags", opt_tags(input.semantic_tags)),
                ("auto_workshop_tags", opt_bool(input.auto_workshop_tags)),
                ("if_match", opt_str(input.if_match)),
            ]))
            .await
        }
        (StoreKind::Vault, StoreWriteOp::Delete) => {
            crate::vault_tools::CognitionVaultDeleteTool::new(event_tx.clone())
                .invoke(json_obj([("path", opt_str(input.path))]))
                .await
        }
        (StoreKind::Vault, StoreWriteOp::Move) => {
            crate::vault_tools::CognitionVaultMoveTool::new(event_tx.clone())
                .invoke(json_obj([
                    ("from_path", opt_str(input.path)),
                    ("to_path", opt_str(input.to_path)),
                ]))
                .await
        }
        (StoreKind::Artifacts, StoreWriteOp::Write) => {
            crate::artifact_tools::CognitionArtifactWriteTool::new(event_tx.clone(), turn_scope)
                .invoke(json_obj([
                    ("title", opt_str(input.title.or(input.path.clone()))),
                    ("html", opt_str(input.content)),
                    ("presentation", opt_str(input.presentation)),
                    ("artifact_id", opt_str(input.artifact_id.or(input.path))),
                    (
                        "if_match_hash64",
                        opt_str(input.if_match_hash64.or(input.if_match)),
                    ),
                    ("height", opt_u64(input.height)),
                ]))
                .await
        }
        (StoreKind::Artifacts, StoreWriteOp::Delete) => {
            crate::artifact_tools::CognitionArtifactDeleteTool::new(event_tx.clone(), turn_scope)
                .invoke(json_obj([(
                    "artifact_id",
                    opt_str(input.path.or(input.artifact_id)),
                )]))
                .await
        }
        (StoreKind::Artifacts, StoreWriteOp::Move) => Err(StasisError::PortFailure(
            "cognition_store_write: artifacts have no move; write a revision or delete".to_string(),
        )),
        (StoreKind::Code, StoreWriteOp::Write) => {
            let expected = input.expected_sha256.clone().ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_store_write: code write needs expected_sha256 (from read) or \"missing\" for a new file"
                        .to_string(),
                )
            })?;
            crate::coding_tools::invoke_code_apply_patch_json(json_obj([
                ("path", opt_str(input.path)),
                ("root", opt_str(input.root)),
                ("content", opt_str(input.content)),
                ("find", opt_str(input.find)),
                ("replace", opt_str(input.replace)),
                ("expected_sha256", Some(Value::String(expected))),
            ]))
            .await
        }
        (StoreKind::Code, StoreWriteOp::Delete | StoreWriteOp::Move) => {
            Err(StasisError::PortFailure(
                "cognition_store_write: code only supports op=write (content or find/replace)"
                    .to_string(),
            ))
        }
        (StoreKind::Scripts, StoreWriteOp::Write) => {
            crate::grapheme_script_tools::CognitionGraphemeScriptSaveTool::new(event_tx.clone())
                .invoke(json_obj([
                    ("id", opt_str(input.id.or(input.path))),
                    ("name", opt_str(input.name)),
                    ("body", opt_str(input.content)),
                    ("modules", opt_tags(input.modules)),
                    ("tags", opt_tags(input.tags)),
                    ("intent", opt_str(input.script_intent)),
                ]))
                .await
        }
        (StoreKind::Scripts, StoreWriteOp::Delete | StoreWriteOp::Move) => {
            Err(StasisError::PortFailure(
                "cognition_store_write: scripts only support op=write".to_string(),
            ))
        }
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

fn opt_u64(value: Option<u64>) -> Option<Value> {
    value.map(|value| json!(value))
}

fn opt_bool(value: Option<bool>) -> Option<Value> {
    value.map(Value::Bool)
}

fn opt_tags(value: Option<Vec<String>>) -> Option<Value> {
    value.map(|tags| json!(tags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_enums_are_snake_case() {
        let read: StoreReadInput = serde_json::from_value(json!({
            "store": "vault",
            "op": "search",
            "query": "hello"
        }))
        .expect("vault search");
        assert_eq!(read.store, StoreKind::Vault);
        assert_eq!(read.op, StoreReadOp::Search);
        let write: StoreWriteInput = serde_json::from_value(json!({
            "store": "code",
            "op": "write",
            "path": "src/lib.rs",
            "expected_sha256": "missing"
        }))
        .expect("code write");
        assert_eq!(write.store, StoreKind::Code);
        assert_eq!(write.op, StoreWriteOp::Write);
    }

    #[test]
    fn advertised_store_schemas_are_discriminators_only() {
        let read = serde_json::to_value(schemars::schema_for!(StoreReadInput)).expect("read");
        let write = serde_json::to_value(schemars::schema_for!(StoreWriteInput)).expect("write");
        for schema in [&read, &write] {
            let props = schema["properties"].as_object().expect("properties");
            assert_eq!(props.len(), 2);
            assert!(props.contains_key("store"));
            assert!(props.contains_key("op"));
            assert_eq!(schema["additionalProperties"], true);
        }
    }
}
