//! Agent tools for capability intent resolution and environment feed bus.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use medousa_types::environment::SurfaceKind;
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::feed::{FeedEvent, FeedRef, FeedSource, is_valid_feed_id};
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::capability_catalog::CapabilityRegistry;
use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::feed_bus::{FeedPublishRequest, publish};
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{ToolId, medousa_tool};

pub const COGNITION_INTENT_RESOLVE: &str = "cognition_intent_resolve";
pub const COGNITION_FEED_SUBSCRIBE: &str = "cognition_feed_subscribe";
pub const COGNITION_FEED_PUBLISH: &str = "cognition_feed_publish";

const COGNITION_FEED_PUBLISH_ID: ToolId = ToolId::new(COGNITION_FEED_PUBLISH);

pub fn register_feed_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_tool(CognitionIntentResolveTool::new(capability_registry.clone()))?;
    registry.register_tool(CognitionFeedSubscribeTool::new(turn_scope))?;
    registry.register_typed_tool(CognitionFeedPublishTool)?;
    Ok(())
}

struct CognitionIntentResolveTool {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
}

impl CognitionIntentResolveTool {
    fn new(capability_registry: Arc<RwLock<CapabilityRegistry>>) -> Self {
        Self {
            capability_registry,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionIntentResolveTool {
    fn name(&self) -> &'static str {
        COGNITION_INTENT_RESOLVE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Resolve an operator intent or fuzzy query to capabilities with suggested feed ids and component templates.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "description": "Exact intent id, e.g. setup_dashboard or workshop_status"
                },
                "query": {
                    "type": "string",
                    "description": "Optional fuzzy query when intent id is unknown"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let intent = input
            .get("intent")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if intent.is_none() && query.is_none() {
            return Err(StasisError::PortFailure(
                "cognition_intent_resolve: intent or query is required".to_string(),
            ));
        }

        let registry = self.capability_registry.read().await;
        let response = registry.resolve_intent(intent, query);
        serde_json::to_value(response).map_err(|error| {
            StasisError::PortFailure(format!(
                "cognition_intent_resolve: failed to encode response: {error}"
            ))
        })
    }
}

struct CognitionFeedSubscribeTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionFeedSubscribeTool {
    fn new(turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>) -> Self {
        Self { turn_scope }
    }
}

#[async_trait]
impl StasisTool for CognitionFeedSubscribeTool {
    fn name(&self) -> &'static str {
        COGNITION_FEED_SUBSCRIBE
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Bind feed ids on a canvas component so runtime publishers can deliver component_patch updates.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["component_id", "feed_ids"],
            "properties": {
                "component_id": { "type": "string" },
                "feed_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Feed ids such as workshop.pulse"
                },
                "profile_id": { "type": "string" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let profile_id = profile_from_input(&input);
        let component_id = input
            .get("component_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("component_id required".to_string()))?;
        let feed_ids = input
            .get("feed_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| StasisError::PortFailure("feed_ids array required".to_string()))?
            .iter()
            .filter_map(|value| value.as_str().map(str::trim).filter(|id| !id.is_empty()))
            .map(str::to_string)
            .collect::<Vec<_>>();

        if feed_ids.is_empty() {
            return Ok(json!({
                "ok": false,
                "errors": ["feed_ids must contain at least one feed id"]
            }));
        }
        for feed_id in &feed_ids {
            if !is_valid_feed_id(feed_id) {
                return Ok(json!({
                    "ok": false,
                    "errors": [format!("invalid feed id '{feed_id}'")]
                }));
            }
        }

        let mut record = environment_hub()
            .get(&profile_id)
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let Some(index) = record
            .spec
            .components
            .iter()
            .position(|component| component.id == component_id)
        else {
            return Ok(json!({
                "ok": false,
                "errors": [format!("component not found: {component_id}")]
            }));
        };

        let surface_id = record.spec.components[index].surface_id.clone();
        let surface = record
            .spec
            .surfaces
            .iter()
            .find(|surface| surface.id == surface_id);
        if surface.map(|surface| &surface.kind) != Some(&SurfaceKind::Custom) {
            return Ok(json!({
                "ok": false,
                "errors": [format!(
                    "component '{component_id}' must live on a custom surface to subscribe feeds"
                )]
            }));
        }

        let previous = record.spec.components[index].feeds.clone();
        record.spec.components[index].feeds = feed_ids.clone();
        record.spec.components[index].updated_at = Some(Utc::now());
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.components[index].feeds = previous;
            return Ok(json!({ "ok": false, "errors": errors }));
        }

        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let nav_visible =
            crate::custom_view_status::surface_nav_visible(&updated.spec, &surface_id);
        let _ = self.turn_scope.read().await;
        Ok(json!({
            "ok": true,
            "revision": updated.revision,
            "component_id": component_id,
            "feed_ids": feed_ids,
            "live": true,
            "nav_visible": nav_visible,
            "feeds_subscribed": feed_ids,
        }))
    }
}

struct CognitionFeedPublishTool;

#[derive(Debug, Default)]
struct CompatibleFeedRefs(Vec<FeedRef>);

impl CompatibleFeedRefs {
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn deserialize_compatible_feed_refs<'de, D>(deserializer: D) -> Result<CompatibleFeedRefs, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    let refs = value
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let ref_type = entry.get("ref_type")?.as_str()?.trim();
                    let ref_id = entry.get("ref_id")?.as_str()?.trim();
                    if ref_type.is_empty() || ref_id.is_empty() {
                        return None;
                    }
                    Some(FeedRef {
                        ref_type: ref_type.to_string(),
                        ref_id: ref_id.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(CompatibleFeedRefs(refs))
}

#[derive(Debug, Default)]
enum CompatiblePayloadSlice {
    #[default]
    Missing,
    Value(Value),
}

impl CompatiblePayloadSlice {
    fn into_option(self) -> Option<Value> {
        match self {
            Self::Missing => None,
            Self::Value(value) => Some(value),
        }
    }
}

impl<'de> Deserialize<'de> for CompatiblePayloadSlice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Value::deserialize(deserializer).map(Self::Value)
    }
}

impl JsonSchema for CompatiblePayloadSlice {
    fn schema_name() -> String {
        "CompatiblePayloadSlice".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::Object.into()),
            ..SchemaObject::default()
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FeedPublishInput {
    feed_id: String,
    summary: String,
    #[serde(default, deserialize_with = "deserialize_compatible_feed_refs")]
    #[schemars(
        with = "Vec<FeedRef>",
        skip_serializing_if = "CompatibleFeedRefs::is_empty"
    )]
    refs: CompatibleFeedRefs,
    /// Optional bounded UI slice (max 2 KB JSON)
    #[serde(default)]
    #[schemars(skip_serializing_if = "CompatiblePayloadSlice::is_missing")]
    payload_slice: CompatiblePayloadSlice,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    profile_id: Option<String>,
}

impl CompatiblePayloadSlice {
    #[allow(dead_code)]
    fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct FeedPublishOutput {
    ok: bool,
    event: FeedEvent,
}

#[medousa_tool(id = COGNITION_FEED_PUBLISH_ID)]
impl CognitionFeedPublishTool {
    /// Publish a bounded feed event for subscribed environment components. Prefer internal publishers for workshop pulse.
    async fn invoke_typed(
        &self,
        input: FeedPublishInput,
    ) -> stasis::prelude::Result<FeedPublishOutput> {
        let profile_id = resolve_profile_id(
            input
                .profile_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        );

        let event = publish(FeedPublishRequest {
            profile_id: Some(profile_id),
            feed_id: input.feed_id,
            source: FeedSource::Agent,
            summary: input.summary,
            refs: input.refs.0,
            payload_slice: input.payload_slice.into_option(),
            payload_max_bytes: None,
        })
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        Ok(FeedPublishOutput { ok: true, event })
    }
}

fn profile_from_input(input: &Value) -> String {
    resolve_profile_id(
        input
            .get("profile_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )
}
