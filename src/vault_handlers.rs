//! HTTP handlers for vault APIs (`/v1/vault/*`).

use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use crate::daemon_api::{
    VaultBacklinksResponse, VaultDeleteResponse, VaultNoteContentResponse, VaultNotesListResponse,
    VaultNotesQuery, VaultSearchQuery, VaultSearchResponse, VaultWriteRequest, VaultWriteResponse,
};
use crate::vault::VaultService;

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
    Ok(Json(VaultService::list_notes(query.prefix.as_deref(), limit)))
}

pub async fn get_vault_note(
    Path(note_path): Path<String>,
) -> Result<Json<VaultNoteContentResponse>, (StatusCode, String)> {
    VaultService::get_note(&note_path)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn put_vault_note(
    Path(note_path): Path<String>,
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
    };
    VaultService::write_note(Some(&note_path), &request, if_match)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn post_vault_note(
    Json(request): Json<VaultWriteRequest>,
) -> Result<Json<VaultWriteResponse>, (StatusCode, String)> {
    VaultService::write_note(None, &request, None)
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
        .filter(|value| !value.is_empty())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "q is required".to_string()))?;
    let limit = query.limit.unwrap_or(20);
    VaultService::search(q, limit)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn get_vault_backlinks(
    Path(note_path): Path<String>,
) -> Result<Json<VaultBacklinksResponse>, (StatusCode, String)> {
    VaultService::backlinks(&note_path)
        .map(Json)
        .map_err(map_vault_error)
}
