//! Foundation contracts for logical coordination channels, not delivery transports.
//!
//! These are not HTTP endpoints. Visibility and execution admission remain
//! separate authorities; an adapter must never infer a grant from membership.

use serde::{Deserialize, Serialize};
pub mod context;

use crate::{AuthorityId, ContextManifest, SessionRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CoordinationChannelRef {
    pub authority_id: AuthorityId,
    pub channel_id: String,
}

/// Initial immutable channel snapshot; future membership edits require revision fencing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CoordinationChannelRecord {
    pub channel: CoordinationChannelRef,
    pub owner_principal_id: String,
    pub member_principal_ids: Vec<String>,
    pub attached_sessions: Vec<SessionRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ExternalPeerRuntime {
    Codex,
    Cursor,
    Hermes,
}

/// An exact adapter on an exact execution workshop, not a model preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalPeerTarget {
    pub authority_id: AuthorityId,
    pub execution_runtime_id: String,
    pub runtime: ExternalPeerRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum PeerAvailability {
    Ready,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalPeerCandidate {
    pub target: ExternalPeerTarget,
    pub availability: PeerAvailability,
}

/// Caller intent only. Grant checks occur outside the model before dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalPeerAssignmentRequest {
    pub assignment_id: String,
    pub idempotency_key: String,
    pub owner_principal_id: String,
    pub owner_session: SessionRef,
    pub channel: CoordinationChannelRef,
    pub target: ExternalPeerTarget,
    pub context: ContextManifest,
    /// Separate executor session: never reuse the owner's interactive session.
    pub execution_session: SessionRef,
    pub instructions: String,
    pub execution_grant_id: String,
    /// Initial local bridge is restricted to a governed Forge work item.
    pub forge_work_id: String,
}

/// Destination-owned approval for one exact assignment, never an ambient tool grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalPeerAssignmentGrant {
    pub request: ExternalPeerAssignmentRequest,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Stable correlation returned by an admitted external execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalPeerAssignmentBinding {
    pub assignment_id: String,
    pub owner_principal_id: String,
    pub channel: CoordinationChannelRef,
    pub target: ExternalPeerTarget,
    pub execution_session: SessionRef,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum PeerAssignmentOutcome {
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

/// Receipt identity is used for owner-inbox deduplication, not text matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalPeerAssignmentReceipt {
    pub receipt_id: String,
    pub binding: ExternalPeerAssignmentBinding,
    pub outcome: PeerAssignmentOutcome,
    pub result: String,
}

/// Separate operator approval for one result-only owner turn. This does not
/// authorize follow-up tool calls or transfer the peer's execution authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerOwnerContinuationGrant {
    pub request: ExternalPeerAssignmentRequest,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerOwnerIntakeAttempt {
    pub receipt: ExternalPeerAssignmentReceipt,
    pub attempt: u32,
    pub turn_id: String,
}

/// Acknowledges a durable owner decision, not merely acceptance of a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerOwnerIntakeAcknowledgment {
    pub intake: PeerOwnerIntakeAttempt,
    pub decision: crate::TranscriptEntryRef,
    pub decision_digest: String,
}

/// Immutable snapshot offered to the owner for explicit approval. No execution
/// authority exists merely because a proposal was submitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerAssignmentProposal {
    pub proposal_id: String,
    pub request: ExternalPeerAssignmentRequest,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub continue_owner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalDecision {
    pub proposal_id: String,
    pub owner_principal_id: String,
    pub approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalReviewRecord {
    pub proposal: PeerAssignmentProposal,
    pub decision: Option<PeerProposalDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalInboxResponse {
    pub proposals: Vec<PeerProposalReviewRecord>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalActionResponse {
    pub proposal_id: String,
    pub binding: Option<ExternalPeerAssignmentBinding>,
}

/// Decisions take no mutable assignment fields or caller-supplied owner id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalActionRequest {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_actions_reject_mutable_fields_and_owner_spoofing() {
        assert!(serde_json::from_str::<PeerProposalActionRequest>("{}").is_ok());
        for body in [
            r#"{"instructions":"replacement"}"#,
            r#"{"owner_principal_id":"other"}"#,
            r#"{"continue_owner":true}"#,
        ] {
            assert!(serde_json::from_str::<PeerProposalActionRequest>(body).is_err());
        }
    }

    #[test]
    fn unavailable_runtime_preserves_reason_on_wire() {
        let state = PeerAvailability::Unavailable {
            reason: "runtime is not authenticated".into(),
        };
        let wire = serde_json::to_value(&state).unwrap();
        assert_eq!(wire["state"], "unavailable");
        assert_eq!(
            serde_json::from_value::<PeerAvailability>(wire).unwrap(),
            state
        );
    }

    #[test]
    fn cloud_cursor_is_not_misrepresented_as_local_cursor() {
        assert!(serde_json::from_str::<ExternalPeerRuntime>("\"cursor_cloud\"").is_err());
    }
}
