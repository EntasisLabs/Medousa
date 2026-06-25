//! Per-turn slice summaries for continuity (Phase 8A).
//!
//! Compact tool-history index persisted on each session turn; injected at turn start
//! as `[MEDOUSA_TOOL_SLICES]` and used to seed the next turn's scratchpad.

use crate::agent_runtime::prompt_prep::truncate_text_for_budget;
use crate::agent_runtime::turn_context::{TurnScratchpad, TurnScratchPhase};
use crate::session::ConversationTurn;
use crate::turn_parts::{TurnPart, compose_turn_markdown};
pub use medousa_types::turn::TurnSliceSummary;
use serde::{Deserialize, Serialize};

pub const TOOL_SLICES_PREFIX: &str = "[MEDOUSA_TOOL_SLICES]";
pub const DEFAULT_SLICE_HOT_LINE_CHARS: usize = 320;
pub const DEFAULT_SLICE_BLOCK_CHARS: usize = 3_000;

/// Ensure a turn has a persisted slice summary (idempotent).
pub fn ensure_turn_slice_summary(
    turn: &ConversationTurn,
    scratch: Option<&TurnScratchpad>,
) -> ConversationTurn {
    if turn.slice_summary.is_some() {
        return turn.clone();
    }
    let mut enriched = turn.clone();
    enriched.slice_summary = Some(compute_slice_summary(turn, scratch));
    enriched
}

pub fn compute_slice_summary(
    turn: &ConversationTurn,
    scratch: Option<&TurnScratchpad>,
) -> TurnSliceSummary {
    if turn.role == "user" {
        return TurnSliceSummary {
            goal: truncate_text_for_budget(turn.content.trim(), 160),
            ..Default::default()
        };
    }

    let (tool_rounds, tools, outcomes, failures) = extract_tool_facts(turn.parts.as_deref());
    let mut summary = TurnSliceSummary {
        goal: infer_goal(turn, scratch),
        tool_rounds,
        tools,
        outcomes,
        failures,
        ..Default::default()
    };

    if let Some(scratch) = scratch {
        merge_scratch_into_summary(&mut summary, scratch);
    } else if summary.goal.is_empty() {
        summary.goal = truncate_text_for_budget(turn.content.trim(), 160);
    }

    summary
}

fn merge_scratch_into_summary(summary: &mut TurnSliceSummary, scratch: &TurnScratchpad) {
    if !scratch.goal.trim().is_empty() {
        summary.goal = truncate_text_for_budget(scratch.goal.trim(), 160);
    }
    summary.scratch_phase = Some(format!("{:?}", scratch.phase).to_ascii_lowercase());
    summary.open_gaps = scratch.open_gaps.clone();
    if let Some(delegate) = scratch.delegate.as_ref() {
        summary.delegate_work_id = Some(delegate.work_id.clone());
        summary.delegate_intent = Some(delegate.intent.clone());
    }
    if summary.tool_rounds == 0 && scratch.step > 0 {
        summary.tool_rounds = scratch.step;
    }
    if summary.tools.is_empty() && !scratch.last_tools.is_empty() {
        summary.tools = scratch.last_tools.clone();
    }
}

fn infer_goal(turn: &ConversationTurn, scratch: Option<&TurnScratchpad>) -> String {
    if let Some(scratch) = scratch {
        if !scratch.goal.trim().is_empty() {
            return truncate_text_for_budget(scratch.goal.trim(), 160);
        }
    }
    if !turn.content.trim().is_empty() {
        return truncate_text_for_budget(turn.content.trim(), 160);
    }
    String::new()
}

fn extract_tool_facts(parts: Option<&[TurnPart]>) -> (usize, Vec<String>, Vec<String>, Vec<String>) {
    let Some(parts) = parts else {
        return (0, Vec::new(), Vec::new(), Vec::new());
    };

    let mut max_round = 0usize;
    let mut tools = Vec::new();
    let mut outcomes = Vec::new();
    let mut failures = Vec::new();

    for part in parts {
        let TurnPart::ToolRun {
            tool_name,
            status,
            output_summary,
            tool_round,
            ..
        } = part
        else {
            continue;
        };
        tools.push(tool_name.clone());
        if let Some(round) = tool_round {
            max_round = max_round.max(*round);
        }
        let status_lower = status.to_ascii_lowercase();
        if status_lower.contains("fail") || status_lower == "error" {
            failures.push(format!("{tool_name} ({status})"));
        }
        if let Some(summary) = output_summary.as_deref().filter(|s| !s.trim().is_empty()) {
            outcomes.push(compact_outcome(tool_name, summary));
        }
    }

    (max_round, tools, outcomes, failures)
}

fn compact_outcome(tool_name: &str, summary: &str) -> String {
    let short = truncate_text_for_budget(summary.trim().replace('\n', " ").as_str(), 96);
    if short.is_empty() {
        tool_name.to_string()
    } else {
        format!("{tool_name}: {short}")
    }
}

fn format_tool_list(tools: &[String]) -> String {
    use std::collections::HashMap;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for name in tools {
        *counts.entry(name.as_str()).or_insert(0) += 1;
    }
    let mut items: Vec<String> = counts
        .into_iter()
        .map(|(name, count)| {
            if count > 1 {
                format!("{name}×{count}")
            } else {
                name.to_string()
            }
        })
        .collect();
    items.sort();
    items.join(",")
}

pub fn format_slice_line(
    turn_index: usize,
    turn: &ConversationTurn,
    summary: &TurnSliceSummary,
    max_chars: usize,
) -> String {
    let role = match turn.role.as_str() {
        "agent" => "assistant",
        other => other,
    };

    let line = if turn.role == "user" {
        format!(
            "turn {turn_index} ({role}): {}",
            truncate_text_for_budget(summary.goal.trim(), max_chars.saturating_sub(24))
        )
    } else if summary.tool_rounds == 0 && summary.tools.is_empty() {
        format!(
            "turn {turn_index} ({role}): {}",
            truncate_text_for_budget(
                if summary.goal.is_empty() {
                    turn.content.trim()
                } else {
                    summary.goal.as_str()
                },
                max_chars.saturating_sub(24)
            )
        )
    } else {
        let mut tail = String::new();
        if !summary.outcomes.is_empty() {
            tail.push_str(" → ");
            tail.push_str(&summary.outcomes.join("; "));
        }
        if !summary.open_gaps.is_empty() {
            tail.push_str(" | pending: ");
            tail.push_str(&summary.open_gaps.join(", "));
        }
        if summary.delegate_work_id.is_some() {
            tail.push_str(" | delegate=");
            tail.push_str(summary.delegate_work_id.as_deref().unwrap_or("?"));
        }
        format!(
            "turn {turn_index} ({role}): rounds={} tools={}{tail}",
            summary.tool_rounds.max(1),
            format_tool_list(&summary.tools)
        )
    };

    truncate_text_for_budget(&line, max_chars)
}

pub fn resolve_slice_summary(turn: &ConversationTurn) -> TurnSliceSummary {
    turn.slice_summary
        .clone()
        .unwrap_or_else(|| compute_slice_summary(turn, None))
}

pub fn build_tool_slices_block(
    all_turns: &[ConversationTurn],
    hot_turns_newest_first: &[&ConversationTurn],
    max_chars: usize,
    line_chars: usize,
) -> String {
    if hot_turns_newest_first.is_empty() || max_chars == 0 {
        return String::new();
    }

    let mut lines = Vec::new();
    for turn in hot_turns_newest_first.iter().rev() {
        let turn_index = turn_session_index(all_turns, turn).unwrap_or(0);
        if turn_index == 0 {
            continue;
        }
        let summary = resolve_slice_summary(turn);
        lines.push(format_slice_line(turn_index, turn, &summary, line_chars));
    }

    if lines.is_empty() {
        return String::new();
    }

    truncate_text_for_budget(
        &format!("{TOOL_SLICES_PREFIX}\n{}", lines.join("\n")),
        max_chars,
    )
}

pub fn format_cold_history_line(turn: &ConversationTurn, content_line_chars: usize) -> Option<String> {
    let role = if turn.role == "agent" {
        "assistant"
    } else {
        turn.role.as_str()
    };

    if role != "user" && role != "assistant" {
        return None;
    }

    let summary = resolve_slice_summary(turn);
    if turn.role == "user" {
        let line = truncate_text_for_budget(summary.goal.trim(), content_line_chars);
        if line.trim().is_empty() {
            return None;
        }
        return Some(format!("{role}: {line}"));
    }

    if !summary.tools.is_empty() {
        let tools = format_tool_list(&summary.tools);
        let line = truncate_text_for_budget(
            &format!("rounds={} tools={tools}", summary.tool_rounds.max(1)),
            content_line_chars,
        );
        return Some(format!("{role}: {line}"));
    }

    let line = truncate_text_for_budget(
        if summary.goal.is_empty() {
            turn.content.trim()
        } else {
            summary.goal.as_str()
        },
        content_line_chars,
    );
    if line.trim().is_empty() {
        return None;
    }
    Some(format!("{role}: {}", line.replace('\n', " ")))
}

/// Seed host scratch from the last assistant slice + current user prompt.
pub fn session_scratch_seed_from_history(
    turns: &[ConversationTurn],
    current_prompt: &str,
) -> TurnScratchpad {
    let mut scratch = TurnScratchpad::from_user_prompt(current_prompt);

    let last_assistant = turns
        .iter()
        .rev()
        .find(|turn| turn.role == "assistant" || turn.role == "agent");

    let Some(turn) = last_assistant else {
        return scratch;
    };

    let summary = resolve_slice_summary(turn);
    if !summary.goal.trim().is_empty() {
        scratch.goal = summary.goal.clone();
    }
    if !summary.open_gaps.is_empty() {
        scratch.open_gaps = summary.open_gaps.clone();
    }
    if let (Some(work_id), Some(intent)) = (
        summary.delegate_work_id.as_ref(),
        summary.delegate_intent.as_ref(),
    ) {
        scratch.set_delegate(work_id.clone(), intent.clone());
    } else if summary.tool_rounds > 0 {
        scratch.phase = TurnScratchPhase::Execute;
        scratch.step = summary.tool_rounds;
        scratch.last_tools = summary.tools.clone();
    }

    scratch
}

fn turn_session_index(all_turns: &[ConversationTurn], turn: &ConversationTurn) -> Option<usize> {
    all_turns
        .iter()
        .position(|candidate| {
            candidate.timestamp == turn.timestamp
                && candidate.role == turn.role
                && candidate.content == turn.content
        })
        .map(|idx| idx + 1)
}

/// 1-based turn index in session history (matches `[MEDOUSA_TOOL_SLICES]` lines).
pub fn session_turn_index(all_turns: &[ConversationTurn], turn: &ConversationTurn) -> Option<usize> {
    turn_session_index(all_turns, turn)
}

pub fn format_slice_id(turn_index: usize) -> String {
    format!("turn:{turn_index}")
}

pub fn parse_turn_index_from_slice_id(slice_id: &str) -> Option<usize> {
    let trimmed = slice_id.trim();
    if let Some(rest) = trimmed.strip_prefix("turn:") {
        return rest.parse().ok().filter(|index| *index > 0);
    }
    if let Some(pos) = trimmed.rfind(":turn:") {
        return trimmed[pos + 6..]
            .parse()
            .ok()
            .filter(|index| *index > 0);
    }
    trimmed.parse().ok().filter(|index| *index > 0)
}

pub const DEFAULT_TOOL_HISTORY_SUMMARY_TURNS: usize = 5;
pub const DEFAULT_TOOL_HISTORY_DETAIL_CHARS: usize = 12_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolHistorySliceRow {
    pub slice_id: String,
    pub turn_index: usize,
    pub role: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub summary: TurnSliceSummary,
    pub line: String,
}

pub fn tool_history_summary_rows(
    turns: &[ConversationTurn],
    last_k: usize,
    tool_filter: Option<&str>,
    keyword: Option<&str>,
) -> Vec<ToolHistorySliceRow> {
    let last_k = last_k.max(1);
    let tool_filter = tool_filter.map(|value| value.to_ascii_lowercase());
    let keyword = keyword.map(|value| value.to_ascii_lowercase());

    turns
        .iter()
        .rev()
        .take(last_k)
        .rev()
        .filter_map(|turn| {
            let turn_index = session_turn_index(turns, turn)?;
            let summary = resolve_slice_summary(turn);
            if let Some(ref tool) = tool_filter {
                if !summary.tools.iter().any(|name| name.to_ascii_lowercase().contains(tool))
                    && !turn
                        .tool_names
                        .iter()
                        .any(|name| name.to_ascii_lowercase().contains(tool))
                {
                    return None;
                }
            }
            let line = format_slice_line(turn_index, turn, &summary, DEFAULT_SLICE_HOT_LINE_CHARS);
            if let Some(ref kw) = keyword {
                let haystack = format!(
                    "{} {} {} {:?}",
                    line, summary.goal, turn.content, summary.outcomes
                )
                .to_ascii_lowercase();
                if !haystack.contains(kw.as_str()) {
                    return None;
                }
            }
            Some(ToolHistorySliceRow {
                slice_id: format_slice_id(turn_index),
                turn_index,
                role: turn.role.clone(),
                timestamp: turn.timestamp,
                summary,
                line,
            })
        })
        .collect()
}

pub fn tool_history_detail_markdown(
    turns: &[ConversationTurn],
    slice_id: &str,
    tool_round: Option<usize>,
    max_chars: usize,
) -> Result<String, String> {
    let turn_index = parse_turn_index_from_slice_id(slice_id)
        .ok_or_else(|| format!("invalid slice_id '{slice_id}' (expected turn:N)"))?;
    let turn = turns.get(turn_index.saturating_sub(1)).ok_or_else(|| {
        format!("turn index {turn_index} out of range (session has {} turns)", turns.len())
    })?;

    let body = if let Some(round) = tool_round {
        detail_for_tool_round(turn, round)?
    } else if turn.parts.as_ref().is_some_and(|parts| !parts.is_empty()) {
        compose_turn_markdown(turn)
    } else {
        turn.content.clone()
    };

    Ok(truncate_text_for_budget(body.trim(), max_chars.max(256)))
}

fn detail_for_tool_round(turn: &ConversationTurn, tool_round: usize) -> Result<String, String> {
    let parts = turn
        .parts
        .as_deref()
        .ok_or_else(|| "turn has no structured tool parts".to_string())?;
    let mut out = String::new();
    for part in parts {
        let TurnPart::ToolRun {
            tool_name,
            status,
            input_summary,
            output_summary,
            tool_round: round,
            ..
        } = part
        else {
            continue;
        };
        if round.unwrap_or(0) != tool_round {
            continue;
        }
        out.push_str(&format!("Tool: {tool_name} ({status})\nInput: {input_summary}\n"));
        if let Some(summary) = output_summary.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!("Output: {summary}\n"));
        }
    }
    if out.trim().is_empty() {
        return Err(format!("no tool runs for round {tool_round} on this turn"));
    }
    Ok(out)
}

/// Attach compact slice index to worker handoff (Phase 8C).
pub fn enrich_handoff_tool_history(capsule: &mut crate::agent_runtime::turn_context::WorkerHandoffCapsule, turns: &[ConversationTurn]) {
    const HANDOFF_SLICE_WINDOW: usize = 8;
    let hot_newest_first: Vec<&ConversationTurn> = turns.iter().rev().take(HANDOFF_SLICE_WINDOW).collect();
    capsule.relevant_slice_ids = hot_newest_first
        .iter()
        .rev()
        .filter_map(|turn| {
            let index = session_turn_index(turns, turn)?;
            let summary = resolve_slice_summary(turn);
            if turn.role == "user" || !summary.tools.is_empty() || summary.tool_rounds > 0 {
                Some(format_slice_id(index))
            } else {
                None
            }
        })
        .collect();
    let excerpt = build_tool_slices_block(
        turns,
        &hot_newest_first,
        2_000,
        DEFAULT_SLICE_HOT_LINE_CHARS,
    );
    capsule.tool_history_excerpt = if excerpt.trim().is_empty() {
        None
    } else {
        Some(excerpt)
    };
}

pub fn prior_turn_content(turn: &ConversationTurn, max_chars: usize) -> String {
    let raw = if turn.role == "user" {
        turn.content.clone()
    } else if turn.parts.as_ref().is_some_and(|parts| !parts.is_empty()) {
        compose_turn_markdown(turn)
    } else {
        turn.content.clone()
    };
    truncate_text_for_budget(&raw, max_chars)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::turn_parts::TurnPart;

    fn assistant_with_tools() -> ConversationTurn {
        ConversationTurn {
            role: "assistant".to_string(),
            content: "I'll spin up workers next.".to_string(),
            timestamp: Utc::now(),
            tool_names: vec![
                "cognition_manuscript_list".to_string(),
                "cognition_manuscript_resolve".to_string(),
            ],
            answer_state: None,
            parts: Some(vec![
                TurnPart::ToolRun {
                    run_id: "r1".to_string(),
                    tool_name: "cognition_manuscript_list".to_string(),
                    status: "succeeded".to_string(),
                    input_summary: "list".to_string(),
                    output_summary: Some("base-researcher available".to_string()),
                    artifact_refs: vec![],
                    tool_round: Some(1),
                    started_at: Utc::now(),
                    finished_at: None,
                },
                TurnPart::ToolRun {
                    run_id: "r2".to_string(),
                    tool_name: "cognition_manuscript_resolve".to_string(),
                    status: "succeeded".to_string(),
                    input_summary: "id=base-researcher".to_string(),
                    output_summary: Some("resolved".to_string()),
                    artifact_refs: vec![],
                    tool_round: Some(2),
                    started_at: Utc::now(),
                    finished_at: None,
                },
                TurnPart::Text {
                    markdown: "I'll spin up workers next.".to_string(),
                },
            ]),
            slice_summary: None,
        }
    }

    #[test]
    fn compute_slice_summary_from_parts() {
        let turn = assistant_with_tools();
        let summary = compute_slice_summary(&turn, None);
        assert_eq!(summary.tool_rounds, 2);
        assert_eq!(summary.tools.len(), 2);
        assert!(summary
            .outcomes
            .iter()
            .any(|o| o.contains("manuscript_resolve")));
    }

    #[test]
    fn tool_slices_block_lists_hot_turns() {
        let user = ConversationTurn {
            role: "user".to_string(),
            content: "spin them up".to_string(),
            timestamp: Utc::now(),
            tool_names: vec![],
            answer_state: None,
            parts: None,
            slice_summary: None,
        };
        let assistant = assistant_with_tools();
        let turns = vec![user.clone(), assistant.clone()];
        let hot = vec![&assistant, &user];
        let block = build_tool_slices_block(&turns, &hot, 2000, 320);
        assert!(block.starts_with(TOOL_SLICES_PREFIX));
        assert!(block.contains("manuscript_list"));
        assert!(block.contains("spin them up"));
    }

    #[test]
    fn session_scratch_seed_carries_goal_and_gaps() {
        let mut turn = assistant_with_tools();
        turn.slice_summary = Some(TurnSliceSummary {
            goal: "scope research workers".to_string(),
            open_gaps: vec!["spawn workers".to_string()],
            tool_rounds: 2,
            tools: vec!["cognition_manuscript_list".to_string()],
            ..Default::default()
        });
        let seed = session_scratch_seed_from_history(&[turn], "do it now");
        assert_eq!(seed.goal, "scope research workers");
        assert_eq!(seed.open_gaps, vec!["spawn workers".to_string()]);
    }

    #[test]
    fn prior_turn_content_uses_compose_markdown() {
        let turn = assistant_with_tools();
        let body = prior_turn_content(&turn, 4000);
        assert!(body.contains("Tool: cognition_manuscript_list"));
        assert!(body.contains("spin up workers"));
    }

    #[test]
    fn slice_id_parse_and_format() {
        assert_eq!(format_slice_id(5), "turn:5");
        assert_eq!(parse_turn_index_from_slice_id("turn:5"), Some(5));
        assert_eq!(
            parse_turn_index_from_slice_id("session:abc:turn:12"),
            Some(12)
        );
    }
}
