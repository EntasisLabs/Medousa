//! Daemon HTTP handlers for Agent Browser sessions and registered client tools.

use std::time::Duration;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use medousa_browser_lite::SearchResponse;
use medousa_world::WorldEffectClass;
use serde::Deserialize;
use uuid::Uuid;

use crate::browser_host_client::browser_host_healthy;
use medousa_browser_lite::search_ddg_html_cached_async;

pub use crate::client_tools::{
    ClientRegistration, ClientRegistry, ClientToolDefinition, ClientToolRequest,
    ClientToolResultRequest, ClientToolResultResponse, RegisterClientRequest,
    RegisterClientResponse,
};

use crate::browser_sessions::{
    BrowserActOutcome, BrowserSessionCompleteRequest, complete_browser_act_session_for_driver,
    complete_browser_session, complete_browser_session_for_driver, get_browser_session,
};
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::request_principal::Capability;

use crate::daemon::isolated_browser_host::{
    CreateIsolatedBrowserWorldRequest, IsolatedBrowserCleanupQuery, IsolatedBrowserError,
    IsolatedBrowserLifecycleRequest, IsolatedBrowserNavigateRequest,
    IsolatedBrowserObserveRequest, IsolatedBrowserScreenshotRequest,
};

fn principal_profile_id(principal: &crate::request_principal::RequestPrincipal) -> String {
    principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id)
}

fn isolated_browser_error(error: IsolatedBrowserError) -> (StatusCode, String) {
    let status = match &error {
        IsolatedBrowserError::Invalid(_) => StatusCode::BAD_REQUEST,
        IsolatedBrowserError::NotFound(_) => StatusCode::NOT_FOUND,
        IsolatedBrowserError::Conflict(_) => StatusCode::CONFLICT,
        IsolatedBrowserError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        IsolatedBrowserError::Driver(_) => StatusCode::BAD_GATEWAY,
        IsolatedBrowserError::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

fn isolated_browser_authority_error(error: String) -> (StatusCode, String) {
    (StatusCode::CONFLICT, error)
}

fn isolated_browser_trace_id(operation: &str) -> String {
    format!("browser-human:{operation}:{}", Uuid::new_v4())
}

pub async fn list_isolated_browser_worlds(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
) -> Json<serde_json::Value> {
    let worlds = state
        .isolated_browser
        .list(&principal_profile_id(&principal))
        .await;
    Json(serde_json::json!({ "ok": true, "worlds": worlds }))
}

pub async fn create_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(request): Json<CreateIsolatedBrowserWorldRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let authority_id = crate::workshop_authority::current()
        .map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error))?
        .to_string();
    let world = state
        .isolated_browser
        .create(
            &principal_profile_id(&principal),
            &authority_id,
            request,
        )
        .await
        .map_err(isolated_browser_error)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "ok": true, "world": world })),
    ))
}

pub async fn get_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(world_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let world = state
        .isolated_browser
        .get(&principal_profile_id(&principal), &world_id)
        .await
        .map_err(isolated_browser_error)?;
    Ok(Json(serde_json::json!({ "ok": true, "world": world })))
}

pub async fn control_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(world_id): Path<String>,
    Json(request): Json<IsolatedBrowserLifecycleRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let world = state
        .isolated_browser
        .lifecycle(
            &principal_profile_id(&principal),
            &world_id,
            request.action,
        )
        .await
        .map_err(isolated_browser_error)?;
    Ok(Json(serde_json::json!({ "ok": true, "world": world })))
}

pub async fn navigate_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(world_id): Path<String>,
    Json(request): Json<IsolatedBrowserNavigateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let profile_id = principal_profile_id(&principal);
    let current = state
        .isolated_browser
        .get(&profile_id, &world_id)
        .await
        .map_err(isolated_browser_error)?;
    let tab_id = current.tab_id.as_deref().ok_or_else(|| {
        isolated_browser_error(IsolatedBrowserError::Unavailable(
            "isolated browser has no active page".to_string(),
        ))
    })?;
    let owner_principal_id = format!("human:{profile_id}");
    let trace_id = isolated_browser_trace_id("navigate");
    let admission = crate::world_authority::admit_owned_browser_human_intent(
        crate::world_authority::OwnedBrowserHumanIntent {
            authority_id: &current.authority_id,
            driver_id: current.driver.driver_id.as_str(),
            tab_group_id: &current.tab_group_id,
            tab_id,
            owner_principal_id: &owner_principal_id,
            trace_id: &trace_id,
            summary: "human navigated isolated browser",
            effect_class: WorldEffectClass::LocalReversible,
        },
    )
    .map_err(isolated_browser_authority_error)?;
    let world = match state
        .isolated_browser
        .navigate(&profile_id, &world_id, &request.url)
        .await
    {
        Ok(world) => world,
        Err(error) => {
            let _ = crate::world_authority::fail_browser_action(&admission, &error.to_string());
            return Err(isolated_browser_error(error));
        }
    };
    crate::world_authority::complete_browser_action(
        &admission,
        "human isolated-browser navigation committed",
    )
    .map_err(isolated_browser_authority_error)?;
    Ok(Json(serde_json::json!({ "ok": true, "world": world })))
}

pub async fn observe_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(world_id): Path<String>,
    Json(request): Json<IsolatedBrowserObserveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let profile_id = principal_profile_id(&principal);
    let current = state
        .isolated_browser
        .get(&profile_id, &world_id)
        .await
        .map_err(isolated_browser_error)?;
    let tab_id = current.tab_id.as_deref().ok_or_else(|| {
        isolated_browser_error(IsolatedBrowserError::Unavailable(
            "isolated browser has no active page".to_string(),
        ))
    })?;
    let owner_principal_id = format!("human:{profile_id}");
    let trace_id = isolated_browser_trace_id("observe");
    let admission = crate::world_authority::admit_owned_browser_human_intent(
        crate::world_authority::OwnedBrowserHumanIntent {
            authority_id: &current.authority_id,
            driver_id: current.driver.driver_id.as_str(),
            tab_group_id: &current.tab_group_id,
            tab_id,
            owner_principal_id: &owner_principal_id,
            trace_id: &trace_id,
            summary: "human observed isolated browser",
            effect_class: WorldEffectClass::Observe,
        },
    )
    .map_err(isolated_browser_authority_error)?;
    let mut observation = match state
        .isolated_browser
        .observe(
            &profile_id,
            &world_id,
            request.since_revision,
            request.max_nodes,
        )
        .await
    {
        Ok(observation) => observation,
        Err(error) => {
            let _ = crate::world_authority::fail_browser_action(&admission, &error.to_string());
            return Err(isolated_browser_error(error));
        }
    };
    if let Err(initial_error) = crate::world_authority::record_browser_observation(
        &admission,
        observation.clone(),
    ) {
        if observation.full {
            let _ = crate::world_authority::fail_browser_action(&admission, &initial_error);
            return Err(isolated_browser_authority_error(initial_error));
        }
        observation = match state
            .isolated_browser
            .observe(&profile_id, &world_id, None, request.max_nodes)
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                let _ = crate::world_authority::fail_browser_action(
                    &admission,
                    &error.to_string(),
                );
                return Err(isolated_browser_error(error));
            }
        };
        if let Err(error) = crate::world_authority::record_browser_observation(
            &admission,
            observation.clone(),
        ) {
            let _ = crate::world_authority::fail_browser_action(&admission, &error);
            return Err(isolated_browser_authority_error(format!(
                "could not recover browser mirror after {initial_error}: {error}"
            )));
        }
    }
    crate::world_authority::complete_browser_action(
        &admission,
        "human isolated-browser observation committed",
    )
    .map_err(isolated_browser_authority_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "observation": observation,
    })))
}

pub async fn screenshot_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(world_id): Path<String>,
    Json(request): Json<IsolatedBrowserScreenshotRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let profile_id = principal_profile_id(&principal);
    let current = state
        .isolated_browser
        .get(&profile_id, &world_id)
        .await
        .map_err(isolated_browser_error)?;
    let tab_id = current.tab_id.as_deref().ok_or_else(|| {
        isolated_browser_error(IsolatedBrowserError::Unavailable(
            "isolated browser has no active page".to_string(),
        ))
    })?;
    crate::world_authority::validate_browser_pixel_fence(
        &current.authority_id,
        current.driver.driver_id.as_str(),
        &current.tab_group_id,
        tab_id,
        &current.url,
        &request.expected_document_id,
        request.expected_observation_revision,
    )
    .map_err(isolated_browser_authority_error)?;
    let owner_principal_id = format!("human:{profile_id}");
    let trace_id = isolated_browser_trace_id("screenshot");
    let admission = crate::world_authority::admit_owned_browser_human_intent(
        crate::world_authority::OwnedBrowserHumanIntent {
            authority_id: &current.authority_id,
            driver_id: current.driver.driver_id.as_str(),
            tab_group_id: &current.tab_group_id,
            tab_id,
            owner_principal_id: &owner_principal_id,
            trace_id: &trace_id,
            summary: "human captured isolated-browser pixels",
            effect_class: WorldEffectClass::ObservePixels,
        },
    )
    .map_err(isolated_browser_authority_error)?;
    let screenshot = match state
        .isolated_browser
        .screenshot(
            &profile_id,
            &world_id,
            &request.expected_document_id,
            request.expected_observation_revision,
            request.max_width,
        )
        .await
    {
        Ok(screenshot) => screenshot,
        Err(error) => {
            let _ = crate::world_authority::fail_browser_action(&admission, &error.to_string());
            return Err(isolated_browser_error(error));
        }
    };
    crate::world_authority::complete_browser_action(
        &admission,
        "human isolated-browser pixel observation committed",
    )
    .map_err(isolated_browser_authority_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "screenshot": screenshot,
    })))
}

pub async fn cleanup_isolated_browser_world(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(world_id): Path<String>,
    Query(query): Query<IsolatedBrowserCleanupQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let world = state
        .isolated_browser
        .cleanup(
            &principal_profile_id(&principal),
            &world_id,
            query.delete_profile,
        )
        .await
        .map_err(isolated_browser_error)?;
    Ok(Json(serde_json::json!({ "ok": true, "world": world })))
}

pub async fn register_client(
    State(state): State<AppState>,
    Json(request): Json<RegisterClientRequest>,
) -> Result<Json<RegisterClientResponse>, (StatusCode, String)> {
    let supports_browser_host = request.supports_browser_host;
    let registered_world_drivers = request
        .world_drivers
        .iter()
        .map(|driver| driver.driver_id.clone())
        .collect::<Vec<_>>();
    let registered_tools = state
        .client_registry
        .register(ClientRegistration {
            client_id: request.client_id,
            channel_surface: request.channel_surface,
            supports_browser_host,
            browser_host_url: request.browser_host_url,
            tools: request.tools,
            world_drivers: request.world_drivers,
            registered_at_utc: chrono::Utc::now(),
            last_seen_at_utc: chrono::Utc::now(),
        })
        .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
    let reachable = if supports_browser_host {
        browser_host_healthy().await
    } else {
        false
    };
    Ok(Json(RegisterClientResponse {
        ok: true,
        browser_host_reachable: reachable,
        registered_tools,
        registered_world_drivers,
    }))
}

pub async fn list_clients(State(state): State<AppState>) -> Json<Vec<ClientRegistration>> {
    Json(state.client_registry.list())
}

const DEFAULT_CLIENT_TOOL_WAIT_MS: u64 = 25_000;

#[derive(Debug, Deserialize)]
pub struct ClientToolPollQuery {
    #[serde(default = "default_client_tool_wait_ms")]
    pub wait_ms: u64,
}

fn default_client_tool_wait_ms() -> u64 {
    DEFAULT_CLIENT_TOOL_WAIT_MS
}

pub async fn next_client_tool_request(
    Path(client_id): Path<String>,
    Query(query): Query<ClientToolPollQuery>,
    State(state): State<AppState>,
) -> Result<Json<Option<ClientToolRequest>>, (StatusCode, String)> {
    let wait = Duration::from_millis(query.wait_ms.min(30_000));
    state
        .client_registry
        .next_tool_request(&client_id, wait)
        .await
        .map(Json)
        .map_err(|error| (StatusCode::NOT_FOUND, error))
}

pub async fn complete_client_tool_request(
    Path((client_id, request_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(request): Json<ClientToolResultRequest>,
) -> Json<ClientToolResultResponse> {
    let result = match (request.error, request.output) {
        (Some(error), _) => Err(error),
        (None, Some(output)) => Ok(output),
        (None, None) => Err("client tool response must include output or error".to_string()),
    };
    let accepted = state
        .client_registry
        .complete_tool_request(&client_id, &request_id, result);
    Json(ClientToolResultResponse {
        ok: accepted,
        accepted,
    })
}

#[derive(Debug, Deserialize)]
pub struct CompleteBrowserSessionRequest {
    #[serde(default)]
    pub world_driver_id: Option<String>,
    #[serde(default)]
    pub search_response: Option<SearchResponse>,
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn complete_browser_session_handler(
    Path(session_id): Path<String>,
    Json(request): Json<CompleteBrowserSessionRequest>,
) -> Json<serde_json::Value> {
    match complete_browser_session_for_driver(
        &session_id,
        request.world_driver_id.as_deref(),
        BrowserSessionCompleteRequest {
            search_response: request.search_response,
            error: request.error,
        },
    ) {
        Ok(Some(session)) => Json(serde_json::json!({
            "ok": true,
            "session_id": session.session_id,
            "status": session.status,
        })),
        Ok(None) => Json(serde_json::json!({
            "ok": false,
            "error": format!("session not found: {session_id}"),
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}

pub async fn get_browser_session_handler(
    Path(session_id): Path<String>,
) -> Json<serde_json::Value> {
    match get_browser_session(&session_id) {
        Some(session) => Json(serde_json::json!({ "ok": true, "session": session })),
        None => Json(serde_json::json!({
            "ok": false,
            "error": format!("session not found: {session_id}"),
        })),
    }
}

pub async fn resume_browser_session_handler(
    Path(session_id): Path<String>,
) -> Json<serde_json::Value> {
    let Some(session) = get_browser_session(&session_id) else {
        return Json(serde_json::json!({
            "ok": false,
            "error": format!("session not found: {session_id}"),
        }));
    };
    let query = session.query.trim();
    if query.is_empty() {
        return Json(serde_json::json!({
            "ok": false,
            "error": "browser session missing query",
        }));
    }
    match search_ddg_html_cached_async(query, session.max_results).await {
        Ok(search) => match complete_browser_session(
            &session_id,
            BrowserSessionCompleteRequest {
                search_response: Some(search.clone()),
                error: None,
            },
        ) {
            Some(updated) => Json(serde_json::json!({
                "ok": true,
                "session_id": updated.session_id,
                "status": updated.status,
                "search_response": search,
            })),
            None => Json(serde_json::json!({
                "ok": false,
                "error": format!("session not found: {session_id}"),
            })),
        },
        Err(err) => {
            let _ = complete_browser_session(
                &session_id,
                BrowserSessionCompleteRequest {
                    search_response: None,
                    error: Some(err.clone()),
                },
            );
            Json(serde_json::json!({ "ok": false, "error": err }))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CompleteBrowserActRequest {
    #[serde(default)]
    pub world_driver_id: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn complete_browser_act_handler(
    Path(session_id): Path<String>,
    Json(request): Json<CompleteBrowserActRequest>,
) -> Json<serde_json::Value> {
    match complete_browser_act_session_for_driver(
        &session_id,
        request.world_driver_id.as_deref(),
        BrowserActOutcome {
            ok: request.ok,
            url: request.url,
            error: request.error,
        },
    ) {
        Ok(Some(session)) => Json(serde_json::json!({
            "ok": true,
            "session_id": session.session_id,
            "status": session.status,
        })),
        Ok(None) => Json(serde_json::json!({
            "ok": false,
            "error": format!("session not found: {session_id}"),
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}

fn browser_routes() -> Router<AppState> {
    Router::new()
        .route("/clients/register", post(register_client))
        .route("/clients", get(list_clients))
        .route(
            "/clients/{client_id}/tools/next",
            get(next_client_tool_request),
        )
        .route(
            "/clients/{client_id}/tools/{request_id}/result",
            post(complete_client_tool_request),
        )
        .route(
            "/browser/sessions/{session_id}",
            get(get_browser_session_handler),
        )
        .route(
            "/browser/sessions/{session_id}/complete",
            post(complete_browser_session_handler),
        )
        .route(
            "/browser/sessions/{session_id}/complete-act",
            post(complete_browser_act_handler),
        )
        .route(
            "/browser/sessions/{session_id}/resume",
            post(resume_browser_session_handler),
        )
}

/// Canonical `/v1` copies of the H08 browser bridge. Unprefixed aliases stay
/// on [`browser_router`] as a reviewed dual-mount compatibility surface.
pub fn browser_surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .route(
            browser_write_policy(axum::http::Method::POST, "/v1/clients/register", 256 * 1024),
            post(register_client),
        )
        .route(browser_read_policy("/v1/clients"), get(list_clients))
        .route(
            browser_policy(
                axum::http::Method::GET,
                "/v1/clients/{client_id}/tools/next",
                Capability::WorkshopInteract,
                1024,
                RateLimitClass::Stream,
            ),
            get(next_client_tool_request),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/clients/{client_id}/tools/{request_id}/result",
                2 * 1024 * 1024,
            ),
            post(complete_client_tool_request),
        )
        .route(
            browser_read_policy("/v1/browser/sessions/{session_id}"),
            get(get_browser_session_handler),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/browser/sessions/{session_id}/complete",
                2 * 1024 * 1024,
            ),
            post(complete_browser_session_handler),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/browser/sessions/{session_id}/complete-act",
                64 * 1024,
            ),
            post(complete_browser_act_handler),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/browser/sessions/{session_id}/resume",
                16 * 1024,
            ),
            post(resume_browser_session_handler),
        )
        .route(
            browser_read_policy("/v1/browser/worlds/isolated"),
            get(list_isolated_browser_worlds),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/browser/worlds/isolated",
                32 * 1024,
            ),
            post(create_isolated_browser_world),
        )
        .route(
            browser_read_policy("/v1/browser/worlds/isolated/{world_id}"),
            get(get_isolated_browser_world),
        )
        .route(
            browser_write_policy(
                axum::http::Method::DELETE,
                "/v1/browser/worlds/isolated/{world_id}",
                1024,
            ),
            delete(cleanup_isolated_browser_world),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/browser/worlds/isolated/{world_id}/lifecycle",
                8 * 1024,
            ),
            post(control_isolated_browser_world),
        )
        .route(
            browser_write_policy(
                axum::http::Method::POST,
                "/v1/browser/worlds/isolated/{world_id}/navigate",
                16 * 1024,
            ),
            post(navigate_isolated_browser_world),
        )
        .route(
            browser_policy(
                axum::http::Method::POST,
                "/v1/browser/worlds/isolated/{world_id}/observe",
                Capability::WorkshopRead,
                8 * 1024,
                RateLimitClass::Read,
            ),
            post(observe_isolated_browser_world),
        )
        .route(
            browser_policy(
                axum::http::Method::POST,
                "/v1/browser/worlds/isolated/{world_id}/screenshot",
                Capability::WorkshopRead,
                8 * 1024,
                RateLimitClass::Read,
            ),
            post(screenshot_isolated_browser_world),
        )
}

fn browser_read_policy(path: &'static str) -> RoutePolicy {
    browser_policy(
        axum::http::Method::GET,
        path,
        Capability::WorkshopRead,
        1024,
        RateLimitClass::Read,
    )
}

fn browser_write_policy(
    method: axum::http::Method,
    path: &'static str,
    body_limit: usize,
) -> RoutePolicy {
    browser_policy(
        method,
        path,
        Capability::WorkshopInteract,
        body_limit,
        RateLimitClass::Mutation,
    )
}

fn browser_policy(
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

/// Unprefixed H08 aliases. Canonical `/v1` copies live on [`browser_surface`].
pub fn browser_router() -> Router<AppState> {
    // Keep `crate::daemon::contract::BROWSER_COMPATIBILITY_MOUNTS` in lockstep.
    browser_routes()
}
