//! Read-only placement discovery for Assistant-owned work.
//!
//! Candidate selection describes current evidence and never grants execution
//! authority. Native admission and operator approval remain authoritative.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const ASSISTANT_PLACEMENT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AssistantExecutorKind {
    Inline,
    InternalWorker,
    Workflow,
    LocalAcp,
    #[default]
    Workshop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AssistantContextLocality {
    /// Any currently authorized location may be considered.
    #[default]
    AnyAuthorized,
    /// Prefer work colocated with the source context, but allow other eligible targets.
    PreferSourceWorkshop,
    /// Require the exact authority that owns the source context.
    RequireSourceWorkshop,
}

/// Optional caller constraints for a read-only placement query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct AssistantPlacementRequest {
    pub schema_version: u16,
    pub required_capabilities: BTreeSet<String>,
    pub requested_executor: Option<AssistantExecutorKind>,
    /// Exact external adapter name, e.g. `cursor`; meaningful for local ACP.
    pub requested_adapter: Option<String>,
    pub requested_runtime_id: Option<String>,
    pub requested_workshop_id: Option<String>,
    pub requested_workshop_authority_id: Option<String>,
    pub forbidden_executors: BTreeSet<AssistantExecutorKind>,
    pub forbidden_adapters: BTreeSet<String>,
    pub forbidden_runtime_ids: BTreeSet<String>,
    pub forbidden_workshop_ids: BTreeSet<String>,
    pub forbidden_workshop_authority_ids: BTreeSet<String>,
    pub governed_work: Option<AssistantGovernedWorkBinding>,
    /// Exact ACP session requested for adoption. This must match inventory evidence.
    pub requested_adoptable_agent_session_id: Option<String>,
    pub source_workshop_authority_id: Option<String>,
    pub context_locality: AssistantContextLocality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantGovernedWorkBinding {
    pub workshop_authority_id: String,
    pub forge_work_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AssistantCandidateAvailability {
    Available,
    Unavailable,
    Unknown,
}

/// One observed executor/work pairing. Missing identity or capability evidence
/// is represented as unknown and cannot satisfy a constrained request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantPlacementCandidate {
    pub candidate_id: String,
    pub label: String,
    pub executor: AssistantExecutorKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workshop_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workshop_authority_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge_work_id: Option<String>,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    pub availability: AssistantCandidateAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adoptable_agent_session_id: Option<String>,
    pub eligible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssistantPlacementResult {
    pub schema_version: u16,
    pub complete: bool,
    pub candidates: Vec<AssistantPlacementCandidate>,
    pub policy: String,
}

impl Default for AssistantPlacementRequest {
    fn default() -> Self {
        Self {
            schema_version: ASSISTANT_PLACEMENT_SCHEMA_VERSION,
            required_capabilities: BTreeSet::new(),
            requested_executor: None,
            requested_adapter: None,
            requested_runtime_id: None,
            requested_workshop_id: None,
            requested_workshop_authority_id: None,
            forbidden_executors: BTreeSet::new(),
            forbidden_adapters: BTreeSet::new(),
            forbidden_runtime_ids: BTreeSet::new(),
            forbidden_workshop_ids: BTreeSet::new(),
            forbidden_workshop_authority_ids: BTreeSet::new(),
            governed_work: None,
            requested_adoptable_agent_session_id: None,
            source_workshop_authority_id: None,
            context_locality: AssistantContextLocality::AnyAuthorized,
        }
    }
}
