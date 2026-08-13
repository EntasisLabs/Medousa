//! HTTP handlers for `/v1/environment/*`.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use chrono::Utc;
use futures_util::stream::{self, Stream};
use medousa_types::environment::{
    EnvironmentPendingProposal, EnvironmentPendingResponse, EnvironmentProposeResponse,
    EnvironmentSpecPutRequest, EnvironmentSpecResponse, EnvironmentStatusResponse,
    EnvironmentStreamEvent, EnvironmentStreamQuery, EnvironmentValidateRequest,
    EnvironmentValidateResponse,
};
use medousa_types::environment_validate::validate_environment_spec;
use stasis::prelude::RuntimeComposition;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::environment_store::{EnvironmentHub, resolve_profile_id};
use crate::request_principal::Capability;

#[derive(Clone)]
pub struct EnvironmentApiState {
    pub hub: &'static EnvironmentHub,
    pub runtime: Option<Arc<RuntimeComposition>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentStatusQuery {
    profile_id: Option<String>,
    surface_id: Option<String>,
    include_runtime: Option<bool>,
}

pub fn environment_surface() -> DeclaredRouter<EnvironmentApiState> {
    DeclaredRouter::default()
        .methods([
            (
                environment_policy(axum::http::Method::GET, "/v1/environment/spec", 1024),
                get(get_spec),
            ),
            (
                environment_policy(axum::http::Method::PUT, "/v1/environment/spec", 1024 * 1024),
                axum::routing::put(put_spec),
            ),
        ])
        .route(
            environment_policy(axum::http::Method::GET, "/v1/environment/status", 1024),
            get(get_status),
        )
        .route(
            environment_policy(
                axum::http::Method::POST,
                "/v1/environment/spec/validate",
                1024 * 1024,
            ),
            post(validate_spec),
        )
        .route(
            environment_policy(
                axum::http::Method::POST,
                "/v1/environment/spec/propose",
                1024 * 1024,
            ),
            post(propose_spec),
        )
        .methods([
            (
                environment_policy(
                    axum::http::Method::GET,
                    "/v1/environment/spec/pending",
                    1024,
                ),
                get(get_pending),
            ),
            (
                environment_policy(
                    axum::http::Method::DELETE,
                    "/v1/environment/spec/pending",
                    1024,
                ),
                axum::routing::delete(dismiss_pending),
            ),
        ])
        .route(
            environment_policy(
                axum::http::Method::POST,
                "/v1/environment/spec/pending/apply",
                1024,
            ),
            post(apply_pending),
        )
        .route(
            environment_stream_policy("/v1/environment/spec/stream"),
            get(stream_spec),
        )
}

fn environment_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminRuntime),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::Administration,
    }
}

fn environment_stream_policy(path: &'static str) -> RoutePolicy {
    RoutePolicy {
        rate_limit_class: RateLimitClass::Stream,
        ..environment_policy(axum::http::Method::GET, path, 1024)
    }
}

async fn get_spec(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStreamQuery>,
) -> Result<Json<EnvironmentSpecResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let record = state.hub.get(&profile_id).await.map_err(internal_error)?;
    Ok(Json(EnvironmentSpecResponse {
        spec: record.spec,
        revision: record.revision,
    }))
}

async fn get_status(
    State(state): State<EnvironmentApiState>,
    Query(query): Query<EnvironmentStatusQuery>,
) -> Result<Json<EnvironmentStatusResponse>, (StatusCode, String)> {
    let profile_id = resolve_profile_id(query.profile_id.as_deref());
    let surface_filter = query
        .surface_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let runtime = state.runtime.as_deref();
    let diagnostics = query.include_runtime.unwrap_or(false).then_some({
        crate::custom_view_status::DoctorDiagnosticOptions {
            component_id_filter: None,
            include_runtime: true,
            include_static_lint: false,
            probe: false,
            session_id: None,
        }
    });
    crate::custom_view_status::build_environment_status(
        state.hub,
        &profile_id,
        surface_filter,
        runtime,
        diagnostics.as_ref(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
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
    let record = state.hub.get(&profile_id).await.map_err(internal_error)?;

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
        runtime_probe: None,
    };

    let rx = state.hub.subscribe();
    let since_revision = since;
    let stream = stream::unfold((rx, Some(initial)), move |state| async move {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_inventory_is_explicitly_runtime_admin() {
        let entries = environment_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 9);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::Administration
                && entry.required_capability == Some("admin.runtime")
        }));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.rate_limit_class == RateLimitClass::Stream)
                .count(),
            1
        );
    }
}
