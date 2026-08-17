#[cfg(feature = "async")]
use medousa_types::{
    AgentModeListResponse, AgentModeTransitionPolicy, ArtifactCommandRequest,
    ArtifactCommandResponse, ArtifactDeleteRequest, ArtifactDeleteResponse, ArtifactFetchRequest,
    ArtifactFetchResponse, ArtifactListUiRequest, ArtifactListUiResponse, ArtifactWriteRequest,
    ArtifactWriteResponse, RuntimeConfigCommandRequest, RuntimeConfigCommandResponse,
    StageRouteCommandRequest, StageRouteCommandResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;

#[cfg(feature = "async")]
pub struct RuntimeApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl RuntimeApi<'_> {
    pub async fn agent_modes(&self) -> Result<AgentModeListResponse, crate::SdkError> {
        self.client.http().get(ops::AGENT_MODES_GET.path).await
    }

    pub async fn agent_mode_transition_policy(
        &self,
    ) -> Result<AgentModeTransitionPolicy, crate::SdkError> {
        self.client
            .http()
            .get(ops::AGENT_MODES_POLICY_GET.path)
            .await
    }

    pub async fn set_agent_mode_transition_policy(
        &self,
        policy: &AgentModeTransitionPolicy,
    ) -> Result<AgentModeTransitionPolicy, crate::SdkError> {
        self.client
            .http()
            .put(ops::AGENT_MODES_POLICY_PUT.path, policy)
            .await
    }

    pub async fn artifact_command(
        &self,
        request: &ArtifactCommandRequest,
    ) -> Result<ArtifactCommandResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_ARTIFACT_COMMAND_POST.path, request)
            .await
    }

    pub async fn artifact_fetch(
        &self,
        request: &ArtifactFetchRequest,
    ) -> Result<ArtifactFetchResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_ARTIFACT_FETCH_POST.path, request)
            .await
    }

    pub async fn artifact_list_ui(
        &self,
        request: &ArtifactListUiRequest,
    ) -> Result<ArtifactListUiResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_ARTIFACT_LIST_UI_POST.path, request)
            .await
    }

    pub async fn artifact_write(
        &self,
        request: &ArtifactWriteRequest,
    ) -> Result<ArtifactWriteResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_ARTIFACT_WRITE_POST.path, request)
            .await
    }

    pub async fn artifact_delete(
        &self,
        request: &ArtifactDeleteRequest,
    ) -> Result<ArtifactDeleteResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_ARTIFACT_DELETE_POST.path, request)
            .await
    }

    pub async fn stage_route_command(
        &self,
        request: &StageRouteCommandRequest,
    ) -> Result<StageRouteCommandResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_STAGE_ROUTE_COMMAND_POST.path, request)
            .await
    }

    pub async fn config_command(
        &self,
        request: &RuntimeConfigCommandRequest,
    ) -> Result<RuntimeConfigCommandResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::RUNTIME_CONFIG_COMMAND_POST.path, request)
            .await
    }
}
