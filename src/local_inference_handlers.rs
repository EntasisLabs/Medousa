use axum::{routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};

use crate::local_inference::{
    build_hardware_profile, builtin_catalog, config_from_catalog_entry, filter_catalog_for_tier,
    load_recommended_engine, probe_hardware, read_hardware_profile, write_hardware_profile,
    CatalogModelEntry, HardwareProfile, HardwareTier, LOCAL_ENGINE, LocalEngineStatus,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHardwareResponse {
    pub profile: HardwareProfile,
    pub engine_available: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCatalogResponse {
    pub tier: HardwareTier,
    pub tier_label: String,
    pub family_default: String,
    pub recommended_model_id: String,
    pub models: Vec<CatalogModelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineLoadRequest {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub bind: Option<String>,
}

pub fn routes() -> Router {
    Router::new()
        .route("/v1/local/hardware", get(local_hardware))
        .route("/v1/local/catalog", get(local_catalog))
        .route("/v1/local/engine/status", get(local_engine_status))
        .route("/v1/local/engine/load", post(local_engine_load))
}

async fn local_hardware() -> Result<Json<LocalHardwareResponse>, (axum::http::StatusCode, String)> {
    let probe = probe_hardware();
    let profile = build_hardware_profile(probe);
    write_hardware_profile(&profile).map_err(internal_error)?;
    let engine_status = LOCAL_ENGINE.as_ref().status().await;
    Ok(Json(LocalHardwareResponse {
        message: format!(
            "Hardware tier {} — {} (recommended: {})",
            profile.tier.as_str(),
            profile.tier_label,
            profile.recommended_display_name
        ),
        engine_available: engine_status.feature_enabled,
        profile,
    }))
}

async fn local_catalog() -> Result<Json<LocalCatalogResponse>, (axum::http::StatusCode, String)> {
    let profile =
        read_hardware_profile().unwrap_or_else(|| build_hardware_profile(probe_hardware()));
    let catalog = builtin_catalog();
    let models = filter_catalog_for_tier(&catalog, profile.tier);
    Ok(Json(LocalCatalogResponse {
        tier: profile.tier,
        tier_label: profile.tier_label.clone(),
        family_default: catalog.family_default.clone(),
        recommended_model_id: profile.recommended_model_id.clone(),
        models,
    }))
}

async fn local_engine_status() -> Json<LocalEngineStatus> {
    Json(LOCAL_ENGINE.as_ref().status().await)
}

async fn local_engine_load(
    Json(request): Json<LocalEngineLoadRequest>,
) -> Result<Json<LocalEngineStatus>, (axum::http::StatusCode, String)> {
    let status = if let Some(model_id) = request
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let catalog = builtin_catalog();
        let entry = catalog
            .models
            .iter()
            .find(|entry| entry.id == model_id)
            .cloned()
            .ok_or_else(|| internal_error(format!("unknown catalog model id: {model_id}")))?;
        let config = config_from_catalog_entry(&entry, request.bind.clone());
        LOCAL_ENGINE.as_ref().load(config).await
    } else {
        load_recommended_engine(request.bind).await
    }
    .map_err(internal_error)?;
    Ok(Json(status))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn local_catalog_route_returns_gemma_models() {
        let app = routes();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/local/catalog")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: LocalCatalogResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.family_default, "gemma-4");
        assert!(!parsed.models.is_empty());
    }

    #[tokio::test]
    async fn local_engine_status_route_returns_idle_by_default() {
        let app = routes();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/local/engine/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: LocalEngineStatus = serde_json::from_slice(&body).unwrap();
        assert!(!parsed.loaded);
    }
}
