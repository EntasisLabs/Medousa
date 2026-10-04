//! Sender-owned responsibility and callbacks, independent of user contact and execution grants.
use crate::{SessionRef, TranscriptEntryRef, coordination::*, work_unit::WorkContactPreference};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum HandoffResponsibility {
    #[default]
    Retain,
    Transfer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum HandoffCompletion {
    #[default]
    SenderReview,
    WorkerResult,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(default, deny_unknown_fields)]
pub struct PeerHandoffPolicy {
    pub responsibility: HandoffResponsibility,
    pub completion: HandoffCompletion,
    pub wake_on_accepted: bool,
    pub wake_on_terminal: bool,
    pub contact: WorkContactPreference,
}
impl Default for PeerHandoffPolicy {
    fn default() -> Self {
        Self {
            responsibility: HandoffResponsibility::Retain,
            completion: HandoffCompletion::SenderReview,
            wake_on_accepted: true,
            wake_on_terminal: true,
            contact: WorkContactPreference::ReturnToOrigin,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PeerHandoffAdmission {
    #[default]
    Delegate,
    Propose,
}
/// Model supplies intent. The native host captures source identity and issues scoped authority.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PeerHandoffIntent {
    pub request_key: String,
    pub instructions: String,
    pub forge_work_id: String,
    pub after_entry_seq: u64,
    pub through_entry_seq: u64,
    #[serde(default)]
    pub runtime: Option<ExternalPeerRuntime>,
    #[serde(default)]
    pub admission: PeerHandoffAdmission,
    #[serde(default)]
    pub handoff: PeerHandoffPolicy,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PeerHandoffRecord {
    pub request: ExternalPeerAssignmentRequest,
    pub policy: PeerHandoffPolicy,
    pub admission: PeerHandoffAdmission,
    pub source: TranscriptEntryRef,
    pub source_digest: String,
    pub sender_bot_id: Option<crate::BotId>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PeerHandoffState {
    AwaitingAcceptance,
    Working,
    AwaitingSenderReview,
    Accepted,
    ChangesRequested,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerHandoffView {
    pub handoff: PeerHandoffRecord,
    pub responsible_session: SessionRef,
    pub state: PeerHandoffState,
    pub binding: Option<ExternalPeerAssignmentBinding>,
    pub receipt: Option<ExternalPeerAssignmentReceipt>,
    pub review: Option<PeerHandoffReview>,
}
/// Inbox metadata beside its existing proposal, binding and receipt; no duplicated payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerHandoffSummary {
    pub policy: PeerHandoffPolicy,
    pub admission: PeerHandoffAdmission,
    pub responsible_session: SessionRef,
    pub state: PeerHandoffState,
    pub review: Option<PeerHandoffReview>,
}
impl From<PeerHandoffView> for PeerHandoffSummary {
    fn from(view: PeerHandoffView) -> Self {
        Self {
            policy: view.handoff.policy,
            admission: view.handoff.admission,
            responsible_session: view.responsible_session,
            state: view.state,
            review: view.review,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PeerHandoffVerdict {
    Accept,
    ChangesRequested,
}
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PeerHandoffReviewInput {
    pub channel: CoordinationChannelRef,
    pub assignment_id: String,
    pub receipt_id: String,
    pub verdict: PeerHandoffVerdict,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PeerHandoffReview {
    pub receipt_id: String,
    pub verdict: PeerHandoffVerdict,
    pub reason: String,
    pub sender_session: SessionRef,
    pub turn_id: String,
}
