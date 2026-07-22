//! HTTP handlers for vault APIs (`/v1/vault/*`).

use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use crate::daemon_api::{
    VaultBacklinksQuery, VaultBacklinksResponse, VaultAddRootRequest, VaultDeleteResponse,
    VaultFileContentResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultNotesQuery,
    VaultPutQuery, VaultRootsResponse, VaultSearchQuery, VaultSearchResponse,
    VaultSetActiveRootRequest, VaultTagsListResponse, VaultTagsQuery, VaultWriteRequest,
    VaultWriteResponse,
};
use crate::vault::VaultService;
use crate::vault::roots::{add_vault_root, list_vault_root_views, set_active_vault_root};

fn map_vault_error(err: anyhow::Error) -> (StatusCode, String) {
    let message = err.to_string();
    if message.contains("not found") {
        (StatusCode::NOT_FOUND, message)
    } else if message.contains("If-Match") || message.contains("content_hash mismatch") {
        (StatusCode::PRECONDITION_FAILED, message)
    } else if message.contains("required") || message.contains("must not") {
        (StatusCode::BAD_REQUEST, message)
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

pub async fn list_vault_notes(
    Query(query): Query<VaultNotesQuery>,
) -> Result<Json<VaultNotesListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(100);
    Ok(Json(VaultService::list_notes(
        query.prefix.as_deref(),
        limit,
        query.tags.as_deref(),
        query.tag_prefix.as_deref(),
    )))
}

pub async fn list_vault_tags(
    Query(query): Query<VaultTagsQuery>,
) -> Result<Json<VaultTagsListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(100);
    Ok(Json(VaultService::list_tags(query.prefix.as_deref(), limit)))
}

pub async fn get_vault_note(
    Path(note_path): Path<String>,
) -> Result<Json<VaultNoteContentResponse>, (StatusCode, String)> {
    VaultService::get_note(&note_path)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn get_vault_file(
    Path(file_path): Path<String>,
) -> Result<Json<VaultFileContentResponse>, (StatusCode, String)> {
    VaultService::read_file(&file_path)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn put_vault_note(
    Path(note_path): Path<String>,
    Query(query): Query<VaultPutQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<VaultWriteResponse>, (StatusCode, String)> {
    let content = String::from_utf8(body.to_vec())
        .map_err(|err| (StatusCode::BAD_REQUEST, format!("invalid utf-8 body: {err}")))?;
    let if_match = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok());
    let request = VaultWriteRequest {
        path: None,
        content,
        session_id: query.session_id,
        semantic_tags: None,
        auto_workshop_tags: query.auto_workshop_tags.unwrap_or(true),
    };
    VaultService::write_note(Some(&note_path), &request, if_match)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn post_vault_note(
    Json(request): Json<VaultWriteRequest>,
) -> Result<Json<VaultWriteResponse>, (StatusCode, String)> {
    VaultService::create_note(&request)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn delete_vault_note(
    Path(note_path): Path<String>,
) -> Result<Json<VaultDeleteResponse>, (StatusCode, String)> {
    VaultService::delete_note(&note_path)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn search_vault_notes(
    Query(query): Query<VaultSearchQuery>,
) -> Result<Json<VaultSearchResponse>, (StatusCode, String)> {
    let q = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if q.is_none() && query.tags.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_none() {
        return Err((StatusCode::BAD_REQUEST, "q or tags is required".to_string()));
    }
    let limit = query.limit.unwrap_or(20);
    VaultService::search(q, limit, query.tags.as_deref())
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn get_vault_backlinks(
    Query(query): Query<VaultBacklinksQuery>,
) -> Result<Json<VaultBacklinksResponse>, (StatusCode, String)> {
    let note_path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "path is required".to_string()))?;
    VaultService::backlinks(note_path)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn list_vault_roots() -> Json<VaultRootsResponse> {
    Json(list_vault_root_views())
}

pub async fn set_vault_active_root(
    Json(request): Json<VaultSetActiveRootRequest>,
) -> Result<Json<VaultRootsResponse>, (StatusCode, String)> {
    set_active_vault_root(&request.root_id)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn add_vault_root_handler(
    Json(request): Json<VaultAddRootRequest>,
) -> Result<Json<VaultRootsResponse>, (StatusCode, String)> {
    add_vault_root(
        &request.label,
        &request.path,
        request.id.as_deref(),
    )
    .map(Json)
    .map_err(map_vault_error)
}
