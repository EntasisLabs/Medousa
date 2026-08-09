use std::sync::atomic::{AtomicUsize, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::orchestration::tool_registry::StasisTool;

use super::{
    ExternalJson, OpaqueToolPayload, SchemaNormalizationError, ToolId, TypedTool,
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
