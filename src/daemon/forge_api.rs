//! HTTP control plane for Forge undertakings (`/v1/forge/...`).
//!
//! Distinct from `/v1/workspace/cards` (activity board) and vault Versions
//! (material memory). Forge owns custody of intentional work episodes.

use std::path::{Component, Path as FsPath, PathBuf};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_forge::adapter::{export_bundle, ScriptAdapter};
use medousa_forge::error::ForgeError;
use medousa_forge::forge::{Forge, SealOptions};
use medousa_forge::git::CheckpointAuthor;
use medousa_forge::model::{
    ActorKind, ActorRef, EvidenceId, ExecutionLease, ExecutorDescriptor, IntegrationStrategy,
    LeaseId, RecoveryDisposition, ReviewDecision, ReviewDecisionId, WorkId, WorkItem, WorkPolicy,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::daemon::forge_projections::{
    build_review, evidence_dir, project_item, project_items, read_lines_page, ItemProjection,
    ReviewProjection,
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
        .route("/v1/forge/items/{work_id}/search", get(search_source))
        .route(
            "/v1/forge/items/{work_id}/workspace-state",
            get(read_workspace_state).put(save_workspace_state),
        )
        .route("/v1/forge/items/{work_id}/review", get(get_review))
        .route(
            "/v1/forge/items/{work_id}/provision",
            post(provision_item),
        )
        .route(
            "/v1/forge/items/{work_id}/attempts",
            post(begin_attempt),
        )
        .route(
            "/v1/forge/items/{work_id}/decisions",
            post(record_decision),
        )
        .route("/v1/forge/items/{work_id}/apply", post(apply_decision))
        .route("/v1/forge/items/{work_id}/discard", post(discard_item))
        .route(
            "/v1/forge/items/{work_id}/run-script",
            post(run_script),
        )
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
        .route(
            "/v1/forge/leases/{lease_id}/complete",
            post(complete_lease),
        )
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
    let (relative, clean) = normalize_source_relative(raw)?;
    let root = std::fs::canonicalize(root).map_err(|err| {
        request_error(
            StatusCode::CONFLICT,
            format!("governed workspace is unavailable: {err}"),
        )
    })?;
    let candidate = std::fs::canonicalize(root.join(&relative)).map_err(|err| {
        request_error(
            StatusCode::NOT_FOUND,
            format!("source file not found: {err}"),
        )
    })?;
    if !candidate.starts_with(&root) || !candidate.is_file() {
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
    let file_name = relative.file_name().ok_or_else(|| {
        request_error(StatusCode::BAD_REQUEST, "source file name is required")
    })?;
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
fn resolve_lease(
    forge: &Forge,
    lease_id: &str,
    generation: u64,
) -> ApiResult<ExecutionLease> {
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

fn default_base_ref() -> String {
    "main".into()
}

async fn register_item(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Json<ItemProjection>> {
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
    Ok(ok_item(&state, item, "registered"))
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
    let output = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
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
    for raw in output.stdout.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
        if files.len() >= MAX_SOURCE_TREE_FILES {
            truncated = true;
            break;
        }
        let relative = String::from_utf8_lossy(raw).replace('\\', "/");
        let Ok((path, clean)) = resolve_source_path(&root, &relative) else {
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
    let Ok(output) = std::process::Command::new("git")
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
    let environment = item.environment.as_ref().ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
    })?;
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
    Ok(Json(list_source_tree(&id, &environment.worktree)?))
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
    let mut child = std::process::Command::new("git")
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
        request_error(StatusCode::INTERNAL_SERVER_ERROR, "repository search had no output")
    })?;
    let mut hits = Vec::new();
    let mut truncated = false;
    for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
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
    let environment = item.environment.as_ref().ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
    })?;
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
    }
    for tab in &mut body.state.tabs {
        let (_, clean) = resolve_source_path(&environment.worktree, &tab.path)?;
        tab.path = clean;
        if tab.draft.as_ref().is_some_and(|draft| draft.len() > MAX_SOURCE_BYTES) {
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

async fn rename_source(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RenameSourceRequest>,
) -> ApiResult<Json<SourceResponse>> {
    let id = parse_work_id(&work_id)?;
    let item = require_work_lease(&state, &id, &body.lease_id, body.generation)?;
    let environment = item.environment.as_ref().ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
    })?;
    let (source, _) = resolve_source_path(&environment.worktree, &body.path)?;
    let current = std::fs::read(&source).map_err(|err| {
        request_error(StatusCode::NOT_FOUND, format!("could not read source file: {err}"))
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
    let environment = item.environment.as_ref().ok_or_else(|| {
        request_error(StatusCode::CONFLICT, "governed workspace is not prepared")
    })?;
    let (path, relative) = resolve_source_path(&environment.worktree, &body.path)?;
    let current = std::fs::read(&path).map_err(|err| {
        request_error(StatusCode::NOT_FOUND, format!("could not read source file: {err}"))
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
    let recovery = body
        .recovery
        .unwrap_or(RecoveryDisposition::RestartAllowed);
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
    let evidence_digest: medousa_forge::model::Digest = serde_json::from_value(
        serde_json::Value::String(digest),
    )
    .map_err(|e| {
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
    let item = forge(&state)
        .discard(&id, &actor)
        .map_err(map_err)?;
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
    let (_item, dir) = find_evidence_dir(
        forge(&state).as_ref(),
        &eid,
        q.work_id.as_deref(),
    )?;
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

async fn evidence_commands(
    State(state): State<AppState>,
    Path(evidence_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<EvidencePageQuery>,
) -> ApiResult<Json<EvidencePage>> {
    let eid = EvidenceId::from(evidence_id);
    let (_item, dir) = find_evidence_dir(
        forge(&state).as_ref(),
        &eid,
        q.work_id.as_deref(),
    )?;
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
    fn source_paths_are_repo_relative_and_canonical() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "fn main() {}\n").unwrap();

        let (path, relative) = resolve_source_path(root.path(), "./src/lib.rs").unwrap();
        assert_eq!(relative, "src/lib.rs");
        assert_eq!(path, std::fs::canonicalize(root.path().join("src/lib.rs")).unwrap());
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
        assert_eq!(path, std::fs::canonicalize(root.path().join("src")).unwrap().join("new.rs"));

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
        let paths = tree.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>();
        assert!(paths.contains(&"tracked.rs"));
        assert!(paths.contains(&"new.rs"));
        assert!(!paths.contains(&"target/noise.rs"));
        assert_eq!(
            tree.files.iter().find(|file| file.path == "new.rs").and_then(|file| file.status.as_deref()),
            Some("??"),
        );
        assert!(!tree.truncated);
    }
}
