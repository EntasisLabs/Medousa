//! On-demand session tool history (Phase 8C) — summary + detail by slice id.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use crate::semantic_values::TrimmedText;
use crate::session_history::load_history;
use crate::turn_slice::{
    DEFAULT_TOOL_HISTORY_DETAIL_CHARS, DEFAULT_TOOL_HISTORY_SUMMARY_TURNS, ToolHistorySliceRow,
    tool_history_detail_markdown, tool_history_summary_rows,
};
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

pub const COGNITION_TOOL_HISTORY_SUMMARY: &str = "cognition_tool_history_summary";
pub const COGNITION_TOOL_HISTORY_DETAIL: &str = "cognition_tool_history_detail";
const COGNITION_TOOL_HISTORY_SUMMARY_ID: ToolId = ToolId::new(COGNITION_TOOL_HISTORY_SUMMARY);
const COGNITION_TOOL_HISTORY_DETAIL_ID: ToolId = ToolId::new(COGNITION_TOOL_HISTORY_DETAIL);

fn optional_trimmed(value: Option<String>) -> Option<TrimmedText> {
    value.and_then(|value| TrimmedText::new(value).ok())
}

fn required_slice_id(value: Option<String>) -> Result<TrimmedText, StasisError> {
    let value = value.ok_or_else(|| {
        StasisError::PortFailure("cognition_tool_history_detail: slice_id is required".to_string())
    })?;
    TrimmedText::new(value).map_err(|_| {
        StasisError::PortFailure("cognition_tool_history_detail: slice_id is required".to_string())
    })
}

pub fn register_tool_history_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionToolHistorySummaryTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_typed_tool(CognitionToolHistoryDetailTool { turn_scope })?;
    Ok(())
}

pub struct CognitionToolHistorySummaryTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolHistorySummaryInput {
    /// Session id (defaults to active turn session)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    session_id: CompatOption<String>,
    /// Recent turns to include (default 5)
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 24),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    last_k: CompatOption<usize>,
    /// Optional substring filter on tool names
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    tool_filter: CompatOption<String>,
    /// Optional keyword filter on slice line / goal / outcomes
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    keyword: CompatOption<String>,
}

#[derive(Debug)]
struct ToolHistorySummaryCommand {
    session_id: Option<TrimmedText>,
    last_k: usize,
    tool_filter: Option<TrimmedText>,
    keyword: Option<TrimmedText>,
}

impl TryFrom<ToolHistorySummaryInput> for ToolHistorySummaryCommand {
    type Error = StasisError;

    fn try_from(input: ToolHistorySummaryInput) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: optional_trimmed(input.session_id.into_option()),
            last_k: input
                .last_k
                .into_option()
                .unwrap_or(DEFAULT_TOOL_HISTORY_SUMMARY_TURNS)
                .clamp(1, 24),
            tool_filter: optional_trimmed(input.tool_filter.into_option()),
            keyword: optional_trimmed(input.keyword.into_option()),
        })
    }
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
    /// Summarize tool activity for recent session turns. Returns slice_id values for cognition_tool_history_detail.
    async fn invoke_typed(
        &self,
        input: ToolHistorySummaryInput,
    ) -> stasis::prelude::Result<ToolHistorySummaryOutput> {
        let command = ToolHistorySummaryCommand::try_from(input)?;
        let session_id = crate::runtime_session::require_active_chat_session_id(
            command.session_id.as_ref().map(TrimmedText::as_str),
            &self.turn_scope,
            COGNITION_TOOL_HISTORY_SUMMARY,
        )
        .await?;
        let last_k = command.last_k;
        let tool_filter = command.tool_filter.as_ref().map(TrimmedText::as_str);
        let keyword = command.keyword.as_ref().map(TrimmedText::as_str);

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
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[derive(Debug, JsonSchema)]
pub struct ToolHistoryDetailInput {
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    session_id: CompatOption<String>,
    /// Turn slice id, e.g. turn:5
    #[schemars(required, with = "String")]
    slice_id: CompatOption<String>,
    /// Optional single tool round
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    tool_round: CompatOption<usize>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 256, max = 24000),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    max_chars: CompatOption<usize>,
}

impl<'de> Deserialize<'de> for ToolHistoryDetailInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            session_id: CompatOption<String>,
            #[serde(default)]
            slice_id: CompatOption<String>,
            #[serde(default)]
            tool_round: CompatOption<usize>,
            #[serde(default)]
            max_chars: CompatOption<usize>,
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

#[derive(Debug)]
struct ToolHistoryDetailCommand {
    session_id: Option<TrimmedText>,
    slice_id: TrimmedText,
    tool_round: Option<usize>,
    max_chars: usize,
}

impl TryFrom<ToolHistoryDetailInput> for ToolHistoryDetailCommand {
    type Error = StasisError;

    fn try_from(input: ToolHistoryDetailInput) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: optional_trimmed(input.session_id.into_option()),
            slice_id: required_slice_id(input.slice_id.into_option())?,
            tool_round: input.tool_round.into_option(),
            max_chars: input
                .max_chars
                .into_option()
                .unwrap_or(DEFAULT_TOOL_HISTORY_DETAIL_CHARS)
                .clamp(256, 24_000),
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
    /// Read tool-run details for one session slice. Optional tool_round selects one round.
    async fn invoke_typed(
        &self,
        input: ToolHistoryDetailInput,
    ) -> stasis::prelude::Result<ToolHistoryDetailOutput> {
        let command = ToolHistoryDetailCommand::try_from(input)?;
        let session_id = crate::runtime_session::require_active_chat_session_id(
            command.session_id.as_ref().map(TrimmedText::as_str),
            &self.turn_scope,
            COGNITION_TOOL_HISTORY_DETAIL,
        )
        .await?;
        let slice_id = command.slice_id.into_string();
        let tool_round = command.tool_round;
        let max_chars = command.max_chars;

        let turns = load_history(&session_id);
        let detail = tool_history_detail_markdown(&turns, &slice_id, tool_round, max_chars)
            .map_err(StasisError::PortFailure)?;

        Ok(ToolHistoryDetailOutput {
            ok: true,
            session_id,
            slice_id,
            tool_round,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_history::ConversationTurn;
    use crate::turn_parts::TurnPart;
    use chrono::Utc;

    #[test]
    fn summary_and_detail_round_trip() {
        let turns = vec![ConversationTurn {
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
        }];
        let rows = tool_history_summary_rows(&turns, 5, None, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slice_id, "turn:1");
        let detail = tool_history_detail_markdown(&turns, "turn:1", Some(1), 4000).unwrap();
        assert!(detail.contains("cognition_manuscript_list"));
        assert!(detail.contains("base-researcher"));
    }

    #[test]
    fn history_commands_normalize_filters_and_bound_detail_reads() {
        let summary = ToolHistorySummaryCommand::try_from(ToolHistorySummaryInput {
            session_id: Some(" session-a ".to_string()).into(),
            last_k: Some(999).into(),
            tool_filter: Some(" cognition_ ".to_string()).into(),
            keyword: Some(" outcome ".to_string()).into(),
        })
        .expect("summary command");
        assert_eq!(
            summary.session_id.as_ref().map(TrimmedText::as_str),
            Some("session-a")
        );
        assert_eq!(summary.last_k, 24);
        assert_eq!(
            summary.tool_filter.as_ref().map(TrimmedText::as_str),
            Some("cognition_")
        );

        let detail = ToolHistoryDetailCommand::try_from(ToolHistoryDetailInput {
            session_id: Some(" session-a ".to_string()).into(),
            slice_id: Some(" turn:4 ".to_string()).into(),
            tool_round: Some(2).into(),
            max_chars: Some(99_999).into(),
        })
        .expect("detail command");
        assert_eq!(detail.slice_id.as_str(), "turn:4");
        assert_eq!(detail.max_chars, 24_000);
    }

    #[test]
    fn history_detail_command_rejects_blank_slice_id() {
        let error = ToolHistoryDetailCommand::try_from(ToolHistoryDetailInput {
            session_id: None.into(),
            slice_id: Some(" \n\t".to_string()).into(),
            tool_round: None.into(),
            max_chars: None.into(),
        })
        .expect_err("blank slice id should fail");
        assert!(error.to_string().contains("slice_id is required"));
    }

    #[test]
    fn history_wire_optionals_remain_lenient_for_legacy_values() {
        let summary: ToolHistorySummaryInput = serde_json::from_value(serde_json::json!({
            "session_id": 42,
            "last_k": "24",
            "tool_filter": false,
            "keyword": [],
        }))
        .expect("summary input");
        assert!(summary.session_id.into_option().is_none());
        assert!(summary.last_k.into_option().is_none());
        assert!(summary.tool_filter.into_option().is_none());
        assert!(summary.keyword.into_option().is_none());

        let detail: ToolHistoryDetailInput = serde_json::from_value(serde_json::json!({
            "session_id": 9,
            "slice_id": false,
            "tool_round": "2",
            "max_chars": [],
        }))
        .expect("detail input");
        assert!(detail.session_id.into_option().is_none());
        assert!(detail.slice_id.into_option().is_none());
        assert!(detail.tool_round.into_option().is_none());
        assert!(detail.max_chars.into_option().is_none());
    }
}
