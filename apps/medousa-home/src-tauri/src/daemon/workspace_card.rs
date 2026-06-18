use crate::daemon::types::{WorkCardDetail, WorkspaceCardActionResponse};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn workspace_get_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkCardDetail, String> {
    let encoded = urlencoding::encode(card_id.trim());
    workshop_http::get_json(&state, &format!("/v1/workspace/cards/{encoded}")).await
}

#[tauri::command]
pub async fn workspace_cancel_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkspaceCardActionResponse, String> {
    let encoded = urlencoding::encode(card_id.trim());
    workshop_http::post_empty_json(&state, &format!("/v1/workspace/cards/{encoded}/cancel")).await
}

#[tauri::command]
pub async fn workspace_archive_card(
    state: State<'_, DaemonState>,
    card_id: String,
    purge_output: Option<bool>,
) -> Result<WorkspaceCardActionResponse, String> {
    let encoded = urlencoding::encode(card_id.trim());
    workshop_http::post_json(
        &state,
        &format!("/v1/workspace/cards/{encoded}/archive"),
        &serde_json::json!({ "purge_output": purge_output.unwrap_or(true) }),
    )
    .await
}

#[tauri::command]
pub async fn workspace_retry_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkspaceCardActionResponse, String> {
    let encoded = urlencoding::encode(card_id.trim());
    workshop_http::post_empty_json(&state, &format!("/v1/workspace/cards/{encoded}/retry")).await
}

#[tauri::command]
pub async fn workspace_fetch_snapshot(
    state: State<'_, DaemonState>,
    since_revision: Option<u64>,
) -> Result<crate::daemon::types::WorkspaceSnapshot, String> {
    let path = if let Some(revision) = since_revision {
        format!("/v1/workspace/snapshot?since_revision={revision}")
    } else {
        "/v1/workspace/snapshot".to_string()
    };
    workshop_http::get_json(&state, &path).await
}
