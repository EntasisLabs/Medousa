#[cfg(feature = "async")]
use medousa_types::{
    VaultAddRootRequest, VaultBacklinksQuery, VaultBacklinksResponse, VaultChangesQuery,
    VaultChangesResponse, VaultDeleteResponse, VaultFileContentResponse, VaultNoteContentResponse,
    VaultNotesListResponse, VaultNotesQuery, VaultPutQuery, VaultRootsResponse, VaultSearchQuery,
    VaultSearchResponse, VaultSetActiveRootRequest, VaultTagsListResponse, VaultTagsQuery,
    VaultWriteRequest, VaultWriteResponse,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
#[cfg(feature = "async")]
use crate::transport::decode;

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
    if let Some(cursor) = &query.cursor {
        params.push(("cursor", cursor.clone()));
    }
    if let Some(generation) = query.generation {
        params.push(("generation", generation.to_string()));
    }
    params
}

#[cfg(feature = "async")]
fn vault_changes_query_params(query: &VaultChangesQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(since) = query.since_generation {
        params.push(("since_generation", since.to_string()));
    }
    if let Some(cursor) = &query.cursor {
        params.push(("cursor", cursor.clone()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
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
            .get_json(self.client.base_url(), ops::VAULT_ROOTS_GET.path)
            .await?;
        decode(value).await
    }

    pub async fn add_root(
        &self,
        request: &VaultAddRootRequest,
    ) -> Result<VaultRootsResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::VAULT_ROOTS_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn set_active_root(
        &self,
        request: &VaultSetActiveRootRequest,
    ) -> Result<VaultRootsResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .put_json(self.client.base_url(), ops::VAULT_ACTIVE_PUT.path, body)
            .await?;
        decode(value).await
    }

    pub async fn list_notes(
        &self,
        query: &VaultNotesQuery,
    ) -> Result<VaultNotesListResponse, crate::SdkError> {
        let path = op_path_query(&ops::VAULT_NOTES_GET, &[], &vault_notes_query_params(query))?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn list_changes(
        &self,
        query: &VaultChangesQuery,
    ) -> Result<VaultChangesResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::VAULT_CHANGES_GET,
            &[],
            &vault_changes_query_params(query),
        )?;
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
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), ops::VAULT_NOTES_POST.path, body)
            .await?;
        decode(value).await
    }

    pub async fn get_note(
        &self,
        note_path: &str,
    ) -> Result<VaultNoteContentResponse, crate::SdkError> {
        let path = op_path(
            &ops::VAULT_NOTES_BY_NOTE_PATH_GET,
            &[("note_path", note_path.trim_start_matches('/'))],
        )?;
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
        let path = op_path(
            &ops::VAULT_FILES_BY_FILE_PATH_GET,
            &[("file_path", file_path.trim_start_matches('/'))],
        )?;
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
        let path = op_path_query(
            &ops::VAULT_NOTES_BY_NOTE_PATH_PUT,
            &[("note_path", note_path.trim_start_matches('/'))],
            &params,
        )?;
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
        let path = op_path(
            &ops::VAULT_NOTES_BY_NOTE_PATH_DELETE,
            &[("note_path", note_path.trim_start_matches('/'))],
        )?;
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
        let path = op_path_query(&ops::VAULT_TAGS_GET, &[], &params)?;
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
        let path = op_path_query(
            &ops::VAULT_SEARCH_GET,
            &[],
            &vault_search_query_params(query),
        )?;
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
        let path = op_path_query(&ops::VAULT_BACKLINKS_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn list_trash(
        &self,
        limit: Option<usize>,
    ) -> Result<serde_json::Value, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let route = op_path_query(&ops::VAULT_TRASH_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &route)
            .await?;
        decode(value).await
    }

    pub async fn restore_trash(&self, path: &str) -> Result<serde_json::Value, crate::SdkError> {
        let body = serde_json::json!({ "path": path });
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::VAULT_TRASH_RESTORE_POST.path,
                body,
            )
            .await?;
        decode(value).await
    }

    pub async fn git_detect(&self) -> Result<serde_json::Value, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::VAULT_GIT_DETECT_GET.path)
            .await?;
        decode(value).await
    }

    pub async fn git_status(&self) -> Result<serde_json::Value, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::VAULT_GIT_STATUS_GET.path)
            .await?;
        decode(value).await
    }

    pub async fn git_enable(
        &self,
        enabled: bool,
        init_if_needed: bool,
    ) -> Result<serde_json::Value, crate::SdkError> {
        let body = serde_json::json!({
            "enabled": enabled,
            "initIfNeeded": init_if_needed,
        });
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::VAULT_GIT_ENABLE_POST.path,
                body,
            )
            .await?;
        decode(value).await
    }

    pub async fn git_init(&self) -> Result<serde_json::Value, crate::SdkError> {
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::VAULT_GIT_INIT_POST.path,
                serde_json::json!({}),
            )
            .await?;
        decode(value).await
    }

    pub async fn git_install(&self) -> Result<serde_json::Value, crate::SdkError> {
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::VAULT_GIT_INSTALL_POST.path,
                serde_json::json!({}),
            )
            .await?;
        decode(value).await
    }

    pub async fn git_log(
        &self,
        path: Option<&str>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value, crate::SdkError> {
        let mut params = Vec::new();
        if let Some(path) = path.filter(|p| !p.is_empty()) {
            params.push(("path", path.to_string()));
        }
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        let route = op_path_query(&ops::VAULT_GIT_LOG_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &route)
            .await?;
        decode(value).await
    }

    pub async fn git_commit(
        &self,
        message: &str,
        paths: &[String],
    ) -> Result<serde_json::Value, crate::SdkError> {
        let body = serde_json::json!({
            "message": message,
            "paths": paths,
        });
        let value = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::VAULT_GIT_COMMIT_POST.path,
                body,
            )
            .await?;
        decode(value).await
    }

    pub async fn git_restore(&self, commit: &str, path: &str) -> Result<(), crate::SdkError> {
        let body = serde_json::json!({
            "commit": commit,
            "path": path,
        });
        let _ = self
            .client
            .transport()
            .post_json(
                self.client.base_url(),
                ops::VAULT_GIT_RESTORE_POST.path,
                body,
            )
            .await?;
        Ok(())
    }

    pub async fn git_diff(
        &self,
        path: &str,
        commit: Option<&str>,
    ) -> Result<serde_json::Value, crate::SdkError> {
        let mut params = vec![("path", path.to_string())];
        if let Some(commit) = commit.filter(|c| !c.is_empty()) {
            params.push(("commit", commit.to_string()));
        }
        let route = op_path_query(&ops::VAULT_GIT_DIFF_GET, &[], &params)?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &route)
            .await?;
        decode(value).await
    }

    pub async fn git_worktrees(&self) -> Result<serde_json::Value, crate::SdkError> {
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), ops::VAULT_GIT_WORKTREES_GET.path)
            .await?;
        decode(value).await
    }
}
