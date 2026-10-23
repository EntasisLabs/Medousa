use crate::daemon::types::{ArchiveAskJobRequest, WorkCardDetail, WorkspaceCardActionResponse};
use medousa_types::WorkspaceSnapshotQuery;
use tauri::State;

use super::window_request::WorkshopRequest;
use super::sdk::{client_for_request as client, sdk_error};
use crate::embedded_daemon::EmbeddedDaemonState;

#[tauri::command]
pub async fn workspace_get_card(
    state: WorkshopRequest,
    card_id: String,
) -> Result<WorkCardDetail, String> {
    client(&state).await?
        .workspace()
        .get_card(card_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn workspace_cancel_card(
    state: WorkshopRequest,
    card_id: String,
) -> Result<WorkspaceCardActionResponse, String> {
    client(&state).await?
        .workspace()
        .cancel_card(card_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn workspace_archive_card(
    state: WorkshopRequest,
    card_id: String,
    purge_output: Option<bool>,
) -> Result<WorkspaceCardActionResponse, String> {
    let request = ArchiveAskJobRequest {
        purge_output: purge_output.unwrap_or(true),
    };
    client(&state).await?
        .workspace()
        .archive_card(card_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn workspace_retry_card(
    state: WorkshopRequest,
    card_id: String,
) -> Result<WorkspaceCardActionResponse, String> {
    client(&state).await?
        .workspace()
        .retry_card(card_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn workspace_fetch_snapshot(
    state: WorkshopRequest,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    since_revision: Option<u64>,
) -> Result<crate::daemon::types::WorkspaceSnapshot, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .workspace_snapshot(&WorkspaceSnapshotQuery {
                since_revision,
                feed_tail_limit: None,
            })
            .await
            .map_err(|error| error.to_string());
    }
    client(&state).await?
        .workspace()
        .snapshot(&WorkspaceSnapshotQuery {
            since_revision,
            feed_tail_limit: None,
        })
        .await
        .map_err(sdk_error)
}
