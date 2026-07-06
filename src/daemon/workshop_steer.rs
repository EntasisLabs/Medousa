use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::agent_runtime::turn_worker_tools::steer_bound_workshop_for_session;
use crate::daemon::state::AppState;

#[derive(Debug, Deserialize)]
pub struct WorkshopSteerRequest {
    pub message: String,
}

pub async fn steer_bound_workshop_handler(
    State(_state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<WorkshopSteerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match steer_bound_workshop_for_session(session_id.trim(), body.message.trim()) {
        Ok(value) => {
            let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            let status = if ok {
                StatusCode::OK
            } else {
                StatusCode::CONFLICT
            };
            (status, Json(value))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "error": err.to_string() })),
        ),
    }
}
