//! Shared HTTP contract between medousa-daemon and medousa-mcp-gateway.
//!
//! Design: docs/internal/mcp-client-gateway-design.md

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_MCP_GATEWAY_BIND: &str = "127.0.0.1:7420";
pub const DEFAULT_MCP_GATEWAY_URL: &str = "http://127.0.0.1:7420";

pub fn resolve_mcp_gateway_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| std::env::var("MEDOUSA_MCP_GATEWAY_URL").ok())
        .unwrap_or_else(|| DEFAULT_MCP_GATEWAY_URL.to_string())
}

/// Execution lane copied from turn context for gateway-side pre-checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTurnLane {
    Interactive,
    Scheduled,
    Heartbeat,
}

impl McpTurnLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Scheduled => "scheduled",
            Self::Heartbeat => "heartbeat",
        }
    }
}

/// Turn metadata forwarded on every discover/invoke for policy and audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTurnContext {
    pub turn_id: String,
    pub session_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub lane: McpTurnLane,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_profile: Option<String>,
}

/// Inferred side-effect class for policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpEffectClass {
    ExternalRead,
    ExternalWrite,
    ExternalSideEffect,
}

impl McpEffectClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExternalRead => "external_read",
            Self::ExternalWrite => "external_write",
            Self::ExternalSideEffect => "external_side_effect",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCatalogEntry {
    pub server_id: String,
    pub server_title: String,
    pub tool_name: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema_summary: Option<String>,
    pub effect_class: McpEffectClass,
    #[serde(default)]
    pub capability_ids: Vec<String>,
    #[serde(default = "default_stability")]
    pub stability: String,
}

fn default_stability() -> String {
    "stable".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpDiscoverRequest {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default)]
    pub limit: usize,
    pub turn_context: McpTurnContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpDiscoverResponse {
    pub query: String,
    pub matches: Vec<McpToolCatalogEntry>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_unreachable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInvokeRequest {
    pub server_id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub turn_context: McpTurnContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInvokeError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInvokeResponse {
    pub invoke_id: String,
    pub server_id: String,
    pub tool_name: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<McpInvokeError>,
    pub duration_ms: u64,
    pub effect_class: McpEffectClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSummary {
    pub server_id: String,
    pub title: String,
    pub enabled: bool,
    pub connected: bool,
    pub tool_count: usize,
    pub allowed_lanes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServersResponse {
    pub servers: Vec<McpServerSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpGatewayHealthResponse {
    pub status: String,
    pub invokes_enabled: bool,
    pub registered_servers: usize,
    pub connected_servers: usize,
    pub catalog_entries: usize,
    pub now_utc: DateTime<Utc>,
}

/// Gateway → daemon policy check before invoke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPolicyEvaluateRequest {
    pub action: String,
    pub server_id: String,
    pub tool_name: String,
    pub effect_class: McpEffectClass,
    pub turn_context: McpTurnContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpPolicyDecision {
    Allow,
    Deny,
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPolicyEvaluateResponse {
    pub allowed: bool,
    pub decision: McpPolicyDecision,
    pub reason: String,
    #[serde(default)]
    pub approval_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAdminStatusResponse {
    pub invokes_enabled: bool,
    pub changed_at_utc: DateTime<Utc>,
    pub reason: Option<String>,
}
