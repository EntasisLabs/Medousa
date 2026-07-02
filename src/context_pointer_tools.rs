//! Context pointer follow tool.

use std::sync::Arc;

use async_trait::async_trait;
use medousa_types::environment::POINTER_KIND_SESSION;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::context_pointer_index::resolve_pointer_slice;
use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::session::load_history;
use crate::turn_continuation::TurnContinuationScope;

pub const COGNITION_CONTEXT_FOLLOW_POINTER: &str = "cognition_context_follow_pointer";
pub const COGNITION_CONTEXT_LIST_POINTERS: &str = "cognition_context_list_pointers";

pub fn register_context_pointer_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_tool(CognitionContextFollowPointerTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_tool(CognitionContextListPointersTool { turn_scope })?;
    Ok(())
}

struct CognitionContextFollowPointerTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[async_trait]
impl StasisTool for CognitionContextFollowPointerTool {
    fn name(&self) -> &'static str {
        COGNITION_CONTEXT_FOLLOW_POINTER
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Pull a focused slice of a context pointer into working memory. \
             Use pointer ids from [MEDOUSA_POINTERS] at turn start. scope examples: last_5_turns.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["pointer_id"],
            "properties": {
                "pointer_id": { "type": "string" },
                "scope": { "type": "string", "default": "last_5_turns" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let pointer_id = input
            .get("pointer_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("pointer_id required".to_string()))?;
        let scope = input
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("last_5_turns");

        let active_session =
            crate::runtime_session::require_active_chat_session_id_async(
                &self.turn_scope,
                crate::runtime_session::runtime_bootstrap_session_id(),
                COGNITION_CONTEXT_FOLLOW_POINTER,
            )
            .await?;

        let sessions = crate::session_catalog::list_sessions(20);
        let env = environment_hub()
            .get(&resolve_profile_id(None))
            .await
            .ok();
        let digest = crate::context_pointer_index::build_pointer_digest(
            &active_session,
            &sessions,
            env.as_ref(),
            &[],
        );
        let pointer = digest
            .pointers
            .iter()
            .find(|p| p.id == pointer_id)
            .cloned()
            .ok_or_else(|| {
                StasisError::PortFailure(format!("pointer not found in digest: {pointer_id}"))
            })?;

        let history = if pointer.kind == POINTER_KIND_SESSION {
            Some(load_history(pointer_id))
        } else {
            None
        };
        let (content, truncated) =
            resolve_pointer_slice(&pointer, scope, history.as_deref());

        Ok(json!({
            "ok": true,
            "pointer_id": pointer_id,
            "kind": pointer.kind,
            "content": content,
            "truncated": truncated,
        }))
    }
}

struct CognitionContextListPointersTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[async_trait]
impl StasisTool for CognitionContextListPointersTool {
    fn name(&self) -> &'static str {
        COGNITION_CONTEXT_LIST_POINTERS
    }

    fn description(&self) -> Option<&'static str> {
        Some("List ranked context pointers for the active session (same as turn bootstrap digest).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let _ = input;
        let active_session =
            crate::runtime_session::require_active_chat_session_id_async(
                &self.turn_scope,
                crate::runtime_session::runtime_bootstrap_session_id(),
                COGNITION_CONTEXT_LIST_POINTERS,
            )
            .await?;
        let sessions = crate::session_catalog::list_sessions(20);
        let env = environment_hub()
            .get(&resolve_profile_id(None))
            .await
            .ok();
        let digest = crate::context_pointer_index::build_pointer_digest(
            &active_session,
            &sessions,
            env.as_ref(),
            &[],
        );
        Ok(json!({
            "ok": true,
            "digest": digest,
            "block": crate::context_pointer_index::format_pointer_digest_block(&digest),
        }))
    }
}
