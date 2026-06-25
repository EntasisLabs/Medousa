#[cfg(feature = "async")]
use medousa_types::{InteractiveTurnRequest, InteractiveTurnResponse};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct InteractiveApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl InteractiveApi<'_> {
    pub async fn start_turn(
        &self,
        request: &InteractiveTurnRequest,
    ) -> Result<InteractiveTurnResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/interactive/turn", body)
            .await?;
        decode(value).await
    }
}
