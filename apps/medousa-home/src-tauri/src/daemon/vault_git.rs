use serde_json::Value;
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn vault_git_detect(state: State<'_, DaemonState>) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_detect()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_status(state: State<'_, DaemonState>) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_status()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_enable(
    state: State<'_, DaemonState>,
    enabled: bool,
    init_if_needed: bool,
) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_enable(enabled, init_if_needed)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_init(state: State<'_, DaemonState>) -> Result<Value, String> {
    client(&state).vault().git_init().await.map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_install(state: State<'_, DaemonState>) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_install()
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_log(
    state: State<'_, DaemonState>,
    path: Option<String>,
    limit: Option<usize>,
) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_log(path.as_deref(), limit)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_commit(
    state: State<'_, DaemonState>,
    message: String,
    paths: Option<Vec<String>>,
) -> Result<Value, String> {
    let paths = paths.unwrap_or_default();
    client(&state)
        .vault()
        .git_commit(&message, &paths)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_restore(
    state: State<'_, DaemonState>,
    commit: String,
    path: String,
) -> Result<(), String> {
    client(&state)
        .vault()
        .git_restore(&commit, &path)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_diff(
    state: State<'_, DaemonState>,
    path: String,
    commit: Option<String>,
) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_diff(&path, commit.as_deref())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn vault_git_worktrees(state: State<'_, DaemonState>) -> Result<Value, String> {
    client(&state)
        .vault()
        .git_worktrees()
        .await
        .map_err(sdk_error)
}
