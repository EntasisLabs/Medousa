use medousa_types::HealthResponse;
#[cfg(feature = "async")]
use medousa_types::{IngestRequest, IngestResponse};

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
        let path = crate::generated::ops::HEALTH_GET.path;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), path)
            .await?;
        decode_health(value, path)
    }
}

pub(crate) fn decode_health(
    value: serde_json::Value,
    path: &str,
) -> Result<HealthResponse, crate::SdkError> {
    if value.get("runtime").is_none() {
        return Err(crate::SdkError::Compatibility(format!(
            "GET {path} responder omitted the required runtime descriptor; client expects daemon contract revision {}",
            medousa_types::DAEMON_API_CONTRACT_REVISION,
        )));
    }
    let response: HealthResponse = serde_json::from_value(value).map_err(|error| {
        crate::SdkError::Compatibility(format!(
            "GET {path} responder returned an invalid health contract: {error}"
        ))
    })?;
    if response.runtime.contract_revision != medousa_types::DAEMON_API_CONTRACT_REVISION {
        return Err(crate::SdkError::Compatibility(format!(
            "GET {path} responder authority {} build {} ({}) uses daemon contract revision {}; client expects {}",
            response.runtime.authority_id,
            response.runtime.build_revision,
            response.runtime.deployment_target,
            response.runtime.contract_revision,
            medousa_types::DAEMON_API_CONTRACT_REVISION,
        )));
    }
    if response.runtime.base_schema_revision == 0 {
        return Err(crate::SdkError::Compatibility(format!(
            "GET {path} responder authority {} build {} reported invalid base schema revision 0",
            response.runtime.authority_id, response.runtime.build_revision,
        )));
    }
    Ok(response)
}

#[cfg(feature = "async")]
pub(crate) async fn missing_authority_error(
    client: &MedousaClient,
    method: &str,
    path: &str,
) -> crate::SdkError {
    let responder = match client.health().get().await {
        Ok(health) => format!(
            "responder authority {} build {} ({}) contract revision {}",
            health.runtime.authority_id,
            health.runtime.build_revision,
            health.runtime.deployment_target,
            health.runtime.contract_revision,
        ),
        Err(error) => format!("responder identity unavailable: {error}"),
    };
    crate::SdkError::Compatibility(format!(
        "{method} {path} response omitted required authority_id; {responder}; client expects daemon contract revision {}",
        medousa_types::DAEMON_API_CONTRACT_REVISION,
    ))
}

#[cfg(feature = "async")]
pub struct IngestApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl IngestApi<'_> {
    pub async fn post(&self, request: &IngestRequest) -> Result<IngestResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                crate::generated::ops::INGEST_POST.path,
                body,
            )
            .await?;
        decode(value).await
    }
}
