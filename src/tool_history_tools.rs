//! On-demand session tool history (Phase 8C) — summary + detail by slice id.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::session::load_history;
use crate::turn_continuation::TurnContinuationScope;
use crate::turn_slice::{
    DEFAULT_TOOL_HISTORY_DETAIL_CHARS, DEFAULT_TOOL_HISTORY_SUMMARY_TURNS, tool_history_detail_markdown,
    tool_history_summary_rows, ToolHistorySliceRow,
};
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_TOOL_HISTORY_SUMMARY: &str = "cognition_tool_history_summary";
pub const COGNITION_TOOL_HISTORY_DETAIL: &str = "cognition_tool_history_detail";
const COGNITION_TOOL_HISTORY_SUMMARY_ID: ToolId = ToolId::new(COGNITION_TOOL_HISTORY_SUMMARY);
const COGNITION_TOOL_HISTORY_DETAIL_ID: ToolId = ToolId::new(COGNITION_TOOL_HISTORY_DETAIL);

pub fn register_tool_history_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionToolHistorySummaryTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_typed_tool(CognitionToolHistoryDetailTool { turn_scope })?;
    Ok(())
}

pub struct CognitionToolHistorySummaryTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolHistorySummaryInput {
    /// Session id (defaults to active turn session)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Recent turns to include (default 5)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
    )]
    #[schemars(
        with = "usize",
        range(min = 1, max = 24),
        skip_serializing_if = "Option::is_none"
    )]
    last_k: Option<usize>,
    /// Optional substring filter on tool names
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    tool_filter: Option<String>,
    /// Optional keyword filter on slice line / goal / outcomes
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    keyword: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ToolHistorySummaryOutput {
    ok: bool,
    session_id: String,
    turn_count: usize,
    last_k: usize,
    #[schemars(with = "Vec<serde_json::Value>")]
    slices: Vec<ToolHistorySliceRow>,
    block: String,
}

#[medousa_tool(id = COGNITION_TOOL_HISTORY_SUMMARY_ID)]
impl CognitionToolHistorySummaryTool {
    /// High-level tool-history slices for recent session turns. Use after reading [MEDOUSA_TOOL_SLICES] at turn start when you need to verify what already ran. Returns slice_id values (turn:N) for detail drill-down.
    async fn invoke_typed(
        &self,
        input: ToolHistorySummaryInput,
    ) -> stasis::prelude::Result<ToolHistorySummaryOutput> {
        let session_id =
            crate::runtime_session::require_active_chat_session_id(
                input.session_id.as_deref(),
                &self.turn_scope,
                COGNITION_TOOL_HISTORY_SUMMARY,
            )
            .await?;
        let last_k = input
            .last_k
            .unwrap_or(DEFAULT_TOOL_HISTORY_SUMMARY_TURNS)
            .clamp(1, 24);
        let tool_filter = input
            .tool_filter
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let keyword = input
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let turns = load_history(&session_id);
        let rows = tool_history_summary_rows(&turns, last_k, tool_filter, keyword);
        let lines: Vec<String> = rows.iter().map(|row| row.line.clone()).collect();

        Ok(ToolHistorySummaryOutput {
            ok: true,
            session_id,
            turn_count: turns.len(),
            last_k,
            slices: rows,
            block: lines.join("\n"),
        })
    }
}

pub struct CognitionToolHistoryDetailTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[derive(Debug, JsonSchema)]
pub struct ToolHistoryDetailInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Turn slice id, e.g. turn:5
    #[schemars(required, with = "String")]
    slice_id: Option<String>,
    /// Optional single tool round
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 1),
        skip_serializing_if = "Option::is_none"
    )]
    tool_round: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "usize",
        range(min = 256, max = 24000),
        skip_serializing_if = "Option::is_none"
    )]
    max_chars: Option<usize>,
}

impl<'de> Deserialize<'de> for ToolHistoryDetailInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            session_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            slice_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            tool_round: Option<usize>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_usize"
            )]
            max_chars: Option<usize>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            session_id: input.session_id,
            slice_id: input.slice_id,
            tool_round: input.tool_round,
            max_chars: input.max_chars,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ToolHistoryDetailOutput {
    ok: bool,
    session_id: String,
    slice_id: String,
    tool_round: Option<usize>,
    detail: String,
}

#[medousa_tool(id = COGNITION_TOOL_HISTORY_DETAIL_ID)]
impl CognitionToolHistoryDetailTool {
    /// Full tool-run detail for one session slice (slice_id=turn:N from summary or [MEDOUSA_TOOL_SLICES]). Optional tool_round for a single round's receipts.
    async fn invoke_typed(
        &self,
        input: ToolHistoryDetailInput,
    ) -> stasis::prelude::Result<ToolHistoryDetailOutput> {
        let session_id =
            crate::runtime_session::require_active_chat_session_id(
                input.session_id.as_deref(),
                &self.turn_scope,
                COGNITION_TOOL_HISTORY_DETAIL,
            )
            .await?;
        let slice_id = input
            .slice_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_tool_history_detail: slice_id is required".to_string())
            })?;
        let tool_round = input.tool_round;
        let max_chars = input
            .max_chars
            .unwrap_or(DEFAULT_TOOL_HISTORY_DETAIL_CHARS)
            .clamp(256, 24_000);

        let turns = load_history(&session_id);
        let detail = tool_history_detail_markdown(&turns, slice_id, tool_round, max_chars)
            .map_err(StasisError::PortFailure)?;

        Ok(ToolHistoryDetailOutput {
            ok: true,
            session_id,
            slice_id: slice_id.to_string(),
            tool_round,
            detail,
        })
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
            speaker_profile_id: None,
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
