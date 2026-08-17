#[cfg(feature = "async")]
use medousa_types::{
    ArchiveAskJobRequest, WorkCardDetail, WorkspaceCardActionResponse, WorkspaceCardsQuery,
    WorkspaceCardsResponse, WorkspaceFeedQuery, WorkspaceFeedResponse, WorkspaceLinkVaultRequest,
    WorkspaceSnapshot, WorkspaceSnapshotQuery,
};

#[cfg(feature = "async")]
use crate::client::MedousaClient;
use crate::generated::ops;
use crate::op::{op_path, op_path_query};
#[cfg(feature = "async")]
use crate::transport::decode;

#[cfg(feature = "async")]
pub struct WorkspaceApi<'a> {
    pub(crate) client: &'a MedousaClient,
}

#[cfg(feature = "async")]
fn workspace_cards_query_params(query: &WorkspaceCardsQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(session_id) = &query.session_id {
        params.push(("session_id", session_id.clone()));
    }
    if let Some(column) = &query.column {
        params.push(("column", column.clone()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    if let Some(include_terminal) = query.include_terminal {
        params.push(("include_terminal", include_terminal.to_string()));
    }
    params
}

#[cfg(feature = "async")]
fn workspace_feed_query_params(query: &WorkspaceFeedQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(since_id) = &query.since_id {
        params.push(("since_id", since_id.clone()));
    }
    if let Some(since_revision) = query.since_revision {
        params.push(("since_revision", since_revision.to_string()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    if let Some(card_id) = &query.card_id {
        params.push(("card_id", card_id.clone()));
    }
    params
}

#[cfg(feature = "async")]
fn workspace_snapshot_query_params(query: &WorkspaceSnapshotQuery) -> Vec<(&str, String)> {
    let mut params = Vec::new();
    if let Some(since_revision) = query.since_revision {
        params.push(("since_revision", since_revision.to_string()));
    }
    if let Some(feed_tail_limit) = query.feed_tail_limit {
        params.push(("feed_tail_limit", feed_tail_limit.to_string()));
    }
    params
}

#[cfg(feature = "async")]
impl WorkspaceApi<'_> {
    pub async fn list_cards(
        &self,
        query: &WorkspaceCardsQuery,
    ) -> Result<WorkspaceCardsResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::WORKSPACE_CARDS_GET,
            &[],
            &workspace_cards_query_params(query),
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn get_card(&self, card_id: &str) -> Result<WorkCardDetail, crate::SdkError> {
        let path = op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_GET,
            &[("card_id", card_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn cancel_card(
        &self,
        card_id: &str,
    ) -> Result<WorkspaceCardActionResponse, crate::SdkError> {
        let path = op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_CANCEL_POST,
            &[("card_id", card_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_empty_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn archive_card(
        &self,
        card_id: &str,
        request: &ArchiveAskJobRequest,
    ) -> Result<WorkspaceCardActionResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_ARCHIVE_POST,
            &[("card_id", card_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn retry_card(
        &self,
        card_id: &str,
    ) -> Result<WorkspaceCardActionResponse, crate::SdkError> {
        let path = op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_RETRY_POST,
            &[("card_id", card_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_empty_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn link_vault(
        &self,
        card_id: &str,
        request: &WorkspaceLinkVaultRequest,
    ) -> Result<WorkspaceCardActionResponse, crate::SdkError> {
        let body =
            serde_json::to_value(request).map_err(|e| crate::SdkError::Serde(e.to_string()))?;
        let path = op_path(
            &ops::WORKSPACE_CARDS_BY_CARD_ID_LINK_VAULT_POST,
            &[("card_id", card_id.trim())],
        )?;
        let value = self
            .client
            .transport()
            .post_json(self.client.base_url(), &path, body)
            .await?;
        decode(value).await
    }

    pub async fn feed(
        &self,
        query: &WorkspaceFeedQuery,
    ) -> Result<WorkspaceFeedResponse, crate::SdkError> {
        let path = op_path_query(
            &ops::WORKSPACE_FEED_GET,
            &[],
            &workspace_feed_query_params(query),
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }

    pub async fn snapshot(
        &self,
        query: &WorkspaceSnapshotQuery,
    ) -> Result<WorkspaceSnapshot, crate::SdkError> {
        let path = op_path_query(
            &ops::WORKSPACE_SNAPSHOT_GET,
            &[],
            &workspace_snapshot_query_params(query),
        )?;
        let value = self
            .client
            .transport()
            .get_json(self.client.base_url(), &path)
            .await?;
        decode(value).await
    }
}
