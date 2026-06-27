#[cfg(feature = "async")]
use serde::{Serialize, de::DeserializeOwned};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::{decode, path_with_query};

#[cfg(feature = "async")]
pub struct HttpApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl HttpApi<'_> {
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), path)
            .await?;
        decode(value).await
    }

    pub async fn get_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, crate::SdkError> {
        let path = path_with_query(path, query);
        self.get(&path).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, crate::SdkError> {
        let body = serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), path, body)
            .await?;
        decode(value).await
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, crate::SdkError> {
        let value = self
            .client
            .transport()
            .post_empty_json(self.client.base_url(), path)
            .await?;
        decode(value).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, crate::SdkError> {
        let body = serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), path, body)
            .await?;
        decode(value).await
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, crate::SdkError> {
        let body = serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .patch_json(self.client.base_url(), path, body)
            .await?;
        decode(value).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, crate::SdkError> {
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), path)
            .await?;
        decode(value).await
    }
}
