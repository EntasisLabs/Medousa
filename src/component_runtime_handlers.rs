//! HTTP handlers for component runtime logs and probes.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_types::component_runtime::{
    ComponentRuntimeEventsQuery, ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult,
};

use crate::component_runtime_store::{component_runtime_hub, default_tail_limit};
use crate::component_store::component_exists_in_profile;
use crate::environment_store::resolve_profile_id;

#[derive(Clone)]
pub struct ComponentRuntimeApiState;

pub fn component_runtime_router() -> Router {
    Router::new()
        .route(
            "/v1/components/{component_id}/runtime/events",
            get(tail_runtime_events).post(append_runtime_events),
        )
        .route(
            "/v1/components/{component_id}/runtime/probe/{probe_id}/result",
            post(complete_probe),
        )
        .with_state(ComponentRuntimeApiState)
}

async fn ensure_component_allowed(
    profile_id: &str,
    component_id: &str,
) -> Result<(), (StatusCode, String)> {
    if !component_exists_in_profile(profile_id, component_id).await {
        return Err((
            StatusCode::NOT_FOUND,
            format!("component '{component_id}' is not registered on profile '{profile_id}'"),
        ));
    }
    Ok(())
}

async fn append_runtime_events(
    State(_state): State<ComponentRuntimeApiState>,
    Path(component_id): Path<String>,
    Json(body): Json<ComponentRuntimeEventsRequest>,
) -> Result<Json<ComponentRuntimeEventsResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(
        body.profile_id
            .as_deref(),
    );
    let component_id = component_id.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    let session_id = body.session_id.as_deref();
    let accepted = component_runtime_hub()
        .append_events(&profile_id, component_id, session_id, &body.events)
        .await
        .map_err(internal_error)?;
    Ok(Json(ComponentRuntimeEventsResponse {
        ok: true,
        accepted,
    }))
}

async fn tail_runtime_events(
    State(_state): State<ComponentRuntimeApiState>,
    Path(component_id): Path<String>,
    Query(query): Query<ComponentRuntimeEventsQuery>,
) -> Result<Json<ComponentRuntimeEventsTailResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let component_id = component_id.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    let limit = default_tail_limit(query.limit);
    let events = component_runtime_hub()
        .tail(&profile_id, component_id, limit)
        .await
        .map_err(internal_error)?;
    Ok(Json(ComponentRuntimeEventsTailResponse {
        component_id: component_id.to_string(),
        events,
    }))
}

async fn complete_probe(
    State(_state): State<ComponentRuntimeApiState>,
    Path((component_id, probe_id)): Path<(String, String)>,
    Json(body): Json<ComponentRuntimeProbeResult>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if body.probe_id != probe_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "probe_id path/body mismatch".to_string(),
        ));
    }
    if body.component_id != component_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "component_id path/body mismatch".to_string(),
        ));
    }
    component_runtime_hub().complete_probe(body).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn internal_error(message: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message)
}
