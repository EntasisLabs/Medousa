//! HTTP handlers for component runtime logs and probes.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
#[cfg(feature = "full-daemon")]
use axum::routing::{get, post};
use medousa_types::component_runtime::{
    ComponentRuntimeEventsQuery, ComponentRuntimeEventsRequest, ComponentRuntimeEventsResponse,
    ComponentRuntimeEventsTailResponse, ComponentRuntimeProbeResult,
};

use crate::component_runtime_store::{component_runtime_hub, default_tail_limit};
use crate::component_store::component_exists_in_profile;
#[cfg(feature = "full-daemon")]
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::environment_store::resolve_profile_id;

#[derive(Clone)]
pub struct ComponentRuntimeApiState;

#[cfg(feature = "full-daemon")]
pub fn component_runtime_surface() -> DeclaredRouter<ComponentRuntimeApiState> {
    DeclaredRouter::default()
        .methods([
            (
                component_runtime_policy(
                    axum::http::Method::GET,
                    "/v1/components/{component_id}/runtime/events",
                    crate::request_principal::Capability::ContentRead,
                    1024,
                    RateLimitClass::Read,
                ),
                get(tail_runtime_events),
            ),
            (
                component_runtime_policy(
                    axum::http::Method::POST,
                    "/v1/components/{component_id}/runtime/events",
                    crate::request_principal::Capability::ContentWrite,
                    1024 * 1024,
                    RateLimitClass::Mutation,
                ),
                post(append_runtime_events),
            ),
        ])
        .route(
            component_runtime_policy(
                axum::http::Method::POST,
                "/v1/components/{component_id}/runtime/probe/{probe_id}/result",
                crate::request_principal::Capability::ContentWrite,
                256 * 1024,
                RateLimitClass::Mutation,
            ),
            post(complete_probe),
        )
}

#[cfg(feature = "full-daemon")]
fn component_runtime_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: crate::request_principal::Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class,
    }
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

pub async fn append_runtime_events(
    State(_state): State<ComponentRuntimeApiState>,
    Path(component_id): Path<String>,
    Json(body): Json<ComponentRuntimeEventsRequest>,
) -> Result<Json<ComponentRuntimeEventsResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(body.profile_id.as_deref());
    let component_id = component_id.trim();
    ensure_component_allowed(&profile_id, component_id).await?;
    let session_id = body.session_id.as_deref();
    let accepted = component_runtime_hub()
        .append_events(&profile_id, component_id, session_id, &body.events)
        .await
        .map_err(internal_error)?;
    Ok(Json(ComponentRuntimeEventsResponse { ok: true, accepted }))
}

pub async fn tail_runtime_events(
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

pub async fn complete_probe(
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
