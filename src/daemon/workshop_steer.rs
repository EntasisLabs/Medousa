use axum::Json;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;

use crate::agent_runtime::turn_worker_tools::steer_bound_workshop_for_session;
use crate::daemon::state::AppState;
use crate::remote_trust::is_trusted_local;

#[derive(Debug, Deserialize)]
pub struct WorkshopSteerRequest {
    pub message: String,
}

pub async fn steer_bound_workshop_handler(
    State(_state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<WorkshopSteerRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let speaker = resolve_steer_speaker(addr.ip(), &headers);
    match steer_bound_workshop_for_session(session_id.trim(), body.message.trim(), speaker) {
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

fn resolve_steer_speaker(ip: std::net::IpAddr, headers: &HeaderMap) -> Option<String> {
    if let Some(bound) = crate::pairing::resolve_request_profile_id(headers) {
        return Some(bound);
    }
    if is_trusted_local(ip, headers) {
        return Some(crate::user_profiles::resolve_workshop_identity_user_id());
    }
    None
}
