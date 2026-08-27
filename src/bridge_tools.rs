//! Phase D3 bridge tools — capability invoke, MCP promote, grapheme templates.
//!
//! Design: docs/internal/runtime-tools-roadmap.md

use std::sync::Arc;

use chrono::Utc;
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::prelude::StasisError;
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::capability_catalog::{
    CapabilityBinding, CapabilityRegistry, CapabilityResolveResponse, CapabilitySource,
};
use crate::events::TuiEvent;
use crate::mcp_gateway_api::{McpInvokeRequest, McpTurnContext, McpTurnLane};
use crate::mcp_gateway_client::McpGatewayClient;
use crate::mcp_turn_token::mint_mcp_turn_token;
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::tools::{run_grapheme_via_runtime, validate_grapheme_source_for_schedule};
use crate::turn_continuation::{ContinuationAwaitMode, continuation_tool_metadata};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};
use crate::web_search_tool::{WebSearchBackend, WebSearchMode, WebSearchRequest};
use crate::workflow::{
    MedousaWorkflowPayload, WORKFLOW_SEQUENTIAL_JOB_TYPE, WorkflowEnqueueContinuation,
    WorkflowRecord, WorkflowRegistry, WorkflowStatus, WorkflowStepSpec, enqueue_workflow_job,
    new_workflow_id, workflow_job_type_for_strategy,
};

const COGNITION_CAPABILITY_INVOKE_ID: ToolId = ToolId::new("cognition_capability_invoke");
const COGNITION_MCP_PROMOTE_TO_JOB_ID: ToolId = ToolId::new("cognition_mcp_promote_to_job");
const COGNITION_GRAPHEME_TEMPLATE_RUN_ID: ToolId = ToolId::new("cognition_grapheme_template_run");

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct BridgeObject(Value);

impl BridgeObject {
    pub(crate) fn from_value(value: Value) -> Self {
        Self(value)
    }
}

impl JsonSchema for BridgeObject {
    fn schema_name() -> String {
        "BridgeObject".to_string()
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CapabilityBindingRef {
    source: String,
    reference: String,
}

fn escape_grapheme_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn binding_ref(binding: &CapabilityBinding) -> CapabilityBindingRef {
    CapabilityBindingRef {
        source: binding.source.as_str().to_string(),
        reference: binding.reference.clone(),
    }
}

fn fallback_bindings(bindings: &[CapabilityBinding]) -> Vec<CapabilityBindingRef> {
    bindings.iter().map(binding_ref).collect()
}

fn effect_class_for_capability(capability_id: &str) -> &'static str {
    match capability_id {
        "send_email" => "external_side_effect",
        "document_search" | "web_research" | "http_fetch" => "external_read",
        _ => "internal_read",
    }
}

fn resolve_capability_from_input(
    registry: &CapabilityRegistry,
    capability_id: Option<&str>,
    query: Option<&str>,
) -> stasis::prelude::Result<CapabilityResolveResponse> {
    if let Some(capability_id) = capability_id {
        return registry.resolve(capability_id).ok_or_else(|| {
            StasisError::PortFailure(format!(
                "cognition_capability_invoke: unknown capability '{capability_id}'"
            ))
        });
    }

    let query = query.ok_or_else(|| {
        StasisError::PortFailure(
            "cognition_capability_invoke: capability or query is required".to_string(),
        )
    })?;
    let search = registry.search(query, 1);
    let Some(first) = search.matches.first() else {
        return Err(StasisError::PortFailure(format!(
            "cognition_capability_invoke: no capabilities matched query '{query}'"
        )));
    };
    registry.resolve(&first.capability).ok_or_else(|| {
        StasisError::PortFailure(format!(
            "cognition_capability_invoke: matched capability '{}' but resolve failed",
            first.capability
        ))
    })
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySourceInput {
    Grapheme,
    Mcp,
}

impl CapabilitySourceInput {
    fn runtime(self) -> CapabilitySource {
        match self {
            Self::Grapheme => CapabilitySource::Grapheme,
            Self::Mcp => CapabilitySource::Mcp,
        }
    }
}

#[derive(Debug, Clone)]
struct CapabilityBindingRequest {
    source: CapabilitySource,
    reference: TrimmedText,
}

impl CapabilityBindingRequest {
    fn new(
        source: CapabilitySource,
        reference: impl Into<String>,
    ) -> stasis::prelude::Result<Self> {
        let reference = TrimmedText::new(reference).map_err(|_| {
            StasisError::PortFailure(
                "cognition_capability_invoke: binding.reference is required".to_string(),
            )
        })?;
        Ok(Self { source, reference })
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CapabilityBindingInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CapabilitySourceInput",
        skip_serializing_if = "Option::is_none"
    )]
    source: Option<CapabilitySourceInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
}

impl TryFrom<CapabilityBindingInput> for CapabilityBindingRequest {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: CapabilityBindingInput) -> Result<Self, Self::Error> {
        let source = input.source.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_capability_invoke: binding.source is required".to_string(),
            )
        })?;
        let reference = input.reference.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_capability_invoke: binding.reference is required".to_string(),
            )
        })?;
        Self::new(source.runtime(), reference)
    }
}

fn ordered_available_bindings(
    response: &CapabilityResolveResponse,
    preferred_source: Option<CapabilitySource>,
) -> Vec<CapabilityBinding> {
    let mut bindings = response
        .implementations
        .grapheme
        .iter()
        .chain(response.implementations.mcp.iter())
        .filter(|binding| binding.available)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(preferred) = preferred_source {
        bindings.retain(|binding| binding.source == preferred);
    }

    bindings.sort_by_key(|binding| binding.priority);
    bindings
}

fn select_binding_for_invoke(
    response: &CapabilityResolveResponse,
    preferred_source: Option<CapabilitySource>,
    explicit: Option<&CapabilityBindingRequest>,
) -> stasis::prelude::Result<(CapabilityBinding, Vec<CapabilityBinding>)> {
    if let Some(explicit) = explicit {
        let parsed_source = explicit.source;
        let reference = explicit.reference.as_str();
        let source = explicit.source.as_str();

        let mut available = ordered_available_bindings(response, preferred_source);
        let Some(primary) = available
            .iter()
            .find(|binding| binding.source == parsed_source && binding.reference == reference)
            .cloned()
        else {
            return Err(StasisError::PortFailure(format!(
                "cognition_capability_invoke: binding {source}.{reference} unavailable for capability '{}'",
                response.capability
            )));
        };
        available.retain(|binding| {
            binding.reference != primary.reference || binding.source != primary.source
        });
        return Ok((primary, available));
    }

    let mut available = ordered_available_bindings(response, preferred_source);
    let Some(primary) = available.first().cloned() else {
        return Err(StasisError::PortFailure(format!(
            "cognition_capability_invoke: no available bindings for capability '{}'",
            response.capability
        )));
    };
    available.remove(0);
    Ok((primary, available))
}

fn grapheme_source_for_web_provider_search(op: &str, escaped_query: &str) -> String {
    format!(
        r#"import core from "grapheme/core"
query CapabilityInvoke {{
  set {{ query: "{escaped_query}" }}
  |> web.{op}(query: $current.query) {{ results {{ title url snippet }} }}
}}"#
    )
}

pub fn grapheme_source_for_binding(
    binding: &CapabilityBinding,
    tool_input: &Value,
) -> stasis::prelude::Result<String> {
    if let Some(source) = tool_input.get("source").and_then(|value| value.as_str()) {
        let source = RequiredContent::new(source.to_string()).map_err(|_| {
            StasisError::PortFailure(
                "cognition_capability_invoke: input.source must be non-empty".to_string(),
            )
        })?;
        return Ok(source.into_string());
    }

    let query = tool_input
        .get("query")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_capability_invoke: grapheme binding requires input.query or input.source"
                    .to_string(),
            )
        })?;
    let escaped = escape_grapheme_literal(query);

    let source = match binding.reference.as_str() {
        "web.providers" => r#"import core from "grapheme/core"
query CapabilityInvoke {
  web.providers() { count providers { id } }
}"#
        .to_string(),
        "web.capabilities" => {
            let provider = tool_input
                .get("provider")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            match provider {
                Some(provider) => {
                    let escaped_provider = escape_grapheme_literal(provider);
                    format!(
                        r#"import core from "grapheme/core"
query CapabilityInvoke {{
  web.capabilities(provider: "{escaped_provider}") {{ available_providers provider }}
}}"#
                    )
                }
                None => r#"import core from "grapheme/core"
query CapabilityInvoke {
  web.capabilities() { available_providers provider }
}"#
                .to_string(),
            }
        }
        "web.duckduckgo" | "web.google" | "web.xaviv" => grapheme_source_for_web_provider_search(
            binding
                .reference
                .rsplit_once('.')
                .map(|(_, op)| op)
                .unwrap_or("duckduckgo"),
            &escaped,
        ),
        "websearch.research_materials" => format!(
            r#"import core from "grapheme/core"
query CapabilityInvoke {{
  set {{ topic: "{escaped}" }}
  |> websearch.research_materials(topic: $current.topic) {{ materials {{ title url snippet }} }}
}}"#
        ),
        "websearch.search" => format!(
            r#"import core from "grapheme/core"
query CapabilityInvoke {{
  set {{ query: "{escaped}" }}
  |> websearch.search(query: $current.query) {{ items {{ title url snippet }} }}
}}"#
        ),
        "websearch.research_report" => format!(
            r#"import core from "grapheme/core"
query CapabilityInvoke {{
  set {{ topic: "{escaped}" }}
  |> websearch.research_report(topic: $current.topic) {{ report {{ summary sources {{ title url }} }} }}
}}"#
        ),
        "docs.search" => format!(
            r#"import core from "grapheme/core"
query CapabilityInvoke {{
  set {{ query: "{escaped}" }}
  |> docs.search(query: $current.query) {{ hits {{ title path snippet }} }}
}}"#
        ),
        "http.fetch" => {
            let url = tool_input
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or(query);
            let escaped_url = escape_grapheme_literal(url);
            format!(
                r#"import core from "grapheme/core"
query CapabilityInvoke {{
  set {{ url: "{escaped_url}" }}
  |> http.get(url: $current.url)
  |> html.to_md(html: $current.body)
  |> core.echo(message: $current.text)
}}"#
            )
        }
        "smtp.send" => {
            return Err(StasisError::PortFailure(
                "cognition_capability_invoke: smtp.send requires explicit input.source".to_string(),
            ));
        }
        other if other.starts_with("web.") => grapheme_source_for_web_provider_search(
            other.strip_prefix("web.").unwrap_or("duckduckgo"),
            &escaped,
        ),
        other => {
            return Err(StasisError::PortFailure(format!(
                "cognition_capability_invoke: no auto grapheme source for binding '{other}'; provide input.source"
            )));
        }
    };

    Ok(source)
}

pub fn render_grapheme_template(template: &str, params: &Value) -> stasis::prelude::Result<String> {
    match template.trim().to_ascii_lowercase().as_str() {
        "research_report" => {
            let topic = params
                .get("topic")
                .or_else(|| params.get("query"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    StasisError::PortFailure(
                        "cognition_grapheme_template_run: research_report requires topic or query"
                            .to_string(),
                    )
                })?;
            Ok(format!(
                r#"import core from "grapheme/core"
query ResearchReport {{
  set {{ topic: "{}" }}
  |> websearch.research_report(topic: $current.topic) {{ report {{ summary sources {{ title url }} }} }}
}}"#,
                escape_grapheme_literal(topic)
            ))
        }
        "http_poll" => {
            let url = params.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_template_run: http_poll requires url".to_string(),
                )
            })?;
            Ok(format!(
                r#"import core from "grapheme/core"
query HttpPoll {{
  set {{ url: "{}" }}
  |> http.fetch(url: $current.url) {{ status body headers {{ name value }} }}
}}"#,
                escape_grapheme_literal(url)
            ))
        }
        "csv_digest" => {
            let url = params.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_grapheme_template_run: csv_digest requires url".to_string(),
                )
            })?;
            Ok(format!(
                r#"import core from "grapheme/core"
query CsvDigest {{
  set {{ url: "{}" }}
  |> http.fetch(url: $current.url) {{ status body }}
}}"#,
                escape_grapheme_literal(url)
            ))
        }
        other => Err(StasisError::PortFailure(format!(
            "cognition_grapheme_template_run: unknown template '{other}' (supported: research_report, http_poll, csv_digest)"
        ))),
    }
}

fn build_agent_mcp_turn_context(session_id: &str) -> McpTurnContext {
    McpTurnContext {
        turn_id: format!("cap-invoke-{}", Uuid::new_v4().simple()),
        session_id: session_id.to_string(),
        user_id: crate::identity_memory::resolve_identity_user_id(None),
        channel_id: crate::identity_memory::resolve_identity_channel_id(Some("interactive")),
        lane: McpTurnLane::Interactive,
        policy_profile: Some("interactive".to_string()),
    }
}

async fn invoke_mcp_binding(
    gateway_client: &McpGatewayClient,
    session_id: &str,
    binding: &CapabilityBinding,
    tool_input: &Value,
) -> stasis::prelude::Result<Value> {
    let server_id = binding.server_id.as_deref().ok_or_else(|| {
        StasisError::PortFailure(format!(
            "cognition_capability_invoke: MCP binding '{}' missing server_id",
            binding.reference
        ))
    })?;
    let tool_name = binding.tool_name.as_deref().ok_or_else(|| {
        StasisError::PortFailure(format!(
            "cognition_capability_invoke: MCP binding '{}' missing tool_name",
            binding.reference
        ))
    })?;

    let turn_context = build_agent_mcp_turn_context(session_id);
    let turn_token = mint_mcp_turn_token(&turn_context).map_err(|error| {
        StasisError::PortFailure(format!("cognition_capability_invoke: {error}"))
    })?;

    let request = McpInvokeRequest {
        server_id: server_id.to_string(),
        tool_name: tool_name.to_string(),
        input: tool_input
            .get("input")
            .cloned()
            .or_else(|| Some(tool_input.clone()))
            .unwrap_or_else(|| json!({})),
        turn_context,
        turn_token,
        operator_approval_granted: None,
    };

    let response = gateway_client.invoke(&request).await.map_err(|error| {
        StasisError::PortFailure(format!("cognition_capability_invoke: {error}"))
    })?;
    serde_json::to_value(response).map_err(|error| {
        StasisError::PortFailure(format!(
            "cognition_capability_invoke: failed to encode MCP response: {error}"
        ))
    })
}

async fn invoke_grapheme_binding(
    runtime: &Arc<RuntimeComposition>,
    binding: &CapabilityBinding,
    tool_input: &Value,
) -> stasis::prelude::Result<Value> {
    let source = grapheme_source_for_binding(binding, tool_input)?;
    let validation = validate_grapheme_source_for_schedule(runtime, &source).await?;
    if !validation
        .get("validated")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Ok(json!({
            "ok": false,
            "reason": "invalid_grapheme_source",
            "validation": validation
        }));
    }

    run_grapheme_via_runtime(runtime, &source, "cognition_capability_invoke").await
}

fn invoke_succeeded(binding: &CapabilityBinding, result: &Value) -> bool {
    match binding.source {
        CapabilitySource::Mcp => result
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        CapabilitySource::Grapheme => result
            .get("succeeded")
            .and_then(|value| value.as_bool())
            .or_else(|| result.get("ok").and_then(|value| value.as_bool()))
            .unwrap_or(false),
    }
}

fn effect_class_from_result(
    binding: &CapabilityBinding,
    result: &Value,
    capability_id: &str,
) -> String {
    if binding.source == CapabilitySource::Mcp {
        result
            .get("effect_class")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| effect_class_for_capability(capability_id).to_string())
    } else {
        effect_class_for_capability(capability_id).to_string()
    }
}

// ── cognition_capability_invoke ───────────────────────────────────────────────

pub struct CognitionCapabilityInvokeTool {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    runtime: Arc<RuntimeComposition>,
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCapabilityInvokeTool {
    pub fn new(
        capability_registry: Arc<RwLock<CapabilityRegistry>>,
        runtime: Arc<RuntimeComposition>,
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            capability_registry,
            runtime,
            gateway_client,
            session_id: session_id.into(),
            turn_scope,
            event_tx,
        }
    }
}

fn default_bridge_fallbacks() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CapabilityInvokeInput {
    /// Capability id, e.g. document_search
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) capability: Option<String>,
    /// Fuzzy resolve when capability id is unknown
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) query: Option<String>,
    /// Arguments forwarded to MCP or used to build Grapheme source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "BridgeObject", skip_serializing_if = "Option::is_none")]
    pub(crate) input: Option<BridgeObject>,
    /// Optional explicit Grapheme source override
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CapabilityBindingInput",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) binding: Option<CapabilityBindingInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "CapabilitySourceInput",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) preferred_source: Option<CapabilitySourceInput>,
    #[serde(default = "default_bridge_fallbacks")]
    #[schemars(default = "default_bridge_fallbacks")]
    pub(crate) try_fallbacks: bool,
    #[serde(flatten)]
    #[schemars(skip)]
    pub(crate) extra: serde_json::Map<String, Value>,
}

impl CapabilityInvokeInput {
    fn tool_input(&self) -> Value {
        if let Some(input) = self.input.as_ref() {
            return input.0.clone();
        }
        let mut input = self.extra.clone();
        if let Some(value) = self.capability.as_ref() {
            input.insert("capability".to_string(), Value::String(value.clone()));
        }
        if let Some(value) = self.query.as_ref() {
            input.insert("query".to_string(), Value::String(value.clone()));
        }
        if let Some(value) = self.source.as_ref() {
            input.insert("source".to_string(), Value::String(value.clone()));
        }
        input.insert("try_fallbacks".to_string(), Value::Bool(self.try_fallbacks));
        Value::Object(input)
    }
}

#[derive(Debug)]
struct CapabilityInvokeCommand {
    capability: Option<TrimmedText>,
    query: Option<TrimmedText>,
    tool_input: Value,
    binding: Option<CapabilityBindingRequest>,
    preferred_source: Option<CapabilitySource>,
    try_fallbacks: bool,
}

impl TryFrom<CapabilityInvokeInput> for CapabilityInvokeCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: CapabilityInvokeInput) -> Result<Self, Self::Error> {
        let tool_input = input.tool_input();
        let capability = input
            .capability
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        let query = input
            .query
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());
        let binding = input.binding.map(TryInto::try_into).transpose()?;
        let preferred_source = input.preferred_source.map(CapabilitySourceInput::runtime);

        Ok(Self {
            capability,
            query,
            tool_input,
            binding,
            preferred_source,
            try_fallbacks: input.try_fallbacks,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum CapabilityInvokeResult {
    External(ExternalJson),
    FailedResult {
        binding: CapabilityBindingRef,
        result: ExternalJson,
    },
    FailedError {
        binding: CapabilityBindingRef,
        error: String,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CapabilityInvokeOutput {
    capability: String,
    binding_used: CapabilityBindingRef,
    decision: String,
    lane: String,
    effect_class: String,
    result: Option<CapabilityInvokeResult>,
    fallback_available: Vec<CapabilityBindingRef>,
}

#[medousa_tool(id = COGNITION_CAPABILITY_INVOKE_ID)]
impl CognitionCapabilityInvokeTool {
    /// Resolve a capability intent and execute the best available binding in one call. Returns a policy receipt with binding_used, decision, result, and fallback_available.
    pub(crate) async fn invoke_typed(
        &self,
        input: CapabilityInvokeInput,
    ) -> stasis::prelude::Result<CapabilityInvokeOutput> {
        let command = CapabilityInvokeCommand::try_from(input)?;
        let capability_id = command.capability.as_ref().map(TrimmedText::as_str);
        let query = command.query.as_ref().map(TrimmedText::as_str);

        let summary = capability_id.or(query).unwrap_or("capability").to_string();
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_CAPABILITY_INVOKE_ID.as_str().to_string(),
                input_summary: summary,
            })
            .await;

        let registry = self.capability_registry.read().await;
        let resolved = resolve_capability_from_input(&registry, capability_id, query)?;

        let (primary, mut fallbacks) = select_binding_for_invoke(
            &resolved,
            command.preferred_source,
            command.binding.as_ref(),
        )?;
        let mut candidates = vec![primary];
        if command.try_fallbacks {
            candidates.append(&mut fallbacks);
        }

        let session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await?;

        let mut last_error: Option<CapabilityInvokeResult> = None;
        for (index, binding) in candidates.iter().enumerate() {
            let result = match binding.source {
                CapabilitySource::Mcp => {
                    invoke_mcp_binding(
                        &self.gateway_client,
                        &session_id,
                        binding,
                        &command.tool_input,
                    )
                    .await
                }
                CapabilitySource::Grapheme => {
                    invoke_grapheme_binding(&self.runtime, binding, &command.tool_input).await
                }
            };

            match result {
                Ok(result) if invoke_succeeded(binding, &result) => {
                    let remaining = candidates
                        .iter()
                        .skip(index + 1)
                        .cloned()
                        .collect::<Vec<_>>();
                    return Ok(CapabilityInvokeOutput {
                        capability: resolved.capability.clone(),
                        binding_used: binding_ref(binding),
                        decision: "allow".to_string(),
                        lane: "interactive".to_string(),
                        effect_class: effect_class_from_result(
                            binding,
                            &result,
                            &resolved.capability,
                        ),
                        result: Some(CapabilityInvokeResult::External(ExternalJson::new(result))),
                        fallback_available: fallback_bindings(&remaining),
                    });
                }
                Ok(result) => {
                    last_error = Some(CapabilityInvokeResult::FailedResult {
                        binding: binding_ref(binding),
                        result: ExternalJson::new(result),
                    });
                }
                Err(error) => {
                    last_error = Some(CapabilityInvokeResult::FailedError {
                        binding: binding_ref(binding),
                        error: error.to_string(),
                    });
                }
            }
        }

        Ok(CapabilityInvokeOutput {
            capability: resolved.capability.clone(),
            binding_used: binding_ref(&candidates[0]),
            decision: "deny".to_string(),
            lane: "interactive".to_string(),
            effect_class: effect_class_for_capability(&resolved.capability).to_string(),
            result: last_error,
            fallback_available: fallback_bindings(&candidates[1..]),
        })
    }
}

// ── cognition_mcp_promote_to_job ──────────────────────────────────────────────

pub struct CognitionMcpPromoteToJobTool {
    runtime: Arc<RuntimeComposition>,
    workflow_registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionMcpPromoteToJobTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        workflow_registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    ) -> Self {
        Self {
            runtime,
            workflow_registry,
            event_tx,
            turn_scope,
        }
    }
}

fn default_bridge_queue() -> String {
    "default".to_string()
}

fn default_mcp_step_id() -> String {
    "mcp_step".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct McpPromoteToJobInput {
    #[schemars(required, with = "String")]
    pub(crate) server_id: Option<String>,
    #[schemars(required, with = "String")]
    pub(crate) tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "BridgeObject", skip_serializing_if = "Option::is_none")]
    pub(crate) input: Option<BridgeObject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
    #[serde(default = "default_bridge_queue")]
    #[schemars(default = "default_bridge_queue")]
    pub(crate) queue: String,
    #[serde(default = "default_mcp_step_id")]
    #[schemars(default = "default_mcp_step_id")]
    pub(crate) step_id: String,
}

#[derive(Debug)]
struct McpPromoteToJobCommand {
    server_id: TrimmedText,
    tool_name: TrimmedText,
    input: Value,
    note: Option<String>,
    queue: TrimmedText,
    step_id: TrimmedText,
}

impl TryFrom<McpPromoteToJobInput> for McpPromoteToJobCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: McpPromoteToJobInput) -> Result<Self, Self::Error> {
        let server_id = required_mcp_identifier(input.server_id, "server_id")?;
        let tool_name = required_mcp_identifier(input.tool_name, "tool_name")?;
        let queue = required_mcp_identifier(Some(input.queue), "queue")?;
        let step_id = required_mcp_identifier(Some(input.step_id), "step_id")?;

        Ok(Self {
            server_id,
            tool_name,
            input: input
                .input
                .map(|input| input.0)
                .unwrap_or_else(|| json!({})),
            note: input.note,
            queue,
            step_id,
        })
    }
}

fn required_mcp_identifier(
    value: Option<String>,
    field: &str,
) -> stasis::prelude::Result<TrimmedText> {
    let value = value.ok_or_else(|| {
        StasisError::PortFailure(format!("cognition_mcp_promote_to_job: {field} is required"))
    })?;
    TrimmedText::new(value).map_err(|_| {
        StasisError::PortFailure(format!("cognition_mcp_promote_to_job: {field} is required"))
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct McpPromoteToJobOutput {
    workflow_id: String,
    job_id: String,
    root_job_id: String,
    job_type: String,
    status: String,
    lane: String,
    note: Option<String>,
    continuation: Option<ExternalJson>,
}

#[medousa_tool(id = COGNITION_MCP_PROMOTE_TO_JOB_ID)]
impl CognitionMcpPromoteToJobTool {
    /// Promote a successful MCP invoke into a durable sequential workflow job with one MCP step.
    pub(crate) async fn invoke_typed(
        &self,
        input: McpPromoteToJobInput,
    ) -> stasis::prelude::Result<McpPromoteToJobOutput> {
        let command = McpPromoteToJobCommand::try_from(input)?;

        let workflow_id = new_workflow_id();
        let payload = MedousaWorkflowPayload {
            workflow_id: workflow_id.clone(),
            name: Some(format!(
                "mcp:{}.{}",
                command.server_id.as_str(),
                command.tool_name.as_str()
            )),
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            on_failure: "stop".to_string(),
            note: command.note.clone(),
            lane: "interactive".to_string(),
            steps: vec![WorkflowStepSpec::Mcp {
                id: command.step_id.as_str().to_string(),
                server_id: command.server_id.as_str().to_string(),
                tool_name: command.tool_name.as_str().to_string(),
                args: command.input,
                effect_class: None,
            }],
        };

        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        let continuation = scope
            .as_ref()
            .map(|turn_scope| WorkflowEnqueueContinuation {
                turn_scope,
                tool_name: COGNITION_MCP_PROMOTE_TO_JOB_ID.as_str(),
                await_mode: ContinuationAwaitMode::Async,
            });
        let job_id = enqueue_workflow_job(
            self.runtime.as_ref(),
            &payload,
            command.queue.as_str(),
            continuation,
        )
        .await?;
        let job_type = workflow_job_type_for_strategy(&payload.strategy)
            .unwrap_or(WORKFLOW_SEQUENTIAL_JOB_TYPE);

        let record = WorkflowRecord {
            workflow_id: workflow_id.clone(),
            name: payload.name.clone(),
            strategy: payload.strategy.clone(),
            mode: payload.mode.clone(),
            on_failure: payload.on_failure.clone(),
            note: payload.note.clone(),
            root_job_id: job_id.clone(),
            status: WorkflowStatus::Enqueued,
            created_at: Utc::now(),
            scheduled_recurring_id: None,
            step_results: Vec::new(),
        };
        self.workflow_registry.insert(record).await;

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_MCP_PROMOTE_TO_JOB_ID.as_str().to_string(),
                input_summary: format!(
                    "{}.{}",
                    command.server_id.as_str(),
                    command.tool_name.as_str()
                ),
            })
            .await;

        let continuation = scope.as_ref().map(|turn_scope| {
            ExternalJson::new(continuation_tool_metadata(
                turn_scope,
                &job_id,
                ContinuationAwaitMode::Async,
            ))
        });
        Ok(McpPromoteToJobOutput {
            workflow_id,
            job_id: job_id.clone(),
            root_job_id: job_id,
            job_type: job_type.to_string(),
            status: "enqueued".to_string(),
            lane: "interactive".to_string(),
            note: command.note,
            continuation,
        })
    }
}

// ── cognition_grapheme_template_run ───────────────────────────────────────────

pub struct CognitionGraphemeTemplateRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionGraphemeTemplateRunTool {
    pub fn new(runtime: Arc<RuntimeComposition>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { runtime, event_tx }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphemeTemplateInput {
    ResearchReport,
    HttpPoll,
    CsvDigest,
}

impl GraphemeTemplateInput {
    fn as_str(self) -> &'static str {
        match self {
            Self::ResearchReport => "research_report",
            Self::HttpPoll => "http_poll",
            Self::CsvDigest => "csv_digest",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GraphemeTemplateRunInput {
    #[schemars(required, with = "GraphemeTemplateInput")]
    pub(crate) template: Option<GraphemeTemplateInput>,
    /// Template parameters (topic/query, url, etc.)
    #[schemars(required, with = "BridgeObject")]
    pub(crate) params: Option<BridgeObject>,
}

#[derive(Debug)]
struct GraphemeTemplateRunCommand {
    template: GraphemeTemplateInput,
    params: Value,
}

impl TryFrom<GraphemeTemplateRunInput> for GraphemeTemplateRunCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: GraphemeTemplateRunInput) -> Result<Self, Self::Error> {
        let template = input.template.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_template_run: template is required".to_string(),
            )
        })?;
        let params = input
            .params
            .map(|params| params.0)
            .unwrap_or_else(|| json!({}));
        Ok(Self { template, params })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GraphemeTemplateRunOutput {
    Rejected {
        template: String,
        status: String,
        reason: String,
        validation: ExternalJson,
    },
    Result(ExternalJson),
}

#[medousa_tool(id = COGNITION_GRAPHEME_TEMPLATE_RUN_ID)]
impl CognitionGraphemeTemplateRunTool {
    /// Run a preset Grapheme workflow template. Supported templates: research_report, http_poll, csv_digest.
    pub(crate) async fn invoke_typed(
        &self,
        input: GraphemeTemplateRunInput,
    ) -> stasis::prelude::Result<GraphemeTemplateRunOutput> {
        let command = GraphemeTemplateRunCommand::try_from(input)?;
        let template = command.template.as_str();

        let source = render_grapheme_template(template, &command.params)?;
        let validation = validate_grapheme_source_for_schedule(&self.runtime, &source).await?;
        if !validation
            .get("validated")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            return Ok(GraphemeTemplateRunOutput::Rejected {
                template: template.to_string(),
                status: "rejected".to_string(),
                reason: "invalid_grapheme_source".to_string(),
                validation: ExternalJson::new(validation),
            });
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: COGNITION_GRAPHEME_TEMPLATE_RUN_ID.as_str().to_string(),
                input_summary: template.to_string(),
            })
            .await;

        let mut result =
            run_grapheme_via_runtime(&self.runtime, &source, "cognition_grapheme_template_run")
                .await?;
        result["template"] = json!(template);
        result["params"] = command.params;
        Ok(GraphemeTemplateRunOutput::Result(ExternalJson::new(result)))
    }
}

// ── cognition_web_search ──────────────────────────────────────────────────────

fn web_search_binding_reference(
    mode: &str,
    provider: Option<&str>,
) -> Option<(CapabilitySource, String)> {
    let mode = mode.trim().to_ascii_lowercase();
    if mode == "research_materials" {
        return Some((
            CapabilitySource::Grapheme,
            "websearch.research_materials".to_string(),
        ));
    }
    if mode == "research_report" {
        return Some((
            CapabilitySource::Grapheme,
            "websearch.research_report".to_string(),
        ));
    }
    if mode == "facade" || mode == "websearch" {
        return Some((CapabilitySource::Grapheme, "websearch.search".to_string()));
    }
    if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
        let normalized = provider
            .strip_prefix("web.")
            .unwrap_or(provider)
            .to_string();
        return Some((CapabilitySource::Grapheme, format!("web.{normalized}")));
    }
    None
}

pub struct CapabilityWebSearchBackend {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    runtime: Arc<RuntimeComposition>,
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CapabilityWebSearchBackend {
    pub fn new(
        capability_registry: Arc<RwLock<CapabilityRegistry>>,
        runtime: Arc<RuntimeComposition>,
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            capability_registry,
            runtime,
            gateway_client,
            session_id: session_id.into(),
            turn_scope,
            event_tx,
        }
    }
}

#[derive(Debug)]
struct WebSearchCommand {
    query: TrimmedText,
    mode: WebSearchMode,
    provider: Option<TrimmedText>,
    try_fallbacks: Option<bool>,
    max_results: Option<u64>,
}

impl TryFrom<WebSearchRequest> for WebSearchCommand {
    type Error = stasis::prelude::StasisError;

    fn try_from(input: WebSearchRequest) -> Result<Self, Self::Error> {
        let query = input.query.ok_or_else(|| {
            StasisError::PortFailure("cognition_web_search: query is required".to_string())
        })?;
        let query = TrimmedText::new(query).map_err(|_| {
            StasisError::PortFailure("cognition_web_search: query is required".to_string())
        })?;
        let provider = input
            .provider
            .as_deref()
            .and_then(|value| TrimmedText::new(value).ok());

        Ok(Self {
            query,
            mode: input.mode,
            provider,
            try_fallbacks: input.try_fallbacks,
            max_results: input.max_results,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WebSearchCapabilityOutput {
    query: String,
    mode: String,
    provider_requested: Option<String>,
    binding_used: CapabilityBindingRef,
    decision: String,
    effect_class: String,
    result: Option<CapabilityInvokeResult>,
    fallback_available: Vec<CapabilityBindingRef>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum WebSearchOutput {
    Browser(ExternalJson),
    Capability(Box<WebSearchCapabilityOutput>),
}

impl CapabilityWebSearchBackend {
    async fn invoke_backend(
        &self,
        input: WebSearchRequest,
    ) -> stasis::prelude::Result<WebSearchOutput> {
        let command = WebSearchCommand::try_from(input)?;
        let query = command.query.as_str();

        let mode = command.mode.as_str();
        let settings = crate::capability_catalog::web_search_settings();
        let provider = command
            .provider
            .as_ref()
            .map(TrimmedText::as_str)
            .or(settings.preferred_provider.as_deref());
        let try_fallbacks = command.try_fallbacks.unwrap_or(settings.try_fallbacks);

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: "cognition_web_search".to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let max_results = command.max_results.unwrap_or(8) as usize;
        let chat_session_id = crate::runtime_session::resolve_active_chat_session_id_async(
            &self.turn_scope,
            &self.session_id,
        )
        .await?;
        let turn_correlation_id =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await
                .map(|scope| scope.turn_correlation_id.clone())
                .unwrap_or_else(|| chat_session_id.clone());

        if mode.eq_ignore_ascii_case("search") {
            match crate::browser_search::run_browser_backed_search(
                query,
                max_results,
                &self.turn_scope,
                &turn_correlation_id,
                &chat_session_id,
                None,
            )
            .await
            {
                Ok(response) if response.challenge.is_none() && !response.results.is_empty() => {
                    let binding = if response.provider.contains("duckduckgo") {
                        "browser_host_lite"
                    } else {
                        "browser_host"
                    };
                    return Ok(WebSearchOutput::Browser(ExternalJson::new(
                        crate::browser_search::search_response_to_tool_json(
                            query, mode, provider, &response, binding,
                        ),
                    )));
                }
                Ok(response) if response.challenge.is_some() => {
                    return Ok(WebSearchOutput::Browser(ExternalJson::new(
                        crate::browser_search::search_response_to_tool_json(
                            query,
                            mode,
                            provider,
                            &response,
                            "browser_host_lite",
                        ),
                    )));
                }
                Ok(_) | Err(_) => {}
            }
        }

        let explicit_binding = web_search_binding_reference(mode, provider)
            .map(|(source, reference)| CapabilityBindingRequest::new(source, reference))
            .transpose()?;

        let registry = self.capability_registry.read().await;
        let resolved = resolve_capability_from_input(&registry, Some("web_research"), None)?;
        let (primary, mut fallbacks) =
            select_binding_for_invoke(&resolved, None, explicit_binding.as_ref())?;
        let mut candidates = vec![primary];
        if try_fallbacks {
            candidates.append(&mut fallbacks);
        }
        candidates
            .retain(|binding| !crate::browser_search::is_discovery_binding(&binding.reference));
        if candidates.is_empty() {
            return Err(StasisError::PortFailure(
                "cognition_web_search: no search bindings available after filtering discovery ops"
                    .to_string(),
            ));
        }
        let mut fallbacks = candidates.split_off(1);
        let primary = candidates.remove(0);

        let tool_input = json!({ "query": query });
        let session_id = chat_session_id;
        let mut last_error: Option<CapabilityInvokeResult> = None;
        let mut candidate_list = vec![primary];
        candidate_list.append(&mut fallbacks);
        for (index, binding) in candidate_list.iter().enumerate() {
            let result = match binding.source {
                CapabilitySource::Mcp => {
                    invoke_mcp_binding(&self.gateway_client, &session_id, binding, &tool_input)
                        .await
                }
                CapabilitySource::Grapheme => {
                    invoke_grapheme_binding(&self.runtime, binding, &tool_input).await
                }
            };

            match result {
                Ok(result) if invoke_succeeded(binding, &result) => {
                    let remaining = candidate_list
                        .iter()
                        .skip(index + 1)
                        .cloned()
                        .collect::<Vec<_>>();
                    return Ok(WebSearchOutput::Capability(Box::new(
                        WebSearchCapabilityOutput {
                            query: query.to_string(),
                            mode: mode.to_string(),
                            provider_requested: provider.map(str::to_string),
                            binding_used: binding_ref(binding),
                            decision: "allow".to_string(),
                            effect_class: effect_class_from_result(
                                binding,
                                &result,
                                "web_research",
                            ),
                            result: Some(CapabilityInvokeResult::External(ExternalJson::new(
                                result,
                            ))),
                            fallback_available: fallback_bindings(&remaining),
                        },
                    )));
                }
                Ok(result) => {
                    last_error = Some(CapabilityInvokeResult::FailedResult {
                        binding: binding_ref(binding),
                        result: ExternalJson::new(result),
                    });
                }
                Err(error) => {
                    last_error = Some(CapabilityInvokeResult::FailedError {
                        binding: binding_ref(binding),
                        error: error.to_string(),
                    });
                }
            }
        }

        Ok(WebSearchOutput::Capability(Box::new(
            WebSearchCapabilityOutput {
                query: query.to_string(),
                mode: mode.to_string(),
                provider_requested: provider.map(str::to_string),
                binding_used: binding_ref(&candidate_list[0]),
                decision: "deny".to_string(),
                effect_class: effect_class_for_capability("web_research").to_string(),
                result: last_error,
                fallback_available: fallback_bindings(&candidate_list[1..]),
            },
        )))
    }
}

#[async_trait::async_trait]
impl WebSearchBackend for CapabilityWebSearchBackend {
    async fn search(&self, request: WebSearchRequest) -> stasis::prelude::Result<Value> {
        let output = self.invoke_backend(request).await?;
        serde_json::to_value(output).map_err(|error| {
            StasisError::PortFailure(format!("serialize cognition_web_search output: {error}"))
        })
    }
}

#[cfg(all(test, feature = "full-daemon"))]
mod tests {
    use super::*;

    #[test]
    fn research_report_template_requires_topic() {
        let err = render_grapheme_template("research_report", &json!({})).unwrap_err();
        assert!(err.to_string().contains("topic"));
    }

    #[test]
    fn research_report_template_renders_source() {
        let source = render_grapheme_template("research_report", &json!({ "topic": "rust async" }))
            .expect("template");
        assert!(source.contains("websearch.research_report"));
        assert!(source.contains("rust async"));
    }

    #[test]
    fn grapheme_source_for_websearch_binding() {
        let binding = CapabilityBinding::grapheme("websearch.search", 10, true);
        let source =
            grapheme_source_for_binding(&binding, &json!({ "query": "medousa" })).expect("source");
        assert!(source.contains("websearch.search"));
        assert!(source.contains("medousa"));
    }

    #[test]
    fn grapheme_source_for_web_provider_binding() {
        let binding = CapabilityBinding::grapheme("web.duckduckgo", 10, true);
        let source = grapheme_source_for_binding(&binding, &json!({ "query": "phoenix events" }))
            .expect("source");
        assert!(source.contains("web.duckduckgo"));
        assert!(source.contains("phoenix events"));
    }

    #[test]
    fn explicit_grapheme_source_preserves_content_bytes() {
        let binding = CapabilityBinding::grapheme("custom.operation", 10, true);
        let source = grapheme_source_for_binding(
            &binding,
            &json!({ "source": "  query Custom { value }  \n" }),
        )
        .expect("source");
        assert_eq!(source, "  query Custom { value }  \n");
    }

    #[test]
    fn explicit_grapheme_source_rejects_blank_content() {
        let binding = CapabilityBinding::grapheme("custom.operation", 10, true);
        let error = grapheme_source_for_binding(&binding, &json!({ "source": " \n\t" }))
            .expect_err("blank source should fail");
        assert!(error.to_string().contains("input.source must be non-empty"));
    }

    #[test]
    fn ordered_bindings_respects_priority() {
        let response = CapabilityResolveResponse {
            capability: "web_research".to_string(),
            title: "Research".to_string(),
            description: None,
            implementations: crate::capability_catalog::CapabilityImplementations {
                grapheme: vec![
                    CapabilityBinding::grapheme("websearch.search", 10, true),
                    CapabilityBinding::grapheme("websearch.research_report", 20, true),
                ],
                mcp: vec![],
            },
            recommended: None,
            gateway_unreachable: None,
        };

        let ordered = ordered_available_bindings(&response, None);
        assert_eq!(ordered[0].reference, "websearch.search");
    }

    #[test]
    fn capability_command_normalizes_selection_without_rewriting_forwarded_input() {
        let command = CapabilityInvokeCommand::try_from(CapabilityInvokeInput {
            capability: Some("  document_search  ".to_string()),
            query: Some("  docs  ".to_string()),
            input: None,
            source: None,
            binding: Some(CapabilityBindingInput {
                source: Some(CapabilitySourceInput::Mcp),
                reference: Some("  docs.search  ".to_string()),
            }),
            preferred_source: Some(CapabilitySourceInput::Mcp),
            try_fallbacks: true,
            extra: serde_json::Map::new(),
        })
        .expect("command");

        assert_eq!(
            command.capability.as_ref().map(TrimmedText::as_str),
            Some("document_search")
        );
        assert_eq!(
            command.query.as_ref().map(TrimmedText::as_str),
            Some("docs")
        );
        assert_eq!(command.preferred_source, Some(CapabilitySource::Mcp));
        assert_eq!(
            command
                .binding
                .as_ref()
                .map(|binding| binding.reference.as_str()),
            Some("docs.search")
        );
        assert_eq!(
            command.tool_input["capability"],
            json!("  document_search  ")
        );
        assert_eq!(command.tool_input["query"], json!("  docs  "));
    }

    #[test]
    fn capability_binding_command_rejects_blank_reference() {
        let error = CapabilityBindingRequest::try_from(CapabilityBindingInput {
            source: Some(CapabilitySourceInput::Grapheme),
            reference: Some(" \n\t".to_string()),
        })
        .expect_err("blank binding reference should fail");

        assert!(error.to_string().contains("binding.reference is required"));
    }

    #[test]
    fn mcp_promotion_command_normalizes_identifiers_and_preserves_arguments() {
        let command = McpPromoteToJobCommand::try_from(McpPromoteToJobInput {
            server_id: Some("  docs  ".to_string()),
            tool_name: Some("  search  ".to_string()),
            input: Some(BridgeObject(json!({
                "query": "  keep payload bytes  "
            }))),
            note: Some("  operator note  ".to_string()),
            queue: "  bridge  ".to_string(),
            step_id: "  search_step  ".to_string(),
        })
        .expect("command");

        assert_eq!(command.server_id.as_str(), "docs");
        assert_eq!(command.tool_name.as_str(), "search");
        assert_eq!(command.queue.as_str(), "bridge");
        assert_eq!(command.step_id.as_str(), "search_step");
        assert_eq!(command.note.as_deref(), Some("  operator note  "));
        assert_eq!(command.input["query"], json!("  keep payload bytes  "));
    }

    #[test]
    fn mcp_promotion_command_rejects_blank_required_identifiers() {
        let error = McpPromoteToJobCommand::try_from(McpPromoteToJobInput {
            server_id: Some(" \n".to_string()),
            tool_name: Some("search".to_string()),
            input: None,
            note: None,
            queue: "default".to_string(),
            step_id: "mcp_step".to_string(),
        })
        .expect_err("blank server id should fail");

        assert!(error.to_string().contains("server_id is required"));
    }

    #[test]
    fn grapheme_template_command_requires_template_and_preserves_params() {
        let command = GraphemeTemplateRunCommand::try_from(GraphemeTemplateRunInput {
            template: Some(GraphemeTemplateInput::HttpPoll),
            params: Some(BridgeObject(json!({ "url": "  https://example.test  " }))),
        })
        .expect("command");

        assert_eq!(command.template.as_str(), "http_poll");
        assert_eq!(command.params["url"], json!("  https://example.test  "));

        let error = GraphemeTemplateRunCommand::try_from(GraphemeTemplateRunInput {
            template: None,
            params: None,
        })
        .expect_err("missing template should fail");
        assert!(error.to_string().contains("template is required"));
    }

    #[test]
    fn web_search_command_normalizes_query_and_provider() {
        let command = WebSearchCommand::try_from(WebSearchRequest {
            query: Some("  rust async  ".to_string()),
            mode: WebSearchMode::Search,
            provider: Some("  duckduckgo  ".to_string()),
            try_fallbacks: Some(true),
            max_results: Some(12),
        })
        .expect("command");

        assert_eq!(command.query.as_str(), "rust async");
        assert_eq!(
            command.provider.as_ref().map(TrimmedText::as_str),
            Some("duckduckgo")
        );
        assert_eq!(command.mode.as_str(), "search");
        assert_eq!(command.try_fallbacks, Some(true));
        assert_eq!(command.max_results, Some(12));
    }

    #[test]
    fn web_search_command_rejects_blank_query() {
        let error = WebSearchCommand::try_from(WebSearchRequest {
            query: Some(" \n\t".to_string()),
            mode: WebSearchMode::default(),
            provider: None,
            try_fallbacks: None,
            max_results: None,
        })
        .expect_err("blank query should fail");
        assert!(error.to_string().contains("query is required"));
    }
}
