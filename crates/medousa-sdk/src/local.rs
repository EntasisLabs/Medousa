pub mod blocking;

use medousa_types::{
    LocalCatalogResponse, LocalEngineStatus, LocalHardwareResponse, LocalModelDownloadRequest,
    LocalModelDownloadResponse, LocalModelsResponse, ModelDownloadProgress,
};

use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::Stream;
#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::StreamExt;

pub struct LocalModelsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

impl LocalModelsApi<'_> {
    #[cfg(feature = "async")]
    pub async fn hardware(&self) -> Result<LocalHardwareResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::LOCAL_HARDWARE_GET.path)
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn catalog(&self) -> Result<LocalCatalogResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::LOCAL_CATALOG_GET.path)
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn list(&self) -> Result<LocalModelsResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::LOCAL_MODELS_GET.path)
            .await?;
        decode(value).await
    }

    #[cfg(feature = "async")]
    pub async fn engine_status(&self) -> Result<LocalEngineStatus, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::LOCAL_ENGINE_STATUS_GET.path)
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
                ops::LOCAL_MODELS_DOWNLOAD_POST.path,
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
                &op_path(
                    &ops::LOCAL_MODELS_BY_MODEL_ID_DELETE,
                    &[("model_id", model_id)],
                )?,
            )
            .await
    }

    #[cfg(feature = "async")]
    pub async fn download_status(
        &self,
        job_id: &str,
    ) -> Result<ModelDownloadProgress, crate::SdkError> {
        let path = op_path(
            &ops::LOCAL_MODELS_DOWNLOAD_BY_JOB_ID_GET,
            &[("job_id", job_id.trim())],
        )?;
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
        let job_id = job_id.into();
        let path = op_path(
            &ops::LOCAL_MODELS_DOWNLOAD_BY_JOB_ID_EVENTS_GET,
            &[("job_id", job_id.trim())],
        )
        .expect("generated path");
        let byte_stream = self
            .client
            .transport()
            .stream_sse(self.client.base_url(), path);
        SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data)))
    }
}
