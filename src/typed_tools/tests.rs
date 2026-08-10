use std::sync::atomic::{AtomicUsize, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::orchestration::tool_registry::{StasisTool, ToolRegistry};

use super::{
    EmptyCallMetadata, ExternalJson, ModeToolAdapter, ModeToolAdapterError, OpaqueToolPayload,
    RegisteredToolKind, SchemaNormalizationError, ToolEffect, ToolExposureRef, ToolId, ToolModeId,
    ToolPlacementIndex, ToolRegistrar, ToolRegistration, ToolSurfaceId, TypedTool,
    normalize_input_schema, normalize_output_schema,
};
use crate::typed_tools::medousa_tool;

const STATEFUL_ECHO_ID: ToolId = ToolId::new("typed_stateful_echo");
const LEGACY_ECHO_ID: &str = "typed_legacy_echo";

#[derive(Debug, Deserialize, JsonSchema)]
struct EchoInput {
    /// Text to prefix with constructor-owned state.
    value: String,
    /// Nested input used to prove local schema references are normalized.
    options: EchoOptions,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EchoOptions {
    /// Whether to uppercase the typed value.
    uppercase: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct EchoOutput {
    value: String,
    invocation: usize,
}

struct StatefulEchoTool {
    prefix: String,
    invocations: AtomicUsize,
}

impl StatefulEchoTool {
    fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            invocations: AtomicUsize::new(0),
        }
    }
}

#[medousa_tool(id = STATEFUL_ECHO_ID)]
impl StatefulEchoTool {
    /// Echo typed input through constructor-owned state.
    async fn invoke_typed(&self, input: EchoInput) -> stasis::prelude::Result<EchoOutput> {
        let value = if input.options.uppercase {
            input.value.to_uppercase()
        } else {
            input.value
        };
        let invocation = self.invocations.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(EchoOutput {
            value: format!("{}{}", self.prefix, value),
            invocation,
        })
    }
}

struct LegacyEchoTool;

#[medousa_tool(id = LEGACY_ECHO_ID)]
impl LegacyEchoTool {
    /// Prove compatibility with static string name constants during migration.
    async fn invoke_typed(&self, input: EchoInput) -> stasis::prelude::Result<EchoOutput> {
        Ok(EchoOutput {
            value: input.value,
            invocation: 1,
        })
    }
}

#[tokio::test]
async fn stateful_tool_stays_typed_until_the_stasis_boundary() {
    let tool = StatefulEchoTool::new("state:");

    let direct = tool
        .invoke_typed(EchoInput {
            value: "first".to_string(),
            options: EchoOptions { uppercase: true },
        })
        .await
        .expect("typed invocation should succeed");
    assert_eq!(direct.value, "state:FIRST");
    assert_eq!(direct.invocation, 1);

    let boundary = StasisTool::invoke(
        &tool,
        json!({
            "value": "second",
            "options": { "uppercase": false }
        }),
    )
    .await
    .expect("Stasis invocation should succeed");
    assert_eq!(boundary["value"], "state:second");
    assert_eq!(boundary["invocation"], 2);
    assert_eq!(tool.invocations.load(Ordering::SeqCst), 2);
}

#[test]
fn macro_projects_id_description_and_normalized_schemas() {
    let tool = StatefulEchoTool::new("unused:");
    assert_eq!(StasisTool::name(&tool), STATEFUL_ECHO_ID.as_str());
    assert_eq!(
        StasisTool::description(&tool),
        Some("Echo typed input through constructor-owned state.")
    );

    let contract = <StatefulEchoTool as TypedTool>::contract();
    assert_eq!(contract.id, STATEFUL_ECHO_ID);
    assert_eq!(
        contract.description,
        StasisTool::description(&tool).unwrap()
    );
    assert!(std::ptr::eq(
        contract,
        <StatefulEchoTool as TypedTool>::contract()
    ));

    let input_schema = StasisTool::input_schema(&tool).expect("input schema");
    assert_eq!(input_schema["type"], "object");
    assert_eq!(input_schema["properties"]["options"]["type"], "object");
    assert_eq!(
        input_schema["properties"]["options"]["properties"]["uppercase"]["description"],
        "Whether to uppercase the typed value."
    );
    assert!(!input_schema.to_string().contains("$ref"));
    assert!(!input_schema.to_string().contains("definitions"));

    let output_schema = StasisTool::output_schema(&tool).expect("output schema");
    assert_eq!(output_schema["type"], "object");
    assert!(!output_schema.to_string().contains("$ref"));
}

fn input_contract_example() -> serde_json::Value {
    json!({ "value": "example", "options": { "uppercase": false } })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(example = "input_contract_example")]
#[allow(dead_code)]
struct ExampleInputContract {
    value: String,
    options: EchoOptions,
}

#[test]
fn schema_normalizer_preserves_provider_singular_example_shape() {
    let schema = normalize_input_schema::<ExampleInputContract>().expect("example schema");
    assert_eq!(schema["example"], input_contract_example());
    assert!(schema.get("examples").is_none());
}

#[tokio::test]
async fn boundary_errors_include_the_typed_tool_id() {
    let error = StasisTool::invoke(&StatefulEchoTool::new("unused:"), json!({"value": 42}))
        .await
        .expect_err("invalid input should fail");
    let message = error.to_string();
    assert!(message.contains(STATEFUL_ECHO_ID.as_str()));
    assert!(message.contains("invalid input for typed tool"));
}

#[test]
fn legacy_static_name_constants_resolve_to_typed_ids() {
    assert_eq!(
        <LegacyEchoTool as TypedTool>::tool_id(),
        ToolId::new(LEGACY_ECHO_ID)
    );
    assert_eq!(StasisTool::name(&LegacyEchoTool), LEGACY_ECHO_ID);
}

#[test]
fn tool_ids_reject_wire_unsafe_names() {
    let error = ToolId::try_new("not a tool").expect_err("spaces are invalid");
    assert_eq!(error.value(), "not a tool");
    assert!(error.to_string().contains("ASCII"));
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct ExternalEnvelope {
    external: ExternalJson,
    payload: OpaqueToolPayload,
}

#[test]
fn opaque_external_types_are_explicit_and_schema_permissive() {
    let external = ExternalJson::new(json!({"vendor": [1, 2, 3]}));
    assert_eq!(external.as_value()["vendor"][1], 2);
    assert_eq!(external.clone().into_value()["vendor"][2], 3);

    let payload = OpaqueToolPayload::new(json!({"runtime_shape": true}));
    assert_eq!(payload.as_value()["runtime_shape"], true);
    assert_eq!(payload.clone().into_value()["runtime_shape"], true);

    let schema = normalize_output_schema::<ExternalEnvelope>().expect("opaque schema");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["external"], true);
    assert_eq!(schema["properties"]["payload"], true);
}

#[test]
fn model_input_contracts_must_be_objects() {
    assert_eq!(
        normalize_input_schema::<String>(),
        Err(SchemaNormalizationError::InputMustBeObject)
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReviewCallMetadata {
    /// Short review outcome this call should establish.
    rationale: String,
}

#[tokio::test]
async fn registrar_catalog_is_the_actual_typed_and_legacy_registration_authority() {
    let review_mode = ToolModeId::new("review");
    let selected_surface = ToolSurfaceId::new("selected");
    let selected = ToolExposureRef::new(review_mode, selected_surface);
    let mut placements = ToolPlacementIndex::default();
    placements.add_exposure(STATEFUL_ECHO_ID, selected);
    placements.set_effect(STATEFUL_ECHO_ID, ToolEffect::Observe);
    placements.set_presentation_summary(ToolId::new(LEGACY_ECHO_ID), "Legacy echo override");

    let mut registrar = ToolRegistrar::new(placements);
    registrar
        .register_typed_tool(StatefulEchoTool::new("catalog:"))
        .expect("register typed tool");
    registrar
        .register_tool(LegacyEchoTool)
        .expect("register legacy compatibility tool");
    let (registry, catalog) = registrar.finish();

    let registered = registry.list_tools().await.expect("list registered tools");
    let catalog_names = catalog
        .entries()
        .map(|entry| entry.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let registered_names = registered
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(catalog_names, registered_names);
    assert_eq!(
        catalog
            .get(STATEFUL_ECHO_ID)
            .expect("typed entry")
            .contract
            .kind,
        RegisteredToolKind::Typed
    );
    assert_eq!(
        catalog
            .get(ToolId::new(LEGACY_ECHO_ID))
            .expect("legacy entry")
            .contract
            .kind,
        RegisteredToolKind::Legacy
    );
    assert_eq!(
        catalog
            .get(STATEFUL_ECHO_ID)
            .expect("typed placement")
            .placement
            .effect,
        ToolEffect::Observe
    );
    assert_eq!(
        catalog.presentation_summary(STATEFUL_ECHO_ID),
        "Echo typed input through constructor-owned state."
    );
    assert_eq!(
        catalog.presentation_summary(ToolId::new(LEGACY_ECHO_ID)),
        "Legacy echo override"
    );
}

#[test]
fn future_mode_adds_typed_metadata_and_selects_a_subset_without_cloning_base_schema() {
    let mode = ToolModeId::new("review");
    let selected = ToolExposureRef::new(mode, ToolSurfaceId::new("selected"));
    let mut placements = ToolPlacementIndex::default();
    placements.add_exposure(STATEFUL_ECHO_ID, selected);
    let mut registrar = ToolRegistrar::new(placements);
    registrar
        .register_typed_tool(StatefulEchoTool::new("review:"))
        .expect("register selected tool");
    registrar
        .register_typed_tool(LegacyEchoTool)
        .expect("register unselected tool");
    let (_registry, catalog) = registrar.finish();

    let adapter = ModeToolAdapter::<ReviewCallMetadata>::new(mode).expect("review adapter");
    let mut surface = adapter
        .compile_surface(&catalog, |entry| entry.placement.exposes(selected))
        .expect("compile selected review surface");
    assert_eq!(surface.len(), 1);
    assert_eq!(surface[0].name.as_str(), STATEFUL_ECHO_ID.as_str());
    let projected = surface.pop().expect("selected tool");
    let schema = projected.schema.expect("projected schema");
    assert!(schema["properties"].get("rationale").is_some());
    assert!(
        <StatefulEchoTool as TypedTool>::contract().input_schema["properties"]
            .get("rationale")
            .is_none()
    );

    let (metadata, input) = adapter
        .split_call(json!({
            "rationale": "Confirm the typed seam",
            "value": "hello",
            "options": { "uppercase": false }
        }))
        .expect("split review call");
    assert_eq!(metadata.rationale, "Confirm the typed seam");
    assert!(input.get("rationale").is_none());
    assert_eq!(input["value"], "hello");

    let general = ModeToolAdapter::<EmptyCallMetadata>::new(ToolModeId::new("general"))
        .expect("General adapter");
    let general_tool = general
        .compose_tool(
            genai::chat::Tool::new(STATEFUL_ECHO_ID.as_str()).with_schema(
                <StatefulEchoTool as TypedTool>::contract()
                    .input_schema
                    .clone(),
            ),
        )
        .expect("empty metadata leaves base unchanged");
    assert_eq!(
        general_tool.schema,
        Some(
            <StatefulEchoTool as TypedTool>::contract()
                .input_schema
                .clone()
        )
    );
}

#[test]
fn mode_schema_composition_rejects_undeclared_reserved_field_collisions() {
    let adapter = ModeToolAdapter::<ReviewCallMetadata>::new(ToolModeId::new("review"))
        .expect("review adapter");
    let colliding = genai::chat::Tool::new("colliding_tool").with_schema(json!({
        "type": "object",
        "properties": {
            "rationale": { "type": "string" }
        }
    }));
    let error = adapter
        .compose_tool(colliding.clone())
        .expect_err("base input must not claim mode metadata");
    assert_eq!(
        error,
        ModeToolAdapterError::ReservedFieldCollision {
            id: "colliding_tool".to_string(),
            field: "rationale".to_string(),
        }
    );

    let projected = adapter
        .compose_tool_with_projection(
            colliding,
            &super::ModeInputProjection::replacing(["rationale"]),
        )
        .expect("an explicit mode projection may replace the colliding wire field");
    assert_eq!(
        projected.schema.expect("projected schema")["properties"]["rationale"]["description"],
        "Short review outcome this call should establish."
    );
}
