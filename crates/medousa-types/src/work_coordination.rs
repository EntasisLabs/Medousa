//! Native execute → review custody. Registration is intent, never a grant.
use crate::{coordination::CoordinationChannelRef, work_unit::UserDomainRef};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinationRoundInput {
    pub executor_proposal_id: String,
    pub reviewer_proposal_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinationInput {
    pub coordination_id: String,
    pub work_unit_id: String,
    pub expected_scope_revision: u64,
    pub channel: CoordinationChannelRef,
    pub executor_proposal_id: String,
    pub reviewer_proposal_id: String,
    /// Absolute bound; at most 24 hours and no later than either approval.
    pub deadline: DateTime<Utc>,
    /// Up to three separately approved fix/review rounds, only after exact changes_requested.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fix_review_rounds: Vec<WorkCoordinationRoundInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinationQuery {
    pub channel: CoordinationChannelRef,
    pub coordination_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinationRoundAssignments {
    pub executor_assignment_id: String,
    pub reviewer_assignment_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinationPlan {
    pub domain: UserDomainRef,
    pub input: WorkCoordinationInput,
    /// Pins scope resource availability and registry/native versions as well
    /// as the unit's scope generation. Contact changes do not alter this pin.
    pub scope_digest: String,
    pub executor_assignment_id: String,
    pub reviewer_assignment_id: String,
    pub forge_work_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fix_review_assignments: Vec<WorkCoordinationRoundAssignments>,
    #[serde(default, skip_serializing_if = "round_is_zero")]
    pub round_index: u8,
}

fn round_is_zero(round: &u8) -> bool {
    *round == 0
}

impl WorkCoordinationPlan {
    /// Derive a round from immutable root intent; no grant or request changes.
    pub fn round(&self, index: u8) -> Option<Self> {
        if self.round_index != 0 {
            return None;
        }
        let mut round = self.clone();
        if index > 0 {
            let input = self.input.fix_review_rounds.get(usize::from(index - 1))?;
            let assignments = self.fix_review_assignments.get(usize::from(index - 1))?;
            round.input.executor_proposal_id = input.executor_proposal_id.clone();
            round.input.reviewer_proposal_id = input.reviewer_proposal_id.clone();
            round.executor_assignment_id = assignments.executor_assignment_id.clone();
            round.reviewer_assignment_id = assignments.reviewer_assignment_id.clone();
        }
        round.round_index = index;
        Some(round)
    }
}

/// Captured from a clean governed checkout after native executor completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkReviewInput {
    pub coordination_id: String,
    pub work_unit_id: String,
    pub executor_assignment_id: String,
    pub executor_receipt_id: String,
    pub forge_work_id: String,
    pub environment_generation: u32,
    pub branch: String,
    pub head_oid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkReviewVerdict {
    Approved,
    ChangesRequested,
}

/// A completed reviewer process is not an approval. Its entire result must be
/// this bounded envelope, naming the exact input supplied by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkReviewDecision {
    pub reviewed: WorkReviewInput,
    pub verdict: WorkReviewVerdict,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkCoordinationOutcome {
    Approved,
    ChangesRequested,
    ExecutorFailed,
    ReviewerFailed,
    InvalidReview,
    RevisionChanged,
    DeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinationResult {
    pub coordination_id: String,
    pub outcome: WorkCoordinationOutcome,
    pub receipt_id: Option<String>,
    pub decision: Option<WorkReviewDecision>,
}
