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
    // Native Medousa Coder. Keep the historic wire enum usable by native peers.
    Medousa,
    Codex,
    Cursor,
    Hermes,
}

impl ExternalPeerRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Medousa => "medousa",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Hermes => "hermes",
        }
    }
}

/// Workshop preference, never an execution grant. Fallbacks apply only before
/// an immutable assignment is prepared; a started assignment never migrates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CodingRuntimePreferences {
    pub preferred: ExternalPeerRuntime,
    #[serde(default)]
    pub fallbacks: Vec<ExternalPeerRuntime>,
}

impl Default for CodingRuntimePreferences {
    fn default() -> Self {
        Self {
            preferred: ExternalPeerRuntime::Medousa,
            fallbacks: Vec::new(),
        }
    }
}

impl CodingRuntimePreferences {
    pub fn ordered(&self) -> Vec<ExternalPeerRuntime> {
        std::iter::once(self.preferred)
            .chain(self.fallbacks.iter().copied())
            .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        let ordered = self.ordered();
        if ordered.len() > 4
            || ordered
                .iter()
                .enumerate()
                .any(|(i, runtime)| ordered[..i].contains(runtime))
        {
            return Err("coding runtimes must be unique, with at most three fallbacks".into());
        }
        Ok(())
    }

    pub fn select(&self, peers: &[ExternalPeerCandidate]) -> Option<ExternalPeerRuntime> {
        self.ordered().into_iter().find(|runtime| {
            peers.iter().any(|peer| {
                peer.target.runtime == *runtime && peer.availability == PeerAvailability::Ready
            })
        })
    }
}

#[cfg(test)]
mod coding_preference_tests {
    use super::*;

    fn peer(runtime: ExternalPeerRuntime, ready: bool) -> ExternalPeerCandidate {
        ExternalPeerCandidate {
            target: ExternalPeerTarget {
                authority_id: format!("auth_{}", "a".repeat(64)).parse().unwrap(),
                execution_runtime_id: "workshop".into(),
                runtime,
            },
            availability: if ready {
                PeerAvailability::Ready
            } else {
                PeerAvailability::Unavailable {
                    reason: "signed out".into(),
                }
            },
        }
    }

    #[test]
    fn default_is_native_without_implicit_external_fallback() {
        let preferences = CodingRuntimePreferences::default();
        assert_eq!(preferences.ordered(), vec![ExternalPeerRuntime::Medousa]);
        assert_eq!(
            preferences.select(&[peer(ExternalPeerRuntime::Codex, true)]),
            None
        );
        assert_eq!(
            preferences.select(&[peer(ExternalPeerRuntime::Medousa, true)]),
            Some(ExternalPeerRuntime::Medousa)
        );
    }

    #[test]
    fn preferences_select_first_ready_runtime_in_saved_order() {
        let preferences = CodingRuntimePreferences {
            preferred: ExternalPeerRuntime::Cursor,
            fallbacks: vec![ExternalPeerRuntime::Hermes, ExternalPeerRuntime::Medousa],
        };
        let peers = [
            peer(ExternalPeerRuntime::Medousa, true),
            peer(ExternalPeerRuntime::Codex, true),
            peer(ExternalPeerRuntime::Cursor, false),
            peer(ExternalPeerRuntime::Hermes, true),
        ];
        assert_eq!(
            preferences.select(&peers),
            Some(ExternalPeerRuntime::Hermes)
        );
        assert!(preferences.validate().is_ok());
        let duplicate = CodingRuntimePreferences {
            preferred: ExternalPeerRuntime::Medousa,
            fallbacks: vec![ExternalPeerRuntime::Medousa],
        };
        assert!(duplicate.validate().is_err());
    }
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
    /// Exact live ACP custody to adopt instead of creating a new provider
    /// session. Absence means this assignment creates new work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_agent_session_id: Option<String>,
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

/// Stable identity for one terminal external-peer assignment receipt.
/// Kept in the shared types crate so the portable Home client and full daemon
/// use identical replay keys without depending on the ACP storage crate.
pub fn peer_terminal_receipt_id(binding: &ExternalPeerAssignmentBinding) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    for part in [
        "medousa/peer-terminal/v1",
        binding.channel.authority_id.as_str(),
        &binding.channel.channel_id,
        &binding.assignment_id,
        &binding.agent_session_id,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    format!("peer_terminal_{:x}", hash.finalize())
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

/// Separate operator approval for one bounded owner turn. The host may expose
/// proposal-only coordination tools, but this grant never authorizes dispatch
/// or transfers the peer's execution authority.
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

/// Source-neutral, durable input for one owning Assistant session. Payloads
/// carry exact references or receipts; they are evidence, never executable text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct OwnerEvent {
    pub schema_version: u16,
    pub event_id: String,
    pub owner_principal_id: String,
    pub owner_session: SessionRef,
    pub channel: CoordinationChannelRef,
    pub source: OwnerEventSource,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub payload: OwnerEventPayload,
    #[serde(default)]
    pub limits: OwnerContinuationLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum OwnerEventSource {
    ExternalPeer {
        receipt_id: String,
    },
    Assignment {
        assignment_id: String,
    },
    Approval {
        approval_ref: String,
    },
    HumanMessage {
        message_ref: String,
    },
    Schedule {
        schedule_id: String,
        occurrence_id: String,
    },
    Delivery {
        delivery_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum OwnerEventPayload {
    AssignmentTerminal {
        assignment_id: String,
        receipt: Box<ExternalPeerAssignmentReceipt>,
    },
    Approval {
        assignment_id: Option<String>,
        approval_ref: String,
    },
    AddressedMessage {
        message_ref: String,
    },
    ScheduleOccurrence {
        schedule_id: String,
        occurrence_id: String,
    },
    Stall {
        assignment_id: String,
        evidence_ref: String,
    },
    DeliveryFailure {
        delivery_id: String,
        receipt_ref: String,
    },
}

/// Immutable ceilings copied onto the event so restart cannot reset a budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct OwnerContinuationLimits {
    pub causal_depth: u16,
    pub wake_count: u16,
    pub elapsed_seconds: u32,
    pub cost_microusd: u64,
    pub retry_count: u16,
    pub review_rounds: u16,
}
impl Default for OwnerContinuationLimits {
    fn default() -> Self {
        Self {
            causal_depth: 8,
            wake_count: 8,
            elapsed_seconds: 120,
            cost_microusd: 0,
            retry_count: 8,
            review_rounds: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct OwnerEventIntakeAttempt {
    pub event: OwnerEvent,
    pub attempt: u32,
    pub turn_id: String,
}

/// Consumption requires a committed owner decision and durable correlation to
/// every resulting command and the terminal delivery (when one was requested).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct OwnerEventIntakeAcknowledgment {
    pub intake: OwnerEventIntakeAttempt,
    pub decision: crate::TranscriptEntryRef,
    pub decision_digest: String,
    #[serde(default)]
    pub command_refs: Vec<String>,
    pub terminal_delivery_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct OwnerEventBlocked {
    pub event_id: String,
    pub reason: String,
    pub blocked_at: chrono::DateTime<chrono::Utc>,
    pub requires_user_decision: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum OwnerEventStatus {
    Pending,
    Started,
    Consumed,
    Blocked,
}

/// Bounded owner inbox inspection projection. Event evidence and lifecycle
/// references remain exact; this view grants no authority to execute it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct OwnerEventView {
    pub event: OwnerEvent,
    pub status: OwnerEventStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_attempt: Option<OwnerEventIntakeAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledgment: Option<OwnerEventIntakeAcknowledgment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<OwnerEventBlocked>,
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

/// Read-only execution observations; only a terminal receipt establishes the
/// assignment outcome. Missing observations never authorize a retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum PeerExecutionState {
    Accepted,
    Running,
    Blocked,
    AwaitingReceipt,
    Unobserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum PeerActivityStatus {
    Running,
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerAssignmentProgress {
    pub state: PeerExecutionState,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_activity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_status: Option<PeerActivityStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalReviewRecord {
    pub proposal: PeerAssignmentProposal,
    pub decision: Option<PeerProposalDecision>,
    /// Recorded custody remains visible until its terminal receipt arrives.
    pub binding: Option<ExternalPeerAssignmentBinding>,
    /// Present for source-session projections so a remote owner can observe the
    /// immutable terminal even though execution belongs to another workshop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ExternalPeerAssignmentReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<PeerAssignmentProgress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerProposalInboxResponse {
    pub proposals: Vec<PeerProposalReviewRecord>,
    pub next_cursor: Option<String>,
    /// Keeps the selected, owner-scoped assignment visible after it leaves the
    /// pending inbox. It does not consume a pagination slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracked_proposal: Option<PeerProposalReviewRecord>,
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
