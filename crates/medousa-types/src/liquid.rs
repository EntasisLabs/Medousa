use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the client expects the daemon to do with a Liquid scene interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LiquidEventDisposition {
    LocalState,
    ContextOnly,
    SubmitTurn,
    Navigation,
    PrivilegedAction,
}

/// Bounded, inert interaction context associated with the message that emitted it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LiquidInteractionEnvelope {
    pub version: u8,
    pub session_id: String,
    pub message_id: String,
    pub node_id: String,
    pub instance_id: String,
    pub event_type: String,
    pub disposition: LiquidEventDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    pub occurred_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_state_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LiquidComponentStateRecord {
    pub schema_version: u8,
    pub session_id: String,
    pub message_id: String,
    pub node_id: String,
    pub instance_id: String,
    pub component_type: String,
    pub revision: u64,
    pub state: Value,
    pub updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LiquidComponentStateResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<LiquidComponentStateRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PutLiquidComponentStateRequest {
    pub schema_version: u8,
    pub component_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    pub state: Value,
}
