//! In-process BrowserHost HTTP service on `127.0.0.1:7422` (desktop only).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_browser_lite::{fetch_url_markdown, search_ddg_html_cached, SearchResponse};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

const DEFAULT_BIND: &str = "127.0.0.1:7422";

#[derive(Clone, Default)]
struct BrowserHostState {
    sessions: Arc<Mutex<std::collections::HashMap<String, BrowserSessionRecord>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserSessionRecord {
    session_id: String,
    challenge_url: Option<String>,
    resumed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    ok: bool,
    version: &'static str,
}

#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: String,
    #[serde(default = "default_max_results")]
    max_results: usize,
}

#[derive(Debug, Deserialize)]
struct FetchRequest {
    url: String,
    #[serde(default = "default_max_chars")]
    max_chars: usize,
}

#[derive(Debug, Deserialize)]
struct ResumeRequest {
    #[serde(default)]
    operator_message: Option<String>,
}

fn default_max_results() -> usize {
    8
}

fn default_max_chars() -> usize {
    4000
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);

fn resolve_bind() -> String {
    std::env::var("MEDOUSA_BROWSER_HOST_BIND")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BIND.to_string())
}

pub fn browser_host_base_url() -> String {
    std::env::var("MEDOUSA_BROWSER_HOST_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("http://{}", resolve_bind()))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn search(Json(request): Json<SearchRequest>) -> Result<Json<SearchResponse>, String> {
    let response = tokio::task::spawn_blocking(move || {
        search_ddg_html_cached(&request.query, request.max_results)
    })
    .await
    .map_err(|err| err.to_string())??;
    Ok(Json(response))
}

async fn fetch(Json(request): Json<FetchRequest>) -> Result<Json<serde_json::Value>, String> {
    let url = request.url.clone();
    let max_chars = request.max_chars;
    let fetched = tokio::task::spawn_blocking(move || fetch_url_markdown(&url, max_chars))
        .await
        .map_err(|err| err.to_string())??;
    Ok(Json(serde_json::json!({
        "url": fetched.url,
        "title": fetched.title,
        "markdown": fetched.markdown,
    })))
}

async fn resume_session(
    State(state): State<BrowserHostState>,
    Path(session_id): Path<String>,
    Json(request): Json<ResumeRequest>,
) -> Json<serde_json::Value> {
    let mut guard = state.sessions.lock().expect("browser sessions");
    if let Some(session) = guard.get_mut(&session_id) {
        session.resumed = true;
        Json(serde_json::json!({
            "ok": true,
            "session_id": session_id,
            "operator_message": request.operator_message,
        }))
    } else {
        guard.insert(
            session_id.clone(),
            BrowserSessionRecord {
                session_id: session_id.clone(),
                challenge_url: None,
                resumed: true,
            },
        );
        Json(serde_json::json!({ "ok": true, "session_id": session_id }))
    }
}

fn build_router(state: BrowserHostState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/search", post(search))
        .route("/v1/fetch", post(fetch))
        .route("/v1/sessions/{session_id}/resume", post(resume_session))
        .with_state(state)
}

pub async fn browser_host_http_healthy() -> bool {
    let url = format!("{}/health", browser_host_base_url());
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    client
        .get(url)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub fn start_browser_host_background() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let bind = resolve_bind();
    let state = BrowserHostState::default();
    let router = build_router(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    if let Ok(mut guard) = SHUTDOWN.lock() {
        *guard = Some(shutdown_tx);
    }
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("browser host runtime");
        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::bind(&bind).await {
                Ok(listener) => listener,
                Err(err) => {
                    eprintln!("browser host bind failed ({bind}): {err}");
                    RUNNING.store(false, Ordering::SeqCst);
                    return;
                }
            };
            eprintln!("browser host listening on http://{bind}");
            let server = axum::serve(listener, router);
            tokio::select! {
                result = server => {
                    if let Err(err) = result {
                        eprintln!("browser host server error: {err}");
                    }
                }
                _ = shutdown_rx => {}
            }
            RUNNING.store(false, Ordering::SeqCst);
        });
    });
}

pub fn stop_browser_host() {
    if let Ok(mut guard) = SHUTDOWN.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
    RUNNING.store(false, Ordering::SeqCst);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostStatusDto {
    pub running: bool,
    pub healthy: bool,
    pub base_url: String,
}

#[tauri::command]
pub async fn browser_host_status() -> Result<BrowserHostStatusDto, String> {
    let healthy = browser_host_http_healthy().await;
    Ok(BrowserHostStatusDto {
        running: RUNNING.load(Ordering::SeqCst) || healthy,
        healthy,
        base_url: browser_host_base_url(),
    })
}

#[tauri::command]
pub async fn browser_host_restart() -> Result<BrowserHostStatusDto, String> {
    stop_browser_host();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    start_browser_host_background();
    for _ in 0..20 {
        if browser_host_http_healthy().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    browser_host_status().await
}

#[tauri::command]
pub async fn browser_host_resume_session(session_id: String) -> Result<serde_json::Value, String> {
    let url = format!(
        "{}/v1/sessions/{}/resume",
        browser_host_base_url(),
        session_id.trim()
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(url)
        .json(&ResumeRequest {
            operator_message: None,
        })
        .send()
        .await
        .map_err(|err| err.to_string())?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.to_string())
}

pub async fn register_browser_client_with_daemon(daemon_url: &str, channel_surface: &str) {
    let supports = if channel_surface.starts_with("home-ios")
        || channel_surface.starts_with("home-android")
    {
        true
    } else {
        browser_host_http_healthy().await
    };
    let client_id = format!("home-{channel_surface}");
    let body = serde_json::json!({
        "client_id": client_id,
        "channel_surface": channel_surface,
        "supports_browser_host": supports,
        "browser_host_url": if supports && !channel_surface.starts_with("home-ios") {
            Some(browser_host_base_url())
        } else {
            None::<String>
        },
    });
    let url = format!("{}/v1/clients/register", daemon_url.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    else {
        return;
    };
    let _ = client.post(url).json(&body).send().await;
}
