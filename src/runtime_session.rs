//! Resolve the active chat session for daemon-hosted tools.
//!
//! The daemon builds one shared agent runtime at startup with a non-chat bootstrap
//! session label. Per-turn chat identity lives in [`TurnContinuationScope`].

use crate::turn_continuation::TurnContinuationScope;
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, StasisError};

/// Assembly-time label for the singleton daemon agent runtime — not a chat session.
pub const RUNTIME_BOOTSTRAP_SESSION_ID: &str = "__runtime_bootstrap__";

/// Legacy bootstrap label retained for reserved-slug checks and migration guards.
pub const LEGACY_RUNTIME_BOOTSTRAP_SESSION_ID: &str = "daemon-agent-runtime";

pub fn runtime_bootstrap_session_id() -> &'static str {
    RUNTIME_BOOTSTRAP_SESSION_ID
}

pub fn is_runtime_bootstrap_session_id(session_id: &str) -> bool {
    let trimmed = session_id.trim();
    trimmed == RUNTIME_BOOTSTRAP_SESSION_ID || trimmed == LEGACY_RUNTIME_BOOTSTRAP_SESSION_ID
}

pub fn explicit_chat_session_id_from_input(input: &Value) -> Option<String> {
    input
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_id_from_scope(
    turn_scope: Option<&TurnContinuationScope>,
    tool_label: &str,
) -> StasisResult<String> {
    let session_id = turn_scope
        .map(|scope| scope.session_id.trim())
        .filter(|session_id| !session_id.is_empty())
        .ok_or_else(|| {
            StasisError::PortFailure(format!(
                "{tool_label}: active turn execution context required"
            ))
        })?;
    reject_bootstrap_chat_session_id(session_id, tool_label)
}

pub async fn resolve_active_chat_session_id_async(
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    _bootstrap_fallback: &str,
) -> StasisResult<String> {
    let scope = crate::agent_runtime::execution_context::turn_continuation_scope(turn_scope).await;
    session_id_from_scope(scope.as_ref(), "tool")
}

pub async fn resolve_active_chat_session_id_from_input(
    input: &Value,
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    bootstrap_fallback: &str,
) -> StasisResult<String> {
    if let Some(explicit) = explicit_chat_session_id_from_input(input) {
        return reject_bootstrap_chat_session_id(&explicit, "tool");
    }
    resolve_active_chat_session_id_async(turn_scope, bootstrap_fallback).await
}

pub async fn require_active_chat_session_id_from_input(
    input: &Value,
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    tool_label: &str,
) -> StasisResult<String> {
    let explicit = explicit_chat_session_id_from_input(input);
    require_active_chat_session_id(explicit.as_deref(), turn_scope, tool_label).await
}

/// Resolve an optional typed session id against the active turn scope.
pub async fn require_active_chat_session_id(
    explicit_session_id: Option<&str>,
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    tool_label: &str,
) -> StasisResult<String> {
    let session_id = if let Some(explicit) = explicit_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        explicit.to_string()
    } else {
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(turn_scope).await;
        session_id_from_scope(scope.as_ref(), tool_label)?
    };
    reject_bootstrap_chat_session_id(&session_id, tool_label)
}

pub async fn require_active_chat_session_id_async(
    turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    bootstrap_fallback: &str,
    tool_label: &str,
) -> StasisResult<String> {
    let _ = bootstrap_fallback;
    let scope = crate::agent_runtime::execution_context::turn_continuation_scope(turn_scope).await;
    session_id_from_scope(scope.as_ref(), tool_label)
}

fn reject_bootstrap_chat_session_id(session_id: &str, tool_label: &str) -> StasisResult<String> {
    if session_id.trim().is_empty() || is_runtime_bootstrap_session_id(session_id) {
        return Err(StasisError::PortFailure(format!(
            "{tool_label}: no active chat session (bootstrap runtime label is not a chat session)"
        )));
    }
    Ok(session_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_scope(session_id: &str) -> TurnContinuationScope {
        TurnContinuationScope {
            turn_correlation_id: "turn-1".to_string(),
            session_id: session_id.to_string(),
            identity_user_id: None,
            original_prompt: "hi".to_string(),
            delivery_target: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: true,
            supports_liquid_markdown: true,
            supports_browser_host: false,
            channel_surface: None,
        }
    }

    #[test]
    fn bootstrap_session_ids_are_detected() {
        assert!(is_runtime_bootstrap_session_id(
            RUNTIME_BOOTSTRAP_SESSION_ID
        ));
        assert!(is_runtime_bootstrap_session_id(
            LEGACY_RUNTIME_BOOTSTRAP_SESSION_ID
        ));
        assert!(!is_runtime_bootstrap_session_id("medousa-home"));
    }

    #[test]
    fn scoped_resolution_uses_the_execution_session() {
        let scope = sample_scope("medousa-home");
        let resolved = session_id_from_scope(Some(&scope), "test_tool").unwrap();
        assert_eq!(resolved, "medousa-home");
    }

    #[test]
    fn scoped_resolution_never_falls_back_to_bootstrap() {
        let error = session_id_from_scope(None, "test_tool").unwrap_err();
        assert!(error.to_string().contains("execution context required"));
    }
}
