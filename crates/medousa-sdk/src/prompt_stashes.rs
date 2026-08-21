#[cfg(feature = "async")]
use medousa_types::{
    CreatePromptStashRequest, DeletePromptStashResponse, PromptStash, PromptStashListResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct PromptStashesApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl PromptStashesApi<'_> {
    pub async fn list(&self) -> Result<PromptStashListResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::PROMPT_STASHES_GET.path)
            .await?;
        decode(value).await
    }

    pub async fn create(
        &self,
        request: &CreatePromptStashRequest,
    ) -> Result<PromptStash, crate::SdkError> {
        let body = serde_json::to_value(request)
            .map_err(|error| crate::SdkError::Serde(error.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::PROMPT_STASHES_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn delete(
        &self,
        stash_id: &str,
    ) -> Result<DeletePromptStashResponse, crate::SdkError> {
        let path = op_path(
            &ops::PROMPT_STASHES_BY_STASH_ID_DELETE,
            &[("stash_id", stash_id)],
        )?;
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
