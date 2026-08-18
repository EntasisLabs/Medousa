use std::convert::Infallible;
use std::time::Duration;

use axum::{
    Json,
    extract::Path,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post},
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::local_inference::{
    CatalogModelEntry, HardwareProfile, HardwareTier, InstalledModelRecord, LOCAL_ENGINE,
    LocalEngineStatus, MODEL_STORE, ModelDownloadProgress, build_hardware_profile, builtin_catalog,
    compiled_backends, filter_catalog_for_tier, probe_hardware, read_hardware_profile,
    resolve_inference_device, write_hardware_profile,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHardwareResponse {
    pub profile: HardwareProfile,
    pub engine_available: bool,
    pub compiled_backends: Vec<String>,
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

pub fn surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .route(
            local_runtime_policy(
                axum::http::Method::GET,
                "/v1/local/hardware",
                1024,
                RateLimitClass::Administration,
            ),
            get(local_hardware),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::GET,
                "/v1/local/catalog",
                1024,
                RateLimitClass::Administration,
            ),
            get(local_catalog),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::GET,
                "/v1/local/models",
                1024,
                RateLimitClass::Administration,
            ),
            get(local_models),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::POST,
                "/v1/local/models/download",
                64 * 1024,
                RateLimitClass::Administration,
            ),
            post(local_model_download),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::GET,
                "/v1/local/models/download/{job_id}",
                1024,
                RateLimitClass::Administration,
            ),
            get(local_model_download_status),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::GET,
                "/v1/local/models/download/{job_id}/events",
                1024,
                RateLimitClass::Stream,
            ),
            get(local_model_download_events),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::DELETE,
                "/v1/local/models/{model_id}",
                1024,
                RateLimitClass::Administration,
            ),
            delete(local_model_delete),
        )
        .route(
            local_runtime_policy(
                axum::http::Method::GET,
                "/v1/local/engine/status",
                1024,
                RateLimitClass::Administration,
            ),
            get(local_engine_status),
        )
}

fn local_runtime_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(crate::request_principal::Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

async fn local_hardware() -> Result<Json<LocalHardwareResponse>, (axum::http::StatusCode, String)> {
    let probe = probe_hardware();
    let profile = build_hardware_profile(probe);
    write_hardware_profile(&profile).map_err(internal_error)?;
    let engine_status = LOCAL_ENGINE.as_ref().status().await;
    let device = resolve_inference_device(&profile.probe);
    Ok(Json(LocalHardwareResponse {
        message: format!(
            "Hardware tier {} — {} (recommended: {}, inference: {})",
            profile.tier.as_str(),
            profile.tier_label,
            profile.recommended_display_name,
            device.label()
        ),
        engine_available: engine_status.feature_enabled,
        compiled_backends: compiled_backends()
            .into_iter()
            .map(str::to_string)
            .collect(),
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
    let rx = MODEL_STORE
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
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

async fn local_model_delete(
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    MODEL_STORE
        .remove_model(&model_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(
        serde_json::json!({ "modelId": model_id, "removed": true }),
    ))
}

async fn local_engine_status() -> Json<LocalEngineStatus> {
    Json(LOCAL_ENGINE.as_ref().status().await)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Extension;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_router() -> axum::Router {
        surface().into_router().layer(Extension(
            crate::request_principal::RequestPrincipal::local_app(
                std::sync::Arc::from("test-local"),
                crate::request_principal::TransportClass::Loopback,
            ),
        ))
    }

    #[tokio::test]
    async fn local_catalog_route_returns_gemma_models() {
        let app = test_router();
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
        let app = test_router();
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
        let app = test_router();
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
