//! Canonical ownership projection over Medousa's existing execution engines.
//!
//! An assignment is an ownership record, not an execution grant. Native worker,
//! workflow, job, schedule, and ACP records remain authoritative for execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AuthorityId, SessionRef};

pub const ASSISTANT_ASSIGNMENT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AssistantAssignmentKind {
    InternalTurnWorker,
    ExternalPeer,
    Job,
    Workflow,
    RecurringSchedule,
    ScheduledOccurrence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AssistantAssignmentStatus {
    Unknown,
    AwaitingApproval,
    Pending,
    Running,
    Blocked,
    Unavailable,
    Interrupted,
    Completed,
    Failed,
    Cancelled,
}

impl AssistantAssignmentStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentSource {
    pub authority_id: AuthorityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordination_channel_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantExecutionBinding {
    pub kind: AssistantAssignmentKind,
    /// Stable id in the subsystem that actually executes the work.
    pub native_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_session: Option<SessionRef>,
    pub workshop_authority_id: AuthorityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge_work_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentReferences {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_id: Option<String>,
}

/// Immutable origin of one user-owned unit of work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignment {
    pub schema_version: u16,
    pub assignment_id: String,
    pub owner_id: String,
    pub principal_id: String,
    pub intent: String,
    pub source: AssistantAssignmentSource,
    pub execution: AssistantExecutionBinding,
    pub budget: AssistantAssignmentBudget,
    pub status: AssistantAssignmentStatus,
    pub references: AssistantAssignmentReferences,
    pub actor_id: String,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentReferencePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_id: Option<String>,
}

/// Create-only observation from a native executor. Sequence is native-source
/// order; terminal identity is additionally fenced by the durable ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentEvent {
    pub event_id: String,
    pub assignment_id: String,
    pub native_sequence: u64,
    pub status: AssistantAssignmentStatus,
    pub actor_id: String,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(default)]
    pub references: AssistantAssignmentReferencePatch,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentCommand {
    pub command_id: String,
    pub assignment_id: String,
    pub action: String,
    pub actor_id: String,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentView {
    #[serde(flatten)]
    pub assignment: AssistantAssignment,
    pub event_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_event_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentListFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AssistantAssignmentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantAssignmentListPage {
    pub assignments: Vec<AssistantAssignmentView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

pub fn apply_assignment_event(
    assignment: &mut AssistantAssignment,
    event: &AssistantAssignmentEvent,
) -> Result<(), &'static str> {
    if event.assignment_id != assignment.assignment_id {
        return Err("assignment event identity mismatch");
    }
    if assignment.status.is_terminal() {
        return Ok(());
    }
    assignment.status = event.status;
    assignment.updated_at = assignment.updated_at.max(event.observed_at);
    assignment.next_action = event.next_action.clone();
    if let Some(value) = &event.references.receipt_id {
        assignment.references.receipt_id = Some(value.clone());
    }
    if let Some(value) = &event.references.delivery_id {
        assignment.references.delivery_id = Some(value.clone());
    }
    if let Some(value) = &event.references.continuation_id {
        assignment.references.continuation_id = Some(value.clone());
    }
    Ok(())
}
