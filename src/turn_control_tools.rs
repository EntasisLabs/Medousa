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

fn message_from_finish_turn_payload(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
            "Signal that your next text-only message is intended as the user-facing final for this turn. \
             Optional after tool receipts are in context. \
             The following text-only message is treated as final when substantive.",
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
            "message": "Send your complete user-facing reply on the next turn without calling other tools.",
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
            "Deliver your complete user-facing final answer now and end this turn immediately. \
             Use when tool work is done but the loop would otherwise keep going. \
             Prefer this over prepare_final when you already have the full reply ready.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["message"],
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Complete user-facing final answer for this turn"
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
