#[cfg(feature = "async")]
use medousa_types::{
    AgentPermissionRequestListResponse, AgentPermissionResolveRequest,
    AgentPermissionResolveResponse, AgentRuntimeListResponse, AgentSessionPromptRequest,
    AgentSessionPromptResponse, CancelAgentSessionResponse, CreateAgentSessionRequest,
    CreateAgentSessionResponse, InteractiveTurnStreamEvent,
};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::{decode, path_with_query};

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
            .get_json(self.client.base_url(), "/v1/agents/runtimes")
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
            .post_json(self.client.base_url(), "/v1/agents/sessions", body)
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
        let path = format!(
            "/v1/agents/sessions/{}/prompt",
            agent_session_id.trim()
        );
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
        let path = format!(
            "/v1/agents/sessions/{}/cancel",
            agent_session_id.trim()
        );
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
        let path = path_with_query("/v1/agents/permission-requests", &params);
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
        let path = format!(
            "/v1/agents/permission-requests/{}/approve",
            request_id.trim()
        );
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
        let path = format!(
            "/v1/agents/permission-requests/{}/deny",
            request_id.trim()
        );
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }
}
