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

use super::turn_completion::TurnCompletionVerdict;

pub const TURN_CONTROL_PREFIX: &str = "[MEDOUSA_TURN_CONTROL]";

/// Max consecutive text-only loop rounds without new tool invocations before we stop.
pub const MAX_TEXT_ONLY_STUCK_CONTINUES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnLedgerEventKind {
    ToolRound,
    TextOnlyContinue,
    GatekeeperContinue,
    ReceiptMissing,
    WorkDelegated,
    WorkCompleted,
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
}

#[derive(Debug, Clone, Default)]
pub struct TurnLoopDiscipline {
    text_only_continues_without_new_tools: usize,
    invocations_at_last_text_continue: usize,
}

impl TurnLoopDiscipline {
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
        self.text_only_continues_without_new_tools >= MAX_TEXT_ONLY_STUCK_CONTINUES
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
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("turn_ledger")
        .join(format!("{safe}.jsonl"))
}

pub fn append_turn_ledger_record(session_id: &str, record: &TurnLedgerRecord) {
    let path = turn_ledger_path(session_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(line) = serde_json::to_string(record) else {
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

pub fn developer_message_for_gatekeeper_continue(verdict: &TurnCompletionVerdict) -> String {
    if !verdict.missing_tools.is_empty() {
        return format!(
            "Turn not complete. Required tool receipt(s) missing: {}. \
             Do not repeat prior status tables or summaries. Call the missing tool(s) next, \
             then cognition_turn_prepare_final before your final user-facing answer.",
            verdict.missing_tools.join(", ")
        );
    }

    match verdict.source {
        "receipt_checklist" if verdict.reason.contains("prepare_final") => {
            "prepare_final was signaled but your draft still looks in-progress. \
             Finish remaining tool work or deliver a true final answer — not another status preamble."
                .to_string()
        }
        "receipt_checklist" => verdict.reason.clone(),
        "gatekeeper_model" => format!(
            "Completion gatekeeper: continue this turn. {}",
            verdict.reason
        ),
        _ => format!(
            "Continue this turn with tool calls as needed. {}",
            verdict.reason
        ),
    }
}

pub fn developer_message_for_heuristic_interim_continue() -> &'static str {
    "Your last message was status-only or in-progress narration, not a final answer. \
     Call the tools needed to complete the user request. Do not resend the same summary or AVEC table."
}

pub fn stuck_turn_user_message(rounds_without_new_tools: usize) -> String {
    format!(
        "I hit a loop limit ({rounds_without_new_tools} text-only replies without running new tools). \
         The turn stopped so we don't burn the tool-round budget. \
         Say which step you want next (e.g. run calibrate, pull moods, or give a shorter final answer)."
    )
}

pub fn record_from_gatekeeper_continue(
    stream_turn_id: u64,
    verdict: &TurnCompletionVerdict,
    rounds_executed: usize,
    tools_invoked: &[String],
) -> TurnLedgerRecord {
    let kind = if !verdict.missing_tools.is_empty() {
        TurnLedgerEventKind::ReceiptMissing
    } else {
        TurnLedgerEventKind::GatekeeperContinue
    };
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind,
        detail: verdict.reason.clone(),
        tools_invoked: tools_invoked.to_vec(),
        missing_tools: verdict.missing_tools.clone(),
        rounds_executed,
    }
}

pub fn record_tool_round(
    stream_turn_id: u64,
    rounds_executed: usize,
    tool_names: &[String],
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: TurnLedgerEventKind::ToolRound,
        detail: format!("round {rounds_executed}"),
        tools_invoked: tool_names.to_vec(),
        missing_tools: Vec::new(),
        rounds_executed,
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
    }
}

pub fn record_stuck(
    stream_turn_id: u64,
    rounds_executed: usize,
    tools_invoked: &[String],
) -> TurnLedgerRecord {
    TurnLedgerRecord {
        timestamp: Utc::now(),
        stream_turn_id,
        kind: TurnLedgerEventKind::Stuck,
        detail: format!(
            "text_only_continue_without_new_tools>={MAX_TEXT_ONLY_STUCK_CONTINUES}"
        ),
        tools_invoked: tools_invoked.to_vec(),
        missing_tools: Vec::new(),
        rounds_executed,
    }
}

pub fn persist_ledger_record(session_id: Option<&str>, record: &TurnLedgerRecord) {
    if let Some(session_id) = session_id.filter(|id| !id.trim().is_empty()) {
        append_turn_ledger_record(session_id, record);
    }
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
    use crate::agent_runtime::turn_completion::TurnCompletionDecision;

    #[test]
    fn discipline_trips_after_three_stuck_continues() {
        let mut d = TurnLoopDiscipline::default();
        assert!(!d.on_text_only_continue(2));
        assert!(!d.on_text_only_continue(2));
        assert!(d.on_text_only_continue(2));
    }

    #[test]
    fn discipline_resets_after_tool_round() {
        let mut d = TurnLoopDiscipline::default();
        assert!(!d.on_text_only_continue(1));
        assert!(!d.on_text_only_continue(1));
        d.on_tool_round();
        assert!(!d.on_text_only_continue(1));
    }

    #[test]
    fn discipline_new_tools_reset_counter() {
        let mut d = TurnLoopDiscipline::default();
        assert!(!d.on_text_only_continue(1));
        assert!(!d.on_text_only_continue(1));
        assert!(!d.on_text_only_continue(3));
    }

    #[test]
    fn gatekeeper_continue_message_mentions_missing() {
        let msg = developer_message_for_gatekeeper_continue(&TurnCompletionVerdict {
            decision: TurnCompletionDecision::Continue,
            confidence: 1.0,
            reason: "ritual incomplete".to_string(),
            source: "receipt_checklist",
            missing_tools: vec!["cognition_memory_calibrate".to_string()],
        });
        assert!(msg.contains("cognition_memory_calibrate"));
        assert!(msg.contains("Do not repeat"));
    }
}
