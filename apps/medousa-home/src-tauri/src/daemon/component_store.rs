use medousa_types::component_store::{
    ComponentStoreDeleteResponse, ComponentStoreGetResponse, ComponentStoreListResponse,
    ComponentStoreSetRequest, ComponentStoreSetResponse,
};
use serde_json::Value;
use tauri::State;

use super::sdk::{client, sdk_error};
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

    client(&state)
        .components()
        .store_get(
            component_id,
            profile_id.as_deref().filter(|id| !id.trim().is_empty()),
            key.as_deref().filter(|value| !value.trim().is_empty()),
        )
        .await
        .map_err(sdk_error)
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

    let request = ComponentStoreSetRequest {
        value,
        profile_id: profile_id.filter(|id| !id.trim().is_empty()),
    };
    client(&state)
        .components()
        .store_put_key(component_id, key, &request)
        .await
        .map_err(sdk_error)
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

    client(&state)
        .components()
        .store_delete_key(
            component_id,
            key,
            profile_id.as_deref().filter(|id| !id.trim().is_empty()),
        )
        .await
        .map_err(sdk_error)
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

    client(&state)
        .components()
        .store_list_keys(
            component_id,
            profile_id.as_deref().filter(|id| !id.trim().is_empty()),
        )
        .await
        .map_err(sdk_error)
}
