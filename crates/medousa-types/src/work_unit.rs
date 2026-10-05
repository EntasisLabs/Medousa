//! Session-independent intent and resource relationships. These records describe
//! scope; they never confer access or replace native execution truth.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    AuthorityId, SessionRef,
    work_coordinator::{
        WorkCoordinatorAttempt, WorkCoordinatorDecision, WorkCoordinatorWake,
        WorkCoordinatorWakeRecord,
    },
    work_provider::{
        WorkProviderDispatch, WorkProviderDispatchRecord, WorkProviderEvent, WorkProviderRecord,
        WorkProviderRequest,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UserDomainRef {
    pub authority_id: AuthorityId,
    /// Authenticated owner identity, resolved at admission rather than supplied
    /// by a model or inferred from a display name.
    pub user_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Project,
    ForgeWork,
    VaultNote,
    VaultFolder,
    Artifact,
    Component,
    Feed,
    Session,
    Assignment,
    Job,
    WorkUnit,
    Bot,
    ExternalAgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ResourceRef {
    pub authority_id: AuthorityId,
    pub kind: ResourceKind,
    /// Native stable identity or a registry identity issued by an adapter.
    /// A path, display title, or content hash alone is not an identity.
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResourceResolution {
    Unresolved,
    Available,
    Unavailable,
    Tombstoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RecordSource {
    UserDirect,
    ModelInferred,
    SystemEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RecordProvenance {
    pub actor_id: String,
    pub source: RecordSource,
    pub evidence: Vec<ResourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ResourceRecord {
    pub reference: ResourceRef,
    /// Mutable adapter locator; never used to determine identity.
    pub locator: Option<String>,
    pub native_revision: Option<String>,
    pub resolution: ResourceResolution,
    pub revision: u64,
    pub provenance: RecordProvenance,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResourceRelationshipKind {
    Supports,
    Informs,
    Produces,
    Tracks,
    RelatedTo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RelationshipRecord {
    pub relationship_id: String,
    pub from: ResourceRef,
    pub to: ResourceRef,
    pub kind: ResourceRelationshipKind,
    pub revision: u64,
    pub provenance: RecordProvenance,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkScope {
    #[serde(default)]
    pub resources: Vec<ResourceRef>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Explicit checkpoints for maintenance members. Omission requires a
    /// terminal satisfied child, which an ongoing responsibility cannot supply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub readiness: Vec<WorkReadinessRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkReadinessRequirement {
    pub work_unit_id: String,
    pub condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkRevisionEvidence {
    pub reference: ResourceRef,
    pub native_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkReadiness {
    pub condition: String,
    pub scope_revision: u64,
    pub evidence: Vec<WorkRevisionEvidence>,
    /// Registry revisions pin availability as well as native content versions.
    pub resource_revisions: Vec<u64>,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkBudgetLimits {
    pub cost_microusd: u64,
    pub execution_count: u32,
    pub concurrent_executions: u16,
    /// Absolute deadline survives process and conversation restarts.
    pub deadline: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkBudgetUsage {
    /// Outstanding reservations plus settled actual cost. An overrun is retained.
    pub cost_microusd: u64,
    /// Saturation never permits another reservation. Individual ledger records
    /// retain actual overrun facts even when the aggregate exceeds u64.
    pub cost_overflowed: bool,
    pub execution_count: u32,
    pub concurrent_executions: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkBudgetDisposition {
    Completed,
    /// Native admission proves the execution never started; release its hold.
    NotStarted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkBudgetReservation {
    pub reservation_id: String,
    pub work_unit_id: String,
    pub execution: ResourceRef,
    pub reserved_cost_microusd: u64,
    /// Sticky aggregate charges: removing a child cannot erase admitted usage.
    pub charged_units: Vec<String>,
    pub disposition: Option<WorkBudgetDisposition>,
    pub actual_cost_microusd: Option<u64>,
    pub revision: u64,
    pub provenance: RecordProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkUnitKind {
    Finite,
    Maintenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkUnitState {
    Accepted,
    Active,
    Waiting,
    NeedsAttention,
    Paused,
    Satisfied,
    Failed,
    Cancelled,
}

impl WorkUnitState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Satisfied | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkContactPreference {
    Silent,
    #[default]
    ReturnToOrigin,
    Participant {
        participant: ResourceRef,
    },
    Channel {
        channel: ResourceRef,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkUnit {
    pub work_unit_id: String,
    pub intent: String,
    pub kind: WorkUnitKind,
    /// Saved selection; graph adjacency never expands this automatically.
    pub scope: WorkScope,
    /// Changes only when scope changes; contact and conversation updates do not
    /// invalidate a checkpoint. Zero is the legacy snapshot representation.
    #[serde(default)]
    pub scope_revision: u64,
    #[serde(default)]
    pub readiness: Option<WorkReadiness>,
    pub completion_condition: String,
    pub state: WorkUnitState,
    pub state_reason: Option<String>,
    pub state_evidence: Vec<ResourceRef>,
    pub contact: WorkContactPreference,
    pub origin: Option<SessionRef>,
    /// Exact local conversations attached to this responsibility. These are
    /// associations, not grants or delivery targets.
    #[serde(default)]
    pub conversations: Vec<SessionRef>,
    #[serde(default)]
    pub budget: Option<WorkBudgetLimits>,
    pub revision: u64,
    pub provenance: RecordProvenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Selected observations, never an implicit grant to execute or contact anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkEventKind {
    ResourceObserved,
    WorkStateChanged,
    WorkScopeChanged,
    ProviderProgress,
    ProviderCompleted,
    ProviderFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkSubscriptionInput {
    pub subscription_id: String,
    pub work_unit_id: String,
    pub expected_scope_revision: u64,
    /// Exact resources within the saved work scope; no transitive selectors.
    pub resources: Vec<ResourceRef>,
    pub event_kinds: Vec<WorkEventKind>,
    /// Exclusive durable journal cursor, including events racing registration.
    pub after_revision: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkEventSubscription {
    pub input: WorkSubscriptionInput,
    /// Frozen authenticated actor; a caller cannot select another recipient.
    pub recipient_actor_id: String,
    pub acknowledged_revision: u64,
    pub stopped_at_revision: Option<u64>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkEventsQuery {
    pub subscription_id: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkSubscriptionStatus {
    Active,
    Paused,
    ScopeChanged,
    Terminal,
    Expired,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkEventsPage {
    pub revision: u64,
    pub subscription: WorkEventSubscription,
    pub status: WorkSubscriptionStatus,
    pub events: Vec<WorkGraphEvent>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkGraphMutation {
    Subscribe {
        input: WorkSubscriptionInput,
    },
    AcknowledgeEvent {
        subscription_id: String,
        event_revision: u64,
        /// Attributable observation/decision receipt, not a provider effect.
        decision: String,
    },
    /// Native runtime decision and inbox acknowledgment commit atomically.
    AdvanceProviderStage {
        subscription_id: String,
        event_revision: u64,
        state: Option<WorkUnitState>,
        reason: String,
    },
    StopSubscription {
        subscription_id: String,
    },
    RecordResource {
        reference: ResourceRef,
        locator: Option<String>,
        native_revision: Option<String>,
        resolution: ResourceResolution,
    },
    PutRelationship {
        relationship_id: String,
        from: ResourceRef,
        to: ResourceRef,
        kind: ResourceRelationshipKind,
    },
    AcceptWork {
        work_unit_id: String,
        intent: String,
        kind: WorkUnitKind,
        scope: WorkScope,
        completion_condition: String,
        #[serde(default)]
        contact: WorkContactPreference,
        origin: Option<SessionRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget: Option<WorkBudgetLimits>,
    },
    SetScope {
        work_unit_id: String,
        scope: WorkScope,
    },
    SetState {
        work_unit_id: String,
        state: WorkUnitState,
        reason: String,
        evidence: Vec<ResourceRef>,
    },
    SetContact {
        work_unit_id: String,
        contact: WorkContactPreference,
    },
    AttachConversation {
        work_unit_id: String,
        session: SessionRef,
    },
    RecordReadiness {
        work_unit_id: String,
        expected_scope_revision: u64,
        condition: String,
        evidence: Vec<WorkRevisionEvidence>,
        /// Checkpoints expire; the runtime does not infer continuing freshness.
        valid_for_seconds: u32,
    },
    SetBudget {
        work_unit_id: String,
        limits: WorkBudgetLimits,
    },
    /// Runtime-only reservation; storing intent is not executor admission.
    ReserveBudget {
        reservation_id: String,
        work_unit_id: String,
        execution: ResourceRef,
        reserved_cost_microusd: u64,
    },
    /// Runtime-only settlement against authoritative native custody.
    SettleBudget {
        reservation_id: String,
        disposition: WorkBudgetDisposition,
        actual_cost_microusd: u64,
    },
    /// Native-only association, retained before provider effects.
    RegisterProviderRequest {
        request: Box<WorkProviderRequest>,
    },
    RegisterProviderDispatch {
        dispatch: Box<WorkProviderDispatch>,
    },
    RegisterCoordinatorWake {
        wake: Box<WorkCoordinatorWake>,
    },
    ClaimCoordinatorWake {
        conversation_id: String,
        request_id: String,
        attempt: WorkCoordinatorAttempt,
    },
    CompleteCoordinatorWake {
        conversation_id: String,
        request_id: String,
        decision: WorkCoordinatorDecision,
    },
    BlockCoordinatorWake {
        conversation_id: String,
        request_id: String,
        reason: String,
    },
    CloseProviderDispatch {
        conversation_id: String,
        request_id: String,
        reason: String,
    },
    /// Native-only single dispatch custody. Unknown claims never relaunch.
    ClaimProviderRequest {
        conversation_id: String,
        request_id: String,
    },
    /// Authenticated provider evidence; not native execution or satisfaction.
    RecordProviderEvent {
        event: Box<WorkProviderEvent>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkGraphCommand {
    /// Replays of the exact command return the original receipt, even after
    /// later commits. Reusing this key for different intent is a conflict.
    pub command_id: String,
    pub expected_revision: u64,
    pub mutation: WorkGraphMutation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkGraphReceipt {
    pub command_id: String,
    pub revision: u64,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkGraphEvent {
    pub command: WorkGraphCommand,
    pub provenance: RecordProvenance,
    pub receipt: WorkGraphReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkGraphCollection {
    #[default]
    Resources,
    Relationships,
    WorkUnits,
    Events,
    BudgetReservations,
    Subscriptions,
    ProviderRequests,
    ProviderDispatches,
    CoordinatorWakes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RelationshipDirection {
    #[default]
    Both,
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkGraphQuery {
    #[serde(default)]
    pub collection: WorkGraphCollection,
    pub anchor: Option<ResourceRef>,
    #[serde(default)]
    pub direction: RelationshipDirection,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum WorkGraphItem {
    Resource(ResourceRecord),
    Relationship(RelationshipRecord),
    WorkUnit(WorkUnit),
    Event(WorkGraphEvent),
    BudgetReservation(WorkBudgetReservation),
    Subscription(WorkEventSubscription),
    ProviderRequest(WorkProviderRecord),
    ProviderDispatch(WorkProviderDispatchRecord),
    CoordinatorWake(WorkCoordinatorWakeRecord),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkGraphPage {
    pub domain: UserDomainRef,
    pub revision: u64,
    pub items: Vec<WorkGraphItem>,
    pub next_cursor: Option<String>,
    /// Coverage is this domain registry on this workshop, not a complete mesh
    /// or a claim that every native resource has already been indexed.
    pub coverage: String,
}
