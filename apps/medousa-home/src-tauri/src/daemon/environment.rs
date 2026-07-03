use crate::daemon::types::{
    EnvironmentPendingResponse, EnvironmentSpecPutRequest, EnvironmentSpecResponse,
    EnvironmentStatusResponse, EnvironmentStreamQuery,
};
use tauri::State;

use super::workshop_http::{self, delete_json, path_with_query, post_empty_json};
use super::DaemonState;

#[tauri::command]
pub async fn environment_get_status(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
    surface_id: Option<String>,
    include_runtime: Option<bool>,
) -> Result<EnvironmentStatusResponse, String> {
    let mut query = Vec::new();
    if let Some(profile_id) = profile_id.filter(|id| !id.trim().is_empty()) {
        query.push(("profile_id", profile_id));
    }
    if let Some(surface_id) = surface_id.filter(|id| !id.trim().is_empty()) {
        query.push(("surface_id", surface_id));
    }
    if include_runtime.unwrap_or(false) {
        query.push(("include_runtime", "true".to_string()));
    }
    let path = if query.is_empty() {
        "/v1/environment/status".to_string()
    } else {
        path_with_query(
            "/v1/environment/status",
            &query.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        )
    };
    workshop_http::get_json(&state, &path).await
}

#[tauri::command]
pub async fn environment_get_spec(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentSpecResponse, String> {
    let query = profile_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| vec![("profile_id", id)])
        .unwrap_or_default();
    if query.is_empty() {
        workshop_http::get_json(&state, "/v1/environment/spec").await
    } else {
        workshop_http::get_json_query(&state, "/v1/environment/spec", &query).await
    }
}

#[tauri::command]
pub async fn environment_put_spec(
    state: State<'_, DaemonState>,
    request: EnvironmentSpecPutRequest,
) -> Result<EnvironmentSpecResponse, String> {
    workshop_http::put_json(&state, "/v1/environment/spec", &request).await
}

#[tauri::command]
pub async fn environment_get_pending(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentPendingResponse, String> {
    let query = profile_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| vec![("profile_id", id)])
        .unwrap_or_default();
    if query.is_empty() {
        workshop_http::get_json(&state, "/v1/environment/spec/pending").await
    } else {
        workshop_http::get_json_query(
            &state,
            "/v1/environment/spec/pending",
            &[("profile_id", query[0].1.clone())],
        )
        .await
    }
}

#[tauri::command]
pub async fn environment_apply_pending(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<EnvironmentSpecResponse, String> {
    let path = profile_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| {
            path_with_query(
                "/v1/environment/spec/pending/apply",
                &[("profile_id", id)],
            )
        })
        .unwrap_or_else(|| "/v1/environment/spec/pending/apply".to_string());
    post_empty_json(&state, &path).await
}

#[tauri::command]
pub async fn environment_dismiss_pending(
    state: State<'_, DaemonState>,
    profile_id: Option<String>,
) -> Result<(), String> {
    let path = profile_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| {
            path_with_query("/v1/environment/spec/pending", &[("profile_id", id)])
        })
        .unwrap_or_else(|| "/v1/environment/spec/pending".to_string());
    delete_json::<serde_json::Value>(&state, &path).await.map(|_| ())
}
