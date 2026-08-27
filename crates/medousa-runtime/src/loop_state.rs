//! Portable state and records for the bounded foreground loop.

use chrono::{DateTime, Utc};
use genai::chat::ChatMessage;
use medousa_engine::TurnScratchpad;
use serde::{Deserialize, Serialize};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::completion_fsm::ContinueReason;

/// Default when a composition does not provide its own text-only limit.
pub const MAX_TEXT_ONLY_STUCK_CONTINUES: usize = 3;
pub const USER_RESPONSE_PREVIEW_MAX_CHARS: usize = 100;
pub const TURN_CONTROL_PREFIX: &str = "[MEDOUSA_TURN_CONTROL]";

pub fn resolve_max_text_only_stuck_continues(max_tool_rounds: usize) -> usize {
    max_tool_rounds.max(1)
}

pub fn push_turn_control_message(messages: &mut Vec<ChatMessage>, body: &str) {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    messages.push(ChatMessage::system(format!(
        "{TURN_CONTROL_PREFIX}\\n{trimmed}"
    )));
}

pub fn ledger_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    invocations
        .iter()
        .map(|invocation| invocation.tool_name.clone())
        .collect()
}

/// Dynamic loop HUD appended to interactive tool-loop prompts.
pub fn append_tool_loop_policy(prompt: &str, max_tool_rounds: usize) -> String {
    let max_tool_rounds = max_tool_rounds.max(1);
    format!(
        "{prompt}\n\n[MEDOUSA_HUD]\n\
         max_tool_rounds={max_tool_rounds}"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnLedgerEventKind {
    ToolRound,
    TextOnlyContinue,
    GatekeeperContinue,
    ReceiptMissing,
    WorkDelegated,
    WorkCompleted,
    WorkFailed,
    Finalized,
    Stuck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnLedgerRecord {
    pub timestamp: DateTime<Utc>,
    pub stream_turn_id: u64,
    pub kind: TurnLedgerEventKind,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools_invoked: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_tools: Vec<String>,
    pub rounds_executed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch: Option<TurnScratchpad>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TurnLoopAwareness {
    user_responses_sent: usize,
    last_response_preview: Option<String>,
}

impl TurnLoopAwareness {
    pub fn restore(&mut self, user_responses_sent: usize, last_response_preview: Option<String>) {
        self.user_responses_sent = user_responses_sent;
        self.last_response_preview = last_response_preview.map(|preview| {
            truncate_user_response_preview(&preview, USER_RESPONSE_PREVIEW_MAX_CHARS)
        });
    }

    pub fn checkpoint_state(&self) -> (usize, Option<String>) {
        (self.user_responses_sent, self.last_response_preview.clone())
    }

    pub fn record_user_response(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        self.user_responses_sent = self.user_responses_sent.saturating_add(1);
        self.last_response_preview = Some(truncate_user_response_preview(
            trimmed,
            USER_RESPONSE_PREVIEW_MAX_CHARS,
        ));
    }

    pub fn loop_budget_message(&self, tool_rounds_remaining: usize) -> String {
        if tool_rounds_remaining > 2 {
            return String::new();
        }
        format!("Rounds remaining in this turn: {tool_rounds_remaining}.")
    }

    pub fn wrap_control_body(&self, tool_rounds_remaining: usize, body: &str) -> String {
        let budget = self.loop_budget_message(tool_rounds_remaining);
        let trimmed = body.trim();
        if trimmed.is_empty() {
            budget
        } else {
            format!("{budget}\n\n{trimmed}")
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnLoopDiscipline {
    max_text_only_stuck_continues: usize,
    text_only_continues_without_new_tools: usize,
    invocations_at_last_text_continue: usize,
}

impl Default for TurnLoopDiscipline {
    fn default() -> Self {
        Self::with_max_text_only_stuck_continues(MAX_TEXT_ONLY_STUCK_CONTINUES)
    }
}

impl TurnLoopDiscipline {
    pub fn with_max_text_only_stuck_continues(limit: usize) -> Self {
        Self {
            max_text_only_stuck_continues: limit.max(1),
            text_only_continues_without_new_tools: 0,
            invocations_at_last_text_continue: 0,
        }
    }

    pub fn on_tool_round(&mut self) {
        self.text_only_continues_without_new_tools = 0;
    }

    pub fn restore_checkpoint_state(
        &mut self,
        text_only_continues_without_new_tools: usize,
        invocations_at_last_text_continue: usize,
    ) {
        self.text_only_continues_without_new_tools =
            text_only_continues_without_new_tools.min(self.max_text_only_stuck_continues);
        self.invocations_at_last_text_continue = invocations_at_last_text_continue;
    }

    pub fn checkpoint_state(&self) -> (usize, usize) {
        (
            self.text_only_continues_without_new_tools,
            self.invocations_at_last_text_continue,
        )
    }

    /// Returns true when the loop should stop with a user-visible stuck message.
    pub fn on_text_only_continue(&mut self, invocations_len: usize) -> bool {
        if invocations_len == self.invocations_at_last_text_continue {
            self.text_only_continues_without_new_tools =
                self.text_only_continues_without_new_tools.saturating_add(1);
        } else {
            self.text_only_continues_without_new_tools = 1;
            self.invocations_at_last_text_continue = invocations_len;
        }
        self.text_only_continues_without_new_tools >= self.max_text_only_stuck_continues
    }
}

pub fn stuck_turn_user_message(
    text_only_limit: usize,
    max_tool_rounds: usize,
    rounds_executed: usize,
) -> String {
    format!(
        "We hit the turn loop limit: {text_only_limit} consecutive principal-visible replies without \
         new tool receipts (turn budget: {max_tool_rounds} rounds; used {rounds_executed} this turn). \
         What should we do next — run the missing ritual (calibrate, moods), call cognition_turn action=turn.checkpoint \
         for a mid-task handoff, cognition_turn action=turn.finish when fully done, \
         with the complete answer, or extend the budget?"
    )
}

pub fn record_fsm_continue(
    stream_turn_id: u64,
    _reason: ContinueReason,
    detail: &str,
    missing_tools: &[String],
    rounds_executed: usize,
    tools_invoked: &[String],
    scratch: &TurnScratchpad,
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: TurnLedgerEventKind::TextOnlyContinue,
        detail: detail.to_string(),
        tools_invoked: tools_invoked.to_vec(),
        missing_tools: missing_tools.to_vec(),
        rounds_executed,
        scratch: Some(scratch.clone()),
        active_profile_id: None,
    }
}

pub fn record_tool_round(
    stream_turn_id: u64,
    rounds_executed: usize,
    tool_names: &[String],
    scratch: &TurnScratchpad,
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: TurnLedgerEventKind::ToolRound,
        detail: format!("round {rounds_executed}"),
        tools_invoked: tool_names.to_vec(),
        missing_tools: Vec::new(),
        rounds_executed,
        scratch: Some(scratch.clone()),
        active_profile_id: None,
    }
}

pub fn record_finalized(
    stream_turn_id: u64,
    termination_reason: &str,
    rounds_executed: usize,
    tools_invoked: &[String],
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: TurnLedgerEventKind::Finalized,
        detail: termination_reason.to_string(),
        tools_invoked: tools_invoked.to_vec(),
        missing_tools: Vec::new(),
        rounds_executed,
        scratch: None,
        active_profile_id: None,
    }
}

pub fn record_stuck(
    stream_turn_id: u64,
    rounds_executed: usize,
    tools_invoked: &[String],
    text_only_limit: usize,
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: TurnLedgerEventKind::Stuck,
        detail: format!("text_only_continue_without_new_tools>={text_only_limit}"),
        tools_invoked: tools_invoked.to_vec(),
        missing_tools: Vec::new(),
        rounds_executed,
        scratch: None,
        active_profile_id: None,
    }
}

pub fn truncate_user_response_preview(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discipline_is_bounded_and_resets_on_new_tools() {
        let mut discipline = TurnLoopDiscipline::with_max_text_only_stuck_continues(3);
        assert!(!discipline.on_text_only_continue(2));
        assert!(!discipline.on_text_only_continue(2));
        assert!(discipline.on_text_only_continue(2));
        discipline.on_tool_round();
        assert!(!discipline.on_text_only_continue(4));
    }

    #[test]
    fn awareness_round_trips_checkpoint_state() {
        let mut awareness = TurnLoopAwareness::default();
        awareness.record_user_response("  an interim   response  ");
        let state = awareness.checkpoint_state();
        let mut restored = TurnLoopAwareness::default();
        restored.restore(state.0, state.1);
        assert_eq!(restored.checkpoint_state().0, 1);
        assert!(restored.loop_budget_message(2).contains("2"));
    }

    #[test]
    fn hud_contains_only_dynamic_round_state() {
        let policy = append_tool_loop_policy("hello", 12);
        assert!(policy.contains("max_tool_rounds=12"));
        assert!(!policy.contains("typed_terminal="));
        assert!(!policy.contains("unlock"));
        assert!(!policy.contains("[MEDOUSA_SCRATCH_POLICY]"));
    }
}
