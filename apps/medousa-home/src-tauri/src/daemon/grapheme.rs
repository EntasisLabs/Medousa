use crate::daemon::types::{
    GraphemeModuleDetailResponse, GraphemeModuleOpsResponse, GraphemeModulesListResponse,
    GraphemeRunRequest, GraphemeRunResponse, GraphemeScriptDetailResponse,
    GraphemeScriptsListResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn grapheme_list_modules(
    state: State<'_, DaemonState>,
) -> Result<GraphemeModulesListResponse, String> {
    workshop_http::get_json(&state, "/v1/grapheme/modules").await
}

#[tauri::command]
pub async fn grapheme_get_module(
    state: State<'_, DaemonState>,
    module_id: String,
) -> Result<GraphemeModuleDetailResponse, String> {
    let id = urlencoding::encode(module_id.trim());
    workshop_http::get_json(&state, &format!("/v1/grapheme/modules/{id}")).await
}

#[tauri::command]
pub async fn grapheme_get_module_ops(
    state: State<'_, DaemonState>,
    module_id: String,
    q: Option<String>,
) -> Result<GraphemeModuleOpsResponse, String> {
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
    query: Option<String>,
    module: Option<String>,
    tag: Option<String>,
    limit: Option<usize>,
) -> Result<GraphemeScriptsListResponse, String> {
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
    script_id: String,
) -> Result<GraphemeScriptDetailResponse, String> {
    let id = urlencoding::encode(script_id.trim());
    workshop_http::get_json(&state, &format!("/v1/grapheme/scripts/{id}")).await
}

#[tauri::command]
pub async fn grapheme_run_source(
    state: State<'_, DaemonState>,
    source: String,
) -> Result<GraphemeRunResponse, String> {
    workshop_http::post_json(
        &state,
        "/v1/grapheme/run",
        &GraphemeRunRequest { source },
    )
    .await
}
