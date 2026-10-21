//! Workshop-owned creative projects. All requests use the generated operation table.
use crate::{
    client::MedousaClient,
    generated::ops,
    op::{op_path, op_path_query},
};
use medousa_types::creative::*;
pub struct CreativeApi<'a> {
    pub(crate) client: &'a MedousaClient,
}
impl CreativeApi<'_> {
    pub async fn create_job(
        &self,
        id: &str,
        request: &CreativeJobRequest,
    ) -> Result<CreativeJob, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_JOBS_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn jobs(&self, id: &str) -> Result<CreativeJobListResponse, crate::SdkError> {
        self.client
            .http()
            .get(&op_path(
                &ops::CREATE_PROJECTS_BY_ID_JOBS_GET,
                &[("id", id)],
            )?)
            .await
    }
    pub async fn job(&self, id: &str, job_id: &str) -> Result<CreativeJob, crate::SdkError> {
        self.client
            .http()
            .get(&op_path(
                &ops::CREATE_PROJECTS_BY_ID_JOBS_BY_JOB_ID_GET,
                &[("id", id), ("job_id", job_id)],
            )?)
            .await
    }
    pub async fn decide_job(
        &self,
        id: &str,
        job_id: &str,
        request: &CreativeProposalDecision,
    ) -> Result<CreativeJob, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(
                    &ops::CREATE_PROJECTS_BY_ID_JOBS_BY_JOB_ID_DECISION_POST,
                    &[("id", id), ("job_id", job_id)],
                )?,
                request,
            )
            .await
    }

    pub async fn list(
        &self,
        query: &CreativeListQuery,
    ) -> Result<CreativeListResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(cursor) = &query.cursor {
            params.push(("cursor", cursor.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        let path = op_path_query(&ops::CREATE_PROJECTS_GET, &[], &params)?;
        self.client.http().get(&path).await
    }
    pub async fn create(
        &self,
        request: &CreativeCreateRequest,
    ) -> Result<CreativeDocument, crate::SdkError> {
        self.client
            .http()
            .post(ops::CREATE_PROJECTS_POST.path, request)
            .await
    }
    pub async fn get(&self, id: &str) -> Result<CreativeMutationResponse, crate::SdkError> {
        self.client
            .http()
            .get(&op_path(&ops::CREATE_PROJECTS_BY_ID_GET, &[("id", id)])?)
            .await
    }
    pub async fn context(
        &self,
        id: &str,
        request: &CreativeContextRequest,
    ) -> Result<CreativeSelectionContext, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_CONTEXT_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn transact(
        &self,
        id: &str,
        request: &CreativeTransaction,
    ) -> Result<CreativeMutationResponse, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_TRANSACTIONS_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn history(
        &self,
        id: &str,
        request: &CreativeHistoryRequest,
    ) -> Result<CreativeMutationResponse, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_HISTORY_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn delete(
        &self,
        id: &str,
        request: &CreativeDeleteRequest,
    ) -> Result<CreativeDeleteResponse, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_DELETE_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn import_asset(
        &self,
        id: &str,
        request: &CreativeAssetImportRequest,
    ) -> Result<CreativeAsset, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_ASSETS_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn asset(
        &self,
        id: &str,
        asset_id: &str,
    ) -> Result<CreativeAssetData, crate::SdkError> {
        self.client
            .http()
            .get(&op_path(
                &ops::CREATE_PROJECTS_BY_ID_ASSETS_BY_ASSET_ID_GET,
                &[("id", id), ("asset_id", asset_id)],
            )?)
            .await
    }
    pub async fn promote_media(
        &self,
        id: &str,
        request: &CreativePromoteMediaRequest,
    ) -> Result<CreativeAsset, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(
                    &ops::CREATE_PROJECTS_BY_ID_PROMOTE_MEDIA_POST,
                    &[("id", id)],
                )?,
                request,
            )
            .await
    }
    pub async fn proposals(
        &self,
        id: &str,
    ) -> Result<CreativeProposalListResponse, crate::SdkError> {
        self.client
            .http()
            .get(&op_path(
                &ops::CREATE_PROJECTS_BY_ID_PROPOSALS_GET,
                &[("id", id)],
            )?)
            .await
    }
    pub async fn propose(
        &self,
        id: &str,
        request: &CreativeProposalRequest,
    ) -> Result<CreativeProposal, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(&ops::CREATE_PROJECTS_BY_ID_PROPOSALS_POST, &[("id", id)])?,
                request,
            )
            .await
    }
    pub async fn decide(
        &self,
        id: &str,
        proposal_id: &str,
        request: &CreativeProposalDecision,
    ) -> Result<CreativeMutationResponse, crate::SdkError> {
        self.client
            .http()
            .post(
                &op_path(
                    &ops::CREATE_PROJECTS_BY_ID_PROPOSALS_BY_PROPOSAL_ID_DECISION_POST,
                    &[("id", id), ("proposal_id", proposal_id)],
                )?,
                request,
            )
            .await
    }
}
