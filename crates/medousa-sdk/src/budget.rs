#[cfg(feature = "async")]
use medousa_types::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct BudgetApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl BudgetApi<'_> {
    pub async fn list(
        &self,
        pending_only: bool,
    ) -> Result<TurnBudgetRequestListResponse, crate::SdkError> {
        let path = if pending_only {
            "/v1/turns/budget-requests?status=pending&limit=20".to_string()
        } else {
            "/v1/turns/budget-requests?limit=20".to_string()
        };
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn approve(
        &self,
        request_id: &str,
        body: &TurnBudgetApproveRequest,
    ) -> Result<TurnBudgetRequestResponse, crate::SdkError> {
        let payload = serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = format!(
            "/v1/turns/budget-requests/{}/approve",
            request_id.trim()
        );
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, payload)
            .await?;
        decode(value).await
    }

    pub async fn deny(
        &self,
        request_id: &str,
        body: &TurnBudgetDenyRequest,
    ) -> Result<TurnBudgetRequestResponse, crate::SdkError> {
        let payload = serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = format!("/v1/turns/budget-requests/{}/deny", request_id.trim());
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, payload)
            .await?;
        decode(value).await
    }
}
