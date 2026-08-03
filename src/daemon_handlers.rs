use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use std::sync::Arc;
use uuid::Uuid;

use stasis::ports::outbound::memory::memory_operations::MemoryOperations;

use crate::daemon_api::{
    AgentModeListResponse, AgentModeScope, CreateSessionRequest, CreateSessionResponse,
    SessionAgentModeResponse,
    SetSessionAgentModeRequest, SessionAppendTurnRequest,
    SessionAppendTurnResponse, SessionDeleteQuery, SessionDeleteResponse, SessionHistoryListRequest,
    SessionHistoryListResponse, SessionHistoryResponse, SessionSetDisplayNameRequest,
    SessionSetDisplayNameResponse,
};
use crate::shared_session_catalog::SessionCatalogKind;
use crate::turn_ticket::TurnTicketRegistry;

#[derive(Clone)]
pub struct SessionDeleteState {
    pub memory_operations: Option<Arc<dyn MemoryOperations>>,
    pub turn_tickets: TurnTicketRegistry,
}

/// Session history HTTP handlers extracted to library so they can be tested.
pub async fn list_session_history(
    headers: HeaderMap,
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
    let profile_id = crate::pairing::resolve_request_profile_id(&headers);
    let mut page = crate::session::list_history_sessions_page_for_profile(
        profile_id.as_deref(),
        limit,
        query,
        cursor,
    );
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

pub async fn create_session(
    headers: HeaderMap,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, (StatusCode, String)> {
    let catalog = SessionCatalogKind::parse(request.catalog.as_deref());
    let session_id = request
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("session-{}", Uuid::new_v4().simple()));
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    match catalog {
        SessionCatalogKind::Single => {
            crate::session_catalog::ensure_named_session(&session_id, display_name.clone());
            Ok(Json(CreateSessionResponse {
                session_id,
                catalog: catalog.as_str().to_string(),
                display_name,
                member_profile_ids: Vec::new(),
                agent_profile_id: None,
            }))
        }
        SessionCatalogKind::Shared => {
            if !crate::shared_mode::is_shared_mode() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "shared sessions require Shared mode".to_string(),
                ));
            }
            let mut members = request.member_profile_ids.unwrap_or_default();
            if members.is_empty() {
                if let Some(bound) = crate::pairing::resolve_request_profile_id(&headers) {
                    members.push(bound);
                } else {
                    members.push(crate::user_profiles::resolve_workshop_identity_user_id());
                }
            }
            let row = crate::shared_session_catalog::create_shared_session(
                &session_id,
                members,
                request.agent_profile_id.clone(),
                display_name.clone(),
            )
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
            if let Some(name) = display_name.as_deref() {
                let _ = crate::session_meta_store::set_session_display_name(&session_id, name);
            }
            Ok(Json(CreateSessionResponse {
                session_id: row.session_id,
                catalog: catalog.as_str().to_string(),
                display_name: row.display_name,
                member_profile_ids: row.member_profile_ids,
                agent_profile_id: row.agent_profile_id,
            }))
        }
    }
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

pub async fn get_session_agent_mode(
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<SessionAgentModeResponse>, (StatusCode, String)> {
    crate::agent_mode_state::get_session_mode(&session_id)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn list_agent_modes() -> Json<AgentModeListResponse> {
    Json(crate::agent_runtime::list_agent_modes())
}

pub async fn set_session_agent_mode(
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SetSessionAgentModeRequest>,
) -> Result<Json<SessionAgentModeResponse>, (StatusCode, String)> {
    crate::agent_mode_state::set_session_mode(&session_id, request)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

#[derive(Debug, serde::Deserialize)]
pub struct ClearSessionAgentModeQuery {
    #[serde(default)]
    scope: AgentModeScope,
}

pub async fn clear_session_agent_mode(
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<ClearSessionAgentModeQuery>,
) -> Result<Json<SessionAgentModeResponse>, (StatusCode, String)> {
    crate::agent_mode_state::clear_session_mode(&session_id, query.scope)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
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
