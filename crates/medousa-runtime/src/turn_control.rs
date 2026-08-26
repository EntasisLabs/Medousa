//! Portable interpretation of Medousa turn-control tool receipts.

use serde_json::Value;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

pub const COGNITION_TURN: &str = "cognition_turn";
pub const COGNITION_WORKSHOP_MUTATE: &str = "cognition_workshop_mutate";
pub const MAX_REQUESTED_ROUNDS_PER_ASK: usize = 8;
pub const ABSOLUTE_MAX_TOOL_ROUNDS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestMoreRoundsPayload {
    pub requested_rounds: usize,
    pub reason: String,
    pub progress_summary: Option<String>,
}

fn turn_action<'a>(tool_name: &str, input: &'a Value) -> Option<&'a str> {
    if tool_name.trim() != COGNITION_TURN {
        return None;
    }
    input
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn is_turn_control_call(tool_name: &str) -> bool {
    tool_name.trim() == COGNITION_TURN
}

pub fn is_prepare_final_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.prepare_final")
}

pub fn is_finish_turn_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.finish")
}

pub fn is_checkpoint_turn_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.checkpoint")
}

pub fn is_request_input_turn_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.request_input")
}

pub fn is_terminal_turn_tool_name(tool_name: &str, input: &Value) -> bool {
    is_finish_turn_tool_name(tool_name, input)
        || is_checkpoint_turn_tool_name(tool_name, input)
        || is_request_input_turn_tool_name(tool_name, input)
}

pub fn is_request_more_rounds_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.request_more_rounds")
}

pub fn is_begin_work_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.begin_work")
}

pub fn is_update_user_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.update_user")
}

pub fn is_propose_mode_tool_name(tool_name: &str, input: &Value) -> bool {
    turn_action(tool_name, input) == Some("turn.propose_mode")
}

pub fn update_user_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    invocations.iter().rev().find_map(|invocation| {
        if !is_update_user_tool_name(&invocation.tool_name, &invocation.tool_input)
            || invocation.tool_output.get("ok") == Some(&Value::Bool(false))
        {
            return None;
        }
        message_from_payload(&invocation.tool_input)
            .or_else(|| message_from_payload(&invocation.tool_output))
    })
}

pub fn turn_progress_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    update_user_message_from_invocations(invocations)
        .or_else(|| begin_work_message_from_invocations(invocations))
}

pub fn begin_work_message_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    invocations.iter().rev().find_map(|invocation| {
        if !is_begin_work_tool_name(&invocation.tool_name, &invocation.tool_input)
            || invocation.tool_output.get("ok") == Some(&Value::Bool(false))
        {
            return None;
        }
        message_from_payload(&invocation.tool_input)
            .or_else(|| message_from_payload(&invocation.tool_output))
    })
}

pub fn begin_work_note_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    invocations.iter().rev().find_map(|invocation| {
        if !is_begin_work_tool_name(&invocation.tool_name, &invocation.tool_input)
            || invocation.tool_output.get("ok") == Some(&Value::Bool(false))
        {
            return None;
        }
        note_from_payload(&invocation.tool_input)
            .or_else(|| note_from_payload(&invocation.tool_output))
    })
}

pub fn finish_turn_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    invocations.iter().rev().find_map(|invocation| {
        if !is_finish_turn_tool_name(&invocation.tool_name, &invocation.tool_input)
            || invocation.tool_output.get("ok") == Some(&Value::Bool(false))
        {
            return None;
        }
        Some(
            message_from_payload(&invocation.tool_input)
                .or_else(|| message_from_payload(&invocation.tool_output))
                .unwrap_or_default(),
        )
    })
}

pub fn checkpoint_turn_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    terminal_message_from_invocations(invocations, is_checkpoint_turn_tool_name)
}

pub fn request_input_from_invocations(invocations: &[ToolInvocation]) -> Option<String> {
    terminal_message_from_invocations(invocations, is_request_input_turn_tool_name)
}

fn terminal_message_from_invocations(
    invocations: &[ToolInvocation],
    matches_action: fn(&str, &Value) -> bool,
) -> Option<String> {
    invocations.iter().rev().find_map(|invocation| {
        if !matches_action(&invocation.tool_name, &invocation.tool_input)
            || invocation.tool_output.get("ok") == Some(&Value::Bool(false))
        {
            return None;
        }
        message_from_payload(&invocation.tool_input)
            .or_else(|| message_from_payload(&invocation.tool_output))
    })
}

/// Completion is structural; terminal prose is never semantically rewritten.
pub fn terminal_text_for_fsm_end(_termination_reason: &str, draft_text: String) -> String {
    draft_text
}

pub fn request_more_rounds_from_invocations(
    invocations: &[ToolInvocation],
) -> Option<RequestMoreRoundsPayload> {
    for invocation in invocations.iter().rev() {
        if !is_request_more_rounds_tool_name(&invocation.tool_name, &invocation.tool_input)
            || invocation.tool_output.get("ok") == Some(&Value::Bool(false))
        {
            continue;
        }
        let requested_rounds = invocation
            .tool_input
            .get("requested_rounds")
            .or_else(|| invocation.tool_output.get("requested_rounds"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(1)
            .clamp(1, MAX_REQUESTED_ROUNDS_PER_ASK);
        let reason = invocation
            .tool_input
            .get("reason")
            .or_else(|| invocation.tool_output.get("reason"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?
            .to_string();
        let progress_summary = invocation
            .tool_input
            .get("progress_summary")
            .or_else(|| invocation.tool_output.get("progress_summary"))
            .and_then(Value::as_str)
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

pub fn workshop_entered_from_invocations(
    invocations: &[ToolInvocation],
) -> Option<(String, String)> {
    invocations.iter().rev().find_map(|invocation| {
        if !is_begin_work_tool_name(&invocation.tool_name, &invocation.tool_input)
            || !invocation
                .tool_output
                .get("workshop_entered")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return None;
        }
        let work_id = invocation.tool_output.get("work_id")?.as_str()?.to_string();
        let ack = invocation
            .tool_output
            .get("user_ack")
            .and_then(Value::as_str)
            .or_else(|| {
                invocation
                    .tool_output
                    .get("message")
                    .and_then(Value::as_str)
            })
            .unwrap_or("Working on that in the workshop.")
            .to_string();
        Some((work_id, ack))
    })
}

pub fn is_workshop_spawn_call(tool_name: &str, input: &Value) -> bool {
    tool_name.trim() == COGNITION_WORKSHOP_MUTATE
        && input.get("action").and_then(Value::as_str) == Some("workshop.spawn")
}

pub fn worker_spawn_from_invocations(invocations: &[ToolInvocation]) -> Option<(String, String)> {
    invocations.iter().rev().find_map(|invocation| {
        if !is_workshop_spawn_call(&invocation.tool_name, &invocation.tool_input)
            || !invocation
                .tool_output
                .get("worker_spawned")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return None;
        }
        let work_id = invocation.tool_output.get("work_id")?.as_str()?.to_string();
        let ack = invocation
            .tool_output
            .get("user_ack")
            .and_then(Value::as_str)
            .or_else(|| {
                invocation
                    .tool_output
                    .get("message")
                    .and_then(Value::as_str)
            })
            .unwrap_or("Working on that in the background.")
            .to_string();
        Some((work_id, ack))
    })
}

fn message_from_payload(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn note_from_payload(payload: &Value) -> Option<String> {
    payload
        .get("note")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn invocation(action: &str, input: Value, output: Value) -> ToolInvocation {
        let mut tool_input = input;
        tool_input["action"] = Value::String(action.to_string());
        ToolInvocation {
            tool_name: COGNITION_TURN.to_string(),
            tool_input,
            tool_output: output,
        }
    }

    #[test]
    fn finish_and_checkpoint_are_structural_receipts() {
        let finish = invocation(
            "turn.finish",
            json!({ "message": "done" }),
            json!({ "ok": true }),
        );
        assert_eq!(
            finish_turn_from_invocations(&[finish]).as_deref(),
            Some("done")
        );

        let silent_finish = invocation("turn.finish", json!({}), json!({ "ok": true }));
        assert_eq!(
            finish_turn_from_invocations(&[silent_finish]),
            Some(String::new())
        );

        let checkpoint = invocation(
            "turn.checkpoint",
            json!({}),
            json!({ "ok": true, "message": "need input" }),
        );
        assert_eq!(
            checkpoint_turn_from_invocations(&[checkpoint]).as_deref(),
            Some("need input")
        );
    }

    #[test]
    fn request_input_is_a_distinct_typed_terminal() {
        let request = invocation(
            "turn.request_input",
            json!({ "message": "Which repository?" }),
            json!({ "ok": true }),
        );
        assert_eq!(
            request_input_from_invocations(&[request]).as_deref(),
            Some("Which repository?")
        );
    }

    #[test]
    fn round_requests_are_bounded_at_the_protocol_edge() {
        let request = invocation(
            "turn.request_more_rounds",
            json!({ "requested_rounds": 99, "reason": "more evidence" }),
            json!({ "ok": true }),
        );
        let payload = request_more_rounds_from_invocations(&[request]).unwrap();
        assert_eq!(payload.requested_rounds, MAX_REQUESTED_ROUNDS_PER_ASK);
    }

    #[test]
    fn malformed_latest_round_request_does_not_revive_an_older_request() {
        let older = invocation(
            "turn.request_more_rounds",
            json!({ "requested_rounds": 2, "reason": "older evidence" }),
            json!({ "ok": true }),
        );
        let latest = invocation(
            "turn.request_more_rounds",
            json!({ "requested_rounds": 4 }),
            json!({ "ok": true }),
        );
        assert!(request_more_rounds_from_invocations(&[older, latest]).is_none());
    }
}
