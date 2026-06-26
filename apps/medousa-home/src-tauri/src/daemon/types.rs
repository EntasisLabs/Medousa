//! Shared daemon API types from `medousa-types`, plus Tauri-only view wrappers.

pub use medousa_types::capability::*;
pub use medousa_types::daemon_api::*;
pub use medousa_types::grapheme_extras::*;
pub use medousa_types::mcp_gateway::*;
pub use medousa_types::session::{ConversationTurn, SessionHistorySummary};
pub use medousa_types::stage_routing::*;
pub use medousa_types::turn::*;
pub use medousa_types::tool_history::*;
pub use medousa_types::workflow_plan::*;

pub use medousa_types::WorkflowStepSpec as WorkflowStepSpecDto;

use chrono::{DateTime, Utc};
use serde::Serialize;

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
