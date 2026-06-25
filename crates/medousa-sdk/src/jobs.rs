#[cfg(feature = "async")]
use medousa_types::{EnqueueAskRequest, EnqueueResponse, RegisterRecurringPromptRequest, RegisterRecurringResponse};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct JobsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl JobsApi<'_> {
    pub async fn enqueue_ask(&self, request: &EnqueueAskRequest) -> Result<EnqueueResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/jobs/ask", body)
            .await?;
        decode(value).await
    }
}

#[cfg(feature = "async")]
pub struct RecurringApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl RecurringApi<'_> {
    pub async fn register_prompt(
        &self,
        request: &RegisterRecurringPromptRequest,
    ) -> Result<RegisterRecurringResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/recurring/prompt", body)
            .await?;
        decode(value).await
    }
}
