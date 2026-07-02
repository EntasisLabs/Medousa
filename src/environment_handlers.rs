//! HTTP handlers for `/v1/environment/*`.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::Utc;
use futures_util::stream::{self, Stream};
use medousa_types::environment::{
    EnvironmentPendingProposal, EnvironmentPendingResponse, EnvironmentProposeResponse,
    EnvironmentSpecPutRequest, EnvironmentSpecResponse, EnvironmentStreamEvent,
    EnvironmentStreamQuery, EnvironmentValidateRequest, EnvironmentValidateResponse,
};
use medousa_types::environment_validate::validate_environment_spec;
use std::convert::Infallible;
use std::time::Duration;

use crate::environment_store::{resolve_profile_id, EnvironmentHub};

#[derive(Clone)]
pub struct EnvironmentApiState {
    pub hub: &'static EnvironmentHub,
}

pub fn environment_router(state: EnvironmentApiState) -> Router {
    Router::new()
        .route("/v1/environment/spec", get(get_spec).put(put_spec))
        .route("/v1/environment/spec/validate", post(validate_spec))
        .route("/v1/environment/spec/propose", post(propose_spec))
        .route("/v1/environment/spec/pending", get(get_pending).delete(dismiss_pending))
        .route(
            "/v1/environment/spec/pending/apply",
            post(apply_pending),
        )
        .route("/v1/environment/spec/stream", get(stream_spec))
        .with_state(state)
}

async fn get_spec(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStreamQuery>,
) -> Result<Json<EnvironmentSpecResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let record = state
        .hub
        .get(&profile_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(EnvironmentSpecResponse {
        spec: record.spec,
        revision: record.revision,
    }))
}

async fn put_spec(
    State(state): State<EnvironmentApiState>,
    Json(body): Json<EnvironmentSpecPutRequest>,
) -> Result<Json<EnvironmentSpecResponse>, (StatusCode, String)> {
    let errors = validate_environment_spec(&body.spec);
    if !errors.is_empty() {
        return Err((StatusCode::BAD_REQUEST, errors.join("; ")));
    }
    let record = state
        .hub
        .put(body.spec, "user")
        .await
        .map_err(internal_error)?;
    Ok(Json(EnvironmentSpecResponse {
        spec: record.spec,
        revision: record.revision,
    }))
}

async fn validate_spec(
    Json(body): Json<EnvironmentValidateRequest>,
) -> Json<EnvironmentValidateResponse> {
    let errors = validate_environment_spec(&body.spec);
    Json(EnvironmentValidateResponse {
        valid: errors.is_empty(),
        errors,
    })
}

async fn propose_spec(
    State(state): State<EnvironmentApiState>,
    Json(body): Json<EnvironmentSpecPutRequest>,
) -> Result<Json<EnvironmentProposeResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(Some(body.spec.profile_id.as_str()));
    let errors = validate_environment_spec(&body.spec);
    let diff_summary = summarize_spec_diff(&body.spec);
    state
        .hub
        .set_pending(
            &profile_id,
            EnvironmentPendingProposal {
                proposed_spec: body.spec.clone(),
                diff_summary: diff_summary.clone(),
                errors: errors.clone(),
                proposed_at: Utc::now(),
                proposed_by: "agent".to_string(),
            },
        )
        .await;
    Ok(Json(EnvironmentProposeResponse {
        valid: errors.is_empty(),
        errors,
        diff_summary,
        proposed_spec: body.spec,
    }))
}

async fn get_pending(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStreamQuery>,
) -> Result<Json<EnvironmentPendingResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let pending = state.hub.pending(&profile_id).await;
    Ok(Json(EnvironmentPendingResponse { pending }))
}

async fn dismiss_pending(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStreamQuery>,
) -> Result<StatusCode, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    state.hub.clear_pending(&profile_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn apply_pending(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStreamQuery>,
) -> Result<Json<EnvironmentSpecResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let record = state
        .hub
        .apply_pending(&profile_id)
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(EnvironmentSpecResponse {
        spec: record.spec,
        revision: record.revision,
    }))
}

async fn stream_spec(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let since = query.since_revision.unwrap_or(0);
    let record = state
        .hub
        .get(&profile_id)
        .await
        .map_err(internal_error)?;

    let initial = EnvironmentStreamEvent {
        revision: record.revision,
        event_type: if record.revision > since {
            "spec_snapshot".to_string()
        } else {
            "heartbeat".to_string()
        },
        emitted_at_utc: Utc::now(),
        spec: if record.revision > since {
            Some(record.spec)
        } else {
            None
        },
        component_patches: None,
        feed_event: None,
    };

    let rx = state.hub.subscribe();
    let since_revision = since;
    let stream = stream::unfold((rx, Some(initial)), move |mut state| async move {
        let (mut rx, pending) = state;
        if let Some(event) = pending {
            let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
            return Some((Ok(Event::default().data(payload)), (rx, None)));
        }
        match rx.recv().await {
            Ok(event) => {
                if event.revision <= since_revision {
                    return Some((Ok(Event::default().data("{}")), (rx, None)));
                }
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                Some((Ok(Event::default().data(payload)), (rx, None)))
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                Some((Ok(Event::default().data("{}")), (rx, None)))
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30))))
}

fn summarize_spec_diff(spec: &medousa_types::environment::EnvironmentSpec) -> String {
    format!(
        "surfaces={} components={} preset={}",
        spec.surfaces.len(),
        spec.components.len(),
        spec.active_preset_id.as_deref().unwrap_or("default")
    )
}

fn internal_error(err: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
