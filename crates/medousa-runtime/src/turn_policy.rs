//! Model-visible completion-policy messages shared by every daemon deployment.

pub const PACK_HOLD_PREFIX: &str = "[MEDOUSA_PACK_HOLD]";

/// Principal content-pack hold — one resolution round before commit or more tools.
pub fn pack_hold_resolution_control_message() -> String {
    format!(
        "{PACK_HOLD_PREFIX}\n\
         consecutive_non_tool_responses=1.\n\
         Next: a tool call continues work and resets the prose count; a non-tool response ends \
         the turn and preserves both responses; cognition_turn action=turn.finish ends immediately and appends \
         its message to the held response. If continuing, call the next tool now instead of \
         narrating intended work. Use cognition_turn action=turn.update_user in a tool round for visible \
         interim status."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_hold_message_describes_the_event_state_machine() {
        let message = pack_hold_resolution_control_message();
        assert!(message.contains("consecutive_non_tool_responses=1"));
        assert!(message.contains("tool call continues work and resets"));
        assert!(message.contains("preserves both responses"));
        assert!(message.contains("cognition_turn action=turn.finish"));
        assert!(message.contains("cognition_turn action=turn.update_user"));
    }
}
