//! HTTP handlers for tool-history index and slice → workflow promotion (W4).

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::daemon_api::{
    ToolHistoryListQuery, ToolHistoryListResponse, WorkflowFromSliceRequest,
    WorkflowFromSliceResponse,
};
use crate::tool_history_index::{
    build_workflow_from_slice_refs, list_tool_history_runs, new_workflow_id_for_promotion,
};
use crate::workflow::{
    MedousaWorkflowPayload, WorkflowRecord, WorkflowStatus, WORKFLOW_SEQUENTIAL_JOB_TYPE,
    enqueue_workflow_job, preflight_grapheme_steps, shared_workflow_registry,
    validate_workflow_request, workflow_job_type_for_strategy,
};
use crate::workflow_handlers::WorkflowApiState;
use chrono::Utc;

pub async fn list_tool_history(
    Query(query): Query<ToolHistoryListQuery>,
) -> Json<ToolHistoryListResponse> {
    Json(list_tool_history_runs(&query))
}

pub async fn workflow_from_slice(
    state: axum::extract::State<WorkflowApiState>,
    Json(request): Json<WorkflowFromSliceRequest>,
) -> Result<Json<WorkflowFromSliceResponse>, (StatusCode, String)> {
    if request.refs.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "refs must include at least one slice reference".to_string(),
        ));
    }

    let (draft, mut notes) = build_workflow_from_slice_refs(&request.refs, request.name.as_deref())
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;

    validate_workflow_request(&draft)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    let preflight = preflight_grapheme_steps(state.composition.as_ref(), &draft.steps)
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

    let promoted_count = draft.steps.len();
    let mut workflow_id = None;

    if request.run {
        let workflow_id_value = new_workflow_id_for_promotion();
        let queue = draft.queue.as_deref().unwrap_or("default");
        let payload = MedousaWorkflowPayload {
            workflow_id: workflow_id_value.clone(),
            name: draft.name.clone(),
            strategy: draft.strategy.clone(),
            mode: draft.mode.clone(),
            on_failure: draft.on_failure.clone(),
            note: draft.note.clone(),
            lane: "interactive".to_string(),
            steps: draft.steps.clone(),
        };
        let job_id = enqueue_workflow_job(state.composition.as_ref(), &payload, queue, None)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        let _job_type = workflow_job_type_for_strategy(&draft.strategy)
            .unwrap_or(WORKFLOW_SEQUENTIAL_JOB_TYPE);

        shared_workflow_registry()
            .insert(WorkflowRecord {
                workflow_id: workflow_id_value.clone(),
                name: draft.name.clone(),
                strategy: draft.strategy.clone(),
                mode: draft.mode.clone(),
                on_failure: draft.on_failure.clone(),
                note: draft.note.clone(),
                root_job_id: job_id,
                status: WorkflowStatus::Enqueued,
                created_at: Utc::now(),
                scheduled_recurring_id: None,
                step_results: Vec::new(),
            })
            .await;

        workflow_id = Some(workflow_id_value);
        notes.push("Workflow enqueued.".to_string());
    }

    Ok(Json(WorkflowFromSliceResponse {
        workflow_id,
        draft,
        promoted_count,
        notes,
    }))
}

pub fn tool_history_router(state: WorkflowApiState) -> Router {
    Router::new()
        .route("/v1/tool-history/slices", get(list_tool_history))
        .route("/v1/workflows/from-slice", post(workflow_from_slice))
        .with_state(state)
}
