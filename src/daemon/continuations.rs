//! Turn continuation status, lineage, and replay-and-resume handlers.


use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;

use crate::daemon::heartbeat::{safe_process_once, safe_publish_pending_events};
use crate::daemon::ingest::{job_succeeded, maybe_resume_agent_turn_from_child_job};
use stasis::prelude::RuntimeSdk;
use crate::daemon_api::{
    ContinuationStatusResponse, ReplayAndResumeResponse, TurnContinuationLineageResponse,
};

use crate::daemon::http::internal_error;
use crate::daemon::state::AppState;

pub async fn continuation_status(_state: State<AppState>) -> Json<ContinuationStatusResponse> {
    let snapshot = crate::turn_continuation::continuation_snapshot().await;
    let last = crate::turn_continuation::last_continuation_resume();
    Json(ContinuationStatusResponse {
        pending_count: snapshot.pending_count,
        consumed_count: snapshot.consumed_count,
        resumed_count: snapshot.resumed_count,
        dead_letter_pending_count: snapshot.dead_letter_pending_count,
        total_count: snapshot.total_count,
        last_resume_at_utc: last.as_ref().map(|event| event.resumed_at),
        last_resume_child_job_id: last.as_ref().map(|event| event.child_job_id.clone()),
        last_resume_turn_correlation_id: last
            .as_ref()
            .map(|event| event.turn_correlation_id.clone()),
    })
}

pub async fn continuation_lineage(
    AxumPath(turn_correlation_id): AxumPath<String>,
) -> Result<Json<TurnContinuationLineageResponse>, (StatusCode, String)> {
    let turn_correlation_id = turn_correlation_id.trim().to_string();
    if turn_correlation_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "turn_correlation_id is required".to_string(),
        ));
    }

    let records = crate::turn_continuation::continuation_lineage_for_turn(&turn_correlation_id, 50)
        .await;
    Ok(Json(TurnContinuationLineageResponse {
        turn_correlation_id,
        records: records
            .iter()
            .map(crate::turn_continuation::lineage_entry_from_record)
            .collect(),
    }))
}

pub async fn replay_and_resume_job(
    State(state): State<AppState>,
    AxumPath(job_id): AxumPath<String>,
) -> Result<Json<ReplayAndResumeResponse>, (StatusCode, String)> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "job_id is required".to_string()));
    }

    let replayed = crate::turn_continuation::replay_dead_letter_job(state.composition(), &job_id)
        .await
        .map_err(internal_error)?;
    if !replayed {
        return Ok(Json(ReplayAndResumeResponse {
            job_id,
            replayed: false,
            job_succeeded: false,
            agent_turn_resumed: false,
            message: "job is not in dead_letter state or does not exist".to_string(),
        }));
    }

    let sdk = RuntimeSdk::new(state.composition().clone());
    let worker_id = format!("{}:replay-resume", state.worker_id);
    for _ in 0..8 {
        let _ = safe_process_once(&sdk, "default", &worker_id).await.map_err(internal_error)?;
        if job_succeeded(state.composition(), &job_id).await {
            break;
        }
    }
    let _ = safe_publish_pending_events(&sdk, 50).await.map_err(internal_error)?;

    let succeeded = job_succeeded(state.composition(), &job_id).await;
    let agent_turn_resumed = if succeeded {
        maybe_resume_agent_turn_from_child_job(&state, &job_id).await
    } else {
        false
    };

    let message = if !succeeded {
        "job replayed from dead_letter but did not reach succeeded state".to_string()
    } else if agent_turn_resumed {
        "job replayed and agent continuation turn started".to_string()
    } else {
        "job replayed and succeeded; no pending agent continuation applied".to_string()
    };

    Ok(Json(ReplayAndResumeResponse {
        job_id,
        replayed: true,
        job_succeeded: succeeded,
        agent_turn_resumed,
        message,
    }))
}
