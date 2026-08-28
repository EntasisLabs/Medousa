//! HTTP client for daemon cognition tools to reach medousa-mcp-gateway.

use anyhow::{Context, Result};
use reqwest::Client;
use std::sync::Arc;

use crate::capability_catalog::McpCatalogSyncResponse;
use crate::mcp_gateway_api::{
    BeginMcpOAuthRequest, BeginMcpOAuthResponse, CompleteMcpOAuthRequest, CompleteMcpOAuthResponse,
    DisconnectMcpOAuthResponse, McpDiscoverRequest, McpDiscoverResponse, McpGatewayHealthResponse,
    McpInvokeRequest, McpInvokeResponse, McpOAuthStatusResponse, McpServersResponse,
    RefreshMcpOAuthRequest, resolve_mcp_gateway_url,
};
use medousa_mcp_gateway::resolve_mcp_gateway_token;

#[derive(Clone)]
pub struct McpGatewayClient {
    backend: McpGatewayBackend,
}

#[derive(Clone)]
enum McpGatewayBackend {
    Http {
        base_url: String,
        token: Option<String>,
        client: Client,
    },
    InProcess {
        active: Arc<tokio::sync::RwLock<Arc<InProcessMcpGateway>>>,
    },
}

struct InProcessMcpGateway {
    registry: Arc<medousa_mcp_gateway::ServerRegistry>,
    invokes_enabled: bool,
    initialized: tokio::sync::OnceCell<()>,
}

impl InProcessMcpGateway {
    fn new(registry: Arc<medousa_mcp_gateway::ServerRegistry>, invokes_enabled: bool) -> Self {
        Self {
            registry,
            invokes_enabled,
            initialized: tokio::sync::OnceCell::new(),
        }
    }

    async fn initialize(&self) {
        self.initialized
            .get_or_init(|| async {
                self.registry.bootstrap().await;
            })
            .await;
    }
}

impl McpGatewayClient {
    pub fn from_env() -> Self {
        Self {
            backend: McpGatewayBackend::Http {
                base_url: resolve_mcp_gateway_url(None),
                token: resolve_mcp_gateway_token(),
                client: Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|_| Client::new()),
            },
        }
    }

    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Result<Self> {
        Ok(Self {
            backend: McpGatewayBackend::Http {
                base_url: base_url.into().trim_end_matches('/').to_string(),
                token,
                client: Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .context("failed to build MCP gateway HTTP client")?,
            },
        })
    }

    pub fn in_process(
        registry: Arc<medousa_mcp_gateway::ServerRegistry>,
        invokes_enabled: bool,
    ) -> Self {
        Self {
            backend: McpGatewayBackend::InProcess {
                active: Arc::new(tokio::sync::RwLock::new(Arc::new(
                    InProcessMcpGateway::new(registry, invokes_enabled),
                ))),
            },
        }
    }

    async fn active_in_process(
        active: &tokio::sync::RwLock<Arc<InProcessMcpGateway>>,
    ) -> Arc<InProcessMcpGateway> {
        active.read().await.clone()
    }

    /// Replace the in-process adapter without rebuilding the daemon or its tool registry.
    pub async fn replace_in_process(
        &self,
        registry: Arc<medousa_mcp_gateway::ServerRegistry>,
        invokes_enabled: bool,
    ) -> Result<()> {
        let McpGatewayBackend::InProcess { active } = &self.backend else {
            anyhow::bail!("MCP gateway backend is not in-process");
        };
        let next = Arc::new(InProcessMcpGateway::new(registry, invokes_enabled));
        next.initialize().await;
        *active.write().await = next;
        Ok(())
    }

    fn apply_auth(
        token: Option<&str>,
        request: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        if let Some(token) = token.filter(|value| !value.is_empty()) {
            request.bearer_auth(token)
        } else {
            request
        }
    }

    pub async fn health(&self) -> Result<McpGatewayHealthResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url, client, ..
            } => {
                let response = client
                    .get(format!("{base_url}/health"))
                    .send()
                    .await
                    .context("failed to reach MCP gateway health endpoint")?
                    .error_for_status()
                    .context("MCP gateway health endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                active.initialize().await;
                let (registered_servers, connected_servers, catalog_entries) =
                    active.registry.health_stats().await;
                Ok(McpGatewayHealthResponse {
                    status: "ok".to_string(),
                    invokes_enabled: active.invokes_enabled,
                    registered_servers,
                    connected_servers,
                    catalog_entries,
                    now_utc: chrono::Utc::now(),
                })
            }
        }
    }

    pub async fn fetch_catalog(&self) -> Result<McpCatalogSyncResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client.get(format!("{base_url}/v1/mcp/catalog")),
                )
                .send()
                .await
                .context("failed to reach MCP gateway catalog endpoint")?
                .error_for_status()
                .context("MCP gateway catalog endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                active.initialize().await;
                Ok(active.registry.catalog_sync().await)
            }
        }
    }

    pub async fn discover(&self, request: &McpDiscoverRequest) -> Result<McpDiscoverResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client
                        .post(format!("{base_url}/v1/mcp/discover"))
                        .json(request),
                )
                .send()
                .await
                .context("failed to reach MCP gateway discover endpoint")?;

                if response.status().is_success() {
                    return Ok(response.json().await?);
                }

                Ok(McpDiscoverResponse {
                    query: request.query.clone(),
                    matches: Vec::new(),
                    truncated: false,
                    gateway_unreachable: Some(true),
                })
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                active.initialize().await;
                let limit = request.limit.clamp(1, 100);
                let matches = active
                    .registry
                    .discover(&request.query, request.server_id.as_deref(), limit)
                    .await;
                Ok(McpDiscoverResponse {
                    query: request.query.clone(),
                    truncated: matches.len() >= limit,
                    matches,
                    gateway_unreachable: None,
                })
            }
        }
    }

    pub async fn invoke(&self, request: &McpInvokeRequest) -> Result<McpInvokeResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client
                        .post(format!("{base_url}/v1/mcp/invoke"))
                        .json(request),
                )
                .send()
                .await
                .context("failed to reach MCP gateway invoke endpoint")?
                .error_for_status()
                .context("MCP gateway invoke endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                active.initialize().await;
                Ok(active
                    .registry
                    .invoke(request.clone(), active.invokes_enabled)
                    .await)
            }
        }
    }

    pub async fn list_servers(&self) -> Result<McpServersResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client.get(format!("{base_url}/v1/mcp/servers")),
                )
                .send()
                .await
                .context("failed to reach MCP gateway servers endpoint")?
                .error_for_status()
                .context("MCP gateway servers endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                active.initialize().await;
                Ok(active.registry.list_servers().await)
            }
        }
    }

    pub async fn oauth_status(&self, server_id: &str) -> Result<McpOAuthStatusResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client.get(format!("{base_url}/v1/mcp/oauth/{}", server_id.trim())),
                )
                .send()
                .await
                .context("failed to reach MCP OAuth status endpoint")?
                .error_for_status()
                .context("MCP OAuth status endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                Ok(active.registry.oauth_status(server_id).await?)
            }
        }
    }

    pub async fn begin_oauth(
        &self,
        request: BeginMcpOAuthRequest,
    ) -> Result<BeginMcpOAuthResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client
                        .post(format!("{base_url}/v1/mcp/oauth/begin"))
                        .json(&request),
                )
                .send()
                .await
                .context("failed to reach MCP OAuth begin endpoint")?
                .error_for_status()
                .context("MCP OAuth begin endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                Ok(active.registry.begin_oauth(request).await?)
            }
        }
    }

    pub async fn complete_oauth(
        &self,
        request: CompleteMcpOAuthRequest,
    ) -> Result<CompleteMcpOAuthResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client
                        .post(format!("{base_url}/v1/mcp/oauth/complete"))
                        .json(&request),
                )
                .send()
                .await
                .context("failed to reach MCP OAuth complete endpoint")?
                .error_for_status()
                .context("MCP OAuth complete endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                Ok(active
                    .registry
                    .complete_oauth(&request.login_id, &request.callback_url)
                    .await?)
            }
        }
    }

    pub async fn refresh_oauth(&self, server_id: &str) -> Result<McpOAuthStatusResponse> {
        let request = RefreshMcpOAuthRequest {
            server_id: server_id.trim().to_string(),
        };
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client
                        .post(format!("{base_url}/v1/mcp/oauth/refresh"))
                        .json(&request),
                )
                .send()
                .await
                .context("failed to reach MCP OAuth refresh endpoint")?
                .error_for_status()
                .context("MCP OAuth refresh endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                Ok(active.registry.refresh_oauth(&request.server_id).await?)
            }
        }
    }

    pub async fn disconnect_oauth(&self, server_id: &str) -> Result<DisconnectMcpOAuthResponse> {
        match &self.backend {
            McpGatewayBackend::Http {
                base_url,
                token,
                client,
            } => {
                let response = Self::apply_auth(
                    token.as_deref(),
                    client.delete(format!("{base_url}/v1/mcp/oauth/{}", server_id.trim())),
                )
                .send()
                .await
                .context("failed to reach MCP OAuth disconnect endpoint")?
                .error_for_status()
                .context("MCP OAuth disconnect endpoint returned error")?;
                Ok(response.json().await?)
            }
            McpGatewayBackend::InProcess { active } => {
                let active = Self::active_in_process(active).await;
                Ok(active.registry.disconnect_oauth(server_id).await?)
            }
        }
    }

    pub fn is_auth_configured(&self) -> bool {
        match &self.backend {
            McpGatewayBackend::Http { token, .. } => {
                token.as_deref().is_some_and(|value| !value.is_empty())
            }
            McpGatewayBackend::InProcess { .. } => true,
        }
    }
}

pub fn gateway_auth_configured() -> bool {
    resolve_mcp_gateway_token().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_with_defaults() {
        let client = McpGatewayClient::from_env();
        assert!(matches!(client.backend, McpGatewayBackend::Http { .. }));
    }
}
