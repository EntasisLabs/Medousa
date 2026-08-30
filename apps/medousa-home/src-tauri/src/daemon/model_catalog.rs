use serde::{Deserialize, Serialize};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::workshop_http;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogListQuery {
    pub provider: Option<String>,
    pub capability: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogRefreshRequest {
    pub providers: Option<Vec<String>>,
}

#[tauri::command]
pub async fn model_catalog_list(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    provider: Option<String>,
    capability: Option<String>,
    q: Option<String>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .model_catalog_list(provider, capability, q)
            .map_err(|error| error.to_string());
    }
    let mut query = Vec::new();
    if let Some(value) = provider.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        query.push(("provider", value.to_string()));
    }
    if let Some(value) = capability
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        query.push(("capability", value.to_string()));
    }
    if let Some(value) = q.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        query.push(("q", value.to_string()));
    }
    workshop_http::get_json_query(&state, "/v1/models/catalog", &query).await
}

#[tauri::command]
pub async fn model_catalog_lookup(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    provider: String,
    model: String,
) -> Result<serde_json::Value, String> {
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        return Err("provider and model are required".to_string());
    }
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .model_capabilities(provider, model)
            .map_err(|error| error.to_string());
    }
    workshop_http::get_json_query(
        &state,
        "/v1/models/capabilities",
        &[
            ("provider", provider.to_string()),
            ("model", model.to_string()),
        ],
    )
    .await
}

#[tauri::command]
pub async fn model_catalog_refresh(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    providers: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .refresh_model_catalog(providers)
            .await
            .map_err(|error| error.to_string());
    }
    let body = ModelCatalogRefreshRequest { providers };
    workshop_http::post_json(&state, "/v1/models/catalog/refresh", &body).await
}
