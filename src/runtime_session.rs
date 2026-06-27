//! Resolve the active chat session for daemon-hosted tools.
//!
//! The daemon builds one shared agent runtime at startup with a non-chat bootstrap
//! session label. Per-turn chat identity lives in [`TurnContinuationScope`].

use serde_json::Value;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::turn_continuation::TurnContinuationScope;

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

/// Turn scope chat id when present, otherwise the runtime bootstrap fallback.
pub fn resolve_active_chat_session_id(
    turn_scope: Option<&TurnContinuationScope>,
    bootstrap_fallback: &str,
) -> String {
    if let Some(scope) = turn_scope {
        let session_id = scope.session_id.trim();
        if !session_id.is_empty() {
            return session_id.to_string();
        }
    }
    bootstrap_fallback.trim().to_string()
}

pub async fn resolve_active_chat_session_id_async(
    turn_scope: &RwLock<Option<TurnContinuationScope>>,
    bootstrap_fallback: &str,
) -> String {
    let scope = turn_scope.read().await;
    resolve_active_chat_session_id(scope.as_ref(), bootstrap_fallback)
}

pub async fn resolve_active_chat_session_id_from_input(
    input: &Value,
    turn_scope: &RwLock<Option<TurnContinuationScope>>,
    bootstrap_fallback: &str,
) -> String {
    if let Some(explicit) = explicit_chat_session_id_from_input(input) {
        return explicit;
    }
    resolve_active_chat_session_id_async(turn_scope, bootstrap_fallback).await
}

pub async fn require_active_chat_session_id_from_input(
    input: &Value,
    turn_scope: &RwLock<Option<TurnContinuationScope>>,
    tool_label: &str,
) -> StasisResult<String> {
    let session_id = if let Some(explicit) = explicit_chat_session_id_from_input(input) {
        explicit
    } else {
        turn_scope
            .read()
            .await
            .as_ref()
            .map(|scope| scope.session_id.clone())
            .ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "{tool_label}: session_id required when no active turn scope"
                ))
            })?
    };
    reject_bootstrap_chat_session_id(&session_id, tool_label)
}

pub async fn require_active_chat_session_id_async(
    turn_scope: &RwLock<Option<TurnContinuationScope>>,
    bootstrap_fallback: &str,
    tool_label: &str,
) -> StasisResult<String> {
    let session_id = resolve_active_chat_session_id_async(turn_scope, bootstrap_fallback).await;
    reject_bootstrap_chat_session_id(&session_id, tool_label)
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
    use serde_json::json;

    fn sample_scope(session_id: &str) -> TurnContinuationScope {
        TurnContinuationScope {
            turn_correlation_id: "turn-1".to_string(),
            session_id: session_id.to_string(),
            original_prompt: "hi".to_string(),
            delivery_target: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            response_depth_mode: "standard".to_string(),
            supports_ui_artifacts: true,
            supports_browser_host: false,
            channel_surface: None,
        }
    }

    #[test]
    fn bootstrap_session_ids_are_detected() {
        assert!(is_runtime_bootstrap_session_id(RUNTIME_BOOTSTRAP_SESSION_ID));
        assert!(is_runtime_bootstrap_session_id(LEGACY_RUNTIME_BOOTSTRAP_SESSION_ID));
        assert!(!is_runtime_bootstrap_session_id("medousa-home"));
    }

    #[test]
    fn resolve_prefers_turn_scope_over_bootstrap() {
        let scope = sample_scope("medousa-home");
        let resolved = resolve_active_chat_session_id(
            Some(&scope),
            RUNTIME_BOOTSTRAP_SESSION_ID,
        );
        assert_eq!(resolved, "medousa-home");
    }

    #[test]
    fn resolve_falls_back_to_bootstrap_without_turn_scope() {
        let resolved = resolve_active_chat_session_id(None, RUNTIME_BOOTSTRAP_SESSION_ID);
        assert_eq!(resolved, RUNTIME_BOOTSTRAP_SESSION_ID);
    }

    #[tokio::test]
    async fn require_active_chat_session_id_rejects_bootstrap_only_resolution() {
        let turn_scope = RwLock::new(None::<TurnContinuationScope>);
        let err = require_active_chat_session_id_async(
            &turn_scope,
            RUNTIME_BOOTSTRAP_SESSION_ID,
            "test_tool",
        )
        .await
        .expect_err("bootstrap-only resolution should fail");
        assert!(err.to_string().contains("not a chat session"));
    }

    #[tokio::test]
    async fn require_active_chat_session_id_accepts_turn_scope() {
        let turn_scope = RwLock::new(Some(sample_scope("medousa-home")));
        let session_id = require_active_chat_session_id_async(
            &turn_scope,
            RUNTIME_BOOTSTRAP_SESSION_ID,
            "test_tool",
        )
        .await
        .expect("turn scope session");
        assert_eq!(session_id, "medousa-home");
    }

    #[tokio::test]
    async fn memory_resolution_uses_turn_scope_not_bootstrap() {
        let turn_scope = RwLock::new(Some(sample_scope("medousa-home")));
        let session_id = crate::locus_memory::resolve_memory_tool_session_id(
            &json!({}),
            &turn_scope,
            RUNTIME_BOOTSTRAP_SESSION_ID,
            false,
        )
        .await;
        assert_eq!(session_id, "medousa-home");
        assert!(!is_runtime_bootstrap_session_id(&session_id));
    }
}
