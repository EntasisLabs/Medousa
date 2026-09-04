#[cfg(feature = "async")]
use medousa_types::{
    BotListResponse, BotOpenResponse, BotProfile, CreateBotRequest, DuplicateBotRequest,
    SessionBotResponse, SetBotArchivedRequest, SetSessionBotRequest, UpdateBotRequest,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct BotsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl BotsApi<'_> {
    pub async fn list(&self) -> Result<BotListResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::BOTS_GET.path)
            .await?;
        decode(value).await
    }

    pub async fn create(
        &self,
        request: &CreateBotRequest,
    ) -> Result<BotOpenResponse, crate::SdkError> {
        let body = serde_json::to_value(request)
            .map_err(|error| crate::SdkError::Serde(error.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::BOTS_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn get(&self, bot_id: &str) -> Result<BotProfile, crate::SdkError> {
        let path = op_path(&ops::BOTS_BY_BOT_ID_GET, &[("bot_id", bot_id)])?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn update(
        &self,
        bot_id: &str,
        request: &UpdateBotRequest,
    ) -> Result<BotProfile, crate::SdkError> {
        let path = op_path(&ops::BOTS_BY_BOT_ID_PUT, &[("bot_id", bot_id)])?;
        let body = serde_json::to_value(request)
            .map_err(|error| crate::SdkError::Serde(error.to_string()))?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn set_archived(
        &self,
        bot_id: &str,
        request: &SetBotArchivedRequest,
    ) -> Result<BotProfile, crate::SdkError> {
        let path = op_path(&ops::BOTS_BY_BOT_ID_ARCHIVE_PUT, &[("bot_id", bot_id)])?;
        let body = serde_json::to_value(request)
            .map_err(|error| crate::SdkError::Serde(error.to_string()))?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn duplicate(
        &self,
        bot_id: &str,
        request: &DuplicateBotRequest,
    ) -> Result<BotOpenResponse, crate::SdkError> {
        let path = op_path(&ops::BOTS_BY_BOT_ID_DUPLICATE_POST, &[("bot_id", bot_id)])?;
        let body = serde_json::to_value(request)
            .map_err(|error| crate::SdkError::Serde(error.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn open(&self, bot_id: &str) -> Result<BotOpenResponse, crate::SdkError> {
        let path = op_path(&ops::BOTS_BY_BOT_ID_OPEN_POST, &[("bot_id", bot_id)])?;
        let value = self
            .client
            .transport()
            .post_empty_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn session(&self, session_id: &str) -> Result<SessionBotResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_BOT_GET,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn bind_session(
        &self,
        session_id: &str,
        request: &SetSessionBotRequest,
    ) -> Result<SessionBotResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_BOT_PUT,
            &[("session_id", session_id)],
        )?;
        let body = serde_json::to_value(request)
            .map_err(|error| crate::SdkError::Serde(error.to_string()))?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn unbind_session(
        &self,
        session_id: &str,
    ) -> Result<SessionBotResponse, crate::SdkError> {
        let path = op_path(
            &ops::SESSIONS_BY_SESSION_ID_BOT_DELETE,
            &[("session_id", session_id)],
        )?;
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
