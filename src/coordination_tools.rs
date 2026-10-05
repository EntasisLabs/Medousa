//! Sender-owned handoffs and proposals; native admission supplies execution authority.
use crate::daemon::coordination::{PeerProposalIntent, local_coordination_host};
use crate::typed_tools::{ToolId, medousa_tool};
use stasis::prelude::{Result, StasisError};

pub const COGNITION_PEER_DISCOVER: &str = "cognition_peer_discover";
pub const COGNITION_PEER_PROPOSE: &str = "cognition_peer_propose";
const PEER_DISCOVER_ID: ToolId = ToolId::new(COGNITION_PEER_DISCOVER);
const PEER_PROPOSE_ID: ToolId = ToolId::new(COGNITION_PEER_PROPOSE);

pub fn register_coordination_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> Result<()> {
    registry.register_typed_tool(PeerDiscoverTool)?;
    registry.register_typed_tool(PeerProposeTool)?;
    registry.register_typed_tool(PeerHandoffTool)?;
    registry.register_typed_tool(PeerReviewTool)?;
    Ok(())
}

fn error(error: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(error.to_string())
}
fn admitted()
-> Result<std::sync::Arc<crate::agent_runtime::execution_context::TurnExecutionContext>> {
    crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| error("peer coordination requires an admitted owner turn"))
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct PeerDiscoverInput {}
struct PeerDiscoverTool;
struct PeerProposeTool;

#[medousa_tool(id = PEER_DISCOVER_ID)]
impl PeerDiscoverTool {
    /// Discover local Medousa Coder/Codex/Cursor/Hermes peers and saved coding runtime preferences, exact adoptable agent sessions, and this chat's bound project/context range. Unavailable explicit targets must not be substituted; omitted runtime follows the saved fallback order. Use peer_handoff for work authorized by the current human request, or peer_propose for recommendations and adoption.
    async fn invoke_typed(&self, _input: PeerDiscoverInput) -> Result<serde_json::Value> {
        let turn = admitted()?;
        let host = local_coordination_host()
            .ok_or_else(|| error("peer coordination is not available on this workshop"))?;
        host.discover_for_turn(
            turn.principal(),
            medousa_types::SessionId::parse(turn.session_id().as_str()).map_err(error)?,
        )
        .await
        .map_err(error)
    }
}
#[medousa_tool(id = PEER_PROPOSE_ID)]
impl PeerProposeTool {
    /// Prepare an immutable coder assignment for an owned local Forge project. Pass forge_work_id from active-work discovery or work.create_project; when the user requests new work in an existing Git folder without an undertaking, use cognition_runtime_mutate action=work.create_project first; the source chat need not be bound to it. Omit runtime to follow saved preferences and available fallbacks, defaulting to Medousa Coder. Set runtime only when the user explicitly requests it. Never ask the user to move chats just to assign work. Discover first and select an explicit committed context range. To adopt existing work, copy only an exact existing_agent_session_id returned by discovery; omission starts new work. Reuse request_key for exact retries only. This requests human review; it does NOT approve, launch, adopt, or complete work. Tell the user to review the approval card and explicitly start approved work. A conversational yes is not execution authority.
    async fn invoke_typed(&self, input: PeerProposalIntent) -> Result<serde_json::Value> {
        let turn = admitted()?;
        let host = local_coordination_host()
            .ok_or_else(|| error("peer coordination is not available on this workshop"))?;
        let proposal = host
            .propose_for_turn(
                turn.principal(),
                medousa_types::SessionId::parse(turn.session_id().as_str()).map_err(error)?,
                input,
            )
            .await
            .map_err(error)?;
        host.proposal_tool_result(turn.principal(), proposal)
            .await
            .map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn discovery_requires_an_admitted_owner_turn() {
        assert!(
            PeerDiscoverTool
                .invoke_typed(PeerDiscoverInput {})
                .await
                .unwrap_err()
                .to_string()
                .contains("admitted owner turn")
        );
    }
    #[tokio::test]
    async fn proposal_requires_an_admitted_owner_turn() {
        let intent = serde_json::from_value(serde_json::json!({
            "request_key":"review", "runtime":"cursor", "instructions":"Review changes",
            "after_entry_seq":0, "through_entry_seq":3, "continue_owner":true
        }))
        .unwrap();
        assert!(
            PeerProposeTool
                .invoke_typed(intent)
                .await
                .unwrap_err()
                .to_string()
                .contains("admitted owner turn")
        );
    }
}

const PEER_HANDOFF_ID: ToolId = ToolId::new("cognition_peer_handoff");
const PEER_REVIEW_ID: ToolId = ToolId::new("cognition_peer_review");
struct PeerHandoffTool;
struct PeerReviewTool;
#[medousa_tool(id = PEER_HANDOFF_ID)]
impl PeerHandoffTool {
    /// Delegate user-requested work in an owned Forge project through native admission without a second approval card. Use admission=propose for recommendations needing a user decision. The current human request must be included in the committed context. Sender chooses retained/transferred responsibility, sender review/worker result, callbacks and user contact. Defaults retain responsibility, require sender review and wake on acceptance/terminal. A worker result cannot waive sender review. Ordinary delegation follows saved coding runtime preferences.
    async fn invoke_typed(
        &self,
        input: medousa_types::work_handoff::PeerHandoffIntent,
    ) -> Result<serde_json::Value> {
        let turn = admitted()?;
        local_coordination_host()
            .ok_or_else(|| error("peer coordination unavailable"))?
            .handoff_for_turn(&turn, input)
            .await
            .map_err(error)
    }
}
#[medousa_tool(id = PEER_REVIEW_ID)]
impl PeerReviewTool {
    /// Record the sender's acceptance or requested changes for an exact completed handoff. Only the originating sender may decide; executor completion is not sender acceptance. This does not launch another agent or mark a whole project satisfied.
    async fn invoke_typed(
        &self,
        input: medousa_types::work_handoff::PeerHandoffReviewInput,
    ) -> Result<serde_json::Value> {
        let turn = admitted()?;
        local_coordination_host()
            .ok_or_else(|| error("peer coordination unavailable"))?
            .review_handoff_for_turn(&turn, input)
            .await
            .map_err(error)
    }
}
