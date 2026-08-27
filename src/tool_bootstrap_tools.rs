//! `cognition_tools_discover` — inspect the session tool-domain catalog.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use crate::tool_bootstrap::{
    COGNITION_TOOLS_DISCOVER, ToolSurfaceLane, bootstrap_tools, discover_session_domain,
    domain_catalog,
};
use crate::typed_tools::{CompatOption, ToolCatalogHandle, ToolId, medousa_tool};

const COGNITION_TOOLS_DISCOVER_ID: ToolId = ToolId::new(COGNITION_TOOLS_DISCOVER);

pub fn register_tool_bootstrap_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    catalog: ToolCatalogHandle,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionToolsDiscoverTool {
        turn_scope,
        catalog,
    })?;
    Ok(())
}

pub struct CognitionToolsDiscoverTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
    #[schemars(with = "DiscoverLaneInput", skip_serializing_if = "Option::is_none")]
    lane: Option<DiscoverLaneInput>,
    /// Session id (defaults to active turn session)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
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
        let lane =
            input
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
    tool_count: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DomainToolSummary {
    name: String,
    summary: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ToolsDiscoverOutput {
    Domains {
        ok: bool,
        session_id: String,
        lane: DiscoverLaneOutput,
        common_tools: Vec<String>,
        domains: Vec<DomainCatalogSummary>,
    },
    Inspected {
        ok: bool,
        session_id: String,
        lane: DiscoverLaneOutput,
        domain: String,
        summary: String,
        tools: Vec<DomainToolSummary>,
        message: String,
    },
}

#[medousa_tool(id = COGNITION_TOOLS_DISCOVER_ID)]
impl CognitionToolsDiscoverTool {
    /// List tools in a domain with concise usage summaries.
    async fn invoke_typed(
        &self,
        input: ToolsDiscoverInput,
    ) -> stasis::prelude::Result<ToolsDiscoverOutput> {
        let session_id = crate::runtime_session::require_active_chat_session_id(
            input.session_id.as_deref(),
            &self.turn_scope,
            "cognition_tools_discover",
        )
        .await?;
        let lane = resolve_lane(&self.turn_scope, input.lane);
        let _list_only = input.list_only.unwrap_or(false);

        if input
            .domain
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Ok(list_domains_catalog(&session_id, lane));
        }

        let domain = input.domain.as_deref().map(str::trim).unwrap_or_default();

        let (_surface, tool_names) =
            discover_session_domain(&session_id, lane, domain).map_err(StasisError::PortFailure)?;

        let entry = domain_catalog(lane)
            .iter()
            .find(|entry| entry.domain == domain.to_ascii_lowercase())
            .expect("discover_session_domain validated the domain");
        let tools = tool_names
            .iter()
            .map(|name| DomainToolSummary {
                name: name.clone(),
                summary: self.catalog.presentation_summary_for_wire(name),
            })
            .collect();

        Ok(ToolsDiscoverOutput::Inspected {
            ok: true,
            session_id,
            lane: lane.into(),
            domain: domain.to_ascii_lowercase(),
            summary: entry.summary.to_string(),
            tools,
            message: format!(
                "Inspected domain '{}' — {} catalogued tools",
                domain.to_ascii_lowercase(),
                tool_names.len()
            ),
        })
    }
}

fn list_domains_catalog(session_id: &str, lane: ToolSurfaceLane) -> ToolsDiscoverOutput {
    let domains = domain_catalog(lane)
        .iter()
        .map(|entry| DomainCatalogSummary {
            domain: entry.domain.to_string(),
            summary: entry.summary.to_string(),
            tool_count: entry.tools.len(),
        })
        .collect();
    ToolsDiscoverOutput::Domains {
        ok: true,
        session_id: session_id.to_string(),
        lane: lane.into(),
        common_tools: bootstrap_tools(lane)
            .iter()
            .map(|tool| (*tool).to_string())
            .collect(),
        domains,
    }
}

fn resolve_lane(
    _turn_scope: &crate::agent_runtime::execution_context::TurnScopeAccess,
    lane: Option<DiscoverLaneInput>,
) -> ToolSurfaceLane {
    match lane {
        Some(DiscoverLaneInput::Worker) => return ToolSurfaceLane::Worker,
        Some(DiscoverLaneInput::Host) => return ToolSurfaceLane::Host,
        Some(DiscoverLaneInput::Auto) | None => {}
    }
    if crate::agent_runtime::execution_context::active_turn_execution_context().is_none() {
        // Worker loops may run without host scope — caller should pass lane=worker.
    }
    ToolSurfaceLane::Host
}
