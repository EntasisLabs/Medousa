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
    ActorKind, ActorRef, EvidenceId, ExecutionLease, ExecutorDescriptor, IntegrationStrategy,
    LeaseId, RecoveryDisposition, ReviewDecision, ReviewDecisionId, WorkId, WorkItem, WorkPolicy,
};
use serde::{Deserialize, Serialize};

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
