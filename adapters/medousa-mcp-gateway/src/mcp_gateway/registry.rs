//! MCP server registry, live catalog refresh, and invoke orchestration.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::Duration;
use uuid::Uuid;

use crate::mcp_gateway::catalog::{
    auto_tag_capabilities, discover_from_entries, mock_tool_catalog,
};
use crate::mcp_gateway::connection_actor::ConnectionActorHandle;
use crate::mcp_gateway::oauth::{McpOAuthBroker, McpOAuthError};
use crate::mcp_gateway::policy_client::{DaemonPolicyClient, McpPolicyEvaluator};
use crate::mcp_gateway::remote_client::RemoteTransport;
use crate::mcp_gateway::server_config::{McpGatewayFullConfig, McpServerConfig};
use medousa_types::mcp_gateway_api::{
    BeginMcpOAuthRequest, BeginMcpOAuthResponse, CompleteMcpOAuthResponse,
    DisconnectMcpOAuthResponse, McpEffectClass, McpInvokeError, McpInvokeRequest,
    McpInvokeResponse, McpOAuthStatusResponse, McpPolicyEvaluateRequest, McpServerSummary,
    McpServersResponse, McpToolAnnotations, McpToolCatalogEntry, McpTurnLane,
};
use medousa_types::mcp_gateway_api::{McpCatalogSyncEntry, McpCatalogSyncResponse};
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
    policy: Arc<dyn McpPolicyEvaluator>,
    oauth: Option<Arc<McpOAuthBroker>>,
    snapshot: Arc<RwLock<CatalogSnapshot>>,
    connections: Arc<Mutex<HashMap<String, ConnectionActorHandle>>>,
    catalog_changes: mpsc::UnboundedSender<String>,
    catalog_change_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<String>>>>,
}

impl ServerRegistry {
    pub fn new(config: Arc<McpGatewayFullConfig>) -> Self {
        let policy = Arc::new(DaemonPolicyClient::new(
            config.daemon_policy_url.clone(),
            config.policy_token.clone(),
        ));

        Self::with_policy_evaluator(config, policy)
    }

    pub fn with_policy_evaluator(
        config: Arc<McpGatewayFullConfig>,
        policy: Arc<dyn McpPolicyEvaluator>,
    ) -> Self {
        let (catalog_changes, catalog_change_rx) = mpsc::unbounded_channel();
        Self {
            config,
            policy,
            oauth: None,
            snapshot: Arc::new(RwLock::new(CatalogSnapshot {
                tools: mock_tool_catalog(),
                servers: Vec::new(),
                updated_at: Utc::now(),
            })),
            connections: Arc::new(Mutex::new(HashMap::new())),
            catalog_changes,
            catalog_change_rx: Arc::new(Mutex::new(Some(catalog_change_rx))),
        }
    }

    pub fn with_oauth(mut self, oauth: Arc<McpOAuthBroker>) -> Self {
        self.oauth = Some(oauth);
        self
    }

    pub async fn bootstrap(&self) {
        let _ = self.refresh_catalog().await;
    }

    pub fn spawn_refresh_loop(self: Arc<Self>) {
        let interval_secs = self.config.catalog_refresh_interval_secs.max(30);
        let registry = Arc::downgrade(&self);
        let change_rx = self.catalog_change_rx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
            let mut catalog_changes = change_rx.lock().await.take();
            loop {
                if let Some(changes) = catalog_changes.as_mut() {
                    tokio::select! {
                        _ = ticker.tick() => {}
                        change = changes.recv() => {
                            if change.is_none() {
                                catalog_changes = None;
                            } else {
                                // Coalesce bursts from servers that update several tools at once.
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                while changes.try_recv().is_ok() {}
                            }
                        }
                    }
                } else {
                    ticker.tick().await;
                }
                let Some(registry) = registry.upgrade() else {
                    break;
                };
                if let Err(error) = registry.refresh_catalog().await {
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
                    .map(|tool| configure_tool_entry(server, tool))
                    .collect();
                let connected = !mock_tools.is_empty();
                let count = enabled_tool_count(server, &mock_tools);
                tools.extend(mock_tools);
                servers.push(status_from_config(
                    server,
                    connected,
                    count,
                    if connected {
                        None
                    } else {
                        Some("mock catalog unavailable for server".to_string())
                    },
                ));
                continue;
            }

            let live_tools = match self.remote_bearer_token(server).await {
                Ok(bearer_token) => {
                    self.list_tools_for_server(server, bearer_token, timeout)
                        .await
                }
                Err(error) => Err(error),
            };
            match live_tools {
                Ok(live_tools) => {
                    let count = enabled_tool_count(server, &live_tools);
                    tools.extend(live_tools);
                    servers.push(status_from_config(server, true, count, None));
                }
                Err(error) => {
                    let message = classify_runtime_error(server, "discover tools", &error).message;
                    if self.config.use_mock_fallback {
                        let mock_tools: Vec<_> = mock_tool_catalog()
                            .into_iter()
                            .filter(|tool| tool.server_id == server.id)
                            .map(|mut tool| {
                                tool.stability = "mock_fallback".to_string();
                                configure_tool_entry(server, tool)
                            })
                            .collect();
                        let count = enabled_tool_count(server, &mock_tools);
                        tools.extend(mock_tools);
                        servers.push(status_from_config(server, false, count, Some(message)));
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
                    let runtime = snapshot
                        .servers
                        .iter()
                        .find(|server| server.server_id == tool.server_id);
                    let runtime_available = runtime.is_some_and(|server| {
                        server.server_id == tool.server_id
                            && (server.connected || server.tool_count > 0)
                    }) || self.config.servers.is_empty();
                    let tool_enabled = self
                        .config
                        .server_by_id(&tool.server_id)
                        .is_none_or(|server| server.tool_enabled(&tool.tool_name));
                    let available = tool_enabled && runtime_available;
                    McpCatalogSyncEntry {
                        server_id: tool.server_id.clone(),
                        tool_name: tool.tool_name.clone(),
                        title: tool.title.clone(),
                        capability_ids: tool.capability_ids.clone(),
                        available,
                        unavailable_reason: if tool_enabled {
                            runtime.and_then(|server| server.last_error.clone())
                        } else {
                            Some("tool disabled".to_string())
                        },
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
        let enabled_tools = snapshot
            .tools
            .iter()
            .filter(|tool| {
                self.config
                    .server_by_id(&tool.server_id)
                    .is_none_or(|server| server.tool_enabled(&tool.tool_name))
            })
            .cloned()
            .collect::<Vec<_>>();
        discover_from_entries(&enabled_tools, query, server_id, limit)
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
                    last_error: server.last_error.clone(),
                })
                .collect(),
        }
    }

    pub async fn oauth_status(
        &self,
        server_id: &str,
    ) -> Result<McpOAuthStatusResponse, McpOAuthError> {
        self.server_url(server_id)?;
        self.oauth_broker()?.status(server_id).await
    }

    pub async fn begin_oauth(
        &self,
        request: BeginMcpOAuthRequest,
    ) -> Result<BeginMcpOAuthResponse, McpOAuthError> {
        let server_url = self.server_url(&request.server_id)?.to_string();
        crate::mcp_gateway::server_config::validate_remote_server_url(&server_url, true).map_err(
            |error| McpOAuthError::OAuth(rmcp::transport::AuthError::AuthorizationFailed(error)),
        )?;
        self.oauth_broker()?
            .begin(crate::mcp_gateway::oauth::McpOAuthBeginRequest {
                server_id: request.server_id,
                server_url,
                redirect_uri: request.redirect_uri,
                scopes: request.scopes,
                client_metadata_url: request.client_metadata_url,
                client_id: request.client_id,
                client_secret: request.client_secret,
                challenge: request.challenge,
            })
            .await
    }

    pub async fn complete_oauth(
        &self,
        login_id: &str,
        callback_url: &str,
    ) -> Result<CompleteMcpOAuthResponse, McpOAuthError> {
        let response = self
            .oauth_broker()?
            .complete(login_id, callback_url)
            .await?;
        self.invalidate_server_connection(&response.connection.server_id)
            .await;
        let _ = self.refresh_catalog().await;
        Ok(response)
    }

    pub async fn refresh_oauth(
        &self,
        server_id: &str,
    ) -> Result<McpOAuthStatusResponse, McpOAuthError> {
        let server_url = self.server_url(server_id)?.to_string();
        let response = self.oauth_broker()?.refresh(server_id, &server_url).await?;
        self.invalidate_server_connection(server_id).await;
        let _ = self.refresh_catalog().await;
        Ok(response)
    }

    pub async fn disconnect_oauth(
        &self,
        server_id: &str,
    ) -> Result<DisconnectMcpOAuthResponse, McpOAuthError> {
        self.server_url(server_id)?;
        let response = self.oauth_broker()?.disconnect(server_id).await?;
        self.invalidate_server_connection(server_id).await;
        let _ = self.refresh_catalog().await;
        Ok(response)
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
            return fail(
                "invokes_disabled",
                "MCP invokes are disabled".to_string(),
                false,
            );
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

        if !server.tool_enabled(&request.tool_name) {
            return fail(
                "tool_disabled",
                format!("MCP tool '{}.{}' is disabled", server.id, request.tool_name),
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

        let tool = self
            .snapshot
            .read()
            .await
            .tools
            .iter()
            .find(|tool| {
                tool.server_id.eq_ignore_ascii_case(&request.server_id)
                    && tool.tool_name.eq_ignore_ascii_case(&request.tool_name)
            })
            .cloned();
        let Some(tool) = tool else {
            return fail(
                "unknown_tool",
                format!(
                    "tool '{}.{}' not found in catalog",
                    request.server_id, request.tool_name
                ),
                false,
            );
        };
        let fail_for_tool = |code: &str, message: String, retryable: bool| {
            let mut response = fail(code, message, retryable);
            response.effect_class = tool.effect_class;
            response
        };

        if !effect_allowed(server, tool.effect_class) {
            return fail_for_tool(
                "effect_denied",
                format!(
                    "Effect '{}' is not allowed for server '{}'. {}",
                    tool.effect_class.as_str(),
                    server.id,
                    tool.approval_summary
                        .as_deref()
                        .unwrap_or("Review the server's allowed effects in Settings.")
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

        match self.policy.evaluate(&policy_request).await {
            Ok(policy) if policy.allowed => {}
            Ok(policy) => {
                return fail_for_tool(
                    if policy.approval_required {
                        "approval_required"
                    } else {
                        "policy_denied"
                    },
                    match tool.approval_summary.as_deref() {
                        Some(summary) => format!("{}. {}", policy.reason, summary),
                        None => policy.reason,
                    },
                    false,
                );
            }
            Err(error) => {
                let auth_failed = error.is::<super::policy_client::PolicyAuthenticationError>();
                return fail_for_tool(
                    if auth_failed {
                        "policy_authentication_failed"
                    } else {
                        "policy_unreachable"
                    },
                    format!("daemon policy evaluate failed: {error:#}"),
                    !auth_failed,
                );
            }
        }

        let timeout = Duration::from_millis(self.config.max_invoke_duration_ms.max(1_000));
        let bearer_token = match self.remote_bearer_token(server).await {
            Ok(bearer_token) => bearer_token,
            Err(error) => {
                let failure = classify_runtime_error(server, "authenticate", &error);
                return fail_for_tool(failure.code, failure.message, failure.retryable);
            }
        };
        match self
            .execute_invoke(
                server,
                &request.tool_name,
                request.input.clone(),
                bearer_token,
                timeout,
            )
            .await
        {
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
            Err(error) => {
                let failure = classify_runtime_error(server, "invoke the tool", &error);
                fail_for_tool(failure.code, failure.message, failure.retryable)
            }
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
        let enabled_tools = snapshot
            .tools
            .iter()
            .filter(|tool| {
                self.config
                    .server_by_id(&tool.server_id)
                    .is_none_or(|server| server.tool_enabled(&tool.tool_name))
            })
            .count();
        (registered, connected, enabled_tools)
    }

    async fn remote_bearer_token(&self, server: &McpServerConfig) -> Result<Option<String>> {
        if remote_transport(server).is_none() {
            return Ok(None);
        }

        if let Some(token) = server
            .bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(Some(token.to_string()));
        }

        if server.bearer_token_configured {
            let oauth = self
                .oauth
                .as_ref()
                .context("MCP credential store unavailable")?;
            return oauth
                .bearer_token(&server.id)
                .map_err(anyhow::Error::from)
                .and_then(|token| token.context("configured MCP bearer token is missing"))
                .map(Some);
        }

        let Some(oauth) = self.oauth.as_ref() else {
            return Ok(None);
        };
        let url = server
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("remote MCP server missing url")?;
        match oauth.access_token(&server.id, url).await {
            Ok(token) => Ok(Some(token)),
            Err(McpOAuthError::NotConnected) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn oauth_broker(&self) -> Result<&McpOAuthBroker, McpOAuthError> {
        self.oauth.as_deref().ok_or(McpOAuthError::Unavailable)
    }

    fn server_url(&self, server_id: &str) -> Result<&str, McpOAuthError> {
        let server_id = server_id.trim();
        if server_id.is_empty() {
            return Err(McpOAuthError::InvalidInput("server id"));
        }
        let server = self
            .config
            .server_by_id(server_id)
            .ok_or_else(|| McpOAuthError::ServerNotFound(server_id.to_string()))?;
        if remote_transport(server).is_none() {
            return Err(McpOAuthError::ServerUrlMissing(server.id.clone()));
        }
        server
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| McpOAuthError::ServerUrlMissing(server.id.clone()))
    }

    async fn connection_for(
        &self,
        server: &McpServerConfig,
        timeout: Duration,
    ) -> ConnectionActorHandle {
        let mut connections = self.connections.lock().await;
        connections
            .entry(server.id.to_ascii_lowercase())
            .or_insert_with(|| {
                ConnectionActorHandle::spawn(server.clone(), timeout, self.catalog_changes.clone())
            })
            .clone()
    }

    async fn invalidate_server_connection(&self, server_id: &str) {
        if let Some(connection) = self
            .connections
            .lock()
            .await
            .get(&server_id.to_ascii_lowercase())
            .cloned()
        {
            connection.invalidate().await;
        }
    }

    async fn list_tools_for_server(
        &self,
        server: &McpServerConfig,
        bearer_token: Option<String>,
        timeout: Duration,
    ) -> Result<Vec<McpToolCatalogEntry>> {
        let tools = self
            .connection_for(server, timeout)
            .await
            .list_tools(bearer_token)
            .await?;
        Ok(tools
            .into_iter()
            .map(|tool| tool_entry_from_definition(server, tool))
            .collect())
    }

    async fn execute_invoke(
        &self,
        server: &McpServerConfig,
        tool_name: &str,
        input: Value,
        bearer_token: Option<String>,
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

        self.connection_for(server, timeout)
            .await
            .call_tool(bearer_token, tool_name.to_string(), input)
            .await
    }
}

fn tool_entry_from_definition(
    server: &McpServerConfig,
    tool: crate::mcp_gateway::stdio_client::McpToolDefinition,
) -> McpToolCatalogEntry {
    let effect_class = infer_effect_class_with_annotations(
        &tool.name,
        tool.description.as_deref(),
        tool.annotations.as_ref(),
    );
    let capability_ids = auto_tag_capabilities(&tool.name, tool.description.as_deref());
    let ui_resource_uri = tool
        .meta
        .as_ref()
        .and_then(|meta| meta.get("ui"))
        .and_then(|ui| ui.get("resourceUri"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let annotations = tool.annotations.map(|annotations| McpToolAnnotations {
        read_only_hint: annotations.read_only_hint,
        destructive_hint: annotations.destructive_hint,
        idempotent_hint: annotations.idempotent_hint,
        open_world_hint: annotations.open_world_hint,
    });
    let meta = tool
        .meta
        .map(|meta| Value::Object(meta.into_iter().collect()));
    let planning_hints = planning_hints(effect_class, annotations.as_ref());
    let approval_summary =
        approval_summary(server, &tool.title, effect_class, annotations.as_ref());
    configure_tool_entry(
        server,
        McpToolCatalogEntry {
            server_id: server.id.clone(),
            server_title: server.title.clone(),
            tool_name: tool.name.clone(),
            title: tool.title,
            description: tool.description,
            input_schema_summary: tool.input_schema.as_ref().map(|schema| schema.to_string()),
            output_schema_summary: tool.output_schema.as_ref().map(|schema| schema.to_string()),
            input_schema: tool.input_schema,
            output_schema: tool.output_schema,
            annotations,
            icons: tool.icons,
            meta,
            ui_resource_uri,
            effect_class,
            planning_hints,
            approval_summary: Some(approval_summary),
            capability_ids,
            stability: "live".to_string(),
        },
    )
}

fn planning_hints(
    effect_class: McpEffectClass,
    annotations: Option<&McpToolAnnotations>,
) -> Vec<String> {
    let mut hints = vec![match effect_class {
        McpEffectClass::ExternalRead => {
            "Reads external data without intentionally changing it.".to_string()
        }
        McpEffectClass::ExternalWrite => {
            "Changes external data; confirm the target and payload before invoking.".to_string()
        }
        McpEffectClass::ExternalSideEffect => {
            "May cause a consequential external action and can require operator approval."
                .to_string()
        }
    }];
    if let Some(annotations) = annotations {
        match annotations.idempotent_hint {
            Some(true) => hints.push(
                "The server declares this operation idempotent. Treat that as untrusted guidance and verify before retrying."
                    .to_string(),
            ),
            Some(false) => hints.push(
                "The server declares this operation non-idempotent; never replay it automatically."
                    .to_string(),
            ),
            None => hints.push(
                "Idempotence is unknown; do not replay an ambiguous failed invocation automatically."
                    .to_string(),
            ),
        }
        match annotations.open_world_hint {
            Some(true) => hints.push(
                "The server says this tool may reach beyond its connected service into the open world."
                    .to_string(),
            ),
            Some(false) => hints.push(
                "The server says this tool stays within its connected service.".to_string(),
            ),
            None => {}
        }
    } else {
        hints.push(
            "Idempotence is unknown; do not replay an ambiguous failed invocation automatically."
                .to_string(),
        );
    }
    hints
}

fn approval_summary(
    server: &McpServerConfig,
    tool_title: &str,
    effect_class: McpEffectClass,
    annotations: Option<&McpToolAnnotations>,
) -> String {
    let impact = if annotations.is_some_and(|value| value.destructive_hint == Some(true)) {
        "a potentially destructive external action"
    } else {
        match effect_class {
            McpEffectClass::ExternalRead => "an external read",
            McpEffectClass::ExternalWrite => "an external change",
            McpEffectClass::ExternalSideEffect => "an external side effect",
        }
    };
    format!("Allow {} to run ‘{}’ ({impact})", server.title, tool_title)
}

fn configure_tool_entry(
    server: &McpServerConfig,
    mut tool: McpToolCatalogEntry,
) -> McpToolCatalogEntry {
    tool.server_id = server.id.clone();
    tool.server_title = server.title.clone();
    if let Some(configured) = server
        .tool_tags
        .iter()
        .find_map(|(name, tags)| name.eq_ignore_ascii_case(&tool.tool_name).then_some(tags))
    {
        merge_capability_ids(&mut tool.capability_ids, configured);
    }
    tool
}

fn merge_capability_ids(target: &mut Vec<String>, configured: &[String]) {
    for capability in configured {
        let capability = capability.trim();
        if capability.is_empty()
            || target
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(capability))
        {
            continue;
        }
        target.push(capability.to_string());
    }
}

fn enabled_tool_count(server: &McpServerConfig, tools: &[McpToolCatalogEntry]) -> usize {
    tools
        .iter()
        .filter(|tool| server.tool_enabled(&tool.tool_name))
        .count()
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

fn infer_effect_class_with_annotations(
    tool_name: &str,
    description: Option<&str>,
    annotations: Option<&crate::mcp_gateway::stdio_client::McpToolAnnotations>,
) -> McpEffectClass {
    let inferred = infer_effect_class(tool_name, description);
    let Some(annotations) = annotations else {
        return inferred;
    };

    // Server annotations are untrusted hints. They may promote Medousa's
    // conservative classification, but never lower it.
    if annotations.destructive_hint == Some(true) {
        return McpEffectClass::ExternalSideEffect;
    }
    if annotations.read_only_hint == Some(false) && inferred == McpEffectClass::ExternalRead {
        return McpEffectClass::ExternalWrite;
    }
    inferred
}

fn dedupe_tools(tools: &mut Vec<McpToolCatalogEntry>) {
    let mut seen = HashSet::new();
    tools.retain(|tool| seen.insert(format!("{}.{}", tool.server_id, tool.tool_name)));
}

struct UserFacingMcpFailure {
    code: &'static str,
    message: String,
    retryable: bool,
}

fn classify_runtime_error(
    server: &McpServerConfig,
    operation: &str,
    error: &anyhow::Error,
) -> UserFacingMcpFailure {
    let diagnostic = format!("{error:#}").to_ascii_lowercase();
    let (code, guidance, retryable) = if diagnostic.contains("403")
        || diagnostic.contains("forbidden")
        || diagnostic.contains("insufficient_scope")
        || diagnostic.contains("insufficient scope")
    {
        (
            "mcp_scope_denied",
            "Reconnect the account with the scopes required by this tool.",
            false,
        )
    } else if diagnostic.contains("401")
        || diagnostic.contains("unauthorized")
        || diagnostic.contains("not connected")
        || diagnostic.contains("bearer token is missing")
    {
        (
            "mcp_authentication_required",
            "Reconnect the account or replace its bearer token in Settings → MCP servers.",
            false,
        )
    } else if diagnostic.contains("timed out") || diagnostic.contains("timeout") {
        (
            "mcp_timeout",
            "The server did not respond in time. Check the network and try again.",
            true,
        )
    } else if diagnostic.contains("protocol")
        || diagnostic.contains("initialize failed")
        || diagnostic.contains("method not found")
        || diagnostic.contains("unexpected response")
    {
        (
            "mcp_protocol_incompatible",
            "The endpoint did not complete a compatible MCP handshake. Verify its URL and transport.",
            false,
        )
    } else if diagnostic.contains("invalid remote mcp url")
        || diagnostic.contains("missing url")
        || diagnostic.contains("missing command")
        || diagnostic.contains("unsupported mcp transport")
    {
        (
            "mcp_configuration_invalid",
            "Review this server's URL, transport, and command settings.",
            false,
        )
    } else if operation == "invoke the tool"
        && (diagnostic.contains("tools/call") || diagnostic.contains("mcp error"))
    {
        (
            "mcp_tool_failed",
            "The server rejected or failed the tool call. Review its input and do not automatically replay an ambiguous write.",
            false,
        )
    } else {
        (
            "mcp_transport_unavailable",
            "Medousa will reconnect with bounded backoff. Check the server and network if this continues.",
            true,
        )
    };

    UserFacingMcpFailure {
        code,
        message: format!("Could not {operation} with '{}'. {guidance}", server.title),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_gateway::oauth::McpOAuthBundleStore;
    use crate::mcp_gateway::server_config::McpServerConfig;
    use std::sync::Arc;

    struct EmptyBundleStore;

    impl McpOAuthBundleStore for EmptyBundleStore {
        fn load_bundle(&self, _server_id: &str) -> Result<Option<String>, String> {
            Ok(None)
        }

        fn save_bundle(&self, _server_id: &str, _bundle: Option<&str>) -> Result<(), String> {
            Ok(())
        }
    }

    struct FailingBundleStore;

    impl McpOAuthBundleStore for FailingBundleStore {
        fn load_bundle(&self, _server_id: &str) -> Result<Option<String>, String> {
            Err("credential store should not be consulted".to_string())
        }

        fn save_bundle(&self, _server_id: &str, _bundle: Option<&str>) -> Result<(), String> {
            Ok(())
        }
    }

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
                bearer_token_configured: false,
                allowed_lanes: vec!["interactive".to_string()],
                allowed_effect_classes: vec!["external_read".to_string()],
                tool_tags: Default::default(),
                disabled_tools: Vec::new(),
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

    #[tokio::test]
    async fn policy_auth_failure_is_not_a_retryable_tool_or_network_failure() {
        struct RejectedCredential;
        #[async_trait::async_trait]
        impl McpPolicyEvaluator for RejectedCredential {
            async fn evaluate(
                &self,
                _: &McpPolicyEvaluateRequest,
            ) -> anyhow::Result<medousa_types::mcp_gateway_api::McpPolicyEvaluateResponse>
            {
                Err(super::super::policy_client::PolicyAuthenticationError.into())
            }
        }
        let registry = ServerRegistry::with_policy_evaluator(
            test_registry().config,
            Arc::new(RejectedCredential),
        );
        registry.bootstrap().await;
        let context = medousa_types::mcp_gateway_api::McpTurnContext {
            turn_id: "turn-auth".into(),
            session_id: "session-auth".into(),
            user_id: "user".into(),
            channel_id: "chat".into(),
            lane: McpTurnLane::Interactive,
            policy_profile: None,
        };
        let response = registry
            .invoke(
                McpInvokeRequest {
                    server_id: "notion".into(),
                    tool_name: "search_pages".into(),
                    input: json!({}),
                    turn_token: medousa_types::mcp_turn_token::mint_mcp_turn_token(&context)
                        .unwrap(),
                    turn_context: context,
                    operator_approval_granted: None,
                },
                true,
            )
            .await;
        assert!(!response.ok);
        assert!(response.output.is_none());
        let error = response.error.unwrap();
        assert_eq!(error.code, "policy_authentication_failed");
        assert_eq!(error.retryable, Some(false));
    }

    #[tokio::test]
    async fn configured_hints_make_mock_tools_discoverable_by_alias() {
        let base = test_registry();
        let mut config = (*base.config).clone();
        config.servers[0]
            .tool_tags
            .insert("search_pages".to_string(), vec!["web_research".to_string()]);
        let registry = ServerRegistry::new(Arc::new(config));
        registry.bootstrap().await;

        let tools = registry.discover("search the internet", None, 10).await;
        assert!(tools.iter().any(|tool| tool.tool_name == "search_pages"));
    }

    #[tokio::test]
    async fn disabled_tools_are_hidden_from_discovery_and_denied_on_invoke() {
        let base = test_registry();
        let mut config = (*base.config).clone();
        config.servers[0]
            .disabled_tools
            .push("search_pages".to_string());
        let registry = ServerRegistry::new(Arc::new(config));
        registry.bootstrap().await;

        assert!(registry.discover("search_pages", None, 10).await.is_empty());
        let catalog = registry.catalog_sync().await;
        let disabled = catalog
            .entries
            .iter()
            .find(|entry| entry.tool_name == "search_pages")
            .expect("disabled tool remains visible to management surfaces");
        assert!(!disabled.available);
        assert_eq!(
            disabled.unavailable_reason.as_deref(),
            Some("tool disabled")
        );

        let response = registry
            .invoke(
                McpInvokeRequest {
                    server_id: "notion".to_string(),
                    tool_name: "search_pages".to_string(),
                    input: json!({}),
                    turn_context: medousa_types::mcp_gateway_api::McpTurnContext {
                        turn_id: "turn-1".to_string(),
                        session_id: "session-1".to_string(),
                        user_id: "user-1".to_string(),
                        channel_id: "chat".to_string(),
                        lane: McpTurnLane::Interactive,
                        policy_profile: None,
                    },
                    turn_token: None,
                    operator_approval_granted: None,
                },
                true,
            )
            .await;
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("tool_disabled")
        );
    }

    #[tokio::test]
    async fn configured_bearer_precedes_oauth() {
        let registry =
            test_registry().with_oauth(Arc::new(McpOAuthBroker::new(Arc::new(FailingBundleStore))));
        let server = McpServerConfig {
            transport: "http".to_string(),
            url: Some("https://example.com/mcp".to_string()),
            bearer_token: Some(" configured-token ".to_string()),
            ..registry.config.servers[0].clone()
        };

        assert_eq!(
            registry.remote_bearer_token(&server).await.unwrap(),
            Some("configured-token".to_string())
        );
    }

    #[tokio::test]
    async fn missing_oauth_grant_preserves_unauthenticated_remote_access() {
        let registry =
            test_registry().with_oauth(Arc::new(McpOAuthBroker::new(Arc::new(EmptyBundleStore))));
        let server = McpServerConfig {
            transport: "http".to_string(),
            url: Some("https://example.com/mcp".to_string()),
            bearer_token: None,
            ..registry.config.servers[0].clone()
        };

        assert_eq!(registry.remote_bearer_token(&server).await.unwrap(), None);
    }

    #[test]
    fn untrusted_annotations_only_promote_effect_risk() {
        use crate::mcp_gateway::stdio_client::McpToolAnnotations;

        let claims_read_only = McpToolAnnotations {
            read_only_hint: Some(true),
            ..Default::default()
        };
        assert_eq!(
            infer_effect_class_with_annotations(
                "delete_message",
                Some("Deletes a message"),
                Some(&claims_read_only),
            ),
            McpEffectClass::ExternalSideEffect,
        );

        let claims_write = McpToolAnnotations {
            read_only_hint: Some(false),
            ..Default::default()
        };
        assert_eq!(
            infer_effect_class_with_annotations("lookup", None, Some(&claims_write)),
            McpEffectClass::ExternalWrite,
        );

        let claims_destructive = McpToolAnnotations {
            destructive_hint: Some(true),
            ..Default::default()
        };
        assert_eq!(
            infer_effect_class_with_annotations("lookup", None, Some(&claims_destructive)),
            McpEffectClass::ExternalSideEffect,
        );
    }

    #[test]
    fn planning_guidance_exposes_retry_and_open_world_claims_conservatively() {
        let annotations = McpToolAnnotations {
            idempotent_hint: Some(true),
            open_world_hint: Some(true),
            ..Default::default()
        };
        let hints = planning_hints(McpEffectClass::ExternalWrite, Some(&annotations));
        assert!(hints.iter().any(|hint| hint.contains("untrusted guidance")));
        assert!(hints.iter().any(|hint| hint.contains("open world")));
    }

    #[test]
    fn catalog_entry_keeps_lossless_schemas_and_approval_copy() {
        let registry = test_registry();
        let server = &registry.config.servers[0];
        let entry = tool_entry_from_definition(
            server,
            crate::mcp_gateway::stdio_client::McpToolDefinition {
                name: "publish".into(),
                title: "Publish page".into(),
                description: Some("Publish a page to the web".into()),
                input_schema: Some(json!({
                    "type": "object",
                    "required": ["page_id"],
                    "properties": { "page_id": { "type": "string" } }
                })),
                output_schema: Some(json!({
                    "type": "object",
                    "properties": { "url": { "type": "string", "format": "uri" } }
                })),
                annotations: Some(crate::mcp_gateway::stdio_client::McpToolAnnotations {
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(true),
                    ..Default::default()
                }),
                icons: Some(json!([{ "src": "https://example.com/icon.png" }])),
                meta: Some(Default::default()),
            },
        );

        assert_eq!(
            entry
                .input_schema
                .as_ref()
                .and_then(|schema| schema.get("required"))
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(entry.effect_class, McpEffectClass::ExternalSideEffect);
        assert!(
            entry
                .approval_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("potentially destructive"))
        );
        assert!(
            entry
                .planning_hints
                .iter()
                .any(|hint| hint.contains("never replay"))
        );
    }

    #[test]
    fn runtime_errors_are_actionable_and_secret_free() {
        let registry = test_registry();
        let server = &registry.config.servers[0];
        let error = anyhow::anyhow!("request returned 401 for bearer super-secret-token");
        let classified = classify_runtime_error(server, "invoke the tool", &error);
        assert_eq!(classified.code, "mcp_authentication_required");
        assert!(!classified.retryable);
        assert!(!classified.message.contains("super-secret-token"));
        assert!(classified.message.contains("Reconnect"));
    }
}
