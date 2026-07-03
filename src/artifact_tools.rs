//! Agent tools for listing, reading, grepping, and revising HTML UI artifacts.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::{RwLock, mpsc};

use crate::events::TuiEvent;
use crate::runtime_session::{require_active_chat_session_id_async, runtime_bootstrap_session_id};
use crate::turn_continuation::TurnContinuationScope;

pub const COGNITION_ARTIFACT_LIST: &str = "cognition_artifact_list";
pub const COGNITION_ARTIFACT_READ: &str = "cognition_artifact_read";
pub const COGNITION_ARTIFACT_GREP: &str = "cognition_artifact_grep";
pub const COGNITION_ARTIFACT_WRITE: &str = "cognition_artifact_write";

pub const ARTIFACT_COGNITION_TOOLS: &[&str] = &[
    COGNITION_ARTIFACT_LIST,
    COGNITION_ARTIFACT_READ,
    COGNITION_ARTIFACT_GREP,
    COGNITION_ARTIFACT_WRITE,
];

const READ_BUDGET_CHARS: usize = 12_000;

pub fn is_artifact_cognition_tool(name: &str) -> bool {
    ARTIFACT_COGNITION_TOOLS.contains(&name)
}

pub fn register_artifact_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_tool(CognitionArtifactListTool::new(event_tx.clone(), turn_scope.clone()))?;
    registry.register_tool(CognitionArtifactReadTool::new(event_tx.clone(), turn_scope.clone()))?;
    registry.register_tool(CognitionArtifactGrepTool::new(event_tx.clone(), turn_scope.clone()))?;
    registry.register_tool(CognitionArtifactWriteTool::new(event_tx, turn_scope))?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

struct ArtifactToolContext {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl ArtifactToolContext {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }

    async fn require_ui_artifacts(&self) -> StasisResult<()> {
        let supported = self
            .turn_scope
            .read()
            .await
            .as_ref()
            .is_some_and(|scope| scope.supports_ui_artifacts);
        if supported {
            Ok(())
        } else {
            Err(StasisError::PortFailure(
                "This channel does not support HTML UI artifacts (supports_ui_artifacts=false)."
                    .to_string(),
            ))
        }
    }

    async fn session_id(&self, tool_name: &str) -> StasisResult<String> {
        require_active_chat_session_id_async(
            &self.turn_scope,
            runtime_bootstrap_session_id(),
            tool_name,
        )
        .await
    }
}

pub struct CognitionArtifactListTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactListTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[async_trait]
impl StasisTool for CognitionArtifactListTool {
    fn name(&self) -> &'static str {
        COGNITION_ARTIFACT_LIST
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "List HTML presentation artifacts for the current chat session (newest first). \
             Workflow: list → grep/read → cognition_artifact_write to revise.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                "query": { "type": "string", "description": "Optional filter on title or artifact_id" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(self.name()).await?;
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 100) as usize;
        let query_owned = input
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        emit_invoked(&self.event_tx, self.name(), query_owned.as_deref().unwrap_or("*"));
        let records = tokio::task::spawn_blocking(move || {
            crate::artifact_store::list_ui_artifacts(
                Some(&session_id),
                limit,
                query_owned.as_deref(),
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact list join error: {err}")))?;
        let artifacts: Vec<Value> = records
            .into_iter()
            .map(|record| {
                json!({
                    "artifact_id": record.artifact_id,
                    "label": record.label,
                    "presentation": record.presentation,
                    "byte_size": record.byte_size,
                    "stored_at_utc": record.stored_at_utc,
                    "supersedes_artifact_id": record.supersedes_artifact_id,
                    "root_artifact_id": record.root_artifact_id,
                })
            })
            .collect();
        Ok(json!({ "artifacts": artifacts, "count": artifacts.len() }))
    }
}

pub struct CognitionArtifactReadTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactReadTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[async_trait]
impl StasisTool for CognitionArtifactReadTool {
    fn name(&self) -> &'static str {
        COGNITION_ARTIFACT_READ
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Read HTML source for a presentation artifact (budget-capped). \
             Optional line_start/line_end for surgical edits.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["artifact_id"],
            "properties": {
                "artifact_id": { "type": "string" },
                "line_start": { "type": "integer", "minimum": 1 },
                "line_end": { "type": "integer", "minimum": 1 },
                "max_chars": { "type": "integer", "minimum": 256, "maximum": 20000 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(self.name()).await?;
        let artifact_id = input
            .get("artifact_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("artifact_id is required".to_string()))?
            .to_string();
        let line_start = input.get("line_start").and_then(Value::as_u64).map(|v| v as usize);
        let line_end = input.get("line_end").and_then(Value::as_u64).map(|v| v as usize);
        let max_chars = input
            .get("max_chars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(READ_BUDGET_CHARS)
            .clamp(256, 20_000);
        emit_invoked(&self.event_tx, self.name(), &artifact_id);
        let artifact_id_for_response = artifact_id.clone();
        let excerpt = tokio::task::spawn_blocking(move || {
            crate::artifact_store::read_ui_artifact_excerpt(
                &session_id,
                &artifact_id,
                line_start,
                line_end,
                max_chars,
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact read join error: {err}")))?
        .map_err(StasisError::PortFailure)?;
        Ok(json!({
            "artifact_id": artifact_id_for_response,
            "content": excerpt.content,
            "truncated": excerpt.truncated,
            "total_lines": excerpt.total_lines,
            "total_chars": excerpt.total_chars,
            "line_start": excerpt.line_start,
            "line_end": excerpt.line_end,
        }))
    }
}

pub struct CognitionArtifactGrepTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactGrepTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[async_trait]
impl StasisTool for CognitionArtifactGrepTool {
    fn name(&self) -> &'static str {
        COGNITION_ARTIFACT_GREP
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Search inside an HTML artifact source (literal case-insensitive match with line numbers). \
             Use before cognition_artifact_write to locate CSS/HTML snippets.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["artifact_id", "pattern"],
            "properties": {
                "artifact_id": { "type": "string" },
                "pattern": { "type": "string" },
                "context_lines": { "type": "integer", "minimum": 0, "maximum": 10 },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(self.name()).await?;
        let artifact_id = input
            .get("artifact_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("artifact_id is required".to_string()))?
            .to_string();
        let pattern = input
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("pattern is required".to_string()))?
            .to_string();
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
        emit_invoked(&self.event_tx, self.name(), &artifact_id);
        let result = tokio::task::spawn_blocking(move || {
            crate::artifact_store::grep_ui_artifact(
                &session_id,
                &artifact_id,
                &pattern,
                context_lines,
                limit,
            )
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact grep join error: {err}")))?
        .map_err(StasisError::PortFailure)?;
        serde_json::to_value(result).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionArtifactWriteTool {
    event_tx: mpsc::Sender<TuiEvent>,
    ctx: ArtifactToolContext,
}

impl CognitionArtifactWriteTool {
    pub fn new(
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            event_tx,
            ctx: ArtifactToolContext::new(turn_scope),
        }
    }
}

#[async_trait]
impl StasisTool for CognitionArtifactWriteTool {
    fn name(&self) -> &'static str {
        COGNITION_ARTIFACT_WRITE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Create or revise an HTML presentation artifact. Pass artifact_id to publish a new revision \
             (content-addressed). Use if_match_hash64 for optimistic concurrency. First-time publish: use cognition_ui_present. \
             Canvas widgets using MedousaStore: get/set/delete return Promises — use async/await (wiki topic artifact_runtime).",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["title", "html", "presentation"],
            "properties": {
                "title": { "type": "string" },
                "html": {
                    "type": "string",
                    "description": "HTML fragment or document. MedousaStore get/set/delete are async — await in async init and handlers (cognition_environment_wiki topic=artifact_runtime)."
                },
                "presentation": { "type": "string", "enum": ["inline", "panel", "fullscreen"] },
                "artifact_id": { "type": "string", "description": "When set, supersedes this artifact revision" },
                "if_match_hash64": { "type": "string", "description": "Optional hash64 of the artifact being revised" },
                "height": { "type": "integer", "minimum": 120, "maximum": 1200 }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        self.ctx.require_ui_artifacts().await?;
        let session_id = self.ctx.session_id(self.name()).await?;
        let title = input
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("title is required".to_string()))?
            .to_string();
        let html = input
            .get("html")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| StasisError::PortFailure("html is required".to_string()))?
            .to_string();
        let presentation = input
            .get("presentation")
            .and_then(Value::as_str)
            .unwrap_or("inline")
            .to_string();
        let height_px = input.get("height").and_then(Value::as_u64).map(|value| {
            value.clamp(120, 1200) as u32
        });
        let artifact_id = input
            .get("artifact_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let if_match_hash64 = input
            .get("if_match_hash64")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        emit_invoked(
            &self.event_tx,
            self.name(),
            artifact_id.as_deref().unwrap_or("new"),
        );

        let record = tokio::task::spawn_blocking(move || {
            if let Some(previous_id) = artifact_id.as_deref() {
                if let Some(expected) = if_match_hash64.as_deref() {
                    let previous = crate::artifact_store::fetch_artifact_at_id(&session_id, previous_id)
                        .ok_or_else(|| format!("artifact not found: {previous_id}"))?;
                    if previous.record.hash64 != expected {
                        return Err(format!(
                            "if_match_hash64 mismatch (expected {expected}, got {})",
                            previous.record.hash64
                        ));
                    }
                }
                crate::artifact_store::persist_ui_artifact_revision(
                    &session_id,
                    &html,
                    &title,
                    &presentation,
                    height_px,
                    Some(previous_id),
                )
            } else {
                crate::artifact_store::persist_ui_artifact(
                    &session_id,
                    &html,
                    &title,
                    &presentation,
                    height_px,
                )
            }
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("artifact write join error: {err}")))?
        .map_err(StasisError::PortFailure)?;

        let previous_artifact_id = record.supersedes_artifact_id.clone();
        Ok(json!({
            "ok": true,
            "artifact_id": record.artifact_id,
            "previous_artifact_id": previous_artifact_id,
            "root_artifact_id": record.root_artifact_id,
            "label": record.label,
            "mime": record.content_type,
            "presentation": record.presentation,
            "height_px": record.height_px,
            "byte_size": record.byte_size,
            "hash64": record.hash64,
        }))
    }
}
