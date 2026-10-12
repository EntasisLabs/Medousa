//! Assistant-only, location-neutral external-peer proposal tool.

use std::sync::Arc;

use medousa_types::coordination::ExternalPeerRuntime;
use stasis::prelude::{Result, StasisError};

use crate::delegation::DelegationService;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_PEER_DELEGATE: &str = "cognition_peer_delegate";
const PEER_DELEGATE_ID: ToolId = ToolId::new(COGNITION_PEER_DELEGATE);

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct PeerDelegateInput {
    /// Exact agent-selectable execution_runtime_id from active-work discovery.
    execution_runtime_id: String,
    /// Exact forge_work_id from that same workshop inventory row.
    forge_work_id: String,
    runtime: ExternalPeerRuntime,
    instructions: String,
    /// Stable retry key for this exact assignment intent.
    request_key: String,
    /// Ask for a separately approved result-only continuation in this chat.
    continue_owner: bool,
    /// Exact adoptable ACP session id from inventory; omission starts new work.
    existing_agent_session_id: Option<String>,
    /// Optional exact committed range in this chat. Omit for a byte-bounded recent tail plus available Locus continuity. Explicit ranges are never silently shortened.
    context: Option<crate::delegated_task::DelegatedContextSelection>,
}

struct PeerDelegateTool {
    service: Arc<DelegationService>,
}

#[medousa_tool(id = PEER_DELEGATE_ID)]
impl PeerDelegateTool {
    /// Delegate an immutable Medousa Coder/Codex/Cursor/Hermes assignment to the exact remote workshop that owns a discovered Forge work item. Omit context to export a recent tail bounded by entries and serialized bytes, with available source-scoped Locus continuity. Put the complete task, constraints and acceptance criteria in instructions. To select older relevant history, supply context.after_entry_seq (exclusive) and context.through_entry_seq (inclusive) from this chat's committed history. A trusted owner-level Assistant policy launches it immediately; narrower policies return a human approval proposal. Use only exact ids returned by cognition_active_work_discover. Unless the user explicitly names a coding runtime, choose the first ready runtime in that workshop inventory's coding_runtime_preferences order; default to Medousa Coder when preferences are absent. Reuse request_key only for an exact retry.
    async fn invoke_typed(&self, input: PeerDelegateInput) -> Result<serde_json::Value> {
        let response = self
            .service
            .propose_remote_peer(
                &input.execution_runtime_id,
                &input.forge_work_id,
                &input.request_key,
                input.runtime,
                &input.instructions,
                input.continue_owner,
                input.existing_agent_session_id,
                input.context,
            )
            .await
            .map_err(|error| StasisError::PortFailure(error.to_string()))?;
        let started = response.binding.is_some();
        Ok(serde_json::json!({
            "status": if started { "started" } else { "pending_approval" },
            "proposal": response.proposal,
            "binding": response.binding,
            "started": started,
            "message": if started {
                "Remote peer assignment was accepted and started by the trusted Assistant workshop."
            } else {
                "Remote peer assignment prepared. The user must approve and start it from the proposal card."
            },
        }))
    }
}

pub fn register_remote_peer_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    service: Arc<DelegationService>,
) -> Result<()> {
    registry.register_typed_tool(PeerDelegateTool { service })?;
    Ok(())
}
