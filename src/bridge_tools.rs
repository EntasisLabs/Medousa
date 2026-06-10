//! Phase D3 bridge tools — capability invoke, MCP promote, grapheme templates.
//!
//! Design: docs/internal/runtime-tools-roadmap.md

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
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
use crate::tools::{run_grapheme_via_runtime, validate_grapheme_source_for_schedule};
use crate::turn_continuation::{
    ContinuationAwaitMode, TurnContinuationScope, continuation_tool_metadata,
};
use crate::workflow::{
    MedousaWorkflowPayload, WorkflowEnqueueContinuation, WorkflowRecord, WorkflowRegistry,
    WorkflowStatus, WorkflowStepSpec, enqueue_workflow_job, new_workflow_id,
    workflow_job_type_for_strategy, WORKFLOW_SEQUENTIAL_JOB_TYPE,
};

fn escape_grapheme_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn binding_ref_json(binding: &CapabilityBinding) -> Value {
    json!({
        "source": binding.source.as_str(),
        "reference": binding.reference
    })
}

fn fallback_bindings_json(bindings: &[CapabilityBinding]) -> Vec<Value> {
    bindings
        .iter()
        .map(binding_ref_json)
        .collect::<Vec<_>>()
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

fn parse_preferred_source(value: Option<&str>) -> Option<CapabilitySource> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "grapheme" => Some(CapabilitySource::Grapheme),
        "mcp" => Some(CapabilitySource::Mcp),
        _ => None,
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
    input: &Value,
) -> stasis::prelude::Result<(CapabilityBinding, Vec<CapabilityBinding>)> {
    let preferred_source = parse_preferred_source(
        input
            .get("preferred_source")
            .and_then(|value| value.as_str()),
    );

    if let Some(explicit) = input.get("binding") {
        let source = explicit
            .get("source")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_capability_invoke: binding.source is required".to_string(),
                )
            })?;
        let reference = explicit
            .get("reference")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                StasisError::PortFailure(
                    "cognition_capability_invoke: binding.reference is required".to_string(),
                )
            })?;

        let parsed_source = match source.trim().to_ascii_lowercase().as_str() {
            "grapheme" => CapabilitySource::Grapheme,
            "mcp" => CapabilitySource::Mcp,
            other => {
                return Err(StasisError::PortFailure(format!(
                    "cognition_capability_invoke: unsupported binding.source '{other}'"
                )));
            }
        };

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
        available.retain(|binding| binding.reference != primary.reference || binding.source != primary.source);
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
        return Ok(source.to_string());
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
        "web.providers" => {
            r#"import core from "grapheme/core"
query CapabilityInvoke {
  web.providers() { count providers { id } }
}"#
            .to_string()
        }
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
                None => {
                    r#"import core from "grapheme/core"
query CapabilityInvoke {
  web.capabilities() { available_providers provider }
}"#
                    .to_string()
                }
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
            let topic = params.get("topic").or_else(|| params.get("query")).and_then(|v| v.as_str()).ok_or_else(|| {
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
    Ok(serde_json::to_value(response).map_err(|error| {
        StasisError::PortFailure(format!(
            "cognition_capability_invoke: failed to encode MCP response: {error}"
        ))
    })?)
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
        CapabilitySource::Mcp => result.get("ok").and_then(|value| value.as_bool()).unwrap_or(false),
        CapabilitySource::Grapheme => result
            .get("succeeded")
            .and_then(|value| value.as_bool())
            .or_else(|| result.get("ok").and_then(|value| value.as_bool()))
            .unwrap_or(false),
    }
}

fn effect_class_from_result(binding: &CapabilityBinding, result: &Value, capability_id: &str) -> String {
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
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCapabilityInvokeTool {
    pub fn new(
        capability_registry: Arc<RwLock<CapabilityRegistry>>,
        runtime: Arc<RuntimeComposition>,
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            capability_registry,
            runtime,
            gateway_client,
            session_id: session_id.into(),
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionCapabilityInvokeTool {
    fn name(&self) -> &'static str {
        "cognition_capability_invoke"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Resolve a capability intent and execute the best available binding in one call. \
             Returns a policy receipt with binding_used, decision, result, and fallback_available.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "capability": { "type": "string", "description": "Capability id, e.g. document_search" },
                "query": { "type": "string", "description": "Fuzzy resolve when capability id is unknown" },
                "input": { "type": "object", "description": "Arguments forwarded to MCP or used to build Grapheme source" },
                "source": { "type": "string", "description": "Optional explicit Grapheme source override" },
                "binding": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "enum": ["grapheme", "mcp"] },
                        "reference": { "type": "string" }
                    }
                },
                "preferred_source": { "type": "string", "enum": ["grapheme", "mcp"] },
                "try_fallbacks": { "type": "boolean", "default": true }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let capability_id = input
            .get("capability")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let summary = capability_id
            .unwrap_or(query.unwrap_or("capability"))
            .to_string();
        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: summary,
            })
            .await;

        let registry = self.capability_registry.read().await;
        let resolved = resolve_capability_from_input(&registry, capability_id, query)?;
        let try_fallbacks = input
            .get("try_fallbacks")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);

        let (primary, mut fallbacks) = select_binding_for_invoke(&resolved, &input)?;
        let mut candidates = vec![primary];
        if try_fallbacks {
            candidates.append(&mut fallbacks);
        }

        let tool_input = input
            .get("input")
            .cloned()
            .unwrap_or_else(|| input.clone());

        let mut last_error = None;
        for (index, binding) in candidates.iter().enumerate() {
            let result = match binding.source {
                CapabilitySource::Mcp => {
                    invoke_mcp_binding(
                        &self.gateway_client,
                        &self.session_id,
                        binding,
                        &tool_input,
                    )
                    .await
                }
                CapabilitySource::Grapheme => {
                    invoke_grapheme_binding(&self.runtime, binding, &tool_input).await
                }
            };

            match result {
                Ok(result) if invoke_succeeded(binding, &result) => {
                    let remaining = candidates.iter().skip(index + 1).cloned().collect::<Vec<_>>();
                    return Ok(json!({
                        "capability": resolved.capability,
                        "binding_used": binding_ref_json(binding),
                        "decision": "allow",
                        "lane": "interactive",
                        "effect_class": effect_class_from_result(binding, &result, &resolved.capability),
                        "result": result,
                        "fallback_available": fallback_bindings_json(&remaining)
                    }));
                }
                Ok(result) => {
                    last_error = Some(json!({
                        "binding": binding_ref_json(binding),
                        "result": result
                    }));
                }
                Err(error) => {
                    last_error = Some(json!({
                        "binding": binding_ref_json(binding),
                        "error": error.to_string()
                    }));
                }
            }
        }

        Ok(json!({
            "capability": resolved.capability,
            "binding_used": binding_ref_json(&candidates[0]),
            "decision": "deny",
            "lane": "interactive",
            "effect_class": effect_class_for_capability(&resolved.capability),
            "result": last_error,
            "fallback_available": fallback_bindings_json(&candidates[1..])
        }))
    }
}

// ── cognition_mcp_promote_to_job ──────────────────────────────────────────────

pub struct CognitionMcpPromoteToJobTool {
    runtime: Arc<RuntimeComposition>,
    workflow_registry: Arc<WorkflowRegistry>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionMcpPromoteToJobTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        workflow_registry: Arc<WorkflowRegistry>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            workflow_registry,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionMcpPromoteToJobTool {
    fn name(&self) -> &'static str {
        "cognition_mcp_promote_to_job"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Promote a successful MCP invoke into a durable sequential workflow job with one MCP step.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "server_id": { "type": "string" },
                "tool_name": { "type": "string" },
                "input": { "type": "object" },
                "note": { "type": "string" },
                "queue": { "type": "string", "default": "default" },
                "step_id": { "type": "string", "default": "mcp_step" }
            },
            "required": ["server_id", "tool_name"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let server_id = input.get("server_id").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_mcp_promote_to_job: server_id is required".to_string(),
            )
        })?;
        let tool_name = input.get("tool_name").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_mcp_promote_to_job: tool_name is required".to_string(),
            )
        })?;
        let args = input
            .get("input")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let note = input
            .get("note")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let queue = input
            .get("queue")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let step_id = input
            .get("step_id")
            .and_then(|v| v.as_str())
            .unwrap_or("mcp_step");

        let workflow_id = new_workflow_id();
        let payload = MedousaWorkflowPayload {
            workflow_id: workflow_id.clone(),
            name: Some(format!("mcp:{server_id}.{tool_name}")),
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            on_failure: "stop".to_string(),
            note: note.clone(),
            lane: "interactive".to_string(),
            steps: vec![WorkflowStepSpec::Mcp {
                id: step_id.to_string(),
                server_id: server_id.to_string(),
                tool_name: tool_name.to_string(),
                args,
                effect_class: None,
            }],
        };

        let scope = self.turn_scope.read().await.clone();
        let continuation = scope.as_ref().map(|turn_scope| WorkflowEnqueueContinuation {
            turn_scope,
            tool_name: self.name(),
            await_mode: ContinuationAwaitMode::Async,
        });
        let job_id = enqueue_workflow_job(self.runtime.as_ref(), &payload, queue, continuation).await?;
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
                tool_name: self.name().to_string(),
                input_summary: format!("{server_id}.{tool_name}"),
            })
            .await;

        Ok(json!({
            "workflow_id": workflow_id,
            "job_id": job_id.clone(),
            "root_job_id": job_id,
            "job_type": job_type,
            "status": "enqueued",
            "lane": "interactive",
            "note": note,
            "continuation": scope.as_ref().map(|turn_scope| {
                continuation_tool_metadata(
                    turn_scope,
                    &job_id,
                    ContinuationAwaitMode::Async,
                )
            }),
        }))
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

#[async_trait]
impl StasisTool for CognitionGraphemeTemplateRunTool {
    fn name(&self) -> &'static str {
        "cognition_grapheme_template_run"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Run a preset Grapheme workflow template. \
             Supported templates: research_report, http_poll, csv_digest.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "template": {
                    "type": "string",
                    "enum": ["research_report", "http_poll", "csv_digest"]
                },
                "params": { "type": "object", "description": "Template parameters (topic/query, url, etc.)" }
            },
            "required": ["template", "params"]
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let template = input.get("template").and_then(|v| v.as_str()).ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_grapheme_template_run: template is required".to_string(),
            )
        })?;
        let params = input.get("params").cloned().unwrap_or_else(|| json!({}));

        let source = render_grapheme_template(template, &params)?;
        let validation = validate_grapheme_source_for_schedule(&self.runtime, &source).await?;
        if !validation
            .get("validated")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            return Ok(json!({
                "template": template,
                "status": "rejected",
                "reason": "invalid_grapheme_source",
                "validation": validation
            }));
        }

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: template.to_string(),
            })
            .await;

        let mut result =
            run_grapheme_via_runtime(&self.runtime, &source, "cognition_grapheme_template_run")
                .await?;
        result["template"] = json!(template);
        result["params"] = params;
        Ok(result)
    }
}

// ── cognition_web_search ──────────────────────────────────────────────────────

fn web_search_binding_reference(mode: &str, provider: Option<&str>) -> Option<(CapabilitySource, String)> {
    let mode = mode.trim().to_ascii_lowercase();
    if mode == "research_materials" {
        return Some((CapabilitySource::Grapheme, "websearch.research_materials".to_string()));
    }
    if mode == "research_report" {
        return Some((CapabilitySource::Grapheme, "websearch.research_report".to_string()));
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

pub struct CognitionWebSearchTool {
    capability_registry: Arc<RwLock<CapabilityRegistry>>,
    runtime: Arc<RuntimeComposition>,
    gateway_client: Arc<McpGatewayClient>,
    session_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionWebSearchTool {
    pub fn new(
        capability_registry: Arc<RwLock<CapabilityRegistry>>,
        runtime: Arc<RuntimeComposition>,
        gateway_client: Arc<McpGatewayClient>,
        session_id: impl Into<String>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            capability_registry,
            runtime,
            gateway_client,
            session_id: session_id.into(),
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionWebSearchTool {
    fn name(&self) -> &'static str {
        "cognition_web_search"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Search the public web with one call. Uses configured provider preference and binding \
             fallbacks (web.<provider>, then websearch.search). For deep reports use mode=research_report.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string", "description": "Search query or research topic" },
                "mode": {
                    "type": "string",
                    "enum": ["search", "facade", "research_materials", "research_report"],
                    "default": "search",
                    "description": "search = provider-native web lookup; research_* = websearch facade pipelines"
                },
                "provider": {
                    "type": "string",
                    "description": "Optional web provider id (duckduckgo, google, tavily, …). Defaults to capabilities.toml [web_search].preferred_provider or MEDOUSA_WEB_SEARCH_PROVIDER"
                },
                "try_fallbacks": {
                    "type": "boolean",
                    "description": "Try lower-priority bindings when the preferred provider fails"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> stasis::prelude::Result<Value> {
        let query = input
            .get("query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                StasisError::PortFailure("cognition_web_search: query is required".to_string())
            })?;

        let mode = input
            .get("mode")
            .and_then(|value| value.as_str())
            .unwrap_or("search");
        let settings = crate::capability_catalog::web_search_settings();
        let provider = input
            .get("provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(settings.preferred_provider.as_deref());
        let try_fallbacks = input
            .get("try_fallbacks")
            .and_then(|value| value.as_bool())
            .unwrap_or(settings.try_fallbacks);

        let _ = self
            .event_tx
            .send(TuiEvent::ToolInvoked {
                tool_name: self.name().to_string(),
                input_summary: query.to_string(),
            })
            .await;

        let mut invoke_input = json!({
            "capability": "web_research",
            "input": { "query": query },
            "try_fallbacks": try_fallbacks
        });
        if let Some((source, reference)) = web_search_binding_reference(mode, provider) {
            invoke_input["binding"] = json!({
                "source": source.as_str(),
                "reference": reference
            });
        }

        let registry = self.capability_registry.read().await;
        let resolved = resolve_capability_from_input(&registry, Some("web_research"), None)?;
        let (primary, mut fallbacks) = select_binding_for_invoke(&resolved, &invoke_input)?;
        let mut candidates = vec![primary];
        if try_fallbacks {
            candidates.append(&mut fallbacks);
        }

        let tool_input = json!({ "query": query });
        let mut last_error = None;
        for (index, binding) in candidates.iter().enumerate() {
            let result = match binding.source {
                CapabilitySource::Mcp => {
                    invoke_mcp_binding(
                        &self.gateway_client,
                        &self.session_id,
                        binding,
                        &tool_input,
                    )
                    .await
                }
                CapabilitySource::Grapheme => {
                    invoke_grapheme_binding(&self.runtime, binding, &tool_input).await
                }
            };

            match result {
                Ok(result) if invoke_succeeded(binding, &result) => {
                    let remaining = candidates.iter().skip(index + 1).cloned().collect::<Vec<_>>();
                    return Ok(json!({
                        "query": query,
                        "mode": mode,
                        "provider_requested": provider,
                        "binding_used": binding_ref_json(binding),
                        "decision": "allow",
                        "effect_class": effect_class_from_result(binding, &result, "web_research"),
                        "result": result,
                        "fallback_available": fallback_bindings_json(&remaining)
                    }));
                }
                Ok(result) => {
                    last_error = Some(json!({
                        "binding": binding_ref_json(binding),
                        "result": result
                    }));
                }
                Err(error) => {
                    last_error = Some(json!({
                        "binding": binding_ref_json(binding),
                        "error": error.to_string()
                    }));
                }
            }
        }

        Ok(json!({
            "query": query,
            "mode": mode,
            "provider_requested": provider,
            "binding_used": binding_ref_json(&candidates[0]),
            "decision": "deny",
            "effect_class": effect_class_for_capability("web_research"),
            "result": last_error,
            "fallback_available": fallback_bindings_json(&candidates[1..])
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_report_template_requires_topic() {
        let err = render_grapheme_template("research_report", &json!({})).unwrap_err();
        assert!(err.to_string().contains("topic"));
    }

    #[test]
    fn research_report_template_renders_source() {
        let source =
            render_grapheme_template("research_report", &json!({ "topic": "rust async" }))
                .expect("template");
        assert!(source.contains("websearch.research_report"));
        assert!(source.contains("rust async"));
    }

    #[test]
    fn grapheme_source_for_websearch_binding() {
        let binding = CapabilityBinding::grapheme("websearch.search", 10, true);
        let source = grapheme_source_for_binding(&binding, &json!({ "query": "medousa" }))
            .expect("source");
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
}
