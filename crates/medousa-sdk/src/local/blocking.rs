#[cfg(feature = "blocking")]
use medousa_types::{
    LocalCatalogResponse, LocalEngineStatus, LocalHardwareResponse, LocalModelDownloadRequest,
    LocalModelDownloadResponse, LocalModelsResponse,
};

#[cfg(feature = "blocking")]
use crate::MedousaClient;

#[cfg(feature = "blocking")]
pub struct BlockingLocalModelsClient {
    inner: MedousaClient,
}

#[cfg(feature = "blocking")]
impl BlockingLocalModelsClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            inner: MedousaClient::new(base_url),
        }
    }

    fn blocking_get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, crate::SdkError> {
        let url = format!("{}{}", self.inner.base_url(), path);
        let response = reqwest::blocking::Client::new()
            .get(&url)
            .send()
            .map_err(|e| crate::SdkError::Http(e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| crate::SdkError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(crate::SdkError::Http(format!("{status}: {body}")));
        }
        serde_json::from_str(&body).map_err(Into::into)
    }

    fn blocking_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, crate::SdkError> {
        let url = format!("{}{}", self.inner.base_url(), path);
        let response = reqwest::blocking::Client::new()
            .post(&url)
            .json(body)
            .send()
            .map_err(|e| crate::SdkError::Http(e.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|e| crate::SdkError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(crate::SdkError::Http(format!("{status}: {text}")));
        }
        serde_json::from_str(&text).map_err(Into::into)
    }

    pub fn hardware(&self) -> Result<LocalHardwareResponse, crate::SdkError> {
        self.blocking_get("/v1/local/hardware")
    }

    pub fn catalog(&self) -> Result<LocalCatalogResponse, crate::SdkError> {
        self.blocking_get("/v1/local/catalog")
    }

    pub fn list(&self) -> Result<LocalModelsResponse, crate::SdkError> {
        self.blocking_get("/v1/local/models")
    }

    pub fn engine_status(&self) -> Result<LocalEngineStatus, crate::SdkError> {
        self.blocking_get("/v1/local/engine/status")
    }

    pub fn start_download(
        &self,
        model_id: &str,
    ) -> Result<LocalModelDownloadResponse, crate::SdkError> {
        self.blocking_post(
            "/v1/local/models/download",
            &LocalModelDownloadRequest {
                model_id: model_id.to_string(),
            },
        )
    }

    pub fn download_status(
        &self,
        job_id: &str,
    ) -> Result<medousa_types::ModelDownloadProgress, crate::SdkError> {
        self.blocking_get(&format!("/v1/local/models/download/{job_id}"))
    }

    pub fn remove_model(&self, model_id: &str) -> Result<(), crate::SdkError> {
        let url = format!("{}{}", self.inner.base_url(), format!("/v1/local/models/{model_id}"));
        let response = reqwest::blocking::Client::new()
            .delete(&url)
            .send()
            .map_err(|e| crate::SdkError::Http(e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| crate::SdkError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(crate::SdkError::Http(format!("{status}: {body}")));
        }
        Ok(())
    }
}
