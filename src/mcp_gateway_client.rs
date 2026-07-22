//! HTTP client for daemon cognition tools to reach medousa-mcp-gateway.

use anyhow::{Context, Result};
use reqwest::Client;

use crate::capability_catalog::McpCatalogSyncResponse;
use medousa_mcp_gateway::resolve_mcp_gateway_token;
use crate::mcp_gateway_api::{
    McpDiscoverRequest, McpDiscoverResponse, McpGatewayHealthResponse, McpInvokeRequest,
    McpInvokeResponse, McpServersResponse, resolve_mcp_gateway_url,
};

#[derive(Clone)]
pub struct McpGatewayClient {
    base_url: String,
    token: Option<String>,
    client: Client,
}

impl McpGatewayClient {
    pub fn from_env() -> Self {
        Self {
            base_url: resolve_mcp_gateway_url(None),
            token: resolve_mcp_gateway_token(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .context("failed to build MCP gateway HTTP client")?,
        })
    }

    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = self.token.as_deref().filter(|value| !value.is_empty()) {
            request.bearer_auth(token)
        } else {
            request
        }
    }

    pub async fn health(&self) -> Result<McpGatewayHealthResponse> {
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("failed to reach MCP gateway health endpoint")?
            .error_for_status()
            .context("MCP gateway health endpoint returned error")?;
        Ok(response.json().await?)
    }

    pub async fn fetch_catalog(&self) -> Result<McpCatalogSyncResponse> {
        let response = self
            .apply_auth(self.client.get(format!("{}/v1/mcp/catalog", self.base_url)))
            .send()
            .await
            .context("failed to reach MCP gateway catalog endpoint")?
            .error_for_status()
            .context("MCP gateway catalog endpoint returned error")?;
        Ok(response.json().await?)
    }

    pub async fn discover(&self, request: &McpDiscoverRequest) -> Result<McpDiscoverResponse> {
        let response = self
            .apply_auth(
                self.client
                    .post(format!("{}/v1/mcp/discover", self.base_url))
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

    pub async fn invoke(&self, request: &McpInvokeRequest) -> Result<McpInvokeResponse> {
        let response = self
            .apply_auth(
                self.client
                    .post(format!("{}/v1/mcp/invoke", self.base_url))
                    .json(request),
            )
            .send()
            .await
            .context("failed to reach MCP gateway invoke endpoint")?
            .error_for_status()
            .context("MCP gateway invoke endpoint returned error")?;
        Ok(response.json().await?)
    }

    pub async fn list_servers(&self) -> Result<McpServersResponse> {
        let response = self
            .apply_auth(self.client.get(format!("{}/v1/mcp/servers", self.base_url)))
            .send()
            .await
            .context("failed to reach MCP gateway servers endpoint")?
            .error_for_status()
            .context("MCP gateway servers endpoint returned error")?;
        Ok(response.json().await?)
    }

    pub fn is_auth_configured(&self) -> bool {
        self.token.as_deref().is_some_and(|value| !value.is_empty())
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
        assert!(!client.base_url.is_empty());
    }
}
