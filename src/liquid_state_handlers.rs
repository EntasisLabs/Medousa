use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use medousa_types::{LiquidComponentStateResponse, PutLiquidComponentStateRequest};

fn profile_id(principal: &crate::request_principal::RequestPrincipal) -> String {
    principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id)
}

fn ensure_visible_session(session_id: &str, profile_id: &str) -> Result<(), (StatusCode, String)> {
    crate::session_storage::SessionId::parse(session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if !crate::session_catalog::session_visible_to_profile(session_id, profile_id) {
        return Err((StatusCode::NOT_FOUND, "conversation not found".to_string()));
    }
    Ok(())
}

fn map_error(error: crate::liquid_state::LiquidStateError) -> (StatusCode, String) {
    let status = match error {
        crate::liquid_state::LiquidStateError::Invalid(_) => StatusCode::BAD_REQUEST,
        crate::liquid_state::LiquidStateError::Conflict { .. } => StatusCode::CONFLICT,
        crate::liquid_state::LiquidStateError::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

pub async fn get_state(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path((session_id, message_id, node_id, instance_id)): Path<(String, String, String, String)>,
) -> Result<Json<LiquidComponentStateResponse>, (StatusCode, String)> {
    let profile_id = profile_id(&principal);
    tokio::task::spawn_blocking(move || {
        ensure_visible_session(&session_id, &profile_id)?;
        crate::liquid_state::get(&session_id, &message_id, &node_id, &instance_id)
            .map(Json)
            .map_err(map_error)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
}

pub async fn put_state(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path((session_id, message_id, node_id, instance_id)): Path<(String, String, String, String)>,
    Json(request): Json<PutLiquidComponentStateRequest>,
) -> Result<Json<LiquidComponentStateResponse>, (StatusCode, String)> {
    let profile_id = profile_id(&principal);
    tokio::task::spawn_blocking(move || {
        ensure_visible_session(&session_id, &profile_id)?;
        crate::liquid_state::put(&session_id, &message_id, &node_id, &instance_id, request)
            .map(Json)
            .map_err(map_error)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
}
