#[cfg(feature = "async")]
use medousa_types::{
    CreateManuscriptRequest, ManuscriptCatalogQuery, ManuscriptCatalogResponse,
    ManuscriptDetailResponse, ManuscriptImportRequest, ManuscriptImportResponse,
    UpdateManuscriptRequest,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::op_path;

#[cfg(feature = "async")]
pub struct ManuscriptsApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
impl ManuscriptsApi<'_> {
    pub async fn list(
        &self,
        query: &ManuscriptCatalogQuery,
    ) -> Result<ManuscriptCatalogResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(prefix) = query
            .prefix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            params.push(("prefix", prefix.to_string()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(skills_only) = query.skills_only {
            params.push(("skills_only", skills_only.to_string()));
        }
        self.client
            .http()
            .get_query(ops::MANUSCRIPTS_GET.path, &params)
            .await
    }

    pub async fn get(
        &self,
        manuscript_id: &str,
    ) -> Result<ManuscriptDetailResponse, crate::SdkError> {
        let id = manuscript_id.trim();
        let path = op_path(
            &ops::MANUSCRIPTS_BY_MANUSCRIPT_ID_GET,
            &[("manuscript_id", id)],
        )?;
        self.client.http().get(&path).await
    }

    pub async fn create(
        &self,
        request: &CreateManuscriptRequest,
    ) -> Result<ManuscriptDetailResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::MANUSCRIPTS_CREATE_POST.path, request)
            .await
    }

    pub async fn update(
        &self,
        manuscript_id: &str,
        request: &UpdateManuscriptRequest,
    ) -> Result<ManuscriptDetailResponse, crate::SdkError> {
        let id = manuscript_id.trim();
        let path = op_path(
            &ops::MANUSCRIPTS_BY_MANUSCRIPT_ID_PATCH,
            &[("manuscript_id", id)],
        )?;
        self.client.http().patch(&path, request).await
    }

    pub async fn import(
        &self,
        request: &ManuscriptImportRequest,
    ) -> Result<ManuscriptImportResponse, crate::SdkError> {
        self.client
            .http()
            .post(ops::MANUSCRIPTS_POST.path, request)
            .await
    }
}
