//! Control-plane tools for agent turn boundaries (explicit finalize signaling).

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;

/// Canonical registry name (snake_case).
pub const COGNITION_TURN_PREPARE_FINAL: &str = "cognition_turn_prepare_final";

/// Dot alias accepted from models trained on dotted tool names.
pub const COGNITION_TURN_PREPARE_FINAL_DOTTED: &str = "cognition.turn.prepare_final";

pub fn is_prepare_final_tool_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed == COGNITION_TURN_PREPARE_FINAL || trimmed == COGNITION_TURN_PREPARE_FINAL_DOTTED
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
            "Mark that your next message will be the final user-facing reply for this turn. \
             Call once after you have finished tool work (memory, MCP, jobs, etc.). \
             Do not use for short status lines; keep using tools while still working. \
             After calling, send one complete answer on the next turn without further tool calls.",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_prepare_final_names() {
        assert!(is_prepare_final_tool_name("cognition_turn_prepare_final"));
        assert!(is_prepare_final_tool_name("cognition.turn.prepare_final"));
        assert!(!is_prepare_final_tool_name("cognition_memory_store"));
    }
}
