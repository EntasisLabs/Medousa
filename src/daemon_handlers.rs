use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;

use stasis::ports::outbound::memory::memory_operations::MemoryOperations;

use crate::daemon_api::{
    SessionAppendTurnRequest, SessionAppendTurnResponse, SessionDeleteQuery, SessionDeleteResponse,
    SessionHistoryListRequest, SessionHistoryListResponse, SessionHistoryResponse,
    SessionSetDisplayNameRequest, SessionSetDisplayNameResponse,
};
use crate::turn_ticket::TurnTicketRegistry;

#[derive(Clone)]
pub struct SessionDeleteState {
    pub memory_operations: Option<Arc<dyn MemoryOperations>>,
    pub turn_tickets: TurnTicketRegistry,
}

/// Session history HTTP handlers extracted to library so they can be tested.
pub async fn list_session_history(
    Query(request): Query<SessionHistoryListRequest>,
) -> Result<Json<SessionHistoryListResponse>, (StatusCode, String)> {
    let limit = request.limit.unwrap_or(200).clamp(1, 1000);
    let query = request
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let cursor = request
        .cursor
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut page = crate::session::list_history_sessions_page(limit, query, cursor);
    if request.include_verification == Some(false) {
        page.sessions = page
            .sessions
            .into_iter()
            .map(|session| session.without_verification_fields())
            .collect();
    }
    Ok(Json(SessionHistoryListResponse {
        sessions: page.sessions,
        next_cursor: page.next_cursor,
    }))
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

pub async fn set_session_display_name(
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SessionSetDisplayNameRequest>,
) -> Result<Json<SessionSetDisplayNameResponse>, (StatusCode, String)> {
    let session_id = session_id.trim().to_string();
    if session_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".to_string()));
    }

    crate::session::set_session_display_name(&session_id, &request.display_name)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;

    let display_name = crate::session::get_session_display_name(&session_id)
        .unwrap_or_else(|| request.display_name.trim().to_string());

    Ok(Json(SessionSetDisplayNameResponse {
        session_id,
        display_name,
    }))
}

pub async fn delete_session(
    State(state): State<SessionDeleteState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<SessionDeleteQuery>,
) -> Result<Json<SessionDeleteResponse>, (StatusCode, String)> {
    let summary = crate::session_lifecycle::delete_session(
        &session_id,
        state.memory_operations,
        &state.turn_tickets,
        query.purge_memory,
    )
    .await
    .map_err(|err| (StatusCode::BAD_REQUEST, err))?;

    Ok(Json(SessionDeleteResponse {
        session_id: summary.session_id,
        deleted: summary.deleted,
        locus_purged: summary.locus_purged,
        locus_nodes_deleted: summary.locus_nodes_deleted,
        cancelled_active_turn: summary.cancelled_active_turn,
    }))
}
