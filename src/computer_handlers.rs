//! Authenticated daemon surface for colocated native-computer drivers.
//!
//! Clients can discover and preflight drivers with workshop read access. A
//! semantic observation is more sensitive than ordinary content reads, so it
//! requires operator execution authority and always crosses the governed
//! computer broker rather than talking to a sidecar directly.

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use medousa_computer_bridge::{
    ComputerAction, ComputerActionRequest, ComputerObservationRequest,
    DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT, DEFAULT_COMPUTER_SCREENSHOT_MAX_WIDTH,
    MAX_COMPUTER_SCREENSHOT_WIDTH, MIN_COMPUTER_SCREENSHOT_WIDTH,
};
use medousa_world::{WorldDriverId, WorldPrincipal, WorldPrincipalId};
use serde::Deserialize;
use uuid::Uuid;

use crate::computer_driver::{
    ComputerActionIntent, ComputerControlHolder, ComputerObservationIntent,
    ComputerPixelObservationIntent, desktop_resource_id,
};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::request_principal::{Capability, RequestPrincipal};

const COMPUTER_OBSERVE_BODY_LIMIT: usize = 16 * 1024;
const COMPUTER_ACTION_BODY_LIMIT: usize = 16 * 1024;
const COMPUTER_WATCH_BODY_LIMIT: usize = 16 * 1024;
const COMPUTER_CONTROL_BODY_LIMIT: usize = 16 * 1024;
const COMPUTER_WATCH_NODE_LIMIT: u32 = 512;

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
    pub value: Option<String>,
    #[serde(default)]
    pub allow_high_risk: bool,
}

#[derive(Debug, Deserialize)]
pub struct WatchComputerRequest {
    pub session_id: String,
    #[serde(default = "default_screenshot_width")]
    pub max_width: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerControlAction {
    TakeControl,
    ReturnToMedousa,
}

#[derive(Debug, Deserialize)]
pub struct ControlComputerRequest {
    pub session_id: String,
    pub action: ComputerControlAction,
}

fn default_observation_node_limit() -> u32 {
    DEFAULT_COMPUTER_OBSERVATION_NODE_LIMIT
}

fn default_screenshot_width() -> u32 {
    DEFAULT_COMPUTER_SCREENSHOT_MAX_WIDTH
}

pub async fn list_computer_drivers(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "drivers": state.computer_drivers.registrations().await,
    }))
}

pub async fn preflight_computer_driver(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(driver_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let driver_id = registered_driver(&state, &driver_id).await?;
    let preflight = state
        .computer_drivers
        .preflight(&driver_id)
        .await
        .map_err(computer_unavailable)?;
    let resource_id = desktop_resource_id(&driver_id, &preflight.session_id);
    let control = state
        .computer_drivers
        .control_state(
            &current_authority_id()?,
            &driver_id,
            &preflight.session_id,
            &request_human(&state, &principal),
        )
        .await
        .map_err(computer_unavailable)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "driver_id": driver_id,
        "resource_id": resource_id,
        "preflight": preflight,
        "control": control,
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

    let authority_id = current_authority_id()?;
    let result = state
        .computer_drivers
        .observe(ComputerObservationIntent {
            authority_id,
            driver_id,
            desktop_session_id: request.session_id,
            principal: request_human(&state, &principal),
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

pub async fn watch_computer_driver(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(driver_id): Path<String>,
    Json(request): Json<WatchComputerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !(MIN_COMPUTER_SCREENSHOT_WIDTH..=MAX_COMPUTER_SCREENSHOT_WIDTH).contains(&request.max_width)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "computer watch max_width must be between {MIN_COMPUTER_SCREENSHOT_WIDTH} and {MAX_COMPUTER_SCREENSHOT_WIDTH}"
            ),
        ));
    }
    let driver_id = registered_driver(&state, &driver_id).await?;
    let resource_id = desktop_resource_id(&driver_id, &request.session_id);
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
        return Err((
            StatusCode::PRECONDITION_FAILED,
            permission_guidance(
                &preflight,
                medousa_computer_bridge::ComputerPermissionKind::Accessibility,
                "Grant the native computer driver Accessibility permission.",
            ),
        ));
    }
    if !preflight.pixel_observation_ready() {
        return Err((
            StatusCode::PRECONDITION_FAILED,
            permission_guidance(
                &preflight,
                medousa_computer_bridge::ComputerPermissionKind::ScreenCapture,
                "Grant the native computer driver Screen Recording permission.",
            ),
        ));
    }

    let authority_id = current_authority_id()?;
    let human = request_human(&state, &principal);
    let control = state
        .computer_drivers
        .control_state(&authority_id, &driver_id, &request.session_id, &human)
        .await
        .map_err(computer_unavailable)?;
    let latest = state
        .computer_drivers
        .latest_observation(&driver_id, &request.session_id)
        .await
        .map_err(computer_unavailable)?;
    let (observation_generation, observation_revision, window_resource_id, observation_provenance) =
        if let Some(latest) = latest {
            let window_resource_id = latest.focused_window_resource_id.ok_or_else(|| {
                (
                    StatusCode::PRECONDITION_FAILED,
                    "the attached desktop does not have a focused window to watch".to_string(),
                )
            })?;
            (latest.generation, latest.revision, window_resource_id, None)
        } else {
            if control.holder != ComputerControlHolder::Available && !control.requester_has_control
            {
                return Err((
                    StatusCode::CONFLICT,
                    "computer operation is between observations; retry the live view shortly"
                        .to_string(),
                ));
            }
            let observation = state
                .computer_drivers
                .observe(ComputerObservationIntent {
                    authority_id: authority_id.clone(),
                    driver_id: driver_id.clone(),
                    desktop_session_id: request.session_id.clone(),
                    principal: human.clone(),
                    resource_id: resource_id.clone(),
                    trace_id: format!("computer-human:watch-observe:{}", Uuid::new_v4()),
                    summary: "human watched the attached desktop".to_string(),
                    after_revision: None,
                    max_nodes: COMPUTER_WATCH_NODE_LIMIT,
                })
                .await
                .map_err(computer_unavailable)?;
            let window_resource_id = observation
                .observation
                .focused_window_resource_id
                .clone()
                .ok_or_else(|| {
                    (
                        StatusCode::PRECONDITION_FAILED,
                        "the attached desktop does not have a focused window to watch".to_string(),
                    )
                })?;
            (
                observation.observation.observation_generation,
                observation.observation.revision,
                window_resource_id,
                Some(observation.provenance),
            )
        };
    let pixels = state
        .computer_drivers
        .capture_pixels(ComputerPixelObservationIntent {
            authority_id: authority_id.clone(),
            driver_id: driver_id.clone(),
            desktop_session_id: request.session_id.clone(),
            principal: human.clone(),
            resource_id,
            trace_id: format!("computer-human:watch-pixels:{}", Uuid::new_v4()),
            summary: "human watched redacted focused-window pixels".to_string(),
            observation_generation: observation_generation.clone(),
            observation_revision,
            window_resource_id: window_resource_id.clone(),
            max_width: request.max_width,
        })
        .await
        .map_err(computer_unavailable)?;
    let control = state
        .computer_drivers
        .control_state(&authority_id, &driver_id, &request.session_id, &human)
        .await
        .map_err(computer_unavailable)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "observation": {
            "generation": observation_generation,
            "revision": observation_revision,
            "focused_window_resource_id": window_resource_id,
        },
        "capture": pixels.capture,
        "control": control,
        "provenance": {
            "observation": observation_provenance,
            "pixels": pixels.provenance,
        },
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
        value: request.value.clone(),
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

    let authority_id = current_authority_id()?;
    let result = state
        .computer_drivers
        .act(ComputerActionIntent {
            authority_id,
            driver_id,
            desktop_session_id: request.session_id,
            principal: request_human(&state, &principal),
            resource_id,
            trace_id: format!("computer-human:act:{}", Uuid::new_v4()),
            summary: format!(
                "human requested semantic desktop action {}",
                request.action.as_str()
            ),
            observation_generation: request.observation_generation,
            observation_revision: request.observation_revision,
            element_ref: request.element_ref,
            action: request.action,
            value: request.value,
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

pub async fn control_computer_driver(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(driver_id): Path<String>,
    Json(request): Json<ControlComputerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let driver_id = registered_driver(&state, &driver_id).await?;
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

    let authority_id = current_authority_id()?;
    let human = request_human(&state, &principal);
    let control = match request.action {
        ComputerControlAction::TakeControl => {
            state
                .computer_drivers
                .take_control(&authority_id, &driver_id, &request.session_id, human)
                .await
        }
        ComputerControlAction::ReturnToMedousa => {
            state
                .computer_drivers
                .return_control(&authority_id, &driver_id, &request.session_id, human)
                .await
        }
    }
    .map_err(computer_control_failed)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "control": control,
    })))
}

fn current_authority_id() -> Result<String, (StatusCode, String)> {
    crate::workshop_authority::current()
        .map(|authority| authority.to_string())
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error))
}

fn request_human(state: &AppState, principal: &RequestPrincipal) -> WorldPrincipal {
    let profile_id = principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(|| state.workshop_identity_user_id());
    WorldPrincipal::human(WorldPrincipalId::new(format!("human:{profile_id}")))
}

fn permission_guidance(
    preflight: &medousa_computer_bridge::ComputerDriverPreflight,
    permission: medousa_computer_bridge::ComputerPermissionKind,
    fallback: &str,
) -> String {
    preflight
        .permissions
        .iter()
        .find(|report| report.permission == permission)
        .and_then(|report| report.guidance.as_deref())
        .unwrap_or(fallback)
        .to_string()
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
    } else if error.contains("element_disabled")
        || error.contains("action_unavailable")
        || error.contains("high_risk_target")
    {
        StatusCode::PRECONDITION_FAILED
    } else {
        StatusCode::BAD_GATEWAY
    };
    (status, error)
}

fn computer_control_failed(error: String) -> (StatusCode, String) {
    let status =
        if error.contains("controlled by another human") || error.contains("control conflict") {
            StatusCode::CONFLICT
        } else if error.contains("authenticated human") || error.contains("does not provide") {
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
                "/v1/computer/drivers/{driver_id}/watch",
                Capability::AdminExecute,
                COMPUTER_WATCH_BODY_LIMIT,
                RateLimitClass::Read,
            ),
            post(watch_computer_driver),
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
        .route(
            computer_policy(
                axum::http::Method::POST,
                "/v1/computer/drivers/{driver_id}/control",
                Capability::AdminExecute,
                COMPUTER_CONTROL_BODY_LIMIT,
                RateLimitClass::Mutation,
            ),
            post(control_computer_driver),
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
        assert_eq!(entries.len(), 6);
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
        let watch = entries
            .iter()
            .find(|entry| entry.path.ends_with("/watch"))
            .expect("watch route");
        assert_eq!(watch.required_capability, Some("admin.execute"));
        assert_eq!(watch.browser_policy, BrowserPolicy::ExactOrigin);
        assert_eq!(watch.rate_limit_class, RateLimitClass::Read);
        let control = entries
            .iter()
            .find(|entry| entry.path.ends_with("/control"))
            .expect("control route");
        assert_eq!(control.required_capability, Some("admin.execute"));
        assert_eq!(control.browser_policy, BrowserPolicy::ExactOrigin);
        assert_eq!(control.rate_limit_class, RateLimitClass::Mutation);
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
        assert_eq!(
            computer_action_failed("action_unavailable: observe again".to_string()).0,
            StatusCode::PRECONDITION_FAILED
        );
    }
}
