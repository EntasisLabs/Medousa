//! HTTP control plane for Forge undertakings (`/v1/forge/...`).
//!
//! Distinct from `/v1/workspace/cards` (activity board) and vault Versions
//! (material memory). Forge owns custody of intentional work episodes.

use std::ffi::OsStr;
use std::path::{Component, Path as FsPath, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post, put};
use medousa_forge::adapter::{ScriptAdapter, export_bundle};
use medousa_forge::error::ForgeError;
use medousa_forge::forge::{Forge, SealOptions};
use medousa_forge::git::{CheckpointAuthor, GitEngine};
use medousa_forge::model::{
    ActorKind, ActorRef, CompactEvidenceReceipt, EvidenceId, ExecutionLease, ExecutorDescriptor,
    GitOid, IntegrationStrategy, LeaseId, RecoveryDisposition, ReviewCommentId, ReviewDecision,
    ReviewDecisionId, WorkId, WorkItem, WorkPolicy, WorkState, WorkTarget,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::daemon_api::{
    CodeProjectSource, SessionCodeProjectResponse, StartSessionCodeProjectRequest,
};

use crate::daemon::forge_events::ForgeProjectEventKind;
use crate::daemon::forge_projections::{
    ItemProjection, ReviewCommentProjection, ReviewProjection, build_review_for_attempt,
    evidence_dir, project_item, project_items, read_lines_page,
};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::semantic_values::TrimmedText;

fn publish_item(state: &AppState, item: &WorkItem, kind: &str) {
    state
        .forge_events
        .publish(item.id.as_str(), &item.state.to_string(), kind);
}

fn remember_worktree(state: &AppState, item: &WorkItem, worktree: &FsPath) {
    state
        .forge_events
        .remember_worktree(item.id.as_str(), worktree.to_path_buf());
}

fn publish_project_change(
    state: &AppState,
    item: &WorkItem,
    kind: ForgeProjectEventKind,
    path: Option<String>,
    old_path: Option<String>,
    digest: Option<String>,
) {
    let event_kind = match kind {
        ForgeProjectEventKind::Created => "source_created",
        ForgeProjectEventKind::Changed => "source_saved",
        ForgeProjectEventKind::Renamed => "source_renamed",
        ForgeProjectEventKind::Deleted => "source_deleted",
        ForgeProjectEventKind::GitStatus => "git_status",
        ForgeProjectEventKind::Snapshot => "project_snapshot",
    };
    publish_item(state, item, event_kind);
    state
        .forge_events
        .publish_project(item.id.as_str(), kind, path, old_path, digest);
}

fn ok_item(state: &AppState, item: WorkItem, kind: &str) -> Json<ItemProjection> {
    publish_item(state, &item, kind);
    Json(project_item(item))
}

pub fn forge_surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .methods([
            (forge_read_policy("/v1/forge/items"), get(list_items)),
            (
                forge_mutation_policy(axum::http::Method::POST, "/v1/forge/items", 1024 * 1024),
                post(register_item),
            ),
        ])
        .route(
            forge_mutation_policy(
                axum::http::Method::POST,
                "/v1/forge/items/start",
                1024 * 1024,
            ),
            post(start_item),
        )
        .route(
            forge_mutation_policy(
                axum::http::Method::POST,
                "/v1/sessions/{session_id}/code-project",
                1024 * 1024,
            ),
            post(start_session_code_project),
        )
        .route(
            forge_post_policy("/v1/forge/repositories/inspect"),
            post(inspect_repository),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/repositories/provider"),
                get(provider_repository_capabilities),
            ),
            (
                forge_post_policy("/v1/forge/repositories/provider"),
                post(clone_provider_repository),
            ),
        ])
        .methods([
            (
                forge_read_policy("/v1/forge/repositories"),
                get(list_repositories),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::PUT,
                    "/v1/forge/repositories",
                    256 * 1024,
                ),
                put(update_repository_pin),
            ),
        ])
        .route(
            forge_read_policy("/v1/forge/repositories/browse"),
            get(browse_repositories),
        )
        .route(
            forge_read_policy("/v1/forge/items/{work_id}"),
            get(get_item),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/source"),
                get(read_source),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::POST,
                    "/v1/forge/items/{work_id}/source",
                    8 * 1024 * 1024,
                ),
                post(create_source),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::PUT,
                    "/v1/forge/items/{work_id}/source",
                    8 * 1024 * 1024,
                ),
                put(save_source),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::PATCH,
                    "/v1/forge/items/{work_id}/source",
                    256 * 1024,
                ),
                patch(rename_source),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::DELETE,
                    "/v1/forge/items/{work_id}/source",
                    1024,
                ),
                delete(delete_source),
            ),
        ])
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/tree"),
            get(source_tree),
        )
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/changes"),
            get(get_changes),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/changes/file"),
                get(get_changes_file),
            ),
            (
                forge_post_policy("/v1/forge/items/{work_id}/changes/file"),
                post(restore_changes_file),
            ),
        ])
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/fetch"),
            post(changes_fetch),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/pull"),
            post(changes_pull),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/push"),
            post(changes_push),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/sync"),
            post(changes_sync),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/checkpoint"),
            post(changes_checkpoint),
        )
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/changes/history"),
            get(changes_history),
        )
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/changes/blame"),
            get(changes_blame),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/conflict"),
            post(resolve_changes_conflict),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/changes/file/hunk"),
            post(revert_changes_hunk),
        )
        .route(
            forge_mutation_policy(
                axum::http::Method::PUT,
                "/v1/forge/items/{work_id}/source/batch",
                32 * 1024 * 1024,
            ),
            put(save_source_batch),
        )
        .route(
            forge_mutation_policy(
                axum::http::Method::PUT,
                "/v1/forge/items/{work_id}/source/workspace-edit",
                MAX_SOURCE_WORKSPACE_EDIT_BODY_BYTES,
            ),
            put(apply_source_workspace_edit),
        )
        .route(
            forge_stream_policy("/v1/forge/items/{work_id}/project-events"),
            get(forge_project_event_stream),
        )
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/search"),
            get(search_source),
        )
        .route(
            forge_mutation_policy(
                axum::http::Method::POST,
                "/v1/forge/items/{work_id}/search/replace",
                8 * 1024 * 1024,
            ),
            post(replace_source),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/workspace-state"),
                get(read_workspace_state),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::PUT,
                    "/v1/forge/items/{work_id}/workspace-state",
                    8 * 1024 * 1024,
                ),
                put(save_workspace_state),
            ),
        ])
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/review"),
            get(get_review),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/review/comments"),
                get(list_review_comments),
            ),
            (
                forge_post_policy("/v1/forge/items/{work_id}/review/comments"),
                post(add_review_comment),
            ),
        ])
        .methods([
            (
                forge_mutation_policy(
                    axum::http::Method::PATCH,
                    "/v1/forge/items/{work_id}/review/comments/{comment_id}",
                    1024 * 1024,
                ),
                patch(patch_review_comment),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::DELETE,
                    "/v1/forge/items/{work_id}/review/comments/{comment_id}",
                    1024,
                ),
                delete(delete_review_comment),
            ),
        ])
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/review/request-changes"),
            post(request_review_changes),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/review/continue-editing"),
            post(continue_editing),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/review/file"),
                get(get_review_file),
            ),
            (
                forge_post_policy("/v1/forge/items/{work_id}/review/file"),
                post(restore_review_file),
            ),
        ])
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/tasks"),
            get(list_project_tasks),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/tasks/{task_id}/run"),
            post(run_project_task),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/tasks/{task_id}/runs"),
            post(start_project_task_run),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/task-runs/{run_id}"),
                get(get_project_task_run),
            ),
            (
                forge_mutation_policy(
                    axum::http::Method::DELETE,
                    "/v1/forge/items/{work_id}/task-runs/{run_id}",
                    1024,
                ),
                delete(cancel_project_task_run),
            ),
        ])
        .route(
            forge_stream_policy("/v1/forge/items/{work_id}/task-runs/{run_id}/events"),
            get(project_task_run_events),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/task-runs/{run_id}/preview"),
            post(create_task_run_preview),
        )
        .route(
            forge_read_policy("/v1/forge/items/{work_id}/tests"),
            get(list_project_tests),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/provision"),
            post(provision_item),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/attempts"),
            post(begin_attempt),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/handoff"),
            post(prepare_handoff),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/provider"),
                get(get_provider_handoff),
            ),
            (
                forge_post_policy("/v1/forge/items/{work_id}/provider"),
                post(share_provider_handoff),
            ),
        ])
        .route(
            forge_mutation_policy(
                axum::http::Method::PUT,
                "/v1/forge/items/{work_id}/provider/context",
                8 * 1024 * 1024,
            ),
            put(save_provider_context),
        )
        .methods([
            (
                forge_read_policy("/v1/forge/items/{work_id}/provider/comments"),
                get(list_provider_comments),
            ),
            (
                forge_post_policy("/v1/forge/items/{work_id}/provider/comments"),
                post(import_provider_comment),
            ),
        ])
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/decisions"),
            post(record_decision),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/apply"),
            post(apply_decision),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/discard"),
            post(discard_item),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/run-script"),
            post(run_script),
        )
        .route(
            forge_post_policy("/v1/forge/items/{work_id}/export"),
            post(export_item),
        )
        .route(
            forge_read_policy("/v1/forge/evidence/{evidence_id}/patch"),
            get(evidence_patch),
        )
        .route(
            forge_read_policy("/v1/forge/evidence/{evidence_id}/commands"),
            get(evidence_commands),
        )
        .route(
            forge_read_policy("/v1/forge/evidence/{evidence_id}/receipts"),
            get(evidence_receipts),
        )
        .route(forge_stream_policy("/v1/forge/stream"), get(forge_stream))
        .route(
            forge_post_policy("/v1/forge/leases/{lease_id}/heartbeat"),
            post(heartbeat_lease),
        )
        .route(
            forge_post_policy("/v1/forge/leases/{lease_id}/complete"),
            post(complete_lease),
        )
        .route(
            forge_post_policy("/v1/forge/leases/{lease_id}/interrupt"),
            post(interrupt_lease),
        )
        .route(
            forge_post_policy("/v1/forge/leases/{lease_id}/fail"),
            post(fail_lease),
        )
}

fn forge_read_policy(path: &'static str) -> RoutePolicy {
    forge_policy(
        axum::http::Method::GET,
        path,
        1024,
        RateLimitClass::Administration,
    )
}

fn forge_stream_policy(path: &'static str) -> RoutePolicy {
    forge_policy(axum::http::Method::GET, path, 1024, RateLimitClass::Stream)
}

fn forge_post_policy(path: &'static str) -> RoutePolicy {
    forge_mutation_policy(axum::http::Method::POST, path, 1024 * 1024)
}

fn forge_mutation_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    forge_policy(method, path, body_limit, RateLimitClass::Administration)
}

fn forge_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(crate::request_principal::Capability::AdminExecute),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
}

type ApiError = (StatusCode, Json<ErrorBody>);
type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
}

fn map_err(err: ForgeError) -> ApiError {
    let (status, kind) = match &err {
        ForgeError::WorkNotFound(_) | ForgeError::AttemptNotFound(_) => {
            (StatusCode::NOT_FOUND, Some("not_found"))
        }
        ForgeError::InvalidState { .. }
        | ForgeError::StaleLease { .. }
        | ForgeError::AttemptAlreadyRunning(_)
        | ForgeError::BaseAdvanced { .. }
        | ForgeError::DecisionInvalid { .. }
        | ForgeError::EvidenceMismatch { .. }
        | ForgeError::EnvironmentDrift(_) => (StatusCode::CONFLICT, Some("conflict")),
        ForgeError::PolicyViolation(_) | ForgeError::CaptureBlocked(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Some("policy"))
        }
        ForgeError::RepositoryEmpty(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, Some("repository_empty"))
        }
        ForgeError::BaseRefMissing { .. } => (StatusCode::CONFLICT, Some("base_ref_missing")),
        ForgeError::Git(_) => (StatusCode::BAD_REQUEST, Some("git")),
        ForgeError::Store(_) | ForgeError::Io(_) | ForgeError::Json(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Some("store"))
        }
        ForgeError::Overloaded(_) => (StatusCode::SERVICE_UNAVAILABLE, Some("overloaded")),
        ForgeError::SlugConflict(_) | ForgeError::Conflict(_) => {
            (StatusCode::CONFLICT, Some("slug_conflict"))
        }
        ForgeError::CatalogStale(_) => (StatusCode::CONFLICT, Some("catalog_stale")),
        ForgeError::ObservationIncomplete(_) => {
            (StatusCode::CONFLICT, Some("observation_incomplete"))
        }
    };
    (
        status,
        Json(ErrorBody {
            error: err.to_string(),
            kind,
        }),
    )
}

/// Admit blocking Forge/Git/fs work off the Tokio request worker (ASYNC-001).
async fn admit_forge<T, F>(
    state: &AppState,
    class: medousa_forge::execution::ExecutionClass,
    estimated_bytes: usize,
    work: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    state
        .forge_execution
        .run(class, estimated_bytes, move || Ok(work()))
        .await
        .map_err(map_err)
        .and_then(|inner| inner)
}

async fn admit_forge_on_repo<T, F>(
    state: &AppState,
    class: medousa_forge::execution::ExecutionClass,
    estimated_bytes: usize,
    repo_key: Option<String>,
    work: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    state
        .forge_execution
        .run_on_repo(class, estimated_bytes, repo_key, move || Ok(work()))
        .await
        .map_err(map_err)
        .and_then(|inner| inner)
}

/// Heavy endpoints: admit work and record an executor-delay canary sample.
async fn admit_forge_canary<T, F>(
    state: &AppState,
    class: medousa_forge::execution::ExecutionClass,
    estimated_bytes: usize,
    repo_key: Option<String>,
    work: F,
) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    state
        .forge_execution
        .run_with_canary(class, estimated_bytes, repo_key, move || Ok(work()))
        .await
        .map_err(map_err)
        .and_then(|inner| inner)
}

fn actor_from_state(state: &AppState) -> ActorRef {
    ActorRef {
        kind: ActorKind::User,
        id: state.workshop_identity_user_id(),
    }
}

fn parse_work_id(raw: &str) -> ApiResult<WorkId> {
    WorkId::parse_storage(raw).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorBody {
                error: "invalid work_id".into(),
                kind: Some("bad_request"),
            }),
        )
    })
}

fn request_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    let kind = match status {
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PAYLOAD_TOO_LARGE => "too_large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media",
        StatusCode::INTERNAL_SERVER_ERROR => "store",
        _ => "bad_request",
    };
    (
        status,
        Json(ErrorBody {
            error: message.into(),
            kind: Some(kind),
        }),
    )
}

const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_BINARY_PREVIEW_BYTES: usize = 4 * 1024;
const BINARY_SCAN_BYTES: usize = 8 * 1024;

fn source_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn normalize_source_relative(raw: &str) -> ApiResult<(PathBuf, String)> {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "source path is required",
        ));
    }
    let relative = PathBuf::from(&normalized);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "source path must stay inside the governed workspace",
        ));
    }
    if relative
        .components()
        .next()
        .is_some_and(|part| part.as_os_str() == ".git")
    {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "the repository metadata directory is not editable",
        ));
    }
    let clean = relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(value) => Some(value.to_string_lossy()),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    Ok((relative, clean))
}

fn resolve_source_path(root: &FsPath, raw: &str) -> ApiResult<(PathBuf, String)> {
    let root = std::fs::canonicalize(root).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("governed workspace is unavailable: {err}"),
        )
    })?;
    resolve_source_path_under(&root, raw)
}

/// Like [`resolve_source_path`], but `root` must already be canonical.
/// Used by tree listing so we do not re-canonicalize the workspace root per file.
fn resolve_source_path_under(root: &FsPath, raw: &str) -> ApiResult<(PathBuf, String)> {
    let (relative, clean) = normalize_source_relative(raw)?;
    let candidate = std::fs::canonicalize(root.join(&relative)).map_err(|err| {
        request_error(
            StatusCode::NOT_FOUND,
            format!("source file not found: {err}"),
        )
    })?;
    if !candidate.starts_with(root) || !candidate.is_file() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "source path must name a file inside the governed workspace",
        ));
    }
    Ok((candidate, clean))
}

fn resolve_new_source_path(root: &FsPath, raw: &str) -> ApiResult<(PathBuf, String)> {
    let (relative, clean) = normalize_source_relative(raw)?;
    let root = std::fs::canonicalize(root).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("governed workspace is unavailable: {err}"),
        )
    })?;
    let parent_rel = relative.parent().unwrap_or_else(|| FsPath::new(""));
    let parent_joined = root.join(parent_rel);
    if !parent_joined.as_os_str().is_empty() && !parent_joined.exists() {
        std::fs::create_dir_all(&parent_joined).map_err(|err| {
            request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not create source parent directory: {err}"),
            )
        })?;
    }
    let parent = std::fs::canonicalize(&parent_joined).map_err(|err| {
        request_error(
            StatusCode::NOT_FOUND,
            format!("source parent directory not found: {err}"),
        )
    })?;
    if !parent.starts_with(&root) {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "source path must stay inside the governed workspace",
        ));
    }
    let file_name = relative
        .file_name()
        .ok_or_else(|| request_error(StatusCode::BAD_REQUEST, "source file name is required"))?;
    let candidate = parent.join(file_name);
    if candidate.exists() {
        return Err(request_error(
            StatusCode::CONFLICT,
            "a source file already exists at that path",
        ));
    }
    Ok((candidate, clean))
}

/// Resolve a new directory path inside the worktree (creates nothing yet).
fn resolve_new_directory_path(root: &FsPath, raw: &str) -> ApiResult<(PathBuf, String)> {
    let trimmed = raw.trim().trim_end_matches('/');
    let (relative, clean) = normalize_source_relative(trimmed)?;
    let root = std::fs::canonicalize(root).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("governed workspace is unavailable: {err}"),
        )
    })?;
    let candidate = root.join(&relative);
    if candidate.exists() {
        return Err(request_error(
            StatusCode::CONFLICT,
            "a path already exists at that location",
        ));
    }
    if let Some(parent) = relative
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let parent_joined = root.join(parent);
        if !parent_joined.exists() {
            std::fs::create_dir_all(&parent_joined).map_err(|err| {
                request_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("could not create parent directory: {err}"),
                )
            })?;
        }
        let parent = std::fs::canonicalize(&parent_joined).map_err(|err| {
            request_error(
                StatusCode::NOT_FOUND,
                format!("parent directory not found: {err}"),
            )
        })?;
        if !parent.starts_with(&root) {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                "directory path must stay inside the governed workspace",
            ));
        }
    }
    if !candidate.starts_with(&root)
        && candidate
            .parent()
            .is_none_or(|parent| !parent.starts_with(&root))
    {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "directory path must stay inside the governed workspace",
        ));
    }
    Ok((candidate, clean))
}

fn forge(state: &AppState) -> Arc<Forge> {
    state.forge.clone()
}

/// Resolve a presented lease by scanning active attempts for matching
/// `lease_id` + fencing `generation`.
fn resolve_lease(forge: &Forge, lease_id: &str, generation: u64) -> ApiResult<ExecutionLease> {
    let want = LeaseId::from(lease_id.to_string());
    forge.find_lease(&want, generation).map_err(map_err)
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    title: String,
    brief: String,
    repo_path: PathBuf,
    #[serde(default = "default_base_ref")]
    base_ref: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    policy: Option<WorkPolicy>,
}

#[derive(Debug, Deserialize)]
struct InspectRepositoryRequest {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct RepositoryInspection {
    path: PathBuf,
    display_name: String,
    current_branch: Option<String>,
    suggested_base_ref: Option<String>,
    has_commits: bool,
    dirty: bool,
    changed_files: usize,
    remotes: Vec<String>,
    local_branches: Vec<String>,
    remote_branches: Vec<RepositoryRemoteBranches>,
    existing_projects: Vec<ExistingRepositoryProject>,
    state_explanation: String,
    trust_explanation: String,
}

#[derive(Debug, Clone, Serialize)]
struct RepositoryRemoteBranches {
    name: String,
    branches: Vec<String>,
    default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ExistingRepositoryProject {
    id: String,
    title: String,
    state: String,
    human_phase: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepositoryCatalogStore {
    #[serde(default)]
    entries: Vec<RepositoryCatalogRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepositoryCatalogRecord {
    path: PathBuf,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    archived: bool,
    last_used_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct RepositoryCatalogEntry {
    #[serde(flatten)]
    repository: RepositoryInspection,
    pinned: bool,
    archived: bool,
    last_used_at: chrono::DateTime<chrono::Utc>,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateRepositoryPinRequest {
    path: PathBuf,
    #[serde(default)]
    pinned: Option<bool>,
    #[serde(default)]
    archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BrowseRepositoriesQuery {
    #[serde(default)]
    path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct RepositoryBrowseEntry {
    name: String,
    path: PathBuf,
    repository: bool,
}

#[derive(Debug, Serialize)]
struct RepositoryBrowseResponse {
    path: PathBuf,
    parent: Option<PathBuf>,
    repository: bool,
    places: Vec<RepositoryBrowseEntry>,
    entries: Vec<RepositoryBrowseEntry>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct ProviderRepositoryAdapter {
    provider: &'static str,
    label: &'static str,
    available: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct ProviderRepositoryCapabilities {
    adapters: Vec<ProviderRepositoryAdapter>,
}

#[derive(Debug, Deserialize)]
struct CloneProviderRepositoryRequest {
    provider: String,
    repository: String,
    parent: PathBuf,
}

static REPOSITORY_CATALOG_LOCK: Mutex<()> = Mutex::new(());

fn default_base_ref() -> String {
    "main".into()
}

fn repository_catalog_path() -> PathBuf {
    crate::daemon::forge_host::forge_root().join("repositories.json")
}

fn read_repository_catalog_unlocked() -> RepositoryCatalogStore {
    std::fs::read(repository_catalog_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn read_repository_catalog() -> RepositoryCatalogStore {
    let Ok(_guard) = REPOSITORY_CATALOG_LOCK.lock() else {
        return RepositoryCatalogStore::default();
    };
    read_repository_catalog_unlocked()
}

fn write_repository_catalog_unlocked(store: &RepositoryCatalogStore) -> ApiResult<()> {
    let bytes = serde_json::to_vec_pretty(store).map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not encode repository catalog: {err}"),
        )
    })?;
    crate::session::atomic_write(&repository_catalog_path(), &bytes).map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not save repository catalog: {err}"),
        )
    })
}

fn touch_repository(path: &FsPath, pinned: Option<bool>) -> ApiResult<()> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let _guard = REPOSITORY_CATALOG_LOCK.lock().map_err(|_| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repository catalog lock failed",
        )
    })?;
    let mut store = read_repository_catalog_unlocked();
    if let Some(entry) = store
        .entries
        .iter_mut()
        .find(|entry| entry.path == canonical)
    {
        entry.last_used_at = chrono::Utc::now();
        if let Some(pinned) = pinned {
            entry.pinned = pinned;
        }
    } else {
        store.entries.push(RepositoryCatalogRecord {
            path: canonical,
            pinned: pinned.unwrap_or(false),
            archived: false,
            last_used_at: chrono::Utc::now(),
        });
    }
    store.entries.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.last_used_at.cmp(&left.last_used_at))
    });
    store.entries.truncate(50);
    write_repository_catalog_unlocked(&store)
}

fn set_repository_archived(path: &FsPath, archived: bool) -> ApiResult<()> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let _guard = REPOSITORY_CATALOG_LOCK.lock().map_err(|_| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repository catalog lock failed",
        )
    })?;
    let mut store = read_repository_catalog_unlocked();
    if let Some(entry) = store
        .entries
        .iter_mut()
        .find(|entry| entry.path == canonical)
    {
        entry.archived = archived;
    } else {
        store.entries.push(RepositoryCatalogRecord {
            path: canonical,
            pinned: false,
            archived,
            last_used_at: chrono::Utc::now(),
        });
    }
    write_repository_catalog_unlocked(&store)
}

fn existing_projects_for_repository(
    items: &[WorkItem],
    path: &FsPath,
) -> Vec<ExistingRepositoryProject> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    items
        .iter()
        .filter(|item| !matches!(item.state, WorkState::Discarded | WorkState::Accepted))
        .filter(|item| match &item.target {
            WorkTarget::Git(target) => {
                target
                    .repo_path
                    .canonicalize()
                    .unwrap_or_else(|_| target.repo_path.clone())
                    == canonical
            }
        })
        .cloned()
        .map(|item| {
            let projection = project_item(item);
            ExistingRepositoryProject {
                id: projection.item.id.to_string(),
                title: projection.item.title,
                state: projection.item.state.to_string(),
                human_phase: projection.human_phase,
            }
        })
        .collect()
}

fn inspect_repository_path_from_items(
    requested: &FsPath,
    items: &[WorkItem],
) -> ApiResult<RepositoryInspection> {
    let git = GitEngine::detect().map_err(map_err)?;
    if !requested.exists() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            format!("folder does not exist: {}", requested.display()),
        ));
    }
    let path = git.worktree_root(requested).map_err(map_err)?;
    let current_branch = git.current_branch(&path).map_err(map_err)?;
    let has_commits = git.has_commits(&path).map_err(map_err)?;
    let suggested_base_ref = if has_commits {
        git.suggested_base_ref(&path).map_err(map_err)?
    } else {
        None
    };
    let changed_files = git.status_porcelain(&path).map_err(map_err)?.len();
    let identity = git.repo_identity(&path).map_err(map_err)?;
    let local_branches = git.local_branches(&path).unwrap_or_default();
    let remote_branches = git
        .remote_names(&path)
        .unwrap_or_default()
        .into_iter()
        .map(|name| RepositoryRemoteBranches {
            branches: git.remote_branches(&path, &name).unwrap_or_default(),
            default_branch: git.remote_default_branch(&path, &name),
            name,
        })
        .collect();
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Repository")
        .to_string();
    let existing_projects = existing_projects_for_repository(items, &path);
    let state_explanation = if !has_commits {
        "This repository has no commits yet. Create an initial commit before starting a Medousa project.".into()
    } else if changed_files > 0 {
        format!(
            "{changed_files} uncommitted {} already exist in the repository. Medousa starts from the committed revision, so those outside changes stay separate.",
            if changed_files == 1 {
                "change"
            } else {
                "changes"
            }
        )
    } else {
        "The repository is clean. Medousa will create an isolated working copy from the selected branch.".into()
    };
    Ok(RepositoryInspection {
        path,
        display_name,
        current_branch,
        suggested_base_ref,
        has_commits,
        dirty: changed_files > 0,
        changed_files,
        remotes: identity.remotes,
        local_branches,
        remote_branches,
        existing_projects,
        state_explanation,
        trust_explanation: "Medousa may read this repository and create an isolated working copy. Project commands run only when you explicitly choose a check or Terminal action.".into(),
    })
}

fn inspect_repository_path(
    state: &AppState,
    requested: &FsPath,
) -> ApiResult<RepositoryInspection> {
    let items = forge(state).list().map_err(map_err)?;
    inspect_repository_path_from_items(requested, &items)
}

async fn register_item(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let repository_path = body.repo_path.clone();
                let actor = actor_from_state(&state);
                let owner = body
                    .owner
                    .unwrap_or_else(|| state.workshop_identity_user_id());
                let forge = forge(&state);
                let item = if let Some(policy) = body.policy {
                    forge.register_with_policy(
                        body.title,
                        body.brief,
                        &body.repo_path,
                        body.base_ref,
                        owner,
                        policy,
                        &actor,
                    )
                } else {
                    forge.register(
                        body.title,
                        body.brief,
                        &body.repo_path,
                        body.base_ref,
                        owner,
                        &actor,
                    )
                }
                .map_err(map_err)?;
                touch_repository(&repository_path, None)?;
                Ok(ok_item(&state, item, "registered"))
            }
        },
    )
    .await
}

async fn inspect_repository(
    State(state): State<AppState>,
    Json(body): Json<InspectRepositoryRequest>,
) -> ApiResult<Json<RepositoryInspection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let repository = inspect_repository_path(&state, &body.path)?;
                touch_repository(&repository.path, None)?;
                Ok(Json(repository))
            }
        },
    )
    .await
}

async fn list_repositories(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<RepositoryCatalogEntry>>> {
    admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let mut store = read_repository_catalog();
                let items = forge(&state).list().map_err(map_err)?;
                for item in &items {
                    let WorkTarget::Git(target) = &item.target;
                    let path = target
                        .repo_path
                        .canonicalize()
                        .unwrap_or_else(|_| target.repo_path.clone());
                    if store.entries.iter().all(|entry| entry.path != path) {
                        store.entries.push(RepositoryCatalogRecord {
                            path,
                            pinned: false,
                            archived: false,
                            last_used_at: item.updated_at,
                        });
                    }
                }
                store.entries.sort_by(|left, right| {
                    right
                        .pinned
                        .cmp(&left.pinned)
                        .then_with(|| right.last_used_at.cmp(&left.last_used_at))
                });
                let entries = store
                    .entries
                    .into_iter()
                    .take(20)
                    .map(|record| {
                        let available = record.path.is_dir();
                        let repository = inspect_repository_path_from_items(&record.path, &items).unwrap_or_else(|_| {
                            RepositoryInspection {
                                display_name: record
                                    .path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("Repository")
                                    .to_string(),
                                path: record.path.clone(),
                                current_branch: None,
                                suggested_base_ref: None,
                                has_commits: false,
                                dirty: false,
                                changed_files: 0,
                                remotes: Vec::new(),
                                local_branches: Vec::new(),
                                remote_branches: Vec::new(),
                                existing_projects: existing_projects_for_repository(&items, &record.path),
                                state_explanation: "This repository is not currently available on the connected workshop.".into(),
                                trust_explanation: "No files or commands can be accessed until this workshop repository is available again.".into(),
                            }
                        });
                        RepositoryCatalogEntry {
                            repository,
                            pinned: record.pinned,
                            archived: record.archived,
                            last_used_at: record.last_used_at,
                            available,
                        }
                    })
                    .collect();
                Ok(Json(entries))
            }
        },
    )
    .await
}

async fn update_repository_pin(
    State(state): State<AppState>,
    Json(body): Json<UpdateRepositoryPinRequest>,
) -> ApiResult<Json<Vec<RepositoryCatalogEntry>>> {
    if body.pinned.is_none() && body.archived.is_none() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "pinned or archived is required",
        ));
    }
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let canonical = body
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| body.path.clone());
                if let Some(pinned) = body.pinned {
                    let repository = inspect_repository_path(&state, &canonical)?;
                    touch_repository(&repository.path, Some(pinned))?;
                }
                if let Some(archived) = body.archived {
                    set_repository_archived(&canonical, archived)?;
                }
                Ok(())
            }
        },
    )
    .await?;
    list_repositories(State(state)).await
}

fn repository_browse_roots(state: &AppState) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir().and_then(|path| path.canonicalize().ok()) {
        roots.push(home);
    }
    for common in ["/workspace", "/workspaces", "/Volumes", "/mnt", "/srv"] {
        let path = PathBuf::from(common);
        if let Ok(path) = path.canonicalize() {
            roots.push(path);
        }
    }
    roots.extend(windows_repository_browse_roots());
    for record in read_repository_catalog().entries {
        if let Some(parent) = record
            .path
            .parent()
            .and_then(|path| path.canonicalize().ok())
        {
            roots.push(parent);
        }
    }
    if let Ok(items) = forge(state).list() {
        for item in items {
            let WorkTarget::Git(target) = item.target;
            if let Some(parent) = target
                .repo_path
                .parent()
                .and_then(|path| path.canonicalize().ok())
            {
                roots.push(parent);
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(windows)]
fn windows_repository_browse_roots() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .filter_map(|letter| {
            PathBuf::from(format!("{}:\\", char::from(letter)))
                .canonicalize()
                .ok()
        })
        .collect()
}

#[cfg(not(windows))]
fn windows_repository_browse_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn browse_path_allowed(path: &FsPath, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

fn browse_entry(path: PathBuf) -> RepositoryBrowseEntry {
    RepositoryBrowseEntry {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| path.to_str().unwrap_or("Folder"))
            .to_string(),
        repository: path.join(".git").exists(),
        path,
    }
}

async fn browse_repositories(
    State(state): State<AppState>,
    Query(query): Query<BrowseRepositoriesQuery>,
) -> ApiResult<Json<RepositoryBrowseResponse>> {
    admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let roots = repository_browse_roots(&state);
                let requested = query.path.or_else(dirs::home_dir).ok_or_else(|| {
                    request_error(StatusCode::NOT_FOUND, "workshop home folder is unavailable")
                })?;
                let path = requested.canonicalize().map_err(|err| {
                    request_error(
                        StatusCode::NOT_FOUND,
                        format!("folder is unavailable: {err}"),
                    )
                })?;
                if !path.is_dir() || !browse_path_allowed(&path, &roots) {
                    return Err(request_error(
                        StatusCode::FORBIDDEN,
                        "folder is outside the repository browser's workshop places",
                    ));
                }
                let parent = path
                    .parent()
                    .and_then(|parent| parent.canonicalize().ok())
                    .filter(|parent| browse_path_allowed(parent, &roots));
                let mut entries = Vec::new();
                let read_dir = std::fs::read_dir(&path).map_err(|err| {
                    request_error(
                        StatusCode::BAD_REQUEST,
                        format!("could not read folder: {err}"),
                    )
                })?;
                let mut truncated = false;
                for entry in read_dir {
                    let Ok(entry) = entry else { continue };
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with('.') || !entry.path().is_dir() {
                        continue;
                    }
                    let Ok(candidate) = entry.path().canonicalize() else {
                        continue;
                    };
                    if !browse_path_allowed(&candidate, &roots) {
                        continue;
                    }
                    if entries.len() == 500 {
                        truncated = true;
                        break;
                    }
                    entries.push(browse_entry(candidate));
                }
                entries.sort_by(|left, right| {
                    right.repository.cmp(&left.repository).then_with(|| {
                        left.name
                            .to_ascii_lowercase()
                            .cmp(&right.name.to_ascii_lowercase())
                    })
                });
                let places = roots.into_iter().map(browse_entry).collect();
                Ok(Json(RepositoryBrowseResponse {
                    repository: path.join(".git").exists(),
                    path,
                    parent,
                    places,
                    entries,
                    truncated,
                }))
            }
        },
    )
    .await
}

fn provider_adapter(
    provider: &'static str,
    label: &'static str,
    command: &str,
) -> ProviderRepositoryAdapter {
    let available = command_available(command);
    ProviderRepositoryAdapter {
        provider,
        label,
        available,
        message: if available {
            format!("{label} is ready on the connected workshop.")
        } else {
            format!("Install and sign in to the {label} CLI on the connected workshop.")
        },
    }
}

async fn provider_repository_capabilities() -> Json<ProviderRepositoryCapabilities> {
    Json(ProviderRepositoryCapabilities {
        adapters: vec![
            provider_adapter("github", "GitHub", "gh"),
            provider_adapter("gitlab", "GitLab", "glab"),
        ],
    })
}

fn normalize_provider_repository(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/').trim_end_matches(".git");
    let repository = if let Some((_, repository)) = provider_repository(trimmed) {
        repository
    } else {
        trimmed.to_string()
    };
    normalize_provider_repository_name(&repository).then_some(repository)
}

async fn clone_provider_repository(
    State(state): State<AppState>,
    Json(body): Json<CloneProviderRepositoryRequest>,
) -> ApiResult<Json<RepositoryInspection>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::NetworkGit,
        64 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let (provider, command, label) =
                    match body.provider.trim().to_ascii_lowercase().as_str() {
                        "github" => ("github", "gh", "GitHub"),
                        "gitlab" => ("gitlab", "glab", "GitLab"),
                        _ => {
                            return Err(request_error(
                                StatusCode::BAD_REQUEST,
                                "Repository provider must be GitHub or GitLab",
                            ));
                        }
                    };
                if !command_available(command) {
                    return Err(request_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        format!(
                            "Install and sign in to the {label} CLI on the connected workshop."
                        ),
                    ));
                }
                let repository =
                    normalize_provider_repository(&body.repository).ok_or_else(|| {
                        request_error(
                            StatusCode::BAD_REQUEST,
                            "Repository must look like owner/project or a supported repository URL",
                        )
                    })?;
                if let Some((remote_provider, _)) = provider_repository(body.repository.trim())
                    && remote_provider != provider
                {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "The repository URL does not match the selected provider",
                    ));
                }
                let parent = body.parent.canonicalize().map_err(|err| {
                    request_error(
                        StatusCode::NOT_FOUND,
                        format!("Destination folder is unavailable: {err}"),
                    )
                })?;
                if !parent.is_dir()
                    || !browse_path_allowed(&parent, &repository_browse_roots(&state))
                {
                    return Err(request_error(
                        StatusCode::FORBIDDEN,
                        "Destination is outside the connected workshop's available places",
                    ));
                }
                let name = repository.rsplit('/').next().ok_or_else(|| {
                    request_error(StatusCode::BAD_REQUEST, "Repository name is unavailable")
                })?;
                let destination = parent.join(name);
                if destination.exists() {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        format!("A folder named {name} already exists here"),
                    ));
                }
                let mut clone = background_command(command);
                clone.args(["repo", "clone", &repository]);
                let output = clone
                    .arg(&destination)
                    .current_dir(&parent)
                    .output()
                    .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
                if !output.status.success() {
                    return Err(provider_command_error("Cloning the repository", &output));
                }
                let inspection = inspect_repository_path(&state, &destination)?;
                touch_repository(&inspection.path, None)?;
                Ok(Json(inspection))
            }
        },
    )
    .await
}

async fn start_item(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                start_item_from_request(&state, body).map(|item| ok_item(&state, item, "started"))
            }
        },
    )
    .await
}

fn start_item_from_request(state: &AppState, body: RegisterRequest) -> ApiResult<WorkItem> {
    let repository_path = body.repo_path.clone();
    let actor = actor_from_state(state);
    let owner = body
        .owner
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let forge = forge(state);
    let registered = if let Some(policy) = body.policy {
        forge.register_with_policy(
            body.title,
            body.brief,
            &body.repo_path,
            body.base_ref,
            owner,
            policy,
            &actor,
        )
    } else {
        forge.register(
            body.title,
            body.brief,
            &body.repo_path,
            body.base_ref,
            owner,
            &actor,
        )
    }
    .map_err(map_err)?;
    touch_repository(&repository_path, None)?;
    publish_item(state, &registered, "registered");
    let item = forge.provision(&registered.id, &actor).map_err(map_err)?;
    if let Some(env) = item.workspace_environment() {
        crate::daemon::detamu_host::spawn_index_forge_item(
            state.detamu.clone(),
            item.id.as_str().to_owned(),
            env.worktree.clone(),
            env.baseline_oid.as_str().to_owned(),
            crate::daemon::detamu_host::BindingKind::Baseline,
        );
    }
    Ok(item)
}

async fn start_session_code_project(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(body): Json<StartSessionCodeProjectRequest>,
) -> ApiResult<Json<SessionCodeProjectResponse>> {
    crate::session_storage::validate_session_id(&session_id)
        .map_err(|error| request_error(StatusCode::BAD_REQUEST, error.to_string()))?;
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        {
            let state = state.clone();
            let session_id = session_id.clone();
            move || start_code_project_for_session_inner(&state, &session_id, body).map(Json)
        },
    )
    .await
}

pub(crate) fn start_code_project_for_session(
    state: &AppState,
    session_id: &str,
    body: StartSessionCodeProjectRequest,
) -> Result<SessionCodeProjectResponse, String> {
    start_code_project_for_session_inner(state, session_id, body)
        .map_err(|(_, Json(error))| error.error)
}

#[derive(Debug)]
struct StartCodeProjectCommand {
    session_id: TrimmedText,
    title: TrimmedText,
    brief: TrimmedText,
    source: CodeProjectSource,
    repo_path: Option<TrimmedText>,
    base_ref: TrimmedText,
}

impl StartCodeProjectCommand {
    fn new(session_id: &str, input: StartSessionCodeProjectRequest) -> Result<Self, String> {
        let StartSessionCodeProjectRequest {
            title,
            brief,
            source,
            repo_path,
            base_ref,
        } = input;
        let (session_id, title, brief) = match (
            TrimmedText::new(session_id.to_string()),
            TrimmedText::new(title),
            TrimmedText::new(brief),
        ) {
            (Ok(session_id), Ok(title), Ok(brief)) => (session_id, title, brief),
            _ => return Err("session_id, title, and brief are required".to_string()),
        };
        let repo_path = repo_path.and_then(|value| TrimmedText::new(value).ok());
        if source == CodeProjectSource::Repository && repo_path.is_none() {
            return Err("repo_path is required for an existing repository".to_string());
        }
        let base_ref = base_ref
            .and_then(|value| TrimmedText::new(value).ok())
            .unwrap_or_else(|| TrimmedText::new("main").expect("literal is nonblank"));
        Ok(Self {
            session_id,
            title,
            brief,
            source,
            repo_path,
            base_ref,
        })
    }
}

fn start_code_project_for_session_inner(
    state: &AppState,
    session_id: &str,
    body: StartSessionCodeProjectRequest,
) -> ApiResult<SessionCodeProjectResponse> {
    let command = StartCodeProjectCommand::new(session_id, body)
        .map_err(|error| request_error(StatusCode::BAD_REQUEST, error))?;
    let session_id = command.session_id.as_str();
    let title = command.title.as_str();
    let brief = command.brief.as_str();
    let base_ref = command.base_ref.as_str().to_string();
    let (repo_path, created_repository) = match command.source {
        CodeProjectSource::Blank => (create_blank_repository(title, &base_ref)?, true),
        CodeProjectSource::Repository => {
            let path = command
                .repo_path
                .as_ref()
                .expect("validated repository path");
            (PathBuf::from(path.as_str()), false)
        }
    };

    let item = match start_item_from_request(
        state,
        RegisterRequest {
            title: title.to_string(),
            brief: brief.to_string(),
            repo_path: repo_path.clone(),
            base_ref: base_ref.clone(),
            owner: None,
            policy: None,
        },
    ) {
        Ok(item) => item,
        Err(err) => {
            if created_repository {
                let _ = std::fs::remove_dir_all(&repo_path);
            }
            return Err(err);
        }
    };
    let worktree = match item.workspace_environment() {
        Some(environment) => environment.worktree.to_string_lossy().into_owned(),
        None => {
            let _ = state.forge.discard(&item.id, &actor_from_state(state));
            if created_repository {
                let _ = std::fs::remove_dir_all(&repo_path);
            }
            return Err(request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Forge did not provision a governed worktree",
            ));
        }
    };
    if let Err(err) =
        crate::agent_mode_state::set_session_code_binding(session_id, item.id.as_str())
    {
        let _ = state.forge.discard(&item.id, &actor_from_state(state));
        if created_repository {
            let _ = std::fs::remove_dir_all(&repo_path);
        }
        return Err(request_error(StatusCode::INTERNAL_SERVER_ERROR, err));
    }

    let WorkTarget::Git(target) = &item.target;
    let response = SessionCodeProjectResponse {
        session_id: session_id.to_string(),
        work_id: item.id.to_string(),
        title: item.title,
        brief: item.brief,
        state: item.state.to_string(),
        human_phase: crate::daemon::forge_projections::human_phase(item.state).to_string(),
        repo_path: target.repo_path.to_string_lossy().into_owned(),
        worktree,
        base_ref: target.base_ref.clone(),
        created_repository,
    };
    Ok(response)
}

fn create_blank_repository(title: &str, base_ref: &str) -> ApiResult<PathBuf> {
    let root = crate::paths::medousa_data_dir().join("projects");
    std::fs::create_dir_all(&root).map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not create the Medousa projects folder: {err}"),
        )
    })?;
    let slug = project_slug(title);
    let mut destination = root.join(&slug);
    for suffix in 2..=999 {
        if !destination.exists() {
            break;
        }
        destination = root.join(format!("{slug}-{suffix}"));
    }
    if destination.exists() {
        return Err(request_error(
            StatusCode::CONFLICT,
            "Could not choose an available project folder",
        ));
    }
    std::fs::create_dir(&destination).map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not create the project folder: {err}"),
        )
    })?;

    if let Err(err) = initialize_blank_repository(&destination, title, base_ref) {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(request_error(StatusCode::INTERNAL_SERVER_ERROR, err));
    }
    Ok(destination)
}

fn initialize_blank_repository(
    destination: &FsPath,
    title: &str,
    base_ref: &str,
) -> Result<(), String> {
    let init = background_command("git")
        .args(["init", "-b", base_ref])
        .current_dir(destination)
        .output()
        .map_err(|err| format!("Could not start Git: {err}"))?;
    if !init.status.success() {
        return Err(format!(
            "Could not initialize Git: {}",
            String::from_utf8_lossy(&init.stderr).trim()
        ));
    }
    std::fs::write(
        destination.join("README.md"),
        format!("# {title}\n\nCreated with Medousa.\n"),
    )
    .map_err(|err| format!("Could not create README.md: {err}"))?;
    let add = background_command("git")
        .args(["add", "README.md"])
        .current_dir(destination)
        .output()
        .map_err(|err| format!("Could not stage the initial project: {err}"))?;
    if !add.status.success() {
        return Err(format!(
            "Could not stage the initial project: {}",
            String::from_utf8_lossy(&add.stderr).trim()
        ));
    }
    let commit = background_command("git")
        .args([
            "-c",
            "user.name=Medousa",
            "-c",
            "user.email=medousa@local",
            "commit",
            "-m",
            "Initial commit",
        ])
        .current_dir(destination)
        .output()
        .map_err(|err| format!("Could not create the initial commit: {err}"))?;
    if !commit.status.success() {
        return Err(format!(
            "Could not create the initial commit: {}",
            String::from_utf8_lossy(&commit.stderr).trim()
        ));
    }
    Ok(())
}

fn project_slug(title: &str) -> String {
    medousa_forge::slug::project_slug(title)
}

#[derive(Debug, Deserialize)]
struct ListItemsQuery {
    limit: Option<usize>,
    cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct ItemPage {
    items: Vec<ItemProjection>,
    next_cursor: Option<String>,
    truncated: bool,
}

async fn list_items(
    State(state): State<AppState>,
    Query(query): Query<ListItemsQuery>,
) -> ApiResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let forge = forge(&state);
    let paginated = query.limit.is_some() || query.cursor.is_some();
    let result = admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        1024 * 1024,
        None,
        move || {
            if paginated {
                let page = forge
                    .list_page(query.limit, query.cursor.as_deref())
                    .map_err(map_err)?;
                let mut items = Vec::with_capacity(page.items.len());
                for entry in page.items {
                    items.push(forge.load(&entry.work_id).map_err(map_err)?);
                }
                Ok(serde_json::to_value(ItemPage {
                    items: project_items(items),
                    next_cursor: page.next_cursor,
                    truncated: page.truncated,
                })
                .unwrap_or(serde_json::Value::Null))
            } else {
                let items = forge.list().map_err(map_err)?;
                Ok(serde_json::to_value(project_items(items)).unwrap_or(serde_json::Value::Null))
            }
        },
    )
    .await?;
    Ok(Json(result).into_response())
}

async fn get_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                Ok(Json(project_item(item)))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct SourceQuery {
    path: String,
}

#[derive(Debug, Serialize)]
struct SourceResponse {
    work_id: String,
    path: String,
    content: String,
    digest: String,
    byte_size: usize,
    /// `utf-8`, `utf-8-lossy`, or `binary`.
    #[serde(skip_serializing_if = "Option::is_none")]
    encoding: Option<String>,
    /// True when `content` is a bounded preview rather than the full editable body.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    preview: bool,
    /// True when a text preview was truncated to the editor byte limit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct SaveSourceRequest {
    path: String,
    content: String,
    lease_id: String,
    generation: u64,
    expected_digest: String,
}

#[derive(Debug, Deserialize)]
struct SaveSourceBatchRequest {
    files: Vec<SaveSourceBatchFile>,
    lease_id: String,
    generation: u64,
}

#[derive(Debug, Deserialize)]
struct SaveSourceBatchFile {
    path: String,
    content: String,
    expected_digest: String,
}

#[derive(Debug, Deserialize)]
struct SourceWorkspaceEditRequest {
    preconditions: Vec<SourceWorkspacePrecondition>,
    operations: Vec<SourceWorkspaceOperation>,
    lease_id: String,
    generation: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SourceWorkspacePrecondition {
    Existing {
        path: String,
        expected_digest: String,
    },
    Missing {
        path: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SourceWorkspaceOperation {
    Write { path: String, content: String },
    Create { path: String, content: String },
    Rename { path: String, destination: String },
    Delete { path: String },
}

#[derive(Debug, Deserialize)]
struct CreateSourceRequest {
    path: String,
    #[serde(default)]
    content: String,
    /// `file` (default) or `directory`.
    #[serde(default)]
    kind: Option<String>,
    lease_id: String,
    generation: u64,
}

#[derive(Debug, Deserialize)]
struct RenameSourceRequest {
    path: String,
    destination: String,
    lease_id: String,
    generation: u64,
    expected_digest: String,
}

#[derive(Debug, Deserialize)]
struct DeleteSourceRequest {
    path: String,
    lease_id: String,
    generation: u64,
    expected_digest: String,
}

#[derive(Debug, Serialize)]
struct DeleteSourceResponse {
    work_id: String,
    path: String,
    deleted: bool,
}

#[derive(Debug, Serialize)]
struct SourceTreeFile {
    path: String,
    byte_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct SourceTreeResponse {
    work_id: String,
    files: Vec<SourceTreeFile>,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct SourceSearchQuery {
    query: String,
    /// `literal` (default) or `regex`.
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    case_sensitive: Option<String>,
    #[serde(default)]
    whole_word: Option<String>,
    /// Comma-separated include globs (git pathspecs).
    #[serde(default)]
    include: Option<String>,
    /// Comma-separated exclude globs.
    #[serde(default)]
    exclude: Option<String>,
    #[serde(default)]
    include_ignored: Option<String>,
    /// `all` (default) or `changed`.
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    /// Opaque skip cursor from a prior `next_cursor`.
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct SourceSearchHit {
    path: String,
    line: u32,
    preview: String,
}

#[derive(Debug, Serialize)]
struct SourceSearchResponse {
    work_id: String,
    hits: Vec<SourceSearchHit>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SourceReplaceRequest {
    query: String,
    replacement: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    whole_word: Option<bool>,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    exclude: Option<String>,
    #[serde(default)]
    include_ignored: Option<bool>,
    #[serde(default)]
    scope: Option<String>,
    /// Cap on files included in the plan (default 50, max 100).
    #[serde(default)]
    limit: Option<u32>,
    /// When true (default), return a preview plan without writing.
    #[serde(default)]
    dry_run: Option<bool>,
    /// Optional subset of paths from a prior preview to apply or re-preview.
    #[serde(default)]
    paths: Option<Vec<String>>,
    #[serde(default)]
    preconditions: Option<Vec<SourceReplacePrecondition>>,
    #[serde(default)]
    lease_id: Option<String>,
    #[serde(default)]
    generation: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct SourceReplacePrecondition {
    path: String,
    expected_digest: String,
}

#[derive(Debug, Serialize)]
struct SourceReplaceFile {
    path: String,
    expected_digest: String,
    match_count: u32,
    before: String,
    after: String,
}

#[derive(Debug, Serialize)]
struct SourceReplaceResponse {
    work_id: String,
    files: Vec<SourceReplaceFile>,
    truncated: bool,
    applied: bool,
}

const MAX_REPLACE_FILES: usize = 100;
const DEFAULT_REPLACE_FILES: usize = 50;

#[derive(Debug, Clone)]
struct SourceSearchOptions {
    needle: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    include: Vec<String>,
    exclude: Vec<String>,
    include_ignored: bool,
    changed_only: bool,
    limit: usize,
    skip: usize,
}

fn parse_query_bool(value: Option<&str>, default: bool) -> bool {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => default,
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        Some(_) => default,
    }
}

fn parse_csv_globs(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.replace('\\', "/"))
        .collect()
}

fn source_search_options_from_query(query: &SourceSearchQuery) -> ApiResult<SourceSearchOptions> {
    let needle = query.query.trim().to_owned();
    if needle.len() < 2 || needle.len() > 200 {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "repository search must be between 2 and 200 characters",
        ));
    }
    let mode = query
        .mode
        .as_deref()
        .unwrap_or("literal")
        .trim()
        .to_ascii_lowercase();
    let regex = match mode.as_str() {
        "literal" | "" => false,
        "regex" => true,
        _ => {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                "mode must be literal or regex",
            ));
        }
    };
    let scope = query
        .scope
        .as_deref()
        .unwrap_or("all")
        .trim()
        .to_ascii_lowercase();
    let changed_only = match scope.as_str() {
        "all" | "" => false,
        "changed" => true,
        _ => {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                "scope must be all or changed",
            ));
        }
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 500) as usize;
    let skip = query
        .cursor
        .as_deref()
        .unwrap_or("0")
        .trim()
        .parse::<usize>()
        .unwrap_or(0);
    Ok(SourceSearchOptions {
        needle,
        regex,
        case_sensitive: parse_query_bool(query.case_sensitive.as_deref(), true),
        whole_word: parse_query_bool(query.whole_word.as_deref(), false),
        include: parse_csv_globs(query.include.as_deref()),
        exclude: parse_csv_globs(query.exclude.as_deref()),
        include_ignored: parse_query_bool(query.include_ignored.as_deref(), false),
        changed_only,
        limit,
        skip,
    })
}

fn changed_repository_paths(root: &FsPath) -> ApiResult<Vec<String>> {
    let output = background_command("git")
        .args(["status", "--porcelain", "-z", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .map_err(|err| {
            request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not list changed files: {err}"),
            )
        })?;
    if !output.status.success() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let mut paths = Vec::new();
    for entry in output
        .stdout
        .split(|&b| b == 0)
        .filter(|chunk| !chunk.is_empty())
    {
        if entry.len() < 3 {
            continue;
        }
        // XY<space>path — rename lines are "R  old\0new\0"; porcelain -z uses
        // "R  old\0new" with the next null-separated chunk as the new path.
        let text = String::from_utf8_lossy(entry);
        if text.len() < 3 {
            continue;
        }
        let path = text[3..].replace('\\', "/");
        if !path.is_empty() {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn run_repository_search(
    root: &FsPath,
    options: &SourceSearchOptions,
) -> ApiResult<(Vec<SourceSearchHit>, bool, Option<String>)> {
    use std::io::BufRead;
    use std::process::Stdio;

    let mut args = vec!["grep".to_owned(), "-n".into(), "-I".into()];
    if options.regex {
        args.push("-E".into());
    } else {
        args.push("-F".into());
    }
    if !options.case_sensitive {
        args.push("-i".into());
    }
    if options.whole_word {
        args.push("-w".into());
    }
    if !options.include_ignored {
        args.push("--exclude-standard".into());
    }
    args.push("--untracked".into());
    args.push("--".into());
    args.push(options.needle.clone());

    let mut pathspecs: Vec<String> = Vec::new();
    if options.changed_only {
        let changed = changed_repository_paths(root)?;
        if changed.is_empty() {
            return Ok((Vec::new(), false, None));
        }
        pathspecs.extend(changed);
    }
    pathspecs.extend(options.include.iter().cloned());
    for exclude in &options.exclude {
        pathspecs.push(format!(":(exclude){exclude}"));
    }
    args.extend(pathspecs);

    let mut child = background_command("git")
        .args(&args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not search repository: {err}"),
            )
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "repository search had no output",
        )
    })?;
    let mut hits = Vec::new();
    let mut seen = 0usize;
    let mut truncated = false;
    let page_end = options.skip.saturating_add(options.limit);
    for line in std::io::BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
    {
        let mut parts = line.splitn(3, ':');
        let Some(path) = parts.next() else { continue };
        let Some(line_no) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let preview = parts.next().unwrap_or_default().trim().to_owned();
        if seen < options.skip {
            seen += 1;
            continue;
        }
        if hits.len() >= options.limit {
            truncated = true;
            break;
        }
        hits.push(SourceSearchHit {
            path: path.replace('\\', "/"),
            line: line_no,
            preview,
        });
        seen += 1;
    }
    if truncated {
        let _ = child.kill();
    }
    let _ = child.wait();
    let next_cursor = if truncated {
        Some(page_end.to_string())
    } else {
        None
    };
    Ok((hits, truncated, next_cursor))
}

fn bool_opt(value: Option<bool>, default: bool) -> bool {
    value.unwrap_or(default)
}

fn source_search_options_from_replace(
    body: &SourceReplaceRequest,
) -> ApiResult<SourceSearchOptions> {
    let query = SourceSearchQuery {
        query: body.query.clone(),
        mode: body.mode.clone(),
        case_sensitive: body.case_sensitive.map(|v| v.to_string()),
        whole_word: body.whole_word.map(|v| v.to_string()),
        include: body.include.clone(),
        exclude: body.exclude.clone(),
        include_ignored: body.include_ignored.map(|v| v.to_string()),
        scope: body.scope.clone(),
        // Collect a wide hit page so we can discover unique paths.
        limit: Some(500),
        cursor: None,
    };
    let mut options = source_search_options_from_query(&query)?;
    options.case_sensitive = bool_opt(body.case_sensitive, true);
    options.whole_word = bool_opt(body.whole_word, false);
    options.include_ignored = bool_opt(body.include_ignored, false);
    Ok(options)
}

fn build_replace_regex(options: &SourceSearchOptions) -> ApiResult<regex::Regex> {
    let escaped = if options.regex {
        options.needle.clone()
    } else {
        regex::escape(&options.needle)
    };
    let pattern = if options.whole_word {
        format!(r"(?m)\b(?:{escaped})\b")
    } else {
        escaped
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .dot_matches_new_line(false)
        .build()
        .map_err(|err| {
            request_error(
                StatusCode::BAD_REQUEST,
                format!("invalid search pattern: {err}"),
            )
        })
}

fn apply_content_replace(
    content: &str,
    options: &SourceSearchOptions,
    replacement: &str,
) -> ApiResult<(String, u32)> {
    let re = build_replace_regex(options)?;
    let mut count = 0u32;
    let after = re
        .replace_all(content, |_: &regex::Captures| {
            count = count.saturating_add(1);
            replacement
        })
        .into_owned();
    Ok((after, count))
}

fn run_repository_replace_plan(
    root: &FsPath,
    options: &SourceSearchOptions,
    replacement: &str,
    file_limit: usize,
    path_filter: Option<&[String]>,
) -> ApiResult<(Vec<SourceReplaceFile>, bool)> {
    let search_opts = SourceSearchOptions {
        limit: 500,
        skip: 0,
        ..options.clone()
    };
    let (hits, search_truncated, _) = run_repository_search(root, &search_opts)?;
    let mut paths = Vec::new();
    for hit in &hits {
        if paths.iter().any(|path| path == &hit.path) {
            continue;
        }
        if let Some(filter) = path_filter
            && !filter.iter().any(|path| path == &hit.path)
        {
            continue;
        }
        paths.push(hit.path.clone());
    }
    paths.sort();
    let mut files = Vec::new();
    let mut truncated = search_truncated || paths.len() > file_limit;
    for path in paths.into_iter().take(file_limit) {
        let (resolved, clean) = resolve_source_path(root, &path)?;
        let bytes = std::fs::read(&resolved).map_err(|err| {
            request_error(
                StatusCode::NOT_FOUND,
                format!("could not read {clean}: {err}"),
            )
        })?;
        if bytes.len() > MAX_SOURCE_BYTES {
            truncated = true;
            continue;
        }
        let before = String::from_utf8(bytes).map_err(|_| {
            request_error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                format!("{clean} is not UTF-8 text and cannot be replaced"),
            )
        })?;
        let digest = source_digest(before.as_bytes());
        let (after, match_count) = apply_content_replace(&before, options, replacement)?;
        if match_count == 0 || after == before {
            continue;
        }
        files.push(SourceReplaceFile {
            path: clean,
            expected_digest: digest,
            match_count,
            before,
            after,
        });
    }
    Ok((files, truncated))
}

fn apply_repository_replace_plan(
    root: &FsPath,
    files: &[SourceReplaceFile],
    preconditions: &[SourceReplacePrecondition],
) -> ApiResult<()> {
    if files.is_empty() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "no replace edits to apply",
        ));
    }
    let mut expected = std::collections::HashMap::new();
    for precondition in preconditions {
        expected.insert(
            precondition.path.replace('\\', "/"),
            precondition.expected_digest.clone(),
        );
    }
    let mut snapshots = Vec::new();
    for file in files {
        let Some(want) = expected.get(&file.path) else {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                format!("replace is missing a digest precondition for {}", file.path),
            ));
        };
        if want != &file.expected_digest {
            return Err(request_error(
                StatusCode::CONFLICT,
                format!("{} changed since the replace preview was built", file.path),
            ));
        }
        let (resolved, _) = resolve_source_path(root, &file.path)?;
        let current = std::fs::read(&resolved).map_err(|err| {
            request_error(
                StatusCode::NOT_FOUND,
                format!("could not read {}: {err}", file.path),
            )
        })?;
        if source_digest(&current) != file.expected_digest {
            return Err(request_error(
                StatusCode::CONFLICT,
                format!("{} changed since the replace preview was built", file.path),
            ));
        }
        snapshots.push((resolved, current));
    }
    for (index, file) in files.iter().enumerate() {
        let path = &snapshots[index].0;
        if let Err(err) = std::fs::write(path, file.after.as_bytes()) {
            for (prior, bytes) in snapshots.iter().take(index) {
                let _ = std::fs::write(prior, bytes);
            }
            return Err(request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not apply replace to {}: {err}", file.path),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodeWorkspaceTabState {
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    draft: Option<String>,
    source_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CodeWorkspaceLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context_panel: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    terminal: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    tests: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    search: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    changes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_task: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CodeWorkspaceState {
    #[serde(default)]
    tabs: Vec<CodeWorkspaceTabState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary_path: Option<String>,
    /// Contextual Code regions (Problems / Terminal / Tests). Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    layout: Option<CodeWorkspaceLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SaveWorkspaceStateRequest {
    #[serde(flatten)]
    state: CodeWorkspaceState,
    #[serde(default)]
    lease_id: Option<String>,
    #[serde(default)]
    generation: Option<u64>,
}

const MAX_SOURCE_TREE_FILES: usize = 20_000;

fn list_source_tree(work_id: &WorkId, root: &FsPath) -> ApiResult<SourceTreeResponse> {
    let root = std::fs::canonicalize(root).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("governed workspace is unavailable: {err}"),
        )
    })?;
    let output = background_command("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(&root)
        .output()
        .map_err(|err| {
            request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not enumerate repository files: {err}"),
            )
        })?;
    if !output.status.success() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let statuses = repository_statuses(&root);
    let mut files = Vec::new();
    let mut truncated = false;
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        if files.len() >= MAX_SOURCE_TREE_FILES {
            truncated = true;
            break;
        }
        let relative = String::from_utf8_lossy(raw).replace('\\', "/");
        // Root is already canonical — avoid re-canonicalizing it per file.
        let Ok((path, clean)) = resolve_source_path_under(&root, &relative) else {
            continue;
        };
        let byte_size = path.metadata().map(|value| value.len()).unwrap_or(0);
        files.push(SourceTreeFile {
            status: statuses.get(&clean).cloned(),
            path: clean,
            byte_size,
        });
    }
    files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    Ok(SourceTreeResponse {
        work_id: work_id.as_str().to_owned(),
        files,
        truncated,
    })
}

fn repository_statuses(root: &FsPath) -> std::collections::HashMap<String, String> {
    let Ok(output) = background_command("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(root)
        .output()
    else {
        return std::collections::HashMap::new();
    };
    if !output.status.success() {
        return std::collections::HashMap::new();
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            if entry.len() < 4 || entry[2] != b' ' {
                return None;
            }
            let status = String::from_utf8_lossy(&entry[..2]).trim().to_owned();
            let path = String::from_utf8_lossy(&entry[3..]).replace('\\', "/");
            (!status.is_empty() && !path.is_empty()).then_some((path, status))
        })
        .collect()
}

fn looks_like_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(BINARY_SCAN_BYTES)];
    if sample.contains(&0) {
        return true;
    }
    if std::str::from_utf8(sample).is_ok() {
        return false;
    }
    let non_text = sample
        .iter()
        .filter(|byte| {
            let value = **byte;
            !(value == b'\n'
                || value == b'\r'
                || value == b'\t'
                || (0x20..0x7f).contains(&value)
                || value >= 0x80)
        })
        .count();
    non_text * 10 > sample.len()
}

fn format_binary_preview(bytes: &[u8], byte_size: usize, digest: &str) -> String {
    let sample = &bytes[..bytes.len().min(MAX_BINARY_PREVIEW_BYTES)];
    let mut out = format!(
        "Binary file · {byte_size} bytes · {digest}\nPreview (first {} bytes):\n",
        sample.len()
    );
    for (row, chunk) in sample.chunks(16).enumerate() {
        let offset = row * 16;
        let hex = chunk
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii = chunk
            .iter()
            .map(|byte| {
                if (0x20..0x7f).contains(byte) {
                    *byte as char
                } else {
                    '.'
                }
            })
            .collect::<String>();
        out.push_str(&format!("{offset:08x}  {hex:<47}  |{ascii}|\n"));
    }
    out
}

fn read_source_response(work_id: &WorkId, root: &FsPath, raw: &str) -> ApiResult<SourceResponse> {
    use std::io::Read;

    let (path, relative) = resolve_source_path(root, raw)?;
    let mut file = std::fs::File::open(&path).map_err(|err| {
        request_error(
            StatusCode::NOT_FOUND,
            format!("could not read source file: {err}"),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut prefix = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut byte_size = 0usize;
    loop {
        let read = file.read(&mut buffer).map_err(|err| {
            request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not read source file: {err}"),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        if prefix.len() < MAX_SOURCE_BYTES {
            let take = (MAX_SOURCE_BYTES - prefix.len()).min(read);
            prefix.extend_from_slice(&buffer[..take]);
        }
        byte_size = byte_size.saturating_add(read);
    }
    let digest = format!("sha256:{:x}", hasher.finalize());
    if looks_like_binary(&prefix) {
        return Ok(SourceResponse {
            work_id: work_id.as_str().to_owned(),
            path: relative,
            content: format_binary_preview(&prefix, byte_size, &digest),
            digest,
            byte_size,
            encoding: Some("binary".into()),
            preview: true,
            truncated: byte_size > MAX_BINARY_PREVIEW_BYTES,
        });
    }
    let truncated = byte_size > prefix.len();
    let (content, encoding, lossy) = match String::from_utf8(prefix) {
        Ok(text) => (text, "utf-8", false),
        Err(err) => (
            String::from_utf8_lossy(&err.into_bytes()).into_owned(),
            "utf-8-lossy",
            true,
        ),
    };
    Ok(SourceResponse {
        work_id: work_id.as_str().to_owned(),
        path: relative,
        content,
        digest,
        byte_size,
        encoding: Some(encoding.into()),
        preview: truncated || lossy,
        truncated,
    })
}

async fn read_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<SourceQuery>,
) -> ApiResult<Json<SourceResponse>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(
                        StatusCode::CONFLICT,
                        "prepare the governed workspace before opening source files",
                    )
                })?;
                Ok(Json(read_source_response(
                    &id,
                    &environment.worktree,
                    &query.path,
                )?))
            }
        },
    )
    .await
}

fn require_work_lease(
    state: &AppState,
    work_id: &WorkId,
    lease_id: &str,
    generation: u64,
) -> ApiResult<(WorkItem, ExecutionLease)> {
    let lease = resolve_lease(forge(state).as_ref(), lease_id.trim(), generation)?;
    if &lease.work_id != work_id {
        return Err(request_error(
            StatusCode::CONFLICT,
            "the presented lease belongs to a different undertaking",
        ));
    }
    let item = forge(state).load(work_id).map_err(map_err)?;
    Ok((item, lease))
}

async fn create_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<CreateSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let kind = body
                    .kind
                    .as_deref()
                    .unwrap_or("file")
                    .trim()
                    .to_ascii_lowercase();
                let is_directory = matches!(kind.as_str(), "directory" | "dir" | "folder");
                if !is_directory && body.content.len() > MAX_SOURCE_BYTES {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("source file exceeds the {MAX_SOURCE_BYTES} byte editor limit"),
                    ));
                }
                let (item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                if is_directory {
                    let (path, clean) =
                        resolve_new_directory_path(&environment.worktree, &body.path)?;
                    std::fs::create_dir_all(&path).map_err(|err| {
                        request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not create directory: {err}"),
                        )
                    })?;
                    // Seed an ignored placeholder so the folder appears in the file tree
                    // and remains durable under git until real files are added.
                    let keep = path.join(".gitkeep");
                    if !keep.exists() {
                        std::fs::write(&keep, b"").map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not initialize directory: {err}"),
                            )
                        })?;
                    }
                    let keep_path = format!("{clean}/.gitkeep");
                    publish_project_change(
                        &state,
                        &item,
                        ForgeProjectEventKind::Created,
                        Some(keep_path.clone()),
                        None,
                        Some(source_digest(b"")),
                    );
                    remember_worktree(&state, &item, &environment.worktree);
                    return Ok(Json(read_source_response(
                        &id,
                        &environment.worktree,
                        &keep_path,
                    )?));
                }
                let (path, _) = resolve_new_source_path(&environment.worktree, &body.path)?;
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|err| {
                        request_error(
                            StatusCode::CONFLICT,
                            format!("could not create source file: {err}"),
                        )
                    })?;
                if let Err(err) = file.write_all(body.content.as_bytes()) {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not initialize source file: {err}"),
                    ));
                }
                publish_project_change(
                    &state,
                    &item,
                    ForgeProjectEventKind::Created,
                    Some(body.path.clone()),
                    None,
                    Some(source_digest(body.content.as_bytes())),
                );
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(read_source_response(
                    &id,
                    &environment.worktree,
                    &body.path,
                )?))
            }
        },
    )
    .await
}

async fn source_tree(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<SourceTreeResponse>> {
    let id = parse_work_id(&work_id)?;
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let item = forge(&state).load(&id).map_err(map_err)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(
                        StatusCode::CONFLICT,
                        "prepare the governed workspace before browsing source files",
                    )
                })?;
                Ok(Json(list_source_tree(&id, &environment.worktree)?))
            }
        },
    )
    .await
}

async fn search_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<SourceSearchQuery>,
) -> ApiResult<Json<SourceSearchResponse>> {
    let id = parse_work_id(&work_id)?;
    let options = source_search_options_from_query(&query)?;
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            move || {
                let item = forge(&state).load(&id).map_err(map_err)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(
                        StatusCode::CONFLICT,
                        "prepare the governed workspace before searching source files",
                    )
                })?;
                let root = std::fs::canonicalize(&environment.worktree).map_err(|err| {
                    request_error(
                        StatusCode::CONFLICT,
                        format!("governed workspace is unavailable: {err}"),
                    )
                })?;
                let (hits, truncated, next_cursor) = run_repository_search(&root, &options)?;
                Ok(Json(SourceSearchResponse {
                    work_id: id.as_str().to_owned(),
                    hits,
                    truncated,
                    next_cursor,
                }))
            }
        },
    )
    .await
}

async fn replace_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SourceReplaceRequest>,
) -> ApiResult<Json<SourceReplaceResponse>> {
    let id = parse_work_id(&work_id)?;
    if body.replacement.len() > 8_000 {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "replacement text must be at most 8000 characters",
        ));
    }
    let options = source_search_options_from_replace(&body)?;
    let dry_run = body.dry_run.unwrap_or(true);
    let file_limit = body
        .limit
        .unwrap_or(DEFAULT_REPLACE_FILES as u32)
        .clamp(1, MAX_REPLACE_FILES as u32) as usize;
    let path_filter = body.paths.as_ref().map(|paths| {
        paths
            .iter()
            .map(|path| path.replace('\\', "/"))
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>()
    });

    if dry_run {
        let replacement = body.replacement.clone();
        let filter = path_filter.clone();
        let state_for_run = state.clone();
        let id_for_run = id.clone();
        let (files, truncated) = admit_forge(
            &state,
            medousa_forge::execution::ExecutionClass::LocalMutation,
            256 * 1024,
            move || {
                let item = forge(&state_for_run).load(&id_for_run).map_err(map_err)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(
                        StatusCode::CONFLICT,
                        "prepare the governed workspace before replacing source files",
                    )
                })?;
                let root = std::fs::canonicalize(&environment.worktree).map_err(|err| {
                    request_error(
                        StatusCode::CONFLICT,
                        format!("governed workspace is unavailable: {err}"),
                    )
                })?;
                run_repository_replace_plan(
                    &root,
                    &options,
                    &replacement,
                    file_limit,
                    filter.as_deref(),
                )
            },
        )
        .await?;
        return Ok(Json(SourceReplaceResponse {
            work_id: id.as_str().to_owned(),
            files,
            truncated,
            applied: false,
        }));
    }

    let lease_id = body.lease_id.as_deref().unwrap_or("").trim();
    let generation = body.generation.ok_or_else(|| {
        request_error(
            StatusCode::BAD_REQUEST,
            "generation is required when applying a replace",
        )
    })?;
    if lease_id.is_empty() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "lease_id is required when applying a replace",
        ));
    }
    let preconditions = body.preconditions.clone().unwrap_or_default();
    if preconditions.is_empty() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "preconditions are required when applying a replace",
        ));
    }
    let lease_id = lease_id.to_owned();
    let replacement = body.replacement.clone();
    let filter = path_filter.clone();
    let state_for_run = state.clone();
    let id_for_run = id.clone();
    let (item, environment, files, truncated) = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        move || {
            let (item, lease) =
                require_work_lease(&state_for_run, &id_for_run, &lease_id, generation)?;
            let environment = item
                .environment_for_attempt(&lease.attempt_id)
                .ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?
                .clone();
            let root = std::fs::canonicalize(&environment.worktree).map_err(|err| {
                request_error(
                    StatusCode::CONFLICT,
                    format!("governed workspace is unavailable: {err}"),
                )
            })?;
            let (files, truncated) = run_repository_replace_plan(
                &root,
                &options,
                &replacement,
                file_limit,
                filter.as_deref(),
            )?;
            apply_repository_replace_plan(&root, &files, &preconditions)?;
            Ok((item, environment, files, truncated))
        },
    )
    .await?;
    for file in &files {
        publish_project_change(
            &state,
            &item,
            ForgeProjectEventKind::Changed,
            Some(file.path.clone()),
            None,
            Some(source_digest(file.after.as_bytes())),
        );
    }
    remember_worktree(&state, &item, &environment.worktree);
    publish_item(&state, &item, "source_replace_applied");
    Ok(Json(SourceReplaceResponse {
        work_id: id.as_str().to_owned(),
        files,
        truncated,
        applied: true,
    }))
}

fn code_workspace_state_path(forge: &Forge, work_id: &WorkId) -> PathBuf {
    forge
        .store()
        .item_dir(work_id)
        .join("ui")
        .join("code-workspace.json")
}

async fn read_workspace_state(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<CodeWorkspaceState>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                forge(&state).load(&id).map_err(map_err)?;
                let path = code_workspace_state_path(forge(&state).as_ref(), &id);
                if !path.exists() {
                    return Ok(Json(CodeWorkspaceState::default()));
                }
                let raw = std::fs::read_to_string(&path).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not read Code workspace state: {err}"),
                    )
                })?;
                let value = serde_json::from_str(&raw).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not parse Code workspace state: {err}"),
                    )
                })?;
                Ok(Json(value))
            }
        },
    )
    .await
}

async fn save_workspace_state(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(mut body): Json<SaveWorkspaceStateRequest>,
) -> ApiResult<Json<CodeWorkspaceState>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                if body.state.tabs.len() > 32 {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Code workspace state may contain at most 32 open files",
                    ));
                }
                let draft_bytes = body
                    .state
                    .tabs
                    .iter()
                    .filter_map(|tab| tab.draft.as_ref())
                    .map(String::len)
                    .sum::<usize>();
                if draft_bytes > 8 * 1024 * 1024 {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Code workspace drafts exceed the 8 MiB recovery limit",
                    ));
                }
                let draft_lease = if body.state.tabs.iter().any(|tab| tab.draft.is_some()) {
                    let lease_id = body.lease_id.as_deref().ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "an active lease is required to persist source drafts",
                        )
                    })?;
                    let generation = body.generation.ok_or_else(|| {
                        request_error(StatusCode::CONFLICT, "lease generation is required")
                    })?;
                    let lease = resolve_lease(forge(&state).as_ref(), lease_id, generation)?;
                    if lease.work_id != id {
                        return Err(request_error(
                            StatusCode::CONFLICT,
                            "the presented lease belongs to a different undertaking",
                        ));
                    }
                    Some(lease)
                } else {
                    None
                };
                let environment = draft_lease
                    .as_ref()
                    .and_then(|lease| item.environment_for_attempt(&lease.attempt_id))
                    .or_else(|| item.workspace_environment())
                    .ok_or_else(|| {
                        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                    })?;
                for tab in &mut body.state.tabs {
                    let (_, clean) = resolve_source_path(&environment.worktree, &tab.path)?;
                    tab.path = clean;
                    if tab
                        .draft
                        .as_ref()
                        .is_some_and(|draft| draft.len() > MAX_SOURCE_BYTES)
                    {
                        return Err(request_error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            format!("draft for {} exceeds the editor limit", tab.path),
                        ));
                    }
                }
                if let Some(active_path) = body.state.active_path.as_mut() {
                    let (_, clean) = resolve_source_path(&environment.worktree, active_path)?;
                    *active_path = clean;
                }
                if let Some(secondary_path) = body.state.secondary_path.as_mut() {
                    let (_, clean) = resolve_source_path(&environment.worktree, secondary_path)?;
                    *secondary_path = clean;
                }
                if let Some(layout) = body.state.layout.as_mut()
                    && let Some(panel) = layout.context_panel.as_deref()
                {
                    match panel {
                        "problems" | "outline" | "references" | "language" => {}
                        _ => layout.context_panel = None,
                    }
                }
                if let Some(layout) = body.state.layout.as_mut()
                    && let Some(task_id) = layout.primary_task.as_mut()
                {
                    *task_id = task_id.trim().chars().take(160).collect();
                    if task_id.is_empty() {
                        layout.primary_task = None;
                    }
                }
                let open_paths = body
                    .state
                    .tabs
                    .iter()
                    .map(|tab| tab.path.as_str())
                    .collect::<std::collections::HashSet<_>>();
                if body
                    .state
                    .active_path
                    .as_deref()
                    .is_some_and(|path| !open_paths.contains(path))
                    || body
                        .state
                        .secondary_path
                        .as_deref()
                        .is_some_and(|path| !open_paths.contains(path))
                {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "Code workspace groups must reference open files",
                    ));
                }
                body.state.updated_at = Some(chrono::Utc::now().to_rfc3339());
                let path = code_workspace_state_path(forge(&state).as_ref(), &id);
                let parent = path.parent().expect("Code workspace state has a parent");
                std::fs::create_dir_all(parent).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not create Code workspace state directory: {err}"),
                    )
                })?;
                let bytes = serde_json::to_vec_pretty(&body.state).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not serialize Code workspace state: {err}"),
                    )
                })?;
                crate::session::atomic_write(&path, &bytes).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not preserve Code workspace state: {err}"),
                    )
                })?;
                Ok(Json(body.state))
            }
        },
    )
    .await
}

async fn save_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SaveSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                if body.content.len() > MAX_SOURCE_BYTES {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("source file exceeds the {MAX_SOURCE_BYTES} byte editor limit"),
                    ));
                }
                let (item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                let (path, _) = resolve_source_path(&environment.worktree, &body.path)?;
                let current = std::fs::read(&path).map_err(|err| {
                    request_error(
                        StatusCode::NOT_FOUND,
                        format!("could not read source file: {err}"),
                    )
                })?;
                if source_digest(&current) != body.expected_digest {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "source changed since it was opened; reload before saving",
                    ));
                }
                std::fs::write(&path, body.content.as_bytes()).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not save source file: {err}"),
                    )
                })?;
                publish_project_change(
                    &state,
                    &item,
                    ForgeProjectEventKind::Changed,
                    Some(body.path.clone()),
                    None,
                    Some(source_digest(body.content.as_bytes())),
                );
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(read_source_response(
                    &id,
                    &environment.worktree,
                    &body.path,
                )?))
            }
        },
    )
    .await
}

async fn save_source_batch(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SaveSourceBatchRequest>,
) -> ApiResult<Json<Vec<SourceResponse>>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                if body.files.is_empty() {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "no source edits supplied",
                    ));
                }
                let (item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                let mut prepared = Vec::with_capacity(body.files.len());
                let mut seen = std::collections::HashSet::new();
                for file in &body.files {
                    if file.content.len() > MAX_SOURCE_BYTES {
                        return Err(request_error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            format!("{} exceeds the source editor limit", file.path),
                        ));
                    }
                    let (path, relative) = resolve_source_path(&environment.worktree, &file.path)?;
                    if !seen.insert(path.clone()) {
                        return Err(request_error(
                            StatusCode::BAD_REQUEST,
                            "duplicate source edit",
                        ));
                    }
                    let original = std::fs::read(&path).map_err(|err| {
                        request_error(
                            StatusCode::NOT_FOUND,
                            format!("could not read {}: {err}", file.path),
                        )
                    })?;
                    if source_digest(&original) != file.expected_digest {
                        return Err(request_error(
                            StatusCode::CONFLICT,
                            format!(
                                "{} changed; reload before applying the language edit",
                                file.path
                            ),
                        ));
                    }
                    prepared.push((path, relative, original, file.content.as_bytes().to_vec()));
                }
                for (written, (path, _, _, content)) in prepared.iter().enumerate() {
                    if let Err(err) = crate::session::atomic_write(path, content) {
                        for (rollback_path, _, original, _) in prepared.iter().take(written) {
                            let _ = crate::session::atomic_write(rollback_path, original);
                        }
                        return Err(request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not apply language edit: {err}"),
                        ));
                    }
                }
                publish_item(&state, &item, "source_batch_saved");
                remember_worktree(&state, &item, &environment.worktree);
                for (_, relative, _, content) in &prepared {
                    state.forge_events.publish_project(
                        item.id.as_str(),
                        ForgeProjectEventKind::Changed,
                        Some(relative.clone()),
                        None,
                        Some(source_digest(content)),
                    );
                }
                let responses = prepared
                    .iter()
                    .map(|(_, relative, _, _)| {
                        read_source_response(&id, &environment.worktree, relative)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Json(responses))
            }
        },
    )
    .await
}

const MAX_SOURCE_WORKSPACE_EDIT_OPERATIONS: usize = 512;
const MAX_SOURCE_WORKSPACE_EDIT_BYTES: usize = 8 * 1024 * 1024;
// JSON escaping can expand valid source text well beyond its decoded size.
const MAX_SOURCE_WORKSPACE_EDIT_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
struct SourceWorkspaceSnapshot {
    path: PathBuf,
    original: Option<Vec<u8>>,
}

#[derive(Debug)]
enum PreparedSourceWorkspaceOperation {
    Write { path: String, content: Vec<u8> },
    Create { path: String, content: Vec<u8> },
    Rename { path: String, destination: String },
    Delete { path: String },
}

#[derive(Debug)]
struct PreparedSourceWorkspaceEdit {
    snapshots: std::collections::BTreeMap<String, SourceWorkspaceSnapshot>,
    operations: Vec<PreparedSourceWorkspaceOperation>,
    final_paths: std::collections::BTreeSet<String>,
}

fn prepare_source_workspace_edit(
    root: &FsPath,
    body: &SourceWorkspaceEditRequest,
) -> ApiResult<PreparedSourceWorkspaceEdit> {
    if body.operations.is_empty() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "no workspace edit operations supplied",
        ));
    }
    if body.operations.len() > MAX_SOURCE_WORKSPACE_EDIT_OPERATIONS
        || body.preconditions.len() > MAX_SOURCE_WORKSPACE_EDIT_OPERATIONS * 2
    {
        return Err(request_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "workspace edit contains too many operations",
        ));
    }

    let mut snapshots = std::collections::BTreeMap::new();
    let mut absolute_paths = std::collections::HashSet::new();
    let mut snapshot_bytes = 0usize;
    for precondition in &body.preconditions {
        let (path, clean, original) = match precondition {
            SourceWorkspacePrecondition::Existing {
                path,
                expected_digest,
            } => {
                let (resolved, clean) = resolve_source_path(root, path)?;
                let original = std::fs::read(&resolved).map_err(|err| {
                    request_error(
                        StatusCode::NOT_FOUND,
                        format!("could not read {clean}: {err}"),
                    )
                })?;
                if original.len() > MAX_SOURCE_BYTES {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("{clean} exceeds the source editor limit"),
                    ));
                }
                snapshot_bytes = snapshot_bytes.saturating_add(original.len());
                if snapshot_bytes > MAX_SOURCE_WORKSPACE_EDIT_BYTES {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "workspace edit snapshots exceed the combined source editor limit",
                    ));
                }
                if source_digest(&original) != *expected_digest {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        format!("{clean} changed; review the refactor again before applying"),
                    ));
                }
                (resolved, clean, Some(original))
            }
            SourceWorkspacePrecondition::Missing { path } => {
                let (resolved, clean) = resolve_new_source_path(root, path)?;
                (resolved, clean, None)
            }
        };
        if snapshots.contains_key(&clean) || !absolute_paths.insert(path.clone()) {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                "duplicate workspace edit precondition",
            ));
        }
        snapshots.insert(clean, SourceWorkspaceSnapshot { path, original });
    }

    let mut virtual_exists = snapshots
        .iter()
        .map(|(path, snapshot)| (path.clone(), snapshot.original.is_some()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut prepared = Vec::with_capacity(body.operations.len());
    let mut touched = std::collections::BTreeSet::new();
    let mut content_bytes = 0usize;
    let normalize_known = |raw: &str| -> ApiResult<String> {
        let (_, clean) = normalize_source_relative(raw)?;
        if !snapshots.contains_key(&clean) {
            return Err(request_error(
                StatusCode::BAD_REQUEST,
                format!("workspace edit is missing a precondition for {clean}"),
            ));
        }
        Ok(clean)
    };
    let require_state =
        |states: &std::collections::BTreeMap<String, bool>, path: &str, exists: bool| {
            if states.get(path).copied() == Some(exists) {
                Ok(())
            } else {
                Err(request_error(
                    StatusCode::CONFLICT,
                    format!(
                        "workspace edit expected {path} to {}",
                        if exists { "exist" } else { "be absent" }
                    ),
                ))
            }
        };

    for operation in &body.operations {
        match operation {
            SourceWorkspaceOperation::Write { path, content } => {
                let path = normalize_known(path)?;
                require_state(&virtual_exists, &path, true)?;
                if content.len() > MAX_SOURCE_BYTES {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("{path} exceeds the source editor limit"),
                    ));
                }
                content_bytes = content_bytes.saturating_add(content.len());
                touched.insert(path.clone());
                prepared.push(PreparedSourceWorkspaceOperation::Write {
                    path,
                    content: content.as_bytes().to_vec(),
                });
            }
            SourceWorkspaceOperation::Create { path, content } => {
                let path = normalize_known(path)?;
                require_state(&virtual_exists, &path, false)?;
                if content.len() > MAX_SOURCE_BYTES {
                    return Err(request_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("{path} exceeds the source editor limit"),
                    ));
                }
                content_bytes = content_bytes.saturating_add(content.len());
                virtual_exists.insert(path.clone(), true);
                touched.insert(path.clone());
                prepared.push(PreparedSourceWorkspaceOperation::Create {
                    path,
                    content: content.as_bytes().to_vec(),
                });
            }
            SourceWorkspaceOperation::Rename { path, destination } => {
                let path = normalize_known(path)?;
                let destination = normalize_known(destination)?;
                require_state(&virtual_exists, &path, true)?;
                require_state(&virtual_exists, &destination, false)?;
                virtual_exists.insert(path.clone(), false);
                virtual_exists.insert(destination.clone(), true);
                touched.insert(path.clone());
                touched.insert(destination.clone());
                prepared.push(PreparedSourceWorkspaceOperation::Rename { path, destination });
            }
            SourceWorkspaceOperation::Delete { path } => {
                let path = normalize_known(path)?;
                require_state(&virtual_exists, &path, true)?;
                virtual_exists.insert(path.clone(), false);
                touched.insert(path.clone());
                prepared.push(PreparedSourceWorkspaceOperation::Delete { path });
            }
        }
    }
    if content_bytes > MAX_SOURCE_WORKSPACE_EDIT_BYTES {
        return Err(request_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "workspace edit content exceeds the combined source editor limit",
        ));
    }
    if snapshots.keys().any(|path| !touched.contains(path)) {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "workspace edit contains an unused precondition",
        ));
    }
    let final_paths = virtual_exists
        .into_iter()
        .filter_map(|(path, exists)| (exists && touched.contains(&path)).then_some(path))
        .collect();
    Ok(PreparedSourceWorkspaceEdit {
        snapshots,
        operations: prepared,
        final_paths,
    })
}

fn rollback_source_workspace_edit(
    snapshots: &std::collections::BTreeMap<String, SourceWorkspaceSnapshot>,
) {
    for snapshot in snapshots.values() {
        if snapshot.path.is_file() {
            let _ = std::fs::remove_file(&snapshot.path);
        }
    }
    for snapshot in snapshots.values() {
        if let Some(original) = snapshot.original.as_ref() {
            let _ = crate::session::atomic_write(&snapshot.path, original);
        }
    }
}

fn execute_source_workspace_edit(
    work_id: &WorkId,
    root: &FsPath,
    body: &SourceWorkspaceEditRequest,
) -> ApiResult<Vec<SourceResponse>> {
    let prepared = prepare_source_workspace_edit(root, body)?;
    for operation in &prepared.operations {
        let result = match operation {
            PreparedSourceWorkspaceOperation::Write { path, content } => {
                crate::session::atomic_write(&prepared.snapshots[path].path, content)
            }
            PreparedSourceWorkspaceOperation::Create { path, content } => {
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&prepared.snapshots[path].path)
                    .and_then(|mut file| file.write_all(content))
            }
            PreparedSourceWorkspaceOperation::Rename { path, destination } => std::fs::rename(
                &prepared.snapshots[path].path,
                &prepared.snapshots[destination].path,
            ),
            PreparedSourceWorkspaceOperation::Delete { path } => {
                std::fs::remove_file(&prepared.snapshots[path].path)
            }
        };
        if let Err(err) = result {
            rollback_source_workspace_edit(&prepared.snapshots);
            return Err(request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not apply workspace edit: {err}"),
            ));
        }
    }

    let mut responses = Vec::with_capacity(prepared.final_paths.len());
    for path in &prepared.final_paths {
        match read_source_response(work_id, root, path) {
            Ok(response) => responses.push(response),
            Err(err) => {
                rollback_source_workspace_edit(&prepared.snapshots);
                return Err(err);
            }
        }
    }
    Ok(responses)
}

async fn apply_source_workspace_edit(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SourceWorkspaceEditRequest>,
) -> ApiResult<Json<Vec<SourceResponse>>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let (item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                let responses = execute_source_workspace_edit(&id, &environment.worktree, &body)?;
                publish_item(&state, &item, "source_workspace_edit_applied");
                remember_worktree(&state, &item, &environment.worktree);
                for response in &responses {
                    state.forge_events.publish_project(
                        item.id.as_str(),
                        ForgeProjectEventKind::Changed,
                        Some(response.path.clone()),
                        None,
                        Some(response.digest.clone()),
                    );
                }
                Ok(Json(responses))
            }
        },
    )
    .await
}

async fn rename_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RenameSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let (item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                let (source, _) = resolve_source_path(&environment.worktree, &body.path)?;
                let current = std::fs::read(&source).map_err(|err| {
                    request_error(
                        StatusCode::NOT_FOUND,
                        format!("could not read source file: {err}"),
                    )
                })?;
                if source_digest(&current) != body.expected_digest {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "source changed since it was opened; reload before renaming",
                    ));
                }
                let (destination, _) =
                    resolve_new_source_path(&environment.worktree, &body.destination)?;
                std::fs::rename(&source, &destination).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not rename source file: {err}"),
                    )
                })?;
                let response = read_source_response(&id, &environment.worktree, &body.destination)?;
                publish_project_change(
                    &state,
                    &item,
                    ForgeProjectEventKind::Renamed,
                    Some(body.destination.clone()),
                    Some(body.path.clone()),
                    Some(response.digest.clone()),
                );
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(response))
            }
        },
    )
    .await
}

async fn delete_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<DeleteSourceRequest>,
) -> ApiResult<Json<DeleteSourceResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let (item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                let (path, relative) = resolve_source_path(&environment.worktree, &body.path)?;
                let current = std::fs::read(&path).map_err(|err| {
                    request_error(
                        StatusCode::NOT_FOUND,
                        format!("could not read source file: {err}"),
                    )
                })?;
                if source_digest(&current) != body.expected_digest {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "source changed since it was opened; reload before deleting",
                    ));
                }
                std::fs::remove_file(&path).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not delete source file: {err}"),
                    )
                })?;
                publish_project_change(
                    &state,
                    &item,
                    ForgeProjectEventKind::Deleted,
                    Some(relative.clone()),
                    None,
                    None,
                );
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(DeleteSourceResponse {
                    work_id: id.as_str().to_owned(),
                    path: relative,
                    deleted: true,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Clone, Serialize)]
struct ForgeChangesFile {
    path: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ForgeChangesResponse {
    work_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    detached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ahead: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    behind: Option<u64>,
    conflict: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    dirty: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    merge_in_progress: bool,
    files: Vec<ForgeChangesFile>,
}

fn porcelain_change_status(entry: &medousa_forge::git::PorcelainEntry) -> Option<&'static str> {
    use medousa_forge::git::PorcelainKind;
    match entry.kind {
        PorcelainKind::Ignored => None,
        PorcelainKind::Untracked => Some("untracked"),
        PorcelainKind::Unmerged => Some("unmerged"),
        PorcelainKind::RenameOrCopy => {
            if entry.xy.as_deref().unwrap_or_default().starts_with('C') {
                Some("copied")
            } else {
                Some("renamed")
            }
        }
        PorcelainKind::Ordinary => {
            let xy = entry.xy.as_deref().unwrap_or_default();
            if xy.contains('A') {
                Some("added")
            } else if xy.contains('D') {
                Some("deleted")
            } else if xy.contains('T') {
                Some("type_changed")
            } else {
                Some("modified")
            }
        }
    }
}

fn build_changes_response(state: &AppState, work_id: &WorkId) -> ApiResult<ForgeChangesResponse> {
    let forge = forge(state);
    let item = forge.load(work_id).map_err(map_err)?;
    let environment = item.workspace_environment().cloned().ok_or_else(|| {
        request_error(
            StatusCode::CONFLICT,
            "prepare the governed workspace before reading changes",
        )
    })?;
    let (tracking, entries) = forge
        .git()
        .status_porcelain_with_branch(&environment.worktree)
        .map_err(map_err)?;
    let mut files = Vec::new();
    let mut conflict = false;
    for entry in entries {
        let Some(status) = porcelain_change_status(&entry) else {
            continue;
        };
        let path = medousa_forge::policy::normalize_git_path(&entry.path);
        if medousa_forge::policy::is_git_internal(&path) {
            continue;
        }
        if status == "unmerged" {
            conflict = true;
        }
        files.push(ForgeChangesFile {
            path,
            status: status.to_owned(),
            old_path: entry
                .orig_path
                .map(|p| medousa_forge::policy::normalize_git_path(&p)),
        });
    }
    files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    let WorkTarget::Git(target) = &item.target;
    Ok(ForgeChangesResponse {
        work_id: work_id.as_str().to_owned(),
        branch: tracking.head.or_else(|| Some(environment.branch.clone())),
        detached: tracking.detached,
        base_ref: Some(target.base_ref.clone()),
        baseline_oid: Some(environment.baseline_oid.as_str().to_owned()),
        upstream: tracking.upstream,
        ahead: tracking.ahead,
        behind: tracking.behind,
        conflict,
        dirty: !files.is_empty() || conflict,
        merge_in_progress: forge.git().merge_in_progress(&environment.worktree),
        files,
    })
}

async fn get_changes(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ForgeChangesResponse>> {
    admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                Ok(Json(build_changes_response(&state, &id)?))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ChangesFileQuery {
    path: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChangesFileDiff {
    work_id: String,
    path: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    baseline_oid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_digest: Option<String>,
    binary: bool,
    conflict: bool,
    baseline: ReviewFileVersion,
    working: ReviewFileVersion,
    hunks: Vec<ReviewDiffHunk>,
    truncated: bool,
}

fn changes_file_diff(state: &AppState, id: &WorkId, raw_path: &str) -> ApiResult<ChangesFileDiff> {
    const MAX_CHANGES_FILE_BYTES: usize = 1024 * 1024;
    let (_, path) = normalize_source_relative(raw_path)?;
    let forge = forge(state);
    let item = forge.load(id).map_err(map_err)?;
    let environment = item.workspace_environment().cloned().ok_or_else(|| {
        request_error(
            StatusCode::CONFLICT,
            "prepare the governed workspace before reading changes",
        )
    })?;
    let baseline_oid = environment.baseline_oid.clone();
    let entries = forge
        .git()
        .status_porcelain(&environment.worktree)
        .map_err(map_err)?;
    let entry = entries.iter().find(|entry| {
        medousa_forge::policy::normalize_git_path(&entry.path) == path
            || entry
                .orig_path
                .as_ref()
                .map(|old| medousa_forge::policy::normalize_git_path(old) == path)
                .unwrap_or(false)
    });
    let status = entry
        .and_then(porcelain_change_status)
        .unwrap_or("modified")
        .to_owned();
    let old_path = entry.and_then(|e| {
        e.orig_path
            .as_ref()
            .map(|p| medousa_forge::policy::normalize_git_path(p))
    });
    let conflict = matches!(status.as_str(), "unmerged");
    let baseline_path = old_path.as_deref().unwrap_or(path.as_str());
    let baseline_bytes = forge
        .git()
        .show_bytes(&environment.worktree, &baseline_oid, baseline_path)
        .ok();
    let work_abs = environment.worktree.join(&path);
    let working_bytes = if work_abs.is_file() {
        std::fs::read(&work_abs).ok()
    } else {
        None
    };
    let binary = baseline_bytes
        .as_ref()
        .is_some_and(|bytes| looks_like_binary(bytes))
        || working_bytes
            .as_ref()
            .is_some_and(|bytes| looks_like_binary(bytes));
    let truncated = baseline_bytes
        .as_ref()
        .is_some_and(|bytes| bytes.len() > MAX_CHANGES_FILE_BYTES)
        || working_bytes
            .as_ref()
            .is_some_and(|bytes| bytes.len() > MAX_CHANGES_FILE_BYTES);
    let version = |bytes: &Option<Vec<u8>>| ReviewFileVersion {
        exists: bytes.is_some(),
        binary,
        byte_size: bytes.as_ref().map(|value| value.len() as u64).unwrap_or(0),
        digest: bytes.as_ref().map(|value| source_digest(value)),
        content: if binary || truncated {
            None
        } else {
            bytes
                .as_ref()
                .and_then(|value| String::from_utf8(value.clone()).ok())
        },
    };
    let patch = if binary {
        Vec::new()
    } else if status == "untracked" || (baseline_bytes.is_none() && working_bytes.is_some()) {
        forge
            .git()
            .diff_untracked_path(&environment.worktree, &path)
            .unwrap_or_default()
    } else {
        forge
            .git()
            .diff_path_worktree(&environment.worktree, &baseline_oid, &path)
            .map_err(map_err)?
    };
    let hunks = parse_review_hunks(&String::from_utf8_lossy(&patch));
    Ok(ChangesFileDiff {
        work_id: id.as_str().to_owned(),
        path,
        status,
        old_path,
        baseline_oid: baseline_oid.as_str().to_owned(),
        working_digest: working_bytes.as_ref().map(|value| source_digest(value)),
        binary,
        conflict,
        baseline: version(&baseline_bytes),
        working: version(&working_bytes),
        hunks,
        truncated,
    })
}

async fn get_changes_file(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ChangesFileQuery>,
) -> ApiResult<Json<ChangesFileDiff>> {
    let id = parse_work_id(&work_id)?;
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            let path = query.path.clone();
            move || Ok(Json(changes_file_diff(&state, &id, &path)?))
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct RestoreChangesFileRequest {
    path: String,
    expected_working_digest: Option<String>,
    lease_id: String,
    generation: u64,
}

#[derive(Debug, Serialize)]
struct RestoreChangesFileResponse {
    work_id: String,
    path: String,
    action: String,
    digest: Option<String>,
}

async fn restore_changes_file(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RestoreChangesFileRequest>,
) -> ApiResult<Json<RestoreChangesFileResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let comparison = changes_file_diff(&state, &id, &body.path)?;
                if comparison.binary && comparison.baseline.exists {
                    return Err(request_error(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        "binary recovery is preserved in Git but cannot yet be restored from Home",
                    ));
                }
                let expected = body
                    .expected_working_digest
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let current = comparison.working_digest.as_deref();
                match (expected, current) {
                    (Some(want), Some(have)) if want != have => {
                        return Err(request_error(
                            StatusCode::CONFLICT,
                            "the working copy changed; refresh Changes before restoring",
                        ));
                    }
                    (Some(_), None) => {
                        return Err(request_error(
                            StatusCode::CONFLICT,
                            "the working copy changed; refresh Changes before restoring",
                        ));
                    }
                    (None, Some(_)) if comparison.working.exists => {
                        return Err(request_error(
                            StatusCode::CONFLICT,
                            "expected_working_digest is required to restore this file",
                        ));
                    }
                    _ => {}
                }
                let (item, _lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                let restore_path = comparison
                    .old_path
                    .as_deref()
                    .unwrap_or(&comparison.path)
                    .to_owned();
                if comparison.path != restore_path {
                    let (renamed, _) =
                        resolve_source_path(&environment.worktree, &comparison.path)?;
                    if renamed.is_file() {
                        std::fs::remove_file(&renamed).map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not restore file: {err}"),
                            )
                        })?;
                    }
                }
                let (action, digest) = if comparison.baseline.exists {
                    let content = comparison.baseline.content.clone().ok_or_else(|| {
                        request_error(
                            StatusCode::UNSUPPORTED_MEDIA_TYPE,
                            "baseline text is unavailable for restore",
                        )
                    })?;
                    let candidate = environment.worktree.join(&restore_path);
                    let (destination, relative) = if candidate.is_file() {
                        resolve_source_path(&environment.worktree, &restore_path)?
                    } else {
                        resolve_new_source_path(&environment.worktree, &restore_path)?
                    };
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent).map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not restore file: {err}"),
                            )
                        })?;
                    }
                    std::fs::write(&destination, content.as_bytes()).map_err(|err| {
                        request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not restore file: {err}"),
                        )
                    })?;
                    let digest = source_digest(content.as_bytes());
                    if comparison.conflict || comparison.status == "unmerged" {
                        let _ = forge(&state)
                            .git()
                            .add_path(&environment.worktree, &restore_path);
                    }
                    publish_project_change(
                        &state,
                        &item,
                        ForgeProjectEventKind::Changed,
                        Some(relative.clone()),
                        None,
                        Some(digest.clone()),
                    );
                    ("restored".into(), Some(digest))
                } else {
                    let (target, relative) =
                        resolve_source_path(&environment.worktree, &comparison.path)?;
                    if target.is_file() {
                        std::fs::remove_file(&target).map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not restore file: {err}"),
                            )
                        })?;
                    }
                    if comparison.conflict || comparison.status == "unmerged" {
                        let _ = forge(&state)
                            .git()
                            .add_path(&environment.worktree, &comparison.path);
                    }
                    publish_project_change(
                        &state,
                        &item,
                        ForgeProjectEventKind::Deleted,
                        Some(relative.clone()),
                        None,
                        None,
                    );
                    ("deleted".into(), None)
                };
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(RestoreChangesFileResponse {
                    work_id: id.as_str().to_owned(),
                    path: restore_path,
                    action,
                    digest,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ChangesLeaseRequest {
    lease_id: String,
    generation: u64,
    #[serde(default)]
    remote: Option<String>,
    #[serde(default)]
    ack_risks: bool,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    author_email: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChangesSyncResult {
    work_id: String,
    fetched: bool,
    pulled: bool,
    pushed: bool,
    message: String,
    changes: ForgeChangesResponse,
}

fn changes_sync_preflight(
    forge: &Forge,
    environment: &medousa_forge::model::GovernedEnv,
) -> ApiResult<()> {
    if forge.git().merge_in_progress(&environment.worktree) {
        return Err(request_error(
            StatusCode::CONFLICT,
            "finish or abort the in-progress merge/rebase before syncing",
        ));
    }
    let snapshot = forge
        .git()
        .status_porcelain(&environment.worktree)
        .map_err(map_err)?;
    if snapshot
        .iter()
        .any(|entry| entry.kind == medousa_forge::git::PorcelainKind::Unmerged)
    {
        return Err(request_error(
            StatusCode::CONFLICT,
            "resolve merge conflicts before pull/push/sync",
        ));
    }
    Ok(())
}

fn has_tracked_worktree_edits(
    forge: &Forge,
    environment: &medousa_forge::model::GovernedEnv,
) -> ApiResult<bool> {
    use medousa_forge::git::PorcelainKind;
    let snapshot = forge
        .git()
        .status_porcelain(&environment.worktree)
        .map_err(map_err)?;
    Ok(snapshot.iter().any(|entry| {
        matches!(
            entry.kind,
            PorcelainKind::Ordinary | PorcelainKind::RenameOrCopy | PorcelainKind::Unmerged
        )
    }))
}

async fn run_network_git(
    state: &AppState,
    worktree: &FsPath,
    args: Vec<String>,
) -> Result<String, ApiError> {
    let git = forge(state).git().binary().to_path_buf();
    let cwd = worktree.to_path_buf();
    let repo = cwd.display().to_string();
    state
        .forge_execution
        .run_async(
            medousa_forge::execution::ExecutionClass::NetworkGit,
            64 * 1024,
            Some(repo),
            async move {
                let (stdout, _stderr, truncated) = medousa_forge::execution::supervise_git(
                    git,
                    cwd,
                    args,
                    std::time::Duration::from_secs(120),
                    medousa_forge::execution::MAX_CAPTURE_BYTES,
                )
                .await?;
                let mut message = String::from_utf8_lossy(&stdout).into_owned();
                if truncated {
                    message.push_str("\n[git output truncated]");
                }
                Ok(message)
            },
        )
        .await
        .map_err(map_err)
}

async fn changes_fetch(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ChangesLeaseRequest>,
) -> ApiResult<Json<ChangesSyncResult>> {
    let id = parse_work_id(&work_id)?;
    let (item, environment, remote) = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        {
            let state = state.clone();
            let id = id.clone();
            let lease_id = body.lease_id.clone();
            let generation = body.generation;
            let remote = body.remote.clone();
            move || {
                let (item, _lease) = require_work_lease(&state, &id, &lease_id, generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                let remote = remote.unwrap_or_else(|| "origin".into());
                Ok((item, environment, remote))
            }
        },
    )
    .await?;
    let message = run_network_git(
        &state,
        &environment.worktree,
        vec!["fetch".into(), "--prune".into(), remote],
    )
    .await?;
    remember_worktree(&state, &item, &environment.worktree);
    publish_project_change(
        &state,
        &item,
        ForgeProjectEventKind::GitStatus,
        None,
        None,
        None,
    );
    Ok(Json(ChangesSyncResult {
        work_id: id.as_str().to_owned(),
        fetched: true,
        pulled: false,
        pushed: false,
        message: if message.trim().is_empty() {
            "Fetched".into()
        } else {
            message.trim().to_owned()
        },
        changes: admit_forge(
            &state,
            medousa_forge::execution::ExecutionClass::Observation,
            64 * 1024,
            {
                let state = state.clone();
                let id = id.clone();
                move || build_changes_response(&state, &id)
            },
        )
        .await?,
    }))
}

async fn changes_pull(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ChangesLeaseRequest>,
) -> ApiResult<Json<ChangesSyncResult>> {
    let id = parse_work_id(&work_id)?;
    let (item, environment, remote) = admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        None,
        {
            let state = state.clone();
            let id = id.clone();
            let lease_id = body.lease_id.clone();
            let generation = body.generation;
            let remote = body.remote.clone();
            move || {
                let (item, _lease) = require_work_lease(&state, &id, &lease_id, generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                changes_sync_preflight(forge(&state).as_ref(), &environment)?;
                if has_tracked_worktree_edits(forge(&state).as_ref(), &environment)? {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "commit or restore local changes before a fast-forward pull",
                    ));
                }
                let remote = remote.unwrap_or_else(|| "origin".into());
                Ok((item, environment, remote))
            }
        },
    )
    .await?;
    let message = run_network_git(
        &state,
        &environment.worktree,
        vec!["pull".into(), "--ff-only".into(), remote],
    )
    .await?;
    remember_worktree(&state, &item, &environment.worktree);
    publish_project_change(
        &state,
        &item,
        ForgeProjectEventKind::GitStatus,
        None,
        None,
        None,
    );
    Ok(Json(ChangesSyncResult {
        work_id: id.as_str().to_owned(),
        fetched: false,
        pulled: true,
        pushed: false,
        message: if message.trim().is_empty() {
            "Pulled (fast-forward)".into()
        } else {
            message.trim().to_owned()
        },
        changes: admit_forge(
            &state,
            medousa_forge::execution::ExecutionClass::Observation,
            64 * 1024,
            {
                let state = state.clone();
                let id = id.clone();
                move || build_changes_response(&state, &id)
            },
        )
        .await?,
    }))
}

async fn changes_push(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ChangesLeaseRequest>,
) -> ApiResult<Json<ChangesSyncResult>> {
    let id = parse_work_id(&work_id)?;
    let (item, environment, remote, branch) = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        {
            let state = state.clone();
            let id = id.clone();
            let lease_id = body.lease_id.clone();
            let generation = body.generation;
            let remote = body.remote.clone();
            move || {
                let (item, _lease) = require_work_lease(&state, &id, &lease_id, generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                changes_sync_preflight(forge(&state).as_ref(), &environment)?;
                let WorkTarget::Git(target) = &item.target;
                if environment.branch == target.base_ref {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "refusing to push the protected base branch from Changes",
                    ));
                }
                let remote = remote.unwrap_or_else(|| "origin".into());
                let branch = environment.branch.clone();
                Ok((item, environment, remote, branch))
            }
        },
    )
    .await?;
    let message = run_network_git(
        &state,
        &environment.worktree,
        vec![
            "push".into(),
            "--set-upstream".into(),
            remote,
            format!("refs/heads/{branch}"),
        ],
    )
    .await?;
    remember_worktree(&state, &item, &environment.worktree);
    publish_project_change(
        &state,
        &item,
        ForgeProjectEventKind::GitStatus,
        None,
        None,
        None,
    );
    Ok(Json(ChangesSyncResult {
        work_id: id.as_str().to_owned(),
        fetched: false,
        pulled: false,
        pushed: true,
        message: if message.trim().is_empty() {
            format!("Pushed {branch}")
        } else {
            message.trim().to_owned()
        },
        changes: admit_forge(
            &state,
            medousa_forge::execution::ExecutionClass::Observation,
            64 * 1024,
            {
                let state = state.clone();
                let id = id.clone();
                move || build_changes_response(&state, &id)
            },
        )
        .await?,
    }))
}

async fn changes_sync(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ChangesLeaseRequest>,
) -> ApiResult<Json<ChangesSyncResult>> {
    let id = parse_work_id(&work_id)?;
    let (item, environment, remote) = admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        None,
        {
            let state = state.clone();
            let id = id.clone();
            let lease_id = body.lease_id.clone();
            let generation = body.generation;
            let remote = body.remote.clone();
            move || {
                let (item, _lease) = require_work_lease(&state, &id, &lease_id, generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                changes_sync_preflight(forge(&state).as_ref(), &environment)?;
                let remote = remote.unwrap_or_else(|| "origin".into());
                Ok((item, environment, remote))
            }
        },
    )
    .await?;
    let mut messages = Vec::new();
    let mut pulled = false;
    let mut pushed = false;
    let fetch_msg = run_network_git(
        &state,
        &environment.worktree,
        vec!["fetch".into(), "--prune".into(), remote.clone()],
    )
    .await?;
    let fetched = true;
    if !fetch_msg.trim().is_empty() {
        messages.push(fetch_msg.trim().to_owned());
    } else {
        messages.push("Fetched".into());
    }
    let after_fetch = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        64 * 1024,
        {
            let state = state.clone();
            let id = id.clone();
            move || build_changes_response(&state, &id)
        },
    )
    .await?;
    if after_fetch.behind.unwrap_or(0) > 0 {
        let blocked = admit_forge(
            &state,
            medousa_forge::execution::ExecutionClass::RepositoryMetadata,
            16 * 1024,
            {
                let state = state.clone();
                let environment = environment.clone();
                move || has_tracked_worktree_edits(forge(&state).as_ref(), &environment)
            },
        )
        .await?;
        if blocked {
            return Err(request_error(
                StatusCode::CONFLICT,
                "remote is ahead; restore or seal local changes before sync pull",
            ));
        }
        let msg = run_network_git(
            &state,
            &environment.worktree,
            vec!["pull".into(), "--ff-only".into(), remote.clone()],
        )
        .await?;
        pulled = true;
        messages.push(if msg.trim().is_empty() {
            "Pulled (fast-forward)".into()
        } else {
            msg.trim().to_owned()
        });
    }
    let after_pull = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        64 * 1024,
        {
            let state = state.clone();
            let id = id.clone();
            move || build_changes_response(&state, &id)
        },
    )
    .await?;
    if after_pull.ahead.unwrap_or(0) > 0 {
        let WorkTarget::Git(target) = &item.target;
        if environment.branch == target.base_ref {
            return Err(request_error(
                StatusCode::CONFLICT,
                "refusing to push the protected base branch from Changes",
            ));
        }
        let msg = run_network_git(
            &state,
            &environment.worktree,
            vec![
                "push".into(),
                "--set-upstream".into(),
                remote,
                format!("refs/heads/{}", environment.branch),
            ],
        )
        .await?;
        pushed = true;
        messages.push(if msg.trim().is_empty() {
            format!("Pushed {}", environment.branch)
        } else {
            msg.trim().to_owned()
        });
    }
    remember_worktree(&state, &item, &environment.worktree);
    publish_project_change(
        &state,
        &item,
        ForgeProjectEventKind::GitStatus,
        None,
        None,
        None,
    );
    Ok(Json(ChangesSyncResult {
        work_id: id.as_str().to_owned(),
        fetched,
        pulled,
        pushed,
        message: messages.join(" · "),
        changes: admit_forge(
            &state,
            medousa_forge::execution::ExecutionClass::Observation,
            64 * 1024,
            {
                let state = state.clone();
                let id = id.clone();
                move || build_changes_response(&state, &id)
            },
        )
        .await?,
    }))
}

async fn changes_checkpoint(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ChangesLeaseRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let (_item, lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let author = match (body.author_name, body.author_email) {
                    (Some(name), Some(email)) => Some(CheckpointAuthor { name, email }),
                    _ => None,
                };
                let options = SealOptions {
                    ack_risks: body.ack_risks,
                    author,
                };
                let actor = actor_from_state(&state);
                let item = forge(&state)
                    .complete_attempt(&lease, &options, &actor)
                    .map_err(map_err)?;
                if let Some(env) = item.environment_for_attempt(&lease.attempt_id) {
                    let sealed_oid = forge(&state)
                        .git()
                        .head_oid(&env.worktree)
                        .ok()
                        .map(|oid| oid.as_str().to_owned())
                        .unwrap_or_else(|| env.baseline_oid.as_str().to_owned());
                    crate::daemon::detamu_host::spawn_index_forge_item(
                        state.detamu.clone(),
                        item.id.as_str().to_owned(),
                        env.worktree.clone(),
                        sealed_oid,
                        crate::daemon::detamu_host::BindingKind::Sealed,
                    );
                }
                Ok(ok_item(&state, item, "sealed"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ChangesHistoryQuery {
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ChangesHistoryEntry {
    oid: String,
    author_name: String,
    author_email: String,
    authored_at: i64,
    subject: String,
}

#[derive(Debug, Serialize)]
struct ChangesHistoryResponse {
    work_id: String,
    commits: Vec<ChangesHistoryEntry>,
}

async fn changes_history(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ChangesHistoryQuery>,
) -> ApiResult<Json<ChangesHistoryResponse>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                let limit = query.limit.unwrap_or(50).clamp(1, 200);
                let range = format!("{}..HEAD", environment.baseline_oid.as_str());
                let commits = forge(&state)
                    .git()
                    .log_commits(&environment.worktree, &range, limit)
                    .or_else(|_| {
                        forge(&state)
                            .git()
                            .log_commits(&environment.worktree, "HEAD", limit)
                    })
                    .map_err(map_err)?
                    .into_iter()
                    .map(|commit| ChangesHistoryEntry {
                        oid: commit.oid.as_str().to_owned(),
                        author_name: commit.author_name,
                        author_email: commit.author_email,
                        authored_at: commit.authored_at,
                        subject: commit.subject,
                    })
                    .collect();
                Ok(Json(ChangesHistoryResponse {
                    work_id: id.as_str().to_owned(),
                    commits,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ChangesBlameQuery {
    path: String,
}

#[derive(Debug, Serialize)]
struct ChangesBlameHunk {
    oid: String,
    author_name: String,
    author_email: String,
    authored_at: i64,
    summary: String,
    start_line: u32,
    line_count: u32,
}

#[derive(Debug, Serialize)]
struct ChangesBlameResponse {
    work_id: String,
    path: String,
    hunks: Vec<ChangesBlameHunk>,
}

async fn changes_blame(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ChangesBlameQuery>,
) -> ApiResult<Json<ChangesBlameResponse>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let (_, path) = normalize_source_relative(&query.path)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                let hunks = forge(&state)
                    .git()
                    .blame(&environment.worktree, &path)
                    .map_err(map_err)?
                    .into_iter()
                    .map(|hunk| ChangesBlameHunk {
                        oid: hunk.oid.as_str().to_owned(),
                        author_name: hunk.author_name,
                        author_email: hunk.author_email,
                        authored_at: hunk.authored_at,
                        summary: hunk.summary,
                        start_line: hunk.start_line,
                        line_count: hunk.line_count,
                    })
                    .collect();
                Ok(Json(ChangesBlameResponse {
                    work_id: id.as_str().to_owned(),
                    path,
                    hunks,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ResolveChangesConflictRequest {
    path: String,
    /// `ours`, `theirs`, or `baseline`.
    resolution: String,
    #[serde(default)]
    expected_working_digest: Option<String>,
    lease_id: String,
    generation: u64,
}

#[derive(Debug, Serialize)]
struct ResolveChangesConflictResponse {
    work_id: String,
    path: String,
    action: String,
    changes: ForgeChangesResponse,
}

async fn resolve_changes_conflict(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ResolveChangesConflictRequest>,
) -> ApiResult<Json<ResolveChangesConflictResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let (_, path) = normalize_source_relative(&body.path)?;
                let (item, _lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                let comparison = changes_file_diff(&state, &id, &path)?;
                if !comparison.conflict && comparison.status != "unmerged" {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "file is not in an unmerged conflict state",
                    ));
                }
                if let Some(expected) = body
                    .expected_working_digest
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    match comparison.working_digest.as_deref() {
                        Some(have) if have != expected => {
                            return Err(request_error(
                                StatusCode::CONFLICT,
                                "the working copy changed; refresh Changes before resolving",
                            ));
                        }
                        None => {
                            return Err(request_error(
                                StatusCode::CONFLICT,
                                "the working copy changed; refresh Changes before resolving",
                            ));
                        }
                        _ => {}
                    }
                }
                let resolution = body.resolution.trim().to_ascii_lowercase();
                let action = match resolution.as_str() {
                    "ours" | "theirs" => {
                        forge(&state)
                            .git()
                            .checkout_conflict_side(&environment.worktree, &path, &resolution)
                            .map_err(map_err)?;
                        forge(&state)
                            .git()
                            .add_path(&environment.worktree, &path)
                            .map_err(map_err)?;
                        format!("resolved_{resolution}")
                    }
                    "baseline" => {
                        if comparison.binary {
                            return Err(request_error(
                                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                                "binary conflict baseline restore is not available from Home",
                            ));
                        }
                        let content = comparison.baseline.content.clone().ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "baseline text is unavailable for this conflict",
                            )
                        })?;
                        let candidate = environment.worktree.join(&path);
                        let (destination, _) = if candidate.is_file() {
                            resolve_source_path(&environment.worktree, &path)?
                        } else {
                            resolve_new_source_path(&environment.worktree, &path)?
                        };
                        if let Some(parent) = destination.parent() {
                            std::fs::create_dir_all(parent).map_err(|err| {
                                request_error(
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    format!("could not resolve conflict: {err}"),
                                )
                            })?;
                        }
                        std::fs::write(&destination, content.as_bytes()).map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not resolve conflict: {err}"),
                            )
                        })?;
                        forge(&state)
                            .git()
                            .add_path(&environment.worktree, &path)
                            .map_err(map_err)?;
                        "resolved_baseline".into()
                    }
                    _ => {
                        return Err(request_error(
                            StatusCode::BAD_REQUEST,
                            "resolution must be ours, theirs, or baseline",
                        ));
                    }
                };
                let digest = std::fs::read(environment.worktree.join(&path))
                    .ok()
                    .map(|bytes| source_digest(&bytes));
                publish_project_change(
                    &state,
                    &item,
                    ForgeProjectEventKind::Changed,
                    Some(path.clone()),
                    None,
                    digest,
                );
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(ResolveChangesConflictResponse {
                    work_id: id.as_str().to_owned(),
                    path,
                    action,
                    changes: build_changes_response(&state, &id)?,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct RevertChangesHunkRequest {
    path: String,
    /// 0-based hunk index from `GET …/changes/file`.
    hunk_index: usize,
    expected_working_digest: String,
    lease_id: String,
    generation: u64,
}

fn apply_hunks_except(baseline: &str, hunks: &[ReviewDiffHunk], skip: usize) -> ApiResult<String> {
    let mut lines: Vec<String> = if baseline.is_empty() {
        Vec::new()
    } else {
        let mut out: Vec<String> = baseline.split('\n').map(str::to_string).collect();
        if baseline.ends_with('\n') {
            out.pop();
        }
        out
    };
    // Apply remaining hunks from bottom to top so earlier offsets stay valid.
    let mut ordered: Vec<(usize, &ReviewDiffHunk)> = hunks
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != skip)
        .collect();
    ordered.sort_by_key(|b| std::cmp::Reverse(b.1.old_start));
    for (_, hunk) in ordered {
        let start = hunk.old_start.saturating_sub(1);
        if start > lines.len() {
            return Err(request_error(
                StatusCode::CONFLICT,
                "hunk no longer applies cleanly; refresh the diff",
            ));
        }
        let mut replacement: Vec<String> = Vec::new();
        let mut consumed = 0usize;
        for line in &hunk.lines {
            match line.kind.as_str() {
                "context" => {
                    replacement.push(line.content.clone());
                    consumed += 1;
                }
                "deletion" => {
                    consumed += 1;
                }
                "addition" => {
                    replacement.push(line.content.clone());
                }
                _ => {}
            }
        }
        let end = (start + consumed).min(lines.len());
        lines.splice(start..end, replacement);
    }
    let mut out = lines.join("\n");
    if baseline.ends_with('\n') && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    } else if baseline.ends_with('\n') && out.is_empty() {
        // empty file with trailing newline convention — leave empty
    }
    Ok(out)
}

async fn revert_changes_hunk(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RevertChangesHunkRequest>,
) -> ApiResult<Json<RestoreChangesFileResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let comparison = changes_file_diff(&state, &id, &body.path)?;
                if comparison.binary {
                    return Err(request_error(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        "hunk revert is only available for text files",
                    ));
                }
                if comparison.hunks.is_empty() {
                    return Err(request_error(StatusCode::CONFLICT, "no hunks to revert"));
                }
                if body.hunk_index >= comparison.hunks.len() {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "hunk_index is out of range",
                    ));
                }
                let expected = body.expected_working_digest.trim();
                match comparison.working_digest.as_deref() {
                    Some(have) if have == expected => {}
                    _ => {
                        return Err(request_error(
                            StatusCode::CONFLICT,
                            "the working copy changed; refresh Changes before reverting",
                        ));
                    }
                }
                let baseline = comparison.baseline.content.clone().unwrap_or_default();
                let next = apply_hunks_except(&baseline, &comparison.hunks, body.hunk_index)?;
                let (item, _lease) =
                    require_work_lease(&state, &id, &body.lease_id, body.generation)?;
                let environment = item.workspace_environment().cloned().ok_or_else(|| {
                    request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
                })?;
                let candidate = environment.worktree.join(&comparison.path);
                let (destination, relative) = if candidate.is_file() || comparison.working.exists {
                    if candidate.is_file() {
                        resolve_source_path(&environment.worktree, &comparison.path)?
                    } else {
                        resolve_new_source_path(&environment.worktree, &comparison.path)?
                    }
                } else {
                    resolve_new_source_path(&environment.worktree, &comparison.path)?
                };
                if next.is_empty() && !comparison.baseline.exists {
                    if destination.is_file() {
                        std::fs::remove_file(&destination).map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not revert hunk: {err}"),
                            )
                        })?;
                    }
                    publish_project_change(
                        &state,
                        &item,
                        ForgeProjectEventKind::Deleted,
                        Some(relative.clone()),
                        None,
                        None,
                    );
                    remember_worktree(&state, &item, &environment.worktree);
                    return Ok(Json(RestoreChangesFileResponse {
                        work_id: id.as_str().to_owned(),
                        path: relative,
                        action: "hunk_reverted_deleted".into(),
                        digest: None,
                    }));
                }
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not revert hunk: {err}"),
                        )
                    })?;
                }
                std::fs::write(&destination, next.as_bytes()).map_err(|err| {
                    request_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("could not revert hunk: {err}"),
                    )
                })?;
                let digest = source_digest(next.as_bytes());
                publish_project_change(
                    &state,
                    &item,
                    ForgeProjectEventKind::Changed,
                    Some(relative.clone()),
                    None,
                    Some(digest.clone()),
                );
                remember_worktree(&state, &item, &environment.worktree);
                Ok(Json(RestoreChangesFileResponse {
                    work_id: id.as_str().to_owned(),
                    path: relative,
                    action: "hunk_reverted".into(),
                    digest: Some(digest),
                }))
            }
        },
    )
    .await
}

async fn get_review(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ReviewSelectionQuery>,
) -> ApiResult<Json<ReviewProjection>> {
    let id = parse_work_id(&work_id)?;
    let mut review = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            let attempt_raw = query.attempt_id.clone();
            move || {
                let item = forge(&state).load(&id).map_err(map_err)?;
                let attempt_id = attempt_raw
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| medousa_forge::model::AttemptId::from(value.to_string()));
                if let Some(attempt_id) = attempt_id.as_ref()
                    && item
                        .attempt(attempt_id)
                        .is_none_or(|attempt| attempt.evidence_id.is_none())
                {
                    return Err(request_error(
                        StatusCode::NOT_FOUND,
                        "review attempt was not found or has no sealed evidence",
                    ));
                }
                Ok(build_review_for_attempt(
                    forge(&state).as_ref(),
                    &item,
                    attempt_id.as_ref(),
                ))
            }
        },
    )
    .await?;
    review.world = Some(state.detamu.binding_status_json(review.work_id.as_str()));
    Ok(Json(review))
}

#[derive(Debug, Deserialize)]
struct AddReviewCommentRequest {
    evidence_id: String,
    #[serde(default)]
    attempt_id: Option<String>,
    path: String,
    #[serde(default = "default_comment_side")]
    side: String,
    start_line: u32,
    #[serde(default)]
    end_line: Option<u32>,
    #[serde(default)]
    anchor_text: Option<String>,
    body: String,
    #[serde(default)]
    parent_id: Option<String>,
}

fn default_comment_side() -> String {
    "new".into()
}

#[derive(Debug, Deserialize)]
struct PatchReviewCommentRequest {
    #[serde(default)]
    resolve: Option<bool>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestReviewChangesRequest {
    evidence_id: String,
    evidence_digest: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    comment_ids: Option<Vec<String>>,
}

async fn list_review_comments(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ReviewSelectionQuery>,
) -> ApiResult<Json<Vec<ReviewCommentProjection>>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                let attempt_id = query
                    .attempt_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| medousa_forge::model::AttemptId::from(value.to_string()));
                let review =
                    build_review_for_attempt(forge(&state).as_ref(), &item, attempt_id.as_ref());
                Ok(Json(review.comments))
            }
        },
    )
    .await
}

async fn add_review_comment(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<AddReviewCommentRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let evidence_id = EvidenceId::from(body.evidence_id);
                let attempt_id = body
                    .attempt_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| medousa_forge::model::AttemptId::from(value.to_string()));
                let parent_id = body
                    .parent_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| ReviewCommentId::from(value.to_string()));
                let end_line = body.end_line.unwrap_or(body.start_line);
                let item = forge(&state)
                    .add_review_comment(
                        &id,
                        evidence_id,
                        attempt_id,
                        body.path,
                        body.side,
                        body.start_line,
                        end_line,
                        body.anchor_text,
                        body.body,
                        parent_id,
                        &actor,
                    )
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "review_comment_added"))
            }
        },
    )
    .await
}

async fn patch_review_comment(
    State(state): State<AppState>,
    Path((work_id, comment_id)): Path<(String, String)>,
    Json(body): Json<PatchReviewCommentRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let comment_id = ReviewCommentId::from(comment_id);
                if body.resolve.is_none() && body.body.is_none() {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "patch requires resolve and/or body",
                    ));
                }
                if body.resolve == Some(false) {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "unresolving comments is not supported",
                    ));
                }
                let forge = forge(&state);
                let mut item = forge.load(&id).map_err(map_err)?;
                if let Some(text) = body.body {
                    item = forge
                        .update_review_comment_body(&id, &comment_id, text, &actor)
                        .map_err(map_err)?;
                }
                if body.resolve == Some(true) {
                    item = forge
                        .resolve_review_comment(&id, &comment_id, &actor)
                        .map_err(map_err)?;
                }
                Ok(ok_item(&state, item, "review_comment_updated"))
            }
        },
    )
    .await
}

async fn delete_review_comment(
    State(state): State<AppState>,
    Path((work_id, comment_id)): Path<(String, String)>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let comment_id = ReviewCommentId::from(comment_id);
                let item = forge(&state)
                    .delete_review_comment(&id, &comment_id, &actor)
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "review_comment_deleted"))
            }
        },
    )
    .await
}

async fn request_review_changes(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RequestReviewChangesRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let evidence_id = EvidenceId::from(body.evidence_id);
                let evidence_digest = medousa_forge::model::Digest::from_hex(body.evidence_digest);
                let comment_ids = body.comment_ids.map(|ids| {
                    ids.into_iter()
                        .map(ReviewCommentId::from)
                        .collect::<Vec<_>>()
                });
                let item = forge(&state)
                    .request_changes(
                        &id,
                        evidence_id,
                        evidence_digest,
                        body.summary,
                        comment_ids,
                        &actor,
                    )
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "changes_requested"))
            }
        },
    )
    .await
}

/// Reopen sealed review for human edits (no agent). Same custody path as
/// restore-from-review, without mutating a specific file.
async fn continue_editing(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<BeginAttemptResponse>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let forge = forge(&state);
                let item = forge.load(&id).map_err(map_err)?;
                if item.state != WorkState::AwaitingReview {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        format!("Cannot continue editing in state {}", item.state),
                    ));
                }
                let source_attempt_id = item
                    .attempts
                    .iter()
                    .filter(|attempt| attempt.evidence_id.is_some())
                    .max_by_key(|attempt| attempt.seq)
                    .map(|attempt| attempt.id.clone())
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "No sealed evidence to continue editing from",
                        )
                    })?;
                forge
                    .reopen_for_changes(&id, "Continue editing after review", &actor)
                    .map_err(map_err)?;
                let (item, lease) = forge
                    .begin_isolated_attempt_from(
                        &id,
                        &source_attempt_id,
                        ExecutorDescriptor {
                            kind: "human".into(),
                            detail: serde_json::json!({"reason": "continue_editing"}),
                        },
                        None,
                        &actor,
                    )
                    .map_err(map_err)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "isolated attempt has no governed environment",
                            )
                        })?;
                let attempt_id = lease.attempt_id.as_str().to_owned();
                let worktree = environment.worktree.display().to_string();
                let branch = environment.branch.clone();
                publish_item(&state, &item, "continue_editing");
                Ok(Json(BeginAttemptResponse {
                    item: project_item(item),
                    lease,
                    attempt_id,
                    worktree,
                    branch,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ReviewSelectionQuery {
    #[serde(default)]
    attempt_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReviewFileQuery {
    path: String,
    #[serde(default)]
    attempt_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewFileVersion {
    exists: bool,
    binary: bool,
    byte_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewDiffLine {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_line: Option<usize>,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewDiffHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<ReviewDiffLine>,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewFileDiff {
    work_id: String,
    attempt_id: String,
    path: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    baseline_oid: String,
    reviewed_oid: String,
    binary: bool,
    baseline: ReviewFileVersion,
    reviewed: ReviewFileVersion,
    hunks: Vec<ReviewDiffHunk>,
    changed_lines: Vec<ReviewChangedLine>,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewChangedLine {
    line: usize,
    kind: String,
}

fn review_file_diff(
    state: &AppState,
    id: &WorkId,
    raw_path: &str,
    selected_attempt_id: Option<&str>,
) -> ApiResult<ReviewFileDiff> {
    const MAX_REVIEW_FILE_BYTES: usize = 1024 * 1024;
    let (_, path) = normalize_source_relative(raw_path)?;
    let forge = forge(state);
    let item = forge.load(id).map_err(map_err)?;
    let attempt_id = selected_attempt_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| medousa_forge::model::AttemptId::from(value.to_string()));
    if let Some(attempt_id) = attempt_id.as_ref()
        && item
            .attempt(attempt_id)
            .is_none_or(|attempt| attempt.evidence_id.is_none())
    {
        return Err(request_error(
            StatusCode::NOT_FOUND,
            "review attempt was not found or has no sealed evidence",
        ));
    }
    let review = build_review_for_attempt(forge.as_ref(), &item, attempt_id.as_ref());
    let changed = review
        .changed_files
        .iter()
        .find(|file| file.path == path)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "file is not part of this review"))?;
    let environment = review
        .attempt_id
        .as_deref()
        .map(|value| medousa_forge::model::AttemptId::from(value.to_string()))
        .as_ref()
        .and_then(|attempt_id| item.attempt(attempt_id))
        .and_then(|attempt| attempt.environment.as_ref())
        .or_else(|| item.workspace_environment())
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
    let baseline_oid =
        GitOid::new(review.baseline_oid.clone().ok_or_else(|| {
            request_error(StatusCode::CONFLICT, "review has no starting revision")
        })?);
    let reviewed_oid = GitOid::new(
        review
            .sealed_head_oid
            .clone()
            .ok_or_else(|| request_error(StatusCode::CONFLICT, "review has no saved revision"))?,
    );
    let baseline_path = changed.old_path.as_deref().unwrap_or(&changed.path);
    let baseline_bytes = forge
        .git()
        .show_bytes(&environment.worktree, &baseline_oid, baseline_path)
        .ok();
    let reviewed_bytes = forge
        .git()
        .show_bytes(&environment.worktree, &reviewed_oid, &changed.path)
        .ok();
    let binary = changed.is_binary
        || baseline_bytes
            .as_ref()
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
        || reviewed_bytes
            .as_ref()
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_err());
    let truncated = baseline_bytes
        .as_ref()
        .is_some_and(|bytes| bytes.len() > MAX_REVIEW_FILE_BYTES)
        || reviewed_bytes
            .as_ref()
            .is_some_and(|bytes| bytes.len() > MAX_REVIEW_FILE_BYTES);
    let version = |bytes: &Option<Vec<u8>>| ReviewFileVersion {
        exists: bytes.is_some(),
        binary,
        byte_size: bytes.as_ref().map(|value| value.len() as u64).unwrap_or(0),
        digest: bytes.as_ref().map(|value| source_digest(value)),
        content: if binary || truncated {
            None
        } else {
            bytes
                .as_ref()
                .and_then(|value| String::from_utf8(value.clone()).ok())
        },
    };
    let patch = if binary {
        Vec::new()
    } else {
        forge
            .git()
            .diff_path(
                &environment.worktree,
                &baseline_oid,
                &reviewed_oid,
                &changed.path,
            )
            .map_err(map_err)?
    };
    let hunks = parse_review_hunks(&String::from_utf8_lossy(&patch));
    let mut changed_lines = Vec::new();
    for hunk in &hunks {
        for line in &hunk.lines {
            if line.kind == "addition" {
                if let Some(number) = line.new_line {
                    changed_lines.push(ReviewChangedLine {
                        line: number,
                        kind: "added".into(),
                    });
                }
            } else if line.kind == "deletion" {
                changed_lines.push(ReviewChangedLine {
                    line: line.new_line.unwrap_or(hunk.new_start.max(1)),
                    kind: "deleted".into(),
                });
            }
        }
    }
    changed_lines.sort_by_key(|line| (line.line, line.kind.clone()));
    changed_lines.dedup_by(|left, right| left.line == right.line && left.kind == right.kind);
    Ok(ReviewFileDiff {
        work_id: id.as_str().to_owned(),
        attempt_id: review.attempt_id.clone().unwrap_or_default(),
        path: changed.path.clone(),
        status: changed.status.clone(),
        old_path: changed.old_path.clone(),
        baseline_oid: baseline_oid.as_str().to_owned(),
        reviewed_oid: reviewed_oid.as_str().to_owned(),
        binary,
        baseline: version(&baseline_bytes),
        reviewed: version(&reviewed_bytes),
        hunks,
        changed_lines,
        truncated,
    })
}

fn parse_range(raw: &str) -> Option<(usize, usize)> {
    let value = raw.trim_start_matches(['-', '+']);
    let mut parts = value.split(',');
    let start = parts.next()?.parse().ok()?;
    let count = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
    Some((start, count))
}

fn parse_review_hunks(patch: &str) -> Vec<ReviewDiffHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<ReviewDiffHunk> = None;
    let mut old_line = 0usize;
    let mut new_line = 0usize;
    for raw in patch.lines() {
        if let Some(header) = raw.strip_prefix("@@ ")
            && let Some((ranges, _)) = header.split_once(" @@")
        {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let mut parts = ranges.split_whitespace();
            let Some((old_start, old_count)) = parts.next().and_then(parse_range) else {
                continue;
            };
            let Some((new_start, new_count)) = parts.next().and_then(parse_range) else {
                continue;
            };
            old_line = old_start;
            new_line = new_start;
            current = Some(ReviewDiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            continue;
        };
        if raw.starts_with("\\ No newline") {
            continue;
        }
        let (kind, old_number, new_number, content) = if let Some(content) = raw.strip_prefix('+') {
            let number = new_line;
            new_line += 1;
            ("addition", None, Some(number), content)
        } else if let Some(content) = raw.strip_prefix('-') {
            let number = old_line;
            old_line += 1;
            ("deletion", Some(number), None, content)
        } else {
            let content = raw.strip_prefix(' ').unwrap_or(raw);
            let old_number = old_line;
            let new_number = new_line;
            old_line += 1;
            new_line += 1;
            ("context", Some(old_number), Some(new_number), content)
        };
        hunk.lines.push(ReviewDiffLine {
            kind: kind.into(),
            old_line: old_number,
            new_line: new_number,
            content: content.into(),
        });
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    hunks
}

async fn get_review_file(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ReviewFileQuery>,
) -> ApiResult<Json<ReviewFileDiff>> {
    let id = parse_work_id(&work_id)?;
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            let path = query.path.clone();
            let attempt_id = query.attempt_id.clone();
            move || {
                Ok(Json(review_file_diff(
                    &state,
                    &id,
                    &path,
                    attempt_id.as_deref(),
                )?))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct RestoreReviewFileRequest {
    path: String,
    expected_reviewed_oid: String,
    #[serde(default)]
    attempt_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct RestoreReviewFileResponse {
    item: ItemProjection,
    lease: ExecutionLease,
    path: String,
    action: String,
    preserved_revision: String,
}

async fn restore_review_file(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RestoreReviewFileRequest>,
) -> ApiResult<Json<RestoreReviewFileResponse>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let comparison =
                    review_file_diff(&state, &id, &body.path, body.attempt_id.as_deref())?;
                if comparison.reviewed_oid != body.expected_reviewed_oid {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "the reviewed revision changed; reopen the comparison before restoring",
                    ));
                }
                if comparison.binary && comparison.baseline.exists {
                    return Err(request_error(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        "binary recovery is preserved in Git but cannot yet be restored from Home",
                    ));
                }
                let forge = forge(&state);
                let actor = actor_from_state(&state);
                let source_attempt_id =
                    medousa_forge::model::AttemptId::from(comparison.attempt_id.clone());
                forge
                    .reopen_for_changes(
                        &id,
                        "A reviewed file was restored for another pass",
                        &actor,
                    )
                    .map_err(map_err)?;
                let (mut item, lease) = forge
                    .begin_isolated_attempt_from(
                        &id,
                        &source_attempt_id,
                        ExecutorDescriptor {
                            kind: "human".into(),
                            detail: serde_json::json!({"reason": "restore_review_file"}),
                        },
                        None,
                        &actor,
                    )
                    .map_err(map_err)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::CONFLICT,
                                "governed workspace is not prepared",
                            )
                        })?;
                let restored_path = comparison
                    .old_path
                    .as_deref()
                    .unwrap_or(&comparison.path)
                    .to_owned();
                if comparison.path != restored_path {
                    let (renamed, _) =
                        resolve_source_path(&environment.worktree, &comparison.path)?;
                    std::fs::remove_file(renamed).map_err(|err| {
                        request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not restore file: {err}"),
                        )
                    })?;
                }
                let action = if let Some(content) = comparison.baseline.content {
                    let candidate = environment.worktree.join(&restored_path);
                    let (destination, _) = if candidate.is_file() {
                        resolve_source_path(&environment.worktree, &restored_path)?
                    } else {
                        resolve_new_source_path(&environment.worktree, &restored_path)?
                    };
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent).map_err(|err| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("could not restore folder: {err}"),
                            )
                        })?;
                    }
                    std::fs::write(destination, content.as_bytes()).map_err(|err| {
                        request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not restore file: {err}"),
                        )
                    })?;
                    "restored"
                } else {
                    let (destination, _) =
                        resolve_source_path(&environment.worktree, &comparison.path)?;
                    std::fs::remove_file(destination).map_err(|err| {
                        request_error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("could not restore file: {err}"),
                        )
                    })?;
                    "removed"
                };
                item = forge.load(&id).map_err(map_err)?;
                publish_item(&state, &item, "review_file_restored");
                Ok(Json(RestoreReviewFileResponse {
                    item: project_item(item),
                    lease,
                    path: restored_path,
                    action: action.into(),
                    preserved_revision: comparison.reviewed_oid,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectTask {
    #[serde(default = "default_project_task_version")]
    version: u8,
    id: String,
    label: String,
    kind: String,
    argv: Vec<String>,
    provider: String,
    #[serde(default = "default_project_task_source")]
    source: String,
    /// Repository-relative directory where the command runs.
    #[serde(default = "default_project_task_root")]
    root: String,
    #[serde(default)]
    interactive: bool,
    #[serde(default)]
    background: bool,
    #[serde(default)]
    default_rank: i32,
    #[serde(default = "default_true")]
    available: bool,
    #[serde(default)]
    requirements: Vec<ProjectTaskRequirement>,
    #[serde(default)]
    long_running: bool,
    /// Optional regex (unanchored) that marks a background task ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ready_pattern: Option<String>,
    /// Optional VS Code-style problem matcher pattern for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    problem_matcher: Option<ProjectProblemPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectTaskRequirement {
    kind: String,
    name: String,
    available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repair: Option<String>,
}

fn default_project_task_version() -> u8 {
    1
}

fn default_project_task_source() -> String {
    "detected".into()
}

fn default_true() -> bool {
    true
}

fn default_project_task_root() -> String {
    ".".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectProblemPattern {
    regexp: String,
    /// 1-based capture group indices (VS Code tasks.json convention).
    file: u8,
    line: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    column: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectTaskResult {
    task: ProjectTask,
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    truncated: bool,
    duration_ms: u128,
    #[serde(default)]
    locations: Vec<ProjectOutputLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectOutputLocation {
    path: String,
    line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u32>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectTest {
    id: String,
    label: String,
    path: String,
    line: u32,
    task_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectTaskRun {
    run_id: String,
    work_id: String,
    state: String,
    task: ProjectTask,
    result: Option<ProjectTaskResult>,
    /// Bounded live stdout retained while the run is active (and after for replay).
    #[serde(default)]
    stdout: String,
    /// Bounded live stderr retained while the run is active (and after for replay).
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    output_truncated: bool,
    /// Next chunk sequence number for SSE `?since=` replay.
    #[serde(default)]
    next_seq: u64,
    /// Incrementally matched problem locations (also on final result).
    #[serde(default)]
    locations: Vec<ProjectOutputLocation>,
    /// Loopback URL detected when a long-running task became ready.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ready_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectTaskOutputEvent {
    seq: u64,
    run_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    available_from: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ProjectTaskResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locations: Option<Vec<ProjectOutputLocation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ready_url: Option<String>,
}

const TASK_OUTPUT_CAP: usize = 256 * 1024;
const TASK_CHUNK_REPLAY_CAP: usize = 400;
const TASK_CHUNK_REPLAY_BYTES: usize = 1024 * 1024;
const PROJECT_TASK_RUN_CAP: usize = 128;
const PROJECT_TASK_TERMINAL_CAP: usize = 64;
const PROJECT_TASK_RUN_MEMORY_RESERVATION: u32 = 2 * 1024 * 1024;
const PROJECT_TASK_GLOBAL_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const PROJECT_TASK_TERMINAL_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const CONFIGURED_TASK_CAP: usize = 24;
const DETECTED_PROJECT_ROOT_CAP: usize = 48;
const DETECTED_PROJECT_TASK_CAP: usize = 256;

#[derive(Default)]
struct TaskOutputTail {
    chunks: std::collections::VecDeque<String>,
    bytes: usize,
}

impl TaskOutputTail {
    fn append(&mut self, chunk: &str) -> bool {
        if chunk.is_empty() {
            return false;
        }
        let mut retained = chunk;
        let mut truncated = false;
        if retained.len() > TASK_OUTPUT_CAP {
            let mut start = retained.len() - TASK_OUTPUT_CAP;
            while start < retained.len() && !retained.is_char_boundary(start) {
                start += 1;
            }
            retained = &retained[start..];
            self.chunks.clear();
            self.bytes = 0;
            truncated = true;
        }
        self.bytes = self.bytes.saturating_add(retained.len());
        self.chunks.push_back(retained.to_owned());
        while self.bytes > TASK_OUTPUT_CAP {
            let Some(evicted) = self.chunks.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.len());
            truncated = true;
        }
        truncated
    }

    fn materialize(&self) -> String {
        let mut output = String::with_capacity(self.bytes);
        for chunk in &self.chunks {
            output.push_str(chunk);
        }
        output
    }

    fn clear(&mut self) {
        self.chunks.clear();
        self.bytes = 0;
    }
}

struct ProjectTaskRunStore {
    run: ProjectTaskRun,
    repository_root: PathBuf,
    working_root: PathBuf,
    ready_re: Option<regex::Regex>,
    problem_re: Option<(regex::Regex, ProjectProblemPattern)>,
    stdout: TaskOutputTail,
    stderr: TaskOutputTail,
    chunks: std::collections::VecDeque<(Arc<ProjectTaskOutputEvent>, usize)>,
    chunk_bytes: usize,
    tx: tokio::sync::broadcast::Sender<Arc<ProjectTaskOutputEvent>>,
}

impl ProjectTaskRunStore {
    fn snapshot(&self) -> ProjectTaskRun {
        let mut run = self.run.clone();
        if let Some(result) = &run.result {
            run.stdout = result.stdout.clone();
            run.stderr = result.stderr.clone();
        } else {
            run.stdout = self.stdout.materialize();
            run.stderr = self.stderr.materialize();
        }
        run
    }
}

struct ProjectTaskRunHandle {
    store: tokio::sync::Mutex<ProjectTaskRunStore>,
    terminal_at: Mutex<Option<std::time::Instant>>,
    _memory_permit: tokio::sync::OwnedSemaphorePermit,
}

static PROJECT_TASK_RUNS: LazyLock<
    tokio::sync::RwLock<std::collections::HashMap<String, Arc<ProjectTaskRunHandle>>>,
> = LazyLock::new(|| tokio::sync::RwLock::new(std::collections::HashMap::new()));
static PROJECT_TASK_MEMORY: LazyLock<Arc<tokio::sync::Semaphore>> = LazyLock::new(|| {
    Arc::new(tokio::sync::Semaphore::new(
        PROJECT_TASK_GLOBAL_MEMORY_BYTES,
    ))
});
static PROJECT_TASK_CHILDREN: LazyLock<
    tokio::sync::RwLock<
        std::collections::HashMap<String, Arc<tokio::sync::Mutex<tokio::process::Child>>>,
    >,
> = LazyLock::new(|| tokio::sync::RwLock::new(std::collections::HashMap::new()));

fn prune_project_task_runs(
    runs: &mut std::collections::HashMap<String, Arc<ProjectTaskRunHandle>>,
) {
    let now = std::time::Instant::now();
    runs.retain(|_, handle| {
        handle
            .terminal_at
            .lock()
            .expect("project task terminal timestamp")
            .is_none_or(|terminal| now.duration_since(terminal) < PROJECT_TASK_TERMINAL_TTL)
    });
    let mut terminal = runs
        .iter()
        .filter_map(|(id, handle)| {
            handle
                .terminal_at
                .lock()
                .expect("project task terminal timestamp")
                .map(|at| (at, id.clone()))
        })
        .collect::<Vec<_>>();
    terminal.sort_by_key(|(at, _)| *at);
    let overflow = terminal.len().saturating_sub(PROJECT_TASK_TERMINAL_CAP);
    for (_, id) in terminal.into_iter().take(overflow) {
        runs.remove(&id);
    }
}

async fn project_task_run_snapshot(run_id: &str) -> Option<ProjectTaskRun> {
    let handle = PROJECT_TASK_RUNS.read().await.get(run_id).cloned()?;
    Some(handle.store.lock().await.snapshot())
}

fn task_run_is_terminal(run: &ProjectTaskRun) -> bool {
    matches!(run.state.as_str(), "passed" | "failed")
        || (run.state == "cancelled" && run.result.is_some())
}

fn task_output_event_is_terminal(event: &ProjectTaskOutputEvent) -> bool {
    event.kind == "state" && event.result.is_some()
}

fn task_replay_gap_event(
    run_id: &str,
    requested: u64,
    available_from: u64,
) -> ProjectTaskOutputEvent {
    ProjectTaskOutputEvent {
        seq: requested,
        run_id: run_id.to_owned(),
        kind: "gap".into(),
        available_from: Some(available_from),
        stream: None,
        text: Some(format!(
            "requested sequence {requested} expired; replay resumes at {available_from}"
        )),
        state: Some("replay_gap".into()),
        result: None,
        locations: None,
        ready_url: None,
    }
}

fn default_ready_pattern() -> &'static str {
    r"(?i)(listening on|local:\s*https?://|ready in\b|compiled successfully|started server|webpack compiled|vite.+ready|nest application successfully started|serving on\b)"
}

fn compile_ready_pattern(task: &ProjectTask) -> Option<regex::Regex> {
    if !task.long_running {
        return None;
    }
    let pattern = task
        .ready_pattern
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_ready_pattern());
    regex::Regex::new(pattern).ok()
}

fn compile_problem_pattern(task: &ProjectTask) -> Option<(regex::Regex, ProjectProblemPattern)> {
    let pattern = task.problem_matcher.as_ref()?;
    let re = regex::Regex::new(&pattern.regexp).ok()?;
    Some((re, pattern.clone()))
}

fn merge_task_locations(
    into: &mut Vec<ProjectOutputLocation>,
    incoming: impl IntoIterator<Item = ProjectOutputLocation>,
) {
    for location in incoming {
        if into.iter().any(|existing| {
            existing.path == location.path
                && existing.line == location.line
                && existing.column == location.column
        }) {
            continue;
        }
        into.push(location);
        if into.len() >= 100 {
            break;
        }
    }
}

fn parse_output_locations_with_matcher(
    repository_root: &FsPath,
    working_root: &FsPath,
    output: &str,
    matcher: Option<&(regex::Regex, ProjectProblemPattern)>,
) -> Vec<ProjectOutputLocation> {
    let Some((re, pattern)) = matcher else {
        return parse_output_locations(repository_root, working_root, output);
    };
    let mut locations = Vec::new();
    for line_text in output.lines() {
        let Some(captures) = re.captures(line_text) else {
            continue;
        };
        let file_idx = usize::from(pattern.file);
        let line_idx = usize::from(pattern.line);
        let Some(raw) = captures.get(file_idx).map(|m| m.as_str()) else {
            continue;
        };
        let line = captures
            .get(line_idx)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1);
        let column = pattern
            .column
            .and_then(|idx| captures.get(usize::from(idx)))
            .and_then(|m| m.as_str().parse().ok());
        let message = pattern
            .message
            .and_then(|idx| captures.get(usize::from(idx)))
            .map(|m| m.as_str().chars().take(300).collect())
            .unwrap_or_else(|| line_text.trim().chars().take(300).collect());
        let path = std::path::Path::new(raw.trim());
        let Some(path) = normalize_output_path(repository_root, working_root, path) else {
            continue;
        };
        locations.push(ProjectOutputLocation {
            path,
            line,
            column,
            message,
        });
        if locations.len() >= 100 {
            break;
        }
    }
    locations
}

fn push_task_event(store: &mut ProjectTaskRunStore, event: ProjectTaskOutputEvent) {
    let encoded_bytes = serde_json::to_vec(&event).map_or(0, |bytes| bytes.len());
    let event = Arc::new(event);
    store.chunk_bytes = store.chunk_bytes.saturating_add(encoded_bytes);
    store.chunks.push_back((Arc::clone(&event), encoded_bytes));
    while store.chunks.len() > TASK_CHUNK_REPLAY_CAP || store.chunk_bytes > TASK_CHUNK_REPLAY_BYTES
    {
        let Some((_evicted, bytes)) = store.chunks.pop_front() else {
            break;
        };
        store.chunk_bytes = store.chunk_bytes.saturating_sub(bytes);
    }
    let _ = store.tx.send(event);
}

async fn publish_task_output(run_id: &str, stream: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    let Some(handle) = PROJECT_TASK_RUNS.read().await.get(run_id).cloned() else {
        return;
    };
    let (became_ready, work_id, ready_url) = {
        let mut store = handle.store.lock().await;
        if store.run.run_id != run_id {
            return;
        }
        if stream == "stderr" {
            store.run.output_truncated |= store.stderr.append(text);
        } else {
            store.run.output_truncated |= store.stdout.append(text);
        }
        let matched = parse_output_locations_with_matcher(
            &store.repository_root,
            &store.working_root,
            text,
            store.problem_re.as_ref(),
        );
        let before = store.run.locations.len();
        merge_task_locations(&mut store.run.locations, matched);
        let new_locations = if store.run.locations.len() > before {
            Some(store.run.locations[before..].to_vec())
        } else {
            None
        };
        let seq = store.run.next_seq;
        store.run.next_seq = seq.saturating_add(1);
        push_task_event(
            &mut store,
            ProjectTaskOutputEvent {
                seq,
                run_id: run_id.to_owned(),
                kind: "output".into(),
                available_from: None,
                stream: Some(stream.into()),
                text: Some(text.to_owned()),
                state: None,
                result: None,
                locations: new_locations,
                ready_url: None,
            },
        );
        let waiting_for_ready = store.run.task.long_running
            && matches!(store.run.state.as_str(), "running")
            && store.ready_re.is_some();
        let matched_ready =
            waiting_for_ready && store.ready_re.as_ref().is_some_and(|re| re.is_match(text));
        if matched_ready {
            let haystack = format!(
                "{}\n{}\n{}",
                store.stdout.materialize(),
                store.stderr.materialize(),
                text
            );
            if let Some(url) = crate::daemon::forge_preview::extract_ready_url(&haystack) {
                store.run.ready_url = Some(url);
            } else if let Some(url) = crate::daemon::forge_preview::extract_ready_url(text) {
                store.run.ready_url = Some(url);
            }
        }
        (
            matched_ready,
            store.run.work_id.clone(),
            store.run.ready_url.clone(),
        )
    };
    if became_ready {
        if let (true, Some(url)) = (!work_id.is_empty(), ready_url.as_ref()) {
            let _ = crate::daemon::forge_preview::mint_preview_grant(&work_id, run_id, url).await;
        }
        publish_task_state(run_id, "ready", None).await;
    }
}

async fn publish_task_state(run_id: &str, state: &str, result: Option<ProjectTaskResult>) {
    let Some(handle) = PROJECT_TASK_RUNS.read().await.get(run_id).cloned() else {
        return;
    };
    let mut store = handle.store.lock().await;
    // Readiness must not clobber cancel/terminal states.
    if state == "ready" && store.run.state != "running" {
        return;
    }
    store.run.state = state.to_owned();
    if let Some(result) = result.clone() {
        store.run.output_truncated = store.run.output_truncated || result.truncated;
        if !result.locations.is_empty() {
            merge_task_locations(&mut store.run.locations, result.locations.clone());
        }
        store.run.result = Some(result);
        store.stdout.clear();
        store.stderr.clear();
    }
    let seq = store.run.next_seq;
    store.run.next_seq = seq.saturating_add(1);
    let event = ProjectTaskOutputEvent {
        seq,
        run_id: run_id.to_owned(),
        kind: "state".into(),
        available_from: None,
        stream: None,
        text: None,
        state: Some(state.to_owned()),
        result: store.run.result.clone(),
        locations: if store.run.locations.is_empty() {
            None
        } else {
            Some(store.run.locations.clone())
        },
        ready_url: store.run.ready_url.clone(),
    };
    push_task_event(&mut store, event);
    if task_run_is_terminal(&store.run) {
        *handle
            .terminal_at
            .lock()
            .expect("project task terminal timestamp") = Some(std::time::Instant::now());
    }
}

async fn pump_task_stream(
    run_id: String,
    stream_name: &'static str,
    mut reader: tokio::process::ChildStdout,
) {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                publish_task_output(&run_id, stream_name, &text).await;
            }
            Err(_) => break,
        }
    }
}

async fn pump_task_stderr(run_id: String, mut reader: tokio::process::ChildStderr) {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                publish_task_output(&run_id, "stderr", &text).await;
            }
            Err(_) => break,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RunProjectTaskRequest {
    lease_id: String,
    generation: u64,
    #[serde(default)]
    test_id: Option<String>,
}

fn target_project_test(
    root: &FsPath,
    task: &mut ProjectTask,
    test_id: Option<&str>,
) -> ApiResult<()> {
    let Some(test_id) = test_id else {
        return Ok(());
    };
    let test = discover_project_tests(root, std::slice::from_ref(task))
        .into_iter()
        .find(|test| test.id == test_id && test.task_id == task.id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Test is no longer available"))?;
    let base_id = task.id.split('@').next().unwrap_or(&task.id);
    let task_relative_path = if task.root == "." {
        test.path.clone()
    } else {
        test.path
            .strip_prefix(&format!("{}/", task.root))
            .unwrap_or(&test.path)
            .to_owned()
    };
    if base_id == "cargo-test" {
        task.argv.push(test.label.clone());
    } else if base_id == "python-test" {
        task.argv
            .push(format!("{}::{}", task_relative_path, test.label));
    } else if base_id.ends_with("-test")
        && matches!(task.provider.as_str(), "npm" | "pnpm" | "yarn" | "bun")
    {
        task.argv.extend(["--".into(), task_relative_path]);
    } else if base_id == "go-test" {
        let package = std::path::Path::new(&task_relative_path)
            .parent()
            .map(|path| format!("./{}", path.display()))
            .unwrap_or_else(|| ".".into());
        task.argv
            .extend([package, "-run".into(), format!("^{}$", test.label)]);
    }
    task.label = format!("Test {}", test.label);
    Ok(())
}

fn scoped_task_id(base: &str, task_root: &str) -> String {
    if task_root == "." {
        return base.into();
    }
    let digest = format!("{:x}", Sha256::digest(task_root.as_bytes()));
    format!(
        "{base}@{}-{}",
        sanitize_task_id_slug(task_root),
        &digest[..8]
    )
}

fn detected_task_default_rank(id: &str, kind: &str) -> i32 {
    match (kind, id) {
        ("run", id) if id.ends_with("-dev") || id == "make-dev" => 500,
        ("run", _) => 450,
        ("build", _) => 300,
        ("test", _) => 220,
        ("verify", _) => 180,
        _ => 100,
    }
}

fn configured_task_default_rank(kind: &str, background: bool) -> i32 {
    if background {
        425
    } else {
        match kind {
            "run" => 400,
            "build" => 280,
            "test" => 200,
            "verify" => 160,
            _ => 90,
        }
    }
}

fn detected_task_is_background(id: &str) -> bool {
    id.ends_with("-dev") || id.ends_with("-start") || id.ends_with("-serve") || id == "make-dev"
}

fn add_detected_task(
    tasks: &mut Vec<ProjectTask>,
    task_root: &str,
    id: &str,
    label: &str,
    kind: &str,
    argv: impl IntoIterator<Item = impl Into<String>>,
) {
    if tasks.len() >= DETECTED_PROJECT_TASK_CAP {
        return;
    }
    let argv = argv.into_iter().map(Into::into).collect::<Vec<_>>();
    let source = if matches!(
        argv.first().map(String::as_str),
        Some("npm" | "pnpm" | "yarn" | "bun" | "uv" | "poetry")
    ) {
        "package"
    } else {
        "detected"
    };
    tasks.push(ProjectTask {
        version: default_project_task_version(),
        id: scoped_task_id(id, task_root),
        label: label.into(),
        kind: kind.into(),
        provider: argv.first().cloned().unwrap_or_else(|| "project".into()),
        source: source.into(),
        argv,
        root: task_root.into(),
        interactive: false,
        background: detected_task_is_background(id),
        default_rank: detected_task_default_rank(id, kind),
        available: true,
        requirements: Vec::new(),
        long_running: kind == "run",
        ready_pattern: None,
        problem_matcher: None,
    });
}

fn detected_project_roots(root: &FsPath) -> Vec<String> {
    let mut roots = std::collections::BTreeSet::from([".".to_string()]);
    let Ok(output) = background_command("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()
    else {
        return roots.into_iter().collect();
    };
    if !output.status.success() {
        return roots.into_iter().collect();
    }
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let relative = String::from_utf8_lossy(raw).replace('\\', "/");
        let path = FsPath::new(&relative);
        if path.components().count() > 8 {
            continue;
        }
        let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        let is_marker = matches!(
            file_name,
            "Cargo.toml" | "go.mod" | "package.json" | "pyproject.toml" | "pytest.ini" | "Makefile"
        ) || matches!(
            path.extension().and_then(OsStr::to_str),
            Some("sln" | "csproj")
        );
        if !is_marker {
            continue;
        }
        let parent = path.parent().unwrap_or_else(|| FsPath::new(""));
        let clean = parent.to_string_lossy().trim_matches('/').to_owned();
        roots.insert(if clean.is_empty() { ".".into() } else { clean });
        if roots.len() >= DETECTED_PROJECT_ROOT_CAP {
            break;
        }
    }
    let mut roots = roots.into_iter().collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        left.matches('/')
            .count()
            .cmp(&right.matches('/').count())
            .then(left.cmp(right))
    });
    roots
}

fn package_manager(
    repository_root: &FsPath,
    project_root: &FsPath,
) -> (&'static str, &'static str) {
    let mut current = Some(project_root);
    while let Some(directory) = current {
        if directory.join("bun.lock").is_file() || directory.join("bun.lockb").is_file() {
            return ("bun", "bun");
        }
        if directory.join("pnpm-lock.yaml").is_file() {
            return ("pnpm", "pnpm");
        }
        if directory.join("yarn.lock").is_file() {
            return ("yarn", "yarn");
        }
        if directory == repository_root {
            break;
        }
        current = directory
            .parent()
            .filter(|parent| parent.starts_with(repository_root));
    }
    ("npm", "npm")
}

fn cargo_named_targets(
    manifest: Option<&toml::Value>,
    project_root: &FsPath,
    kind: &str,
) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    if let Some(entries) = manifest
        .and_then(|value| value.get(kind))
        .and_then(toml::Value::as_array)
    {
        for name in entries
            .iter()
            .filter_map(|entry| entry.get("name"))
            .filter_map(toml::Value::as_str)
            .filter(|name| npm_script_is_safe(name))
        {
            names.insert(name.to_owned());
        }
    }
    let directory = match kind {
        "bin" => project_root.join("src/bin"),
        "example" => project_root.join("examples"),
        _ => return names.into_iter().take(8).collect(),
    };
    for entry in std::fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = entry.path();
        let name = if path.extension().and_then(OsStr::to_str) == Some("rs") {
            path.file_stem().and_then(OsStr::to_str)
        } else if path.is_dir() && path.join("main.rs").is_file() {
            path.file_name().and_then(OsStr::to_str)
        } else {
            None
        };
        if let Some(name) = name.filter(|name| npm_script_is_safe(name)) {
            names.insert(name.to_owned());
        }
        if names.len() >= 8 {
            break;
        }
    }
    names.into_iter().take(8).collect()
}

fn python_project_scripts(manifest: Option<&toml::Value>) -> Vec<String> {
    manifest
        .and_then(|value| value.get("project"))
        .and_then(|value| value.get("scripts"))
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|scripts| scripts.keys())
        .filter(|name| npm_script_is_safe(name))
        .take(8)
        .cloned()
        .collect()
}

fn preferred_python_executable() -> &'static str {
    if task_executable_available("python") || !task_executable_available("python3") {
        "python"
    } else {
        "python3"
    }
}

fn dotnet_runnable_projects(project_root: &FsPath) -> Vec<String> {
    std::fs::read_dir(project_root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(OsStr::to_str) != Some("csproj") {
                return None;
            }
            let raw = std::fs::read_to_string(&path).ok()?;
            let runnable = raw.contains("<OutputType>Exe</OutputType>")
                || raw.contains("<OutputType>WinExe</OutputType>")
                || raw.contains("Sdk=\"Microsoft.NET.Sdk.Web\"")
                || raw.contains("Sdk='Microsoft.NET.Sdk.Web'");
            runnable
                .then(|| path.file_name()?.to_str().map(str::to_owned))
                .flatten()
        })
        .take(8)
        .collect()
}

fn detected_tasks_for_root(
    repository_root: &FsPath,
    task_root: &str,
    tasks: &mut Vec<ProjectTask>,
) {
    let project_root = if task_root == "." {
        repository_root.to_path_buf()
    } else {
        repository_root.join(task_root)
    };
    if project_root.join("Cargo.toml").is_file() {
        add_detected_task(
            tasks,
            task_root,
            "cargo-build",
            "Build",
            "build",
            ["cargo", "build"],
        );
        add_detected_task(
            tasks,
            task_root,
            "cargo-check",
            "Check",
            "verify",
            ["cargo", "check"],
        );
        add_detected_task(
            tasks,
            task_root,
            "cargo-test",
            "Test",
            "test",
            ["cargo", "test"],
        );
        let manifest = std::fs::read_to_string(project_root.join("Cargo.toml"))
            .ok()
            .and_then(|raw| raw.parse::<toml::Value>().ok());
        let default_run = manifest
            .as_ref()
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("default-run"))
            .and_then(toml::Value::as_str);
        if let Some(binary) = default_run {
            add_detected_task(
                tasks,
                task_root,
                "cargo-run",
                "Run project",
                "run",
                ["cargo", "run", "--bin", binary],
            );
        } else if project_root.join("src/main.rs").is_file() {
            add_detected_task(
                tasks,
                task_root,
                "cargo-run",
                "Run project",
                "run",
                ["cargo", "run"],
            );
        } else if let Some(binary) = manifest
            .as_ref()
            .and_then(|value| value.get("bin"))
            .and_then(toml::Value::as_array)
            .filter(|bins| bins.len() == 1)
            .and_then(|bins| bins[0].get("name"))
            .and_then(toml::Value::as_str)
        {
            add_detected_task(
                tasks,
                task_root,
                "cargo-run",
                "Run project",
                "run",
                ["cargo", "run", "--bin", binary],
            );
        }
        for binary in cargo_named_targets(manifest.as_ref(), &project_root, "bin") {
            add_detected_task(
                tasks,
                task_root,
                &format!("cargo-run-bin-{}", sanitize_task_id_slug(&binary)),
                &format!("Run binary: {binary}"),
                "run",
                vec!["cargo".into(), "run".into(), "--bin".into(), binary],
            );
        }
        for example in cargo_named_targets(manifest.as_ref(), &project_root, "example") {
            add_detected_task(
                tasks,
                task_root,
                &format!("cargo-run-example-{}", sanitize_task_id_slug(&example)),
                &format!("Run example: {example}"),
                "run",
                vec!["cargo".into(), "run".into(), "--example".into(), example],
            );
        }
    }
    if project_root.join("go.mod").is_file() {
        add_detected_task(
            tasks,
            task_root,
            "go-build",
            "Build",
            "build",
            ["go", "build", "./..."],
        );
        add_detected_task(
            tasks,
            task_root,
            "go-test",
            "Test",
            "test",
            ["go", "test", "./..."],
        );
        let has_main = std::fs::read_dir(&project_root)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| {
                entry.path().extension().and_then(OsStr::to_str) == Some("go")
                    && std::fs::read_to_string(entry.path())
                        .is_ok_and(|raw| raw.lines().any(|line| line.trim() == "package main"))
            });
        if has_main {
            add_detected_task(
                tasks,
                task_root,
                "go-run",
                "Run project",
                "run",
                ["go", "run", "."],
            );
        }
    }
    if project_root.join("package.json").is_file() {
        let scripts = std::fs::read(project_root.join("package.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("scripts")
                    .and_then(|scripts| scripts.as_object())
                    .cloned()
            })
            .unwrap_or_default();
        let (manager, id_prefix) = package_manager(repository_root, &project_root);
        for (script, label, kind) in [
            ("build", "Build", "build"),
            ("check", "Check", "verify"),
            ("lint", "Lint", "verify"),
            ("test", "Test", "test"),
            ("typecheck", "Type check", "verify"),
        ] {
            if scripts.contains_key(script) {
                add_detected_task(
                    tasks,
                    task_root,
                    &format!("{id_prefix}-{script}"),
                    label,
                    kind,
                    [manager, "run", script],
                );
            }
        }
        let run_script = ["dev", "start", "serve"]
            .into_iter()
            .find(|script| scripts.contains_key(*script));
        if let Some(script) = run_script {
            add_detected_task(
                tasks,
                task_root,
                &format!("{id_prefix}-{script}"),
                if script == "dev" {
                    "Development server"
                } else {
                    "Run project"
                },
                "run",
                [manager, "run", script],
            );
        }
    }
    if project_root.join("pyproject.toml").is_file() || project_root.join("pytest.ini").is_file() {
        let python = preferred_python_executable();
        let manifest = std::fs::read_to_string(project_root.join("pyproject.toml"))
            .ok()
            .and_then(|raw| raw.parse::<toml::Value>().ok());
        add_detected_task(
            tasks,
            task_root,
            "python-test",
            "Test",
            "test",
            [python, "-m", "pytest"],
        );
        if project_root.join("main.py").is_file() {
            add_detected_task(
                tasks,
                task_root,
                "python-run",
                "Run project",
                "run",
                [python, "main.py"],
            );
        } else if project_root.join("__main__.py").is_file() {
            add_detected_task(
                tasks,
                task_root,
                "python-run",
                "Run project",
                "run",
                [python, "__main__.py"],
            );
        } else if let Some(module) = std::fs::read_dir(&project_root)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .find_map(|entry| {
                let path = entry.path();
                (path.is_dir() && path.join("__main__.py").is_file())
                    .then(|| path.file_name()?.to_str().map(str::to_owned))
                    .flatten()
            })
        {
            add_detected_task(
                tasks,
                task_root,
                "python-run",
                "Run project",
                "run",
                vec![python.into(), "-m".into(), module],
            );
        }
        let script_runner = if project_root.join("uv.lock").is_file() {
            Some("uv")
        } else if project_root.join("poetry.lock").is_file() {
            Some("poetry")
        } else {
            None
        };
        if let Some(runner) = script_runner {
            for script in python_project_scripts(manifest.as_ref()) {
                add_detected_task(
                    tasks,
                    task_root,
                    &format!("python-script-{}", sanitize_task_id_slug(&script)),
                    &format!("Run script: {script}"),
                    "run",
                    vec![runner.into(), "run".into(), script],
                );
            }
        }
    }
    if project_root.join("Makefile").is_file() {
        let makefile = std::fs::read_to_string(project_root.join("Makefile")).unwrap_or_default();
        for (target, label, kind) in [
            ("build", "Build", "build"),
            ("check", "Check", "verify"),
            ("test", "Test", "test"),
            ("run", "Run project", "run"),
            ("dev", "Development server", "run"),
        ] {
            if makefile
                .lines()
                .any(|line| line.starts_with(&format!("{target}:")))
            {
                add_detected_task(
                    tasks,
                    task_root,
                    &format!("make-{target}"),
                    label,
                    kind,
                    ["make", target],
                );
            }
        }
    }
    if std::fs::read_dir(&project_root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            matches!(
                entry.path().extension().and_then(OsStr::to_str),
                Some("sln" | "csproj")
            )
        })
    {
        add_detected_task(
            tasks,
            task_root,
            "dotnet-build",
            "Build",
            "build",
            ["dotnet", "build"],
        );
        add_detected_task(
            tasks,
            task_root,
            "dotnet-test",
            "Test",
            "test",
            ["dotnet", "test"],
        );
        let runnable_projects = dotnet_runnable_projects(&project_root);
        let one_runnable_project = runnable_projects.len() == 1;
        for project in runnable_projects {
            let label = if one_runnable_project {
                "Run project".into()
            } else {
                format!("Run {project}")
            };
            add_detected_task(
                tasks,
                task_root,
                &format!("dotnet-run-{}", sanitize_task_id_slug(&project)),
                &label,
                "run",
                vec!["dotnet".into(), "run".into(), "--project".into(), project],
            );
        }
    }
}

fn detected_project_tasks(root: &FsPath) -> Vec<ProjectTask> {
    let mut tasks = Vec::new();
    for task_root in detected_project_roots(root) {
        detected_tasks_for_root(root, &task_root, &mut tasks);
        if tasks.len() >= DETECTED_PROJECT_TASK_CAP {
            break;
        }
    }
    tasks
}

fn strip_tasks_jsonc(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn load_tasks_json_value(root: &FsPath) -> Option<serde_json::Value> {
    let bytes = std::fs::read(root.join(".vscode").join("tasks.json")).ok()?;
    if let Ok(value) = serde_json::from_slice(&bytes) {
        return Some(value);
    }
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_str(&strip_tasks_jsonc(&text)).ok()
}

fn sanitize_task_id_slug(label: &str) -> String {
    let slug = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "task".into()
    } else {
        slug.chars().take(48).collect()
    }
}

fn npm_script_is_safe(script: &str) -> bool {
    !script.is_empty()
        && script.len() <= 64
        && script
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
}

fn infer_configured_task_kind(label: &str, script: Option<&str>, background: bool) -> &'static str {
    if background {
        return "run";
    }
    let hay = format!("{} {}", label, script.unwrap_or("")).to_ascii_lowercase();
    if hay.contains("test") {
        "test"
    } else if hay.contains("build") || hay.contains("compile") {
        "build"
    } else {
        "verify"
    }
}

fn parse_problem_matcher_value(value: &serde_json::Value) -> Option<ProjectProblemPattern> {
    let pattern = if value.is_object() && value.get("pattern").is_some() {
        value.get("pattern")?
    } else {
        value
    };
    let regexp = pattern.get("regexp")?.as_str()?.to_owned();
    if regexp.trim().is_empty() || regexp.len() > 512 {
        return None;
    }
    let file = pattern.get("file")?.as_u64()? as u8;
    let line = pattern.get("line")?.as_u64()? as u8;
    if file == 0 || line == 0 {
        return None;
    }
    Some(ProjectProblemPattern {
        regexp,
        file,
        line,
        column: pattern
            .get("column")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .filter(|v| *v > 0),
        message: pattern
            .get("message")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .filter(|v| *v > 0),
    })
}

fn first_problem_matcher(value: &serde_json::Value) -> Option<ProjectProblemPattern> {
    match value {
        serde_json::Value::Array(items) => items.iter().find_map(parse_problem_matcher_value),
        other => parse_problem_matcher_value(other),
    }
}

fn configured_task_ready_pattern(task: &serde_json::Value) -> Option<String> {
    let matcher = task.get("problemMatcher")?;
    let matchers = match matcher {
        serde_json::Value::Array(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    for entry in matchers {
        if let Some(pattern) = entry
            .get("background")
            .and_then(|bg| bg.get("endsPattern"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 512)
        {
            return Some(pattern.to_owned());
        }
        if let Some(pattern) = entry
            .get("background")
            .and_then(|bg| bg.get("beginsPattern"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 512)
        {
            // Prefer endsPattern; beginsPattern alone is still a useful readiness hint.
            return Some(pattern.to_owned());
        }
    }
    None
}

fn configured_project_tasks(root: &FsPath) -> Vec<ProjectTask> {
    let Some(document) = load_tasks_json_value(root) else {
        return Vec::new();
    };
    let Some(items) = document.get("tasks").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut tasks = Vec::new();
    let mut used_ids = std::collections::HashSet::new();
    for item in items.iter().take(CONFIGURED_TASK_CAP.saturating_mul(2)) {
        if tasks.len() >= CONFIGURED_TASK_CAP {
            break;
        }
        let label = item
            .get("label")
            .or_else(|| item.get("script"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("");
        if label.is_empty() {
            continue;
        }
        let task_type = item
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("shell")
            .to_ascii_lowercase();
        let background = item
            .get("isBackground")
            .or_else(|| item.get("is_background"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || item
                .get("problemMatcher")
                .map(|matcher| match matcher {
                    serde_json::Value::Array(items) => {
                        items.iter().any(|entry| entry.get("background").is_some())
                    }
                    other => other.get("background").is_some(),
                })
                .unwrap_or(false);
        let (argv, script_name) = match task_type.as_str() {
            "npm" => {
                let script = item
                    .get("script")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap_or("");
                if !npm_script_is_safe(script) {
                    continue;
                }
                (
                    vec!["npm".into(), "run".into(), script.to_owned()],
                    Some(script.to_owned()),
                )
            }
            "process" | "shell" => {
                let Some(command) = item
                    .get("command")
                    .and_then(|v| {
                        v.as_str().map(str::to_owned).or_else(|| {
                            v.get("value")
                                .and_then(|inner| inner.as_str())
                                .map(str::to_owned)
                        })
                    })
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let args = item
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if args.is_empty() && command.contains(char::is_whitespace) && task_type == "shell"
                {
                    (vec!["sh".into(), "-c".into(), command], None)
                } else {
                    let mut argv = vec![command];
                    argv.extend(args);
                    (argv, None)
                }
            }
            _ => continue,
        };
        if argv.is_empty() || argv.len() > 32 || argv.iter().any(|part| part.len() > 512) {
            continue;
        }
        let mut id = format!("configured-{}", sanitize_task_id_slug(label));
        let mut suffix = 2u32;
        while !used_ids.insert(id.clone()) {
            id = format!("configured-{}-{suffix}", sanitize_task_id_slug(label));
            suffix = suffix.saturating_add(1);
        }
        let kind = infer_configured_task_kind(label, script_name.as_deref(), background);
        tasks.push(ProjectTask {
            version: default_project_task_version(),
            id,
            label: label.to_owned(),
            kind: kind.into(),
            argv,
            provider: "vscode-tasks".into(),
            source: "vscode-task".into(),
            root: ".".into(),
            interactive: false,
            background,
            default_rank: configured_task_default_rank(kind, background),
            available: true,
            requirements: Vec::new(),
            long_running: background || kind == "run",
            ready_pattern: configured_task_ready_pattern(item),
            problem_matcher: item.get("problemMatcher").and_then(first_problem_matcher),
        });
    }
    tasks
}

fn executable_repair(executable: &str) -> String {
    format!(
        "Install {executable} on the workshop machine and make it available on PATH, then reload the project commands."
    )
}

fn task_executable_available(executable: &str) -> bool {
    let path = FsPath::new(executable);
    if path.components().count() > 1 {
        return path.is_file();
    }
    let Some(search_path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&search_path).any(|directory| {
        let candidate = directory.join(executable);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            return ["exe", "cmd", "bat", "com"].into_iter().any(|extension| {
                directory
                    .join(format!("{executable}.{extension}"))
                    .is_file()
            });
        }
        #[cfg(not(windows))]
        false
    })
}

fn javascript_dependency_requirement(
    repository_root: &FsPath,
    task: &ProjectTask,
) -> Option<ProjectTaskRequirement> {
    let manager = task.argv.first()?.as_str();
    if !matches!(manager, "npm" | "pnpm" | "yarn" | "bun") {
        return None;
    }
    let project_root = if task.root == "." {
        repository_root.to_path_buf()
    } else {
        repository_root.join(&task.root)
    };
    let package = std::fs::read(project_root.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())?;
    let has_dependencies = ["dependencies", "devDependencies", "optionalDependencies"]
        .into_iter()
        .any(|key| {
            package
                .get(key)
                .and_then(serde_json::Value::as_object)
                .is_some_and(|dependencies| !dependencies.is_empty())
        });
    if !has_dependencies {
        return None;
    }
    let mut install_root = project_root.as_path();
    let mut current = Some(project_root.as_path());
    while let Some(directory) = current {
        if [
            "bun.lock",
            "bun.lockb",
            "pnpm-lock.yaml",
            "yarn.lock",
            "package-lock.json",
        ]
        .into_iter()
        .any(|name| directory.join(name).is_file())
        {
            install_root = directory;
            break;
        }
        if directory == repository_root {
            break;
        }
        current = directory
            .parent()
            .filter(|parent| parent.starts_with(repository_root));
    }
    let available = install_root.join("node_modules").is_dir()
        || install_root.join(".pnp.cjs").is_file()
        || install_root.join(".pnp.loader.mjs").is_file();
    let display_root = install_root
        .strip_prefix(repository_root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| "project root".into());
    Some(ProjectTaskRequirement {
        kind: "package".into(),
        name: "project dependencies".into(),
        available,
        repair: (!available).then(|| {
            format!(
                "Run `{manager} install` from {display_root} on the workshop machine, then reload the project commands."
            )
        }),
    })
}

fn annotate_task_requirements(repository_root: &FsPath, tasks: &mut [ProjectTask]) {
    let mut availability = std::collections::HashMap::<String, bool>::new();
    for task in tasks {
        let Some(executable) = task.argv.first().cloned() else {
            task.available = false;
            task.requirements = vec![ProjectTaskRequirement {
                kind: "executable".into(),
                name: "command".into(),
                available: false,
                repair: Some("Choose a task with a configured executable.".into()),
            }];
            continue;
        };
        let available = *availability
            .entry(executable.clone())
            .or_insert_with(|| task_executable_available(&executable));
        let mut requirements = vec![ProjectTaskRequirement {
            kind: "executable".into(),
            name: executable.clone(),
            available,
            repair: (!available).then(|| executable_repair(&executable)),
        }];
        if let Some(requirement) = javascript_dependency_requirement(repository_root, task) {
            requirements.push(requirement);
        }
        task.available = requirements.iter().all(|requirement| requirement.available);
        task.requirements = requirements;
    }
}

fn unavailable_task_message(task: &ProjectTask) -> Option<String> {
    if task.available {
        return None;
    }
    task.requirements
        .iter()
        .find_map(|requirement| requirement.repair.clone())
        .or_else(|| {
            Some(format!(
                "{} is unavailable on the workshop machine",
                task.label
            ))
        })
}

fn project_tasks(root: &FsPath) -> Vec<ProjectTask> {
    let mut tasks = detected_project_tasks(root);
    let mut seen_ids = tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let seen_argv = tasks
        .iter()
        .map(|task| (task.root.clone(), task.argv.clone()))
        .collect::<std::collections::HashSet<_>>();
    for task in configured_project_tasks(root) {
        if !seen_ids.insert(task.id.clone())
            || seen_argv.contains(&(task.root.clone(), task.argv.clone()))
        {
            continue;
        }
        tasks.push(task);
    }
    annotate_task_requirements(root, &mut tasks);
    tasks
}

fn resolve_project_task_root(repository_root: &FsPath, task_root: &str) -> ApiResult<PathBuf> {
    let repository_root = std::fs::canonicalize(repository_root).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("Project workspace is unavailable: {err}"),
        )
    })?;
    let relative = FsPath::new(task_root);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "Project command has an invalid working root",
        ));
    }
    let candidate = std::fs::canonicalize(repository_root.join(relative)).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("Project command root is unavailable: {err}"),
        )
    })?;
    if !candidate.starts_with(&repository_root) || !candidate.is_dir() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "Project command root leaves the workspace",
        ));
    }
    Ok(candidate)
}

fn normalize_output_path(
    repository_root: &FsPath,
    working_root: &FsPath,
    raw: &FsPath,
) -> Option<String> {
    let relative = if raw.is_absolute() {
        raw.strip_prefix(repository_root).ok()?.to_path_buf()
    } else {
        working_root
            .join(raw)
            .strip_prefix(repository_root)
            .ok()?
            .to_path_buf()
    };
    if relative
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return None;
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn parse_output_locations(
    repository_root: &FsPath,
    working_root: &FsPath,
    output: &str,
) -> Vec<ProjectOutputLocation> {
    let mut locations = Vec::new();
    for line_text in output.lines() {
        let Some(token) = line_text
            .split_whitespace()
            .map(|token| {
                token.trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | ',' | ':'))
            })
            .find(|token| {
                let mut parts = token.rsplitn(3, ':');
                parts.next().is_some_and(|part| part.parse::<u32>().is_ok())
                    && parts
                        .next()
                        .is_some_and(|part| part.parse::<u32>().is_ok() || part.contains('.'))
            })
        else {
            continue;
        };
        let mut parts = token.rsplitn(3, ':');
        let last = parts.next().unwrap_or("1");
        let middle = parts.next().unwrap_or("1");
        let (raw, line, column) = if let Some(path) = parts.next() {
            (path, middle.parse().unwrap_or(1), last.parse().ok())
        } else {
            let Some((path, line)) = token.rsplit_once(':') else {
                continue;
            };
            (path, line.parse().unwrap_or(1), None)
        };
        let raw = raw.trim_start_matches("-->");
        let path = std::path::Path::new(raw);
        let Some(path) = normalize_output_path(repository_root, working_root, path) else {
            continue;
        };
        let message = line_text.trim().chars().take(300).collect();
        locations.push(ProjectOutputLocation {
            path,
            line,
            column,
            message,
        });
        if locations.len() >= 100 {
            break;
        }
    }
    locations
}

async fn list_project_tasks(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<Vec<ProjectTask>>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                let root = item
                    .workspace_environment()
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "Set up this project before running it",
                        )
                    })?
                    .worktree
                    .clone();
                Ok(Json(project_tasks(&root)))
            }
        },
    )
    .await
}

fn discover_project_tests(root: &FsPath, tasks: &[ProjectTask]) -> Vec<ProjectTest> {
    let Some(task) = tasks
        .iter()
        .find(|task| task.kind == "test" || task.id.ends_with("-test"))
    else {
        return Vec::new();
    };
    let mut tests = Vec::new();
    let task_prefix = if task.root == "." {
        None
    } else {
        Some(format!("{}/", task.root))
    };
    let tree = list_source_tree(&WorkId::from("test-discovery".to_string()), root).ok();
    for file in tree.into_iter().flat_map(|tree| tree.files).take(20_000) {
        if task_prefix
            .as_ref()
            .is_some_and(|prefix| !file.path.starts_with(prefix))
        {
            continue;
        }
        let path = root.join(&file.path);
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        if !matches!(
            extension.as_str(),
            "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go"
        ) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut previous_test_attribute = false;
        for (index, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let name = if extension == "rs" && previous_test_attribute {
                trimmed.strip_prefix("fn ").and_then(|rest| {
                    rest.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                        .next()
                })
            } else if extension == "py" {
                trimmed
                    .strip_prefix("def test_")
                    .and_then(|rest| rest.split('(').next())
                    .map(|name| &trimmed[4..4 + 5 + name.len()])
            } else if matches!(extension.as_str(), "js" | "jsx" | "ts" | "tsx")
                && (trimmed.starts_with("test(") || trimmed.starts_with("it("))
            {
                trimmed.split(['\'', '"']).nth(1)
            } else if extension == "go" {
                trimmed
                    .strip_prefix("func Test")
                    .and_then(|rest| rest.split('(').next())
                    .map(|name| &trimmed[5..5 + 4 + name.len()])
            } else {
                None
            };
            if let Some(name) = name.filter(|name| !name.is_empty()) {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                tests.push(ProjectTest {
                    id: format!("{}::{name}", relative),
                    label: name.to_string(),
                    path: relative,
                    line: (index + 1) as u32,
                    task_id: task.id.clone(),
                });
                if tests.len() >= 2_000 {
                    return tests;
                }
            }
            previous_test_attribute = extension == "rs" && trimmed == "#[test]";
        }
    }
    tests
}

async fn list_project_tests(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ReviewSelectionQuery>,
) -> ApiResult<Json<Vec<ProjectTest>>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::Observation,
        256 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let item = forge(&state).load(&id).map_err(map_err)?;
                let selected_attempt = query.attempt_id.as_deref().map(|attempt_id| {
                    medousa_forge::model::AttemptId::from(attempt_id.to_string())
                });
                if selected_attempt
                    .as_ref()
                    .is_some_and(|attempt_id| item.attempt(attempt_id).is_none())
                {
                    return Err(request_error(
                        StatusCode::NOT_FOUND,
                        "test-discovery attempt does not belong to this undertaking",
                    ));
                }
                let root = selected_attempt
                    .as_ref()
                    .and_then(|attempt_id| item.environment_for_attempt(attempt_id))
                    .or_else(|| item.workspace_environment())
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "Set up this project before finding tests",
                        )
                    })?
                    .worktree
                    .clone();
                let tasks = project_tasks(&root);
                Ok(Json(discover_project_tests(&root, &tasks)))
            }
        },
    )
    .await
}

async fn start_project_task_run(
    State(state): State<AppState>,
    Path((work_id, task_id)): Path<(String, String)>,
    Json(body): Json<RunProjectTaskRequest>,
) -> ApiResult<Json<ProjectTaskRun>> {
    let id = parse_work_id(&work_id)?;
    let (item, lease, root, working_root, task) = admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            let lease_id = body.lease_id.clone();
            let generation = body.generation;
            let task_id = task_id.clone();
            let test_id = body.test_id.clone();
            move || {
                let (item, lease) = require_work_lease(&state, &id, &lease_id, generation)?;
                let root = item
                    .environment_for_attempt(&lease.attempt_id)
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "Set up this project before running it",
                        )
                    })?
                    .worktree
                    .clone();
                let mut task = project_tasks(&root)
                    .into_iter()
                    .find(|task| task.id == task_id)
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::NOT_FOUND,
                            "Project command is no longer available",
                        )
                    })?;
                if let Some(message) = unavailable_task_message(&task) {
                    return Err(request_error(StatusCode::CONFLICT, message));
                }
                target_project_test(&root, &mut task, test_id.as_deref())?;
                let working_root = resolve_project_task_root(&root, &task.root)?;
                Ok((item, lease, root, working_root, task))
            }
        },
    )
    .await?;
    let forge = forge(&state);
    let run_id = format!("run-{}", uuid::Uuid::new_v4());
    let ready_re = compile_ready_pattern(&task);
    let problem_re = compile_problem_pattern(&task);
    let run = ProjectTaskRun {
        run_id: run_id.clone(),
        work_id: work_id.clone(),
        state: "running".into(),
        task: task.clone(),
        result: None,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        next_seq: 0,
        locations: Vec::new(),
        ready_url: None,
    };
    let memory_permit = Arc::clone(&PROJECT_TASK_MEMORY)
        .try_acquire_many_owned(PROJECT_TASK_RUN_MEMORY_RESERVATION)
        .map_err(|_| {
            request_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Project task memory budget is exhausted; wait for an earlier run to expire",
            )
        })?;
    let mut child = background_tokio_command(&task.argv[0])
        .args(&task.argv[1..])
        .current_dir(&working_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| {
            request_error(
                StatusCode::BAD_REQUEST,
                format!("Could not run {}: {err}", task.label),
            )
        })?;
    let (tx, _) = tokio::sync::broadcast::channel(256);
    {
        let mut runs = PROJECT_TASK_RUNS.write().await;
        prune_project_task_runs(&mut runs);
        if runs.len() >= PROJECT_TASK_RUN_CAP {
            return Err(request_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Project task run registry is full; wait for a terminal run to expire",
            ));
        }
        runs.insert(
            run_id.clone(),
            Arc::new(ProjectTaskRunHandle {
                store: tokio::sync::Mutex::new(ProjectTaskRunStore {
                    run: run.clone(),
                    repository_root: root.clone(),
                    working_root: working_root.clone(),
                    ready_re,
                    problem_re,
                    stdout: TaskOutputTail::default(),
                    stderr: TaskOutputTail::default(),
                    chunks: std::collections::VecDeque::new(),
                    chunk_bytes: 0,
                    tx,
                }),
                terminal_at: Mutex::new(None),
                _memory_permit: memory_permit,
            }),
        );
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Some(stdout) = stdout {
        tokio::spawn(pump_task_stream(run_id.clone(), "stdout", stdout));
    }
    if let Some(stderr) = stderr {
        tokio::spawn(pump_task_stderr(run_id.clone(), stderr));
    }
    let child = Arc::new(tokio::sync::Mutex::new(child));
    PROJECT_TASK_CHILDREN
        .write()
        .await
        .insert(run_id.clone(), Arc::clone(&child));
    let state_for_run = state.clone();
    let run_id_for_task = run_id.clone();
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        loop {
            let status = { child.lock().await.try_wait().ok().flatten() };
            if let Some(status) = status {
                // Give stream pumps a moment to flush final bytes.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let (stdout, stderr, output_truncated, prior_locations, problem_re) = {
                    let handle = PROJECT_TASK_RUNS
                        .read()
                        .await
                        .get(&run_id_for_task)
                        .cloned();
                    if let Some(handle) = handle {
                        let store = handle.store.lock().await;
                        (
                            store.stdout.materialize(),
                            store.stderr.materialize(),
                            store.run.output_truncated,
                            store.run.locations.clone(),
                            store.problem_re.clone(),
                        )
                    } else {
                        (String::new(), String::new(), false, Vec::new(), None)
                    }
                };
                let mut locations = prior_locations;
                merge_task_locations(
                    &mut locations,
                    parse_output_locations_with_matcher(
                        &root,
                        &working_root,
                        &format!("{stdout}\n{stderr}"),
                        problem_re.as_ref(),
                    ),
                );
                let result = ProjectTaskResult {
                    task: task.clone(),
                    success: status.success(),
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    truncated: output_truncated,
                    duration_ms: started.elapsed().as_millis(),
                    locations,
                };
                let handle = PROJECT_TASK_RUNS
                    .read()
                    .await
                    .get(&run_id_for_task)
                    .cloned();
                let cancelled = if let Some(handle) = handle {
                    let store = handle.store.lock().await;
                    matches!(store.run.state.as_str(), "cancelled")
                } else {
                    false
                };
                let _ = forge.append_command_log(&lease, &serde_json::json!({"kind":if cancelled {"project_task_cancelled"} else {"project_task"},"run_id":run_id_for_task,"task":result.task,"success":result.success,"exit_code":result.exit_code,"duration_ms":result.duration_ms,"stdout":result.stdout,"stderr":result.stderr,"truncated":result.truncated,"locations":result.locations}));
                let final_state = if cancelled {
                    "cancelled"
                } else if result.success {
                    "passed"
                } else {
                    "failed"
                };
                publish_task_state(&run_id_for_task, final_state, Some(result)).await;
                PROJECT_TASK_CHILDREN.write().await.remove(&run_id_for_task);
                publish_item(&state_for_run, &item, "task_finished");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    Ok(Json(run))
}

async fn get_project_task_run(
    Path((work_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<ProjectTaskRun>> {
    let run = project_task_run_snapshot(&run_id)
        .await
        .filter(|run| run.work_id == work_id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run was not found"))?;
    Ok(Json(run))
}

#[derive(Debug, Serialize)]
struct TaskRunPreviewResponse {
    work_id: String,
    run_id: String,
    ready_url: String,
    port: u16,
    token: String,
    preview_path: String,
}

async fn create_task_run_preview(
    Path((work_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<TaskRunPreviewResponse>> {
    let run = project_task_run_snapshot(&run_id)
        .await
        .filter(|run| run.work_id == work_id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run was not found"))?;
    let ready_url = run.ready_url.clone().ok_or_else(|| {
        request_error(
            StatusCode::CONFLICT,
            "This run is not ready for Browser preview yet",
        )
    })?;
    let port = crate::daemon::forge_preview::port_from_ready_url(&ready_url).ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "Could not determine the preview port")
    })?;
    let token = match crate::daemon::forge_preview::preview_token_for_run(&work_id, &run_id).await {
        Some(token) => token,
        None => crate::daemon::forge_preview::mint_preview_grant(&work_id, &run_id, &ready_url)
            .await
            .ok_or_else(|| request_error(StatusCode::CONFLICT, "Could not mint a preview grant"))?,
    };
    Ok(Json(TaskRunPreviewResponse {
        work_id,
        run_id,
        ready_url,
        port,
        preview_path: crate::daemon::forge_preview::preview_path_for_token(&token),
        token,
    }))
}

async fn cancel_project_task_run(
    Path((work_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<ProjectTaskRun>> {
    if project_task_run_snapshot(&run_id)
        .await
        .is_none_or(|run| run.work_id != work_id)
    {
        return Err(request_error(
            StatusCode::NOT_FOUND,
            "Project run was not found",
        ));
    }
    let child = PROJECT_TASK_CHILDREN
        .read()
        .await
        .get(&run_id)
        .cloned()
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run is no longer active"))?;
    child.lock().await.start_kill().map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not stop project run: {err}"),
        )
    })?;
    publish_task_state(&run_id, "cancelled", None).await;
    let run = project_task_run_snapshot(&run_id)
        .await
        .filter(|run| run.work_id == work_id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run was not found"))?;
    Ok(Json(run))
}

#[derive(Debug, Deserialize)]
struct TaskRunEventsQuery {
    #[serde(default)]
    since: Option<u64>,
}

async fn project_task_run_events(
    Path((work_id, run_id)): Path<(String, String)>,
    Query(query): Query<TaskRunEventsQuery>,
) -> ApiResult<
    axum::response::Sse<
        impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>
        + Send,
    >,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::stream::unfold;
    use std::convert::Infallible;
    use std::time::Duration;

    let since = query.since.unwrap_or(0);
    let (pending, rx, terminal) = {
        let handle = PROJECT_TASK_RUNS
            .read()
            .await
            .get(&run_id)
            .cloned()
            .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run was not found"))?;
        let store = handle.store.lock().await;
        if store.run.work_id != work_id {
            return Err(request_error(
                StatusCode::NOT_FOUND,
                "Project run was not found",
            ));
        }
        let mut pending = store
            .chunks
            .iter()
            .filter(|(event, _)| event.seq >= since)
            .map(|(event, _)| Arc::clone(event))
            .collect::<std::collections::VecDeque<_>>();
        let available_from = store
            .chunks
            .front()
            .map_or(store.run.next_seq, |(event, _)| event.seq);
        if available_from > since {
            pending.push_front(Arc::new(task_replay_gap_event(
                &run_id,
                since,
                available_from,
            )));
        }
        // Cancel may flip state before the process exits and final result lands.
        let terminal = task_run_is_terminal(&store.run);
        (pending, store.tx.subscribe(), terminal)
    };

    struct StreamState {
        run_id: String,
        pending: std::collections::VecDeque<Arc<ProjectTaskOutputEvent>>,
        rx: tokio::sync::broadcast::Receiver<Arc<ProjectTaskOutputEvent>>,
        last_seq: u64,
        terminal: bool,
    }

    let initial = StreamState {
        run_id: run_id.clone(),
        pending,
        rx,
        last_seq: since.saturating_sub(1),
        terminal,
    };

    let stream = unfold(initial, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                let gap = event.kind == "gap";
                if !gap && event.seq <= state.last_seq {
                    continue;
                }
                if !gap {
                    state.last_seq = event.seq;
                }
                let done = task_output_event_is_terminal(&event);
                let data = serde_json::to_string(event.as_ref()).unwrap_or_else(|_| "{}".into());
                if done {
                    state.terminal = true;
                }
                return Some((
                    Ok::<_, Infallible>(Event::default().event("task").data(data)),
                    state,
                ));
            }
            if state.terminal {
                return None;
            }
            match state.rx.recv().await {
                Ok(event) => {
                    if event.seq <= state.last_seq {
                        continue;
                    }
                    state.pending.push_back(event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    state.pending.push_back(Arc::new(task_replay_gap_event(
                        &state.run_id,
                        state.last_seq.saturating_add(1),
                        state.last_seq.saturating_add(skipped).saturating_add(1),
                    )));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    state.terminal = true;
                }
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

async fn run_project_task(
    State(state): State<AppState>,
    Path((work_id, task_id)): Path<(String, String)>,
    Json(body): Json<RunProjectTaskRequest>,
) -> ApiResult<Json<ProjectTaskResult>> {
    let id = parse_work_id(&work_id)?;
    let (item, lease, root, working_root, task) = admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        None,
        {
            let state = state.clone();
            let lease_id = body.lease_id.clone();
            let generation = body.generation;
            let task_id = task_id.clone();
            move || {
                let (item, lease) = require_work_lease(&state, &id, &lease_id, generation)?;
                let root = item
                    .environment_for_attempt(&lease.attempt_id)
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "Set up this project before running it",
                        )
                    })?
                    .worktree
                    .clone();
                let task = project_tasks(&root)
                    .into_iter()
                    .find(|task| task.id == task_id)
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::NOT_FOUND,
                            "Project command is no longer available",
                        )
                    })?;
                if let Some(message) = unavailable_task_message(&task) {
                    return Err(request_error(StatusCode::CONFLICT, message));
                }
                let working_root = resolve_project_task_root(&root, &task.root)?;
                Ok((item, lease, root, working_root, task))
            }
        },
    )
    .await?;
    let forge = forge(&state);
    let argv = task.argv.clone();
    let root_for_command = working_root.clone();
    let started = std::time::Instant::now();
    let (stdout_bytes, stderr_bytes, capture_truncated, status) = state
        .forge_execution
        .run(
            medousa_forge::execution::ExecutionClass::LocalMutation,
            medousa_forge::execution::MAX_CAPTURE_BYTES,
            move || {
                let mut command = background_command(&argv[0]);
                command.args(&argv[1..]).current_dir(root_for_command);
                medousa_forge::execution::run_command_bounded(
                    command,
                    medousa_forge::execution::MAX_CAPTURE_BYTES,
                )
            },
        )
        .await
        .map_err(|err| {
            request_error(
                StatusCode::BAD_REQUEST,
                format!("Could not run {}: {err}", task.label),
            )
        })?;
    const OUTPUT_CAP: usize = 64 * 1024;
    let truncated =
        capture_truncated || stdout_bytes.len() > OUTPUT_CAP || stderr_bytes.len() > OUTPUT_CAP;
    let stdout =
        String::from_utf8_lossy(&stdout_bytes[..stdout_bytes.len().min(OUTPUT_CAP)]).into_owned();
    let stderr =
        String::from_utf8_lossy(&stderr_bytes[..stderr_bytes.len().min(OUTPUT_CAP)]).into_owned();
    let locations = parse_output_locations(&root, &working_root, &format!("{stdout}\n{stderr}"));
    let result = ProjectTaskResult {
        task: task.clone(),
        success: status.success(),
        exit_code: status.code(),
        stdout,
        stderr,
        truncated,
        duration_ms: started.elapsed().as_millis(),
        locations,
    };
    let _ = forge.append_command_log(
        &lease,
        &serde_json::json!({
            "kind": "project_task",
                "task": result.task,
                "success": result.success,
                "exit_code": result.exit_code,
                "duration_ms": result.duration_ms,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "truncated": result.truncated,
        }),
    );
    publish_item(&state, &item, "task_finished");
    Ok(Json(result))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProviderContext {
    #[serde(default)]
    links: Vec<String>,
    #[serde(default)]
    review_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProviderHandoff {
    provider: String,
    available: bool,
    repository: Option<String>,
    remote_url: Option<String>,
    branch: Option<String>,
    base_branch: Option<String>,
    shared: bool,
    review_url: Option<String>,
    links: Vec<String>,
    message: String,
}

#[derive(Debug, Deserialize)]
struct SaveProviderContextRequest {
    links: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ShareProviderRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    attempt_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProviderComment {
    id: String,
    author: String,
    body: String,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportProviderCommentRequest {
    id: String,
    body: String,
    #[serde(default)]
    url: Option<String>,
}

fn provider_context_path(forge: &Forge, id: &WorkId) -> PathBuf {
    forge.store().item_dir(id).join("provider-context.json")
}

fn load_provider_context(forge: &Forge, id: &WorkId) -> ProviderContext {
    std::fs::read(provider_context_path(forge, id))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn store_provider_context(forge: &Forge, id: &WorkId, context: &ProviderContext) -> ApiResult<()> {
    let bytes = serde_json::to_vec_pretty(context)
        .map_err(|err| request_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    crate::session::atomic_write(&provider_context_path(forge, id), &bytes)
        .map_err(|err| request_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

fn command_available(name: &str) -> bool {
    background_command(name)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn background_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    medousa_host::hide_subprocess_window(&mut command);
    command
}

#[cfg(windows)]
fn background_tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    medousa_host::hide_tokio_subprocess_window(&mut command);
    command
}

#[cfg(not(windows))]
fn background_tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    tokio::process::Command::new(program)
}

fn repository_remote(worktree: &FsPath) -> Option<String> {
    let output = background_command("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(worktree)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn provider_repository(remote: &str) -> Option<(&'static str, String)> {
    for (provider, prefixes) in [
        (
            "github",
            [
                "git@github.com:",
                "https://github.com/",
                "http://github.com/",
                "ssh://git@github.com/",
            ],
        ),
        (
            "gitlab",
            [
                "git@gitlab.com:",
                "https://gitlab.com/",
                "http://gitlab.com/",
                "ssh://git@gitlab.com/",
            ],
        ),
    ] {
        if let Some(repository) = prefixes
            .iter()
            .find_map(|prefix| remote.strip_prefix(prefix))
        {
            let repository = repository.trim_end_matches('/').trim_end_matches(".git");
            if normalize_provider_repository_name(repository) {
                return Some((provider, repository.to_string()));
            }
        }
    }
    None
}

fn normalize_provider_repository_name(repository: &str) -> bool {
    let segments = repository.split('/').collect::<Vec<_>>();
    segments.len() >= 2
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && *segment != "."
                && *segment != ".."
                && !segment.starts_with('-')
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
                })
        })
}

fn provider_handoff(forge: &Forge, item: &WorkItem) -> ProviderHandoff {
    let context = load_provider_context(forge, &item.id);
    let remote_url = item
        .workspace_environment()
        .and_then(|env| repository_remote(&env.worktree));
    let parsed = remote_url.as_deref().and_then(provider_repository);
    let provider = parsed
        .as_ref()
        .map(|(provider, _)| *provider)
        .unwrap_or("none");
    let available = match provider {
        "github" => command_available("gh"),
        "gitlab" => command_available("glab"),
        _ => false,
    };
    let branch = item.workspace_environment().map(|env| env.branch.clone());
    let WorkTarget::Git(target) = &item.target;
    let base_branch = Some(target.base_ref.clone());
    let shared = item.workspace_environment().is_some_and(|env| {
        background_command("git")
            .args([
                "show-ref",
                "--verify",
                &format!("refs/remotes/origin/{}", env.branch),
            ])
            .current_dir(&env.worktree)
            .output()
            .is_ok_and(|output| output.status.success())
    });
    ProviderHandoff {
        provider: provider.into(),
        available,
        repository: parsed.map(|(_, repository)| repository),
        remote_url,
        branch,
        base_branch,
        shared,
        review_url: context.review_url,
        links: context.links,
        message: match (provider, available) {
            ("none", _) => "No supported repository provider was found for origin.".into(),
            (_, false) => {
                format!("Install and sign in to the {provider} CLI on the workshop machine.")
            }
            _ => "Ready to share from the connected workshop.".into(),
        },
    }
}

async fn get_provider_handoff(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ProviderHandoff>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::RepositoryMetadata,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let forge = forge(&state);
                let item = forge.load(&id).map_err(map_err)?;
                Ok(Json(provider_handoff(forge.as_ref(), &item)))
            }
        },
    )
    .await
}

async fn save_provider_context(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SaveProviderContextRequest>,
) -> ApiResult<Json<ProviderHandoff>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let forge = forge(&state);
                let item = forge.load(&id).map_err(map_err)?;
                if body.links.len() > 20 {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "Too many linked items",
                    ));
                }
                let mut context = load_provider_context(forge.as_ref(), &id);
                let mut links = body
                    .links
                    .into_iter()
                    .map(|link| link.trim().to_string())
                    .collect::<Vec<_>>();
                if links
                    .iter()
                    .any(|link| !link.starts_with("https://") || link.len() > 2_048)
                {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "Linked work must use a valid HTTPS URL",
                    ));
                }
                links.dedup();
                context.links = links;
                store_provider_context(forge.as_ref(), &id, &context)?;
                Ok(Json(provider_handoff(forge.as_ref(), &item)))
            }
        },
    )
    .await
}

fn provider_command_error(label: &str, output: &std::process::Output) -> ApiError {
    let detail = String::from_utf8_lossy(&output.stderr)
        .trim()
        .chars()
        .take(500)
        .collect::<String>();
    request_error(
        StatusCode::BAD_GATEWAY,
        format!(
            "{label} failed{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        ),
    )
}

fn provider_review_body(
    forge: &Forge,
    item: &WorkItem,
    requested: Option<&str>,
    attempt_id: Option<&medousa_forge::model::AttemptId>,
) -> String {
    let review = build_review_for_attempt(forge, item, attempt_id);
    let introduction = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&item.brief);
    let verification = review
        .synthesis
        .verification
        .as_ref()
        .map(|verification| {
            format!(
                "- {}: {} (`{}`)",
                verification.label,
                if verification.success {
                    "passed"
                } else {
                    "failed"
                },
                verification.command.join(" ")
            )
        })
        .unwrap_or_else(|| "- No recorded verification command".into());
    let evidence = review
        .evidence_digest
        .as_deref()
        .map(|digest| format!("`{digest}`"))
        .unwrap_or_else(|| "No sealed evidence digest".into());
    let linked = load_provider_context(forge, &item.id);
    let links = if linked.links.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n## Related work\n{}",
            linked
                .links
                .iter()
                .map(|link| format!("- {link}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    format!(
        "{introduction}\n\n## Medousa outcome\n{}\n\n- Status: {}\n- Risk: {}\n- Changed files: {}\n{verification}\n- Forge evidence: {evidence}{links}",
        review.synthesis.outcome,
        review.synthesis.status_summary,
        review.synthesis.risk_summary,
        review.changed_files.len(),
    )
    .chars()
    .take(60_000)
    .collect()
}

fn existing_provider_review_url(
    provider: &str,
    repository: &str,
    branch: &str,
    worktree: &FsPath,
) -> ApiResult<Option<String>> {
    let output = if provider == "github" {
        background_command("gh")
            .args([
                "pr", "view", branch, "--repo", repository, "--json", "url", "--jq", ".url",
            ])
            .current_dir(worktree)
            .output()
    } else {
        background_command("glab")
            .args(["mr", "view", branch, "--output", "json"])
            .current_dir(worktree)
            .output()
    }
    .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
    if !output.status.success() {
        return Err(provider_command_error("Opening the review", &output));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = if provider == "github" {
        let url = stdout.trim();
        (!url.is_empty()).then(|| url.to_string())
    } else {
        serde_json::from_str::<serde_json::Value>(&stdout)
            .ok()
            .and_then(|value| {
                value
                    .get("web_url")
                    .and_then(|url| url.as_str())
                    .map(str::to_string)
            })
    };
    Ok(url)
}

async fn share_provider_handoff(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ShareProviderRequest>,
) -> ApiResult<Json<ProviderHandoff>> {
    admit_forge_on_repo(
        &state,
        medousa_forge::execution::ExecutionClass::NetworkGit,
        64 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let forge = forge(&state);
                let item = forge.load(&id).map_err(map_err)?;
                if !matches!(item.state, WorkState::AwaitingReview | WorkState::Accepted) {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "Finish and review the project before sharing it",
                    ));
                }
                let handoff = provider_handoff(forge.as_ref(), &item);
                if !handoff.available {
                    return Err(request_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        handoff.message,
                    ));
                }
                let attempt_id = body
                    .attempt_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| medousa_forge::model::AttemptId::from(value.to_string()));
                let environment = attempt_id
                    .as_ref()
                    .and_then(|attempt_id| item.environment_for_attempt(attempt_id))
                    .or_else(|| item.workspace_environment())
                    .ok_or_else(|| {
                        request_error(StatusCode::CONFLICT, "Project workspace is unavailable")
                    })?;
                let push = background_command("git")
                    .args(["push", "--set-upstream", "origin", &environment.branch])
                    .current_dir(&environment.worktree)
                    .output()
                    .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
                if !push.status.success() {
                    return Err(provider_command_error("Sharing the branch", &push));
                }
                let repository = handoff.repository.as_deref().ok_or_else(|| {
                    request_error(
                        StatusCode::BAD_REQUEST,
                        "Repository identity is unavailable",
                    )
                })?;
                let base = handoff.base_branch.as_deref().unwrap_or("main");
                let title = body
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(&item.title)
                    .chars()
                    .take(256)
                    .collect::<String>();
                let description = provider_review_body(
                    forge.as_ref(),
                    &item,
                    body.body.as_deref(),
                    attempt_id.as_ref(),
                );
                let output = if handoff.provider == "github" {
                    background_command("gh")
                        .args([
                            "pr",
                            "create",
                            "--repo",
                            repository,
                            "--head",
                            &environment.branch,
                            "--base",
                            base,
                            "--title",
                            &title,
                            "--body",
                            &description,
                        ])
                        .current_dir(&environment.worktree)
                        .output()
                } else {
                    background_command("glab")
                        .args([
                            "mr",
                            "create",
                            "--source-branch",
                            &environment.branch,
                            "--target-branch",
                            base,
                            "--title",
                            &title,
                            "--description",
                            &description,
                            "--yes",
                        ])
                        .current_dir(&environment.worktree)
                        .output()
                }
                .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
                let created_url = output
                    .status
                    .success()
                    .then(|| {
                        String::from_utf8_lossy(&output.stdout)
                            .split_whitespace()
                            .find(|value| value.starts_with("http"))
                            .map(|value| value.trim().to_string())
                    })
                    .flatten();
                let review_url = if output.status.success() {
                    if created_url.is_some() {
                        created_url
                    } else {
                        existing_provider_review_url(
                            &handoff.provider,
                            repository,
                            &environment.branch,
                            &environment.worktree,
                        )?
                    }
                } else {
                    let update = if handoff.provider == "github" {
                        background_command("gh")
                            .args([
                                "pr",
                                "edit",
                                &environment.branch,
                                "--repo",
                                repository,
                                "--title",
                                &title,
                                "--body",
                                &description,
                            ])
                            .current_dir(&environment.worktree)
                            .output()
                    } else {
                        background_command("glab")
                            .args([
                                "mr",
                                "update",
                                &environment.branch,
                                "--title",
                                &title,
                                "--description",
                                &description,
                            ])
                            .current_dir(&environment.worktree)
                            .output()
                    }
                    .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
                    if !update.status.success() {
                        return Err(provider_command_error("Updating the review", &update));
                    }
                    existing_provider_review_url(
                        &handoff.provider,
                        repository,
                        &environment.branch,
                        &environment.worktree,
                    )?
                };
                let mut context = load_provider_context(forge.as_ref(), &id);
                context.review_url = review_url;
                store_provider_context(forge.as_ref(), &id, &context)?;
                publish_item(&state, &item, "provider_shared");
                Ok(Json(provider_handoff(forge.as_ref(), &item)))
            }
        },
    )
    .await
}

async fn list_provider_comments(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<Vec<ProviderComment>>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let forge = forge(&state);
                let item = forge.load(&id).map_err(map_err)?;
                let handoff = provider_handoff(forge.as_ref(), &item);
                if handoff.provider != "github" || handoff.review_url.is_none() {
                    return Ok(Json(Vec::new()));
                }
                let repository = handoff.repository.ok_or_else(|| {
                    request_error(
                        StatusCode::BAD_REQUEST,
                        "Repository identity is unavailable",
                    )
                })?;
                let branch = handoff.branch.ok_or_else(|| {
                    request_error(StatusCode::BAD_REQUEST, "Project branch is unavailable")
                })?;
                let output = background_command("gh")
                    .args([
                        "pr",
                        "view",
                        &branch,
                        "--repo",
                        &repository,
                        "--json",
                        "comments,reviews",
                    ])
                    .output()
                    .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
                if !output.status.success() {
                    return Err(provider_command_error("Reading review comments", &output));
                }
                let value: serde_json::Value = serde_json::from_slice(&output.stdout)
                    .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
                let mut comments = Vec::new();
                for (kind, entries) in [
                    ("comment", value.get("comments")),
                    ("review", value.get("reviews")),
                ] {
                    for (index, entry) in entries
                        .and_then(|entries| entries.as_array())
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        let body = entry
                            .get("body")
                            .and_then(|body| body.as_str())
                            .unwrap_or("")
                            .trim();
                        if body.is_empty() {
                            continue;
                        }
                        comments.push(ProviderComment {
                            id: format!("{kind}-{index}"),
                            author: entry
                                .pointer("/author/login")
                                .and_then(|author| author.as_str())
                                .unwrap_or("Reviewer")
                                .into(),
                            body: body.chars().take(8_000).collect(),
                            url: entry
                                .get("url")
                                .and_then(|url| url.as_str())
                                .map(str::to_string),
                        });
                    }
                }
                Ok(Json(comments))
            }
        },
    )
    .await
}

async fn import_provider_comment(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ImportProviderCommentRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let forge = forge(&state);
                let source = forge.load(&id).map_err(map_err)?;
                let WorkTarget::Git(target) = &source.target;
                let body_text = body.body.trim();
                if body.id.trim().is_empty() || body_text.is_empty() || body_text.len() > 8_000 {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "Review comment is invalid",
                    ));
                }
                let brief = format!(
                    "Follow up on review feedback for {}:\n\n{}{}",
                    source.title,
                    body_text,
                    body.url
                        .as_deref()
                        .map(|url| format!("\n\nSource: {url}"))
                        .unwrap_or_default()
                );
                let actor = actor_from_state(&state);
                let item = forge
                    .register(
                        format!("Follow up: {}", source.title),
                        brief,
                        &target.repo_path,
                        target.base_ref.clone(),
                        state.workshop_identity_user_id(),
                        &actor,
                    )
                    .map_err(map_err)?;
                publish_item(&state, &item, "provider_follow_up_created");
                Ok(Json(project_item(item)))
            }
        },
    )
    .await
}

async fn provision_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let item = forge(&state).provision(&id, &actor).map_err(map_err)?;
                if let Some(env) = item.workspace_environment() {
                    crate::daemon::detamu_host::spawn_index_forge_item(
                        state.detamu.clone(),
                        item.id.as_str().to_owned(),
                        env.worktree.clone(),
                        env.baseline_oid.as_str().to_owned(),
                        crate::daemon::detamu_host::BindingKind::Baseline,
                    );
                }
                Ok(ok_item(&state, item, "provisioned"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct BeginAttemptRequest {
    #[serde(default)]
    executor: Option<ExecutorDescriptor>,
    #[serde(default)]
    pid: Option<u32>,
}

#[derive(Debug, Serialize)]
struct BeginAttemptResponse {
    item: ItemProjection,
    lease: ExecutionLease,
    attempt_id: String,
    worktree: String,
    branch: String,
}

async fn begin_attempt(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<BeginAttemptRequest>,
) -> ApiResult<Json<BeginAttemptResponse>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let executor = body.executor.unwrap_or(ExecutorDescriptor {
                    kind: "human".into(),
                    detail: serde_json::json!({}),
                });
                let (item, lease) = forge(&state)
                    .begin_isolated_attempt(&id, executor, body.pid, &actor)
                    .map_err(map_err)?;
                let environment =
                    item.environment_for_attempt(&lease.attempt_id)
                        .ok_or_else(|| {
                            request_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "isolated attempt has no governed environment",
                            )
                        })?;
                let attempt_id = lease.attempt_id.as_str().to_owned();
                let worktree = environment.worktree.display().to_string();
                let branch = environment.branch.clone();
                publish_item(&state, &item, "attempt_begun");
                Ok(Json(BeginAttemptResponse {
                    item: project_item(item),
                    lease,
                    attempt_id,
                    worktree,
                    branch,
                }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct PrepareHandoffRequest {
    lease_id: String,
    generation: u64,
    to_executor: String,
}

/// End the current executor's custody while preserving its worktree for the
/// next executor. Starting the external provider remains a separate retryable
/// operation, so a provider failure leaves the item safely Ready.
async fn prepare_handoff(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<PrepareHandoffRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let target = body.to_executor.trim().to_ascii_lowercase();
                if !matches!(target.as_str(), "codex" | "cursor" | "human") {
                    return Err(request_error(
                        StatusCode::BAD_REQUEST,
                        "handoff target must be codex, cursor, or human",
                    ));
                }
                let lease = resolve_lease(forge(&state).as_ref(), &body.lease_id, body.generation)?;
                if lease.work_id != id {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "the active editor belongs to a different project",
                    ));
                }
                let item = forge(&state).load(&id).map_err(map_err)?;
                let from = item
                    .attempt(&lease.attempt_id)
                    .map(|attempt| attempt.executor.kind.as_str())
                    .unwrap_or("unknown");
                forge(&state)
                    .append_command_log(
                        &lease,
                        &serde_json::json!({
                            "kind": "executor_handoff",
                            "from": from,
                            "to": target,
                            "at": chrono::Utc::now(),
                        }),
                    )
                    .map_err(map_err)?;
                let actor = actor_from_state(&state);
                let item = forge(&state)
                    .interrupt_attempt(&lease, RecoveryDisposition::RestartAllowed, &actor)
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "handoff_prepared"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct LeaseMutationRequest {
    generation: u64,
}

#[derive(Debug, Deserialize)]
struct CompleteLeaseRequest {
    generation: u64,
    #[serde(default)]
    ack_risks: bool,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    author_email: Option<String>,
}

async fn heartbeat_lease(
    State(state): State<AppState>,
    Path(lease_id): Path<String>,
    Json(body): Json<LeaseMutationRequest>,
) -> ApiResult<StatusCode> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
                forge(&state).heartbeat(&lease).map_err(map_err)?;
                Ok(StatusCode::NO_CONTENT)
            }
        },
    )
    .await
}

async fn complete_lease(
    State(state): State<AppState>,
    Path(lease_id): Path<String>,
    Json(body): Json<CompleteLeaseRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
                let author = match (body.author_name, body.author_email) {
                    (Some(name), Some(email)) => Some(CheckpointAuthor { name, email }),
                    _ => None,
                };
                let options = SealOptions {
                    ack_risks: body.ack_risks,
                    author,
                };
                let actor = actor_from_state(&state);
                let item = forge(&state)
                    .complete_attempt(&lease, &options, &actor)
                    .map_err(map_err)?;
                if let Some(env) = item.environment_for_attempt(&lease.attempt_id) {
                    let sealed_oid = forge(&state)
                        .git()
                        .head_oid(&env.worktree)
                        .ok()
                        .map(|oid| oid.as_str().to_owned())
                        .unwrap_or_else(|| env.baseline_oid.as_str().to_owned());
                    crate::daemon::detamu_host::spawn_index_forge_item(
                        state.detamu.clone(),
                        item.id.as_str().to_owned(),
                        env.worktree.clone(),
                        sealed_oid,
                        crate::daemon::detamu_host::BindingKind::Sealed,
                    );
                }
                Ok(ok_item(&state, item, "sealed"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct InterruptLeaseRequest {
    generation: u64,
    #[serde(default)]
    recovery: Option<RecoveryDisposition>,
}

async fn interrupt_lease(
    State(state): State<AppState>,
    Path(lease_id): Path<String>,
    Json(body): Json<InterruptLeaseRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
                let actor = actor_from_state(&state);
                let recovery = body.recovery.unwrap_or(RecoveryDisposition::RestartAllowed);
                let item = forge(&state)
                    .interrupt_attempt(&lease, recovery, &actor)
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "interrupted"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct FailLeaseRequest {
    generation: u64,
    #[serde(default)]
    error: Option<String>,
}

async fn fail_lease(
    State(state): State<AppState>,
    Path(lease_id): Path<String>,
    Json(body): Json<FailLeaseRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
                let actor = actor_from_state(&state);
                let message = body.error.unwrap_or_else(|| "attempt failed".into());
                let item = forge(&state)
                    .fail_attempt(&lease, &message, &actor)
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "failed"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ReviewIntentRequest {
    evidence_id: String,
    evidence_digest: String,
    #[serde(default = "default_strategy")]
    strategy: String,
    #[serde(default)]
    acknowledged_violations: Vec<String>,
    #[serde(default)]
    rationale: Option<String>,
}

fn default_strategy() -> String {
    "preserve_branch".into()
}

fn parse_strategy(raw: &str) -> ApiResult<IntegrationStrategy> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "preserve_branch" | "preserve" => Ok(IntegrationStrategy::PreserveBranch),
        "fast_forward_only" | "fast_forward" | "ff" => Ok(IntegrationStrategy::FastForwardOnly),
        "export_patch" | "export" => Ok(IntegrationStrategy::ExportPatch),
        other => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorBody {
                error: format!("unknown strategy: {other}"),
                kind: Some("bad_request"),
            }),
        )),
    }
}

async fn record_decision(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ReviewIntentRequest>,
) -> ApiResult<Json<ItemProjection>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let actor = actor_from_state(&state);
                let item = forge(&state).load(&id).map_err(map_err)?;
                let strategy = parse_strategy(&body.strategy)?;
                let evidence_id = EvidenceId::from(body.evidence_id.clone());
                let attempt = item
                    .attempts
                    .iter()
                    .rev()
                    .find(|a| a.evidence_id.as_ref() == Some(&evidence_id))
                    .ok_or_else(|| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorBody {
                                error: "evidence_id does not match a sealed attempt".into(),
                                kind: Some("bad_request"),
                            }),
                        )
                    })?;
                let env = item.environment_for_attempt(&attempt.id).ok_or_else(|| {
                    (
                        StatusCode::CONFLICT,
                        Json(ErrorBody {
                            error: "no governed environment".into(),
                            kind: Some("conflict"),
                        }),
                    )
                })?;
                let manifest = evidence_dir(forge(&state).as_ref(), &item, &evidence_id)
                    .and_then(|dir| crate::daemon::forge_projections::load_manifest(&dir))
                    .ok_or_else(|| {
                        request_error(
                            StatusCode::CONFLICT,
                            "sealed evidence manifest is unavailable",
                        )
                    })?;
                if manifest.attempt_id != attempt.id || manifest.evidence_id != evidence_id {
                    return Err(request_error(
                        StatusCode::CONFLICT,
                        "sealed evidence identity does not match the selected attempt",
                    ));
                }
                let digest = manifest
                    .bundle_digest
                    .as_ref()
                    .map(|value| value.as_str().to_owned())
                    .ok_or_else(|| {
                        request_error(StatusCode::CONFLICT, "sealed evidence has no digest")
                    })?;
                if !body.evidence_digest.is_empty() && digest != body.evidence_digest {
                    return Err((
                        StatusCode::CONFLICT,
                        Json(ErrorBody {
                            error: "evidence_digest mismatch".into(),
                            kind: Some("conflict"),
                        }),
                    ));
                }
                let sealed_head = manifest.sealed_head_oid.as_str().to_owned();
                let evidence_digest: medousa_forge::model::Digest =
                    serde_json::from_value(serde_json::Value::String(digest)).map_err(|e| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorBody {
                                error: format!("invalid evidence_digest: {e}"),
                                kind: Some("bad_request"),
                            }),
                        )
                    })?;
                let WorkTarget::Git(target) = &item.target;
                let expected_base_oid = forge(&state)
                    .git()
                    .ref_oid(&target.repo_path, &target.base_ref)
                    .map_err(map_err)?;
                let decision = ReviewDecision {
                    id: ReviewDecisionId::new(),
                    actor: actor.clone(),
                    attempt_id: attempt.id.clone(),
                    environment_generation: env.generation,
                    evidence_id,
                    evidence_digest,
                    baseline_oid: manifest.baseline_oid.clone(),
                    reviewed_head_oid: medousa_forge::model::GitOid::new(sealed_head),
                    expected_base_oid,
                    acknowledged_violations: body
                        .acknowledged_violations
                        .into_iter()
                        .map(medousa_forge::model::PolicyViolationId::from)
                        .collect(),
                    strategy,
                    rationale: body.rationale,
                    decided_at: chrono::Utc::now(),
                };
                let item = forge(&state)
                    .decide(&id, decision, &actor)
                    .map_err(map_err)?;
                Ok(ok_item(&state, item, "decision_recorded"))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ApplyRequest {
    decision_id: String,
}

async fn apply_decision(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ApplyRequest>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    let decision_id = ReviewDecisionId::from(body.decision_id);
    let item = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::LocalMutation,
        256 * 1024,
        {
            let state = state.clone();
            let decision_id = decision_id.clone();
            move || {
                let actor = actor_from_state(&state);
                forge(&state)
                    .apply_decision(&id, &decision_id, &actor)
                    .map_err(map_err)
            }
        },
    )
    .await?;
    let memory_lineage = crate::agent_runtime::coder_tools::finalize_coder_memory_lineage(
        state.platform.agent().tool_registry.clone(),
        forge(&state),
        &item,
        Some(&decision_id),
    )
    .await;
    if memory_lineage
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        tracing::warn!(
            work_id = %item.id,
            report = %memory_lineage,
            "accepted Coder memory lineage finalized with deferred work"
        );
    }
    Ok(ok_item(&state, item, "applied"))
}

async fn discard_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    let item = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let actor = actor_from_state(&state);
                forge(&state).discard(&id, &actor).map_err(map_err)
            }
        },
    )
    .await?;
    let memory_lineage = crate::agent_runtime::coder_tools::finalize_coder_memory_lineage(
        state.platform.agent().tool_registry.clone(),
        forge(&state),
        &item,
        None,
    )
    .await;
    if memory_lineage
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        tracing::warn!(
            work_id = %item.id,
            report = %memory_lineage,
            "discarded Coder memory lineage finalized with deferred work"
        );
    }
    Ok(ok_item(&state, item, "discarded"))
}

#[derive(Debug, Deserialize)]
struct RunScriptRequest {
    argv: Vec<String>,
}

async fn run_script(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RunScriptRequest>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    if body.argv.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorBody {
                error: "argv must not be empty".into(),
                kind: Some("bad_request"),
            }),
        ));
    }
    let forge = forge(&state);
    let argv = body.argv;
    let item = state
        .forge_execution
        .run(
            medousa_forge::execution::ExecutionClass::LocalMutation,
            64 * 1024,
            move || ScriptAdapter::new(forge.as_ref()).run_script(&id, &argv),
        )
        .await
        .map_err(map_err)?;
    Ok(ok_item(&state, item, "script_ran"))
}

#[derive(Debug, Deserialize)]
struct ExportRequest {
    destination: PathBuf,
}

#[derive(Debug, Serialize)]
struct ExportResponse {
    destination: PathBuf,
}

async fn export_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ExportRequest>,
) -> ApiResult<Json<ExportResponse>> {
    admit_forge_canary(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        1024 * 1024,
        None,
        {
            let state = state.clone();
            move || {
                let id = parse_work_id(&work_id)?;
                let forge = forge(&state);
                let destination = body.destination;
                if destination.exists() {
                    let mut entries = std::fs::read_dir(&destination)
                        .map_err(ForgeError::from)
                        .map_err(map_err)?;
                    if entries.next().is_some() {
                        return Err((
                            StatusCode::CONFLICT,
                            Json(ErrorBody {
                                error: "export destination already exists and is not empty".into(),
                                kind: Some("destination_not_empty"),
                            }),
                        ));
                    }
                }
                export_bundle(forge.as_ref(), &id, &destination).map_err(map_err)?;
                Ok(Json(ExportResponse { destination }))
            }
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct EvidencePageQuery {
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    work_id: Option<String>,
}

fn find_evidence_dir(
    forge: &Forge,
    evidence_id: &EvidenceId,
    work_id_hint: Option<&str>,
) -> ApiResult<(WorkItem, PathBuf)> {
    if let Some(wid) = work_id_hint {
        let id = parse_work_id(wid)?;
        let item = forge.load(&id).map_err(map_err)?;
        if let Some(dir) = evidence_dir(forge, &item, evidence_id) {
            return Ok((item, dir));
        }
    }
    for item in forge.list().map_err(map_err)? {
        if let Some(dir) = evidence_dir(forge, &item, evidence_id) {
            return Ok((item, dir));
        }
    }
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: format!("evidence not found: {}", evidence_id.as_str()),
            kind: Some("not_found"),
        }),
    ))
}

#[derive(Debug, Serialize)]
struct EvidencePage {
    evidence_id: String,
    offset: usize,
    limit: usize,
    total_lines: usize,
    truncated: bool,
    lines: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EvidenceReceiptsResponse {
    evidence_id: String,
    receipts: Vec<CompactEvidenceReceipt>,
}

async fn evidence_patch(
    State(state): State<AppState>,
    Path(evidence_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<EvidencePageQuery>,
) -> ApiResult<Json<EvidencePage>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let eid = EvidenceId::from(evidence_id);
                let (_item, dir) =
                    find_evidence_dir(forge(&state).as_ref(), &eid, q.work_id.as_deref())?;
                let offset = q.offset.unwrap_or(0);
                let limit = q.limit.unwrap_or(200).clamp(1, 2000);
                let (lines, total, truncated) =
                    read_lines_page(&dir.join("patch.diff"), offset, limit).map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorBody {
                                error: e,
                                kind: Some("store"),
                            }),
                        )
                    })?;
                Ok(Json(EvidencePage {
                    evidence_id: eid.as_str().to_owned(),
                    offset,
                    limit,
                    total_lines: total,
                    truncated,
                    lines,
                }))
            }
        },
    )
    .await
}

async fn evidence_commands(
    State(state): State<AppState>,
    Path(evidence_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<EvidencePageQuery>,
) -> ApiResult<Json<EvidencePage>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let eid = EvidenceId::from(evidence_id);
                let (_item, dir) =
                    find_evidence_dir(forge(&state).as_ref(), &eid, q.work_id.as_deref())?;
                let offset = q.offset.unwrap_or(0);
                let limit = q.limit.unwrap_or(200).clamp(1, 2000);
                let (lines, total, truncated) =
                    read_lines_page(&dir.join("commands.jsonl"), offset, limit).map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorBody {
                                error: e,
                                kind: Some("store"),
                            }),
                        )
                    })?;
                Ok(Json(EvidencePage {
                    evidence_id: eid.as_str().to_owned(),
                    offset,
                    limit,
                    total_lines: total,
                    truncated,
                    lines,
                }))
            }
        },
    )
    .await
}

async fn evidence_receipts(
    State(state): State<AppState>,
    Path(evidence_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<EvidencePageQuery>,
) -> ApiResult<Json<EvidenceReceiptsResponse>> {
    admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            move || {
                let eid = EvidenceId::from(evidence_id);
                let (_item, dir) =
                    find_evidence_dir(forge(&state).as_ref(), &eid, q.work_id.as_deref())?;
                let bytes = match std::fs::read(dir.join("receipts.json")) {
                    Ok(bytes) => bytes,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => b"[]".to_vec(),
                    Err(err) => {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorBody {
                                error: err.to_string(),
                                kind: Some("store"),
                            }),
                        ));
                    }
                };
                let receipts = serde_json::from_slice(&bytes).map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorBody {
                            error: err.to_string(),
                            kind: Some("store"),
                        }),
                    )
                })?;
                Ok(Json(EvidenceReceiptsResponse {
                    evidence_id: eid.as_str().to_owned(),
                    receipts,
                }))
            }
        },
    )
    .await
}

async fn forge_stream(
    State(state): State<AppState>,
) -> axum::response::Sse<
    impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::stream::unfold;
    use std::convert::Infallible;
    use std::time::Duration;

    let rx = state.forge_events.subscribe();
    let stream = unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
                    return Some((
                        Ok::<_, Infallible>(Event::default().event("forge").data(data)),
                        rx,
                    ));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[derive(Debug, Deserialize)]
struct ProjectEventStreamQuery {
    #[serde(default)]
    since: Option<u64>,
}

async fn forge_project_event_stream(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<ProjectEventStreamQuery>,
) -> ApiResult<
    axum::response::Sse<
        impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>
        + use<>,
    >,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures_util::stream::unfold;
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::time::Duration;

    let id = parse_work_id(&work_id)?;
    let item = admit_forge(
        &state,
        medousa_forge::execution::ExecutionClass::StoreIo,
        64 * 1024,
        {
            let state = state.clone();
            let id = id.clone();
            move || forge(&state).load(&id).map_err(map_err)
        },
    )
    .await?;
    if let Some(env) = item
        .attempts
        .last()
        .and_then(|attempt| item.environment_for_attempt(&attempt.id))
    {
        remember_worktree(&state, &item, &env.worktree);
    }

    let since = query.since.unwrap_or(0);
    // Subscribe before snapshot so live events cannot slip between the two.
    let receiver = state.forge_events.subscribe_project();
    let pending: VecDeque<_> = state
        .forge_events
        .snapshot_project_since(id.as_str(), since)
        .into();
    let work_id = id.as_str().to_owned();

    struct StreamState {
        work_id: String,
        receiver: tokio::sync::broadcast::Receiver<crate::daemon::forge_events::ForgeProjectEvent>,
        pending: VecDeque<crate::daemon::forge_events::ForgeProjectEvent>,
        last_seq: u64,
        bus: crate::daemon::forge_events::ForgeEventBus,
    }

    let initial = StreamState {
        work_id: work_id.clone(),
        receiver,
        pending,
        last_seq: since,
        bus: state.forge_events.clone(),
    };

    let stream = unfold(initial, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                if event.seq <= state.last_seq {
                    continue;
                }
                state.last_seq = event.seq;
                let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                return Some((
                    Ok::<_, Infallible>(Event::default().event("project").data(data)),
                    state,
                ));
            }
            match state.receiver.recv().await {
                Ok(event) => {
                    if event.work_id != state.work_id || event.seq <= state.last_seq {
                        continue;
                    }
                    state.last_seq = event.seq;
                    let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                    return Some((
                        Ok::<_, Infallible>(Event::default().event("project").data(data)),
                        state,
                    ));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    state.pending.extend(
                        state
                            .bus
                            .snapshot_project_since(&state.work_id, state.last_seq),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[cfg(test)]
mod source_tests {
    use super::*;

    #[test]
    fn start_code_project_command_normalizes_request_fields() {
        let command = StartCodeProjectCommand::new(
            " session-a ",
            StartSessionCodeProjectRequest {
                title: " Project ".into(),
                brief: " Brief ".into(),
                source: CodeProjectSource::Repository,
                repo_path: Some(" /workspace/project ".into()),
                base_ref: Some("  ".into()),
            },
        )
        .expect("valid project request");

        assert_eq!(command.session_id.as_str(), "session-a");
        assert_eq!(command.title.as_str(), "Project");
        assert_eq!(command.brief.as_str(), "Brief");
        assert_eq!(
            command.repo_path.as_ref().unwrap().as_str(),
            "/workspace/project"
        );
        assert_eq!(command.base_ref.as_str(), "main");
    }

    #[test]
    fn start_code_project_command_rejects_missing_required_values() {
        let missing_repo = StartCodeProjectCommand::new(
            "session-a",
            StartSessionCodeProjectRequest {
                title: "Project".into(),
                brief: "Brief".into(),
                source: CodeProjectSource::Repository,
                repo_path: Some(" \n\t".into()),
                base_ref: None,
            },
        )
        .expect_err("repository source requires a path");
        assert_eq!(
            missing_repo,
            "repo_path is required for an existing repository"
        );

        let missing_title = StartCodeProjectCommand::new(
            "session-a",
            StartSessionCodeProjectRequest {
                title: " \n\t".into(),
                brief: "Brief".into(),
                source: CodeProjectSource::Blank,
                repo_path: None,
                base_ref: None,
            },
        )
        .expect_err("title is required");
        assert_eq!(missing_title, "session_id, title, and brief are required");
    }

    #[test]
    fn porcelain_change_status_maps_kinds() {
        use medousa_forge::git::{PorcelainEntry, PorcelainKind};
        assert_eq!(
            porcelain_change_status(&PorcelainEntry {
                path: "a.rs".into(),
                kind: PorcelainKind::Unmerged,
                orig_path: None,
                xy: Some("UU".into()),
            }),
            Some("unmerged")
        );
        assert_eq!(
            porcelain_change_status(&PorcelainEntry {
                path: "b.rs".into(),
                kind: PorcelainKind::Untracked,
                orig_path: None,
                xy: None,
            }),
            Some("untracked")
        );
        assert_eq!(
            porcelain_change_status(&PorcelainEntry {
                path: "c.rs".into(),
                kind: PorcelainKind::Ordinary,
                orig_path: None,
                xy: Some(".M".into()),
            }),
            Some("modified")
        );
        assert_eq!(
            porcelain_change_status(&PorcelainEntry {
                path: "d.rs".into(),
                kind: PorcelainKind::Ignored,
                orig_path: None,
                xy: None,
            }),
            None
        );
    }

    #[test]
    fn apply_hunks_except_skips_selected_hunk() {
        let baseline = "a\nb\nc\n";
        let hunks = vec![
            ReviewDiffHunk {
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 1,
                lines: vec![
                    ReviewDiffLine {
                        kind: "deletion".into(),
                        old_line: Some(1),
                        new_line: None,
                        content: "a".into(),
                    },
                    ReviewDiffLine {
                        kind: "addition".into(),
                        old_line: None,
                        new_line: Some(1),
                        content: "A".into(),
                    },
                ],
            },
            ReviewDiffHunk {
                old_start: 3,
                old_count: 1,
                new_start: 3,
                new_count: 1,
                lines: vec![
                    ReviewDiffLine {
                        kind: "deletion".into(),
                        old_line: Some(3),
                        new_line: None,
                        content: "c".into(),
                    },
                    ReviewDiffLine {
                        kind: "addition".into(),
                        old_line: None,
                        new_line: Some(3),
                        content: "C".into(),
                    },
                ],
            },
        ];
        let with_both = apply_hunks_except(baseline, &hunks, usize::MAX).unwrap();
        assert_eq!(with_both, "A\nb\nC\n");
        let skip_first = apply_hunks_except(baseline, &hunks, 0).unwrap();
        assert_eq!(skip_first, "a\nb\nC\n");
    }

    #[test]
    fn task_output_is_bounded_and_marks_truncation() {
        let mut tail = TaskOutputTail::default();
        assert!(!tail.append(&"a".repeat(TASK_OUTPUT_CAP)));
        assert_eq!(tail.bytes, TASK_OUTPUT_CAP);
        assert!(tail.append("xyz"));
        let output = tail.materialize();
        assert!(output.len() <= TASK_OUTPUT_CAP);
        assert!(output.ends_with("xyz"));
        assert!(tail.chunks.len() <= 2);
    }

    fn test_project_task_run_store(run_id: &str) -> ProjectTaskRunStore {
        let task = ProjectTask {
            version: 1,
            id: "cargo-check".into(),
            label: "Check".into(),
            kind: "verify".into(),
            argv: vec!["cargo".into(), "check".into()],
            provider: "cargo".into(),
            source: "detected".into(),
            root: ".".into(),
            interactive: false,
            background: false,
            default_rank: 180,
            available: true,
            requirements: Vec::new(),
            long_running: false,
            ready_pattern: None,
            problem_matcher: None,
        };
        let (tx, _) = tokio::sync::broadcast::channel(8);
        ProjectTaskRunStore {
            run: ProjectTaskRun {
                run_id: run_id.into(),
                work_id: "work-1".into(),
                state: "running".into(),
                task,
                result: None,
                stdout: String::new(),
                stderr: String::new(),
                output_truncated: false,
                next_seq: 0,
                locations: Vec::new(),
                ready_url: None,
            },
            repository_root: PathBuf::new(),
            working_root: PathBuf::new(),
            ready_re: None,
            problem_re: None,
            stdout: TaskOutputTail::default(),
            stderr: TaskOutputTail::default(),
            chunks: std::collections::VecDeque::new(),
            chunk_bytes: 0,
            tx,
        }
    }

    fn test_project_task_handle(
        run_id: &str,
        terminal_at: Option<std::time::Instant>,
    ) -> Arc<ProjectTaskRunHandle> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
        Arc::new(ProjectTaskRunHandle {
            store: tokio::sync::Mutex::new(test_project_task_run_store(run_id)),
            terminal_at: Mutex::new(terminal_at),
            _memory_permit: semaphore.try_acquire_owned().unwrap(),
        })
    }

    #[test]
    fn task_replay_is_bounded_by_count_and_bytes_with_explicit_gap() {
        let mut store = test_project_task_run_store("run-1");
        for seq in 0..600 {
            push_task_event(
                &mut store,
                ProjectTaskOutputEvent {
                    seq,
                    run_id: "run-1".into(),
                    kind: "output".into(),
                    available_from: None,
                    stream: Some("stdout".into()),
                    text: Some("x".repeat(4096)),
                    state: None,
                    result: None,
                    locations: None,
                    ready_url: None,
                },
            );
        }
        assert!(store.chunks.len() <= TASK_CHUNK_REPLAY_CAP);
        assert!(store.chunk_bytes <= TASK_CHUNK_REPLAY_BYTES);
        let available_from = store.chunks.front().unwrap().0.seq;
        assert!(available_from > 0);
        let gap = task_replay_gap_event("run-1", 0, available_from);
        assert_eq!(gap.kind, "gap");
        assert_eq!(gap.state.as_deref(), Some("replay_gap"));
        assert_eq!(gap.available_from, Some(available_from));
    }

    #[test]
    fn terminal_registry_retention_never_evicts_active_runs() {
        let now = std::time::Instant::now();
        let mut runs = std::collections::HashMap::new();
        runs.insert(
            "expired".into(),
            test_project_task_handle(
                "expired",
                Some(now - PROJECT_TASK_TERMINAL_TTL - std::time::Duration::from_secs(1)),
            ),
        );
        for index in 0..70 {
            let id = format!("terminal-{index}");
            runs.insert(id.clone(), test_project_task_handle(&id, Some(now)));
        }
        for index in 0..3 {
            let id = format!("active-{index}");
            runs.insert(id.clone(), test_project_task_handle(&id, None));
        }

        prune_project_task_runs(&mut runs);

        assert!(!runs.contains_key("expired"));
        assert_eq!(
            runs.keys().filter(|id| id.starts_with("active-")).count(),
            3
        );
        assert_eq!(
            runs.keys().filter(|id| id.starts_with("terminal-")).count(),
            PROJECT_TASK_TERMINAL_CAP
        );
    }

    #[test]
    fn task_run_terminal_requires_final_result_after_cancel() {
        let task = ProjectTask {
            version: 1,
            id: "cargo-check".into(),
            label: "Check".into(),
            kind: "verify".into(),
            argv: vec!["cargo".into(), "check".into()],
            provider: "cargo".into(),
            source: "detected".into(),
            root: ".".into(),
            interactive: false,
            background: false,
            default_rank: 180,
            available: true,
            requirements: Vec::new(),
            long_running: false,
            ready_pattern: None,
            problem_matcher: None,
        };
        let mut run = ProjectTaskRun {
            run_id: "run-1".into(),
            work_id: "work-1".into(),
            state: "cancelled".into(),
            task: task.clone(),
            result: None,
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
            next_seq: 1,
            locations: Vec::new(),
            ready_url: None,
        };
        assert!(!task_run_is_terminal(&run));
        run.result = Some(ProjectTaskResult {
            task,
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
            truncated: false,
            duration_ms: 10,
            locations: Vec::new(),
        });
        assert!(task_run_is_terminal(&run));
        assert!(task_output_event_is_terminal(&ProjectTaskOutputEvent {
            seq: 2,
            run_id: "run-1".into(),
            kind: "state".into(),
            available_from: None,
            stream: None,
            text: None,
            state: Some("cancelled".into()),
            result: run.result.clone(),
            locations: None,
            ready_url: None,
        }));
        assert!(!task_output_event_is_terminal(&ProjectTaskOutputEvent {
            seq: 1,
            run_id: "run-1".into(),
            kind: "state".into(),
            available_from: None,
            stream: None,
            text: None,
            state: Some("cancelled".into()),
            result: None,
            locations: None,
            ready_url: None,
        }));
    }

    #[test]
    fn configured_tasks_json_merges_safe_shell_and_npm_entries() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".vscode")).unwrap();
        std::fs::write(
            root.path().join(".vscode/tasks.json"),
            r#"{
              // project tasks
              "version": "2.0.0",
              "tasks": [
                {
                  "label": "Lint",
                  "type": "npm",
                  "script": "lint",
                  "problemMatcher": {
                    "pattern": {
                      "regexp": "^(.*):(\\d+):(\\d+):\\s+(.*)$",
                      "file": 1,
                      "line": 2,
                      "column": 3,
                      "message": 4
                    }
                  }
                },
                {
                  "label": "Dev server",
                  "type": "shell",
                  "command": "npm",
                  "args": ["run", "dev"],
                  "isBackground": true,
                  "problemMatcher": {
                    "background": { "endsPattern": "Local:" },
                    "pattern": { "regexp": "^(.*):(\\d+):(\\d+):\\s+(.*)$", "file": 1, "line": 2, "column": 3, "message": 4 }
                  }
                },
                { "label": "Danger", "type": "npm", "script": "rm -rf /" }
              ]
            }"#,
        )
        .unwrap();
        let tasks = project_tasks(root.path());
        let lint = tasks
            .iter()
            .find(|task| task.id == "configured-lint")
            .unwrap();
        assert_eq!(lint.argv, vec!["npm", "run", "lint"]);
        assert_eq!(lint.provider, "vscode-tasks");
        assert!(lint.problem_matcher.is_some());
        let dev = tasks
            .iter()
            .find(|task| task.id == "configured-dev-server")
            .unwrap();
        assert!(dev.long_running);
        assert_eq!(dev.ready_pattern.as_deref(), Some("Local:"));
        assert!(!tasks.iter().any(|task| task.label == "Danger"));
    }

    #[test]
    fn readiness_and_matcher_patterns_parse_incremental_output() {
        let root = PathBuf::from("/work/project");
        let matcher = (
            regex::Regex::new(r"^(.*):(\d+):(\d+):\s+(.*)$").unwrap(),
            ProjectProblemPattern {
                regexp: r"^(.*):(\d+):(\d+):\s+(.*)$".into(),
                file: 1,
                line: 2,
                column: Some(3),
                message: Some(4),
            },
        );
        let locations = parse_output_locations_with_matcher(
            &root,
            &root,
            "src/app.ts:10:2: Unexpected token",
            Some(&matcher),
        );
        assert_eq!(locations[0].path, "src/app.ts");
        assert_eq!(locations[0].line, 10);
        assert_eq!(locations[0].column, Some(2));
        assert_eq!(locations[0].message, "Unexpected token");
        let ready = regex::Regex::new(default_ready_pattern()).unwrap();
        assert!(ready.is_match("  ➜  Local:   http://localhost:5173/"));
        assert!(!ready.is_match("compiling modules"));
    }

    #[test]
    fn repository_readiness_errors_keep_domain_specific_statuses() {
        let (status, Json(empty)) = map_err(ForgeError::RepositoryEmpty(PathBuf::from("repo")));
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(empty.kind, Some("repository_empty"));

        let (status, Json(missing)) = map_err(ForgeError::BaseRefMissing {
            repo_path: PathBuf::from("repo"),
            reference: "master".into(),
        });
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(missing.kind, Some("base_ref_missing"));
    }

    #[test]
    fn source_paths_are_repo_relative_and_canonical() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "fn main() {}\n").unwrap();

        let (path, relative) = resolve_source_path(root.path(), "./src/lib.rs").unwrap();
        assert_eq!(relative, "src/lib.rs");
        assert_eq!(
            path,
            std::fs::canonicalize(root.path().join("src/lib.rs")).unwrap()
        );
    }

    #[test]
    fn source_paths_reject_traversal_and_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();

        assert!(resolve_source_path(root.path(), "../outside.rs").is_err());
        assert!(resolve_source_path(root.path(), root.path().to_string_lossy().as_ref()).is_err());
        assert!(resolve_source_path(root.path(), "src").is_err());
        assert!(resolve_source_path(root.path(), ".git/config").is_err());
    }

    #[test]
    fn new_source_paths_create_missing_parents_safely() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();

        let (path, relative) = resolve_new_source_path(root.path(), "./src/new.rs").unwrap();
        assert_eq!(relative, "src/new.rs");
        assert_eq!(
            path,
            std::fs::canonicalize(root.path().join("src"))
                .unwrap()
                .join("new.rs")
        );

        let (nested, nested_relative) =
            resolve_new_source_path(root.path(), "missing/new.rs").unwrap();
        assert_eq!(nested_relative, "missing/new.rs");
        assert!(
            nested
                .parent()
                .is_some_and(|parent| parent.ends_with("missing"))
        );
        assert!(resolve_new_source_path(root.path(), ".git/hooks/new-hook").is_err());
        std::fs::write(root.path().join("src/existing.rs"), "fn existing() {}\n").unwrap();
        assert!(resolve_new_source_path(root.path(), "src/existing.rs").is_err());
    }

    #[test]
    fn source_reads_preview_binary_and_large_text() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("ok.rs"), "fn ok() {}\n").unwrap();
        let ok = read_source_response(&WorkId::from("work-1".to_string()), root.path(), "ok.rs")
            .unwrap();
        assert_eq!(ok.encoding.as_deref(), Some("utf-8"));
        assert!(!ok.preview);
        assert!(!ok.truncated);
        assert!(ok.content.contains("fn ok"));

        std::fs::write(root.path().join("blob.bin"), [0u8, 1, 2, 255, b'A']).unwrap();
        let binary =
            read_source_response(&WorkId::from("work-1".to_string()), root.path(), "blob.bin")
                .unwrap();
        assert_eq!(binary.encoding.as_deref(), Some("binary"));
        assert!(binary.preview);
        assert!(binary.content.contains("Binary file"));
        assert!(binary.content.contains("00000000"));

        let large = "x".repeat(MAX_SOURCE_BYTES + 64);
        std::fs::write(root.path().join("huge.txt"), &large).unwrap();
        let preview =
            read_source_response(&WorkId::from("work-1".to_string()), root.path(), "huge.txt")
                .unwrap();
        assert!(preview.truncated);
        assert!(preview.preview);
        assert_eq!(preview.byte_size, large.len());
        assert_eq!(preview.content.len(), MAX_SOURCE_BYTES);
        assert_eq!(preview.encoding.as_deref(), Some("utf-8"));
    }

    #[test]
    fn workspace_source_edits_apply_mixed_operations_atomically() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("modify.rs"), "old\n").unwrap();
        std::fs::write(root.path().join("move.rs"), "moving\n").unwrap();
        std::fs::write(root.path().join("delete.rs"), "gone\n").unwrap();
        let request = SourceWorkspaceEditRequest {
            preconditions: vec![
                SourceWorkspacePrecondition::Existing {
                    path: "modify.rs".into(),
                    expected_digest: source_digest(b"old\n"),
                },
                SourceWorkspacePrecondition::Existing {
                    path: "move.rs".into(),
                    expected_digest: source_digest(b"moving\n"),
                },
                SourceWorkspacePrecondition::Existing {
                    path: "delete.rs".into(),
                    expected_digest: source_digest(b"gone\n"),
                },
                SourceWorkspacePrecondition::Missing {
                    path: "created.rs".into(),
                },
                SourceWorkspacePrecondition::Missing {
                    path: "moved.rs".into(),
                },
            ],
            operations: vec![
                SourceWorkspaceOperation::Write {
                    path: "modify.rs".into(),
                    content: "new\n".into(),
                },
                SourceWorkspaceOperation::Create {
                    path: "created.rs".into(),
                    content: "created\n".into(),
                },
                SourceWorkspaceOperation::Rename {
                    path: "move.rs".into(),
                    destination: "moved.rs".into(),
                },
                SourceWorkspaceOperation::Delete {
                    path: "delete.rs".into(),
                },
            ],
            lease_id: "lease-1".into(),
            generation: 1,
        };

        let responses = execute_source_workspace_edit(
            &WorkId::from("work-1".to_string()),
            root.path(),
            &request,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(root.path().join("modify.rs")).unwrap(),
            "new\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("created.rs")).unwrap(),
            "created\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("moved.rs")).unwrap(),
            "moving\n"
        );
        assert!(!root.path().join("move.rs").exists());
        assert!(!root.path().join("delete.rs").exists());
        assert_eq!(
            responses
                .iter()
                .map(|response| response.path.as_str())
                .collect::<Vec<_>>(),
            vec!["created.rs", "modify.rs", "moved.rs"]
        );
    }

    #[test]
    fn workspace_source_edits_support_create_edit_rename_order() {
        let root = tempfile::tempdir().unwrap();
        let request = SourceWorkspaceEditRequest {
            preconditions: vec![
                SourceWorkspacePrecondition::Missing {
                    path: "temporary.ts".into(),
                },
                SourceWorkspacePrecondition::Missing {
                    path: "final.ts".into(),
                },
            ],
            operations: vec![
                SourceWorkspaceOperation::Create {
                    path: "temporary.ts".into(),
                    content: String::new(),
                },
                SourceWorkspaceOperation::Write {
                    path: "temporary.ts".into(),
                    content: "export const ready = true;\n".into(),
                },
                SourceWorkspaceOperation::Rename {
                    path: "temporary.ts".into(),
                    destination: "final.ts".into(),
                },
            ],
            lease_id: "lease-1".into(),
            generation: 1,
        };

        execute_source_workspace_edit(&WorkId::from("work-1".to_string()), root.path(), &request)
            .unwrap();

        assert!(!root.path().join("temporary.ts").exists());
        assert_eq!(
            std::fs::read_to_string(root.path().join("final.ts")).unwrap(),
            "export const ready = true;\n"
        );
    }

    #[test]
    fn workspace_source_edit_validation_leaves_every_file_unchanged() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("keep.rs"), "original\n").unwrap();
        let request = SourceWorkspaceEditRequest {
            preconditions: vec![SourceWorkspacePrecondition::Existing {
                path: "keep.rs".into(),
                expected_digest: source_digest(b"original\n"),
            }],
            operations: vec![
                SourceWorkspaceOperation::Write {
                    path: "keep.rs".into(),
                    content: "changed\n".into(),
                },
                SourceWorkspaceOperation::Delete {
                    path: "keep.rs".into(),
                },
                SourceWorkspaceOperation::Delete {
                    path: "keep.rs".into(),
                },
            ],
            lease_id: "lease-1".into(),
            generation: 1,
        };

        assert!(
            execute_source_workspace_edit(
                &WorkId::from("work-1".to_string()),
                root.path(),
                &request,
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("keep.rs")).unwrap(),
            "original\n"
        );
    }

    #[test]
    fn workspace_source_edits_require_digest_or_absence_for_every_path() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("unfenced.rs"), "original\n").unwrap();
        let request = SourceWorkspaceEditRequest {
            preconditions: vec![],
            operations: vec![SourceWorkspaceOperation::Write {
                path: "unfenced.rs".into(),
                content: "changed\n".into(),
            }],
            lease_id: "lease-1".into(),
            generation: 1,
        };

        assert!(
            execute_source_workspace_edit(
                &WorkId::from("work-1".to_string()),
                root.path(),
                &request,
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("unfenced.rs")).unwrap(),
            "original\n"
        );
    }

    #[test]
    fn workspace_source_edits_reject_stale_and_unused_preconditions() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("target.rs"), "current\n").unwrap();
        std::fs::write(root.path().join("extra.rs"), "extra\n").unwrap();

        let stale = SourceWorkspaceEditRequest {
            preconditions: vec![SourceWorkspacePrecondition::Existing {
                path: "target.rs".into(),
                expected_digest: source_digest(b"stale\n"),
            }],
            operations: vec![SourceWorkspaceOperation::Write {
                path: "target.rs".into(),
                content: "changed\n".into(),
            }],
            lease_id: "lease-1".into(),
            generation: 1,
        };
        assert!(
            execute_source_workspace_edit(
                &WorkId::from("work-1".to_string()),
                root.path(),
                &stale,
            )
            .is_err()
        );

        let unused = SourceWorkspaceEditRequest {
            preconditions: vec![
                SourceWorkspacePrecondition::Existing {
                    path: "target.rs".into(),
                    expected_digest: source_digest(b"current\n"),
                },
                SourceWorkspacePrecondition::Existing {
                    path: "extra.rs".into(),
                    expected_digest: source_digest(b"extra\n"),
                },
            ],
            operations: vec![SourceWorkspaceOperation::Write {
                path: "target.rs".into(),
                content: "changed\n".into(),
            }],
            lease_id: "lease-1".into(),
            generation: 1,
        };
        assert!(
            execute_source_workspace_edit(
                &WorkId::from("work-1".to_string()),
                root.path(),
                &unused,
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("target.rs")).unwrap(),
            "current\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("extra.rs")).unwrap(),
            "extra\n"
        );
    }

    #[test]
    fn code_workspace_state_keeps_backward_compatible_editor_groups() {
        let legacy: CodeWorkspaceState = serde_json::from_value(serde_json::json!({
            "tabs": [{
                "path": "src/lib.rs",
                "source_digest": "sha256:old",
                "line": 12
            }],
            "active_path": "src/lib.rs"
        }))
        .unwrap();
        assert_eq!(legacy.active_path.as_deref(), Some("src/lib.rs"));
        assert!(legacy.secondary_path.is_none());
        assert!(legacy.layout.is_none());

        let split = CodeWorkspaceState {
            tabs: legacy.tabs,
            active_path: legacy.active_path,
            secondary_path: Some("src/main.rs".into()),
            layout: Some(CodeWorkspaceLayout {
                context_panel: Some("problems".into()),
                terminal: true,
                tests: false,
                search: false,
                changes: false,
                primary_task: Some("cargo-run".into()),
            }),
            updated_at: None,
        };
        let encoded = serde_json::to_value(split).unwrap();
        assert_eq!(encoded["secondary_path"], "src/main.rs");
        assert_eq!(encoded["layout"]["context_panel"], "problems");
        assert_eq!(encoded["layout"]["terminal"], true);
        assert_eq!(encoded["layout"]["primary_task"], "cargo-run");
        assert!(encoded["layout"].get("tests").is_none());
    }

    #[test]
    fn source_tree_includes_untracked_but_not_ignored_files() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        std::fs::write(root.path().join("tracked.rs"), "fn tracked() {}\n").unwrap();
        std::fs::write(root.path().join("new.rs"), "fn new() {}\n").unwrap();
        std::fs::write(root.path().join(".gitignore"), "target/\n").unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::fs::write(root.path().join("target/noise.rs"), "ignored\n").unwrap();
        git(&["add", "tracked.rs", ".gitignore"]);

        let tree = list_source_tree(&WorkId::from("work-1".to_string()), root.path()).unwrap();
        let paths = tree
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"tracked.rs"));
        assert!(paths.contains(&"new.rs"));
        assert!(!paths.contains(&"target/noise.rs"));
        assert_eq!(
            tree.files
                .iter()
                .find(|file| file.path == "new.rs")
                .and_then(|file| file.status.as_deref()),
            Some("??"),
        );
        assert!(!tree.truncated);
    }

    #[test]
    fn repository_search_finds_untracked_honors_ignore_and_paginates() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        std::fs::write(root.path().join("tracked.rs"), "fn alpha_hit() {}\n").unwrap();
        std::fs::write(root.path().join("fresh.rs"), "fn alpha_hit() {}\n").unwrap();
        std::fs::write(root.path().join(".gitignore"), "secret/\n").unwrap();
        std::fs::create_dir(root.path().join("secret")).unwrap();
        std::fs::write(root.path().join("secret/hidden.rs"), "fn alpha_hit() {}\n").unwrap();
        git(&["add", "tracked.rs", ".gitignore"]);

        let options = SourceSearchOptions {
            needle: "alpha_hit".into(),
            regex: false,
            case_sensitive: true,
            whole_word: false,
            include: Vec::new(),
            exclude: Vec::new(),
            include_ignored: false,
            changed_only: false,
            limit: 1,
            skip: 0,
        };
        let (page1, truncated, next) = run_repository_search(root.path(), &options).unwrap();
        assert_eq!(page1.len(), 1);
        assert!(truncated);
        assert_eq!(next.as_deref(), Some("1"));
        let paths: Vec<_> = page1.iter().map(|hit| hit.path.as_str()).collect();
        assert!(
            paths.contains(&"tracked.rs") || paths.contains(&"fresh.rs"),
            "{paths:?}"
        );

        let page2 = run_repository_search(
            root.path(),
            &SourceSearchOptions {
                skip: 1,
                limit: 10,
                ..options.clone()
            },
        )
        .unwrap();
        assert!(!page2.0.is_empty());
        let all_paths: Vec<_> = page1
            .iter()
            .chain(page2.0.iter())
            .map(|hit| hit.path.as_str())
            .collect();
        assert!(all_paths.contains(&"tracked.rs"));
        assert!(all_paths.contains(&"fresh.rs"));
        assert!(!all_paths.contains(&"secret/hidden.rs"));

        let regex_hits = run_repository_search(
            root.path(),
            &SourceSearchOptions {
                needle: "alpha_h.t".into(),
                regex: true,
                limit: 50,
                skip: 0,
                ..options.clone()
            },
        )
        .unwrap()
        .0;
        assert!(regex_hits.len() >= 2);

        let include_hits = run_repository_search(
            root.path(),
            &SourceSearchOptions {
                include: vec!["fresh.rs".into()],
                limit: 50,
                skip: 0,
                ..options.clone()
            },
        )
        .unwrap()
        .0;
        assert_eq!(include_hits.len(), 1);
        assert_eq!(include_hits[0].path, "fresh.rs");
    }

    #[test]
    fn repository_replace_plans_and_applies_with_digest_fencing() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        std::fs::write(root.path().join("a.rs"), "fn alpha_hit() {}\n").unwrap();
        std::fs::write(root.path().join("b.rs"), "fn alpha_hit() {}\n").unwrap();
        git(&["add", "a.rs", "b.rs"]);

        let options = SourceSearchOptions {
            needle: "alpha_hit".into(),
            regex: false,
            case_sensitive: true,
            whole_word: false,
            include: Vec::new(),
            exclude: Vec::new(),
            include_ignored: false,
            changed_only: false,
            limit: 500,
            skip: 0,
        };
        let (plan, truncated) =
            run_repository_replace_plan(root.path(), &options, "beta_hit", 50, None).unwrap();
        assert!(!truncated);
        assert_eq!(plan.len(), 2);
        assert!(plan.iter().all(|file| file.match_count == 1));
        assert!(plan.iter().all(|file| file.after.contains("beta_hit")));

        let preconditions: Vec<_> = plan
            .iter()
            .map(|file| SourceReplacePrecondition {
                path: file.path.clone(),
                expected_digest: file.expected_digest.clone(),
            })
            .collect();
        apply_repository_replace_plan(root.path(), &plan, &preconditions).unwrap();
        assert!(
            std::fs::read_to_string(root.path().join("a.rs"))
                .unwrap()
                .contains("beta_hit")
        );

        let stale = apply_repository_replace_plan(root.path(), &plan, &preconditions);
        assert!(stale.is_err());
    }

    #[test]
    fn project_tasks_are_inferred_from_manifests_not_arbitrary_input() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("package.json"),
            r#"{"scripts":{"check":"svelte-check","build":"vite build","dev":"vite","custom":"danger"}}"#,
        )
        .unwrap();
        let tasks = detected_project_tasks(root.path());
        assert!(tasks.iter().any(|task| task.id == "npm-check"));
        assert!(tasks.iter().any(|task| task.id == "npm-build"));
        assert!(
            tasks
                .iter()
                .any(|task| task.id == "npm-dev" && task.long_running)
        );
        assert!(!tasks.iter().any(|task| task.id == "custom"));
    }

    #[test]
    fn project_tasks_include_bounded_nested_roots_and_lockfile_package_managers() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        let app = root.path().join("apps/web");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            app.join("package.json"),
            r#"{"scripts":{"build":"vite build","dev":"vite","lint":"eslint ."}}"#,
        )
        .unwrap();
        std::fs::write(
            root.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .unwrap();

        let tasks = detected_project_tasks(root.path());
        let build = tasks
            .iter()
            .find(|task| task.id.starts_with("pnpm-build@apps-web-"))
            .expect("nested build task");
        assert_eq!(build.root, "apps/web");
        assert_eq!(build.argv, ["pnpm", "run", "build"]);
        assert_eq!(build.source, "package");
        assert!(tasks.iter().any(|task| {
            task.id.starts_with("pnpm-dev@apps-web-")
                && task.kind == "run"
                && task.background
                && task.default_rank == 500
                && task.long_running
        }));
        assert!(
            tasks
                .iter()
                .any(|task| task.id.starts_with("pnpm-lint@apps-web-"))
        );
    }

    #[test]
    fn cargo_tasks_offer_build_and_only_unambiguous_run() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::create_dir_all(root.path().join("src/bin")).unwrap();
        std::fs::create_dir_all(root.path().join("examples")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.path().join("src/bin/worker.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.path().join("examples/hello.rs"), "fn main() {}\n").unwrap();
        let tasks = detected_project_tasks(root.path());
        assert!(tasks.iter().any(|task| task.id == "cargo-build"));
        assert!(tasks.iter().any(|task| {
            task.id == "cargo-run"
                && task.kind == "run"
                && !task.background
                && task.argv == ["cargo", "run"]
        }));
        assert!(tasks.iter().any(|task| {
            task.id == "cargo-run-bin-worker" && task.argv == ["cargo", "run", "--bin", "worker"]
        }));
        assert!(tasks.iter().any(|task| {
            task.id == "cargo-run-example-hello"
                && task.argv == ["cargo", "run", "--example", "hello"]
        }));
    }

    #[test]
    fn python_and_dotnet_application_entry_points_are_detected() {
        let python = tempfile::tempdir().unwrap();
        std::fs::write(
            python.path().join("pyproject.toml"),
            "[project]\nname = \"demo\"\nversion = \"0.1.0\"\n[project.scripts]\nserve = \"demo.cli:main\"\n",
        )
        .unwrap();
        std::fs::write(python.path().join("uv.lock"), "version = 1\n").unwrap();
        std::fs::create_dir(python.path().join("demo")).unwrap();
        std::fs::write(python.path().join("demo/__main__.py"), "print('ok')\n").unwrap();
        let python_tasks = detected_project_tasks(python.path());
        assert!(python_tasks.iter().any(|task| {
            task.id == "python-run"
                && matches!(
                    task.argv.first().map(String::as_str),
                    Some("python" | "python3")
                )
                && task
                    .argv
                    .get(1..)
                    .is_some_and(|argv| argv == ["-m", "demo"])
        }));
        assert!(python_tasks.iter().any(|task| {
            task.id == "python-script-serve" && task.argv == ["uv", "run", "serve"]
        }));

        let dotnet = tempfile::tempdir().unwrap();
        std::fs::write(
            dotnet.path().join("Demo.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType></PropertyGroup></Project>"#,
        )
        .unwrap();
        let dotnet_tasks = detected_project_tasks(dotnet.path());
        assert!(dotnet_tasks.iter().any(|task| {
            task.id == "dotnet-run-demo-csproj"
                && task.argv == ["dotnet", "run", "--project", "Demo.csproj"]
        }));
    }

    #[test]
    fn project_task_roots_cannot_escape_the_repository() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("apps/web")).unwrap();
        let nested = resolve_project_task_root(root.path(), "apps/web").unwrap();
        assert_eq!(
            nested,
            std::fs::canonicalize(root.path().join("apps/web")).unwrap()
        );
        assert!(resolve_project_task_root(root.path(), "../outside").is_err());
        assert!(resolve_project_task_root(root.path(), "/tmp").is_err());
    }

    #[test]
    fn project_task_root_is_backward_compatible_and_scoped_ids_do_not_alias() {
        let task: ProjectTask = serde_json::from_value(serde_json::json!({
            "id": "cargo-check",
            "label": "Check",
            "kind": "verify",
            "argv": ["cargo", "check"],
            "provider": "cargo"
        }))
        .unwrap();
        assert_eq!(task.root, ".");
        assert_eq!(task.version, 1);
        assert_eq!(task.source, "detected");
        assert!(task.available);
        assert_ne!(
            scoped_task_id("npm-build", "apps/web"),
            scoped_task_id("npm-build", "apps_web")
        );
    }

    #[test]
    fn project_task_requirements_include_actionable_missing_executable_health() {
        let root = tempfile::tempdir().unwrap();
        let mut task: ProjectTask = serde_json::from_value(serde_json::json!({
            "id": "missing-check",
            "label": "Check",
            "kind": "verify",
            "argv": ["medousa-definitely-missing-task-command", "check"],
            "provider": "fixture"
        }))
        .unwrap();
        annotate_task_requirements(root.path(), std::slice::from_mut(&mut task));
        assert!(!task.available);
        assert_eq!(
            task.requirements[0].name,
            "medousa-definitely-missing-task-command"
        );
        assert!(
            task.requirements[0]
                .repair
                .as_deref()
                .is_some_and(|repair| repair.contains("workshop machine"))
        );
    }

    #[test]
    fn package_task_health_reports_the_correct_install_root() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("apps/web");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            root.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .unwrap();
        std::fs::write(
            app.join("package.json"),
            r#"{"scripts":{"build":"vite build"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();
        let mut tasks = Vec::new();
        detected_tasks_for_root(root.path(), "apps/web", &mut tasks);
        annotate_task_requirements(root.path(), &mut tasks);
        let build = tasks.iter().find(|task| task.kind == "build").unwrap();
        let package = build
            .requirements
            .iter()
            .find(|requirement| requirement.kind == "package")
            .unwrap();
        assert!(!package.available);
        assert!(package.repair.as_deref().is_some_and(
            |repair| repair.contains("pnpm install") && repair.contains("project root")
        ));

        std::fs::create_dir(root.path().join("node_modules")).unwrap();
        annotate_task_requirements(root.path(), &mut tasks);
        assert!(
            tasks
                .iter()
                .find(|task| task.kind == "build")
                .unwrap()
                .requirements
                .iter()
                .find(|requirement| requirement.kind == "package")
                .is_some_and(|requirement| requirement.available)
        );
    }

    #[test]
    fn project_output_locations_stay_repository_relative() {
        let root = PathBuf::from("/work/project");
        let locations = parse_output_locations(
            &root,
            &root,
            "error --> src/lib.rs:42:7\nat /work/project/tests/app.test.ts:9:2",
        );
        assert_eq!(locations[0].path, "src/lib.rs");
        assert_eq!(locations[0].line, 42);
        assert_eq!(locations[1].path, "tests/app.test.ts");
    }

    #[test]
    fn nested_task_output_locations_are_repository_relative() {
        let root = PathBuf::from("/work/project");
        let working = root.join("apps/web");
        let locations =
            parse_output_locations(&root, &working, "src/app.ts:12:3: Unexpected token");
        assert_eq!(locations[0].path, "apps/web/src/app.ts");
    }

    #[test]
    fn discovers_rust_tests_without_executing_project_code() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("lib.rs"),
            "#[test]\nfn intent_stays_clear() {}\n",
        )
        .unwrap();
        let tasks = detected_project_tasks(root.path());
        let tests = discover_project_tests(root.path(), &tasks);
        assert!(tests.iter().any(|test| test.label == "intent_stays_clear"));
    }

    #[test]
    fn review_patch_is_parsed_into_addressable_hunks() {
        let hunks = parse_review_hunks(
            "diff --git a/app.txt b/app.txt\n--- a/app.txt\n+++ b/app.txt\n@@ -1,2 +1,3 @@\n first\n-old\n+new\n+third\n",
        );
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[0].new_count, 3);
        assert_eq!(hunks[0].lines[1].kind, "deletion");
        assert_eq!(hunks[0].lines[2].new_line, Some(2));
        assert_eq!(hunks[0].lines[3].new_line, Some(3));
    }

    #[test]
    fn repository_browser_stays_inside_workshop_places() {
        let roots = vec![PathBuf::from("/workspaces"), PathBuf::from("/srv/code")];
        assert!(browse_path_allowed(
            FsPath::new("/workspaces/team/repo"),
            &roots
        ));
        assert!(browse_path_allowed(
            FsPath::new("/srv/code/project"),
            &roots
        ));
        assert!(!browse_path_allowed(FsPath::new("/etc"), &roots));
        assert!(!browse_path_allowed(FsPath::new("/srv/other"), &roots));
    }

    #[cfg(windows)]
    #[test]
    fn repository_browser_exposes_the_current_windows_drive() {
        let current = std::env::current_dir().unwrap().canonicalize().unwrap();
        let roots = windows_repository_browse_roots();
        assert!(browse_path_allowed(&current, &roots));
    }

    #[test]
    fn repository_browser_marks_git_worktrees() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("project");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let entry = browse_entry(repo.clone());
        assert_eq!(entry.name, "project");
        assert_eq!(entry.path, repo);
        assert!(entry.repository);
    }

    #[test]
    fn provider_repositories_accept_supported_urls_and_nested_namespaces() {
        assert_eq!(
            provider_repository("git@github.com:EntasisLabs/Medousa.git"),
            Some(("github", "EntasisLabs/Medousa".into()))
        );
        assert_eq!(
            provider_repository("https://gitlab.com/team/platform/service.git"),
            Some(("gitlab", "team/platform/service".into()))
        );
        assert_eq!(
            normalize_provider_repository("team/platform/service"),
            Some("team/platform/service".into())
        );
    }

    #[test]
    fn provider_repositories_reject_options_and_path_traversal() {
        assert!(normalize_provider_repository("--upload-pack=malicious").is_none());
        assert!(normalize_provider_repository("team/../service").is_none());
        assert!(normalize_provider_repository("lonely-project").is_none());
        assert!(provider_repository("ssh://example.com/team/service.git").is_none());
        assert!(provider_repository("https://evilgithub.com/team/service.git").is_none());
    }

    #[test]
    fn blank_project_names_are_safe_and_initialize_a_commit() {
        assert_eq!(
            project_slug(" Personal Finance / Dashboard "),
            "personal-finance-dashboard"
        );
        assert_eq!(project_slug("!!!"), "new-project");
        let root = tempfile::tempdir().unwrap();
        initialize_blank_repository(root.path(), "Finance Dashboard", "main").unwrap();
        assert!(root.path().join("README.md").is_file());
        let head = background_command("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(head.status.success());
    }
}
