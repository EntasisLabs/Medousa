//! Daemon HTTP handlers for Agent Browser sessions and client registration.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_browser_lite::SearchResponse;
use serde::{Deserialize, Serialize};

use crate::browser_host_client::browser_host_healthy;
use medousa_browser_lite::search_ddg_html_cached_async;

use crate::browser_sessions::{
    complete_browser_session, get_browser_session, BrowserSessionCompleteRequest,
};
use crate::daemon::state::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    pub channel_surface: String,
    pub supports_browser_host: bool,
    #[serde(default)]
    pub browser_host_url: Option<String>,
    pub registered_at_utc: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientRegistry {
    inner: Arc<Mutex<Vec<ClientRegistration>>>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn register(&self, registration: ClientRegistration) {
        let mut guard = self.inner.lock().expect("client registry");
        guard.retain(|entry| entry.client_id != registration.client_id);
        guard.push(registration);
    }

    pub fn list(&self) -> Vec<ClientRegistration> {
        self.inner.lock().expect("client registry").clone()
    }

    pub fn browser_host_available(&self) -> bool {
        self.inner
            .lock()
            .expect("client registry")
            .iter()
            .any(|entry| entry.supports_browser_host)
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterClientRequest {
    pub client_id: String,
    pub channel_surface: String,
    pub supports_browser_host: bool,
    #[serde(default)]
    pub browser_host_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterClientResponse {
    pub ok: bool,
    pub browser_host_reachable: bool,
}

pub async fn register_client(
    State(state): State<AppState>,
    Json(request): Json<RegisterClientRequest>,
) -> Json<RegisterClientResponse> {
    state.client_registry.register(ClientRegistration {
        client_id: request.client_id,
        channel_surface: request.channel_surface,
        supports_browser_host: request.supports_browser_host,
        browser_host_url: request.browser_host_url,
        registered_at_utc: chrono::Utc::now(),
    });
    let reachable = if request.supports_browser_host {
        browser_host_healthy().await
    } else {
        false
    };
    Json(RegisterClientResponse {
        ok: true,
        browser_host_reachable: reachable,
    })
}

pub async fn list_clients(State(state): State<AppState>) -> Json<Vec<ClientRegistration>> {
    Json(state.client_registry.list())
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

fn browser_routes() -> Router<AppState> {
    Router::new()
        .route("/clients/register", post(register_client))
        .route("/clients", get(list_clients))
        .route(
            "/browser/sessions/{session_id}",
            get(get_browser_session_handler),
        )
        .route(
            "/browser/sessions/{session_id}/complete",
            post(complete_browser_session_handler),
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
