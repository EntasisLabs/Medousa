//! Host-bus vault tools: list, read, search, write.

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::daemon_api::VaultWriteRequest;
use crate::events::TuiEvent;
use crate::vault::VaultService;

const READ_BUDGET_CHARS: usize = 12_000;

pub fn register_vault_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_tool(CognitionVaultListTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultReadTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultSearchTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionVaultWriteTool::new(event_tx))?;
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

#[async_trait]
impl StasisTool for CognitionVaultListTool {
    fn name(&self) -> &'static str {
        "cognition_vault_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some("List vault notes (path + title metadata).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prefix": { "type": "string", "description": "Optional path prefix filter" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let prefix = input.get("prefix").and_then(Value::as_str);
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(50);
        emit_invoked(&self.event_tx, self.name(), prefix.unwrap_or("*"));
        let response = VaultService::list_notes(prefix, limit);
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
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
                "max_chars": { "type": "integer", "minimum": 256, "maximum": 20000 }
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
        emit_invoked(&self.event_tx, self.name(), path);
        let note = VaultService::get_note(path)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let truncated = truncate_chars(&note.content, max_chars);
        Ok(json!({
            "note": note.note,
            "content": truncated.body,
            "truncated": truncated.truncated,
        }))
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
        Some("Full-text search over vault notes with ranked hits.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["q"],
            "properties": {
                "q": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let query = input
            .get("q")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("q is required".to_string()))?;
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(20);
        emit_invoked(&self.event_tx, self.name(), query);
        let response = VaultService::search(query, limit)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionVaultWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionVaultWriteTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionVaultWriteTool {
    fn name(&self) -> &'static str {
        "cognition_vault_write"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Create or update a vault markdown note.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" },
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
        emit_invoked(&self.event_tx, self.name(), path);
        let request = VaultWriteRequest {
            path: Some(path.to_string()),
            content: content.to_string(),
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
