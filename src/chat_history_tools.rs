//! Read-only, profile-scoped access to durable Medousa chat history.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use crate::semantic_values::TrimmedText;
use crate::session::{ConversationTurn, SessionHistorySummary, load_history};
use crate::turn_parts::TurnPart;
use crate::typed_tools::{CompatOption, ToolId, medousa_tool};

pub const COGNITION_CHAT_HISTORY_SEARCH: &str = "cognition_chat_history_search";
pub const COGNITION_CHAT_HISTORY_READ: &str = "cognition_chat_history_read";

const COGNITION_CHAT_HISTORY_SEARCH_ID: ToolId = ToolId::new(COGNITION_CHAT_HISTORY_SEARCH);
const COGNITION_CHAT_HISTORY_READ_ID: ToolId = ToolId::new(COGNITION_CHAT_HISTORY_READ);
const DEFAULT_SEARCH_LIMIT: usize = 8;
const MAX_SEARCH_LIMIT: usize = 20;
const DEFAULT_SESSION_SCAN_LIMIT: usize = 80;
const MAX_SESSION_SCAN_LIMIT: usize = 200;
const MAX_TURNS_SCANNED_PER_SESSION: usize = 120;
const DEFAULT_READ_TURNS: usize = 12;
const MAX_READ_TURNS: usize = 40;
const DEFAULT_READ_CHARS: usize = 12_000;
const MAX_READ_CHARS: usize = 24_000;
const MAX_MESSAGE_CHARS: usize = 2_400;
#[cfg(test)]
const SEARCH_EXCERPT_CHARS: usize = 420;

pub fn register_chat_history_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionChatHistorySearchTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_typed_tool(CognitionChatHistoryReadTool { turn_scope })?;
    Ok(())
}

#[derive(Debug, Clone)]
struct HistoryAccess {
    source_session_id: String,
    profile_id: String,
}

async fn history_access(
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    tool_name: &str,
) -> Result<HistoryAccess, StasisError> {
    let scope = crate::agent_runtime::execution_context::turn_continuation_scope(turn_scope)
        .await
        .ok_or_else(|| {
            StasisError::PortFailure(format!("{tool_name}: active turn scope required"))
        })?;
    let source_session_id = TrimmedText::new(scope.session_id)
        .map(TrimmedText::into_string)
        .map_err(|_| StasisError::PortFailure(format!("{tool_name}: active session required")))?;
    let profile_id = scope
        .identity_user_id
        .and_then(|value| TrimmedText::new(value).ok())
        .map(TrimmedText::into_string)
        .ok_or_else(|| {
            StasisError::PortFailure(format!(
                "{tool_name}: active principal identity required for cross-session reads"
            ))
        })?;
    Ok(HistoryAccess {
        source_session_id,
        profile_id,
    })
}

fn require_visible_session(
    access: &HistoryAccess,
    target_session_id: &str,
    tool_name: &str,
) -> Result<(), StasisError> {
    if crate::session_catalog::session_visible_to_profile(target_session_id, &access.profile_id) {
        return Ok(());
    }
    Err(StasisError::PortFailure(format!(
        "{tool_name}: session not found or not visible to the active profile"
    )))
}

fn visible_turn_text(turn: &ConversationTurn) -> Option<String> {
    if !matches!(turn.role.as_str(), "user" | "assistant" | "agent") {
        return None;
    }
    let content = turn.content.trim();
    if !content.is_empty() {
        return Some(content.to_string());
    }
    let parts = turn.parts.as_deref()?;
    let visible = parts
        .iter()
        .filter_map(|part| match part {
            TurnPart::Text { markdown, .. } | TurnPart::Progress { markdown } => {
                Some(markdown.trim())
            }
            TurnPart::Handoff { text, .. } => Some(text.trim()),
            TurnPart::Reasoning { .. }
            | TurnPart::ModelReceipt { .. }
            | TurnPart::ToolRun { .. }
            | TurnPart::UserMedia { .. }
            | TurnPart::HostContext { .. }
            | TurnPart::AttachmentRef { .. }
            | TurnPart::Unknown => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!visible.is_empty()).then_some(visible)
}

fn display_role(role: &str) -> String {
    if role == "agent" {
        "assistant".to_string()
    } else {
        role.to_string()
    }
}

fn bounded_text(text: &str, max_chars: usize) -> String {
    crate::agent_runtime::prompt_prep::truncate_text_for_budget(text, max_chars)
}

#[cfg(test)]
fn search_excerpt(text: &str, query: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = collapsed.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let start = lower
        .find(&needle)
        .map(|byte| collapsed[..byte].chars().count().saturating_sub(90))
        .unwrap_or(0);
    let excerpt = collapsed
        .chars()
        .skip(start)
        .take(SEARCH_EXCERPT_CHARS)
        .collect::<String>();
    if start > 0 {
        format!("…{excerpt}")
    } else {
        excerpt
    }
}

fn metadata_matches(summary: &SessionHistorySummary, query: &str) -> bool {
    let query = query.to_ascii_lowercase();
    summary.session_id.to_ascii_lowercase().contains(&query)
        || summary.preview.to_ascii_lowercase().contains(&query)
        || summary
            .display_name
            .as_ref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(&query))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChatHistorySearchInput {
    /// Optional text to search across visible chat prose. Omit to list recent chats.
    #[serde(default)]
    #[schemars(with = "String", skip_serializing_if = "CompatOption::is_none")]
    query: CompatOption<String>,
    /// Maximum results (default 8, maximum 20).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 20),
        skip_serializing_if = "CompatOption::is_none"
    )]
    limit: CompatOption<usize>,
    /// Maximum recent sessions searched when query is present (default 80, maximum 200).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 200),
        skip_serializing_if = "CompatOption::is_none"
    )]
    session_scan_limit: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistorySearchMatch {
    session_id: String,
    display_name: Option<String>,
    preview: String,
    last_activity_at: Option<String>,
    turn_count: usize,
    turn_index: Option<usize>,
    role: Option<String>,
    excerpt: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistorySearchOutput {
    ok: bool,
    source_session_id: String,
    query: Option<String>,
    scanned_sessions: usize,
    results: Vec<ChatHistorySearchMatch>,
}

pub struct CognitionChatHistorySearchTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[medousa_tool(id = COGNITION_CHAT_HISTORY_SEARCH_ID)]
impl CognitionChatHistorySearchTool {
    /// Search prior Medousa chats visible to the active profile. Omit query to list recent chats. Searches only user/assistant prose; reasoning traces and raw tool receipts are excluded.
    async fn invoke_typed(
        &self,
        input: ChatHistorySearchInput,
    ) -> stasis::prelude::Result<ChatHistorySearchOutput> {
        let access = history_access(&self.turn_scope, COGNITION_CHAT_HISTORY_SEARCH).await?;
        let query = input
            .query
            .into_option()
            .and_then(|value| TrimmedText::new(value).ok())
            .map(TrimmedText::into_string);
        let limit = input
            .limit
            .into_option()
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let scan_limit = input
            .session_scan_limit
            .into_option()
            .unwrap_or(DEFAULT_SESSION_SCAN_LIMIT)
            .clamp(1, MAX_SESSION_SCAN_LIMIT);

        let page = crate::session::list_history_sessions_page_for_profile(
            Some(&access.profile_id),
            if query.is_some() { scan_limit } else { limit },
            None,
            None,
        );
        let scanned_sessions = page.sessions.len();
        let mut results = Vec::new();
        if query.is_none() {
            for summary in page.sessions {
                results.push(search_match_from_summary(summary, None, None, None));
            }
        } else if let Some(query_text) = query.as_deref() {
            let mut summaries = page
                .sessions
                .into_iter()
                .map(|summary| (summary.session_id.clone(), summary))
                .collect::<std::collections::HashMap<_, _>>();
            let session_ids = summaries.keys().cloned().collect::<Vec<_>>();
            let candidate_limit = scan_limit.saturating_mul(MAX_TURNS_SCANNED_PER_SESSION);
            let transcript_hits = crate::session_store::get_session_store()
                .search_transcripts(&session_ids, query_text, candidate_limit)
                .map_err(|error| {
                    StasisError::PortFailure(format!("{COGNITION_CHAT_HISTORY_SEARCH}: {error}"))
                })?;
            for hit in transcript_hits {
                if results.len() >= limit {
                    break;
                }
                let Some(summary) = summaries.remove(&hit.session_id) else {
                    continue;
                };
                results.push(search_match_from_summary(
                    summary,
                    None,
                    Some(hit.role),
                    Some(hit.excerpt),
                ));
            }

            if results.len() < limit {
                let mut metadata_hits = summaries
                    .into_values()
                    .filter(|summary| metadata_matches(summary, query_text))
                    .collect::<Vec<_>>();
                metadata_hits.sort_by(|left, right| right.last_timestamp.cmp(&left.last_timestamp));
                for summary in metadata_hits.into_iter().take(limit - results.len()) {
                    results.push(search_match_from_summary(summary, None, None, None));
                }
            }
        }

        Ok(ChatHistorySearchOutput {
            ok: true,
            source_session_id: access.source_session_id,
            query,
            scanned_sessions,
            results,
        })
    }
}

fn search_match_from_summary(
    summary: SessionHistorySummary,
    turn_index: Option<usize>,
    role: Option<String>,
    excerpt: Option<String>,
) -> ChatHistorySearchMatch {
    ChatHistorySearchMatch {
        session_id: summary.session_id,
        display_name: summary.display_name,
        preview: bounded_text(&summary.preview, 160),
        last_activity_at: summary.last_timestamp.map(|value| value.to_rfc3339()),
        turn_count: summary.turns,
        turn_index,
        role,
        excerpt,
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChatHistoryReadInput {
    /// Session returned by cognition_chat_history_search.
    session_id: String,
    /// Number of most recent visible turns (default 12, maximum 40).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 40),
        skip_serializing_if = "CompatOption::is_none"
    )]
    last_k: CompatOption<usize>,
    /// Total prose budget (default 12000, maximum 24000 characters).
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 512, max = 24000),
        skip_serializing_if = "CompatOption::is_none"
    )]
    max_chars: CompatOption<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistoryMessage {
    turn_index: usize,
    role: String,
    timestamp: String,
    content: String,
    tool_names: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChatHistoryReadOutput {
    ok: bool,
    session_id: String,
    display_name: Option<String>,
    total_turns: usize,
    returned_turns: usize,
    truncated: bool,
    messages: Vec<ChatHistoryMessage>,
}

pub struct CognitionChatHistoryReadTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

#[medousa_tool(id = COGNITION_CHAT_HISTORY_READ_ID)]
impl CognitionChatHistoryReadTool {
    /// Read a bounded window from one prior Medousa chat. Returns user/assistant prose and compact tool names only; reasoning traces and raw tool receipts are never returned.
    async fn invoke_typed(
        &self,
        input: ChatHistoryReadInput,
    ) -> stasis::prelude::Result<ChatHistoryReadOutput> {
        let access = history_access(&self.turn_scope, COGNITION_CHAT_HISTORY_READ).await?;
        let session_id = TrimmedText::new(input.session_id)
            .map(TrimmedText::into_string)
            .map_err(|_| {
                StasisError::PortFailure(
                    "cognition_chat_history_read: session_id is required".to_string(),
                )
            })?;
        require_visible_session(&access, &session_id, COGNITION_CHAT_HISTORY_READ)?;
        let last_k = input
            .last_k
            .into_option()
            .unwrap_or(DEFAULT_READ_TURNS)
            .clamp(1, MAX_READ_TURNS);
        let max_chars = input
            .max_chars
            .into_option()
            .unwrap_or(DEFAULT_READ_CHARS)
            .clamp(512, MAX_READ_CHARS);

        let turns = load_history(&session_id);
        let total_turns = turns.len();
        let mut visible = turns
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, turn)| {
                visible_turn_text(turn).map(|content| (index, turn, content))
            })
            .take(last_k + 1)
            .collect::<Vec<_>>();
        let mut truncated = visible.len() > last_k;
        visible.truncate(last_k);
        visible.reverse();

        let mut remaining = max_chars;
        let mut messages = Vec::new();
        for (index, turn, content) in visible {
            if remaining == 0 {
                truncated = true;
                break;
            }
            let per_message = remaining.min(MAX_MESSAGE_CHARS);
            let bounded = bounded_text(&content, per_message);
            truncated |= bounded.chars().count() < content.chars().count();
            remaining = remaining.saturating_sub(bounded.chars().count());
            messages.push(ChatHistoryMessage {
                turn_index: index + 1,
                role: display_role(&turn.role),
                timestamp: turn.timestamp.to_rfc3339(),
                content: bounded,
                tool_names: turn.tool_names.iter().take(16).cloned().collect(),
            });
        }

        let display_name = crate::session_catalog::get_summary(&session_id)
            .and_then(|summary| summary.display_name)
            .or_else(|| {
                crate::shared_session_catalog::get_shared_row(&session_id)
                    .and_then(|row| row.display_name)
            });
        let returned_turns = messages.len();
        Ok(ChatHistoryReadOutput {
            ok: true,
            session_id,
            display_name,
            total_turns,
            returned_turns,
            truncated,
            messages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn turn(role: &str, content: &str, parts: Option<Vec<TurnPart>>) -> ConversationTurn {
        ConversationTurn {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_names: Vec::new(),
            answer_state: None,
            parts,
            slice_summary: None,
            speaker_profile_id: None,
        }
    }

    #[test]
    fn visible_text_never_returns_reasoning_or_tool_receipts() {
        let only_private = turn(
            "assistant",
            "",
            Some(vec![
                TurnPart::Reasoning {
                    markdown: "private chain".to_string(),
                },
                TurnPart::ToolRun {
                    run_id: "run-1".to_string(),
                    tool_name: "secret_tool".to_string(),
                    status: "succeeded".to_string(),
                    input_summary: "private input".to_string(),
                    output_summary: Some("private output".to_string()),
                    artifact_refs: Vec::new(),
                    tool_round: Some(1),
                    started_at: Utc::now(),
                    finished_at: Some(Utc::now()),
                },
            ]),
        );
        assert_eq!(visible_turn_text(&only_private), None);

        let mixed = turn(
            "assistant",
            "",
            Some(vec![
                TurnPart::Reasoning {
                    markdown: "private chain".to_string(),
                },
                TurnPart::Text {
                    markdown: "visible answer".to_string(),
                    segment_id: None,
                    model_round: None,
                },
            ]),
        );
        assert_eq!(visible_turn_text(&mixed).as_deref(), Some("visible answer"));
    }

    #[test]
    fn excerpt_centers_the_matching_text_and_stays_bounded() {
        let text = format!(
            "{} pager sentinel {}",
            "before ".repeat(80),
            "after ".repeat(80)
        );
        let excerpt = search_excerpt(&text, "pager sentinel");
        assert!(excerpt.contains("pager sentinel"));
        assert!(excerpt.chars().count() <= SEARCH_EXCERPT_CHARS + 1);
    }

    #[tokio::test]
    async fn cross_session_access_requires_a_turn_principal() {
        let scope = crate::agent_runtime::execution_context::TurnScopeAccess::for_test(
            crate::turn_continuation::TurnContinuationScope {
                turn_correlation_id: "turn-1".to_string(),
                session_id: "session-a".to_string(),
                identity_user_id: None,
                original_prompt: "find the pager chat".to_string(),
                delivery_target: None,
                provider: "test".to_string(),
                model: "test".to_string(),
                response_depth_mode: "standard".to_string(),
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
                channel_surface: None,
            },
        );
        let error = history_access(&scope, COGNITION_CHAT_HISTORY_SEARCH)
            .await
            .expect_err("missing principal must be denied");
        assert!(error.to_string().contains("principal identity required"));
    }
}
