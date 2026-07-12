//! HTTP handlers for declarative workflow APIs (`/v1/workflows`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use stasis::application::runtime::runtime_factory::RuntimeComposition;
use stasis::domain::runtime::job::{Job, JobState};
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::ports::outbound::runtime::job_store::JobStore;
use uuid::Uuid;

use crate::daemon_api::{
    RecurringRunEntry, WorkflowDetailResponse, WorkflowListEntry, WorkflowPlanRequest,
    WorkflowPlanResponse, WorkflowRunRequest, WorkflowRunResponse, WorkflowRunsQuery,
    WorkflowRunsResponse, WorkflowScheduleRequest, WorkflowScheduleResponse,
    WorkflowStepResultDto, WorkflowsListQuery, WorkflowsListResponse,
};
use crate::recurring_delivery::{DeliveryResolveContext, bind_recurring_delivery_for_registration};
use crate::workflow::{
    MedousaWorkflowPayload, WorkflowRecord, WorkflowStatus, WORKFLOW_SEQUENTIAL_JOB_TYPE,
    decode_workflow_payload, encode_workflow_payload, enqueue_workflow_job, new_workflow_id,
    preflight_grapheme_steps, shared_workflow_registry, validate_workflow_request,
    workflow_job_type_for_strategy,
};
use crate::workflow_plan::plan_workflow_from_goal;

#[derive(Clone)]
pub struct WorkflowApiState {
    pub composition: Arc<RuntimeComposition>,
}

async fn get_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> stasis::prelude::Result<Option<Job>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await,
    }
}

async fn list_jobs_by_state(
    runtime: &RuntimeComposition,
    state: JobState,
) -> stasis::prelude::Result<Vec<Job>> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.list_by_state(state).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.list_by_state(state).await,
    }
}

async fn register_recurring_definition(
    runtime: &RuntimeComposition,
    definition: RecurringDefinition,
) -> stasis::prelude::Result<()> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.register_recurring(definition).await,
        RuntimeComposition::Surreal(rt) => rt.register_recurring(definition).await,
    }
}

fn job_state_label(state: &JobState) -> &'static str {
    match state {
        JobState::Enqueued => "enqueued",
        JobState::Leased => "leased",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::DeadLetter => "dead_letter",
        JobState::Canceled => "canceled",
    }
}

fn build_workflow_payload(
    workflow_id: &str,
    request: &WorkflowRunRequest,
    lane: &str,
) -> MedousaWorkflowPayload {
    MedousaWorkflowPayload {
        workflow_id: workflow_id.to_string(),
        name: request.name.clone(),
        strategy: request.strategy.clone(),
        mode: request.mode.clone(),
        on_failure: request.on_failure.clone(),
        note: request.note.clone(),
        lane: lane.to_string(),
        steps: request.steps.clone(),
    }
}

async fn validate_grapheme_steps_for_workflow(
    runtime: &RuntimeComposition,
    request: &WorkflowRunRequest,
) -> Result<(), (StatusCode, String)> {
    let preflight = preflight_grapheme_steps(runtime, &request.steps)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    for entry in preflight {
        let validated = entry
            .get("validation")
            .and_then(|value| value.get("validated"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !validated {
            return Err((
                StatusCode::BAD_REQUEST,
                "one or more grapheme steps failed runtime preflight".to_string(),
            ));
        }
    }
    Ok(())
}

fn step_results_to_dto(
    results: &[crate::workflow::WorkflowStepResult],
) -> Vec<WorkflowStepResultDto> {
    results
        .iter()
        .map(|result| WorkflowStepResultDto {
            id: result.id.clone(),
            kind: result.kind.clone(),
            status: result.status.clone(),
            output: result.output.clone(),
            error: result.error.clone(),
        })
        .collect()
}

async fn payload_for_record(
    runtime: &RuntimeComposition,
    record: &WorkflowRecord,
) -> Option<MedousaWorkflowPayload> {
    if record.root_job_id.is_empty() {
        return None;
    }
    let job = get_job(runtime, &record.root_job_id).await.ok()??;
    decode_workflow_payload(&job.payload_ref).ok()
}

pub async fn list_workflows(
    State(state): State<WorkflowApiState>,
    Query(query): Query<WorkflowsListQuery>,
) -> Result<Json<WorkflowsListResponse>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50);
    let registry = shared_workflow_registry();
    let records = registry.list(limit).await;
    let mut workflows = Vec::with_capacity(records.len());

    for record in records {
        let root_job = if record.root_job_id.is_empty() {
            None
        } else {
            get_job(state.composition.as_ref(), &record.root_job_id)
                .await
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        };
        let payload = if let Some(job) = root_job.as_ref() {
            decode_workflow_payload(&job.payload_ref).ok()
        } else {
            None
        };
        workflows.push(WorkflowListEntry {
            workflow_id: record.workflow_id.clone(),
            name: record.name.clone(),
            status: record.status.as_str().to_string(),
            strategy: record.strategy.clone(),
            mode: record.mode.clone(),
            root_job_id: record.root_job_id.clone(),
            root_job_state: root_job.as_ref().map(|job| job_state_label(&job.state).to_string()),
            scheduled_recurring_id: record.scheduled_recurring_id.clone(),
            created_at_utc: record.created_at,
            step_count: payload
                .as_ref()
                .map(|value| value.steps.len())
                .unwrap_or(record.step_results.len()),
        });
    }

    let count = workflows.len();
    Ok(Json(WorkflowsListResponse { count, workflows }))
}

pub async fn get_workflow_detail(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
) -> Result<Json<WorkflowDetailResponse>, (StatusCode, String)> {
    let workflow_id = workflow_id.trim();
    if workflow_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "workflow_id is required".to_string()));
    }

    let registry = shared_workflow_registry();
    let Some(record) = registry.get(workflow_id).await else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("unknown workflow '{workflow_id}'"),
        ));
    };

    let root_job = if record.root_job_id.is_empty() {
        None
    } else {
        get_job(state.composition.as_ref(), &record.root_job_id)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    };
    let payload = payload_for_record(state.composition.as_ref(), &record).await;

    Ok(Json(WorkflowDetailResponse {
        workflow_id: record.workflow_id,
        name: record.name,
        status: record.status.as_str().to_string(),
        strategy: record.strategy,
        mode: record.mode,
        on_failure: record.on_failure,
        note: record.note,
        root_job_id: record.root_job_id,
        root_job_state: root_job.as_ref().map(|job| job_state_label(&job.state).to_string()),
        scheduled_recurring_id: record.scheduled_recurring_id,
        created_at_utc: record.created_at,
        steps: payload.map(|value| value.steps).unwrap_or_default(),
        step_results: step_results_to_dto(&record.step_results),
    }))
}

pub async fn run_workflow(
    State(state): State<WorkflowApiState>,
    Json(request): Json<WorkflowRunRequest>,
) -> Result<Json<WorkflowRunResponse>, (StatusCode, String)> {
    validate_workflow_request(&request)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    validate_grapheme_steps_for_workflow(state.composition.as_ref(), &request).await?;

    let workflow_id = new_workflow_id();
    let queue = request.queue.as_deref().unwrap_or("default");
    let payload = build_workflow_payload(&workflow_id, &request, "interactive");
    let job_id = enqueue_workflow_job(state.composition.as_ref(), &payload, queue, None)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let job_type = workflow_job_type_for_strategy(&request.strategy)
        .unwrap_or(WORKFLOW_SEQUENTIAL_JOB_TYPE);

    let record = WorkflowRecord {
        workflow_id: workflow_id.clone(),
        name: request.name.clone(),
        strategy: request.strategy.clone(),
        mode: request.mode.clone(),
        on_failure: request.on_failure.clone(),
        note: request.note.clone(),
        root_job_id: job_id.clone(),
        status: WorkflowStatus::Enqueued,
        created_at: Utc::now(),
        scheduled_recurring_id: None,
        step_results: Vec::new(),
    };
    shared_workflow_registry().insert(record).await;

    Ok(Json(WorkflowRunResponse {
        workflow_id,
        status: "enqueued".to_string(),
        strategy: request.strategy,
        root_job_id: job_id,
        job_type: job_type.to_string(),
        lane: "interactive".to_string(),
    }))
}

pub async fn plan_workflow(
    Json(request): Json<WorkflowPlanRequest>,
) -> Json<WorkflowPlanResponse> {
    Json(plan_workflow_from_goal(&request))
}

pub async fn schedule_workflow(
    State(state): State<WorkflowApiState>,
    Json(request): Json<WorkflowScheduleRequest>,
) -> Result<Json<WorkflowScheduleResponse>, (StatusCode, String)> {
    validate_workflow_request(&request.workflow)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    validate_grapheme_steps_for_workflow(state.composition.as_ref(), &request.workflow).await?;

    let cron_expr = request.cron_expr.trim();
    if cron_expr.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "cron_expr is required".to_string()));
    }

    let workflow_id = new_workflow_id();
    let queue = request.workflow.queue.as_deref().unwrap_or("default");
    let payload = build_workflow_payload(&workflow_id, &request.workflow, "scheduled");
    let payload_template_ref = encode_workflow_payload(&payload)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let recurring_id = request
        .recurring_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("wf-recur-{}", Uuid::new_v4().simple()));
    let timezone = request
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("UTC");
    let enabled = request.enabled.unwrap_or(true);

    let now = Utc::now();
    let job_type = workflow_job_type_for_strategy(&request.workflow.strategy)
        .unwrap_or(WORKFLOW_SEQUENTIAL_JOB_TYPE)
        .to_string();
    let mut definition = RecurringDefinition {
        id: recurring_id.clone(),
        queue: queue.to_string(),
        job_type,
        payload_template_ref,
        cron_expr: cron_expr.to_string(),
        timezone: timezone.to_string(),
        jitter_seconds: 0,
        enabled,
        max_attempts: 1,
        next_run_at: now,
        last_run_at: None,
        lease_owner: None,
        lease_expires_at: None,
    };
    definition.next_run_at = definition
        .compute_next_run_at(now)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    let delivery_input = serde_json::json!({
        "delivery": request.delivery,
        "display_name": request.display_name,
    });
    let _ = bind_recurring_delivery_for_registration(
        &recurring_id,
        cron_expr,
        timezone,
        &delivery_input,
        DeliveryResolveContext {
            ambient: None,
            fallback_session_id: format!("recurring-{recurring_id}"),
        },
    )
    .await
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    register_recurring_definition(state.composition.as_ref(), definition.clone())
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let record = WorkflowRecord {
        workflow_id: workflow_id.clone(),
        name: request.workflow.name.clone(),
        strategy: request.workflow.strategy.clone(),
        mode: request.workflow.mode.clone(),
        on_failure: request.workflow.on_failure.clone(),
        note: request.workflow.note.clone(),
        root_job_id: String::new(),
        status: WorkflowStatus::Enqueued,
        created_at: now,
        scheduled_recurring_id: Some(recurring_id.clone()),
        step_results: Vec::new(),
    };
    shared_workflow_registry().insert(record).await;

    Ok(Json(WorkflowScheduleResponse {
        workflow_id,
        status: "scheduled".to_string(),
        recurring_id,
        cron_expr: cron_expr.to_string(),
        timezone: timezone.to_string(),
        next_run_at_utc: definition.next_run_at,
        materialized_job_id: None,
    }))
}

pub async fn list_workflow_runs(
    State(state): State<WorkflowApiState>,
    Path(workflow_id): Path<String>,
    Query(query): Query<WorkflowRunsQuery>,
) -> Result<Json<WorkflowRunsResponse>, (StatusCode, String)> {
    let workflow_id = workflow_id.trim();
    if workflow_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "workflow_id is required".to_string()));
    }

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let mut runs = Vec::new();

    for job_state in [
        JobState::Running,
        JobState::Leased,
        JobState::Enqueued,
        JobState::Succeeded,
        JobState::Failed,
        JobState::DeadLetter,
        JobState::Canceled,
    ] {
        let batch = list_jobs_by_state(state.composition.as_ref(), job_state)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        for job in batch {
            if job.correlation_id != workflow_id {
                continue;
            }
            runs.push(job_to_run_entry(state.composition.as_ref(), &job).await?);
            if runs.len() >= limit {
                break;
            }
        }
        if runs.len() >= limit {
            break;
        }
    }

    runs.sort_by_key(|right| std::cmp::Reverse(right.scheduled_at_utc));
    runs.truncate(limit);
    let count = runs.len();
    Ok(Json(WorkflowRunsResponse {
        workflow_id: workflow_id.to_string(),
        count,
        runs,
    }))
}

async fn job_to_run_entry(
    runtime: &RuntimeComposition,
    job: &Job,
) -> Result<RecurringRunEntry, (StatusCode, String)> {
    let attempts = match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(&job.id).await,
        RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(&job.id).await,
    }
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let latest = attempts.last();
    let output_text = latest.and_then(|attempt| attempt.diagnostics.clone());
    let updated_at = job
        .finished_at
        .or(job.started_at)
        .unwrap_or(job.scheduled_at);

    Ok(RecurringRunEntry {
        job_id: job.id.clone(),
        status: job_state_label(&job.state).to_string(),
        is_terminal: matches!(
            job.state,
            JobState::Succeeded | JobState::Failed | JobState::DeadLetter | JobState::Canceled
        ),
        attempt_count: attempts.len(),
        latest_outcome: latest.map(|attempt| format!("{:?}", attempt.outcome)),
        output_text,
        scheduled_at_utc: job.scheduled_at,
        updated_at_utc: updated_at,
    })
}

pub fn workflow_router(state: WorkflowApiState) -> Router {
    Router::new()
        .route("/v1/workflows", get(list_workflows).post(run_workflow))
        .route("/v1/workflows/plan", post(plan_workflow))
        .route("/v1/workflows/schedule", post(schedule_workflow))
        .route(
            "/v1/workflows/{workflow_id}",
            get(get_workflow_detail),
        )
        .route(
            "/v1/workflows/{workflow_id}/runs",
            get(list_workflow_runs),
        )
        .with_state(state)
}
