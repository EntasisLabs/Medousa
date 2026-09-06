//! Authenticated daemon surface for colocated native-computer drivers.
//!
//! Clients can discover and preflight drivers with workshop read access. A
//! semantic observation is more sensitive than ordinary content reads, so it
//! requires operator execution authority and always crosses the governed
//! computer broker rather than talking to a sidecar directly.

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Json;
use medousa_computer_bridge::{
    ComputerAction, ComputerActionRequest, ComputerObservationRequest,
    DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT,
};
use medousa_world::{WorldDriverId, WorldPrincipal, WorldPrincipalId};
use serde::Deserialize;
use uuid::Uuid;

use crate::computer_driver::{
    ComputerActionIntent, ComputerObservationIntent, desktop_resource_id,
};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::request_principal::{Capability, RequestPrincipal};

const COMPUTER_OBSERVE_BODY_LIMIT: usize = 16 * 1024;
const COMPUTER_ACTION_BODY_LIMIT: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
pub struct ObserveComputerRequest {
    pub session_id: String,
    #[serde(default)]
    pub after_revision: Option<u64>,
    #[serde(default = "default_observation_node_limit")]
    pub max_nodes: u32,
}

#[derive(Debug, Deserialize)]
pub struct ActOnComputerRequest {
    pub session_id: String,
    pub observation_generation: String,
    pub observation_revision: u64,
    pub element_ref: String,
    pub action: ComputerAction,
    #[serde(default)]
    pub allow_high_risk: bool,
}

fn default_observation_node_limit() -> u32 {
    DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT
}

pub async fn list_computer_drivers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "drivers": state.computer_drivers.registrations().await,
    }))
}

pub async fn preflight_computer_driver(
    State(state): State<AppState>,
    Path(driver_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let driver_id = registered_driver(&state, &driver_id).await?;
    let preflight = state
        .computer_drivers
        .preflight(&driver_id)
        .await
        .map_err(computer_unavailable)?;
    let resource_id = desktop_resource_id(&driver_id, &preflight.session_id);
    Ok(Json(serde_json::json!({
        "ok": true,
        "driver_id": driver_id,
        "resource_id": resource_id,
        "preflight": preflight,
    })))
}

pub async fn observe_computer_driver(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(driver_id): Path<String>,
    Json(request): Json<ObserveComputerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let driver_id = registered_driver(&state, &driver_id).await?;
    let resource_id = desktop_resource_id(&driver_id, &request.session_id);
    ComputerObservationRequest {
        resource_id: resource_id.clone(),
        session_id: request.session_id.clone(),
        after_revision: request.after_revision,
        max_nodes: request.max_nodes,
    }
    .validate()
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;

    // Recheck immediately before admission. This diagnoses permission loss
    // early and ensures a stale client cannot cross login sessions.
    let preflight = state
        .computer_drivers
        .preflight(&driver_id)
        .await
        .map_err(computer_unavailable)?;
    if preflight.session_id != request.session_id {
        return Err((
            StatusCode::CONFLICT,
            "desktop session changed; preflight the computer driver again".to_string(),
        ));
    }
    if !preflight.semantic_observation_ready() {
        let guidance = preflight
            .permissions
            .iter()
            .find(|permission| {
                permission.permission
                    == medousa_computer_bridge::ComputerPermissionKind::Accessibility
            })
            .and_then(|permission| permission.guidance.as_deref())
            .unwrap_or("Grant the native computer driver Accessibility permission.");
        return Err((StatusCode::PRECONDITION_FAILED, guidance.to_string()));
    }

    let authority_id = crate::workshop_authority::current()
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error))?
        .to_string();
    let profile_id = principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let result = state
        .computer_drivers
        .observe(ComputerObservationIntent {
            authority_id,
            driver_id,
            desktop_session_id: request.session_id,
            principal: WorldPrincipal::human(WorldPrincipalId::new(format!(
                "human:{profile_id}"
            ))),
            resource_id,
            trace_id: format!("computer-human:observe:{}", Uuid::new_v4()),
            summary: "human observed attached desktop".to_string(),
            after_revision: request.after_revision,
            max_nodes: request.max_nodes,
        })
        .await
        .map_err(computer_unavailable)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "observation": result.observation,
        "provenance": result.provenance,
    })))
}

pub async fn act_on_computer_driver(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(driver_id): Path<String>,
    Json(request): Json<ActOnComputerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let driver_id = registered_driver(&state, &driver_id).await?;
    let resource_id = desktop_resource_id(&driver_id, &request.session_id);
    ComputerActionRequest {
        resource_id: resource_id.clone(),
        session_id: request.session_id.clone(),
        observation_generation: request.observation_generation.clone(),
        observation_revision: request.observation_revision,
        element_ref: request.element_ref.clone(),
        action: request.action,
    }
    .validate()
    .map_err(|error| (StatusCode::BAD_REQUEST, error))?;

    // Permission and login-session state are rechecked immediately before the
    // governed action is admitted. Native dispatch then enforces the exact
    // observation generation, revision, and opaque element reference again.
    let preflight = state
        .computer_drivers
        .preflight(&driver_id)
        .await
        .map_err(computer_unavailable)?;
    if preflight.session_id != request.session_id {
        return Err((
            StatusCode::CONFLICT,
            "desktop session changed; preflight and observe the computer again".to_string(),
        ));
    }
    if !preflight.semantic_observation_ready() {
        return Err((
            StatusCode::PRECONDITION_FAILED,
            "macOS Accessibility permission is required before computer actions".to_string(),
        ));
    }

    let authority_id = crate::workshop_authority::current()
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error))?
        .to_string();
    let profile_id = principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(|| state.workshop_identity_user_id());
    let result = state
        .computer_drivers
        .act(ComputerActionIntent {
            authority_id,
            driver_id,
            desktop_session_id: request.session_id,
            principal: WorldPrincipal::human(WorldPrincipalId::new(format!(
                "human:{profile_id}"
            ))),
            resource_id,
            trace_id: format!("computer-human:act:{}", Uuid::new_v4()),
            summary: "human requested a semantic desktop press".to_string(),
            observation_generation: request.observation_generation,
            observation_revision: request.observation_revision,
            element_ref: request.element_ref,
            action: request.action,
            allow_high_risk: request.allow_high_risk,
        })
        .await
        .map_err(computer_action_failed)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "receipt": result.receipt,
        "provenance": result.provenance,
    })))
}

async fn registered_driver(
    state: &AppState,
    requested: &str,
) -> Result<WorldDriverId, (StatusCode, String)> {
    let requested = requested.trim();
    if requested.is_empty() || requested.len() > 256 || requested.chars().any(char::is_control) {
        return Err((
            StatusCode::BAD_REQUEST,
            "computer driver id is invalid".to_string(),
        ));
    }
    let requested = WorldDriverId::new(requested);
    state
        .computer_drivers
        .registrations()
        .await
        .into_iter()
        .find(|registration| registration.driver_id == requested)
        .map(|registration| registration.driver_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("computer driver '{requested}' is not registered"),
            )
        })
}

fn computer_unavailable(error: String) -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, error)
}

fn computer_action_failed(error: String) -> (StatusCode, String) {
    let status = if error.contains("stale_observation")
        || error.contains("element_not_found")
        || error.contains("observation_required")
    {
        StatusCode::CONFLICT
    } else if error.contains("element_disabled") || error.contains("high_risk_target") {
        StatusCode::PRECONDITION_FAILED
    } else {
        StatusCode::BAD_GATEWAY
    };
    (status, error)
}

pub fn computer_surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .route(
            computer_policy(
                axum::http::Method::GET,
                "/v1/computer/drivers",
                Capability::WorkshopRead,
                1024,
                RateLimitClass::Read,
            ),
            get(list_computer_drivers),
        )
        .route(
            computer_policy(
                axum::http::Method::GET,
                "/v1/computer/drivers/{driver_id}/preflight",
                Capability::WorkshopRead,
                1024,
                RateLimitClass::Read,
            ),
            get(preflight_computer_driver),
        )
        .route(
            computer_policy(
                axum::http::Method::POST,
                "/v1/computer/drivers/{driver_id}/observe",
                Capability::AdminExecute,
                COMPUTER_OBSERVE_BODY_LIMIT,
                RateLimitClass::Read,
            ),
            post(observe_computer_driver),
        )
        .route(
            computer_policy(
                axum::http::Method::POST,
                "/v1/computer/drivers/{driver_id}/act",
                Capability::AdminExecute,
                COMPUTER_ACTION_BODY_LIMIT,
                RateLimitClass::Mutation,
            ),
            post(act_on_computer_driver),
        )
}

fn computer_policy(
    method: axum::http::Method,
    path: &'static str,
    required_capability: Capability,
    body_limit: usize,
    rate_limit_class: RateLimitClass,
) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(required_capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::ExactOrigin,
        body_limit,
        rate_limit_class,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_resource_identity_is_stable_and_session_bound() {
        let driver = WorldDriverId::new("driver:computer:test");
        assert_eq!(
            desktop_resource_id(&driver, "session:one"),
            desktop_resource_id(&driver, "session:one")
        );
        assert_ne!(
            desktop_resource_id(&driver, "session:one"),
            desktop_resource_id(&driver, "session:two")
        );
    }

    #[test]
    fn observation_route_is_operator_only_and_exact_origin() {
        let entries = computer_surface().inventory().entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 4);
        let observe = entries
            .iter()
            .find(|entry| entry.path.ends_with("/observe"))
            .expect("observe route");
        assert_eq!(observe.required_capability, Some("admin.execute"));
        assert_eq!(observe.browser_policy, BrowserPolicy::ExactOrigin);
        assert_eq!(observe.rate_limit_class, RateLimitClass::Read);
        let act = entries
            .iter()
            .find(|entry| entry.path.ends_with("/act"))
            .expect("act route");
        assert_eq!(act.required_capability, Some("admin.execute"));
        assert_eq!(act.browser_policy, BrowserPolicy::ExactOrigin);
        assert_eq!(act.rate_limit_class, RateLimitClass::Mutation);
    }

    #[test]
    fn semantic_action_policy_failures_are_not_reported_as_driver_failures() {
        assert_eq!(
            computer_action_failed("high_risk_target: confirm intent".to_string()).0,
            StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(
            computer_action_failed("element_disabled: unavailable".to_string()).0,
            StatusCode::PRECONDITION_FAILED
        );
    }
}
