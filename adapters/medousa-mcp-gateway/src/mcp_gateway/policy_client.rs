use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use reqwest::Client;
use tokio::sync::Mutex;

use medousa_types::mcp_gateway_api::{McpPolicyEvaluateRequest, McpPolicyEvaluateResponse};

#[derive(Debug)]
pub(crate) struct PolicyAuthenticationError;

impl std::fmt::Display for PolicyAuthenticationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("daemon policy authentication rejected (401 Unauthorized); restart or upgrade the local daemon and MCP gateway together, or check MEDOUSA_MCP_POLICY_TOKEN for a custom deployment")
    }
}

impl std::error::Error for PolicyAuthenticationError {}

#[async_trait::async_trait]
pub trait McpPolicyEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        request: &McpPolicyEvaluateRequest,
    ) -> Result<McpPolicyEvaluateResponse>;
}

#[derive(Clone)]
pub struct DaemonPolicyClient {
    policy_url: String,
    policy_token: Option<String>,
    local_data_dir: Option<PathBuf>,
    local_token: Arc<Mutex<Option<String>>>,
    client: Client,
}

impl DaemonPolicyClient {
    pub fn new(policy_url: String, policy_token: Option<String>) -> Self {
        let policy_token = policy_token
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty());
        let local_data_dir = super::policy_credentials::is_local_policy_url(&policy_url)
            .then(super::policy_credentials::data_dir);
        Self {
            policy_url,
            policy_token,
            local_data_dir,
            local_token: Arc::new(Mutex::new(None)),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("build MCP policy HTTP client"),
        }
    }

    async fn token(&self, refresh: bool) -> Result<Option<String>> {
        if let Some(token) = self.policy_token.as_ref().filter(|token| !token.is_empty()) {
            return Ok(Some(token.clone()));
        }
        let Some(root) = &self.local_data_dir else {
            return Ok(None);
        };
        let mut token = self.local_token.lock().await;
        if refresh || token.is_none() {
            let root = root.clone();
            *token = tokio::task::spawn_blocking(move || {
                super::policy_credentials::load_local_policy_token(&root)
            })
            .await
            .context("read local MCP policy credential task")??;
        }
        Ok(token.clone())
    }

    async fn send(
        &self,
        request: &McpPolicyEvaluateRequest,
        token: Option<&str>,
    ) -> Result<reqwest::Response> {
        let mut builder = self.client.post(&self.policy_url).json(request);
        if let Some(token) = token {
            builder = builder.bearer_auth(token);
        }
        builder
            .send()
            .await
            .context("failed to reach daemon policy endpoint")
    }
}

#[async_trait::async_trait]
impl McpPolicyEvaluator for DaemonPolicyClient {
    async fn evaluate(
        &self,
        request: &McpPolicyEvaluateRequest,
    ) -> Result<McpPolicyEvaluateResponse> {
        let token = self.token(false).await?;
        let mut response = self.send(request, token.as_deref()).await?;
        // A daemon restart may rotate its persisted service credential. Retry
        // only policy evaluation, never the remote tool invocation itself.
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            && self.policy_token.is_none()
            && self.local_data_dir.is_some()
        {
            let refreshed = self.token(true).await?;
            if refreshed != token {
                response = self.send(request, refreshed.as_deref()).await?;
            }
        }
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(PolicyAuthenticationError.into());
        }
        let response = response
            .error_for_status()
            .context("daemon policy endpoint returned error")?;
        Ok(response.json().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::policy_credentials::{
        initialize_local_policy_token, load_local_policy_token,
    };
    use super::*;
    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use medousa_types::mcp_gateway_api::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct PolicyState {
        expected: Arc<Mutex<String>>,
        calls: Arc<AtomicUsize>,
    }

    async fn evaluate(
        State(state): State<PolicyState>,
        headers: HeaderMap,
    ) -> Result<Json<McpPolicyEvaluateResponse>, StatusCode> {
        state.calls.fetch_add(1, Ordering::SeqCst);
        let expected = format!("Bearer {}", state.expected.lock().await);
        if headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            != Some(expected.as_str())
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(Json(McpPolicyEvaluateResponse {
            allowed: true,
            decision: McpPolicyDecision::Allow,
            reason: "test policy".into(),
            approval_required: false,
        }))
    }

    #[tokio::test]
    async fn local_credentials_survive_restart_refresh_rotation_and_preserve_explicit_auth() {
        // Run secret-store integration in a hermetic child rather than changing
        // process-wide keyring settings under other parallel tests.
        const NAME: &str = "mcp_gateway::policy_client::tests::local_credentials_survive_restart_refresh_rotation_and_preserve_explicit_auth";
        if std::env::var("MEDOUSA_MCP_POLICY_TEST").as_deref() != Ok(NAME) {
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", NAME, "--nocapture"])
                    .env("MEDOUSA_TEST_HERMETIC", "1")
                    .env("MEDOUSA_MCP_POLICY_TEST", NAME)
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        assert!(load_local_policy_token(root.path()).unwrap().is_none());
        assert!(
            !root.path().join("installation.json").exists(),
            "gateway reads never mint authority"
        );
        let token = initialize_local_policy_token(root.path(), None).unwrap();
        assert_eq!(
            initialize_local_policy_token(root.path(), None).unwrap(),
            token
        );
        let other = tempfile::tempdir().unwrap();
        assert_ne!(
            initialize_local_policy_token(other.path(), None).unwrap(),
            token
        );
        let state = PolicyState {
            expected: Arc::new(Mutex::new(token)),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/v1/mcp/policy/evaluate",
            listener.local_addr().unwrap()
        );
        let router = Router::new()
            .route("/v1/mcp/policy/evaluate", post(evaluate))
            .with_state(state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut client = DaemonPolicyClient::new(url.clone(), None);
        client.local_data_dir = Some(root.path().into());
        let request = McpPolicyEvaluateRequest {
            action: "mcp.invoke".into(),
            server_id: "hashmap".into(),
            tool_name: "capabilities".into(),
            effect_class: McpEffectClass::ExternalRead,
            turn_context: McpTurnContext {
                turn_id: "turn".into(),
                session_id: "session".into(),
                user_id: "user".into(),
                channel_id: "chat".into(),
                lane: McpTurnLane::Interactive,
                policy_profile: None,
            },
            operator_approval_granted: None,
        };
        assert!(client.evaluate(&request).await.unwrap().allowed);
        assert_eq!(state.calls.load(Ordering::SeqCst), 1);

        let rotated =
            initialize_local_policy_token(root.path(), Some("rotated-test-policy".into())).unwrap();
        *state.expected.lock().await = rotated;
        assert!(client.evaluate(&request).await.unwrap().allowed);
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            3,
            "one rejected credential followed by one policy retry"
        );

        let mut explicit =
            DaemonPolicyClient::new(url.clone(), Some("wrong-explicit-token".into()));
        explicit.local_data_dir = Some(root.path().into());
        assert!(
            explicit
                .evaluate(&request)
                .await
                .unwrap_err()
                .is::<PolicyAuthenticationError>()
        );
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            4,
            "explicit auth must not silently fall back"
        );

        *state.expected.lock().await = "unavailable-test-token".into();
        assert!(
            client
                .evaluate(&request)
                .await
                .unwrap_err()
                .is::<PolicyAuthenticationError>()
        );
        assert_eq!(
            state.calls.load(Ordering::SeqCst),
            5,
            "unchanged stored credential does not cause a retry loop"
        );
        server.abort();
    }
}
