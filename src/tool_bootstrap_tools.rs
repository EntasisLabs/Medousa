//! `cognition_tools_discover` — session-scoped tool domain unlock (Phase 9C).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::agent_runtime::turn_worker::{host_bus_tool_names, tool_allowed};
use crate::tool_bootstrap::{
    COGNITION_TOOLS_DISCOVER, ToolSurfaceLane, bootstrap_tools, discover_session_domain,
    domain_catalog, load_session_tool_surface, tool_one_liner,
};
use crate::turn_continuation::TurnContinuationScope;

pub fn register_tool_bootstrap_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> StasisResult<()> {
    registry.register_tool(CognitionToolsDiscoverTool { turn_scope })?;
    Ok(())
}

pub struct CognitionToolsDiscoverTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

#[async_trait]
impl StasisTool for CognitionToolsDiscoverTool {
    fn name(&self) -> &'static str {
        COGNITION_TOOLS_DISCOVER
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Unlock a tool domain for this session and return its catalog. Host: memory + vault auto-unlock at session start. \
             Other host domains: catalog, runtime, history, identity, skill, overlay. Worker domains: execute, discover, memory, \
             vault, openshell, scripts. Bootstrap tools stay visible without discover.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["domain"],
            "properties": {
                "domain": {
                    "type": "string",
                    "description": "Domain id — host: memory|catalog|runtime|vault|history|identity|skill|overlay; worker: execute|discover|memory|vault|openshell|scripts"
                },
                "lane": {
                    "type": "string",
                    "enum": ["host", "worker", "auto"],
                    "description": "Surface lane (default auto from active turn scope)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session id (defaults to active turn session)"
                },
                "list_only": {
                    "type": "boolean",
                    "description": "If true, return catalog without unlocking"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id =
            crate::runtime_session::require_active_chat_session_id_from_input(
                &input,
                &self.turn_scope,
                "cognition_tools_discover",
            )
            .await?;
        let lane = resolve_lane(&self.turn_scope, &input);
        let list_only = input
            .get("list_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if input
            .get("domain")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Ok(list_domains_catalog(&session_id, lane));
        }

        let domain = input
            .get("domain")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();

        if list_only {
            return Ok(domain_detail(&session_id, lane, domain, &host_bus_tool_names()));
        }

        let allowlist = match lane {
            ToolSurfaceLane::Host => host_bus_tool_names(),
            ToolSurfaceLane::Worker => {
                // Worker discover uses worker general allowlist as ceiling for catalog display.
                crate::agent_runtime::turn_worker::allowed_tool_names_for_intent(
                    crate::agent_runtime::turn_worker::TurnWorkerIntent::Research,
                )
            }
        };

        let (surface, tools) =
            discover_session_domain(&session_id, lane, domain, &allowlist).map_err(|err| {
                StasisError::PortFailure(err)
            })?;

        let catalog = domain_catalog(lane)
            .iter()
            .find(|entry| entry.domain == domain.to_ascii_lowercase())
            .map(|entry| {
                json!({
                    "domain": entry.domain,
                    "summary": entry.summary,
                    "tools": entry.tools,
                })
            });

        Ok(json!({
            "ok": true,
            "session_id": session_id,
            "lane": match lane { ToolSurfaceLane::Host => "host", ToolSurfaceLane::Worker => "worker" },
            "domain": domain.to_ascii_lowercase(),
            "unlocked_domains": surface.unlocked_domains,
            "tools_unlocked": tools,
            "catalog": catalog,
            "bootstrap_tools": bootstrap_tools(lane),
            "message": format!(
                "Unlocked domain '{}' for session — {} tools now on surface",
                domain.to_ascii_lowercase(),
                tools.len()
            ),
        }))
    }
}

fn list_domains_catalog(session_id: &str, lane: ToolSurfaceLane) -> Value {
    let surface = load_session_tool_surface(session_id);
    let domains: Vec<Value> = domain_catalog(lane)
        .iter()
        .map(|entry| {
            json!({
                "domain": entry.domain,
                "summary": entry.summary,
                "unlocked": surface.unlocked_domains.iter().any(|d| d == entry.domain),
                "tool_count": entry.tools.len(),
            })
        })
        .collect();
    json!({
        "ok": true,
        "session_id": session_id,
        "lane": match lane { ToolSurfaceLane::Host => "host", ToolSurfaceLane::Worker => "worker" },
        "bootstrap_tools": bootstrap_tools(lane),
        "domains": domains,
        "unlocked_domains": surface.unlocked_domains,
        "hint": "Call with domain=memory|catalog|runtime|… to unlock a group for this session.",
    })
}

fn domain_detail(
    session_id: &str,
    lane: ToolSurfaceLane,
    domain: &str,
    allowlist: &HashSet<String>,
) -> Value {
    let normalized = domain.trim().to_ascii_lowercase();
    let entry = domain_catalog(lane)
        .iter()
        .find(|entry| entry.domain == normalized);
    let Some(entry) = entry else {
        return json!({
            "ok": false,
            "error": format!("unknown domain: {domain}"),
        });
    };
    let tools: Vec<Value> = entry
        .tools
        .iter()
        .filter(|name| tool_allowed(name, allowlist))
        .map(|name| {
            json!({
                "name": name,
                "summary": tool_one_liner(name),
            })
        })
        .collect();
    let surface = load_session_tool_surface(session_id);
    json!({
        "ok": true,
        "session_id": session_id,
        "domain": entry.domain,
        "summary": entry.summary,
        "unlocked": surface.unlocked_domains.iter().any(|d| d == entry.domain),
        "tools": tools,
    })
}

fn resolve_lane(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
    input: &Value,
) -> ToolSurfaceLane {
    if let Some(lane) = input.get("lane").and_then(Value::as_str) {
        match lane.trim().to_ascii_lowercase().as_str() {
            "worker" => return ToolSurfaceLane::Worker,
            "host" => return ToolSurfaceLane::Host,
            _ => {}
        }
    }
    if let Ok(scope) = turn_scope.try_read() {
        if scope.is_none() {
            // Worker loops may run without host scope — caller should pass lane=worker.
        }
    }
    ToolSurfaceLane::Host
}
