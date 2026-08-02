//! Daemon HTTP handlers for Agent Browser sessions and registered client tools.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_browser_lite::SearchResponse;
use serde::Deserialize;

use crate::browser_host_client::browser_host_healthy;
use medousa_browser_lite::search_ddg_html_cached_async;

pub use crate::client_tools::{
    ClientRegistration, ClientRegistry, ClientToolDefinition, ClientToolRequest,
    ClientToolResultRequest, ClientToolResultResponse, RegisterClientRequest,
    RegisterClientResponse,
};

use crate::browser_sessions::{
    complete_browser_act_session, complete_browser_session, get_browser_session,
    BrowserActOutcome, BrowserSessionCompleteRequest,
};
use crate::daemon::state::AppState;

pub async fn register_client(
    State(state): State<AppState>,
    Json(request): Json<RegisterClientRequest>,
) -> Result<Json<RegisterClientResponse>, (StatusCode, String)> {
    let supports_browser_host = request.supports_browser_host;
    let registered_tools = state
        .client_registry
        .register(ClientRegistration {
            client_id: request.client_id,
            channel_surface: request.channel_surface,
            supports_browser_host,
            browser_host_url: request.browser_host_url,
            tools: request.tools,
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
    pub search_response: Option<SearchResponse>,
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn complete_browser_session_handler(
    Path(session_id): Path<String>,
    Json(request): Json<CompleteBrowserSessionRequest>,
) -> Json<serde_json::Value> {
    match complete_browser_session(
        &session_id,
        BrowserSessionCompleteRequest {
            search_response: request.search_response,
            error: request.error,
        },
    ) {
        Some(session) => Json(serde_json::json!({
            "ok": true,
            "session_id": session.session_id,
            "status": session.status,
        })),
        None => Json(serde_json::json!({
            "ok": false,
            "error": format!("session not found: {session_id}"),
        })),
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
    match complete_browser_act_session(
        &session_id,
        BrowserActOutcome {
            ok: request.ok,
            url: request.url,
            error: request.error,
        },
    ) {
        Some(session) => Json(serde_json::json!({
            "ok": true,
            "session_id": session.session_id,
            "status": session.status,
        })),
        None => Json(serde_json::json!({
            "ok": false,
            "error": format!("session not found: {session_id}"),
        })),
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

pub fn browser_router() -> Router<AppState> {
    let routes = browser_routes();
    Router::new().merge(routes.clone()).nest("/v1", routes)
}
