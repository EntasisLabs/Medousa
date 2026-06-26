#[cfg(feature = "async")]
use medousa_types::{CapabilityListResponse, CapabilityResolveResponse};

#[cfg(feature = "async")]
use crate::client::MedousaClient;

#[cfg(feature = "async")]
pub struct CapabilitiesApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl CapabilitiesApi<'_> {
    pub async fn list(&self) -> Result<CapabilityListResponse, crate::SdkError> {
        self.client.http().get("/v1/capabilities").await
    }

    pub async fn get(&self, capability_id: &str) -> Result<CapabilityResolveResponse, crate::SdkError> {
        let id = capability_id.trim();
        let path = format!("/v1/capabilities/{}", urlencoding::encode(id));
        self.client.http().get(&path).await
    }

    pub async fn reindex(&self) -> Result<serde_json::Value, crate::SdkError> {
        self.client.http().post_empty("/v1/capabilities/reindex").await
    }
}
