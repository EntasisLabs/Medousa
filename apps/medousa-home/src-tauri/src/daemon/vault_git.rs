use serde_json::Value;

use super::window_request::WorkshopRequest;
use super::sdk::{client_for_request as client, sdk_error};

#[tauri::command]
pub async fn vault_git_detect(state: WorkshopRequest) -> Result<Value, String> {
    client(&state).await?.vault().git_detect().await.map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_status(state: WorkshopRequest) -> Result<Value, String> {
    client(&state).await?.vault().git_status().await.map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_enable(
    state: WorkshopRequest,
    enabled: bool,
    init_if_needed: bool,
) -> Result<Value, String> {
    client(&state).await?
        .vault()
        .git_enable(enabled, init_if_needed)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_init(state: WorkshopRequest) -> Result<Value, String> {
    client(&state).await?.vault().git_init().await.map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_install(state: WorkshopRequest) -> Result<Value, String> {
    client(&state).await?
        .vault()
        .git_install()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_log(
    state: WorkshopRequest,
    path: Option<String>,
    limit: Option<usize>,
) -> Result<Value, String> {
    client(&state).await?
        .vault()
        .git_log(path.as_deref(), limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_commit(
    state: WorkshopRequest,
    message: String,
    paths: Option<Vec<String>>,
) -> Result<Value, String> {
    let paths = paths.unwrap_or_default();
    client(&state).await?
        .vault()
        .git_commit(&message, &paths)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_restore(
    state: WorkshopRequest,
    commit: String,
    path: String,
) -> Result<(), String> {
    client(&state).await?
        .vault()
        .git_restore(&commit, &path)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_diff(
    state: WorkshopRequest,
    path: String,
    commit: Option<String>,
) -> Result<Value, String> {
    client(&state).await?
        .vault()
        .git_diff(&path, commit.as_deref())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_worktrees(state: WorkshopRequest) -> Result<Value, String> {
    client(&state).await?
        .vault()
        .git_worktrees()
        .await
        .map_err(sdk_error)
}
