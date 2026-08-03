//! Shared mode status / enable HTTP handlers.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::daemon::http::internal_error;
use crate::daemon::state::AppState;
use crate::daemon_api::{SetSharedModeRequest, SharedModeStatusResponse};
use crate::shared_mode::{
    DaemonWorkshopMode, current_mode, disable_shared_mode, enable_shared_mode, enabled_at,
    general_profile_id, root_profile_id,
};

pub async fn shared_mode_status() -> Json<SharedModeStatusResponse> {
    Json(SharedModeStatusResponse {
        mode: current_mode().as_str().to_string(),
        enabled_at: enabled_at(),
        root_profile_id: root_profile_id(),
        general_profile_id: general_profile_id(),
    })
}

pub async fn set_shared_mode(
    State(state): State<AppState>,
    Json(request): Json<SetSharedModeRequest>,
) -> Result<Json<SharedModeStatusResponse>, (StatusCode, String)> {
    match DaemonWorkshopMode::parse(&request.mode) {
        DaemonWorkshopMode::Shared => {
            enable_shared_mode(state.profile_registry.clone()).map_err(internal_error)?;
        }
        DaemonWorkshopMode::Personal => {
            disable_shared_mode().map_err(internal_error)?;
        }
    }
    Ok(Json(SharedModeStatusResponse {
        mode: current_mode().as_str().to_string(),
        enabled_at: enabled_at(),
        root_profile_id: root_profile_id(),
        general_profile_id: general_profile_id(),
    }))
}
