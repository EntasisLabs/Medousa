//! Deliver typed terminal agent outcomes.

use super::stream_sink::SharedAgentStreamSink;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTurnDeliveryKind {
    Final,
    NeedsInput,
    Checkpoint,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentTurnDeliveryHint<'a> {
    /// Retained for caller compatibility; wording/classifier hints do not own
    /// terminal state.
    pub activation_reason: &'a str,
    pub termination_reason: Option<&'a str>,
}

pub fn classify_agent_turn_delivery(
    _text: &str,
    _tool_names: &[String],
    hint: AgentTurnDeliveryHint<'_>,
) -> AgentTurnDeliveryKind {
    let _ = hint.activation_reason;
    if hint.termination_reason == Some("cognition_turn_request_input") {
        return AgentTurnDeliveryKind::NeedsInput;
    }
    AgentTurnDeliveryKind::Final
}

pub async fn deliver_agent_turn_outcome(
    sink: &SharedAgentStreamSink,
    turn_id: u64,
    text: String,
    tool_names: Vec<String>,
    hint: AgentTurnDeliveryHint<'_>,
) {
    match classify_agent_turn_delivery(&text, &tool_names, hint) {
        AgentTurnDeliveryKind::NeedsInput => {
            sink.agent_needs_input(turn_id, text, tool_names).await;
        }
        AgentTurnDeliveryKind::Checkpoint => {
            sink.agent_turn_checkpoint(turn_id, text, tool_names).await;
        }
        AgentTurnDeliveryKind::Final => {
            sink.agent_response(turn_id, text, tool_names).await;
        }
    }
}

pub async fn deliver_agent_turn_checkpoint(
    sink: &SharedAgentStreamSink,
    turn_id: u64,
    text: String,
    tool_names: Vec<String>,
) {
    sink.agent_turn_checkpoint(turn_id, text, tool_names).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_request_input_routes_to_needs_input() {
        let kind = classify_agent_turn_delivery(
            "Which repo should I search?",
            &[],
            AgentTurnDeliveryHint {
                activation_reason: "classifier_clarify",
                termination_reason: Some("cognition_turn_request_input"),
            },
        );
        assert_eq!(kind, AgentTurnDeliveryKind::NeedsInput);
    }

    #[test]
    fn prose_question_without_typed_outcome_is_final() {
        let kind = classify_agent_turn_delivery(
            "Do you want the backup database or production?",
            &["llm.chat".to_string()],
            AgentTurnDeliveryHint {
                activation_reason: "configured_default",
                termination_reason: None,
            },
        );
        assert_eq!(kind, AgentTurnDeliveryKind::Final);
    }

    #[test]
    fn substantive_answer_stays_final() {
        let kind = classify_agent_turn_delivery(
            "Based on tool output, the daemon binds 127.0.0.1:7419 and serves the dashboard at /dashboard.",
            &["cognition_capability".to_string()],
            AgentTurnDeliveryHint {
                activation_reason: "tool_intent_detected",
                termination_reason: None,
            },
        );
        assert_eq!(kind, AgentTurnDeliveryKind::Final);
    }
}
