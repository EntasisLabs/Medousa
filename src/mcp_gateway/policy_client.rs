use anyhow::{Context, Result};
use reqwest::Client;

use crate::mcp_gateway_api::{McpPolicyEvaluateRequest, McpPolicyEvaluateResponse};

#[derive(Clone)]
pub struct DaemonPolicyClient {
    policy_url: String,
    policy_token: Option<String>,
    client: Client,
}

impl DaemonPolicyClient {
    pub fn new(policy_url: String, policy_token: Option<String>) -> Self {
        Self {
            policy_url,
            policy_token,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    pub async fn evaluate(
        &self,
        request: &McpPolicyEvaluateRequest,
    ) -> Result<McpPolicyEvaluateResponse> {
        let mut builder = self
            .client
            .post(&self.policy_url)
            .json(request);
        if let Some(token) = self.policy_token.as_deref().filter(|value| !value.is_empty()) {
            builder = builder.bearer_auth(token);
        }

        let response = builder
            .send()
            .await
            .context("failed to reach daemon policy endpoint")?
            .error_for_status()
            .context("daemon policy endpoint returned error")?;
        Ok(response.json().await?)
    }
}
