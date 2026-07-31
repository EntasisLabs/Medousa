//! HTTP control plane for Forge undertakings (`/v1/forge/...`).
//!
//! Distinct from `/v1/workspace/cards` (activity board) and vault Versions
//! (material memory). Forge owns custody of intentional work episodes.

use std::ffi::OsStr;
use std::path::{Component, Path as FsPath, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use medousa_forge::adapter::{ScriptAdapter, export_bundle};
use medousa_forge::error::ForgeError;
use medousa_forge::forge::{Forge, SealOptions};
use medousa_forge::git::{CheckpointAuthor, GitEngine};
use medousa_forge::model::{
    ActorKind, ActorRef, EvidenceId, ExecutionLease, ExecutorDescriptor, GitOid,
    IntegrationStrategy, LeaseId, RecoveryDisposition, ReviewDecision, ReviewDecisionId, WorkId,
    WorkItem, WorkPolicy, WorkState, WorkTarget,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::daemon::forge_projections::{
    ItemProjection, ReviewProjection, build_review, evidence_dir, project_item, project_items,
    read_lines_page,
};
use crate::daemon::state::AppState;

fn publish_item(state: &AppState, item: &WorkItem, kind: &str) {
    state
        .forge_events
        .publish(item.id.as_str(), &item.state.to_string(), kind);
}

fn ok_item(state: &AppState, item: WorkItem, kind: &str) -> Json<ItemProjection> {
    publish_item(state, &item, kind);
    Json(project_item(item))
}

pub fn forge_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/forge/items", get(list_items).post(register_item))
        .route("/v1/forge/items/start", post(start_item))
        .route("/v1/forge/repositories/inspect", post(inspect_repository))
        .route(
            "/v1/forge/repositories/provider",
            get(provider_repository_capabilities).post(clone_provider_repository),
        )
        .route(
            "/v1/forge/repositories",
            get(list_repositories).put(update_repository_pin),
        )
        .route("/v1/forge/repositories/browse", get(browse_repositories))
        .route("/v1/forge/items/{work_id}", get(get_item))
        .route(
            "/v1/forge/items/{work_id}/source",
            get(read_source)
                .post(create_source)
                .put(save_source)
                .patch(rename_source)
                .delete(delete_source),
        )
        .route("/v1/forge/items/{work_id}/tree", get(source_tree))
        .route(
            "/v1/forge/items/{work_id}/source/batch",
            put(save_source_batch),
        )
        .route("/v1/forge/items/{work_id}/search", get(search_source))
        .route(
            "/v1/forge/items/{work_id}/workspace-state",
            get(read_workspace_state).put(save_workspace_state),
        )
        .route("/v1/forge/items/{work_id}/review", get(get_review))
        .route(
            "/v1/forge/items/{work_id}/review/file",
            get(get_review_file).post(restore_review_file),
        )
        .route("/v1/forge/items/{work_id}/tasks", get(list_project_tasks))
        .route(
            "/v1/forge/items/{work_id}/tasks/{task_id}/run",
            post(run_project_task),
        )
        .route(
            "/v1/forge/items/{work_id}/tasks/{task_id}/runs",
            post(start_project_task_run),
        )
        .route(
            "/v1/forge/items/{work_id}/task-runs/{run_id}",
            get(get_project_task_run).delete(cancel_project_task_run),
        )
        .route("/v1/forge/items/{work_id}/tests", get(list_project_tests))
        .route("/v1/forge/items/{work_id}/provision", post(provision_item))
        .route("/v1/forge/items/{work_id}/attempts", post(begin_attempt))
        .route("/v1/forge/items/{work_id}/handoff", post(prepare_handoff))
        .route(
            "/v1/forge/items/{work_id}/provider",
            get(get_provider_handoff).post(share_provider_handoff),
        )
        .route(
            "/v1/forge/items/{work_id}/provider/context",
            put(save_provider_context),
        )
        .route(
            "/v1/forge/items/{work_id}/provider/comments",
            get(list_provider_comments).post(import_provider_comment),
        )
        .route("/v1/forge/items/{work_id}/decisions", post(record_decision))
        .route("/v1/forge/items/{work_id}/apply", post(apply_decision))
        .route("/v1/forge/items/{work_id}/discard", post(discard_item))
        .route("/v1/forge/items/{work_id}/run-script", post(run_script))
        .route("/v1/forge/items/{work_id}/export", post(export_item))
        .route(
            "/v1/forge/evidence/{evidence_id}/patch",
            get(evidence_patch),
        )
        .route(
            "/v1/forge/evidence/{evidence_id}/commands",
            get(evidence_commands),
        )
        .route("/v1/forge/stream", get(forge_stream))
        .route(
            "/v1/forge/leases/{lease_id}/heartbeat",
            post(heartbeat_lease),
        )
        .route("/v1/forge/leases/{lease_id}/complete", post(complete_lease))
        .route(
            "/v1/forge/leases/{lease_id}/interrupt",
            post(interrupt_lease),
        )
        .route("/v1/forge/leases/{lease_id}/fail", post(fail_lease))
        .with_state(state)
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
        ForgeError::BaseRefMissing { .. } => {
            (StatusCode::CONFLICT, Some("base_ref_missing"))
        }
        ForgeError::Git(_) => (StatusCode::BAD_REQUEST, Some("git")),
        ForgeError::Store(_) | ForgeError::Io(_) | ForgeError::Json(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Some("store"))
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

fn actor_from_state(state: &AppState) -> ActorRef {
    ActorRef {
        kind: ActorKind::User,
        id: state.workshop_identity_user_id(),
    }
}

fn parse_work_id(raw: &str) -> ApiResult<WorkId> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorBody {
                error: "work_id is required".into(),
                kind: Some("bad_request"),
            }),
        ));
    }
    Ok(WorkId::from(trimmed.to_string()))
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
    let parent = relative.parent().unwrap_or_else(|| FsPath::new(""));
    let parent = std::fs::canonicalize(root.join(parent)).map_err(|err| {
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

fn forge(state: &AppState) -> Arc<Forge> {
    state.forge.clone()
}

/// Resolve a presented lease by scanning active attempts for matching
/// `lease_id` + fencing `generation`.
fn resolve_lease(forge: &Forge, lease_id: &str, generation: u64) -> ApiResult<ExecutionLease> {
    let want = LeaseId::from(lease_id.to_string());
    let items = forge.list().map_err(map_err)?;
    for item in items {
        let Some(active_id) = &item.active_attempt else {
            continue;
        };
        let Some(attempt) = item.attempt(active_id) else {
            continue;
        };
        let Some(lease) = &attempt.lease else {
            continue;
        };
        if lease.lease_id == want {
            if lease.generation != generation {
                return Err(map_err(ForgeError::StaleLease {
                    presented: want,
                    presented_generation: generation,
                    active: lease.lease_id.clone(),
                    active_generation: lease.generation,
                }));
            }
            return Ok(lease.clone());
        }
    }
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            error: format!("active lease not found: {lease_id}"),
            kind: Some("not_found"),
        }),
    ))
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
    existing_projects: Vec<ExistingRepositoryProject>,
    state_explanation: String,
    trust_explanation: String,
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
    last_used_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct RepositoryCatalogEntry {
    #[serde(flatten)]
    repository: RepositoryInspection,
    pinned: bool,
    last_used_at: chrono::DateTime<chrono::Utc>,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateRepositoryPinRequest {
    path: PathBuf,
    pinned: bool,
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
    let _guard = REPOSITORY_CATALOG_LOCK
        .lock()
        .map_err(|_| request_error(StatusCode::INTERNAL_SERVER_ERROR, "repository catalog lock failed"))?;
    let mut store = read_repository_catalog_unlocked();
    if let Some(entry) = store.entries.iter_mut().find(|entry| entry.path == canonical) {
        entry.last_used_at = chrono::Utc::now();
        if let Some(pinned) = pinned {
            entry.pinned = pinned;
        }
    } else {
        store.entries.push(RepositoryCatalogRecord {
            path: canonical,
            pinned: pinned.unwrap_or(false),
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

fn existing_projects_for_repository(
    items: &[WorkItem],
    path: &FsPath,
) -> Vec<ExistingRepositoryProject> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    items
        .iter()
        .filter(|item| !matches!(item.state, WorkState::Discarded | WorkState::Accepted))
        .filter(|item| match &item.target {
            WorkTarget::Git(target) => target
                .repo_path
                .canonicalize()
                .unwrap_or_else(|_| target.repo_path.clone())
                == canonical,
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
            if changed_files == 1 { "change" } else { "changes" }
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
        existing_projects,
        state_explanation,
        trust_explanation: "Medousa may read this repository and create an isolated working copy. Project commands run only when you explicitly choose a check or Terminal action.".into(),
    })
}

fn inspect_repository_path(state: &AppState, requested: &FsPath) -> ApiResult<RepositoryInspection> {
    let items = forge(state).list().map_err(map_err)?;
    inspect_repository_path_from_items(requested, &items)
}

async fn register_item(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Json<ItemProjection>> {
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

async fn inspect_repository(
    State(state): State<AppState>,
    Json(body): Json<InspectRepositoryRequest>,
) -> ApiResult<Json<RepositoryInspection>> {
    let repository = inspect_repository_path(&state, &body.path)?;
    touch_repository(&repository.path, None)?;
    Ok(Json(repository))
}

async fn list_repositories(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<RepositoryCatalogEntry>>> {
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
                    existing_projects: existing_projects_for_repository(&items, &record.path),
                    state_explanation: "This repository is not currently available on the connected workshop.".into(),
                    trust_explanation: "No files or commands can be accessed until this workshop repository is available again.".into(),
                }
            });
            RepositoryCatalogEntry {
                repository,
                pinned: record.pinned,
                last_used_at: record.last_used_at,
                available,
            }
        })
        .collect();
    Ok(Json(entries))
}

async fn update_repository_pin(
    State(state): State<AppState>,
    Json(body): Json<UpdateRepositoryPinRequest>,
) -> ApiResult<Json<Vec<RepositoryCatalogEntry>>> {
    let repository = inspect_repository_path(&state, &body.path)?;
    touch_repository(&repository.path, Some(body.pinned))?;
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
        if let Some(parent) = record.path.parent().and_then(|path| path.canonicalize().ok()) {
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
    let roots = repository_browse_roots(&state);
    let requested = query
        .path
        .or_else(dirs::home_dir)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "workshop home folder is unavailable"))?;
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
        let Ok(candidate) = entry.path().canonicalize() else { continue };
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
        right
            .repository
            .cmp(&left.repository)
            .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
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

fn provider_adapter(provider: &'static str, label: &'static str, command: &str) -> ProviderRepositoryAdapter {
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
    let (provider, command, label) = match body.provider.trim().to_ascii_lowercase().as_str() {
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
            format!("Install and sign in to the {label} CLI on the connected workshop."),
        ));
    }
    let repository = normalize_provider_repository(&body.repository).ok_or_else(|| {
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
    if !parent.is_dir() || !browse_path_allowed(&parent, &repository_browse_roots(&state)) {
        return Err(request_error(
            StatusCode::FORBIDDEN,
            "Destination is outside the connected workshop's available places",
        ));
    }
    let name = repository
        .rsplit('/')
        .next()
        .ok_or_else(|| request_error(StatusCode::BAD_REQUEST, "Repository name is unavailable"))?;
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

async fn start_item(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Json<ItemProjection>> {
    let repository_path = body.repo_path.clone();
    let actor = actor_from_state(&state);
    let owner = body
        .owner
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let forge = forge(&state);
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
    publish_item(&state, &registered, "registered");
    let item = forge.provision(&registered.id, &actor).map_err(map_err)?;
    if let Some(env) = item.environment.as_ref() {
        crate::daemon::detamu_host::spawn_index_forge_item(
            state.detamu.clone(),
            item.id.as_str().to_owned(),
            env.worktree.clone(),
            env.baseline_oid.as_str().to_owned(),
            crate::daemon::detamu_host::BindingKind::Baseline,
        );
    }
    Ok(ok_item(&state, item, "started"))
}

async fn list_items(State(state): State<AppState>) -> ApiResult<Json<Vec<ItemProjection>>> {
    let items = forge(&state).list().map_err(map_err)?;
    Ok(Json(project_items(items)))
}

async fn get_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    Ok(Json(project_item(item)))
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
struct CreateSourceRequest {
    path: String,
    #[serde(default)]
    content: String,
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
struct CodeWorkspaceState {
    #[serde(default)]
    tabs: Vec<CodeWorkspaceTabState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary_path: Option<String>,
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

fn read_source_response(work_id: &WorkId, root: &FsPath, raw: &str) -> ApiResult<SourceResponse> {
    let (path, relative) = resolve_source_path(root, raw)?;
    let bytes = std::fs::read(&path).map_err(|err| {
        request_error(
            StatusCode::NOT_FOUND,
            format!("could not read source file: {err}"),
        )
    })?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(request_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("source file exceeds the {MAX_SOURCE_BYTES} byte editor limit"),
        ));
    }
    let digest = source_digest(&bytes);
    let byte_size = bytes.len();
    let content = String::from_utf8(bytes).map_err(|_| {
        request_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "binary files cannot be opened in the source editor",
        )
    })?;
    Ok(SourceResponse {
        work_id: work_id.as_str().to_owned(),
        path: relative,
        content,
        digest,
        byte_size,
    })
}

async fn read_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<SourceQuery>,
) -> ApiResult<Json<SourceResponse>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    let environment = item.environment.ok_or_else(|| {
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

fn require_work_lease(
    state: &AppState,
    work_id: &WorkId,
    lease_id: &str,
    generation: u64,
) -> ApiResult<WorkItem> {
    let lease = resolve_lease(forge(state).as_ref(), lease_id.trim(), generation)?;
    if &lease.work_id != work_id {
        return Err(request_error(
            StatusCode::CONFLICT,
            "the presented lease belongs to a different undertaking",
        ));
    }
    forge(state).load(work_id).map_err(map_err)
}

async fn create_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<CreateSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    let id = parse_work_id(&work_id)?;
    if body.content.len() > MAX_SOURCE_BYTES {
        return Err(request_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("source file exceeds the {MAX_SOURCE_BYTES} byte editor limit"),
        ));
    }
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let environment = item
        .environment
        .as_ref()
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
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
    publish_item(&state, &item, "source_created");
    Ok(Json(read_source_response(
        &id,
        &environment.worktree,
        &body.path,
    )?))
}

async fn source_tree(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<SourceTreeResponse>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    let environment = item.environment.ok_or_else(|| {
        request_error(
            StatusCode::CONFLICT,
            "prepare the governed workspace before browsing source files",
        )
    })?;
    let worktree = environment.worktree.clone();
    let listed = tokio::task::spawn_blocking(move || list_source_tree(&id, &worktree))
        .await
        .map_err(|err| {
            request_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("source tree enumeration failed: {err}"),
            )
        })??;
    Ok(Json(listed))
}

async fn search_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Query(query): Query<SourceSearchQuery>,
) -> ApiResult<Json<SourceSearchResponse>> {
    use std::io::BufRead;
    use std::process::Stdio;

    let id = parse_work_id(&work_id)?;
    let needle = query.query.trim();
    if needle.len() < 2 || needle.len() > 200 {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "repository search must be between 2 and 200 characters",
        ));
    }
    let item = forge(&state).load(&id).map_err(map_err)?;
    let environment = item.environment.ok_or_else(|| {
        request_error(
            StatusCode::CONFLICT,
            "prepare the governed workspace before searching source files",
        )
    })?;
    let mut child = background_command("git")
        .args(["grep", "-n", "-I", "-F", "--", needle])
        .current_dir(&environment.worktree)
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
    let mut truncated = false;
    for line in std::io::BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
    {
        if hits.len() >= 500 {
            truncated = true;
            break;
        }
        let mut parts = line.splitn(3, ':');
        let Some(path) = parts.next() else { continue };
        let Some(line) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let preview = parts.next().unwrap_or_default().trim().to_owned();
        hits.push(SourceSearchHit {
            path: path.replace('\\', "/"),
            line,
            preview,
        });
    }
    if truncated {
        let _ = child.kill();
    }
    let _ = child.wait();
    Ok(Json(SourceSearchResponse {
        work_id: id.as_str().to_owned(),
        hits,
        truncated,
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

async fn save_workspace_state(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(mut body): Json<SaveWorkspaceStateRequest>,
) -> ApiResult<Json<CodeWorkspaceState>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    let environment = item
        .environment
        .as_ref()
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
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
    if body.state.tabs.iter().any(|tab| tab.draft.is_some()) {
        let lease_id = body.lease_id.as_deref().ok_or_else(|| {
            request_error(
                StatusCode::CONFLICT,
                "an active lease is required to persist source drafts",
            )
        })?;
        let generation = body
            .generation
            .ok_or_else(|| request_error(StatusCode::CONFLICT, "lease generation is required"))?;
        let lease = resolve_lease(forge(&state).as_ref(), lease_id, generation)?;
        if lease.work_id != id {
            return Err(request_error(
                StatusCode::CONFLICT,
                "the presented lease belongs to a different undertaking",
            ));
        }
    }
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

async fn save_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SaveSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    let id = parse_work_id(&work_id)?;
    if body.content.len() > MAX_SOURCE_BYTES {
        return Err(request_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("source file exceeds the {MAX_SOURCE_BYTES} byte editor limit"),
        ));
    }
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let environment = item
        .environment
        .as_ref()
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
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
    publish_item(&state, &item, "source_saved");
    Ok(Json(read_source_response(
        &id,
        &environment.worktree,
        &body.path,
    )?))
}

async fn save_source_batch(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SaveSourceBatchRequest>,
) -> ApiResult<Json<Vec<SourceResponse>>> {
    let id = parse_work_id(&work_id)?;
    if body.files.is_empty() {
        return Err(request_error(
            StatusCode::BAD_REQUEST,
            "no source edits supplied",
        ));
    }
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let environment = item
        .environment
        .as_ref()
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
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
    let responses = prepared
        .iter()
        .map(|(_, relative, _, _)| read_source_response(&id, &environment.worktree, relative))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(responses))
}

async fn rename_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RenameSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    let id = parse_work_id(&work_id)?;
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let environment = item
        .environment
        .as_ref()
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
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
    let (destination, _) = resolve_new_source_path(&environment.worktree, &body.destination)?;
    std::fs::rename(&source, &destination).map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not rename source file: {err}"),
        )
    })?;
    publish_item(&state, &item, "source_renamed");
    Ok(Json(read_source_response(
        &id,
        &environment.worktree,
        &body.destination,
    )?))
}

async fn delete_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<DeleteSourceRequest>,
) -> ApiResult<Json<DeleteSourceResponse>> {
    let id = parse_work_id(&work_id)?;
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let environment = item
        .environment
        .as_ref()
        .ok_or_else(|| request_error(StatusCode::CONFLICT, "governed workspace is not prepared"))?;
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
    publish_item(&state, &item, "source_deleted");
    Ok(Json(DeleteSourceResponse {
        work_id: id.as_str().to_owned(),
        path: relative,
        deleted: true,
    }))
}

async fn get_review(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ReviewProjection>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    let mut review = build_review(forge(&state).as_ref(), &item);
    if let Some(host) = state.detamu.as_ref() {
        review.world = Some(host.binding_status_json(item.id.as_str()).await);
    }
    Ok(Json(review))
}

#[derive(Debug, Deserialize)]
struct ReviewFileQuery {
    path: String,
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

fn review_file_diff(state: &AppState, id: &WorkId, raw_path: &str) -> ApiResult<ReviewFileDiff> {
    const MAX_REVIEW_FILE_BYTES: usize = 1024 * 1024;
    let (_, path) = normalize_source_relative(raw_path)?;
    let forge = forge(state);
    let item = forge.load(id).map_err(map_err)?;
    let review = build_review(forge.as_ref(), &item);
    let changed = review
        .changed_files
        .iter()
        .find(|file| file.path == path)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "file is not part of this review"))?;
    let environment = item.environment.as_ref().ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
    })?;
    let baseline_oid = GitOid::new(
        review
            .baseline_oid
            .clone()
            .ok_or_else(|| request_error(StatusCode::CONFLICT, "review has no starting revision"))?,
    );
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
        || baseline_bytes.as_ref().is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
        || reviewed_bytes.as_ref().is_some_and(|bytes| std::str::from_utf8(bytes).is_err());
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
    Ok(Json(review_file_diff(&state, &id, &query.path)?))
}

#[derive(Debug, Deserialize)]
struct RestoreReviewFileRequest {
    path: String,
    expected_reviewed_oid: String,
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
    let id = parse_work_id(&work_id)?;
    let comparison = review_file_diff(&state, &id, &body.path)?;
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
    forge
        .reopen_for_changes(&id, "A reviewed file was restored for another pass", &actor)
        .map_err(map_err)?;
    let (mut item, lease) = forge
        .begin_attempt(
            &id,
            ExecutorDescriptor {
                kind: "human".into(),
                detail: serde_json::json!({"reason": "restore_review_file"}),
            },
            None,
            &actor,
        )
        .map_err(map_err)?;
    let environment = item.environment.as_ref().ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
    })?;
    let restored_path = comparison
        .old_path
        .as_deref()
        .unwrap_or(&comparison.path)
        .to_owned();
    if comparison.path != restored_path {
        let (renamed, _) = resolve_source_path(&environment.worktree, &comparison.path)?;
        std::fs::remove_file(renamed).map_err(|err| {
            request_error(StatusCode::INTERNAL_SERVER_ERROR, format!("could not restore file: {err}"))
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
                request_error(StatusCode::INTERNAL_SERVER_ERROR, format!("could not restore folder: {err}"))
            })?;
        }
        std::fs::write(destination, content.as_bytes()).map_err(|err| {
            request_error(StatusCode::INTERNAL_SERVER_ERROR, format!("could not restore file: {err}"))
        })?;
        "restored"
    } else {
        let (destination, _) = resolve_source_path(&environment.worktree, &comparison.path)?;
        std::fs::remove_file(destination).map_err(|err| {
            request_error(StatusCode::INTERNAL_SERVER_ERROR, format!("could not restore file: {err}"))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectTask {
    id: String,
    label: String,
    kind: String,
    argv: Vec<String>,
    provider: String,
    #[serde(default)]
    long_running: bool,
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
}

static PROJECT_TASK_RUNS: LazyLock<tokio::sync::RwLock<std::collections::HashMap<String, ProjectTaskRun>>> =
    LazyLock::new(|| tokio::sync::RwLock::new(std::collections::HashMap::new()));
static PROJECT_TASK_CHILDREN: LazyLock<
    tokio::sync::RwLock<
        std::collections::HashMap<String, Arc<tokio::sync::Mutex<tokio::process::Child>>>,
    >,
> = LazyLock::new(|| tokio::sync::RwLock::new(std::collections::HashMap::new()));

#[derive(Debug, Deserialize)]
struct RunProjectTaskRequest {
    lease_id: String,
    generation: u64,
    #[serde(default)]
    test_id: Option<String>,
}

fn target_project_test(root: &FsPath, task: &mut ProjectTask, test_id: Option<&str>) -> ApiResult<()> {
    let Some(test_id) = test_id else { return Ok(()) };
    let test = discover_project_tests(root, std::slice::from_ref(task))
        .into_iter().find(|test| test.id == test_id && test.task_id == task.id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Test is no longer available"))?;
    if task.id == "cargo-test" {
        task.argv.push(test.label.clone());
    } else if task.id == "python-test" {
        task.argv.push(format!("{}::{}", test.path, test.label));
    } else if task.id == "npm-test" {
        task.argv.extend(["--".into(), test.path]);
    } else if task.id == "go-test" {
        let package = std::path::Path::new(&test.path).parent().map(|path| format!("./{}", path.display())).unwrap_or_else(|| ".".into());
        task.argv.extend([package, "-run".into(), format!("^{}$", test.label)]);
    }
    task.label = format!("Test {}", test.label);
    Ok(())
}

fn detected_project_tasks(root: &FsPath) -> Vec<ProjectTask> {
    let mut tasks = Vec::new();
    let mut add = |id: &str, label: &str, kind: &str, argv: &[&str]| {
        tasks.push(ProjectTask {
            id: id.into(),
            label: label.into(),
            kind: kind.into(),
            argv: argv.iter().map(|part| (*part).to_string()).collect(),
            provider: argv.first().copied().unwrap_or("project").into(),
            long_running: kind == "run",
        });
    };
    if root.join("Cargo.toml").is_file() {
        add("cargo-check", "Check", "verify", &["cargo", "check"]);
        add("cargo-test", "Test", "test", &["cargo", "test"]);
    }
    if root.join("go.mod").is_file() {
        add("go-test", "Test", "verify", &["go", "test", "./..."]);
    }
    if root.join("package.json").is_file() {
        let scripts = std::fs::read(root.join("package.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| {
                value
                    .get("scripts")
                    .and_then(|scripts| scripts.as_object())
                    .cloned()
            })
            .unwrap_or_default();
        if scripts.contains_key("check") {
            add("npm-check", "Check", "verify", &["npm", "run", "check"]);
        }
        if scripts.contains_key("test") {
            add("npm-test", "Test", "test", &["npm", "test"]);
        }
        if scripts.contains_key("build") {
            add("npm-build", "Build", "build", &["npm", "run", "build"]);
        }
        if scripts.contains_key("dev") {
            add("npm-dev", "Development server", "run", &["npm", "run", "dev"]);
        } else if scripts.contains_key("start") {
            add("npm-start", "Run project", "run", &["npm", "start"]);
        }
    }
    if root.join("pyproject.toml").is_file() || root.join("pytest.ini").is_file() {
        add("python-test", "Test", "verify", &["python", "-m", "pytest"]);
    }
    if root.join("Makefile").is_file() {
        let makefile = std::fs::read_to_string(root.join("Makefile")).unwrap_or_default();
        for (target, label, kind) in [
            ("check", "Check", "verify"),
            ("test", "Test", "test"),
            ("build", "Build", "build"),
        ] {
            if makefile
                .lines()
                .any(|line| line.starts_with(&format!("{target}:")))
            {
                add(&format!("make-{target}"), label, kind, &["make", target]);
            }
        }
    }
    if std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            matches!(
                entry.path().extension().and_then(|value| value.to_str()),
                Some("sln" | "csproj")
            )
        })
    {
        add("dotnet-test", "Test", "verify", &["dotnet", "test"]);
    }
    tasks
}

fn parse_output_locations(root: &FsPath, output: &str) -> Vec<ProjectOutputLocation> {
    let mut locations = Vec::new();
    for line_text in output.lines() {
        let Some(token) = line_text
            .split_whitespace()
            .map(|token| token.trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | ',')))
            .find(|token| {
                let mut parts = token.rsplitn(3, ':');
                parts.next().is_some_and(|part| part.parse::<u32>().is_ok())
                    && parts.next().is_some_and(|part| part.parse::<u32>().is_ok() || part.contains('.'))
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
            let Some((path, line)) = token.rsplit_once(':') else { continue };
            (path, line.parse().unwrap_or(1), None)
        };
        let raw = raw.trim_start_matches("-->");
        let path = std::path::Path::new(raw);
        let relative = if path.is_absolute() {
            path.strip_prefix(root).ok()
        } else {
            Some(path)
        };
        let Some(relative) = relative else { continue };
        if relative.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
            continue;
        }
        let message = line_text.trim().chars().take(300).collect();
        locations.push(ProjectOutputLocation {
            path: relative.to_string_lossy().replace('\\', "/"),
            line,
            column,
            message,
        });
        if locations.len() >= 100 { break; }
    }
    locations
}

async fn list_project_tasks(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<Vec<ProjectTask>>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    let root = item
        .environment
        .as_ref()
        .ok_or_else(|| {
            request_error(
                StatusCode::CONFLICT,
                "Set up this project before running it",
            )
        })?
        .worktree
        .clone();
    Ok(Json(detected_project_tasks(&root)))
}

fn discover_project_tests(root: &FsPath, tasks: &[ProjectTask]) -> Vec<ProjectTest> {
    let Some(task) = tasks.iter().find(|task| task.kind == "test" || task.id.ends_with("-test")) else {
        return Vec::new();
    };
    let mut tests = Vec::new();
    let tree = list_source_tree(&WorkId::from("test-discovery".to_string()), root).ok();
    for file in tree.into_iter().flat_map(|tree| tree.files).take(20_000) {
        let path = root.join(&file.path);
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("").to_string();
        if !matches!(extension.as_str(), "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        let mut previous_test_attribute = false;
        for (index, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let name = if extension == "rs" && previous_test_attribute {
                trimmed.strip_prefix("fn ").and_then(|rest| rest.split(|ch: char| !ch.is_alphanumeric() && ch != '_').next())
            } else if extension == "py" {
                trimmed.strip_prefix("def test_").and_then(|rest| rest.split('(').next()).map(|name| &trimmed[4..4 + 5 + name.len()])
            } else if matches!(extension.as_str(), "js" | "jsx" | "ts" | "tsx") && (trimmed.starts_with("test(") || trimmed.starts_with("it(")) {
                trimmed.split(['\'', '"']).nth(1)
            } else if extension == "go" {
                trimmed.strip_prefix("func Test").and_then(|rest| rest.split('(').next()).map(|name| &trimmed[5..5 + 4 + name.len()])
            } else {
                None
            };
            if let Some(name) = name.filter(|name| !name.is_empty()) {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                tests.push(ProjectTest {
                    id: format!("{}::{name}", relative),
                    label: name.to_string(),
                    path: relative,
                    line: (index + 1) as u32,
                    task_id: task.id.clone(),
                });
                if tests.len() >= 2_000 { return tests; }
            }
            previous_test_attribute = extension == "rs" && trimmed == "#[test]";
        }
    }
    tests
}

async fn list_project_tests(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<Vec<ProjectTest>>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    let root = item.environment.as_ref().ok_or_else(|| request_error(StatusCode::CONFLICT, "Set up this project before finding tests"))?.worktree.clone();
    let tasks = detected_project_tasks(&root);
    Ok(Json(discover_project_tests(&root, &tasks)))
}

async fn start_project_task_run(
    State(state): State<AppState>,
    Path((work_id, task_id)): Path<(String, String)>,
    Json(body): Json<RunProjectTaskRequest>,
) -> ApiResult<Json<ProjectTaskRun>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let lease = resolve_lease(forge.as_ref(), &body.lease_id, body.generation)?;
    let root = item.environment.as_ref().ok_or_else(|| request_error(StatusCode::CONFLICT, "Set up this project before running it"))?.worktree.clone();
    let mut task = detected_project_tasks(&root).into_iter().find(|task| task.id == task_id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project command is no longer available"))?;
    target_project_test(&root, &mut task, body.test_id.as_deref())?;
    let run_id = format!("run-{}", uuid::Uuid::new_v4());
    let run = ProjectTaskRun { run_id: run_id.clone(), work_id: work_id.clone(), state: "running".into(), task: task.clone(), result: None };
    let mut child = background_tokio_command(&task.argv[0])
        .args(&task.argv[1..]).current_dir(&root)
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .kill_on_drop(true).spawn()
        .map_err(|err| request_error(StatusCode::BAD_REQUEST, format!("Could not run {}: {err}", task.label)))?;
    PROJECT_TASK_RUNS.write().await.insert(run_id.clone(), run.clone());
    let mut stdout_stream = child.stdout.take();
    let mut stderr_stream = child.stderr.take();
    let stdout_reader = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        if let Some(ref mut stream) = stdout_stream { let _ = stream.read_to_end(&mut bytes).await; }
        bytes
    });
    let stderr_reader = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        if let Some(ref mut stream) = stderr_stream { let _ = stream.read_to_end(&mut bytes).await; }
        bytes
    });
    let child = Arc::new(tokio::sync::Mutex::new(child));
    PROJECT_TASK_CHILDREN.write().await.insert(run_id.clone(), Arc::clone(&child));
    let state_for_run = state.clone();
    let run_id_for_task = run_id.clone();
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        loop {
            let status = { child.lock().await.try_wait().ok().flatten() };
            if let Some(status) = status {
                let stdout_bytes = stdout_reader.await.unwrap_or_default();
                let stderr_bytes = stderr_reader.await.unwrap_or_default();
                const CAP: usize = 64 * 1024;
                let truncated = stdout_bytes.len() > CAP || stderr_bytes.len() > CAP;
                let stdout = String::from_utf8_lossy(&stdout_bytes[..stdout_bytes.len().min(CAP)]).into_owned();
                let stderr = String::from_utf8_lossy(&stderr_bytes[..stderr_bytes.len().min(CAP)]).into_owned();
                let locations = parse_output_locations(&root, &format!("{stdout}\n{stderr}"));
                let result = ProjectTaskResult { task: task.clone(), success: status.success(), exit_code: status.code(), stdout, stderr, truncated, duration_ms: started.elapsed().as_millis(), locations };
                let cancelled = PROJECT_TASK_RUNS.read().await.get(&run_id_for_task).is_some_and(|run| run.state == "cancelled");
                let _ = forge.append_command_log(&lease, &serde_json::json!({"kind":if cancelled {"project_task_cancelled"} else {"project_task"},"run_id":run_id_for_task,"task":result.task,"success":result.success,"exit_code":result.exit_code,"duration_ms":result.duration_ms,"stdout":result.stdout,"stderr":result.stderr,"truncated":result.truncated,"locations":result.locations}));
                if let Some(stored) = PROJECT_TASK_RUNS.write().await.get_mut(&run_id_for_task) {
                    if !cancelled {
                        stored.state = if result.success { "passed" } else { "failed" }.into();
                    }
                    stored.result = Some(result);
                }
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
    let run = PROJECT_TASK_RUNS.read().await.get(&run_id).cloned()
        .filter(|run| run.work_id == work_id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run was not found"))?;
    Ok(Json(run))
}

async fn cancel_project_task_run(
    Path((work_id, run_id)): Path<(String, String)>,
) -> ApiResult<Json<ProjectTaskRun>> {
    if PROJECT_TASK_RUNS.read().await.get(&run_id).is_none_or(|run| run.work_id != work_id) {
        return Err(request_error(StatusCode::NOT_FOUND, "Project run was not found"));
    }
    let child = PROJECT_TASK_CHILDREN.read().await.get(&run_id).cloned()
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run is no longer active"))?;
    child.lock().await.start_kill().map_err(|err| request_error(StatusCode::INTERNAL_SERVER_ERROR, format!("Could not stop project run: {err}")))?;
    let mut runs = PROJECT_TASK_RUNS.write().await;
    let run = runs.get_mut(&run_id).filter(|run| run.work_id == work_id)
        .ok_or_else(|| request_error(StatusCode::NOT_FOUND, "Project run was not found"))?;
    run.state = "cancelled".into();
    Ok(Json(run.clone()))
}

async fn run_project_task(
    State(state): State<AppState>,
    Path((work_id, task_id)): Path<(String, String)>,
    Json(body): Json<RunProjectTaskRequest>,
) -> ApiResult<Json<ProjectTaskResult>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let lease = resolve_lease(forge.as_ref(), &body.lease_id, body.generation)?;
    let root = item
        .environment
        .as_ref()
        .ok_or_else(|| {
            request_error(
                StatusCode::CONFLICT,
                "Set up this project before running it",
            )
        })?
        .worktree
        .clone();
    let task = detected_project_tasks(&root)
        .into_iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| {
            request_error(
                StatusCode::NOT_FOUND,
                "Project command is no longer available",
            )
        })?;
    let argv = task.argv.clone();
    let root_for_command = root.clone();
    let started = std::time::Instant::now();
    let output = tokio::task::spawn_blocking(move || {
        background_command(&argv[0])
            .args(&argv[1..])
            .current_dir(root_for_command)
            .output()
    })
    .await
    .map_err(|err| {
        request_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Project command stopped unexpectedly: {err}"),
        )
    })?
    .map_err(|err| {
        request_error(
            StatusCode::BAD_REQUEST,
            format!("Could not run {}: {err}", task.label),
        )
    })?;
    const OUTPUT_CAP: usize = 64 * 1024;
    let truncated = output.stdout.len() > OUTPUT_CAP || output.stderr.len() > OUTPUT_CAP;
    let stdout =
        String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(OUTPUT_CAP)]).into_owned();
    let stderr =
        String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(OUTPUT_CAP)]).into_owned();
    let locations = parse_output_locations(&root, &format!("{stdout}\n{stderr}"));
    let result = ProjectTaskResult {
        task: task.clone(),
        success: output.status.success(),
        exit_code: output.status.code(),
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
    command.creation_flags(0x0800_0000);
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
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        if let Some(repository) = prefixes.iter().find_map(|prefix| remote.strip_prefix(prefix)) {
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
                && segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        })
}

fn provider_handoff(forge: &Forge, item: &WorkItem) -> ProviderHandoff {
    let context = load_provider_context(forge, &item.id);
    let remote_url = item.environment.as_ref().and_then(|env| repository_remote(&env.worktree));
    let parsed = remote_url.as_deref().and_then(provider_repository);
    let provider = parsed.as_ref().map(|(provider, _)| *provider).unwrap_or("none");
    let available = match provider {
        "github" => command_available("gh"),
        "gitlab" => command_available("glab"),
        _ => false,
    };
    let branch = item.environment.as_ref().map(|env| env.branch.clone());
    let WorkTarget::Git(target) = &item.target;
    let base_branch = Some(target.base_ref.clone());
    let shared = item.environment.as_ref().is_some_and(|env| {
        background_command("git")
            .args(["show-ref", "--verify", &format!("refs/remotes/origin/{}", env.branch)])
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
            (_, false) => format!("Install and sign in to the {provider} CLI on the workshop machine."),
            _ => "Ready to share from the connected workshop.".into(),
        },
    }
}

async fn get_provider_handoff(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ProviderHandoff>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let item = forge.load(&id).map_err(map_err)?;
    Ok(Json(provider_handoff(forge.as_ref(), &item)))
}

async fn save_provider_context(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<SaveProviderContextRequest>,
) -> ApiResult<Json<ProviderHandoff>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let item = forge.load(&id).map_err(map_err)?;
    if body.links.len() > 20 {
        return Err(request_error(StatusCode::BAD_REQUEST, "Too many linked items"));
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

fn provider_command_error(label: &str, output: &std::process::Output) -> ApiError {
    let detail = String::from_utf8_lossy(&output.stderr).trim().chars().take(500).collect::<String>();
    request_error(StatusCode::BAD_GATEWAY, format!("{label} failed{}", if detail.is_empty() { String::new() } else { format!(": {detail}") }))
}

fn provider_review_body(forge: &Forge, item: &WorkItem, requested: Option<&str>) -> String {
    let review = build_review(forge, item);
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
                if verification.success { "passed" } else { "failed" },
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
            .args(["pr", "view", branch, "--repo", repository, "--json", "url", "--jq", ".url"])
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
            .and_then(|value| value.get("web_url").and_then(|url| url.as_str()).map(str::to_string))
    };
    Ok(url)
}

async fn share_provider_handoff(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ShareProviderRequest>,
) -> ApiResult<Json<ProviderHandoff>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let item = forge.load(&id).map_err(map_err)?;
    if !matches!(item.state, WorkState::AwaitingReview | WorkState::Accepted) {
        return Err(request_error(StatusCode::CONFLICT, "Finish and review the project before sharing it"));
    }
    let handoff = provider_handoff(forge.as_ref(), &item);
    if !handoff.available {
        return Err(request_error(StatusCode::SERVICE_UNAVAILABLE, handoff.message));
    }
    let environment = item.environment.as_ref().ok_or_else(|| request_error(StatusCode::CONFLICT, "Project workspace is unavailable"))?;
    let push = background_command("git")
        .args(["push", "--set-upstream", "origin", &environment.branch])
        .current_dir(&environment.worktree)
        .output()
        .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
    if !push.status.success() {
        return Err(provider_command_error("Sharing the branch", &push));
    }
    let repository = handoff.repository.as_deref().ok_or_else(|| request_error(StatusCode::BAD_REQUEST, "Repository identity is unavailable"))?;
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
    let description = provider_review_body(forge.as_ref(), &item, body.body.as_deref());
    let output = if handoff.provider == "github" {
        background_command("gh")
            .args(["pr", "create", "--repo", repository, "--head", &environment.branch, "--base", base, "--title", &title, "--body", &description])
            .current_dir(&environment.worktree).output()
    } else {
        background_command("glab")
            .args(["mr", "create", "--source-branch", &environment.branch, "--target-branch", base, "--title", &title, "--description", &description, "--yes"])
            .current_dir(&environment.worktree).output()
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
            background_command("gh").args(["pr", "edit", &environment.branch, "--repo", repository, "--title", &title, "--body", &description]).current_dir(&environment.worktree).output()
        } else {
            background_command("glab").args(["mr", "update", &environment.branch, "--title", &title, "--description", &description]).current_dir(&environment.worktree).output()
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

async fn list_provider_comments(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<Vec<ProviderComment>>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let item = forge.load(&id).map_err(map_err)?;
    let handoff = provider_handoff(forge.as_ref(), &item);
    if handoff.provider != "github" || handoff.review_url.is_none() {
        return Ok(Json(Vec::new()));
    }
    let repository = handoff.repository.ok_or_else(|| request_error(StatusCode::BAD_REQUEST, "Repository identity is unavailable"))?;
    let branch = handoff.branch.ok_or_else(|| request_error(StatusCode::BAD_REQUEST, "Project branch is unavailable"))?;
    let output = background_command("gh").args(["pr", "view", &branch, "--repo", &repository, "--json", "comments,reviews"]).output()
        .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
    if !output.status.success() { return Err(provider_command_error("Reading review comments", &output)); }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| request_error(StatusCode::BAD_GATEWAY, err.to_string()))?;
    let mut comments = Vec::new();
    for (kind, entries) in [("comment", value.get("comments")), ("review", value.get("reviews"))] {
        for (index, entry) in entries.and_then(|entries| entries.as_array()).into_iter().flatten().enumerate() {
            let body = entry.get("body").and_then(|body| body.as_str()).unwrap_or("").trim();
            if body.is_empty() { continue; }
            comments.push(ProviderComment {
                id: format!("{kind}-{index}"),
                author: entry.pointer("/author/login").and_then(|author| author.as_str()).unwrap_or("Reviewer").into(),
                body: body.chars().take(8_000).collect(),
                url: entry.get("url").and_then(|url| url.as_str()).map(str::to_string),
            });
        }
    }
    Ok(Json(comments))
}

async fn import_provider_comment(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ImportProviderCommentRequest>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    let forge = forge(&state);
    let source = forge.load(&id).map_err(map_err)?;
    let WorkTarget::Git(target) = &source.target;
    let body_text = body.body.trim();
    if body.id.trim().is_empty() || body_text.is_empty() || body_text.len() > 8_000 {
        return Err(request_error(StatusCode::BAD_REQUEST, "Review comment is invalid"));
    }
    let brief = format!(
        "Follow up on review feedback for {}:\n\n{}{}",
        source.title,
        body_text,
        body.url.as_deref().map(|url| format!("\n\nSource: {url}")).unwrap_or_default()
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

async fn provision_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    let actor = actor_from_state(&state);
    let item = forge(&state).provision(&id, &actor).map_err(map_err)?;
    if let Some(env) = item.environment.as_ref() {
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
}

async fn begin_attempt(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<BeginAttemptRequest>,
) -> ApiResult<Json<BeginAttemptResponse>> {
    let id = parse_work_id(&work_id)?;
    let actor = actor_from_state(&state);
    let executor = body.executor.unwrap_or(ExecutorDescriptor {
        kind: "human".into(),
        detail: serde_json::json!({}),
    });
    let (item, lease) = forge(&state)
        .begin_attempt(&id, executor, body.pid, &actor)
        .map_err(map_err)?;
    publish_item(&state, &item, "attempt_begun");
    Ok(Json(BeginAttemptResponse {
        item: project_item(item),
        lease,
    }))
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
        .active_attempt
        .as_ref()
        .and_then(|attempt_id| item.attempt(attempt_id))
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
    let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
    forge(&state).heartbeat(&lease).map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn complete_lease(
    State(state): State<AppState>,
    Path(lease_id): Path<String>,
    Json(body): Json<CompleteLeaseRequest>,
) -> ApiResult<Json<ItemProjection>> {
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
    if let Some(env) = item.environment.as_ref() {
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
    let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
    let actor = actor_from_state(&state);
    let recovery = body.recovery.unwrap_or(RecoveryDisposition::RestartAllowed);
    let item = forge(&state)
        .interrupt_attempt(&lease, recovery, &actor)
        .map_err(map_err)?;
    Ok(ok_item(&state, item, "interrupted"))
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
    let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
    let actor = actor_from_state(&state);
    let message = body.error.unwrap_or_else(|| "attempt failed".into());
    let item = forge(&state)
        .fail_attempt(&lease, &message, &actor)
        .map_err(map_err)?;
    Ok(ok_item(&state, item, "failed"))
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
    let env = item.environment.as_ref().ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            Json(ErrorBody {
                error: "no governed environment".into(),
                kind: Some("conflict"),
            }),
        )
    })?;
    let review = build_review(forge(&state).as_ref(), &item);
    let digest = review.evidence_digest.clone().unwrap_or_default();
    if !body.evidence_digest.is_empty() && digest != body.evidence_digest {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorBody {
                error: "evidence_digest mismatch".into(),
                kind: Some("conflict"),
            }),
        ));
    }
    let sealed_head = review
        .sealed_head_oid
        .clone()
        .unwrap_or_else(|| env.baseline_oid.as_str().to_owned());
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
    let decision = ReviewDecision {
        id: ReviewDecisionId::new(),
        actor: actor.clone(),
        attempt_id: attempt.id.clone(),
        environment_generation: env.generation,
        evidence_id,
        evidence_digest,
        baseline_oid: env.baseline_oid.clone(),
        reviewed_head_oid: medousa_forge::model::GitOid::new(sealed_head),
        expected_base_oid: env.baseline_oid.clone(),
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
    let actor = actor_from_state(&state);
    let item = forge(&state)
        .apply_decision(&id, &decision_id, &actor)
        .map_err(map_err)?;
    Ok(ok_item(&state, item, "applied"))
}

async fn discard_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<ItemProjection>> {
    let id = parse_work_id(&work_id)?;
    let actor = actor_from_state(&state);
    let item = forge(&state).discard(&id, &actor).map_err(map_err)?;
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
    let item = tokio::task::spawn_blocking(move || {
        ScriptAdapter::new(forge.as_ref()).run_script(&id, &argv)
    })
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: format!("run-script join failed: {err}"),
                kind: Some("store"),
            }),
        )
    })?
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

async fn evidence_patch(
    State(state): State<AppState>,
    Path(evidence_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<EvidencePageQuery>,
) -> ApiResult<Json<EvidencePage>> {
    let eid = EvidenceId::from(evidence_id);
    let (_item, dir) = find_evidence_dir(forge(&state).as_ref(), &eid, q.work_id.as_deref())?;
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    let (lines, total, truncated) = read_lines_page(&dir.join("patch.diff"), offset, limit)
        .map_err(|e| {
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

async fn evidence_commands(
    State(state): State<AppState>,
    Path(evidence_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<EvidencePageQuery>,
) -> ApiResult<Json<EvidencePage>> {
    let eid = EvidenceId::from(evidence_id);
    let (_item, dir) = find_evidence_dir(forge(&state).as_ref(), &eid, q.work_id.as_deref())?;
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    let (lines, total, truncated) = read_lines_page(&dir.join("commands.jsonl"), offset, limit)
        .map_err(|e| {
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

#[cfg(test)]
mod source_tests {
    use super::*;

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
    fn new_source_paths_require_a_safe_existing_parent() {
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

        assert!(resolve_new_source_path(root.path(), "missing/new.rs").is_err());
        assert!(resolve_new_source_path(root.path(), ".git/hooks/new-hook").is_err());
        std::fs::write(root.path().join("src/existing.rs"), "fn existing() {}\n").unwrap();
        assert!(resolve_new_source_path(root.path(), "src/existing.rs").is_err());
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

        let split = CodeWorkspaceState {
            tabs: legacy.tabs,
            active_path: legacy.active_path,
            secondary_path: Some("src/main.rs".into()),
            updated_at: None,
        };
        let encoded = serde_json::to_value(split).unwrap();
        assert_eq!(encoded["secondary_path"], "src/main.rs");
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
        assert!(tasks.iter().any(|task| task.id == "npm-dev" && task.long_running));
        assert!(!tasks.iter().any(|task| task.id == "custom"));
    }

    #[test]
    fn project_output_locations_stay_repository_relative() {
        let root = PathBuf::from("/work/project");
        let locations = parse_output_locations(
            &root,
            "error --> src/lib.rs:42:7\nat /work/project/tests/app.test.ts:9:2",
        );
        assert_eq!(locations[0].path, "src/lib.rs");
        assert_eq!(locations[0].line, 42);
        assert_eq!(locations[1].path, "tests/app.test.ts");
    }

    #[test]
    fn discovers_rust_tests_without_executing_project_code() {
        let root = tempfile::tempdir().unwrap();
        assert!(Command::new("git").args(["init", "-q"]).current_dir(root.path()).status().unwrap().success());
        std::fs::write(root.path().join("Cargo.toml"), "[package]\nname='demo'\nversion='0.1.0'\n").unwrap();
        std::fs::write(root.path().join("lib.rs"), "#[test]\nfn intent_stays_clear() {}\n").unwrap();
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
        assert!(browse_path_allowed(FsPath::new("/workspaces/team/repo"), &roots));
        assert!(browse_path_allowed(FsPath::new("/srv/code/project"), &roots));
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
}
