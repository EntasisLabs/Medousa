#[cfg(feature = "async")]
use medousa_types::{
    ArtifactCommandRequest, ArtifactCommandResponse, ArtifactFetchRequest, ArtifactFetchResponse,
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, StageRouteCommandRequest,
    StageRouteCommandResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;

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
        self.client
            .http()
            .post("/v1/runtime/artifact/command", request)
            .await
    }

    pub async fn artifact_fetch(
        &self,
        request: &ArtifactFetchRequest,
    ) -> Result<ArtifactFetchResponse, crate::SdkError> {
        self.client
            .http()
            .post("/v1/runtime/artifact/fetch", request)
            .await
    }

    pub async fn stage_route_command(
        &self,
        request: &StageRouteCommandRequest,
    ) -> Result<StageRouteCommandResponse, crate::SdkError> {
        self.client
            .http()
            .post("/v1/runtime/stage-route/command", request)
            .await
    }

    pub async fn config_command(
        &self,
        request: &RuntimeConfigCommandRequest,
    ) -> Result<RuntimeConfigCommandResponse, crate::SdkError> {
        self.client
            .http()
            .post("/v1/runtime/config/command", request)
            .await
    }
}
