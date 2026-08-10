//! Agent tools for capability intent resolution and environment feed bus.

use std::sync::Arc;

use chrono::Utc;
use medousa_types::environment::SurfaceKind;
use medousa_types::environment_validate::validate_environment_spec;
use medousa_types::feed::{
    FeedEvent, FeedRef, FeedSource, IntentResolveResponse, is_valid_feed_id,
};
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::capability_catalog::CapabilityRegistry;
use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::feed_bus::{FeedPublishRequest, publish};
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{CompatList, CompatOption, ToolId, medousa_tool};

pub const COGNITION_INTENT_RESOLVE: &str = "cognition_intent_resolve";
pub const COGNITION_FEED_SUBSCRIBE: &str = "cognition_feed_subscribe";
pub const COGNITION_FEED_PUBLISH: &str = "cognition_feed_publish";

const COGNITION_FEED_PUBLISH_ID: ToolId = ToolId::new(COGNITION_FEED_PUBLISH);
const COGNITION_INTENT_RESOLVE_ID: ToolId = ToolId::new(COGNITION_INTENT_RESOLVE);
const COGNITION_FEED_SUBSCRIBE_ID: ToolId = ToolId::new(COGNITION_FEED_SUBSCRIBE);

pub fn register_feed_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionIntentResolveTool::new(capability_registry.clone()))?;
    registry.register_typed_tool(CognitionFeedSubscribeTool::new(turn_scope))?;
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

#[derive(Debug, Deserialize, JsonSchema)]
struct IntentResolveInput {
    /// Exact intent id, e.g. setup_dashboard or workshop_status
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    intent: CompatOption<String>,
    /// Optional fuzzy query when intent id is unknown
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    query: CompatOption<String>,
}

#[derive(Debug)]
struct IntentResolveCommand {
    intent: Option<TrimmedText>,
    query: Option<TrimmedText>,
}

impl TryFrom<IntentResolveInput> for IntentResolveCommand {
    type Error = StasisError;

    fn try_from(input: IntentResolveInput) -> Result<Self, Self::Error> {
        let intent = input
            .intent
            .into_option()
            .and_then(|value| TrimmedText::new(value).ok());
        let query = input
            .query
            .into_option()
            .and_then(|value| TrimmedText::new(value).ok());
        if intent.is_none() && query.is_none() {
            return Err(StasisError::PortFailure(
                "cognition_intent_resolve: intent or query is required".to_string(),
            ));
        }
        Ok(Self { intent, query })
    }
}

#[medousa_tool(id = COGNITION_INTENT_RESOLVE_ID)]
impl CognitionIntentResolveTool {
    /// Resolve an operator intent or fuzzy query to capabilities with suggested feed ids and component templates.
    async fn invoke_typed(
        &self,
        input: IntentResolveInput,
    ) -> stasis::prelude::Result<IntentResolveResponse> {
        let command = IntentResolveCommand::try_from(input)?;
        let intent = command.intent.as_ref().map(TrimmedText::as_str);
        let query = command.query.as_ref().map(TrimmedText::as_str);

        let registry = self.capability_registry.read().await;
        Ok(registry.resolve_intent(intent, query))
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

#[derive(Debug, JsonSchema)]
struct FeedSubscribeInput {
    #[schemars(required, with = "String")]
    component_id: CompatOption<String>,
    /// Feed ids such as workshop.pulse
    #[schemars(required, with = "Vec<String>")]
    feed_ids: CompatList<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    profile_id: CompatOption<String>,
}

impl<'de> Deserialize<'de> for FeedSubscribeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            component_id: CompatOption<String>,
            #[serde(default)]
            feed_ids: CompatList<String>,
            #[serde(default)]
            profile_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            component_id: input.component_id,
            feed_ids: input.feed_ids,
            profile_id: input.profile_id,
        })
    }
}

#[derive(Debug)]
struct FeedSubscribeCommand {
    component_id: TrimmedText,
    feed_ids: Vec<TrimmedText>,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<FeedSubscribeInput> for FeedSubscribeCommand {
    type Error = StasisError;

    fn try_from(input: FeedSubscribeInput) -> Result<Self, Self::Error> {
        let component_id = input
            .component_id
            .into_option()
            .ok_or_else(|| StasisError::PortFailure("component_id required".to_string()))
            .and_then(|value| {
                TrimmedText::new(value)
                    .map_err(|_| StasisError::PortFailure("component_id required".to_string()))
            })?;
        let feed_ids = input
            .feed_ids
            .into_option()
            .ok_or_else(|| StasisError::PortFailure("feed_ids array required".to_string()))?
            .into_iter()
            .filter_map(|value| TrimmedText::new(value).ok())
            .collect();
        Ok(Self {
            component_id,
            feed_ids,
            profile_id: input
                .profile_id
                .into_option()
                .and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
enum FeedSubscribeOutput {
    Success {
        ok: bool,
        revision: u64,
        component_id: String,
        feed_ids: Vec<String>,
        live: bool,
        nav_visible: bool,
        feeds_subscribed: Vec<String>,
    },
    Error {
        ok: bool,
        errors: Vec<String>,
    },
}

#[medousa_tool(id = COGNITION_FEED_SUBSCRIBE_ID)]
impl CognitionFeedSubscribeTool {
    /// Bind feed ids on a canvas component so runtime publishers can deliver component_patch updates.
    async fn invoke_typed(
        &self,
        input: FeedSubscribeInput,
    ) -> stasis::prelude::Result<FeedSubscribeOutput> {
        let command = FeedSubscribeCommand::try_from(input)?;
        let profile_id =
            profile_from_typed_input(command.profile_id.as_ref().map(TrimmedText::as_str));
        let component_id = command.component_id.as_str().to_string();
        let feed_ids = command
            .feed_ids
            .iter()
            .map(TrimmedText::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();

        if feed_ids.is_empty() {
            return Ok(FeedSubscribeOutput::Error {
                ok: false,
                errors: vec!["feed_ids must contain at least one feed id".to_string()],
            });
        }
        for feed_id in &feed_ids {
            if !is_valid_feed_id(feed_id) {
                return Ok(FeedSubscribeOutput::Error {
                    ok: false,
                    errors: vec![format!("invalid feed id '{feed_id}'")],
                });
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
            return Ok(FeedSubscribeOutput::Error {
                ok: false,
                errors: vec![format!("component not found: {component_id}")],
            });
        };

        let surface_id = record.spec.components[index].surface_id.clone();
        let surface = record
            .spec
            .surfaces
            .iter()
            .find(|surface| surface.id == surface_id);
        if surface.map(|surface| &surface.kind) != Some(&SurfaceKind::Custom) {
            return Ok(FeedSubscribeOutput::Error {
                ok: false,
                errors: vec![format!(
                    "component '{component_id}' must live on a custom surface to subscribe feeds"
                )],
            });
        }

        let previous = record.spec.components[index].feeds.clone();
        record.spec.components[index].feeds = feed_ids.clone();
        record.spec.components[index].updated_at = Some(Utc::now());
        let errors = validate_environment_spec(&record.spec);
        if !errors.is_empty() {
            record.spec.components[index].feeds = previous;
            return Ok(FeedSubscribeOutput::Error { ok: false, errors });
        }

        let updated = environment_hub()
            .put(record.spec, "agent")
            .await
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        let nav_visible =
            crate::custom_view_status::surface_nav_visible(&updated.spec, &surface_id);
        let _ = self.turn_scope.read().await;
        Ok(FeedSubscribeOutput::Success {
            ok: true,
            revision: updated.revision,
            component_id: component_id.to_string(),
            feed_ids: feed_ids.clone(),
            live: true,
            nav_visible,
            feeds_subscribed: feed_ids,
        })
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

#[derive(Debug)]
struct FeedPublishCommand {
    feed_id: TrimmedText,
    summary: RequiredContent,
    refs: Vec<FeedRef>,
    payload_slice: Option<Value>,
    profile_id: Option<TrimmedText>,
}

impl TryFrom<FeedPublishInput> for FeedPublishCommand {
    type Error = StasisError;

    fn try_from(input: FeedPublishInput) -> Result<Self, Self::Error> {
        let feed_id = TrimmedText::new(input.feed_id)
            .map_err(|_| StasisError::PortFailure("feed_id is required".to_string()))?;
        let summary = RequiredContent::new(input.summary)
            .map_err(|_| StasisError::PortFailure("summary is required".to_string()))?;
        Ok(Self {
            feed_id,
            summary,
            refs: input.refs.0,
            payload_slice: input.payload_slice.into_option(),
            profile_id: input
                .profile_id
                .into_option()
                .and_then(|value| TrimmedText::new(value).ok()),
        })
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    profile_id: CompatOption<String>,
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
        let command = FeedPublishCommand::try_from(input)?;
        let profile_id = resolve_profile_id(command.profile_id.as_ref().map(TrimmedText::as_str));

        let event = publish(FeedPublishRequest {
            profile_id: Some(profile_id),
            feed_id: command.feed_id.into_string(),
            source: FeedSource::Agent,
            summary: command.summary.into_string(),
            refs: command.refs,
            payload_slice: command.payload_slice,
            payload_max_bytes: None,
        })
        .await
        .map_err(|err| StasisError::PortFailure(err.to_string()))?;

        Ok(FeedPublishOutput { ok: true, event })
    }
}

fn profile_from_typed_input(profile_id: Option<&str>) -> String {
    resolve_profile_id(profile_id.map(str::trim).filter(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_commands_normalize_identifiers_and_preserve_summary_content() {
        let intent = IntentResolveCommand::try_from(IntentResolveInput {
            intent: Some(" setup_dashboard ".to_string()).into(),
            query: None.into(),
        })
        .expect("intent command");
        assert_eq!(
            intent.intent.as_ref().map(TrimmedText::as_str),
            Some("setup_dashboard")
        );

        let subscribe = FeedSubscribeCommand::try_from(FeedSubscribeInput {
            component_id: Some(" component-a ".to_string()).into(),
            feed_ids: Some(vec![
                " workshop.pulse ".to_string(),
                " \n".to_string(),
                "trip.london.trains".to_string(),
            ])
            .into(),
            profile_id: Some(" profile-a ".to_string()).into(),
        })
        .expect("subscribe command");
        assert_eq!(subscribe.component_id.as_str(), "component-a");
        assert_eq!(subscribe.feed_ids.len(), 2);
        assert_eq!(subscribe.feed_ids[0].as_str(), "workshop.pulse");

        let publish = FeedPublishCommand::try_from(FeedPublishInput {
            feed_id: " workshop.pulse ".to_string(),
            summary: "  Worker ready  \n".to_string(),
            refs: CompatibleFeedRefs(vec![FeedRef {
                ref_type: "work".to_string(),
                ref_id: "work-1".to_string(),
            }]),
            payload_slice: CompatiblePayloadSlice::Value(serde_json::json!({"phase": "done"})),
            profile_id: Some(" profile-a ".to_string()).into(),
        })
        .expect("publish command");
        assert_eq!(publish.feed_id.as_str(), "workshop.pulse");
        assert_eq!(publish.summary.as_str(), "  Worker ready  \n");
        assert_eq!(publish.refs.len(), 1);
        assert!(publish.payload_slice.is_some());
    }

    #[test]
    fn feed_commands_reject_missing_intent_and_blank_summary() {
        let intent_error = IntentResolveCommand::try_from(IntentResolveInput {
            intent: Some(" \n".to_string()).into(),
            query: None.into(),
        })
        .expect_err("missing intent should fail");
        assert!(
            intent_error
                .to_string()
                .contains("intent or query is required")
        );

        let publish_error = FeedPublishCommand::try_from(FeedPublishInput {
            feed_id: "workshop.pulse".to_string(),
            summary: " \n\t".to_string(),
            refs: CompatibleFeedRefs::default(),
            payload_slice: CompatiblePayloadSlice::Missing,
            profile_id: None.into(),
        })
        .expect_err("blank summary should fail");
        assert!(publish_error.to_string().contains("summary is required"));
    }

    #[test]
    fn feed_wire_optionals_remain_lenient_for_legacy_values() {
        let intent: IntentResolveInput = serde_json::from_value(serde_json::json!({
            "intent": 42,
            "query": false,
        }))
        .expect("intent input");
        assert!(intent.intent.into_option().is_none());
        assert!(intent.query.into_option().is_none());

        let subscribe: FeedSubscribeInput = serde_json::from_value(serde_json::json!({
            "component_id": 9,
            "feed_ids": ["workshop.pulse", 2, null],
            "profile_id": [],
        }))
        .expect("subscribe input");
        assert!(subscribe.component_id.into_option().is_none());
        assert_eq!(
            subscribe.feed_ids.into_option(),
            Some(vec!["workshop.pulse".to_string()])
        );
        assert!(subscribe.profile_id.into_option().is_none());

        let publish: FeedPublishInput = serde_json::from_value(serde_json::json!({
            "feed_id": "workshop.pulse",
            "summary": "ready",
            "profile_id": 42,
        }))
        .expect("publish input");
        assert!(publish.profile_id.into_option().is_none());
    }
}
