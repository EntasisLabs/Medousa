//! HTTP handlers for artifact retention settings (`/v1/maintenance/artifacts`).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::daemon::state::AppState;
use crate::daemon_api::{
    ArtifactRetentionStatusResponse, UpdateArtifactRetentionRequest, UpdateArtifactRetentionResponse,
};

pub async fn get_artifact_retention_status(
    State(state): State<AppState>,
) -> Result<Json<ArtifactRetentionStatusResponse>, (StatusCode, String)> {
    crate::artifact_retention::get_status(state.composition())
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

pub async fn update_artifact_retention(
    State(state): State<AppState>,
    Json(request): Json<UpdateArtifactRetentionRequest>,
) -> Result<Json<UpdateArtifactRetentionResponse>, (StatusCode, String)> {
    crate::artifact_retention::update_settings(state.composition(), request)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}
