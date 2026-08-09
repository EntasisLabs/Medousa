//! Host-bus vault tools: list, read, search, write, tags.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::daemon_api::{VaultNotesListResponse, VaultWriteRequest};
use crate::events::TuiEvent;
use crate::locus_semantic_tags::parse_semantic_tags_from_value;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};
use crate::vault::VaultService;

const READ_BUDGET_CHARS: usize = 12_000;
const COGNITION_VAULT_LIST_ID: ToolId = ToolId::new("cognition_vault_list");

pub fn register_vault_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    fallback_chat_session_id: String,
) -> StasisResult<()> {
    registry.register_tool(CognitionVaultListTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultReadTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultGrepTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultSearchTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultTagsTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultWriteTool::new(
        event_tx.clone(),
        turn_scope.clone(),
        fallback_chat_session_id.clone(),
    ))?;
    registry.register_tool(CognitionVaultDeleteTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultMoveTool::new(event_tx))?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

pub struct CognitionVaultListTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultListTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct VaultListInput {
    /// Optional path prefix filter
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<usize>,
    /// Indexed-style tag filter (match-all), aligned with Locus tags
    #[serde(default, deserialize_with = "deserialize_lenient_semantic_tags")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    pub semantic_tags: Option<Vec<String>>,
    /// Filter notes with tags sharing this prefix
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub tag_prefix: Option<String>,
}

fn deserialize_lenient_semantic_tags<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(parse_semantic_tags_from_value(Some(&value)))
}

#[medousa_tool(id = COGNITION_VAULT_LIST_ID)]
impl CognitionVaultListTool {
    /// List vault notes (path + title + semantic tags). Optional tag filter (match-all).
    async fn invoke_typed(
        &self,
        input: VaultListInput,
    ) -> stasis::prelude::Result<VaultNotesListResponse> {
        let tags = input.semantic_tags.map(|tags| tags.join(","));
        let tag_prefix = input
            .tag_prefix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        emit_invoked(
            &self.event_tx,
            COGNITION_VAULT_LIST_ID.as_str(),
            input.prefix.as_deref().unwrap_or("*"),
        );
        let response = VaultService::list_notes(
            input.prefix.as_deref(),
            input.limit.unwrap_or(50),
            tags.as_deref(),
            tag_prefix,
        );
        Ok(response)
    }
}

pub struct CognitionVaultReadTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultReadTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultReadTool {
    fn name(&self) -> &'static str {
        "cognition_vault_read"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Read a vault note body (budget-capped).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "max_chars": { "type": "integer", "minimum": 256, "maximum": 20000 },
                "line_start": { "type": "integer", "minimum": 1 },
                "line_end": { "type": "integer", "minimum": 1 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        let max_chars = input
            .get("max_chars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(READ_BUDGET_CHARS)
            .clamp(256, 20_000);
        let line_start = input.get("line_start").and_then(Value::as_u64).map(|v| v as usize);
        let line_end = input.get("line_end").and_then(Value::as_u64).map(|v| v as usize);
        emit_invoked(&self.event_tx, self.name(), path);
        let note = VaultService::get_note(path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        if line_start.is_some() || line_end.is_some() {
            let excerpt = crate::line_grep::excerpt_lines(
                &note.content,
                line_start,
                line_end,
                max_chars,
            );
            return Ok(json!({
                "note": note.note,
                "content": excerpt.content,
                "truncated": excerpt.truncated,
                "total_lines": excerpt.total_lines,
                "total_chars": excerpt.total_chars,
                "line_start": excerpt.line_start,
                "line_end": excerpt.line_end,
            }));
        }
        let truncated = truncate_chars(&note.content, max_chars);
        Ok(json!({
            "note": note.note,
            "content": truncated.body,
            "truncated": truncated.truncated,
            "total_lines": note.content.lines().count(),
            "total_chars": note.content.chars().count(),
        }))
    }
}

pub struct CognitionVaultGrepTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultGrepTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultGrepTool {
    fn name(&self) -> &'static str {
        "cognition_vault_grep"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Search inside a vault note (literal case-insensitive match with line numbers). \
             Use cognition_vault_search to discover notes; use grep for surgical edits.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["path", "pattern"],
            "properties": {
                "path": { "type": "string" },
                "pattern": { "type": "string" },
                "context_lines": { "type": "integer", "minimum": 0, "maximum": 10 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        let pattern = input
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("pattern is required".to_string()))?;
        let context_lines = input
            .get("context_lines")
            .and_then(Value::as_u64)
            .unwrap_or(2)
            .clamp(0, 10) as usize;
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 200) as usize;
        emit_invoked(&self.event_tx, self.name(), path);
        let note = VaultService::get_note(path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let result = crate::line_grep::grep_lines(&note.content, pattern, context_lines, limit)
            .map_err(StasisError::PortFailure)?;
        serde_json::to_value(result).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionVaultSearchTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultSearchTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultSearchTool {
    fn name(&self) -> &'static str {
        "cognition_vault_search"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Search vault notes by full text and/or semantic tags (match-all).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "q": { "type": "string", "description": "Full-text query (optional if semantic_tags set)" },
                "semantic_tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let query = input
            .get("q")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let tags = parse_semantic_tags_from_value(input.get("semantic_tags"))
            .map(|tags| tags.join(","));
        if query.is_none() && tags.is_none() {
            return Err(StasisError::PortFailure(
                "q or semantic_tags is required".to_string(),
            ));
        }
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(20);
        emit_invoked(
            &self.event_tx,
            self.name(),
            query.unwrap_or("tags-only"),
        );
        let response = VaultService::search(query, limit, tags.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionVaultTagsTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultTagsTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultTagsTool {
    fn name(&self) -> &'static str {
        "cognition_vault_tags"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List semantic tags used across vault notes (shared vocabulary with Locus memory).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prefix": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let prefix = input
            .get("prefix")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(100)
            .clamp(1, 500) as usize;
        emit_invoked(&self.event_tx, self.name(), prefix.unwrap_or("all"));
        let response = VaultService::list_tags(prefix, limit);
        Ok(json!({
            "tags": response.tags,
            "count": response.count,
            "usage": "Use semantic_tags on cognition_vault_list/search/write or match Locus via cognition_memory_tags.",
        }))
    }
}

pub struct CognitionVaultWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    fallback_chat_session_id: String,
}

impl CognitionVaultWriteTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
        fallback_chat_session_id: String,
    ) -> Self {
        Self {
            event_tx,
            turn_scope,
            fallback_chat_session_id,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultWriteTool {
    fn name(&self) -> &'static str {
        "cognition_vault_write"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Create or update a vault markdown note. Merges Locus-aligned semantic tags into frontmatter.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
                "session_id": {
                    "type": "string",
                    "description": "Chat session for workshop linking tags (defaults to current turn session)"
                },
                "semantic_tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "auto_workshop_tags": {
                    "type": "boolean",
                    "description": "Merge medousa/vault/session/profile/chat defaults (default true)"
                },
                "if_match": { "type": "string", "description": "Optional content_hash for optimistic concurrency" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        let content = input
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("content is required".to_string()))?;
        let if_match = input.get("if_match").and_then(Value::as_str);
        let session_id = if input.get("session_id").is_some() {
            input
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        } else {
            Some(crate::locus_memory::resolve_memory_tool_session_id(
                &input,
                &self.turn_scope,
                &self.fallback_chat_session_id,
                true,
            )
            .await)
        };
        let auto_workshop_tags = input
            .get("auto_workshop_tags")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        emit_invoked(&self.event_tx, self.name(), path);
        let request = VaultWriteRequest {
            path: Some(path.to_string()),
            content: content.to_string(),
            session_id,
            semantic_tags: parse_semantic_tags_from_value(input.get("semantic_tags")),
            auto_workshop_tags,
        };
        let response = VaultService::write_note_with_actor(
            Some(path),
            &request,
            if_match,
            crate::daemon_api::WorkspaceEventActor::Agent,
            Some("cognition_vault_write"),
        )
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

struct TruncatedBody {
    body: String,
    truncated: bool,
}

fn truncate_chars(body: &str, max_chars: usize) -> TruncatedBody {
    if body.chars().count() <= max_chars {
        return TruncatedBody {
            body: body.to_string(),
            truncated: false,
        };
    }
    TruncatedBody {
        body: format!("{}…", body.chars().take(max_chars).collect::<String>()),
        truncated: true,
    }
}

pub struct CognitionVaultDeleteTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultDeleteTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultDeleteTool {
    fn name(&self) -> &'static str {
        "cognition_vault_delete"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Soft-delete a vault markdown note (moves to .trash). Use after confirming the path with list/read.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "Relative vault note path to delete" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("path is required".to_string()))?;
        emit_invoked(&self.event_tx, self.name(), path);
        let response = VaultService::delete_note(path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionVaultMoveTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultMoveTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultMoveTool {
    fn name(&self) -> &'static str {
        "cognition_vault_move"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Move/rename a vault note to a new relative path. Creates parent folders as needed and removes the source note.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["from_path", "to_path"],
            "properties": {
                "from_path": { "type": "string", "description": "Existing note path" },
                "to_path": { "type": "string", "description": "Destination path" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let from_path = input
            .get("from_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("from_path is required".to_string()))?;
        let to_path = input
            .get("to_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("to_path is required".to_string()))?;
        emit_invoked(
            &self.event_tx,
            self.name(),
            &format!("{from_path} -> {to_path}"),
        );
        let response = VaultService::relocate_note(from_path, to_path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use stasis::application::orchestration::tool_registry::StasisTool;

    use super::{COGNITION_VAULT_LIST_ID, CognitionVaultListTool, VaultListInput};
    use crate::events::TuiEvent;

    #[test]
    fn typed_vault_list_keeps_injected_events_and_lenient_wire_inputs() {
        crate::vault::service::with_temp_vault(|| {
            let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1);
            let tool = CognitionVaultListTool::new(event_tx);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime");

            let input = crate::typed_tools::deserialize_input::<VaultListInput>(
                COGNITION_VAULT_LIST_ID,
                json!({
                    "prefix": 42,
                    "limit": "many",
                    "semantic_tags": "alpha, beta",
                    "tag_prefix": false
                }),
            )
            .expect("legacy optional forms");
            assert_eq!(input.prefix, None);
            assert_eq!(input.limit, None);
            assert_eq!(
                input.semantic_tags,
                Some(vec!["alpha".to_string(), "beta".to_string()])
            );
            assert_eq!(input.tag_prefix, None);

            let output = runtime
                .block_on(tool.invoke_typed(input))
                .expect("typed list response");
            assert!(output.notes.is_empty());

            match event_rx.try_recv().expect("injected event sender used") {
                TuiEvent::ToolInvoked {
                    tool_name,
                    input_summary,
                } => {
                    assert_eq!(tool_name, COGNITION_VAULT_LIST_ID.as_str());
                    assert_eq!(input_summary, "*");
                }
                other => panic!("unexpected event: {other:?}"),
            }

            let boundary = runtime
                .block_on(StasisTool::invoke(&tool, json!({ "limit": 1 })))
                .expect("generated Stasis boundary");
            assert!(boundary["notes"].is_array());
        });
    }
}
