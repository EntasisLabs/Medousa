#[cfg(feature = "async")]
use medousa_types::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestRecord, TurnBudgetRequestResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
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
            op_path_query(
                &ops::TURNS_BUDGET_REQUESTS_GET,
                &[],
                &[
                    ("status", "pending".to_string()),
                    ("limit", "20".to_string()),
                ],
            )?
        } else {
            op_path_query(
                &ops::TURNS_BUDGET_REQUESTS_GET,
                &[],
                &[("limit", "20".to_string())],
            )?
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
        let payload =
            serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::TURNS_BUDGET_REQUESTS_BY_REQUEST_ID_APPROVE_POST,
            &[("request_id", request_id.trim())],
        )?;
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
        let payload =
            serde_json::to_value(body).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::TURNS_BUDGET_REQUESTS_BY_REQUEST_ID_DENY_POST,
            &[("request_id", request_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, payload)
            .await?;
        decode(value).await
    }

    pub async fn get(&self, request_id: &str) -> Result<TurnBudgetRequestRecord, crate::SdkError> {
        let path = op_path(
            &ops::TURNS_BUDGET_REQUESTS_BY_REQUEST_ID_GET,
            &[("request_id", request_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
