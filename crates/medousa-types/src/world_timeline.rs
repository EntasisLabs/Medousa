//! Redacted causal-timeline DTOs for governed browser and computer worlds.

use serde::{Deserialize, Serialize};

pub const WORLD_TIMELINE_EVENT_SCHEMA_VERSION: u16 = 1;
pub const WORLD_EVIDENCE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldTimelineCheckpoint {
    pub surface: String,
    pub world_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_generation: Option<u64>,
    pub admitted_at_ms: u64,
    pub permit_expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldTimelineCompensation {
    pub strategy: String,
    pub automatic_dispatch_allowed: bool,
    pub requires_new_intent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldTimelineRecovery {
    pub strategy: String,
    pub requires_fresh_admission: bool,
    /// Absent only when reading a legacy durable record that predated an
    /// explicit compensation boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compensation: Option<WorldTimelineCompensation>,
}

/// One bounded, secret-free event in the workshop daemon's cross-world ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldTimelineEvent {
    pub schema_version: u16,
    pub sequence: u64,
    pub recorded_at_ms: u64,
    pub world_id: String,
    pub authority_id: String,
    pub driver_id: String,
    pub ownership: String,
    pub surface: String,
    pub world_revision: u64,
    pub occurred_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorldTimelineCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<WorldTimelineRecovery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldTimelineResponse {
    pub events: Vec<WorldTimelineEvent>,
    pub next_sequence: u64,
    pub has_more: bool,
}

/// Durable, payload-free evidence promoted from a high-value world event.
///
/// Raw page text, action values, selectors, coordinates, screenshots, driver
/// errors, and reusable authority are deliberately absent. The causal ledger
/// remains the source of the human-readable summary for the referenced event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldEvidenceRecord {
    pub schema_version: u16,
    pub evidence_id: String,
    pub ledger_sequence: u64,
    pub promoted_at_ms: u64,
    pub world_id: String,
    pub authority_id: String,
    pub driver_id: String,
    pub ownership: String,
    pub surface: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_elapsed_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability_latency_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<WorldTimelineCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<WorldTimelineRecovery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorldEvidenceResponse {
    pub evidence: Vec<WorldEvidenceRecord>,
    pub next_sequence: u64,
    pub has_more: bool,
}
