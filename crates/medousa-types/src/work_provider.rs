//! Exact provider request/result association. A provider outcome is distinct
//! from native execution custody, work satisfaction, and contact delivery.
use crate::{
    ExternalEventKind, ExternalProvider,
    coordination::CoordinationChannelRef,
    work_coordination::{WorkReviewDecision, WorkReviewInput},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderReviewSource {
    pub channel: CoordinationChannelRef,
    pub executor_assignment_id: String,
}

/// Exact request identity within the admitted owner's local user domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderRequestRef {
    pub conversation_id: String,
    pub request_id: String,
}

/// Native-derived pin of the immutable predecessor terminal, never a caller verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderTerminalRef {
    pub request: WorkProviderRequestRef,
    pub event_id: String,
    pub event_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderDispatchSource {
    pub request: WorkProviderRequestRef,
    pub target_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderRequestInput {
    pub work_unit_id: String,
    pub expected_scope_revision: u64,
    pub deadline: DateTime<Utc>,
    /// The runtime derives the receipt/revision pin; callers cannot assert it.
    #[serde(default)]
    pub review_of: Option<WorkProviderReviewSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderRequest {
    pub conversation_id: String,
    pub request_id: String,
    pub provider: ExternalProvider,
    pub input: WorkProviderRequestInput,
    pub instruction_digest: String,
    pub scope_digest: String,
    pub completion_condition: String,
    pub reviewed: Option<WorkReviewInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<WorkProviderTerminalRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkProviderQualification {
    OutcomeOnly,
    ReviewApproved,
    ChangesRequested,
    InvalidReview,
    RevisionChanged,
    ScopeChanged,
    InactiveWork,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderEvent {
    pub conversation_id: String,
    pub request_id: String,
    pub event_id: String,
    pub actor_id: String,
    pub request_sequence: u64,
    pub kind: ExternalEventKind,
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub qualification: WorkProviderQualification,
    pub review_decision: Option<WorkReviewDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkProviderRecord {
    pub request: WorkProviderRequest,
    pub events: Vec<WorkProviderEvent>,
    pub dispatch_claimed: bool,
    pub revision: u64,
}

/// A future stage supplied with native operator admission. Work scope and
/// deadline are inherited; the runtime freezes its destination separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderStageInput {
    pub conversation_id: String,
    pub request_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub coordinator_wake: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderPlannedStage {
    pub input: WorkProviderStageInput,
    pub provider: ExternalProvider,
    pub target_digest: String,
}

/// One immutable, operator-admitted handoff after a native executor or exact
/// provider predecessor completes. Recovery derives its result/revision pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkProviderDispatch {
    pub conversation_id: String,
    pub request_id: String,
    pub provider: ExternalProvider,
    pub input: WorkProviderRequestInput,
    pub instructions: String,
    pub target_digest: String,
    pub scope_digest: String,
    pub source_request_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_provider_completion: Option<WorkProviderDispatchSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remaining_stages: Vec<WorkProviderPlannedStage>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub coordinator_wake: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkProviderDispatchRecord {
    pub dispatch: WorkProviderDispatch,
    pub closed_reason: Option<String>,
    pub revision: u64,
}
