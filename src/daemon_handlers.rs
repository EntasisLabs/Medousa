use axum::Json;
use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use std::sync::Arc;

fn validated_session_id(session_id: String) -> Result<String, (StatusCode, String)> {
    crate::session_storage::validate_session_id(&session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(session_id)
}

use stasis::ports::outbound::memory::memory_operations::MemoryOperations;

use crate::daemon_api::{
    AgentModeListResponse, AgentModeProposalListResponse, AgentModeProposalResponse,
    AgentModeScope, AgentModeTransitionPolicy, CreatePromptStashRequest, CreateSessionRequest,
    CreateSessionResponse, DecideAgentModeProposalRequest, DeletePromptStashResponse,
    DeriveSessionRequest, DeriveSessionResponse, PromptStash, PromptStashListResponse,
    SessionAgentModeResponse, SessionAppendTurnRequest, SessionAppendTurnResponse,
    SessionCodeBindingResponse, SessionDeleteQuery, SessionDeleteResponse,
    SessionHistoryListRequest, SessionHistoryListResponse, SessionHistoryRequest,
    SessionHistoryResponse, SessionSetDisplayNameRequest, SessionSetDisplayNameResponse,
    SessionTranscriptSearchHit, SessionTranscriptSearchRequest, SessionTranscriptSearchResponse,
    SetSessionAgentModeRequest, SetSessionCodeBindingRequest,
};
use crate::shared_session_catalog::SessionCatalogKind;
use crate::turn_ticket::TurnTicketRegistry;
use medousa_types::PromptStashId;

#[derive(Clone)]
pub struct SessionDeleteState {
    pub memory_operations: Option<Arc<dyn MemoryOperations>>,
    pub turn_tickets: TurnTicketRegistry,
    pub turn_streams: Option<crate::daemon::turn_stream_registry::TurnStreamRegistry>,
}

pub async fn search_session_transcripts(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Query(request): Query<SessionTranscriptSearchRequest>,
) -> Result<Json<SessionTranscriptSearchResponse>, (StatusCode, String)> {
    let query = request.q.trim();
    if query.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "q is required".to_string()));
    }
    if query.chars().count() > crate::session_store::MAX_TRANSCRIPT_SEARCH_QUERY_CHARS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "q must be at most {} characters",
                crate::session_store::MAX_TRANSCRIPT_SEARCH_QUERY_CHARS
            ),
        ));
    }
    let limit = request.limit.unwrap_or(20).clamp(1, 100);
    let profile_id = principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id);
    let visible_sessions = crate::session::list_history_sessions_page_for_profile(
        Some(&profile_id),
        10_000,
        None,
        None,
    )
    .sessions
    .into_iter()
    .filter(|session| {
        crate::session_catalog::session_visible_to_profile(&session.session_id, &profile_id)
    })
    .collect::<Vec<_>>();
    let session_ids = visible_sessions
        .iter()
        .map(|session| session.session_id.clone())
        .collect::<Vec<_>>();
    let display_names = visible_sessions
        .into_iter()
        .map(|session| (session.session_id, session.display_name))
        .collect::<std::collections::HashMap<_, _>>();
    let hits = crate::session_store::get_session_store()
        .search_transcripts(&session_ids, query, limit)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .into_iter()
        .map(|hit| SessionTranscriptSearchHit {
            display_name: display_names.get(&hit.session_id).cloned().flatten(),
            session_id: hit.session_id,
            role: hit.role,
            timestamp: hit.timestamp,
            excerpt: hit.excerpt,
        })
        .collect();
    Ok(Json(SessionTranscriptSearchResponse {
        query: query.to_string(),
        hits,
    }))
}

/// Session history HTTP handlers extracted to library so they can be tested.
pub async fn list_session_history(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
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
    let mut page = crate::session::list_history_sessions_page_for_profile(
        principal.profile_id(),
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
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, (StatusCode, String)> {
    let authority_id = crate::workshop_authority::current()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .clone();
    let catalog = SessionCatalogKind::parse(request.catalog.as_deref());
    if request.session_id.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "caller-supplied session_id is no longer supported".to_string(),
        ));
    }
    let session_id = crate::session_storage::new_session_id().to_string();
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
                authority_id,
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
                if let Some(bound) = principal.profile_id() {
                    members.push(bound.to_string());
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
                authority_id,
                session_id: row.session_id,
                catalog: catalog.as_str().to_string(),
                display_name: row.display_name,
                member_profile_ids: row.member_profile_ids,
                agent_profile_id: row.agent_profile_id,
            }))
        }
    }
}

pub async fn derive_session(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    headers: HeaderMap,
    Json(request): Json<DeriveSessionRequest>,
) -> Result<Json<DeriveSessionResponse>, (StatusCode, String)> {
    let idempotency_key = headers
        .get("idempotency-key")
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "Idempotency-Key header is required".to_string(),
            )
        })?
        .to_str()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Idempotency-Key header is invalid".to_string(),
            )
        })?;
    crate::context_derivation::derive_session(&principal, request, idempotency_key)
        .await
        .map(Json)
        .map_err(|error| {
            let status = match error.kind {
                crate::context_derivation::DerivationErrorKind::Invalid => StatusCode::BAD_REQUEST,
                crate::context_derivation::DerivationErrorKind::Forbidden => StatusCode::FORBIDDEN,
                crate::context_derivation::DerivationErrorKind::Conflict => StatusCode::CONFLICT,
                crate::context_derivation::DerivationErrorKind::Internal => {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            };
            (status, error.message)
        })
}

fn principal_profile_id(principal: &crate::request_principal::RequestPrincipal) -> String {
    principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id)
}

pub async fn create_prompt_stash(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(request): Json<CreatePromptStashRequest>,
) -> Result<(StatusCode, Json<PromptStash>), (StatusCode, String)> {
    let profile_id = principal_profile_id(&principal);
    if let Some(source) = request.source_session.as_ref() {
        let authority = crate::workshop_authority::current()
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
        if &source.authority_id != authority
            || !crate::session_catalog::session_visible_to_profile(
                source.session_id.as_str(),
                &profile_id,
            )
        {
            return Err((
                StatusCode::FORBIDDEN,
                "source session is not available".to_string(),
            ));
        }
    }
    let stash = tokio::task::spawn_blocking(move || {
        crate::prompt_stash::PromptStashStore::daemon_default().create(&profile_id, request)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    Ok((StatusCode::CREATED, Json(stash)))
}

pub async fn list_prompt_stashes(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
) -> Result<Json<PromptStashListResponse>, (StatusCode, String)> {
    let profile_id = principal_profile_id(&principal);
    let stashes = tokio::task::spawn_blocking(move || {
        crate::prompt_stash::PromptStashStore::daemon_default().list(&profile_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(PromptStashListResponse { stashes }))
}

pub async fn delete_prompt_stash(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    AxumPath(stash_id): AxumPath<String>,
) -> Result<Json<DeletePromptStashResponse>, (StatusCode, String)> {
    let stash_id = PromptStashId::parse(stash_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let profile_id = principal_profile_id(&principal);
    let response = tokio::task::spawn_blocking(move || {
        crate::prompt_stash::PromptStashStore::daemon_default().delete(&profile_id, &stash_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(response))
}

pub async fn get_session_history(
    AxumPath(session_id): AxumPath<String>,
    Query(request): Query<SessionHistoryRequest>,
) -> Result<Json<SessionHistoryResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;

    let paged = request.limit.is_some() || request.cursor.is_some();
    let (turns, next_cursor) = if paged {
        let limit = request.limit.unwrap_or(40).clamp(1, 200);
        let before_entry_seq = request
            .cursor
            .as_deref()
            .map(str::trim)
            .filter(|cursor| !cursor.is_empty())
            .map(|cursor| {
                cursor
                    .parse::<u64>()
                    .ok()
                    .filter(|seq| *seq > 0)
                    .ok_or_else(|| {
                        (
                            StatusCode::BAD_REQUEST,
                            "invalid session history cursor".to_string(),
                        )
                    })
            })
            .transpose()?;
        let page =
            crate::session::load_transcript_entries_page(&session_id, limit, before_entry_seq);
        (
            page.entries,
            page.next_cursor.map(|cursor| cursor.to_string()),
        )
    } else {
        (crate::session::load_transcript_entries(&session_id), None)
    };
    let authority_id = crate::workshop_authority::current()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?
        .clone();
    Ok(Json(SessionHistoryResponse {
        authority_id,
        session_id,
        turns,
        next_cursor,
    }))
}

pub async fn append_session_turn(
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SessionAppendTurnRequest>,
) -> Result<Json<SessionAppendTurnResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;

    crate::session::try_append_turn_with_scratch(&session_id, &request.turn, None)
        .await
        .map_err(|error| (StatusCode::CONFLICT, error.to_string()))?;
    Ok(Json(SessionAppendTurnResponse {
        session_id,
        stored: true,
    }))
}

pub async fn set_session_display_name(
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SessionSetDisplayNameRequest>,
) -> Result<Json<SessionSetDisplayNameResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;

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
    let session_id = validated_session_id(session_id)?;
    crate::agent_mode_state::get_session_mode(&session_id)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn list_agent_modes() -> Json<AgentModeListResponse> {
    Json(crate::agent_runtime::list_agent_modes())
}

pub async fn get_agent_mode_transition_policy() -> Json<AgentModeTransitionPolicy> {
    Json(crate::agent_mode_state::get_transition_policy())
}

pub async fn set_agent_mode_transition_policy(
    Json(policy): Json<AgentModeTransitionPolicy>,
) -> Result<Json<AgentModeTransitionPolicy>, (StatusCode, String)> {
    crate::agent_mode_state::set_transition_policy(policy)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn set_session_agent_mode(
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SetSessionAgentModeRequest>,
) -> Result<Json<SessionAgentModeResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
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
    let session_id = validated_session_id(session_id)?;
    crate::agent_mode_state::clear_session_mode(&session_id, query.scope)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn get_session_code_binding(
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<SessionCodeBindingResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
    crate::agent_mode_state::get_session_code_binding(&session_id)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn set_session_code_binding(
    State(state): State<crate::daemon::state::AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<SetSessionCodeBindingRequest>,
) -> Result<Json<SessionCodeBindingResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
    let work_id = request.work_id.trim();
    if work_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "work_id is required".to_string()));
    }
    let local_runtime_id = crate::workshop_authority::current()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))?
        .as_str()
        .to_string();
    let requested_runtime_id = request
        .execution_runtime_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let is_local = requested_runtime_id.is_none_or(|runtime_id| runtime_id == local_runtime_id);
    let resolved_repo_id = if is_local {
        let item = state
            .forge
            .load(&medousa_forge::model::WorkId::from(work_id.to_string()))
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("cannot bind undertaking: {err}"),
                )
            })?;
        item.workspace_environment()
            .map(|environment| environment.repo.repo_id.to_string())
            .or_else(|| request.repo_id.clone())
    } else {
        request.repo_id.clone()
    };
    if !is_local
        && resolved_repo_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "remote Coder binding requires a destination-authored repo_id".to_string(),
        ));
    }
    crate::agent_mode_state::set_session_code_binding_authority(
        &session_id,
        work_id,
        requested_runtime_id,
        resolved_repo_id.as_deref(),
    )
    .map(Json)
    .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn clear_session_code_binding(
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<SessionCodeBindingResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
    crate::agent_mode_state::clear_session_code_binding(&session_id)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn list_session_agent_mode_proposals(
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<AgentModeProposalListResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
    crate::agent_mode_state::list_mode_proposals(&session_id)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn decide_session_agent_mode_proposal(
    AxumPath((session_id, proposal_id)): AxumPath<(String, String)>,
    Json(request): Json<DecideAgentModeProposalRequest>,
) -> Result<Json<AgentModeProposalResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
    crate::agent_mode_state::decide_mode_proposal(&session_id, &proposal_id, request.accept)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}

pub async fn delete_session(
    State(state): State<SessionDeleteState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<SessionDeleteQuery>,
) -> Result<Json<SessionDeleteResponse>, (StatusCode, String)> {
    let session_id = validated_session_id(session_id)?;
    let summary = crate::session_lifecycle::delete_session(
        &session_id,
        state.memory_operations,
        &state.turn_tickets,
        state.turn_streams.as_ref(),
        query.purge_memory,
    )
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))?;

    Ok(Json(SessionDeleteResponse {
        session_id: summary.session_id,
        deletion_id: summary.deletion_id,
        status: summary.status,
        deleted: summary.deleted,
        locus_purged: summary.locus_purged,
        locus_nodes_deleted: summary.locus_nodes_deleted,
        cancelled_active_turn: summary.cancelled_active_turn,
        surfaces: summary.surfaces,
    }))
}

pub async fn get_session_deletion(
    AxumPath(deletion_id): AxumPath<String>,
) -> Result<Json<SessionDeleteResponse>, (StatusCode, String)> {
    let record = crate::session_deletion::coordinator()
        .find_record(&deletion_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "session deletion not found".to_string(),
            )
        })?;
    let summary = crate::session_lifecycle::SessionDeleteSummary::from_record(record);
    Ok(Json(SessionDeleteResponse {
        session_id: summary.session_id,
        deletion_id: summary.deletion_id,
        status: summary.status,
        deleted: summary.deleted,
        locus_purged: summary.locus_purged,
        locus_nodes_deleted: summary.locus_nodes_deleted,
        cancelled_active_turn: summary.cancelled_active_turn,
        surfaces: summary.surfaces,
    }))
}
