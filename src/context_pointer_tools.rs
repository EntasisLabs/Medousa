//! Context pointer follow tool.

use std::sync::Arc;

use medousa_types::environment::POINTER_KIND_SESSION;
use medousa_types::environment::ContextPointerDigest;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::context_pointer_index::resolve_pointer_slice;
use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::session::load_history;
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_CONTEXT_FOLLOW_POINTER: &str = "cognition_context_follow_pointer";
pub const COGNITION_CONTEXT_LIST_POINTERS: &str = "cognition_context_list_pointers";
const COGNITION_CONTEXT_FOLLOW_POINTER_ID: ToolId =
    ToolId::new(COGNITION_CONTEXT_FOLLOW_POINTER);
const COGNITION_CONTEXT_LIST_POINTERS_ID: ToolId =
    ToolId::new(COGNITION_CONTEXT_LIST_POINTERS);

pub fn register_context_pointer_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionContextFollowPointerTool {
        turn_scope: turn_scope.clone(),
    })?;
    registry.register_typed_tool(CognitionContextListPointersTool { turn_scope })?;
    Ok(())
}

struct CognitionContextFollowPointerTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

fn default_pointer_scope() -> String {
    "last_5_turns".to_string()
}

#[derive(Debug, JsonSchema)]
struct ContextFollowPointerInput {
    #[schemars(required, with = "String")]
    pointer_id: Option<String>,
    #[serde(default = "default_pointer_scope")]
    scope: String,
}

impl<'de> Deserialize<'de> for ContextFollowPointerInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            pointer_id: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            scope: Option<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            pointer_id: input.pointer_id,
            scope: input.scope.unwrap_or_else(default_pointer_scope),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct ContextFollowPointerOutput {
    ok: bool,
    pointer_id: String,
    kind: String,
    content: String,
    truncated: bool,
}

#[medousa_tool(id = COGNITION_CONTEXT_FOLLOW_POINTER_ID)]
impl CognitionContextFollowPointerTool {
    /// Pull a focused slice of a context pointer into working memory. Use pointer ids from [MEDOUSA_POINTERS] at turn start. scope examples: last_5_turns.
    async fn invoke_typed(
        &self,
        input: ContextFollowPointerInput,
    ) -> stasis::prelude::Result<ContextFollowPointerOutput> {
        let pointer_id = input
            .pointer_id
            .as_deref()
            .ok_or_else(|| StasisError::PortFailure("pointer_id required".to_string()))?;
        let scope = input.scope.as_str();

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
            &crate::context_pointer_index::collect_work_card_hints(&active_session),
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

        Ok(ContextFollowPointerOutput {
            ok: true,
            pointer_id: pointer_id.to_string(),
            kind: pointer.kind,
            content,
            truncated,
        })
    }
}

struct CognitionContextListPointersTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ContextListPointersInput {}

#[derive(Debug, Serialize, JsonSchema)]
struct ContextListPointersOutput {
    ok: bool,
    #[schemars(with = "serde_json::Value")]
    digest: ContextPointerDigest,
    block: String,
}

#[medousa_tool(id = COGNITION_CONTEXT_LIST_POINTERS_ID)]
impl CognitionContextListPointersTool {
    /// List ranked context pointers for the active session (same as turn bootstrap digest).
    async fn invoke_typed(
        &self,
        _input: ContextListPointersInput,
    ) -> stasis::prelude::Result<ContextListPointersOutput> {
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
            &crate::context_pointer_index::collect_work_card_hints(&active_session),
        );
        let block = crate::context_pointer_index::format_pointer_digest_block(&digest);
        Ok(ContextListPointersOutput {
            ok: true,
            digest,
            block,
        })
    }
}
