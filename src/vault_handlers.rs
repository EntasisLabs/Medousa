//! HTTP handlers for vault APIs (`/v1/vault/*`).

use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post, put};
use axum::Json;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon_api::{
    VaultBacklinksQuery, VaultBacklinksResponse, VaultAddRootRequest, VaultDeleteResponse,
    VaultFileContentResponse, VaultNoteContentResponse, VaultNotesListResponse, VaultNotesQuery,
    VaultPutQuery, VaultRootsResponse, VaultSearchQuery, VaultSearchResponse,
    VaultSetActiveRootRequest, VaultTagsListResponse, VaultTagsQuery, VaultTrashListResponse,
    VaultTrashRestoreRequest, VaultTrashRestoreResponse, VaultWriteRequest, VaultWriteResponse,
};
use crate::vault::VaultService;
use crate::vault::roots::{add_vault_root, list_vault_root_views, set_active_vault_root};

pub fn vault_surface() -> DeclaredRouter {
    DeclaredRouter::default()
        .methods([
            (
                vault_admin_policy(axum::http::Method::GET, "/v1/vault/roots", 1024),
                get(list_vault_roots),
            ),
            (
                vault_admin_policy(axum::http::Method::POST, "/v1/vault/roots", 64 * 1024),
                post(add_vault_root_handler),
            ),
        ])
        .route(
            vault_admin_policy(axum::http::Method::PUT, "/v1/vault/active", 64 * 1024),
            put(set_vault_active_root),
        )
        .methods([
            (vault_read_policy("/v1/vault/notes"), get(list_vault_notes)),
            (
                vault_write_policy(axum::http::Method::POST, "/v1/vault/notes", 8 * 1024 * 1024),
                post(post_vault_note),
            ),
        ])
        .route(vault_read_policy("/v1/vault/tags"), get(list_vault_tags))
        .route(
            vault_read_policy("/v1/vault/search"),
            get(search_vault_notes),
        )
        .route(
            vault_read_policy("/v1/vault/backlinks"),
            get(get_vault_backlinks),
        )
        .route(
            vault_read_policy("/v1/vault/files/{*file_path}"),
            get(get_vault_file),
        )
        .methods([
            (
                vault_read_policy("/v1/vault/notes/{*note_path}"),
                get(get_vault_note),
            ),
            (
                vault_write_policy(
                    axum::http::Method::PUT,
                    "/v1/vault/notes/{*note_path}",
                    8 * 1024 * 1024,
                ),
                put(put_vault_note),
            ),
            (
                vault_write_policy(
                    axum::http::Method::DELETE,
                    "/v1/vault/notes/{*note_path}",
                    1024,
                ),
                delete(delete_vault_note),
            ),
        ])
        .route(vault_read_policy("/v1/vault/trash"), get(list_vault_trash))
        .route(
            vault_write_policy(
                axum::http::Method::POST,
                "/v1/vault/trash/restore",
                64 * 1024,
            ),
            post(restore_vault_trash),
        )
        .route(
            vault_admin_policy(axum::http::Method::GET, "/v1/vault/git/detect", 1024),
            get(crate::vault_git_handlers::vault_git_detect),
        )
        .route(
            vault_admin_policy(axum::http::Method::GET, "/v1/vault/git/status", 1024),
            get(crate::vault_git_handlers::vault_git_status),
        )
        .route(
            vault_admin_policy(axum::http::Method::POST, "/v1/vault/git/enable", 64 * 1024),
            post(crate::vault_git_handlers::vault_git_enable),
        )
        .route(
            vault_admin_policy(axum::http::Method::POST, "/v1/vault/git/init", 1024),
            post(crate::vault_git_handlers::vault_git_init),
        )
        .route(
            vault_admin_policy(axum::http::Method::POST, "/v1/vault/git/install", 1024),
            post(crate::vault_git_handlers::vault_git_install),
        )
        .route(
            vault_admin_policy(axum::http::Method::GET, "/v1/vault/git/log", 1024),
            get(crate::vault_git_handlers::vault_git_log),
        )
        .route(
            vault_admin_policy(axum::http::Method::POST, "/v1/vault/git/commit", 256 * 1024),
            post(crate::vault_git_handlers::vault_git_commit),
        )
        .route(
            vault_admin_policy(axum::http::Method::POST, "/v1/vault/git/restore", 64 * 1024),
            post(crate::vault_git_handlers::vault_git_restore),
        )
        .route(
            vault_admin_policy(axum::http::Method::GET, "/v1/vault/git/diff", 1024),
            get(crate::vault_git_handlers::vault_git_diff),
        )
        .methods([
            (
                vault_admin_policy(axum::http::Method::GET, "/v1/vault/git/worktrees", 1024),
                get(crate::vault_git_handlers::vault_git_worktrees_list),
            ),
        ])
}

fn vault_read_policy(path: &'static str) -> RoutePolicy {
    vault_policy(
        axum::http::Method::GET,
        path,
        RouteGroup::Portal,
        crate::request_principal::Capability::ContentRead,
        1024,
        RateLimitClass::Read,
    )
}

fn vault_write_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    vault_policy(
        method,
        path,
        RouteGroup::Portal,
        crate::request_principal::Capability::ContentWrite,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn vault_admin_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    vault_policy(
        method,
        path,
        RouteGroup::Administration,
        crate::request_principal::Capability::AdminRuntime,
        body_limit,
        RateLimitClass::Administration,
    )
}

fn vault_policy(
    method: axum::http::Method,
    path: &'static str,
    group: RouteGroup,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

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
    let content = String::from_utf8(body.into())
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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VaultTrashListQuery {
    pub limit: Option<usize>,
}

pub async fn list_vault_trash(
    Query(query): Query<VaultTrashListQuery>,
) -> Result<Json<VaultTrashListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(100);
    VaultService::list_trash(limit)
        .map(Json)
        .map_err(map_vault_error)
}

pub async fn restore_vault_trash(
    Json(request): Json<VaultTrashRestoreRequest>,
) -> Result<Json<VaultTrashRestoreResponse>, (StatusCode, String)> {
    VaultService::restore_from_trash(&request.path)
        .map(Json)
        .map_err(map_vault_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_inventory_separates_content_and_host_authority() {
        let entries = vault_surface().inventory().entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 24);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("content.read"))
                .count(),
            7
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("content.write"))
                .count(),
            4
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.required_capability == Some("admin.runtime"))
                .count(),
            13
        );
        assert!(entries.iter().any(|entry| {
            entry.path == "/v1/vault/git/worktrees" && entry.method == "GET"
        }));
        assert!(!entries.iter().any(|entry| {
            entry.path == "/v1/vault/git/worktrees" && entry.method != "GET"
        }));
    }
}
