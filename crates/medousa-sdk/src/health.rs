#[cfg(feature = "async")]
use medousa_types::{HealthResponse, IngestRequest, IngestResponse};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct HealthApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl HealthApi<'_> {
    pub async fn get(&self) -> Result<HealthResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/health")
            .await?;
        decode(value).await
    }
}

#[cfg(feature = "async")]
pub struct IngestApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl IngestApi<'_> {
    pub async fn post(&self, request: &IngestRequest) -> Result<IngestResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/ingest", body)
            .await?;
        decode(value).await
    }
}
