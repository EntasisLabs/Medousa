//! Daemon-side MCP invoke policy evaluation.

use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::ports::outbound::memory::identity_memory_models::{
    AutonomyScope, GetIdentityContextRequest,
};

use crate::identity_memory::resolve_identity_persona_id;
use crate::mcp_gateway_api::{
    McpEffectClass, McpPolicyDecision, McpPolicyEvaluateRequest, McpPolicyEvaluateResponse,
    McpTurnLane,
};

pub fn evaluate_mcp_policy(request: &McpPolicyEvaluateRequest) -> McpPolicyEvaluateResponse {
    if operator_approval_granted(request) {
        return allow("operator approval granted for MCP invoke");
    }
    evaluate_lane_policy(request)
}

pub async fn evaluate_mcp_policy_with_identity(
    request: &McpPolicyEvaluateRequest,
    identity_service: &IdentityMemoryService,
) -> McpPolicyEvaluateResponse {
    if operator_approval_granted(request) {
        return allow("operator approval granted for MCP invoke");
    }
    let action = effect_to_action_class(request.effect_class);
    let context = match identity_service
        .get_identity_context(&GetIdentityContextRequest {
            user_id: request.turn_context.user_id.clone(),
            persona_id: resolve_identity_persona_id(),
            channel_id: request.turn_context.channel_id.clone(),
            relationship_limit: 16,
        })
        .await
    {
        Ok(context) => context,
        Err(error) => {
            return McpPolicyEvaluateResponse {
                allowed: false,
                decision: McpPolicyDecision::Deny,
                reason: format!("identity context unavailable: {error}"),
                approval_required: false,
            };
        }
    };

    let merged = merge_autonomy_scopes(
        context
            .relationships
            .iter()
            .map(|relationship| &relationship.autonomy_scope),
    );

    if merged.deny.iter().any(|entry| entry == action) {
        return deny(format!("identity deny list blocks '{action}'"));
    }

    if merged
        .approval_required
        .iter()
        .any(|entry| entry == action)
    {
        return approval_required(format!(
            "identity approval_required includes '{action}'"
        ));
    }

    if merged.allow.is_empty() || merged.allow.iter().any(|entry| entry == action) {
        return allow(format!("identity allow permits '{action}'"));
    }

    evaluate_lane_policy(request)
}

fn operator_approval_granted(request: &McpPolicyEvaluateRequest) -> bool {
    request
        .operator_approval_granted
        .unwrap_or(false)
        && request.turn_context.lane == McpTurnLane::Interactive
}

fn evaluate_lane_policy(request: &McpPolicyEvaluateRequest) -> McpPolicyEvaluateResponse {
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
            "scheduled lane denies external_write and external_side_effect",
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

fn merge_autonomy_scopes<'a>(scopes: impl Iterator<Item = &'a AutonomyScope>) -> AutonomyScope {
    let mut merged = AutonomyScope {
        allow: Vec::new(),
        deny: Vec::new(),
        approval_required: Vec::new(),
    };

    for scope in scopes {
        merged.allow.extend(scope.allow.clone());
        merged.deny.extend(scope.deny.clone());
        merged
            .approval_required
            .extend(scope.approval_required.clone());
    }

    merged.allow.sort();
    merged.allow.dedup();
    merged.deny.sort();
    merged.deny.dedup();
    merged.approval_required.sort();
    merged.approval_required.dedup();
    merged
}

pub fn effect_to_action_class(effect: McpEffectClass) -> &'static str {
    match effect {
        McpEffectClass::ExternalRead => "external_read",
        McpEffectClass::ExternalWrite => "external_write",
        McpEffectClass::ExternalSideEffect => "external_side_effect",
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
    use crate::identity_memory::build_seeded_identity_memory_store;
    use crate::mcp_gateway_api::McpTurnContext;
    use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;

    fn sample_request(lane: McpTurnLane, effect: McpEffectClass) -> McpPolicyEvaluateRequest {
        McpPolicyEvaluateRequest {
            action: "mcp.invoke".to_string(),
            server_id: "notion".to_string(),
            tool_name: "search_pages".to_string(),
            effect_class: effect,
            turn_context: McpTurnContext {
                turn_id: "turn_1".to_string(),
                session_id: "sess_1".to_string(),
                user_id: resolve_identity_user_id_for_test(),
                channel_id: resolve_identity_channel_id_for_test(),
                lane,
                policy_profile: Some("interactive".to_string()),
            },
            operator_approval_granted: None,
        }
    }

    fn resolve_identity_user_id_for_test() -> String {
        crate::identity_memory::resolve_identity_user_id(None)
    }

    fn resolve_identity_channel_id_for_test() -> String {
        crate::identity_memory::resolve_identity_channel_id(Some("interactive"))
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

    #[test]
    fn operator_approval_granted_bypasses_side_effect_gate() {
        let mut request = sample_request(
            McpTurnLane::Interactive,
            McpEffectClass::ExternalSideEffect,
        );
        request.operator_approval_granted = Some(true);
        let response = evaluate_mcp_policy(&request);
        assert!(response.allowed);
        assert_eq!(response.decision, McpPolicyDecision::Allow);
    }

    #[test]
    fn operator_approval_granted_does_not_bypass_scheduled_write_deny() {
        let mut request = sample_request(
            McpTurnLane::Scheduled,
            McpEffectClass::ExternalWrite,
        );
        request.operator_approval_granted = Some(true);
        let response = evaluate_mcp_policy(&request);
        assert!(!response.allowed);
        assert_eq!(response.decision, McpPolicyDecision::Deny);
    }

    #[tokio::test]
    async fn identity_allows_external_read_for_default_user() {
        let store = build_seeded_identity_memory_store().expect("store");
        let service = IdentityMemoryService::new(store);
        let response = evaluate_mcp_policy_with_identity(
            &sample_request(McpTurnLane::Interactive, McpEffectClass::ExternalRead),
            &service,
        )
        .await;
        assert!(response.allowed, "{}", response.reason);
    }
}
