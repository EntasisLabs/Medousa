use medousa_types::component_store::{
    ComponentStoreDeleteResponse, ComponentStoreGetResponse, ComponentStoreListResponse,
    ComponentStoreSetResponse,
};
use serde_json::Value;
use tauri::State;

use super::workshop_http::{self, path_with_query};
use super::DaemonState;

#[tauri::command]
pub async fn component_store_get(
    state: State<'_, DaemonState>,
    component_id: String,
    key: Option<String>,
    profile_id: Option<String>,
) -> Result<ComponentStoreGetResponse, String> {
    let component_id = component_id.trim();
    if component_id.is_empty() {
        return Err("component_id is required".to_string());
    }

    let mut query = Vec::new();
    if let Some(profile_id) = profile_id.filter(|id| !id.trim().is_empty()) {
        query.push(("profile_id", profile_id));
    }
    if let Some(key) = key.filter(|value| !value.trim().is_empty()) {
        query.push(("key", key));
    }

    let path = if query.is_empty() {
        format!("/v1/components/{component_id}/store")
    } else {
        path_with_query(
            &format!("/v1/components/{component_id}/store"),
            &query.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        )
    };

    workshop_http::get_json(&state, &path).await
}

#[tauri::command]
pub async fn component_store_set(
    state: State<'_, DaemonState>,
    component_id: String,
    key: String,
    value: Value,
    profile_id: Option<String>,
) -> Result<ComponentStoreSetResponse, String> {
    let component_id = component_id.trim();
    let key = key.trim();
    if component_id.is_empty() {
        return Err("component_id is required".to_string());
    }
    if key.is_empty() {
        return Err("key is required".to_string());
    }

    let path = format!("/v1/components/{component_id}/store/{key}");
    let body = serde_json::json!({
        "value": value,
        "profileId": profile_id,
    });
    workshop_http::put_json(&state, &path, &body).await
}

#[tauri::command]
pub async fn component_store_delete(
    state: State<'_, DaemonState>,
    component_id: String,
    key: String,
    profile_id: Option<String>,
) -> Result<ComponentStoreDeleteResponse, String> {
    let component_id = component_id.trim();
    let key = key.trim();
    if component_id.is_empty() || key.is_empty() {
        return Err("component_id and key are required".to_string());
    }

    let mut query = Vec::new();
    if let Some(profile_id) = profile_id.filter(|id| !id.trim().is_empty()) {
        query.push(("profile_id", profile_id));
    }
    let path = if query.is_empty() {
        format!("/v1/components/{component_id}/store/{key}")
    } else {
        path_with_query(
            &format!("/v1/components/{component_id}/store/{key}"),
            &query.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        )
    };
    workshop_http::delete_json(&state, &path).await
}

#[tauri::command]
pub async fn component_store_list_keys(
    state: State<'_, DaemonState>,
    component_id: String,
    profile_id: Option<String>,
) -> Result<ComponentStoreListResponse, String> {
    let component_id = component_id.trim();
    if component_id.is_empty() {
        return Err("component_id is required".to_string());
    }

    let mut query = Vec::new();
    if let Some(profile_id) = profile_id.filter(|id| !id.trim().is_empty()) {
        query.push(("profile_id", profile_id));
    }
    let path = if query.is_empty() {
        format!("/v1/components/{component_id}/store/keys")
    } else {
        path_with_query(
            &format!("/v1/components/{component_id}/store/keys"),
            &query.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        )
    };
    workshop_http::get_json(&state, &path).await
}
