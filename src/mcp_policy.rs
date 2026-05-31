//! Daemon-side MCP invoke policy evaluation (Phase A stub).

use crate::mcp_gateway_api::{
    McpEffectClass, McpPolicyDecision, McpPolicyEvaluateRequest, McpPolicyEvaluateResponse,
    McpTurnLane,
};

pub fn evaluate_mcp_policy(request: &McpPolicyEvaluateRequest) -> McpPolicyEvaluateResponse {
    match request.turn_context.lane {
        McpTurnLane::Interactive => evaluate_interactive(request),
        McpTurnLane::Scheduled => evaluate_scheduled(request),
        McpTurnLane::Heartbeat => evaluate_heartbeat(request),
    }
}

fn evaluate_interactive(request: &McpPolicyEvaluateRequest) -> McpPolicyEvaluateResponse {
    match request.effect_class {
        McpEffectClass::ExternalRead => allow("interactive lane permits external_read"),
        McpEffectClass::ExternalWrite => allow("interactive lane permits external_write"),
        McpEffectClass::ExternalSideEffect => approval_required(
            "interactive lane requires approval for external_side_effect",
        ),
    }
}

fn evaluate_scheduled(request: &McpPolicyEvaluateRequest) -> McpPolicyEvaluateResponse {
    match request.effect_class {
        McpEffectClass::ExternalRead => allow("scheduled lane permits external_read"),
        McpEffectClass::ExternalWrite | McpEffectClass::ExternalSideEffect => deny(
            "scheduled lane denies external_write and external_side_effect in phase A",
        ),
    }
}

fn evaluate_heartbeat(request: &McpPolicyEvaluateRequest) -> McpPolicyEvaluateResponse {
    match request.effect_class {
        McpEffectClass::ExternalRead => allow("heartbeat lane permits read-only MCP"),
        McpEffectClass::ExternalWrite | McpEffectClass::ExternalSideEffect => deny(
            "heartbeat lane denies mutating MCP actions",
        ),
    }
}

fn allow(reason: impl Into<String>) -> McpPolicyEvaluateResponse {
    McpPolicyEvaluateResponse {
        allowed: true,
        decision: McpPolicyDecision::Allow,
        reason: reason.into(),
        approval_required: false,
    }
}

fn deny(reason: impl Into<String>) -> McpPolicyEvaluateResponse {
    McpPolicyEvaluateResponse {
        allowed: false,
        decision: McpPolicyDecision::Deny,
        reason: reason.into(),
        approval_required: false,
    }
}

fn approval_required(reason: impl Into<String>) -> McpPolicyEvaluateResponse {
    McpPolicyEvaluateResponse {
        allowed: false,
        decision: McpPolicyDecision::ApprovalRequired,
        reason: reason.into(),
        approval_required: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_gateway_api::McpTurnContext;

    fn sample_request(lane: McpTurnLane, effect: McpEffectClass) -> McpPolicyEvaluateRequest {
        McpPolicyEvaluateRequest {
            action: "mcp.invoke".to_string(),
            server_id: "notion".to_string(),
            tool_name: "search_pages".to_string(),
            effect_class: effect,
            turn_context: McpTurnContext {
                turn_id: "turn_1".to_string(),
                session_id: "sess_1".to_string(),
                user_id: "user_1".to_string(),
                channel_id: "channel_1".to_string(),
                lane,
                policy_profile: None,
            },
        }
    }

    #[test]
    fn interactive_read_allowed() {
        let response = evaluate_mcp_policy(&sample_request(
            McpTurnLane::Interactive,
            McpEffectClass::ExternalRead,
        ));
        assert!(response.allowed);
        assert_eq!(response.decision, McpPolicyDecision::Allow);
    }

    #[test]
    fn scheduled_write_denied() {
        let response = evaluate_mcp_policy(&sample_request(
            McpTurnLane::Scheduled,
            McpEffectClass::ExternalWrite,
        ));
        assert!(!response.allowed);
        assert_eq!(response.decision, McpPolicyDecision::Deny);
    }

    #[test]
    fn interactive_side_effect_requires_approval() {
        let response = evaluate_mcp_policy(&sample_request(
            McpTurnLane::Interactive,
            McpEffectClass::ExternalSideEffect,
        ));
        assert!(!response.allowed);
        assert_eq!(response.decision, McpPolicyDecision::ApprovalRequired);
    }
}
