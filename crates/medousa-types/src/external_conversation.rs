//! Wire types for conversations with provider-hosted agents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::message_effect::MessageReaction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExternalProvider {
    Muse,
    GrokBot,
    Instinct,
    Dots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExternalAgentScope {
    Read,
    Work,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalAgentAccessStatus {
    pub scopes: Vec<ExternalAgentScope>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateExternalAgentTokenRequest {
    pub scopes: Vec<ExternalAgentScope>,
    pub expires_in_days: u32,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct CreateExternalAgentTokenResponse {
    pub token: String,
    pub access: ExternalAgentAccessStatus,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<MessageReaction>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalConversationView {
    pub id: String,
    pub provider: ExternalProvider,
    pub label: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub events: Vec<ExternalConversationEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_access: Option<ExternalAgentAccessStatus>,
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
    #[serde(default)]
    pub slack_user_token: Option<String>,
    #[serde(default)]
    pub dot_user_id: Option<String>,
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
    /// Optional exact work association; ordinary messages retain their behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<crate::work_provider::WorkProviderRequestInput>,
    /// Native operator admission for one review send after the exact executor
    /// completes. Requires `work.review_of`; it does not launch the executor.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub after_native_completion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalProviderEventRequest {
    pub event_id: String,
    pub request_id: String,
    pub kind: ExternalEventKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<MessageReaction>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalWhatsAppInboundRequest {
    pub chat_jid: String,
    pub sender_jid: String,
    pub message_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<MessageReaction>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalSlackInboundRequest {
    pub channel_id: String,
    pub sender_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExternalWhatsAppPairingState {
    Waiting,
    QrReady,
    Connected,
    LoggedOut,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExternalWhatsAppPairingUpdateRequest {
    pub state: ExternalWhatsAppPairingState,
    pub qr_code: Option<String>,
    pub expires_in_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExternalWhatsAppPairingStatus {
    pub state: ExternalWhatsAppPairingState,
    pub qr_svg: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
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
