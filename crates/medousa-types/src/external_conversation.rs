//! Wire types for conversations with provider-hosted agents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExternalProvider {
    Muse,
    GrokBot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExternalEventKind {
    UserMessage,
    TransportPending,
    TransportAccepted,
    TransportFailed,
    TransportUncertain,
    ProviderMessage,
    Progress,
    Question,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalConversationEvent {
    pub sequence: u64,
    pub event_id: String,
    pub request_id: Option<String>,
    pub kind: ExternalEventKind,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalConversationView {
    pub id: String,
    pub provider: ExternalProvider,
    pub label: String,
    pub target: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub events: Vec<ExternalConversationEvent>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateExternalConversationRequest {
    pub provider: ExternalProvider,
    pub label: String,
    pub target: String,
    pub webhook_url: Option<String>,
    pub webhook_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreateExternalConversationResponse {
    pub conversation: ExternalConversationView,
    pub callback_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalConversationListResponse {
    pub conversations: Vec<ExternalConversationView>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalConversationSendRequest {
    pub request_id: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalProviderEventRequest {
    pub event_id: String,
    pub request_id: String,
    pub kind: ExternalEventKind,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalWhatsAppInboundRequest {
    pub chat_jid: String,
    pub sender_jid: String,
    pub message_id: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalInboundClaimResponse {
    pub claimed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalMuseDiscoveryStatus {
    pub challenge: String,
    pub observed_chat_jid: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct RotateExternalCallbackResponse {
    pub callback_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DeleteExternalConversationResponse {
    pub deleted: bool,
}
