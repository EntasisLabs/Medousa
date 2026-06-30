//! In-process BrowserHost HTTP service on `127.0.0.1:7422` (desktop only).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use medousa_browser_lite::{fetch_url_markdown, search_ddg_html_cached, SearchResponse};
use medousa_browser_bridge::{
    BrowserControl, BrowserSnapshot, TabGroup, TabGroupManager, TabOpenedBy,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
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
    if let Some(app) = crate::human_browser::app_handle() {
        if crate::human_browser::urls_match_for_snapshot(
            &crate::human_browser::human_browser_active_url(),
            &url,
        ) {
            if let Ok(fetched) =
                crate::human_browser::snapshot_markdown_for_url(&app, &url, max_chars).await
            {
                return Ok(Json(serde_json::json!({
                    "url": fetched.url,
                    "title": fetched.title,
                    "markdown": fetched.markdown,
                    "binding_used": "human_webview",
                })));
            }
        }
    }
    let fetched = tokio::task::spawn_blocking(move || fetch_url_markdown(&url, max_chars))
        .await
        .map_err(|err| err.to_string())??;
    Ok(Json(serde_json::json!({
        "url": fetched.url,
        "title": fetched.title,
        "markdown": fetched.markdown,
        "binding_used": "browser_host_lite",
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

#[derive(Debug, Deserialize)]
struct TabGroupCreateRequest {
    #[serde(default)]
    chat_session_id: Option<String>,
    #[serde(default)]
    work_card_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TabOpenRequest {
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    opened_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TabNavigateRequest {
    url: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    opened_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TabControlRequest {
    control: String,
}

#[derive(Debug, Deserialize)]
struct TabSnapshotRequest {
    #[serde(default = "default_max_chars")]
    max_chars: usize,
}

fn parse_opened_by(value: Option<&str>) -> TabOpenedBy {
    match value.unwrap_or("user").trim().to_lowercase().as_str() {
        "agent" => TabOpenedBy::Agent,
        _ => TabOpenedBy::User,
    }
}

fn parse_control(value: &str) -> BrowserControl {
    match value.trim().to_lowercase().as_str() {
        "agent" => BrowserControl::Agent,
        "awaiting_operator" => BrowserControl::AwaitingOperator,
        _ => BrowserControl::User,
    }
}

async fn create_tab_group(Json(request): Json<TabGroupCreateRequest>) -> Json<TabGroup> {
    Json(TabGroupManager::create_group(
        request.chat_session_id,
        request.work_card_id,
    ))
}

async fn get_tab_group(Path(tab_group_id): Path<String>) -> Json<serde_json::Value> {
    match TabGroupManager::get_group(&tab_group_id) {
        Some(group) => Json(serde_json::json!({ "ok": true, "tab_group": group })),
        None => Json(serde_json::json!({
            "ok": false,
            "error": format!("tab group not found: {tab_group_id}"),
        })),
    }
}

async fn open_tab(
    Path(tab_group_id): Path<String>,
    Json(request): Json<TabOpenRequest>,
) -> Json<serde_json::Value> {
    match TabGroupManager::open_tab(
        &tab_group_id,
        &request.url,
        request.title.as_deref(),
        parse_opened_by(request.opened_by.as_deref()),
    ) {
        Some(tab) => {
            let group = TabGroupManager::get_group(&tab_group_id);
            Json(serde_json::json!({ "ok": true, "tab": tab, "tab_group": group }))
        }
        None => Json(serde_json::json!({ "ok": false, "error": "tab group not found" })),
    }
}

async fn navigate_tab(
    Path(tab_group_id): Path<String>,
    Json(request): Json<TabNavigateRequest>,
) -> Json<serde_json::Value> {
    TabGroupManager::ensure_group(&tab_group_id);
    match TabGroupManager::navigate_active_tab(
        &tab_group_id,
        &request.url,
        request.title.as_deref(),
        parse_opened_by(request.opened_by.as_deref()),
    ) {
        Some(tab) => {
            let group = TabGroupManager::get_group(&tab_group_id);
            Json(serde_json::json!({ "ok": true, "tab": tab, "tab_group": group }))
        }
        None => Json(serde_json::json!({ "ok": false, "error": "navigation failed" })),
    }
}

async fn activate_tab(
    Path((tab_group_id, tab_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    match TabGroupManager::activate_tab(&tab_group_id, &tab_id) {
        Some(group) => Json(serde_json::json!({ "ok": true, "tab_group": group })),
        None => Json(serde_json::json!({ "ok": false, "error": "tab not found" })),
    }
}

async fn close_tab(
    Path((tab_group_id, tab_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    match TabGroupManager::close_tab(&tab_group_id, &tab_id) {
        Some(group) => Json(serde_json::json!({ "ok": true, "tab_group": group })),
        None => Json(serde_json::json!({ "ok": false, "error": "tab group not found" })),
    }
}

async fn set_tab_group_control(
    Path(tab_group_id): Path<String>,
    Json(request): Json<TabControlRequest>,
) -> Json<serde_json::Value> {
    match TabGroupManager::set_control(&tab_group_id, parse_control(&request.control)) {
        Some(group) => Json(serde_json::json!({ "ok": true, "tab_group": group })),
        None => Json(serde_json::json!({ "ok": false, "error": "tab group not found" })),
    }
}

#[derive(Debug, Deserialize)]
struct LinkWorkCardRequest {
    work_card_id: Option<String>,
}

async fn link_work_card_handler(
    Path(tab_group_id): Path<String>,
    Json(request): Json<LinkWorkCardRequest>,
) -> Json<serde_json::Value> {
    match TabGroupManager::link_work_card(
        &tab_group_id,
        request.work_card_id.as_deref(),
    ) {
        Some(group) => Json(serde_json::json!({ "ok": true, "tab_group": group })),
        None => Json(serde_json::json!({ "ok": false, "error": "tab group not found" })),
    }
}

async fn snapshot_tab_group(
    Path(tab_group_id): Path<String>,
    Json(request): Json<TabSnapshotRequest>,
) -> Result<Json<BrowserSnapshot>, String> {
    if let Some(app) = crate::human_browser::app_handle() {
        if let Some(group) = TabGroupManager::get_group(&tab_group_id) {
            if let Some(tab) = group.tabs.iter().find(|tab| tab.active) {
                if crate::human_browser::urls_match_for_snapshot(
                    &crate::human_browser::human_browser_active_url(),
                    &tab.url,
                ) {
                    if let Ok(fetched) = crate::human_browser::snapshot_markdown_for_url(
                        &app,
                        &tab.url,
                        request.max_chars,
                    )
                    .await
                    {
                        return Ok(Json(BrowserSnapshot {
                            tab_id: tab.id.clone(),
                            url: fetched.url,
                            title: fetched.title,
                            markdown: fetched.markdown,
                            links: Vec::new(),
                        }));
                    }
                }
            }
        }
    }
    let snapshot =
        TabGroupManager::snapshot_active_tab(&tab_group_id, request.max_chars)?;
    Ok(Json(snapshot))
}

fn build_router(state: BrowserHostState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/search", post(search))
        .route("/v1/fetch", post(fetch))
        .route("/v1/sessions/{session_id}/resume", post(resume_session))
        .route("/v1/tab-groups", post(create_tab_group))
        .route("/v1/tab-groups/{tab_group_id}", get(get_tab_group))
        .route("/v1/tab-groups/{tab_group_id}/tabs", post(open_tab))
        .route("/v1/tab-groups/{tab_group_id}/navigate", post(navigate_tab))
        .route(
            "/v1/tab-groups/{tab_group_id}/tabs/{tab_id}/activate",
            post(activate_tab),
        )
        .route(
            "/v1/tab-groups/{tab_group_id}/tabs/{tab_id}",
            axum::routing::delete(close_tab),
        )
        .route("/v1/tab-groups/{tab_group_id}/control", post(set_tab_group_control))
        .route(
            "/v1/tab-groups/{tab_group_id}/link-work",
            post(link_work_card_handler),
        )
        .route("/v1/tab-groups/{tab_group_id}/snapshot", post(snapshot_tab_group))
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
pub async fn browser_host_search(
    query: String,
    max_results: Option<usize>,
) -> Result<SearchResponse, String> {
    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Err("empty query".to_string());
    }
    let limit = max_results.unwrap_or(5).clamp(1, 10);
    tokio::task::spawn_blocking(move || search_ddg_html_cached(&trimmed, limit))
        .await
        .map_err(|err| err.to_string())?
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
pub async fn browser_host_resume_session(
    session_id: String,
    daemon_url: Option<String>,
) -> Result<serde_json::Value, String> {
    let session_id = session_id.trim().to_string();
    let session = fetch_daemon_browser_session(&session_id, daemon_url.as_deref()).await?;
    let query = session
        .get("query")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if query.is_empty() {
        return Err("browser session missing query".to_string());
    }
    let max_results = session
        .get("max_results")
        .and_then(|value| value.as_u64())
        .unwrap_or(8) as usize;

    let search = tokio::task::spawn_blocking(move || {
        medousa_browser_lite::search_ddg_html_cached(&query, max_results)
    })
    .await
    .map_err(|err| err.to_string())??;

    if let Some(base) = daemon_url {
        complete_daemon_browser_session(&base, &session_id, &search).await?;
    }

    Ok(serde_json::json!({
        "ok": true,
        "session_id": session_id,
        "search_response": search,
    }))
}

async fn fetch_daemon_browser_session(
    session_id: &str,
    daemon_url: Option<&str>,
) -> Result<serde_json::Value, String> {
    let base = resolve_daemon_url(daemon_url)?;
    let url = format!(
        "{}/v1/browser/sessions/{}",
        base.trim_end_matches('/'),
        session_id
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let body: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    body.get("session")
        .cloned()
        .ok_or_else(|| "daemon session response missing session".to_string())
}

async fn complete_daemon_browser_session(
    daemon_base: &str,
    session_id: &str,
    search: &SearchResponse,
) -> Result<(), String> {
    let url = format!(
        "{}/v1/browser/sessions/{}/complete",
        daemon_base.trim_end_matches('/'),
        session_id
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(url)
        .json(&serde_json::json!({
            "search_response": search,
            "error": null,
        }))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("daemon complete failed: {}", response.status()));
    }
    Ok(())
}

fn resolve_daemon_url(daemon_url: Option<&str>) -> Result<String, String> {
    if let Some(url) = daemon_url.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(url.trim_end_matches('/').to_string());
    }
    std::env::var("MEDOUSA_DAEMON_URL")
        .or_else(|_| std::env::var("STASIS_DAEMON_URL"))
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .map_err(|_| "MEDOUSA_DAEMON_URL not set".to_string())
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

#[tauri::command]
pub async fn browser_host_register_client(
    daemon_url: String,
    channel_surface: String,
) -> Result<(), String> {
    register_browser_client_with_daemon(&daemon_url, &channel_surface).await;
    Ok(())
}

// ── Browser bridge (in-process; avoids CORS from Vite dev → :7422) ───────────

fn emit_browser_context_updated(app: &AppHandle, tab_group_id: &str) {
    let _ = app.emit("browser-context-updated", tab_group_id.to_string());
}

#[tauri::command]
pub fn browser_bridge_create_tab_group(
    app: AppHandle,
    chat_session_id: Option<String>,
    work_card_id: Option<String>,
) -> Result<TabGroup, String> {
    let group = TabGroupManager::create_group(chat_session_id, work_card_id);
    emit_browser_context_updated(&app, &group.id);
    Ok(group)
}

#[tauri::command]
pub fn browser_bridge_get_tab_group(tab_group_id: String) -> Result<Option<TabGroup>, String> {
    Ok(TabGroupManager::get_group(&tab_group_id))
}

#[tauri::command]
pub fn browser_bridge_open_tab(
    app: AppHandle,
    tab_group_id: String,
    url: String,
    opened_by: Option<String>,
    title: Option<String>,
) -> Result<TabGroup, String> {
    TabGroupManager::open_tab(
        &tab_group_id,
        &url,
        title.as_deref(),
        parse_opened_by(opened_by.as_deref()),
    )
    .and_then(|_| TabGroupManager::get_group(&tab_group_id))
    .map(|group| {
        emit_browser_context_updated(&app, &tab_group_id);
        group
    })
    .ok_or_else(|| "tab group not found".to_string())
}

#[tauri::command]
pub fn browser_bridge_navigate_tab(
    app: AppHandle,
    tab_group_id: String,
    url: String,
    opened_by: Option<String>,
    title: Option<String>,
) -> Result<TabGroup, String> {
    TabGroupManager::ensure_group(&tab_group_id);
    TabGroupManager::navigate_active_tab(
        &tab_group_id,
        &url,
        title.as_deref(),
        parse_opened_by(opened_by.as_deref()),
    )
    .and_then(|_| TabGroupManager::get_group(&tab_group_id))
    .map(|group| {
        emit_browser_context_updated(&app, &tab_group_id);
        group
    })
    .ok_or_else(|| "navigation failed".to_string())
}

#[tauri::command]
pub fn browser_bridge_activate_tab(
    app: AppHandle,
    tab_group_id: String,
    tab_id: String,
) -> Result<TabGroup, String> {
    let group = TabGroupManager::activate_tab(&tab_group_id, &tab_id)
        .ok_or_else(|| "tab not found".to_string())?;
    emit_browser_context_updated(&app, &tab_group_id);
    Ok(group)
}

#[tauri::command]
pub fn browser_bridge_close_tab(
    app: AppHandle,
    tab_group_id: String,
    tab_id: String,
) -> Result<TabGroup, String> {
    let group = TabGroupManager::close_tab(&tab_group_id, &tab_id)
        .ok_or_else(|| "tab group not found".to_string())?;
    emit_browser_context_updated(&app, &tab_group_id);
    Ok(group)
}

#[tauri::command]
pub fn browser_bridge_set_control(
    app: AppHandle,
    tab_group_id: String,
    control: String,
) -> Result<TabGroup, String> {
    let group = TabGroupManager::set_control(&tab_group_id, parse_control(&control))
        .ok_or_else(|| "tab group not found".to_string())?;
    emit_browser_context_updated(&app, &tab_group_id);
    Ok(group)
}

#[tauri::command]
pub fn browser_bridge_link_work_card(
    app: AppHandle,
    tab_group_id: String,
    work_card_id: Option<String>,
) -> Result<TabGroup, String> {
    let group = TabGroupManager::link_work_card(&tab_group_id, work_card_id.as_deref())
        .ok_or_else(|| "tab group not found".to_string())?;
    emit_browser_context_updated(&app, &tab_group_id);
    Ok(group)
}

#[tauri::command]
pub async fn browser_bridge_snapshot(
    app: AppHandle,
    tab_group_id: String,
    max_chars: Option<usize>,
) -> Result<BrowserSnapshot, String> {
    let max_chars = max_chars.unwrap_or(4000);
    if let Some(group) = TabGroupManager::get_group(&tab_group_id) {
        if let Some(tab) = group.tabs.iter().find(|tab| tab.active) {
            if crate::human_browser::urls_match_for_snapshot(
                &crate::human_browser::human_browser_active_url(),
                &tab.url,
            ) {
                if let Ok(fetched) =
                    crate::human_browser::snapshot_markdown_for_url(&app, &tab.url, max_chars).await
                {
                    return Ok(BrowserSnapshot {
                        tab_id: tab.id.clone(),
                        url: fetched.url,
                        title: fetched.title,
                        markdown: fetched.markdown,
                        links: Vec::new(),
                    });
                }
            }
        }
    }
    tokio::task::spawn_blocking(move || TabGroupManager::snapshot_active_tab(&tab_group_id, max_chars))
        .await
        .map_err(|err| err.to_string())?
}
