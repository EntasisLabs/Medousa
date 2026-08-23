#[cfg(feature = "async")]
use medousa_types::{
    AgentPermissionRequestListResponse, AgentPermissionResolveRequest,
    AgentPermissionResolveResponse, AgentRuntimeListResponse, AgentSecretDenyRequest,
    AgentSecretFulfillRequest, AgentSecretRequestListResponse, AgentSecretResolveResponse,
    AgentSessionPromptRequest, AgentSessionPromptResponse, CancelAgentSessionResponse,
    CreateAgentSessionRequest, CreateAgentSessionResponse, InteractiveTurnStreamEvent,
    SetAgentSessionConfigOptionRequest, SetAgentSessionConfigOptionResponse,
};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};

#[cfg(feature = "async")]
pub struct AgentsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl AgentsApi<'_> {
    pub async fn list_runtimes(&self) -> Result<AgentRuntimeListResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::AGENTS_RUNTIMES_GET.path)
            .await?;
        decode(value).await
    }

    pub async fn create_session(
        &self,
        request: &CreateAgentSessionRequest,
    ) -> Result<CreateAgentSessionResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::AGENTS_SESSIONS_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn prompt(
        &self,
        agent_session_id: &str,
        request: &AgentSessionPromptRequest,
    ) -> Result<AgentSessionPromptResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::AGENTS_SESSIONS_BY_AGENT_SESSION_ID_PROMPT_POST,
            &[("agent_session_id", agent_session_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn set_config_option(
        &self,
        agent_session_id: &str,
        request: &SetAgentSessionConfigOptionRequest,
    ) -> Result<SetAgentSessionConfigOptionResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::AGENTS_SESSIONS_BY_AGENT_SESSION_ID_CONFIG_POST,
            &[("agent_session_id", agent_session_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn cancel(
        &self,
        agent_session_id: &str,
    ) -> Result<CancelAgentSessionResponse, crate::SdkError> {
        let path = op_path(
            &ops::AGENTS_SESSIONS_BY_AGENT_SESSION_ID_CANCEL_POST,
            &[("agent_session_id", agent_session_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_empty_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    #[cfg(feature = "sse")]
    pub fn stream(
        &self,
        stream_url: impl Into<String>,
    ) -> impl Stream<Item = Result<InteractiveTurnStreamEvent, crate::SdkError>> + '_ {
        let byte_stream = self
            .client
            .transport()
            .stream_sse(self.client.base_url(), stream_url.into());
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }

    #[cfg(feature = "sse")]
    pub async fn stream_session(
        &self,
        request: &CreateAgentSessionRequest,
    ) -> Result<
        impl Stream<Item = Result<InteractiveTurnStreamEvent, crate::SdkError>> + '_,
        crate::SdkError,
    > {
        let response = self.create_session(request).await?;
        Ok(self.stream(response.stream_url))
    }

    pub async fn list_permission_requests(
        &self,
        status: Option<&str>,
        limit: Option<usize>,
    ) -> Result<AgentPermissionRequestListResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(status) = status {
            params.push(("status", status.to_string()));
        }
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let path = op_path_query(&ops::AGENTS_PERMISSION_REQUESTS_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn approve_permission(
        &self,
        request_id: &str,
        request: &AgentPermissionResolveRequest,
    ) -> Result<AgentPermissionResolveResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::AGENTS_PERMISSION_REQUESTS_BY_REQUEST_ID_APPROVE_POST,
            &[("request_id", request_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn deny_permission(
        &self,
        request_id: &str,
        request: &AgentPermissionResolveRequest,
    ) -> Result<AgentPermissionResolveResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::AGENTS_PERMISSION_REQUESTS_BY_REQUEST_ID_DENY_POST,
            &[("request_id", request_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn list_secret_requests(
        &self,
        status: Option<&str>,
        limit: Option<usize>,
    ) -> Result<AgentSecretRequestListResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(status) = status {
            params.push(("status", status.to_string()));
        }
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let path = op_path_query(&ops::AGENTS_SECRET_REQUESTS_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn fulfill_secret_request(
        &self,
        request_id: &str,
        request: &AgentSecretFulfillRequest,
    ) -> Result<AgentSecretResolveResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::AGENTS_SECRET_REQUESTS_BY_REQUEST_ID_FULFILL_POST,
            &[("request_id", request_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn deny_secret_request(
        &self,
        request_id: &str,
        request: &AgentSecretDenyRequest,
    ) -> Result<AgentSecretResolveResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::AGENTS_SECRET_REQUESTS_BY_REQUEST_ID_DENY_POST,
            &[("request_id", request_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }
}
