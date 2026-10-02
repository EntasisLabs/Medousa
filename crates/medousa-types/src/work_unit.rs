//! Session-independent intent and resource relationships. These records describe
//! scope; they never confer access or replace native execution truth.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AuthorityId, SessionRef};

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
    pub completion_condition: String,
    pub state: WorkUnitState,
    pub state_reason: Option<String>,
    pub state_evidence: Vec<ResourceRef>,
    pub contact: WorkContactPreference,
    pub origin: Option<SessionRef>,
    pub revision: u64,
    pub provenance: RecordProvenance,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkGraphMutation {
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
