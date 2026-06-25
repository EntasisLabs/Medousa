#[cfg(feature = "async")]
use medousa_types::{
    ArtifactCommandRequest, ArtifactCommandResponse, RuntimeConfigCommandRequest,
    RuntimeConfigCommandResponse, StageRouteCommandRequest, StageRouteCommandResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct RuntimeApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl RuntimeApi<'_> {
    pub async fn artifact_command(
        &self,
        request: &ArtifactCommandRequest,
    ) -> Result<ArtifactCommandResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/runtime/artifact/command", body)
            .await?;
        decode(value).await
    }

    pub async fn stage_route_command(
        &self,
        request: &StageRouteCommandRequest,
    ) -> Result<StageRouteCommandResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/runtime/stage-route/command", body)
            .await?;
        decode(value).await
    }

    pub async fn config_command(
        &self,
        request: &RuntimeConfigCommandRequest,
    ) -> Result<RuntimeConfigCommandResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/runtime/config/command", body)
            .await?;
        decode(value).await
    }
}
