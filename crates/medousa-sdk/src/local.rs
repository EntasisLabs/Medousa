pub mod blocking;

use medousa_types::{
    LocalCatalogResponse, LocalEngineStatus, LocalHardwareResponse, LocalModelDownloadRequest,
    LocalModelDownloadResponse, LocalModelsResponse, ModelDownloadProgress,
};

use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;
#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};

pub struct LocalModelsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

impl LocalModelsApi<'_> {
    #[cfg(feature = "async")]
    pub async fn hardware(&self) -> Result<LocalHardwareResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/v1/local/hardware")
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn catalog(&self) -> Result<LocalCatalogResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/v1/local/catalog")
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn list(&self) -> Result<LocalModelsResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/v1/local/models")
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn engine_status(&self) -> Result<LocalEngineStatus, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/v1/local/engine/status")
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn start_download(
        &self,
        model_id: &str,
    ) -> Result<LocalModelDownloadResponse, crate::SdkError> {
        let body = serde_json::to_value(LocalModelDownloadRequest {
            model_id: model_id.to_string(),
        })
        .map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                "/v1/local/models/download",
                body,
            )
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn remove_model(&self, model_id: &str) -> Result<serde_json::Value, crate::SdkError> {
        self.client
            .transport()
            .delete_json(
                self.client.base_url(),
                &format!("/v1/local/models/{model_id}"),
            )
            .await
    }

    #[cfg(feature = "async")]
    pub async fn download_status(
        &self,
        job_id: &str,
    ) -> Result<ModelDownloadProgress, crate::SdkError> {
        let path = format!("/v1/local/models/download/{}", job_id.trim());
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    #[cfg(all(feature = "async", feature = "sse"))]
    pub fn download_events(
        &self,
        job_id: impl Into<String>,
    ) -> impl Stream<Item = Result<ModelDownloadProgress, crate::SdkError>> + '_ {
        let path = format!("/v1/local/models/download/{}/events", job_id.into().trim());
        let byte_stream = self.client.transport().stream_sse(self.client.base_url(), path);
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }
}
