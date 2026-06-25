#[cfg(feature = "async")]
use medousa_types::{
    SessionAppendTurnRequest, SessionAppendTurnResponse, SessionHistoryListResponse,
    SessionHistoryResponse, SessionSetDisplayNameRequest, SessionSetDisplayNameResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct SessionsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl SessionsApi<'_> {
    pub async fn list(&self, limit: usize) -> Result<SessionHistoryListResponse, crate::SdkError> {
        let path = format!("/v1/sessions?limit={limit}");
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn history(&self, session_id: &str) -> Result<SessionHistoryResponse, crate::SdkError> {
        let path = format!("/v1/sessions/{session_id}/history");
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn set_display_name(
        &self,
        session_id: &str,
        display_name: &str,
    ) -> Result<SessionSetDisplayNameResponse, crate::SdkError> {
        let body = serde_json::to_value(SessionSetDisplayNameRequest {
            display_name: display_name.to_string(),
        })
        .map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = format!("/v1/sessions/{session_id}/name");
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn append_turn(
        &self,
        session_id: &str,
        request: &SessionAppendTurnRequest,
    ) -> Result<SessionAppendTurnResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = format!("/v1/sessions/{session_id}/turns");
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }
}
