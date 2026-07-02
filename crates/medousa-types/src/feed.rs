//! Environment component feed bus types (Phase 2).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_FEED_PAYLOAD_MAX_BYTES: usize = 2048;
pub const WORKSHOP_PULSE_FEED_ID: &str = "workshop.pulse";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum FeedSource {
    BoundWorkshop,
    RecurringJob,
    StasisJob,
    CapabilityInvoke,
    WorkspaceBridge,
    Agent,
}

impl FeedSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BoundWorkshop => "bound_workshop",
            Self::RecurringJob => "recurring_job",
            Self::StasisJob => "stasis_job",
            Self::CapabilityInvoke => "capability_invoke",
            Self::WorkspaceBridge => "workspace_bridge",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct FeedRef {
    pub ref_type: String,
    pub ref_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedEvent {
    pub id: String,
    pub feed_id: String,
    pub emitted_at_utc: DateTime<Utc>,
    pub source: String,
    pub summary: String,
    #[serde(default)]
    pub refs: Vec<FeedRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComponentFeedPatch {
    pub component_id: String,
    pub feed_id: String,
    pub patch: Value,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedListEntry {
    pub feed_id: String,
    pub event_count: u64,
    pub subscriber_component_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedListResponse {
    pub feeds: Vec<FeedListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedTailQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedTailResponse {
    pub feed_id: String,
    pub events: Vec<FeedEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedStreamQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedStreamEvent {
    pub seq: u64,
    pub event_type: String,
    pub emitted_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_event: Option<FeedEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_patches: Option<Vec<ComponentFeedPatch>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct FeedReadRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComponentTemplateHint {
    pub surface_id: String,
    pub slot: String,
    #[serde(rename = "type")]
    pub component_type: String,
    #[serde(default)]
    pub feeds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CapabilityIntentEntry {
    pub intent: String,
    pub capability_id: String,
    pub title: String,
    #[serde(default)]
    pub publish_feeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_feed_component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_template: Option<ComponentTemplateHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CapabilityIntentsResponse {
    pub intents: Vec<CapabilityIntentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct IntentResolveMatch {
    pub capability_id: String,
    pub title: String,
    #[serde(default)]
    pub publish_feeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_feed_component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_template: Option<ComponentTemplateHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct IntentResolveResponse {
    pub query: String,
    pub matches: Vec<IntentResolveMatch>,
}

pub fn is_valid_feed_id(feed_id: &str) -> bool {
    let feed_id = feed_id.trim();
    if feed_id.is_empty() {
        return false;
    }
    let mut chars = feed_id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    feed_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}
