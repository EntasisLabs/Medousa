#[cfg(feature = "async")]
use medousa_types::{
    DeleteRecurringResponse, RecurringDeliveryResponse, RecurringListQuery, RecurringListResponse,
    RecurringRunsQuery, RecurringRunsResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse, UpdateRecurringRequest, UpdateRecurringResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
#[cfg(feature = "async")]
use crate::transport::{decode, path_with_query};

#[cfg(feature = "async")]
pub struct RecurringApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl RecurringApi<'_> {
    pub async fn register_prompt(
        &self,
        request: &RegisterRecurringPromptRequest,
    ) -> Result<RegisterRecurringResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::RECURRING_PROMPT_POST.path,
                body,
            )
            .await?;
        decode(value).await
    }

    pub async fn list(
        &self,
        query: &RecurringListQuery,
    ) -> Result<RecurringListResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(enabled_only) = query.enabled_only {
            params.push(("enabled_only", enabled_only.to_string()));
        }
        let path = op_path_query(&ops::RECURRING_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn update(
        &self,
        recurring_id: &str,
        request: &UpdateRecurringRequest,
    ) -> Result<UpdateRecurringResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::RECURRING_BY_RECURRING_ID_PATCH,
            &[("recurring_id", recurring_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .patch_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn delete(
        &self,
        recurring_id: &str,
    ) -> Result<DeleteRecurringResponse, crate::SdkError> {
        let path = op_path(
            &ops::RECURRING_BY_RECURRING_ID_DELETE,
            &[("recurring_id", recurring_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn runs(
        &self,
        recurring_id: &str,
        query: &RecurringRunsQuery,
    ) -> Result<RecurringRunsResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        let path = path_with_query(
            &op_path(
                &ops::RECURRING_BY_RECURRING_ID_RUNS_GET,
                &[("recurring_id", recurring_id.trim())],
            )?,
            &params,
        );
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn delivery_status(
        &self,
        recurring_id: &str,
    ) -> Result<RecurringDeliveryResponse, crate::SdkError> {
        let path = op_path(
            &ops::RECURRING_BY_RECURRING_ID_DELIVERY_GET,
            &[("recurring_id", recurring_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
