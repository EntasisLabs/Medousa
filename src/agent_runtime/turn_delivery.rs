//! Classify and deliver terminal agent turn text (final answer vs needs operator input).

use super::stream_sink::SharedAgentStreamSink;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTurnDeliveryKind {
    Final,
    NeedsInput,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentTurnDeliveryHint<'a> {
    pub activation_reason: &'a str,
}

pub fn classify_agent_turn_delivery(
    text: &str,
    tool_names: &[String],
    hint: AgentTurnDeliveryHint<'_>,
) -> AgentTurnDeliveryKind {
    if hint.activation_reason == "classifier_clarify"
        || hint.activation_reason.contains("clarify")
    {
        return AgentTurnDeliveryKind::NeedsInput;
    }

    if crate::turn_text_heuristics::looks_like_clarifying_question(text)
        && !tool_names.iter().any(|name| is_heavy_tool_name(name))
    {
        return AgentTurnDeliveryKind::NeedsInput;
    }

    AgentTurnDeliveryKind::Final
}

fn is_heavy_tool_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("cognition_grapheme")
        || lower.starts_with("cognition_mcp")
        || lower.starts_with("cognition_capability_invoke")
        || lower.starts_with("cognition_spawn_turn_worker")
        || lower.starts_with("cognition_runtime_workflow")
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
        AgentTurnDeliveryKind::Final => {
            sink.agent_response(turn_id, text, tool_names).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_clarify_routes_to_needs_input() {
        let kind = classify_agent_turn_delivery(
            "Which repo should I search?",
            &[],
            AgentTurnDeliveryHint {
                activation_reason: "classifier_clarify",
            },
        );
        assert_eq!(kind, AgentTurnDeliveryKind::NeedsInput);
    }

    #[test]
    fn short_question_without_heavy_tools_is_needs_input() {
        let kind = classify_agent_turn_delivery(
            "Do you want the backup database or production?",
            &["llm.chat".to_string()],
            AgentTurnDeliveryHint {
                activation_reason: "configured_default",
            },
        );
        assert_eq!(kind, AgentTurnDeliveryKind::NeedsInput);
    }

    #[test]
    fn substantive_answer_stays_final() {
        let kind = classify_agent_turn_delivery(
            "Based on tool output, the daemon binds 127.0.0.1:7419 and serves the dashboard at /dashboard.",
            &["cognition_grapheme_run".to_string()],
            AgentTurnDeliveryHint {
                activation_reason: "tool_intent_detected",
            },
        );
        assert_eq!(kind, AgentTurnDeliveryKind::Final);
    }
}
