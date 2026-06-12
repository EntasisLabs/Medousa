use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::Path,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};

use crate::local_inference::{
    build_hardware_profile, builtin_catalog, config_from_catalog_entry, filter_catalog_for_tier,
    load_recommended_engine, probe_hardware, read_hardware_profile, write_hardware_profile,
    CatalogModelEntry, HardwareProfile, HardwareTier, InstalledModelRecord, LocalEngineStatus,
    ModelDownloadProgress, MODEL_STORE, LOCAL_ENGINE,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelsResponse {
    pub installed: Vec<InstalledModelRecord>,
    pub active_downloads: Vec<ModelDownloadProgress>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineLoadRequest {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub bind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelDownloadRequest {
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelDownloadResponse {
    pub job: ModelDownloadProgress,
}

pub fn routes() -> Router {
    Router::new()
        .route("/v1/local/hardware", get(local_hardware))
        .route("/v1/local/catalog", get(local_catalog))
        .route("/v1/local/models", get(local_models))
        .route("/v1/local/models/download", post(local_model_download))
        .route("/v1/local/models/download/{job_id}", get(local_model_download_status))
        .route(
            "/v1/local/models/download/{job_id}/events",
            get(local_model_download_events),
        )
        .route("/v1/local/models/{model_id}", delete(local_model_delete))
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

async fn local_models() -> Json<LocalModelsResponse> {
    let installed = MODEL_STORE.list_installed().await;
    let active_downloads = MODEL_STORE.list_active_downloads().await;
    Json(LocalModelsResponse {
        installed,
        active_downloads,
    })
}

async fn local_model_download(
    Json(request): Json<LocalModelDownloadRequest>,
) -> Result<Json<LocalModelDownloadResponse>, (axum::http::StatusCode, String)> {
    let model_id = request.model_id.trim();
    if model_id.is_empty() {
        return Err(internal_error("modelId is required".to_string()));
    }
    let catalog = builtin_catalog();
    let entry = catalog
        .models
        .iter()
        .find(|entry| entry.id == model_id)
        .cloned()
        .ok_or_else(|| internal_error(format!("unknown catalog model id: {model_id}")))?;
    let job = MODEL_STORE
        .start_download(entry)
        .await
        .map_err(internal_error)?;
    Ok(Json(LocalModelDownloadResponse { job }))
}

async fn local_model_download_status(
    Path(job_id): Path<String>,
) -> Result<Json<ModelDownloadProgress>, (axum::http::StatusCode, String)> {
    MODEL_STORE
        .get_job_progress(&job_id)
        .await
        .map(Json)
        .ok_or_else(|| internal_error(format!("unknown download job: {job_id}")))
}

async fn local_model_download_events(
    Path(job_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (axum::http::StatusCode, String)> {
    let mut rx = MODEL_STORE
        .subscribe_job_async(&job_id)
        .await
        .ok_or_else(|| internal_error(format!("unknown download job: {job_id}")))?;

    let stream = stream::unfold((rx, false), |(mut rx, finished)| async move {
        if finished {
            return None;
        }
        loop {
            match rx.recv().await {
                Ok(progress) => {
                    let payload = match serde_json::to_string(&progress) {
                        Ok(value) => value,
                        Err(err) => {
                            let event = Event::default()
                                .event("error")
                                .data(format!("progress serialization error: {err}"));
                            return Some((Ok(event), (rx, true)));
                        }
                    };
                    let event = Event::default().event("progress").data(payload);
                    let done = progress.phase == "ready" || progress.phase == "failed";
                    return Some((Ok(event), (rx, done)));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"),
    ))
}

async fn local_model_delete(
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    MODEL_STORE
        .remove_model(&model_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "modelId": model_id, "removed": true })))
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
        if !MODEL_STORE.is_installed(model_id).await {
            return Err(internal_error(format!(
                "model {model_id} is not installed — download it first"
            )));
        }
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

    #[tokio::test]
    async fn local_models_route_returns_lists() {
        let app = routes();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/local/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
