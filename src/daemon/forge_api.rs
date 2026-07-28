//! HTTP control plane for Forge undertakings (`/v1/forge/...`).
//!
//! Distinct from `/v1/workspace/cards` (activity board) and vault Versions
//! (material memory). Forge owns custody of intentional work episodes.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_forge::adapter::{export_bundle, ScriptAdapter};
use medousa_forge::error::ForgeError;
use medousa_forge::forge::{Forge, SealOptions};
use medousa_forge::git::CheckpointAuthor;
use medousa_forge::model::{
    ActorKind, ActorRef, ExecutionLease, ExecutorDescriptor, LeaseId, RecoveryDisposition,
    ReviewDecision, ReviewDecisionId, WorkId, WorkItem, WorkPolicy,
};
use serde::{Deserialize, Serialize};

use crate::daemon::state::AppState;

pub fn forge_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/forge/items", get(list_items).post(register_item))
        .route("/v1/forge/items/{work_id}", get(get_item))
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
) -> ApiResult<Json<WorkItem>> {
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
    Ok(Json(item))
}

async fn list_items(State(state): State<AppState>) -> ApiResult<Json<Vec<WorkItem>>> {
    let items = forge(&state).list().map_err(map_err)?;
    Ok(Json(items))
}

async fn get_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<WorkItem>> {
    let id = parse_work_id(&work_id)?;
    let item = forge(&state).load(&id).map_err(map_err)?;
    Ok(Json(item))
}

async fn provision_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<WorkItem>> {
    let id = parse_work_id(&work_id)?;
    let actor = actor_from_state(&state);
    let item = forge(&state).provision(&id, &actor).map_err(map_err)?;
    Ok(Json(item))
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
    item: WorkItem,
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
    Ok(Json(BeginAttemptResponse { item, lease }))
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
) -> ApiResult<Json<WorkItem>> {
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
    Ok(Json(item))
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
) -> ApiResult<Json<WorkItem>> {
    let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
    let actor = actor_from_state(&state);
    let recovery = body
        .recovery
        .unwrap_or(RecoveryDisposition::RestartAllowed);
    let item = forge(&state)
        .interrupt_attempt(&lease, recovery, &actor)
        .map_err(map_err)?;
    Ok(Json(item))
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
) -> ApiResult<Json<WorkItem>> {
    let lease = resolve_lease(forge(&state).as_ref(), &lease_id, body.generation)?;
    let actor = actor_from_state(&state);
    let message = body.error.unwrap_or_else(|| "attempt failed".into());
    let item = forge(&state)
        .fail_attempt(&lease, &message, &actor)
        .map_err(map_err)?;
    Ok(Json(item))
}

#[derive(Debug, Deserialize)]
struct DecideRequest {
    decision: ReviewDecision,
}

async fn record_decision(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<DecideRequest>,
) -> ApiResult<Json<WorkItem>> {
    let id = parse_work_id(&work_id)?;
    let actor = actor_from_state(&state);
    let item = forge(&state)
        .decide(&id, body.decision, &actor)
        .map_err(map_err)?;
    Ok(Json(item))
}

#[derive(Debug, Deserialize)]
struct ApplyRequest {
    decision_id: String,
}

async fn apply_decision(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<ApplyRequest>,
) -> ApiResult<Json<WorkItem>> {
    let id = parse_work_id(&work_id)?;
    let decision_id = ReviewDecisionId::from(body.decision_id);
    let actor = actor_from_state(&state);
    let item = forge(&state)
        .apply_decision(&id, &decision_id, &actor)
        .map_err(map_err)?;
    Ok(Json(item))
}

async fn discard_item(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
) -> ApiResult<Json<WorkItem>> {
    let id = parse_work_id(&work_id)?;
    let actor = actor_from_state(&state);
    let item = forge(&state).discard(&id, &actor).map_err(map_err)?;
    Ok(Json(item))
}

#[derive(Debug, Deserialize)]
struct RunScriptRequest {
    argv: Vec<String>,
}

async fn run_script(
    State(state): State<AppState>,
    Path(work_id): Path<String>,
    Json(body): Json<RunScriptRequest>,
) -> ApiResult<Json<WorkItem>> {
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
    // Blocking subprocess — run off the async runtime.
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
    Ok(Json(item))
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
    export_bundle(forge.as_ref(), &id, &destination).map_err(map_err)?;
    Ok(Json(ExportResponse { destination }))
}
