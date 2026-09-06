use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

pub const WORLD_SCHEMA_VERSION: u16 = 1;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into().trim().to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

string_id!(WorldId);
string_id!(WorldAuthorityId);
string_id!(WorldDriverId);
string_id!(WorldPrincipalId);
string_id!(WorldResourceId);
string_id!(WorldGrantId);
string_id!(WorldIntentId);
string_id!(WorldTraceId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldOwnership {
    Owned,
    Managed,
    Attached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldSurfaceKind {
    Browser,
    Desktop,
    Application,
    Terminal,
    Composite,
}

/// Concrete adapter implementation that senses and acts on a world.
///
/// This is placement metadata, not authority. A registered driver can only
/// execute capabilities admitted by the world authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldDriverKind {
    EmbeddedBrowser,
    BrowserExtension,
    MobileBrowser,
    IsolatedBrowser,
    NativeDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldDriverTransport {
    InProcess,
    LoopbackHttp,
    ClientQueue,
    LocalSidecar,
}

/// Mechanical features offered by a driver instance. These do not grant a
/// principal permission to use any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldDriverCapability {
    SemanticObservation,
    PixelObservation,
    Navigation,
    Interaction,
    GuardedBatch,
    HumanTakeover,
    PersistentProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldDriverRegistration {
    pub driver_id: WorldDriverId,
    pub kind: WorldDriverKind,
    pub surface: WorldSurfaceKind,
    pub ownership: WorldOwnership,
    pub transport: WorldDriverTransport,
    pub capabilities: BTreeSet<WorldDriverCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldPrincipalKind {
    Human,
    Agent,
    Bot,
    Worker,
    Peer,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldPrincipal {
    pub principal_id: WorldPrincipalId,
    pub kind: WorldPrincipalKind,
}

impl WorldPrincipal {
    pub fn new(principal_id: impl Into<WorldPrincipalId>, kind: WorldPrincipalKind) -> Self {
        Self {
            principal_id: principal_id.into(),
            kind,
        }
    }

    pub fn human(principal_id: impl Into<WorldPrincipalId>) -> Self {
        Self::new(principal_id, WorldPrincipalKind::Human)
    }

    pub fn agent(principal_id: impl Into<WorldPrincipalId>) -> Self {
        Self::new(principal_id, WorldPrincipalKind::Agent)
    }

    pub fn system(principal_id: impl Into<WorldPrincipalId>) -> Self {
        Self::new(principal_id, WorldPrincipalKind::System)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldCapability {
    Observe,
    ObservePixels,
    Interact,
    ExternalEffect,
    IrreversibleEffect,
    UseCredentials,
    ClipboardRead,
    ClipboardWrite,
    FileTransfer,
    Debug,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum WorldResourceScope {
    All,
    Exact {
        resources: BTreeSet<WorldResourceId>,
    },
}

impl WorldResourceScope {
    pub fn exact(resources: impl IntoIterator<Item = WorldResourceId>) -> Self {
        Self::Exact {
            resources: resources.into_iter().collect(),
        }
    }

    pub fn includes(&self, resource_id: &WorldResourceId) -> bool {
        match self {
            Self::All => true,
            Self::Exact { resources } => resources.contains(resource_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldEffectClass {
    Observe,
    ObservePixels,
    LocalReversible,
    LocalMutation,
    ExternalEffect,
    Irreversible,
}

impl WorldEffectClass {
    pub fn required_capability(self) -> WorldCapability {
        match self {
            Self::Observe => WorldCapability::Observe,
            Self::ObservePixels => WorldCapability::ObservePixels,
            Self::LocalReversible | Self::LocalMutation => WorldCapability::Interact,
            Self::ExternalEffect => WorldCapability::ExternalEffect,
            Self::Irreversible => WorldCapability::IrreversibleEffect,
        }
    }

    pub fn requires_control(self) -> bool {
        !matches!(self, Self::Observe | Self::ObservePixels)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSessionSpec {
    pub world_id: WorldId,
    pub authority_id: WorldAuthorityId,
    pub driver_id: WorldDriverId,
    pub ownership: WorldOwnership,
    pub surface: WorldSurfaceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldControlLease {
    pub principal: WorldPrincipal,
    pub generation: u64,
    pub acquired_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

impl WorldControlLease {
    pub fn is_active_at(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_none_or(|expires| expires > now_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSession {
    pub schema_version: u16,
    pub world_id: WorldId,
    pub authority_id: WorldAuthorityId,
    pub driver_id: WorldDriverId,
    pub ownership: WorldOwnership,
    pub surface: WorldSurfaceKind,
    pub revision: u64,
    pub control_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_control_lease: Option<WorldControlLease>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCapabilityGrant {
    pub grant_id: WorldGrantId,
    pub world_id: WorldId,
    pub issued_by: WorldPrincipal,
    pub subject: WorldPrincipal,
    pub capabilities: BTreeSet<WorldCapability>,
    pub resource_scope: WorldResourceScope,
    pub issued_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at_ms: Option<u64>,
}

impl WorldCapabilityGrant {
    pub fn is_active_at(&self, now_ms: u64) -> bool {
        self.revoked_at_ms.is_none() && self.expires_at_ms.is_none_or(|expires| expires > now_ms)
    }

    pub fn admits(
        &self,
        principal: &WorldPrincipal,
        capability: WorldCapability,
        resource_id: &WorldResourceId,
        now_ms: u64,
    ) -> bool {
        self.subject == *principal
            && self.is_active_at(now_ms)
            && self.capabilities.contains(&capability)
            && self.resource_scope.includes(resource_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldGrantRequest {
    pub grant_id: WorldGrantId,
    pub issued_by: WorldPrincipal,
    pub subject: WorldPrincipal,
    pub capabilities: BTreeSet<WorldCapability>,
    pub resource_scope: WorldResourceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldActionIntent {
    pub intent_id: WorldIntentId,
    pub trace_id: WorldTraceId,
    pub principal: WorldPrincipal,
    pub resource_id: WorldResourceId,
    pub expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_control_generation: Option<u64>,
    pub required_capability: WorldCapability,
    pub effect_class: WorldEffectClass,
    pub idempotency_key: String,
    pub permit_expires_at_ms: u64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldActionPermit {
    pub world_id: WorldId,
    pub driver_id: WorldDriverId,
    pub intent_id: WorldIntentId,
    pub trace_id: WorldTraceId,
    pub principal: WorldPrincipal,
    pub resource_id: WorldResourceId,
    pub grant_id: WorldGrantId,
    pub admitted_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_generation: Option<u64>,
    pub effect_class: WorldEffectClass,
    pub idempotency_key: String,
    pub expires_at_ms: u64,
    pub admitted_event_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldActionStatus {
    Confirmed,
    NeedsReconciliation,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldActionOutcome {
    pub world_id: WorldId,
    pub driver_id: WorldDriverId,
    pub intent_id: WorldIntentId,
    pub trace_id: WorldTraceId,
    pub principal: WorldPrincipal,
    pub resource_id: WorldResourceId,
    pub effect_class: WorldEffectClass,
    pub status: WorldActionStatus,
    pub committed_revision: u64,
    pub event_sequence: u64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum WorldAdmission {
    Admitted { permit: WorldActionPermit },
    Replay { outcome: WorldActionOutcome },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorldEventKind {
    WorldCreated {
        driver_id: WorldDriverId,
        ownership: WorldOwnership,
        surface: WorldSurfaceKind,
    },
    CapabilityGranted {
        grant_id: WorldGrantId,
        subject: WorldPrincipal,
    },
    CapabilityRevoked {
        grant_id: WorldGrantId,
    },
    ControlAcquired {
        generation: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preempted: Option<WorldPrincipal>,
    },
    ControlReleased {
        generation: u64,
    },
    ActionAdmitted {
        grant_id: WorldGrantId,
        effect_class: WorldEffectClass,
    },
    ActionCommitted {
        effect_class: WorldEffectClass,
        status: WorldActionStatus,
        summary: String,
    },
    ActionFailed {
        effect_class: WorldEffectClass,
        error: String,
    },
    ExternalMutationObserved {
        summary: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldEvent {
    pub sequence: u64,
    pub world_id: WorldId,
    pub world_revision: u64,
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<WorldPrincipal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<WorldResourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<WorldIntentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<WorldTraceId>,
    pub event: WorldEventKind,
}
