//! On-demand session tool history (Phase 8C) — summary + detail by slice id.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::session::load_history;
use crate::turn_continuation::TurnContinuationScope;
use crate::turn_slice::{
    DEFAULT_TOOL_HISTORY_DETAIL_CHARS, DEFAULT_TOOL_HISTORY_SUMMARY_TURNS, tool_history_detail_markdown,
    tool_history_summary_rows,
};

pub const COGNITION_TOOL_HISTORY_SUMMARY: &str = "cognition_tool_history_summary";
pub const COGNITION_TOOL_HISTORY_DETAIL: &str = "cognition_tool_history_detail";

pub fn register_tool_history_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_tool(CognitionToolHistorySummaryTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_tool(CognitionToolHistoryDetailTool { turn_scope })?;
    Ok(())
}

async fn resolve_session_id(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
    input: &Value,
) -> StasisResult<String> {
    if let Some(session_id) = input
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(session_id.to_string());
    }
    let scope = turn_scope.read().await;
    scope
        .as_ref()
        .map(|turn| turn.session_id.clone())
        .ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_tool_history_*: session_id required when no active turn scope".to_string(),
            )
        })
}

pub struct CognitionToolHistorySummaryTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[async_trait]
impl StasisTool for CognitionToolHistorySummaryTool {
    fn name(&self) -> &'static str {
        COGNITION_TOOL_HISTORY_SUMMARY
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "High-level tool-history slices for recent session turns. Use after reading [MEDOUSA_TOOL_SLICES] \
             at turn start when you need to verify what already ran. Returns slice_id values (turn:N) for detail drill-down.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session id (defaults to active turn session)" },
                "last_k": { "type": "integer", "minimum": 1, "maximum": 24, "description": "Recent turns to include (default 5)" },
                "tool_filter": { "type": "string", "description": "Optional substring filter on tool names" },
                "keyword": { "type": "string", "description": "Optional keyword filter on slice line / goal / outcomes" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = resolve_session_id(&self.turn_scope, &input).await?;
        let last_k = input
            .get("last_k")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_TOOL_HISTORY_SUMMARY_TURNS)
            .clamp(1, 24);
        let tool_filter = input
            .get("tool_filter")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let keyword = input
            .get("keyword")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let turns = load_history(&session_id);
        let rows = tool_history_summary_rows(&turns, last_k, tool_filter, keyword);
        let lines: Vec<String> = rows.iter().map(|row| row.line.clone()).collect();

        Ok(json!({
            "ok": true,
            "session_id": session_id,
            "turn_count": turns.len(),
            "last_k": last_k,
            "slices": rows,
            "block": lines.join("\n"),
        }))
    }
}

pub struct CognitionToolHistoryDetailTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[async_trait]
impl StasisTool for CognitionToolHistoryDetailTool {
    fn name(&self) -> &'static str {
        COGNITION_TOOL_HISTORY_DETAIL
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Full tool-run detail for one session slice (slice_id=turn:N from summary or [MEDOUSA_TOOL_SLICES]). \
             Optional tool_round for a single round's receipts.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "slice_id": { "type": "string", "description": "Turn slice id, e.g. turn:5" },
                "tool_round": { "type": "integer", "minimum": 1, "description": "Optional single tool round" },
                "max_chars": { "type": "integer", "minimum": 256, "maximum": 24000 }
            },
            "required": ["slice_id"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = resolve_session_id(&self.turn_scope, &input).await?;
        let slice_id = input
            .get("slice_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_tool_history_detail: slice_id is required".to_string())
            })?;
        let tool_round = input
            .get("tool_round")
            .and_then(Value::as_u64)
            .map(|value| value as usize);
        let max_chars = input
            .get("max_chars")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_TOOL_HISTORY_DETAIL_CHARS)
            .clamp(256, 24_000);

        let turns = load_history(&session_id);
        let detail = tool_history_detail_markdown(&turns, slice_id, tool_round, max_chars)
            .map_err(StasisError::PortFailure)?;

        Ok(json!({
            "ok": true,
            "session_id": session_id,
            "slice_id": slice_id,
            "tool_round": tool_round,
            "detail": detail,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ConversationTurn;
    use crate::turn_parts::TurnPart;
    use chrono::Utc;

    #[test]
    fn summary_and_detail_round_trip() {
        let turns = vec![
            ConversationTurn {
                role: "assistant".to_string(),
                content: "done".to_string(),
                timestamp: Utc::now(),
                tool_names: vec!["cognition_manuscript_list".to_string()],
                answer_state: None,
                parts: Some(vec![TurnPart::ToolRun {
                    run_id: "r1".to_string(),
                    tool_name: "cognition_manuscript_list".to_string(),
                    status: "succeeded".to_string(),
                    input_summary: "list".to_string(),
                    output_summary: Some("base-researcher".to_string()),
                    artifact_refs: vec![],
                    tool_round: Some(1),
                    started_at: Utc::now(),
                    finished_at: None,
                }]),
                slice_summary: None,
            },
        ];
        let rows = tool_history_summary_rows(&turns, 5, None, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slice_id, "turn:1");
        let detail = tool_history_detail_markdown(&turns, "turn:1", Some(1), 4000).unwrap();
        assert!(detail.contains("cognition_manuscript_list"));
        assert!(detail.contains("base-researcher"));
    }
}
