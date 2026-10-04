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

/// One immutable, operator-admitted provider review handoff. The runtime derives
/// the checkout pin after the saved native executor finishes.
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub coordinator_wake: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkProviderDispatchRecord {
    pub dispatch: WorkProviderDispatch,
    pub closed_reason: Option<String>,
    pub revision: u64,
}
