//! `cognition_tools_discover` — session-scoped tool domain unlock (Phase 9C).

use std::collections::HashSet;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::RwLock;

use crate::agent_runtime::turn_worker::{host_bus_tool_names, tool_allowed};
use crate::tool_bootstrap::{
    COGNITION_TOOLS_DISCOVER, ToolSurfaceLane, bootstrap_tools, discover_session_domain,
    domain_catalog, load_session_tool_surface,
};
use crate::turn_continuation::TurnContinuationScope;
use crate::typed_tools::{CompatOption, ToolCatalogHandle, ToolId, medousa_tool};

const COGNITION_TOOLS_DISCOVER_ID: ToolId = ToolId::new(COGNITION_TOOLS_DISCOVER);

pub fn register_tool_bootstrap_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    catalog: ToolCatalogHandle,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionToolsDiscoverTool {
        turn_scope,
        catalog,
    })?;
    Ok(())
}

pub struct CognitionToolsDiscoverTool {
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    catalog: ToolCatalogHandle,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum DiscoverLaneInput {
    Host,
    Worker,
    Auto,
}

#[derive(Debug, JsonSchema)]
pub struct ToolsDiscoverInput {
    /// Domain id — host: memory|catalog|runtime|vault|history|identity|skill|overlay|environment|browser; worker: execute|discover|memory|vault|openshell|scripts
    #[schemars(required, with = "String")]
    domain: Option<String>,
    /// Surface lane (default auto from active turn scope)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "DiscoverLaneInput",
        skip_serializing_if = "Option::is_none"
    )]
    lane: Option<DiscoverLaneInput>,
    /// Session id (defaults to active turn session)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// If true, return catalog without unlocking
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    list_only: Option<bool>,
}

impl<'de> Deserialize<'de> for ToolsDiscoverInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            domain: CompatOption<String>,
            #[serde(default)]
            lane: CompatOption<String>,
            #[serde(default)]
            session_id: CompatOption<String>,
            #[serde(default)]
            list_only: CompatOption<bool>,
        }

        let input = WireInput::deserialize(deserializer)?;
        let lane = input
            .lane
            .into_option()
            .map(|lane| match lane.trim().to_ascii_lowercase().as_str() {
                "worker" => DiscoverLaneInput::Worker,
                "host" => DiscoverLaneInput::Host,
                _ => DiscoverLaneInput::Auto,
            });
        Ok(Self {
            domain: input.domain.into_option(),
            lane,
            session_id: input.session_id.into_option(),
            list_only: input.list_only.into_option(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_wire_optionals_remain_lenient_for_legacy_values() {
        let input: ToolsDiscoverInput = serde_json::from_value(serde_json::json!({
            "domain": false,
            "lane": 42,
            "session_id": [],
            "list_only": "true",
        }))
        .expect("discover input");
        assert!(input.domain.is_none());
        assert!(input.lane.is_none());
        assert!(input.session_id.is_none());
        assert!(input.list_only.is_none());
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DiscoverLaneOutput {
    Host,
    Worker,
}

impl From<ToolSurfaceLane> for DiscoverLaneOutput {
    fn from(value: ToolSurfaceLane) -> Self {
        match value {
            ToolSurfaceLane::Host => Self::Host,
            ToolSurfaceLane::Worker => Self::Worker,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DomainCatalogSummary {
    domain: String,
    summary: String,
    unlocked: bool,
    tool_count: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DomainToolSummary {
    name: String,
    summary: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DomainUnlockCatalog {
    domain: String,
    summary: String,
    tools: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ToolsDiscoverOutput {
    Domains {
        ok: bool,
        session_id: String,
        lane: DiscoverLaneOutput,
        bootstrap_tools: Vec<String>,
        domains: Vec<DomainCatalogSummary>,
        unlocked_domains: Vec<String>,
        hint: String,
    },
    Detail {
        ok: bool,
        session_id: String,
        domain: String,
        summary: String,
        unlocked: bool,
        tools: Vec<DomainToolSummary>,
    },
    Error {
        ok: bool,
        error: String,
    },
    Unlocked {
        ok: bool,
        session_id: String,
        lane: DiscoverLaneOutput,
        domain: String,
        unlocked_domains: Vec<String>,
        tools_unlocked: Vec<String>,
        catalog: Option<DomainUnlockCatalog>,
        bootstrap_tools: Vec<String>,
        message: String,
    },
}

#[medousa_tool(id = COGNITION_TOOLS_DISCOVER_ID)]
impl CognitionToolsDiscoverTool {
    /// Unlock a tool domain for this session and return its catalog. Host: memory + vault auto-unlock at session start; environment auto-unlocks on UI-capable clients (Home). Other host domains: catalog, runtime, history, identity, skill, overlay, environment. Worker domains: execute, discover, memory, vault, openshell, scripts. Bootstrap tools stay visible without discover.
    async fn invoke_typed(
        &self,
        input: ToolsDiscoverInput,
    ) -> stasis::prelude::Result<ToolsDiscoverOutput> {
        let session_id =
            crate::runtime_session::require_active_chat_session_id(
                input.session_id.as_deref(),
                &self.turn_scope,
                "cognition_tools_discover",
            )
            .await?;
        let lane = resolve_lane(&self.turn_scope, input.lane);
        let list_only = input.list_only.unwrap_or(false);

        if input
            .domain
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Ok(list_domains_catalog(&session_id, lane));
        }

        let domain = input
            .domain
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();

        if list_only {
            return Ok(domain_detail(
                &session_id,
                lane,
                domain,
                &host_bus_tool_names(),
                &self.catalog,
            ));
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
            .map(|entry| DomainUnlockCatalog {
                domain: entry.domain.to_string(),
                summary: entry.summary.to_string(),
                tools: entry.tools.iter().map(|tool| (*tool).to_string()).collect(),
            });

        Ok(ToolsDiscoverOutput::Unlocked {
            ok: true,
            session_id,
            lane: lane.into(),
            domain: domain.to_ascii_lowercase(),
            unlocked_domains: surface.unlocked_domains,
            tools_unlocked: tools.clone(),
            catalog,
            bootstrap_tools: bootstrap_tools(lane)
                .iter()
                .map(|tool| (*tool).to_string())
                .collect(),
            message: format!(
                "Unlocked domain '{}' for session — {} tools now on surface",
                domain.to_ascii_lowercase(),
                tools.len()
            ),
        })
    }
}

fn list_domains_catalog(session_id: &str, lane: ToolSurfaceLane) -> ToolsDiscoverOutput {
    let surface = load_session_tool_surface(session_id);
    let domains = domain_catalog(lane)
        .iter()
        .map(|entry| DomainCatalogSummary {
            domain: entry.domain.to_string(),
            summary: entry.summary.to_string(),
            unlocked: surface.unlocked_domains.iter().any(|domain| domain == entry.domain),
            tool_count: entry.tools.len(),
        })
        .collect();
    ToolsDiscoverOutput::Domains {
        ok: true,
        session_id: session_id.to_string(),
        lane: lane.into(),
        bootstrap_tools: bootstrap_tools(lane)
            .iter()
            .map(|tool| (*tool).to_string())
            .collect(),
        domains,
        unlocked_domains: surface.unlocked_domains,
        hint: "Call with domain=memory|catalog|runtime|… to unlock a group for this session."
            .to_string(),
    }
}

fn domain_detail(
    session_id: &str,
    lane: ToolSurfaceLane,
    domain: &str,
    allowlist: &HashSet<String>,
    catalog: &ToolCatalogHandle,
) -> ToolsDiscoverOutput {
    let normalized = domain.trim().to_ascii_lowercase();
    let entry = domain_catalog(lane)
        .iter()
        .find(|entry| entry.domain == normalized);
    let Some(entry) = entry else {
        return ToolsDiscoverOutput::Error {
            ok: false,
            error: format!("unknown domain: {domain}"),
        };
    };
    let tools = entry
        .tools
        .iter()
        .filter(|name| tool_allowed(name, allowlist))
        .map(|name| DomainToolSummary {
            name: (*name).to_string(),
            summary: catalog.presentation_summary_for_wire(name),
        })
        .collect();
    let surface = load_session_tool_surface(session_id);
    ToolsDiscoverOutput::Detail {
        ok: true,
        session_id: session_id.to_string(),
        domain: entry.domain.to_string(),
        summary: entry.summary.to_string(),
        unlocked: surface
            .unlocked_domains
            .iter()
            .any(|domain| domain == entry.domain),
        tools,
    }
}

fn resolve_lane(
    turn_scope: &Arc<RwLock<Option<TurnContinuationScope>>>,
    lane: Option<DiscoverLaneInput>,
) -> ToolSurfaceLane {
    match lane {
        Some(DiscoverLaneInput::Worker) => return ToolSurfaceLane::Worker,
        Some(DiscoverLaneInput::Host) => return ToolSurfaceLane::Host,
        Some(DiscoverLaneInput::Auto) | None => {}
    }
    if let Ok(scope) = turn_scope.try_read()
        && scope.is_none() {
            // Worker loops may run without host scope — caller should pass lane=worker.
        }
    ToolSurfaceLane::Host
}
