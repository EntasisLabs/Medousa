#[cfg(feature = "async")]
use medousa_types::{
    VaultAddRootRequest, VaultBacklinksQuery, VaultBacklinksResponse, VaultDeleteResponse,
    VaultFileContentResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultNotesQuery,
    VaultPutQuery, VaultRootsResponse, VaultSearchQuery, VaultSearchResponse,
    VaultSetActiveRootRequest, VaultTagsListResponse, VaultTagsQuery, VaultWriteRequest,
    VaultWriteResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
#[cfg(feature = "async")]
use crate::transport::{decode, path_with_query};

#[cfg(feature = "async")]
pub struct VaultApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
fn vault_notes_query_params(query: &VaultNotesQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(prefix) = &query.prefix {
        params.push(("prefix", prefix.clone()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    if let Some(tags) = &query.tags {
        params.push(("tags", tags.clone()));
    }
    if let Some(tag_prefix) = &query.tag_prefix {
        params.push(("tag_prefix", tag_prefix.clone()));
    }
    params
}

#[cfg(feature = "async")]
fn vault_search_query_params(query: &VaultSearchQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(q) = &query.q {
        params.push(("q", q.clone()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    if let Some(tags) = &query.tags {
        params.push(("tags", tags.clone()));
    }
    params
}

#[cfg(feature = "async")]
impl VaultApi<'_> {
    pub async fn list_roots(&self) -> Result<VaultRootsResponse, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), "/v1/vault/roots")
            .await?;
        decode(value).await
    }

    pub async fn add_root(
        &self,
        request: &VaultAddRootRequest,
    ) -> Result<VaultRootsResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/vault/roots", body)
            .await?;
        decode(value).await
    }

    pub async fn set_active_root(
        &self,
        request: &VaultSetActiveRootRequest,
    ) -> Result<VaultRootsResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), "/v1/vault/active", body)
            .await?;
        decode(value).await
    }

    pub async fn list_notes(
        &self,
        query: &VaultNotesQuery,
    ) -> Result<VaultNotesListResponse, crate::SdkError> {
        let path = path_with_query("/v1/vault/notes", &vault_notes_query_params(query));
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn create_note(
        &self,
        request: &VaultWriteRequest,
    ) -> Result<VaultWriteResponse, crate::SdkError> {
        let body = serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), "/v1/vault/notes", body)
            .await?;
        decode(value).await
    }

    pub async fn get_note(
        &self,
        note_path: &str,
    ) -> Result<VaultNoteContentResponse, crate::SdkError> {
        let path = format!("/v1/vault/notes/{}", note_path.trim_start_matches('/'));
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn get_file(
        &self,
        file_path: &str,
    ) -> Result<VaultFileContentResponse, crate::SdkError> {
        let path = format!("/v1/vault/files/{}", file_path.trim_start_matches('/'));
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn update_note(
        &self,
        note_path: &str,
        content: &str,
        query: &VaultPutQuery,
        if_match: Option<&str>,
    ) -> Result<VaultWriteResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(session_id) = &query.session_id {
            params.push(("session_id", session_id.clone()));
        }
        if let Some(auto_workshop_tags) = query.auto_workshop_tags {
            params.push(("auto_workshop_tags", auto_workshop_tags.to_string()));
        }
        let path = path_with_query(
            &format!("/v1/vault/notes/{}", note_path.trim_start_matches('/')),
            &params,
        );
        let mut headers = Vec::new();
        if let Some(etag) = if_match {
            headers.push(("if-match", etag.to_string()));
        }
        let value = self
            .client
            .transport()
            .put_text(self.client.base_url(), &path, content.to_string(), headers)
            .await?;
        decode(value).await
    }

    pub async fn delete_note(
        &self,
        note_path: &str,
    ) -> Result<VaultDeleteResponse, crate::SdkError> {
        let path = format!("/v1/vault/notes/{}", note_path.trim_start_matches('/'));
        let value = self
            .client
            .transport()
            .delete_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn list_tags(
        &self,
        query: &VaultTagsQuery,
    ) -> Result<VaultTagsListResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(prefix) = &query.prefix {
            params.push(("prefix", prefix.clone()));
        }
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }
        let path = path_with_query("/v1/vault/tags", &params);
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn search(
        &self,
        query: &VaultSearchQuery,
    ) -> Result<VaultSearchResponse, crate::SdkError> {
        let path = path_with_query("/v1/vault/search", &vault_search_query_params(query));
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn backlinks(
        &self,
        query: &VaultBacklinksQuery,
    ) -> Result<VaultBacklinksResponse, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(path) = &query.path {
            params.push(("path", path.clone()));
        }
        let path = path_with_query("/v1/vault/backlinks", &params);
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
