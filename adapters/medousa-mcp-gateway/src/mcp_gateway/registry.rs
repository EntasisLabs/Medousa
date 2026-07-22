//! MCP server registry, live catalog refresh, and invoke orchestration.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tokio::time::Duration;
use uuid::Uuid;

use medousa_types::mcp_gateway_api::{McpCatalogSyncEntry, McpCatalogSyncResponse};
use crate::mcp_gateway::catalog::{auto_tag_capabilities, discover_from_entries, mock_tool_catalog};
use crate::mcp_gateway::policy_client::DaemonPolicyClient;
use crate::mcp_gateway::remote_client::{RemoteMcpSession, RemoteTransport};
use crate::mcp_gateway::server_config::{McpGatewayFullConfig, McpServerConfig};
use crate::mcp_gateway::stdio_client::StdioMcpSession;
use medousa_types::mcp_gateway_api::{
    McpEffectClass, McpInvokeError, McpInvokeRequest, McpInvokeResponse, McpPolicyEvaluateRequest,
    McpServerSummary, McpServersResponse, McpToolCatalogEntry, McpTurnLane,
};
use medousa_types::mcp_turn_token::verify_mcp_turn_token;

#[derive(Debug, Clone)]
pub struct ServerRuntimeStatus {
    pub server_id: String,
    pub title: String,
    pub enabled: bool,
    pub connected: bool,
    pub tool_count: usize,
    pub allowed_lanes: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    pub tools: Vec<McpToolCatalogEntry>,
    pub servers: Vec<ServerRuntimeStatus>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Clone)]
pub struct ServerRegistry {
    config: Arc<McpGatewayFullConfig>,
    policy_client: DaemonPolicyClient,
    snapshot: Arc<RwLock<CatalogSnapshot>>,
}

impl ServerRegistry {
    pub fn new(config: Arc<McpGatewayFullConfig>) -> Self {
        let policy_client = DaemonPolicyClient::new(
            config.daemon_policy_url.clone(),
            config.policy_token.clone(),
        );
        
        Self {
            config,
            policy_client,
            snapshot: Arc::new(RwLock::new(CatalogSnapshot {
                tools: mock_tool_catalog(),
                servers: Vec::new(),
                updated_at: Utc::now(),
            })),
        }
    }

    pub async fn bootstrap(&self) {
        let _ = self.refresh_catalog().await;
    }

    pub fn spawn_refresh_loop(self: Arc<Self>) {
        let interval_secs = self.config.catalog_refresh_interval_secs.max(30);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                ticker.tick().await;
                if let Err(error) = self.refresh_catalog().await {
                    eprintln!("medousa-mcp-gateway catalog refresh failed: {error:#}");
                }
            }
        });
    }

    pub async fn refresh_catalog(&self) -> Result<()> {
        let mut tools = Vec::new();
        let mut servers = Vec::new();
        let timeout = Duration::from_millis(self.config.max_invoke_duration_ms.max(1_000));

        for server in &self.config.servers {
            if !server.enabled {
                servers.push(status_from_config(
                    server,
                    false,
                    0,
                    Some("server disabled".to_string()),
                ));
                continue;
            }

            if server_unconfigured(server) {
                let mock_tools: Vec<_> = mock_tool_catalog()
                    .into_iter()
                    .filter(|tool| tool.server_id == server.id)
                    .collect();
                let count = mock_tools.len();
                tools.extend(mock_tools);
                servers.push(status_from_config(
                    server,
                    count > 0,
                    count,
                    if count == 0 {
                        Some("mock catalog unavailable for server".to_string())
                    } else {
                        None
                    },
                ));
                continue;
            }

            match list_tools_for_server(server, timeout).await {
                Ok(live_tools) => {
                    let count = live_tools.len();
                    tools.extend(live_tools);
                    servers.push(status_from_config(server, true, count, None));
                }
                Err(error) => {
                    let message = error.to_string();
                    if self.config.use_mock_fallback {
                        let mock_tools: Vec<_> = mock_tool_catalog()
                            .into_iter()
                            .filter(|tool| tool.server_id == server.id)
                            .map(|mut tool| {
                                tool.stability = "mock_fallback".to_string();
                                tool
                            })
                            .collect();
                        let count = mock_tools.len();
                        tools.extend(mock_tools);
                        servers.push(status_from_config(
                            server,
                            false,
                            count,
                            Some(message),
                        ));
                    } else {
                        servers.push(status_from_config(server, false, 0, Some(message)));
                    }
                }
            }
        }

        if self.config.servers.is_empty() {
            tools = mock_tool_catalog();
        }

        dedupe_tools(&mut tools);
        let mut snapshot = self.snapshot.write().await;
        snapshot.tools = tools;
        snapshot.servers = servers;
        snapshot.updated_at = Utc::now();
        Ok(())
    }

    pub async fn catalog_sync(&self) -> McpCatalogSyncResponse {
        let snapshot = self.snapshot.read().await;
        McpCatalogSyncResponse {
            entries: snapshot
                .tools
                .iter()
                .map(|tool| {
                    let available = snapshot.servers.iter().any(|server| {
                        server.server_id == tool.server_id && (server.connected || server.tool_count > 0)
                    }) || self.config.servers.is_empty();
                    McpCatalogSyncEntry {
                        server_id: tool.server_id.clone(),
                        tool_name: tool.tool_name.clone(),
                        title: tool.title.clone(),
                        capability_ids: tool.capability_ids.clone(),
                        available,
                        unavailable_reason: snapshot
                            .servers
                            .iter()
                            .find(|server| server.server_id == tool.server_id)
                            .and_then(|server| server.last_error.clone()),
                    }
                })
                .collect(),
            now_utc: snapshot.updated_at,
        }
    }

    pub async fn discover(
        &self,
        query: &str,
        server_id: Option<&str>,
        limit: usize,
    ) -> Vec<McpToolCatalogEntry> {
        let snapshot = self.snapshot.read().await;
        discover_from_entries(&snapshot.tools, query, server_id, limit)
    }

    pub async fn list_servers(&self) -> McpServersResponse {
        let snapshot = self.snapshot.read().await;
        McpServersResponse {
            servers: snapshot
                .servers
                .iter()
                .map(|server| McpServerSummary {
                    server_id: server.server_id.clone(),
                    title: server.title.clone(),
                    enabled: server.enabled,
                    connected: server.connected,
                    tool_count: server.tool_count,
                    allowed_lanes: server.allowed_lanes.clone(),
                })
                .collect(),
        }
    }

    pub async fn invoke(
        &self,
        request: McpInvokeRequest,
        invokes_enabled: bool,
    ) -> McpInvokeResponse {
        let started = Instant::now();
        let invoke_id = format!("inv_{}", Uuid::new_v4());
        let fail = |code: &str, message: String, retryable: bool| -> McpInvokeResponse {
            McpInvokeResponse {
                invoke_id: invoke_id.clone(),
                server_id: request.server_id.clone(),
                tool_name: request.tool_name.clone(),
                ok: false,
                output: None,
                error: Some(McpInvokeError {
                    code: code.to_string(),
                    message,
                    retryable: Some(retryable),
                }),
                duration_ms: started.elapsed().as_millis() as u64,
                effect_class: McpEffectClass::ExternalRead,
            }
        };

        if !invokes_enabled {
            return fail("invokes_disabled", "MCP invokes are disabled".to_string(), false);
        }

        if let Some(token) = request.turn_token.as_deref() {
            if let Err(error) = verify_mcp_turn_token(token, &request.turn_context) {
                return fail("invalid_turn_token", error.to_string(), false);
            }
        } else if medousa_types::mcp_turn_token::resolve_mcp_turn_token_secret().is_some() {
            return fail(
                "missing_turn_token",
                "turn token required for MCP invoke".to_string(),
                false,
            );
        }

        let Some(server) = self.config.server_by_id(&request.server_id) else {
            return fail(
                "unknown_server",
                format!("unknown MCP server '{}'", request.server_id),
                false,
            );
        };

        if !server.enabled {
            return fail(
                "server_disabled",
                format!("MCP server '{}' is disabled", server.id),
                false,
            );
        }

        if !lane_allowed(server, request.turn_context.lane) {
            return fail(
                "lane_denied",
                format!(
                    "lane '{}' not allowed for server '{}'",
                    request.turn_context.lane.as_str(),
                    server.id
                ),
                false,
            );
        }

        let snapshot = self.snapshot.read().await;
        let Some(tool) = snapshot.tools.iter().find(|tool| {
            tool.server_id.eq_ignore_ascii_case(&request.server_id)
                && tool.tool_name.eq_ignore_ascii_case(&request.tool_name)
        }) else {
            return fail(
                "unknown_tool",
                format!(
                    "tool '{}.{}' not found in catalog",
                    request.server_id, request.tool_name
                ),
                false,
            );
        };

        if !effect_allowed(server, tool.effect_class) {
            return fail(
                "effect_denied",
                format!(
                    "effect '{}' not allowed for server '{}'",
                    tool.effect_class.as_str(),
                    server.id
                ),
                false,
            );
        }

        let policy_request = McpPolicyEvaluateRequest {
            action: "mcp.invoke".to_string(),
            server_id: request.server_id.clone(),
            tool_name: request.tool_name.clone(),
            effect_class: tool.effect_class,
            turn_context: request.turn_context.clone(),
            operator_approval_granted: request.operator_approval_granted,
        };

        match self.policy_client.evaluate(&policy_request).await {
            Ok(policy) if policy.allowed => {}
            Ok(policy) => {
                return fail(
                    if policy.approval_required {
                        "approval_required"
                    } else {
                        "policy_denied"
                    },
                    policy.reason,
                    false,
                );
            }
            Err(error) => {
                return fail(
                    "policy_unreachable",
                    format!("daemon policy evaluate failed: {error:#}"),
                    true,
                );
            }
        }

        let timeout = Duration::from_millis(self.config.max_invoke_duration_ms.max(1_000));
        match execute_invoke(server, &request.tool_name, request.input.clone(), timeout).await {
            Ok(output) => McpInvokeResponse {
                invoke_id,
                server_id: request.server_id,
                tool_name: request.tool_name,
                ok: true,
                output: Some(output),
                error: None,
                duration_ms: started.elapsed().as_millis() as u64,
                effect_class: tool.effect_class,
            },
            Err(error) => fail("invoke_failed", error.to_string(), true),
        }
    }

    pub async fn health_stats(&self) -> (usize, usize, usize) {
        let snapshot = self.snapshot.read().await;
        let registered = snapshot.servers.len();
        let connected = snapshot
            .servers
            .iter()
            .filter(|server| server.connected)
            .count();
        (registered, connected, snapshot.tools.len())
    }
}

async fn list_tools_for_server(
    server: &McpServerConfig,
    timeout: Duration,
) -> Result<Vec<McpToolCatalogEntry>> {
    let tools = match remote_transport(server) {
        Some(transport) => list_tools_from_remote(server, transport, timeout).await?,
        None => list_tools_from_stdio(server, timeout).await?,
    };
    Ok(tools
        .into_iter()
        .map(|tool| tool_entry_from_definition(server, tool))
        .collect())
}

async fn list_tools_from_stdio(
    server: &McpServerConfig,
    timeout: Duration,
) -> Result<Vec<crate::mcp_gateway::stdio_client::McpToolDefinition>> {
    let command = server
        .command
        .as_deref()
        .context("stdio server missing command")?;
    let mut session = StdioMcpSession::spawn(command, &server.args, timeout).await?;
    session.list_tools().await
}

async fn list_tools_from_remote(
    server: &McpServerConfig,
    transport: RemoteTransport,
    timeout: Duration,
) -> Result<Vec<crate::mcp_gateway::stdio_client::McpToolDefinition>> {
    let url = server
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("remote MCP server missing url")?;
    let bearer_token = server
        .bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut session = RemoteMcpSession::connect(url, transport, bearer_token, timeout).await?;
    session.list_tools().await
}

fn tool_entry_from_definition(
    server: &McpServerConfig,
    tool: crate::mcp_gateway::stdio_client::McpToolDefinition,
) -> McpToolCatalogEntry {
    let effect_class = infer_effect_class(&tool.name, tool.description.as_deref());
    let capability_ids = server
        .tool_tags
        .get(&tool.name)
        .cloned()
        .unwrap_or_else(|| auto_tag_capabilities(&tool.name, tool.description.as_deref()));
    McpToolCatalogEntry {
        server_id: server.id.clone(),
        server_title: server.title.clone(),
        tool_name: tool.name.clone(),
        title: tool.title,
        description: tool.description,
        input_schema_summary: tool
            .input_schema
            .as_ref()
            .map(|schema| schema.to_string()),
        effect_class,
        capability_ids,
        stability: "live".to_string(),
    }
}

async fn execute_invoke(
    server: &McpServerConfig,
    tool_name: &str,
    input: Value,
    timeout: Duration,
) -> Result<Value> {
    if server_unconfigured(server) {
        return Ok(json!({
            "mock": true,
            "server_id": server.id,
            "tool_name": tool_name,
            "input": input,
            "message": "mock MCP invoke (configure server command or url for live MCP)"
        }));
    }

    if let Some(transport) = remote_transport(server) {
        let url = server
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("remote MCP server missing url")?;
        let bearer_token = server
            .bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let mut session = RemoteMcpSession::connect(url, transport, bearer_token, timeout).await?;
        return session.call_tool(tool_name, input).await;
    }

    let command = server
        .command
        .as_deref()
        .context("stdio server missing command")?;
    let mut session = StdioMcpSession::spawn(command, &server.args, timeout).await?;
    session.call_tool(tool_name, input).await
}

fn remote_transport(server: &McpServerConfig) -> Option<RemoteTransport> {
    RemoteTransport::parse(&server.transport)
}

fn server_unconfigured(server: &McpServerConfig) -> bool {
    if server.use_mock {
        return true;
    }
    if remote_transport(server).is_some() {
        return server
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();
    }
    server.command.as_deref().unwrap_or("").trim().is_empty()
}

fn status_from_config(
    server: &McpServerConfig,
    connected: bool,
    tool_count: usize,
    last_error: Option<String>,
) -> ServerRuntimeStatus {
    ServerRuntimeStatus {
        server_id: server.id.clone(),
        title: server.title.clone(),
        enabled: server.enabled,
        connected,
        tool_count,
        allowed_lanes: server.allowed_lanes.clone(),
        last_error,
    }
}

fn lane_allowed(server: &McpServerConfig, lane: McpTurnLane) -> bool {
    server
        .allowed_lanes
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(lane.as_str()))
}

fn effect_allowed(server: &McpServerConfig, effect: McpEffectClass) -> bool {
    server
        .allowed_effect_classes
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(effect.as_str()))
}

pub fn infer_effect_class(tool_name: &str, description: Option<&str>) -> McpEffectClass {
    let corpus = format!(
        "{} {}",
        tool_name.to_ascii_lowercase(),
        description.unwrap_or("").to_ascii_lowercase()
    );
    if corpus.contains("send")
        || corpus.contains("post")
        || corpus.contains("email")
        || corpus.contains("delete")
        || corpus.contains("charge")
    {
        return McpEffectClass::ExternalSideEffect;
    }
    if corpus.contains("create")
        || corpus.contains("update")
        || corpus.contains("write")
        || corpus.contains("insert")
    {
        return McpEffectClass::ExternalWrite;
    }
    McpEffectClass::ExternalRead
}

fn dedupe_tools(tools: &mut Vec<McpToolCatalogEntry>) {
    let mut seen = HashSet::new();
    tools.retain(|tool| {
        seen.insert(format!("{}.{}", tool.server_id, tool.tool_name))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_gateway::server_config::McpServerConfig;
    use std::sync::Arc;

    fn test_registry() -> ServerRegistry {
        let config = Arc::new(McpGatewayFullConfig {
            bind: "127.0.0.1:7420".to_string(),
            gateway_token: None,
            admin_token: None,
            invokes_enabled: true,
            daemon_policy_url: "http://127.0.0.1:7419/v1/mcp/policy/evaluate".to_string(),
            policy_token: None,
            max_invoke_duration_ms: 5_000,
            catalog_refresh_interval_secs: 300,
            use_mock_fallback: true,
            servers: vec![McpServerConfig {
                id: "notion".to_string(),
                title: "Notion MCP".to_string(),
                enabled: true,
                transport: "stdio".to_string(),
                command: None,
                args: Vec::new(),
                url: None,
                bearer_token: None,
                allowed_lanes: vec!["interactive".to_string()],
                allowed_effect_classes: vec!["external_read".to_string()],
                tool_tags: Default::default(),
                use_mock: true,
            }],
        });
        ServerRegistry::new(config)
    }

    #[tokio::test]
    async fn bootstrap_loads_mock_notion_tools() {
        let registry = test_registry();
        registry.bootstrap().await;
        let tools = registry.discover("notion", None, 10).await;
        assert!(tools.iter().any(|tool| tool.tool_name == "search_pages"));
    }
}
