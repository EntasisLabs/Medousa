//! Phase 0 turn ledger: structured in-turn events + model-visible control messages.
//!
//! Persists per-session JSONL for debugging and Phase 1 worker bus; injects
//! `[MEDOUSA_TURN_CONTROL]` system lines into the tool-loop transcript.

use std::io::Write;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use genai::chat::ChatMessage;
use serde::{Deserialize, Serialize};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use super::turn_context::TurnScratchpad;
use crate::agent_runtime::turn_completion_fsm::ContinueReason;

pub const TURN_CONTROL_PREFIX: &str = "[MEDOUSA_TURN_CONTROL]";

/// Injected on host/worker tool turns and echoed in STTP — strict runtime boundary for prose vs tools.
pub const TURN_RUNTIME_BOUNDARY_APPENDIX: &str = r#"[MEDOUSA_TURN_RUNTIME]
Runtime boundary (strict — enforced by the daemon, not negotiable):
- Any assistant message with NO tool calls and non-empty prose ENDS this turn immediately. The runtime will not loop for more work in the same turn.
- Still need tools? Call them in the same model round or a later round — never stream plan-only prose ("I'll check…", "next I'll spawn workers…", "let me calibrate…").
- Progress or status for the principal: cognition_turn_begin_work (a tool call) — not naked interim chat prose.
- After tool work is done: cognition_turn_finish with the complete answer (required — runtime rejects naked prose as final). Plain prose without tools ends the turn but does not commit as the principal-facing answer after tools have run.
- Mid-task handoff (principal replies to continue): cognition_turn_checkpoint — not cognition_turn_finish.
- Delegate execution: cognition_spawn_turn_worker in a tool round with a complete task prompt — announcing delegation in prose does not run workers.
- Host console auto-unlocks memory + vault each session (calibrate, vault write, …) — call tools directly; use cognition_tools_discover only for catalog/runtime/history/identity/skill/overlay.
- Between tool rounds, streamed progress is archived automatically; use begin_work when you want a visible status line before heavy tools."#;

/// Default when no host gate passes a configured tool-round budget (tests, bare loops).
pub const MAX_TEXT_ONLY_STUCK_CONTINUES: usize = 3;

/// Interim text-only continue limit follows the operator's tool-round budget.
pub fn resolve_max_text_only_stuck_continues(max_tool_rounds: usize) -> usize {
    max_tool_rounds.max(1)
}

/// `[MEDOUSA_TOOL_POLICY]` block appended to interactive prompts that run the tool loop.
pub fn append_tool_loop_policy(prompt: &str, max_tool_rounds: usize) -> String {
    let max_tool_rounds = max_tool_rounds.max(1);
    format!(
        "{prompt}\n\n[MEDOUSA_TOOL_POLICY]\n\
         mode=tool_loop\n\
         max_tool_rounds={max_tool_rounds}\n\
         {TURN_RUNTIME_BOUNDARY_APPENDIX}\n\
         Turn start injects [MEDOUSA_TOOL_SLICES], [MEDOUSA_TOOL_HINTS], and matched [MEDOUSA_GRAPHEME_SCRIPTS]. \
         Call cognition_tools_discover(domain=…) to unlock tool groups for this session; drill history with cognition_tool_history_detail(slice_id=turn:N)."
    )
}

/// Preview length for the last user-visible assistant reply injected into the tool loop.
pub const USER_RESPONSE_PREVIEW_MAX_CHARS: usize = 100;

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
    /// Workshop profile active when the event was recorded (`user:{slug}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile_id: Option<String>,
}

/// Tracks tool-round budget and user-visible interim replies without bloating the transcript.
#[derive(Debug, Clone, Default)]
pub struct TurnLoopAwareness {
    user_responses_sent: usize,
    last_response_preview: Option<String>,
}

impl TurnLoopAwareness {
    pub fn record_user_response(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        self.user_responses_sent = self.user_responses_sent.saturating_add(1);
        self.last_response_preview =
            Some(truncate_user_response_preview(trimmed, USER_RESPONSE_PREVIEW_MAX_CHARS));
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

pub fn turn_ledger_path(session_id: &str) -> PathBuf {
    let safe = session_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let safe = if safe.is_empty() { "default".to_string() } else { safe };
    crate::paths::medousa_data_dir()
        .join("turn_ledger")
        .join(format!("{safe}.jsonl"))
}

pub fn append_turn_ledger_record(session_id: &str, record: &TurnLedgerRecord) {
    let mut record = record.clone();
    if record.active_profile_id.is_none() {
        record.active_profile_id =
            Some(crate::user_profiles::resolve_workshop_active_profile_id());
    }
    let path = turn_ledger_path(session_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(line) = serde_json::to_string(&record) else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

pub fn push_turn_control_message(messages: &mut Vec<ChatMessage>, body: &str) {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    messages.push(ChatMessage::system(format!(
        "{TURN_CONTROL_PREFIX}\n{trimmed}"
    )));
}

pub fn stuck_turn_user_message(
    text_only_limit: usize,
    max_tool_rounds: usize,
    rounds_executed: usize,
) -> String {
    format!(
        "We hit the turn loop limit: {text_only_limit} consecutive principal-visible replies without \
         new tool receipts (turn budget: {max_tool_rounds} rounds; used {rounds_executed} this turn). \
         What should we do next — run the missing ritual (calibrate, moods), call cognition_turn_checkpoint \
         for a mid-task handoff, cognition_turn_finish when fully done, \
         with the complete answer, or extend the budget?"
    )
}

fn ledger_kind_for_continue(_reason: ContinueReason) -> TurnLedgerEventKind {
    TurnLedgerEventKind::TextOnlyContinue
}

pub fn record_fsm_continue(
    stream_turn_id: u64,
    reason: ContinueReason,
    detail: &str,
    missing_tools: &[String],
    rounds_executed: usize,
    tools_invoked: &[String],
    scratch: &TurnScratchpad,
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: ledger_kind_for_continue(reason),
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
        detail: format!(
            "text_only_continue_without_new_tools>={text_only_limit}"
        ),
        tools_invoked: tools_invoked.to_vec(),
        missing_tools: Vec::new(),
        rounds_executed,
        scratch: None,
        active_profile_id: None,
    }
}

pub fn persist_ledger_record(session_id: Option<&str>, record: &TurnLedgerRecord) {
    if let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) {
        append_turn_ledger_record(session_id, record);
    }
}

/// First `max_chars` of assistant text, collapsed to one line for turn-control hints.
pub fn truncate_user_response_preview(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out = String::new();
    for ch in collapsed.chars().take(max_chars) {
        out.push(ch);
    }
    out.push('…');
    out
}

pub fn ledger_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    invocations
        .iter()
        .map(|inv| inv.tool_name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_ledger_record_stamps_active_profile_id() {
        let record = record_tool_round(1, 1, &["cognition_memory_recall".to_string()], &TurnScratchpad::default());
        assert!(record.active_profile_id.is_none());
        let session = "test-ledger-profile-stamp";
        append_turn_ledger_record(session, &record);
        let path = turn_ledger_path(session);
        let raw = std::fs::read_to_string(&path).expect("ledger file");
        let parsed: TurnLedgerRecord = serde_json::from_str(raw.lines().next().unwrap()).expect("json");
        assert!(parsed.active_profile_id.is_some());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn discipline_trips_after_three_stuck_continues() {
        let mut d = TurnLoopDiscipline::with_max_text_only_stuck_continues(3);
        assert!(!d.on_text_only_continue(2));
        assert!(!d.on_text_only_continue(2));
        assert!(d.on_text_only_continue(2));
    }

    #[test]
    fn discipline_resets_after_tool_round() {
        let mut d = TurnLoopDiscipline::with_max_text_only_stuck_continues(3);
        assert!(!d.on_text_only_continue(1));
        assert!(!d.on_text_only_continue(1));
        d.on_tool_round();
        assert!(!d.on_text_only_continue(1));
    }

    #[test]
    fn discipline_new_tools_reset_counter() {
        let mut d = TurnLoopDiscipline::with_max_text_only_stuck_continues(3);
        assert!(!d.on_text_only_continue(1));
        assert!(!d.on_text_only_continue(1));
        assert!(!d.on_text_only_continue(3));
    }

    #[test]
    fn truncate_preview_collapses_whitespace_and_caps_length() {
        let long = "word ".repeat(40);
        let preview = truncate_user_response_preview(&long, 20);
        assert!(preview.chars().count() <= 21);
        assert!(preview.ends_with('…'));
        assert!(!preview.contains('\n'));
    }

    #[test]
    fn resolve_stuck_limit_matches_tool_round_budget() {
        assert_eq!(resolve_max_text_only_stuck_continues(10), 10);
        assert_eq!(resolve_max_text_only_stuck_continues(0), 1);
    }

    #[test]
    fn tool_loop_policy_includes_configured_rounds() {
        let p = append_tool_loop_policy("hello", 12);
        assert!(p.contains("max_tool_rounds=12"));
        assert!(p.contains("[MEDOUSA_TOOL_SLICES]"));
        assert!(p.contains("cognition_tool_history_detail"));
        assert!(p.contains("cognition_tools_discover"));
        assert!(p.contains("[MEDOUSA_TOOL_HINTS]"));
        assert!(p.contains("cognition_spawn_turn_worker"));
        assert!(p.contains("[MEDOUSA_TURN_RUNTIME]"));
        assert!(p.contains("ENDS this turn immediately"));
    }

    #[test]
    fn awareness_message_quiet_until_low_rounds() {
        let a = TurnLoopAwareness::default();
        assert!(a.loop_budget_message(5).is_empty());
        let msg = a.loop_budget_message(2);
        assert!(msg.contains("Rounds remaining in this turn: 2"));
    }

    #[test]
    fn awareness_message_includes_budget_when_low() {
        let a = TurnLoopAwareness::default();
        let msg = a.loop_budget_message(1);
        assert!(msg.contains("Rounds remaining in this turn: 1"));
    }

    #[test]
    fn stuck_user_message_uses_configured_limits() {
        let msg = stuck_turn_user_message(10, 10, 7);
        assert!(msg.contains("10"));
        assert!(msg.contains("turn budget: 10"));
        assert!(msg.contains("used 7 this turn"));
    }
}
