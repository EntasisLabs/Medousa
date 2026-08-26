//! Portable state and records for the bounded foreground loop.

use chrono::{DateTime, Utc};
use genai::chat::ChatMessage;
use medousa_engine::TurnScratchpad;
use serde::{Deserialize, Serialize};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::completion_fsm::ContinueReason;
use crate::turn_policy::pack_hold_resolution_control_message;

/// Default when a composition does not provide its own text-only limit.
pub const MAX_TEXT_ONLY_STUCK_CONTINUES: usize = 3;
pub const USER_RESPONSE_PREVIEW_MAX_CHARS: usize = 100;
pub const TURN_CONTROL_PREFIX: &str = "[MEDOUSA_TURN_CONTROL]";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssistantPackHold {
    pub fragments: Vec<String>,
}

/// Legacy export retained for V2 prompt consumers. Production V3 policy lives
/// in the compiled STTP document; this block is dynamic runtime HUD only.
pub const TURN_RUNTIME_BOUNDARY_APPENDIX: &str = r#"[MEDOUSA_HUD]
turn_state=direct|active_work
direct=prose_without_action_delivers_and_ends
active_work=prose_delivers_and_continues_until_typed_terminal
typed_terminal=turn.finish|turn.request_input|turn.checkpoint
terminal_batch=one_terminal_without_ordinary_actions
timeline=responses_and_receipts_persist_in_occurrence_order"#;

pub const TURN_SCRATCH_APPENDIX: &str = r#"[MEDOUSA_SCRATCH_POLICY]
[MEDOUSA_SCRATCH] is your engine sticky notes — persists across tool rounds and client disconnect.
The streamed UI draft may reset between rounds; scratch does not.
Check scratch digests_recent / tools_this_turn / open_gaps before re-calling tools you already ran."#;

/// Merge held assistant fragments with the resolution prose into one body.
pub fn merge_assistant_pack_fragments(fragments: &[String], resolution: &str) -> String {
    let mut parts: Vec<String> = fragments
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    let trimmed_resolution = resolution.trim();
    if !trimmed_resolution.is_empty() {
        parts.push(trimmed_resolution.to_string());
    }
    parts.join("\n\n")
}

pub fn resolve_max_text_only_stuck_continues(max_tool_rounds: usize) -> usize {
    max_tool_rounds.max(1)
}

pub fn push_pack_hold_message(messages: &mut Vec<ChatMessage>) {
    messages.push(ChatMessage::system(pack_hold_resolution_control_message()));
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
         turn_state=active_work_after_first_action\n\
         max_tool_rounds={max_tool_rounds}\n\
         {TURN_RUNTIME_BOUNDARY_APPENDIX}\n\
         {TURN_SCRATCH_APPENDIX}\n\
         Turn start injects [MEDOUSA_TOOL_SLICES], [MEDOUSA_TOOL_HINTS], and matched [MEDOUSA_GRAPHEME_SCRIPTS]. \
         Call cognition_tools_discover(domain=…) to unlock tool groups for this session; drill history with cognition_tool_history_detail(slice_id=turn:N)."
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
    fn assistant_pack_preserves_every_non_empty_fragment() {
        assert_eq!(
            merge_assistant_pack_fragments(
                &["Which repo?".to_string(), "".to_string()],
                "Medousa."
            ),
            "Which repo?\n\nMedousa."
        );
    }

    #[test]
    fn hud_carries_the_structural_completion_contract() {
        let policy = append_tool_loop_policy("hello", 12);
        assert!(policy.contains("max_tool_rounds=12"));
        assert!(policy.contains("active_work=prose_delivers_and_continues"));
        assert!(policy.contains("typed_terminal=turn.finish|turn.request_input|turn.checkpoint"));
        assert!(policy.contains("[MEDOUSA_SCRATCH_POLICY]"));
    }
}
