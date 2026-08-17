#[cfg(feature = "async")]
use medousa_types::{CapabilityListResponse, CapabilityResolveResponse};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;

#[cfg(feature = "async")]
pub struct CapabilitiesApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl CapabilitiesApi<'_> {
    pub async fn list(&self) -> Result<CapabilityListResponse, crate::SdkError> {
        self.client.http().get(ops::CAPABILITIES_GET.path).await
    }

    pub async fn get(
        &self,
        capability_id: &str,
    ) -> Result<CapabilityResolveResponse, crate::SdkError> {
        let id = capability_id.trim();
        let path = op_path(
            &ops::CAPABILITIES_BY_CAPABILITY_ID_GET,
            &[("capability_id", id)],
        )?;
        self.client.http().get(&path).await
    }

    pub async fn reindex(&self) -> Result<serde_json::Value, crate::SdkError> {
        self.client
            .http()
            .post_empty(ops::CAPABILITIES_REINDEX_POST.path)
            .await
    }
}
