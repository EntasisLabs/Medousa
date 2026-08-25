use crate::daemon::types::{
    GraphemeAllowlistResponse, GraphemeAllowlistUpdateRequest, GraphemeCompileRequest,
    GraphemeCompileResponse, GraphemeLifecycleResponse, GraphemeLspWorkspaceResponse,
    GraphemeModuleDetailResponse, GraphemeModuleLoadRequest, GraphemeModuleLoadResponse,
    GraphemeModuleOpsResponse, GraphemeModulesListResponse, GraphemeRunRequest,
    GraphemeRunResponse, GraphemeScriptDeleteResponse, GraphemeScriptDetailResponse,
    GraphemeScriptRenameRequest, GraphemeScriptSaveRequest, GraphemeScriptSaveResponse,
    GraphemeScriptsListResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn grapheme_list_modules(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<GraphemeModulesListResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_list_modules()
            .map_err(|error| format!("embedded Grapheme modules: {error:#}"));
    }
    workshop_http::get_json(&state, "/v1/grapheme/modules").await
}

#[tauri::command]
pub async fn grapheme_get_module(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    module_id: String,
) -> Result<GraphemeModuleDetailResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_get_module(&module_id)
            .map_err(|error| format!("embedded Grapheme module: {error:#}"));
    }
    let id = urlencoding::encode(module_id.trim());
    workshop_http::get_json(&state, &format!("/v1/grapheme/modules/{id}")).await
}

#[tauri::command]
pub async fn grapheme_get_module_ops(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    module_id: String,
    q: Option<String>,
) -> Result<GraphemeModuleOpsResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_get_module_ops(&module_id, q.as_deref())
            .map_err(|error| format!("embedded Grapheme module operations: {error:#}"));
    }
    let id = urlencoding::encode(module_id.trim());
    let path = if let Some(query) = q.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        format!(
            "/v1/grapheme/modules/{id}/ops?q={}",
            urlencoding::encode(query)
        )
    } else {
        format!("/v1/grapheme/modules/{id}/ops")
    };
    workshop_http::get_json(&state, &path).await
}

#[tauri::command]
pub async fn grapheme_list_scripts(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    query: Option<String>,
    module: Option<String>,
    tag: Option<String>,
    limit: Option<usize>,
) -> Result<GraphemeScriptsListResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_list_scripts(crate::daemon::types::GraphemeScriptsListQuery {
                query,
                module,
                tag,
                limit,
            })
            .map_err(|error| format!("embedded Grapheme scripts: {error:#}"));
    }
    let mut params = Vec::new();
    if let Some(value) = query.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("query", value.to_string()));
    }
    if let Some(value) = module.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("module", value.to_string()));
    }
    if let Some(value) = tag.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("tag", value.to_string()));
    }
    if let Some(value) = limit {
        params.push(("limit", value.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/grapheme/scripts", &params).await
}

#[tauri::command]
pub async fn grapheme_get_script(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    script_id: String,
) -> Result<GraphemeScriptDetailResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_get_script(&script_id)
            .map_err(|error| format!("embedded Grapheme script: {error:#}"));
    }
    let id = urlencoding::encode(script_id.trim());
    workshop_http::get_json(&state, &format!("/v1/grapheme/scripts/{id}")).await
}

#[tauri::command]
pub async fn grapheme_run_source(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    source: String,
) -> Result<GraphemeRunResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_run_source(&source)
            .await
            .map_err(|error| format!("embedded Grapheme run: {error:#}"));
    }
    workshop_http::post_json(&state, "/v1/grapheme/run", &GraphemeRunRequest { source }).await
}

#[tauri::command]
pub async fn grapheme_get_allowlist(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<GraphemeAllowlistResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_get_allowlist()
            .await
            .map_err(|error| format!("embedded Grapheme allowlist: {error:#}"));
    }
    workshop_http::get_json(&state, "/v1/grapheme/allowlist").await
}

#[tauri::command]
pub async fn grapheme_update_allowlist(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    allowed_modules: Vec<String>,
) -> Result<GraphemeAllowlistResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_update_allowlist(GraphemeAllowlistUpdateRequest { allowed_modules })
            .await
            .map_err(|error| format!("embedded Grapheme allowlist update: {error:#}"));
    }
    workshop_http::put_json(
        &state,
        "/v1/grapheme/allowlist",
        &GraphemeAllowlistUpdateRequest { allowed_modules },
    )
    .await
}

#[tauri::command]
pub async fn grapheme_save_script(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: GraphemeScriptSaveRequest,
) -> Result<GraphemeScriptSaveResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_save_script(request)
            .map_err(|error| format!("embedded Grapheme script save: {error:#}"));
    }
    workshop_http::post_json(&state, "/v1/grapheme/scripts", &request).await
}

#[tauri::command]
pub async fn grapheme_delete_script(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    script_id: String,
) -> Result<GraphemeScriptDeleteResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_delete_script(&script_id)
            .map_err(|error| format!("embedded Grapheme script delete: {error:#}"));
    }
    let id = urlencoding::encode(script_id.trim());
    workshop_http::delete_json(&state, &format!("/v1/grapheme/scripts/{id}")).await
}

#[tauri::command]
pub async fn grapheme_rename_script(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    script_id: String,
    name: String,
) -> Result<GraphemeScriptSaveResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_rename_script(&script_id, &name)
            .map_err(|error| format!("embedded Grapheme script rename: {error:#}"));
    }
    let id = urlencoding::encode(script_id.trim());
    workshop_http::post_json(
        &state,
        &format!("/v1/grapheme/scripts/{id}/rename"),
        &GraphemeScriptRenameRequest { name },
    )
    .await
}

#[tauri::command]
pub async fn grapheme_compile_source(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    source: String,
    mode: Option<String>,
) -> Result<GraphemeCompileResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_compile_source(GraphemeCompileRequest { source, mode })
            .await
            .map_err(|error| format!("embedded Grapheme compile: {error:#}"));
    }
    workshop_http::post_json(
        &state,
        "/v1/grapheme/compile",
        &GraphemeCompileRequest { source, mode },
    )
    .await
}

#[tauri::command]
pub async fn grapheme_load_module(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: GraphemeModuleLoadRequest,
) -> Result<GraphemeModuleLoadResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_load_module(request)
            .await
            .map_err(|error| format!("embedded Grapheme module load: {error:#}"));
    }
    workshop_http::post_json(&state, "/v1/grapheme/modules/load", &request).await
}

#[tauri::command]
pub async fn grapheme_get_lifecycle(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<GraphemeLifecycleResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .grapheme_lifecycle()
            .await
            .map_err(|error| format!("embedded Grapheme lifecycle: {error:#}"));
    }
    workshop_http::get_json(&state, "/v1/grapheme/lifecycle").await
}

#[tauri::command]
pub async fn grapheme_get_lsp_workspace(
    state: State<'_, DaemonState>,
) -> Result<GraphemeLspWorkspaceResponse, String> {
    workshop_http::get_json(&state, "/v1/grapheme/lsp/workspace").await
}

#[tauri::command]
pub async fn coding_engine_info(
    state: State<'_, DaemonState>,
) -> Result<serde_json::Value, String> {
    workshop_http::get_json(&state, "/v1/coding-engine").await
}
