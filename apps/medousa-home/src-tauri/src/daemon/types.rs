//! Shared daemon API types from `medousa-types`, plus Tauri-only view wrappers.

pub use medousa_types::daemon_api::*;
pub use medousa_types::session::{ConversationTurn, SessionHistorySummary};
pub use medousa_types::stage_routing::*;
pub use medousa_types::turn::*;
pub use medousa_types::tool_history::*;
pub use medousa_types::workflow_plan::*;

pub use medousa_types::WorkflowStepSpec as WorkflowStepSpecDto;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tauri health probe summary (derived from [`HealthResponse`]).
#[derive(Debug, Clone, Serialize)]
pub struct DaemonHealth {
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_registry_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_runtime_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent_turn_at_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent_turn_latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTurnAccepted {
    pub turn_id: String,
    pub stream_url: String,
}

// ── Capability catalog (not yet in medousa-types) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBindingSummary {
    pub source: String,
    pub reference: String,
    pub available: bool,
    #[serde(default)]
    pub effect_class: Option<String>,
    #[serde(default)]
    pub invoke_via: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListEntry {
    pub id: String,
    pub title: String,
    pub binding_count: usize,
    #[serde(default)]
    pub description: Option<String>,
    pub domain: String,
    pub has_grapheme: bool,
    pub has_mcp: bool,
    #[serde(default)]
    pub bindings_summary: Vec<CapabilityBindingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListResponse {
    pub capabilities: Vec<CapabilityListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub source: String,
    pub reference: String,
    pub priority: u16,
    pub available: bool,
    #[serde(default)]
    pub unavailable_reason: Option<String>,
    #[serde(default)]
    pub invoke_via: Option<String>,
    #[serde(default)]
    pub effect_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityImplementations {
    #[serde(default)]
    pub grapheme: Vec<CapabilityBinding>,
    #[serde(default)]
    pub mcp: Vec<CapabilityBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRecommendation {
    pub source: String,
    pub reference: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResolveResponse {
    pub capability: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub implementations: CapabilityImplementations,
    #[serde(default)]
    pub recommended: Option<CapabilityRecommendation>,
    #[serde(default)]
    pub gateway_unreachable: Option<bool>,
}

// ── Grapheme workshop extras (not yet in medousa-types) ───────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeAllowlistResponse {
    pub allowed_modules: Vec<String>,
    pub enforce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeAllowlistUpdateRequest {
    pub allowed_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeCompileRequest {
    pub source: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeCompileResponse {
    pub mode: String,
    pub validated: bool,
    pub artifact_id: Option<String>,
    pub lint_warnings: Vec<String>,
    pub compile_hints: Vec<String>,
    pub aot_stage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeModuleLoadRequest {
    pub module_id: String,
    pub wasm_path: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub abi: Option<String>,
    #[serde(default)]
    pub compatibility_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeModuleLoadResponse {
    pub module_id: String,
    pub generation_id: u64,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeLifecycleEventDto {
    pub kind: String,
    pub module_id: String,
    #[serde(default)]
    pub generation_id: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeLifecycleResponse {
    pub events: Vec<GraphemeLifecycleEventDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeLspWorkspaceResponse {
    pub root_path: String,
    pub root_uri: String,
    pub scripts_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeScriptSaveRequest {
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub source_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphemeScriptSaveResponse {
    pub script: GraphemeScriptEntryDto,
}
