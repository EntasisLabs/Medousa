use axum::extract::{Path as AxumPath, Query};
use axum::http::StatusCode;
use axum::Json;

use crate::daemon_api::{
    SessionAppendTurnRequest, SessionAppendTurnResponse, SessionHistoryListRequest,
    SessionHistoryListResponse, SessionHistoryResponse,
};

/// Session history HTTP handlers extracted to library so they can be tested.
pub async fn list_session_history(
    Query(request): Query<SessionHistoryListRequest>,
) -> Result<Json<SessionHistoryListResponse>, (StatusCode, String)> {
    let limit = request.limit.unwrap_or(200).clamp(1, 1000);
    let sessions = crate::session::list_history_sessions(limit);
    Ok(Json(SessionHistoryListResponse { sessions }))
}

pub async fn get_session_history(
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<SessionHistoryResponse>, (StatusCode, String)> {
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    let turns = crate::session::load_history(&session_id);
    Ok(Json(SessionHistoryResponse { session_id, turns }))
}

pub async fn append_session_turn(
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SessionAppendTurnRequest>,
) -> Result<Json<SessionAppendTurnResponse>, (StatusCode, String)> {
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    crate::session::append_turn(&session_id, &request.turn);
    Ok(Json(SessionAppendTurnResponse {
        session_id,
        stored: true,
    }))
}
