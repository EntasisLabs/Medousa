//! Control-plane tools for agent turn boundaries (explicit finalize signaling).

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
use stasis::application::orchestration::tool_registry::StasisTool;

/// Canonical registry name (snake_case).
pub const COGNITION_TURN_PREPARE_FINAL: &str = "cognition_turn_prepare_final";

/// Dot alias accepted from models trained on dotted tool names.
pub const COGNITION_TURN_PREPARE_FINAL_DOTTED: &str = "cognition.turn.prepare_final";

/// Hard-stop: deliver final user-facing text in the tool call and end the loop immediately.
pub const COGNITION_TURN_FINISH: &str = "cognition_turn_finish";

pub const COGNITION_TURN_FINISH_DOTTED: &str = "cognition.turn.finish";

/// Hand mid-task update to the principal and end this agent turn (conversation continues on their reply).
pub const COGNITION_TURN_CHECKPOINT: &str = "cognition_turn_checkpoint";

pub const COGNITION_TURN_CHECKPOINT_DOTTED: &str = "cognition.turn.checkpoint";

pub const COGNITION_TURN_REQUEST_MORE_ROUNDS: &str = "cognition_turn_request_more_rounds";

pub const COGNITION_TURN_REQUEST_MORE_ROUNDS_DOTTED: &str = "cognition.turn.request_more_rounds";

/// Signal tool-loop entry with a principal-facing progress message (does not end the turn).
pub const COGNITION_TURN_BEGIN_WORK: &str = "cognition_turn_begin_work";

pub const COGNITION_TURN_BEGIN_WORK_DOTTED: &str = "cognition.turn.begin_work";

/// Short principal-facing status while the turn continues (retries, course-corrections, light updates).
pub const COGNITION_TURN_UPDATE_USER: &str = "cognition_turn_update_user";

pub const COGNITION_TURN_UPDATE_USER_DOTTED: &str = "cognition.turn.update_user";

/// Principal-facing body when the model ends with prose after tools without `cognition_turn_finish`.
pub const PROSE_REQUIRES_FINISH_STUB: &str =
    "I finished the tool work but didn't commit a final answer. Reply to continue and I'll \
     deliver the full summary with cognition_turn_finish.";

pub struct RequestMoreRoundsPayload {
    pub requested_rounds: usize,
    pub reason: String,
    pub progress_summary: Option<String>,
}

pub fn is_prepare_final_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_PREPARE_FINAL
        || trimmed == COGNITION_TURN_PREPARE_FINAL_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed)
            == COGNITION_TURN_PREPARE_FINAL
}

pub fn is_finish_turn_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_FINISH
        || trimmed == COGNITION_TURN_FINISH_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_FINISH
}

pub fn is_checkpoint_turn_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_CHECKPOINT
        || trimmed == COGNITION_TURN_CHECKPOINT_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_CHECKPOINT
}

pub fn is_request_more_rounds_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_REQUEST_MORE_ROUNDS
        || trimmed == COGNITION_TURN_REQUEST_MORE_ROUNDS_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed)
            == COGNITION_TURN_REQUEST_MORE_ROUNDS
}

pub fn is_begin_work_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_BEGIN_WORK
        || trimmed == COGNITION_TURN_BEGIN_WORK_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_BEGIN_WORK
}

pub fn is_update_user_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_UPDATE_USER
        || trimmed == COGNITION_TURN_UPDATE_USER_DOTTED
        || crate::tool_aliases::sanitize_tool_advertised_name(trimmed) == COGNITION_TURN_UPDATE_USER
}

/// Extract the latest successful user-update line from a tool batch.
pub fn update_user_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_update_user_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

/// Latest principal-visible progress line from update_user or begin_work in this batch.
pub fn turn_progress_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    update_user_message_from_invocations(invocations)
        .or_else(|| begin_work_message_from_invocations(invocations))
}

/// Extract the latest successful begin-work progress message from a tool batch.
pub fn begin_work_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_begin_work_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_begin_work_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_begin_work_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

fn note_from_begin_work_payload(payload: &Value) -> Option<String> {
    payload
        .get("note")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Extract the latest successful begin-work sticky note from a tool batch.
pub fn begin_work_note_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_begin_work_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(note) = note_from_begin_work_payload(&inv.tool_input) {
            return Some(note);
        }
        if let Some(note) = note_from_begin_work_payload(&inv.tool_output) {
            return Some(note);
        }
    }
    None
}

fn message_from_begin_work_payload(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Extract the operator-facing final message from a tool batch, if `cognition_turn_finish` ran.
pub fn finish_turn_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_finish_turn_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_finish_turn_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_finish_turn_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

/// Map FSM termination to the principal-visible body (stub when finish was required but missing).
pub fn terminal_text_for_fsm_end(termination_reason: &str, draft_text: String) -> String {
    if termination_reason == "prose_requires_finish" {
        let trimmed = draft_text.trim();
        if trimmed.is_empty()
            || crate::turn_text_heuristics::looks_like_substantive_final_answer(trimmed)
        {
            PROSE_REQUIRES_FINISH_STUB.to_string()
        } else if crate::turn_text_heuristics::looks_like_interim_status(trimmed)
            || trimmed.chars().count()
                <= crate::agent_runtime::turn_completion_fsm::INTERIM_MAX_CHARS
        {
            draft_text
        } else {
            PROSE_REQUIRES_FINISH_STUB.to_string()
        }
    } else {
        draft_text
    }
}

pub fn request_more_rounds_from_invocations(
    invocations: &[ToolInvocation],
) -> Option<RequestMoreRoundsPayload> {
    for inv in invocations.iter().rev() {
        if !is_request_more_rounds_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        let requested_rounds = inv
            .tool_input
            .get("requested_rounds")
            .or_else(|| inv.tool_output.get("requested_rounds"))
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(1)
            .clamp(1, crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK);
        let reason = inv
            .tool_input
            .get("reason")
            .or_else(|| inv.tool_output.get("reason"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)?;
        let progress_summary = inv
            .tool_input
            .get("progress_summary")
            .or_else(|| inv.tool_output.get("progress_summary"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        return Some(RequestMoreRoundsPayload {
            requested_rounds,
            reason,
            progress_summary,
        });
    }
    None
}

fn message_from_finish_turn_payload(payload: &Value) -> Option<String> {
    message_from_turn_control_message_payload(payload)
}

fn message_from_turn_control_message_payload(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Extract the principal-facing checkpoint from a tool batch, if `cognition_turn_checkpoint` ran.
pub fn checkpoint_turn_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    for inv in invocations.iter().rev() {
        if !is_checkpoint_turn_tool_name(&inv.tool_name) {
            continue;
        }
        if inv.tool_output.get("ok") == Some(&Value::Bool(false)) {
            continue;
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_input) {
            return Some(message);
        }
        if let Some(message) = message_from_turn_control_message_payload(&inv.tool_output) {
            return Some(message);
        }
    }
    None
}

/// Signal tool-loop entry with a principal-facing progress line (loop continues).
pub struct CognitionTurnBeginWorkTool;

#[async_trait]
impl StasisTool for CognitionTurnBeginWorkTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_BEGIN_WORK
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Signal that heavy or long-running tool work is starting (multi-step research, worker \
             delegation, large vault crawl). Call before that work — not for quick status, retries, \
             or course-corrections (use cognition_turn_update_user). Optional note= pins intent in scratch.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Short principal-facing line before heavy tool work"
                },
                "note": {
                    "type": "string",
                    "description": "Optional sticky working note in engine scratch (not shown to principal)"
                },
                "intent": {
                    "type": "string",
                    "description": "Optional note for logs (not shown to the principal)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let Some(message) = message_from_begin_work_payload(&input) else {
            return Ok(json!({
                "ok": false,
                "begin_work": false,
                "error": "message is required and must be non-empty",
            }));
        };
        let intent = input
            .get("intent")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let note = note_from_begin_work_payload(&input);

        Ok(json!({
            "ok": true,
            "begin_work": true,
            "message": message,
            "intent": intent,
            "note": note,
        }))
    }
}

/// Short principal-facing status while the turn continues (not a final answer).
pub struct CognitionTurnUpdateUserTool;

#[async_trait]
impl StasisTool for CognitionTurnUpdateUserTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_UPDATE_USER
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Tell the principal what you are doing right now — retries, quick course-corrections, \
             \"pulling schemas\", \"one sec\". Call in the same model round as your next tool. \
             Does not end the turn. Prefer this over naked chat prose (prose without tools fights \
             the turn loop). For heavy/long-running work starting, use cognition_turn_begin_work instead.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Short principal-facing status line"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let Some(message) = message_from_turn_control_message_payload(&input) else {
            return Ok(json!({
                "ok": false,
                "update_user": false,
                "error": "message is required and must be non-empty",
            }));
        };

        Ok(json!({
            "ok": true,
            "update_user": true,
            "message": message,
        }))
    }
}

/// Signal that the **next** assistant message (text-only) should be the user-facing final answer.
pub struct CognitionTurnPrepareFinalTool;

#[async_trait]
impl StasisTool for CognitionTurnPrepareFinalTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_PREPARE_FINAL
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Deprecated — prefer cognition_turn_finish with the complete answer. \
             Workshop workers may still call this; host turns should use cognition_turn_update_user for \
             quick status, cognition_turn_begin_work before heavy work, and cognition_turn_finish to commit.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Optional short note for logs (not shown to the user)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let reason = input
            .get("reason")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        Ok(json!({
            "ok": true,
            "prepare_final": true,
            "deprecated": true,
            "message": "Deprecated — call cognition_turn_finish with the complete principal-facing reply. Workshop lane may still send one final prose round.",
            "reason": reason,
        }))
    }
}

/// End the turn immediately with the final user-facing answer (bypasses gatekeeper continue).
pub struct CognitionTurnFinishTool;

#[async_trait]
impl StasisTool for CognitionTurnFinishTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_FINISH
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Deliver the complete principal-facing final answer now and end this turn immediately. \
             Required after tool work — naked prose without this tool ends the turn with a stub, not your draft. \
             Mid-task updates use cognition_turn_checkpoint.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Complete principal-facing final answer for this turn"
                },
                "reason": {
                    "type": "string",
                    "description": "Optional short note for logs (not shown to the user)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let Some(message) = message_from_finish_turn_payload(&input) else {
            return Ok(json!({
                "ok": false,
                "finish_turn": false,
                "error": "message is required and must be non-empty",
            }));
        };
        let reason = input
            .get("reason")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        Ok(json!({
            "ok": true,
            "finish_turn": true,
            "message": message,
            "reason": reason,
        }))
    }
}

/// Hand a mid-task update to the principal and end this agent turn (await their reply to continue).
pub struct CognitionTurnCheckpointTool;

#[async_trait]
impl StasisTool for CognitionTurnCheckpointTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_CHECKPOINT
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Share a substantive mid-task update with the principal and hand the turn back to them. \
             The conversation is not over — you may continue after they reply. \
             Use when tool work produced real progress but you are not done (not a final answer). \
             Prefer this over streaming long interim prose that the runtime may loop on.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Principal-facing update: what you did, what you found, and what happens next or what you need from them"
                },
                "awaiting": {
                    "type": "string",
                    "description": "Optional: what you need from the principal before more tool work (decision, confirmation, missing detail)"
                },
                "reason": {
                    "type": "string",
                    "description": "Optional short note for logs (not shown to the principal)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let Some(message) = message_from_turn_control_message_payload(&input) else {
            return Ok(json!({
                "ok": false,
                "checkpoint_turn": false,
                "error": "message is required and must be non-empty",
            }));
        };
        let awaiting = input
            .get("awaiting")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let reason = input
            .get("reason")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        Ok(json!({
            "ok": true,
            "checkpoint_turn": true,
            "message": message,
            "awaiting": awaiting,
            "reason": reason,
        }))
    }
}

/// Pause the turn and ask the operator for more tool rounds.
pub struct CognitionTurnRequestMoreRoundsTool;

#[async_trait]
impl StasisTool for CognitionTurnRequestMoreRoundsTool {
    fn name(&self) -> &'static str {
        COGNITION_TURN_REQUEST_MORE_ROUNDS
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Request additional tool rounds when the current budget is too tight. \
             Pauses until the principal approves or denies. Include reason and progress summary.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["requested_rounds", "reason"],
            "properties": {
                "requested_rounds": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK,
                    "description": "How many additional model/tool rounds you need"
                },
                "reason": {
                    "type": "string",
                    "description": "Why the current budget is insufficient"
                },
                "progress_summary": {
                    "type": "string",
                    "description": "What is done and what remains"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let requested_rounds = input
            .get("requested_rounds")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(0)
            .clamp(1, crate::turn_budget_request::MAX_REQUESTED_ROUNDS_PER_ASK);
        let reason = input
            .get("reason")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(reason) = reason else {
            return Ok(json!({
                "ok": false,
                "budget_request": false,
                "error": "reason is required",
            }));
        };
        let progress_summary = input
            .get("progress_summary")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        Ok(json!({
            "ok": true,
            "budget_request": true,
            "requested_rounds": requested_rounds,
            "reason": reason,
            "progress_summary": progress_summary,
            "message": "Turn paused — awaiting principal approval for additional tool rounds.",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_interim_text_on_prose_requires_finish() {
        let draft = "Now I see what went wrong — let me grab the schemas.".to_string();
        let out = terminal_text_for_fsm_end("prose_requires_finish", draft.clone());
        assert_eq!(out, draft);
    }

    #[test]
    fn substantive_after_tools_uses_finish_stub() {
        let draft = "Focused preset pulled and applied: stability is now 0.95, friction dropped \
                       to 0.12, and autonomy holds at 0.80. I stored the calibration summary.";
        let out = terminal_text_for_fsm_end("prose_requires_finish", draft.to_string());
        assert_eq!(out, PROSE_REQUIRES_FINISH_STUB);
    }

    #[test]
    fn recognizes_update_user_names() {
        assert!(is_update_user_tool_name("cognition_turn_update_user"));
        assert!(is_update_user_tool_name("cognition.turn.update_user"));
        assert!(!is_update_user_tool_name("cognition_turn_begin_work"));
    }

    #[test]
    fn update_user_from_invocations_reads_latest_successful_call() {
        let invocations = vec![ToolInvocation {
            tool_name: COGNITION_TURN_UPDATE_USER.to_string(),
            tool_input: json!({ "message": "Retrying propose with custom surface." }),
            tool_output: json!({ "ok": true, "update_user": true }),
        }];
        assert_eq!(
            update_user_message_from_invocations(&invocations).as_deref(),
            Some("Retrying propose with custom surface.")
        );
    }

    #[test]
    fn turn_progress_prefers_update_user_over_begin_work() {
        let invocations = vec![
            ToolInvocation {
                tool_name: COGNITION_TURN_BEGIN_WORK.to_string(),
                tool_input: json!({ "message": "Starting research worker." }),
                tool_output: json!({ "ok": true, "begin_work": true }),
            },
            ToolInvocation {
                tool_name: COGNITION_TURN_UPDATE_USER.to_string(),
                tool_input: json!({ "message": "Grabbing wiki schemas." }),
                tool_output: json!({ "ok": true, "update_user": true }),
            },
        ];
        assert_eq!(
            turn_progress_message_from_invocations(&invocations).as_deref(),
            Some("Grabbing wiki schemas.")
        );
    }

    #[test]
    fn recognizes_begin_work_names() {
        assert!(is_begin_work_tool_name("cognition_turn_begin_work"));
        assert!(is_begin_work_tool_name("cognition.turn.begin_work"));
        assert!(!is_begin_work_tool_name("cognition_turn_finish"));
    }

    #[test]
    fn begin_work_from_invocations_reads_latest_successful_call() {
        let invocations = vec![
            ToolInvocation {
                tool_name: COGNITION_TURN_BEGIN_WORK.to_string(),
                tool_input: json!({ "message": "Checking memory nodes." }),
                tool_output: json!({ "ok": true, "begin_work": true }),
            },
            ToolInvocation {
                tool_name: "cognition_memory_list".to_string(),
                tool_input: Value::Null,
                tool_output: Value::Null,
            },
        ];
        assert_eq!(
            begin_work_message_from_invocations(&invocations).as_deref(),
            Some("Checking memory nodes.")
        );
    }

    #[test]
    fn recognizes_prepare_final_names() {
        assert!(is_prepare_final_tool_name("cognition_turn_prepare_final"));
        assert!(is_prepare_final_tool_name("cognition.turn.prepare_final"));
        assert!(!is_prepare_final_tool_name("cognition_memory_store"));
    }

    #[test]
    fn recognizes_finish_turn_names() {
        assert!(is_finish_turn_tool_name("cognition_turn_finish"));
        assert!(is_finish_turn_tool_name("cognition.turn.finish"));
        assert!(!is_finish_turn_tool_name("cognition_turn_prepare_final"));
    }

    #[test]
    fn finish_turn_from_invocations_reads_latest_successful_call() {
        let invocations = vec![
            ToolInvocation {
                tool_name: "cognition_memory_recall".to_string(),
                tool_input: json!({}),
                tool_output: json!({"ok": true}),
            },
            ToolInvocation {
                tool_name: COGNITION_TURN_FINISH.to_string(),
                tool_input: json!({"message": "Here is the complete answer."}),
                tool_output: json!({"ok": true, "finish_turn": true, "message": "Here is the complete answer."}),
            },
        ];
        assert_eq!(
            finish_turn_from_invocations(&invocations).as_deref(),
            Some("Here is the complete answer.")
        );
    }

    #[test]
    fn finish_turn_from_invocations_skips_failed_tool_output() {
        let invocations = vec![ToolInvocation {
            tool_name: COGNITION_TURN_FINISH.to_string(),
            tool_input: json!({"message": ""}),
            tool_output: json!({"ok": false, "error": "message is required and must be non-empty"}),
        }];
        assert!(finish_turn_from_invocations(&invocations).is_none());
    }

    #[tokio::test]
    async fn finish_turn_tool_requires_message() {
        let tool = CognitionTurnFinishTool;
        let out = tool.invoke(json!({})).await.expect("invoke");
        assert_eq!(out["ok"], false);
    }

    #[tokio::test]
    async fn finish_turn_tool_returns_message() {
        let tool = CognitionTurnFinishTool;
        let out = tool
            .invoke(json!({"message": "Done.", "reason": "task complete"}))
            .await
            .expect("invoke");
        assert_eq!(out["ok"], true);
        assert_eq!(out["finish_turn"], true);
        assert_eq!(out["message"], "Done.");
        assert_eq!(out["reason"], "task complete");
    }

    #[test]
    fn recognizes_checkpoint_turn_names() {
        assert!(is_checkpoint_turn_tool_name("cognition_turn_checkpoint"));
        assert!(is_checkpoint_turn_tool_name("cognition.turn.checkpoint"));
        assert!(!is_checkpoint_turn_tool_name("cognition_turn_finish"));
    }

    #[test]
    fn checkpoint_turn_from_invocations_reads_latest_successful_call() {
        let invocations = vec![ToolInvocation {
            tool_name: COGNITION_TURN_CHECKPOINT.to_string(),
            tool_input: json!({"message": "Found three blockers — need your pick on scope."}),
            tool_output: json!({"ok": true, "checkpoint_turn": true}),
        }];
        assert_eq!(
            checkpoint_turn_from_invocations(&invocations).as_deref(),
            Some("Found three blockers — need your pick on scope.")
        );
    }

    #[tokio::test]
    async fn checkpoint_turn_tool_requires_message() {
        let tool = CognitionTurnCheckpointTool;
        let out = tool.invoke(json!({})).await.expect("invoke");
        assert_eq!(out["ok"], false);
    }
}
