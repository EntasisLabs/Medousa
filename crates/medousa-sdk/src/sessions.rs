#[cfg(feature = "async")]
use medousa_types::{
    ActiveSessionTurnResponse, AgentModeProposalListResponse, AgentModeProposalResponse,
    AgentModeScope, CancelActiveSessionTurnResponse, DecideAgentModeProposalRequest,
    SessionActiveTurnsResponse, SessionAgentModeResponse, SessionAppendTurnRequest,
    SessionAppendTurnResponse, SessionCodeBindingResponse, SessionCodeProjectResponse,
    SessionDeleteQuery, SessionDeleteResponse, SessionHistoryListResponse, SessionHistoryResponse,
    SessionSetDisplayNameRequest, SessionSetDisplayNameResponse, SetSessionAgentModeRequest,
    SetSessionCodeBindingRequest, StartSessionCodeProjectRequest,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
#[cfg(feature = "async")]
use crate::transport::{decode, path_with_query};

#[cfg(feature = "async")]
pub struct SessionsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl SessionsApi<'_> {
    pub async fn list(&self, limit: usize) -> Result<SessionHistoryListResponse, crate::SdkError> {
        let path = op_path_query(&ops::SESSIONS_GET, &[], &[("limit", limit.to_string())])?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn history(
        &self,
        session_id: &str,
    ) -> Result<SessionHistoryResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_HISTORY_GET,
            &[("session_id", session_id)],
        )?;
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
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_NAME_PUT,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn agent_mode(
        &self,
        session_id: &str,
    ) -> Result<SessionAgentModeResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_GET,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn set_agent_mode(
        &self,
        session_id: &str,
        request: &SetSessionAgentModeRequest,
    ) -> Result<SessionAgentModeResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_PUT,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn clear_agent_mode(
        &self,
        session_id: &str,
        scope: AgentModeScope,
    ) -> Result<SessionAgentModeResponse, crate::SdkError> {
        let scope = match scope {
            AgentModeScope::Session => "session",
            AgentModeScope::Task => "task",
        };
        let path = op_path_query(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_DELETE,
            &[("session_id", session_id)],
            &[("scope", scope.to_string())],
        )?;
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn agent_mode_proposals(
        &self,
        session_id: &str,
    ) -> Result<AgentModeProposalListResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_PROPOSALS_GET,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn decide_agent_mode_proposal(
        &self,
        session_id: &str,
        proposal_id: &str,
        accept: bool,
    ) -> Result<AgentModeProposalResponse, crate::SdkError> {
        let body = serde_json::to_value(DecideAgentModeProposalRequest { accept })
            .map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_AGENT_MODE_PROPOSALS_BY_PROPOSAL_ID_PUT,
            &[("session_id", session_id), ("proposal_id", proposal_id)],
        )?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn code_binding(
        &self,
        session_id: &str,
    ) -> Result<SessionCodeBindingResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_GET,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn set_code_binding(
        &self,
        session_id: &str,
        work_id: &str,
    ) -> Result<SessionCodeBindingResponse, crate::SdkError> {
        let body = serde_json::to_value(SetSessionCodeBindingRequest {
            work_id: work_id.to_string(),
        })
        .map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_PUT,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn clear_code_binding(
        &self,
        session_id: &str,
    ) -> Result<SessionCodeBindingResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_CODE_BINDING_DELETE,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn start_code_project(
        &self,
        session_id: &str,
        request: &StartSessionCodeProjectRequest,
    ) -> Result<SessionCodeProjectResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_CODE_PROJECT_POST,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn append_turn(
        &self,
        session_id: &str,
        request: &SessionAppendTurnRequest,
    ) -> Result<SessionAppendTurnResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_TURNS_POST,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn delete(
        &self,
        session_id: &str,
        query: &SessionDeleteQuery,
    ) -> Result<SessionDeleteResponse, crate::SdkError> {
        let path = path_with_query(
            &op_path(
                &ops::SESSIONS_BY_SESSION_ID_DELETE,
                &[("session_id", session_id)],
            )?,
            &[("purge_memory", query.purge_memory.to_string())],
        );
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn list_turns(
        &self,
        session_id: &str,
    ) -> Result<SessionActiveTurnsResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_TURNS_GET,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn active_turn(
        &self,
        session_id: &str,
    ) -> Result<ActiveSessionTurnResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_ACTIVE_TURN_GET,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn cancel_active_turn(
        &self,
        session_id: &str,
    ) -> Result<CancelActiveSessionTurnResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_ACTIVE_TURN_POST,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .post_empty_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
