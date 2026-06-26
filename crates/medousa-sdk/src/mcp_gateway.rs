#[cfg(feature = "async")]
use medousa_types::McpGatewayStatusResponse;

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct McpGatewayApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl McpGatewayApi<'_> {
    pub async fn status(&self) -> Result<McpGatewayStatusResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/v1/mcp/gateway/status")
            .await?;
        decode(value).await
    }
}
