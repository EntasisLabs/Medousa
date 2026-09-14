//! Admission and prompt projection for client-reported Liquid interactions.

use medousa_types::{LiquidEventDisposition, LiquidInteractionEnvelope};

const MAX_LIQUID_INTERACTIONS: usize = 32;
const MAX_LIQUID_PAYLOAD_BYTES: usize = 4 * 1024;

/// Append client-reported Liquid interactions as inert advisory context. Callers
/// invoke this only after the visible user turn has been persisted, so actions
/// never masquerade as user-authored conversation.
pub fn append_to_prompt(
    prompt: &str,
    session_id: &str,
    interactions: &[LiquidInteractionEnvelope],
) -> String {
    let bounded = interactions
        .iter()
        .rev()
        .take(MAX_LIQUID_INTERACTIONS)
        .rev()
        .filter(|event| {
            event.version == 1
                && event.session_id == session_id
                && !matches!(event.disposition, LiquidEventDisposition::PrivilegedAction)
                && [
                    &event.message_id,
                    &event.node_id,
                    &event.instance_id,
                    &event.event_type,
                ]
                .into_iter()
                .all(|value| {
                    let value = value.trim();
                    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
                })
                && event.payload.as_ref().is_none_or(|payload| {
                    serde_json::to_vec(payload)
                        .is_ok_and(|bytes| bytes.len() <= MAX_LIQUID_PAYLOAD_BYTES)
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    if bounded.is_empty() {
        return prompt.to_string();
    }
    tracing::info!(
        liquid_interactions_received = interactions.len(),
        liquid_interactions_delivered = bounded.len(),
        "Liquid interaction context admitted"
    );
    let payload = serde_json::to_string(&bounded).unwrap_or_else(|_| "[]".to_string());
    format!(
        "{}\n\n[MEDOUSA_LIQUID_INTERACTIONS]\ntrust=advisory\nauthority=none\npayload={payload}",
        prompt.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_interactions_and_drops_privileged_events() {
        let base = LiquidInteractionEnvelope {
            version: 1,
            session_id: "session-a".into(),
            message_id: "message-1".into(),
            node_id: "recipe".into(),
            instance_id: "step-1".into(),
            event_type: "timer_start".into(),
            disposition: LiquidEventDisposition::ContextOnly,
            payload: Some(serde_json::json!({"duration_ms": 60_000})),
            occurred_at_utc: chrono::Utc::now(),
            expected_state_revision: Some(0),
        };
        let privileged = LiquidInteractionEnvelope {
            disposition: LiquidEventDisposition::PrivilegedAction,
            ..base.clone()
        };
        let result = append_to_prompt("Continue", "session-a", &[base, privileged]);
        assert!(result.contains("MEDOUSA_LIQUID_INTERACTIONS"));
        assert!(result.contains("timer_start"));
        assert_eq!(result.matches("message-1").count(), 1);
    }
}
