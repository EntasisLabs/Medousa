//! Daemon-side host for the LSP Interoperability Orchestrator (`medousa-code`).
//!
//! Discovers / lazily spawns the coding engine beside the workshop and advertises
//! its URL to Home. Path policy: engine always sees workshop disk.
//! Home connects via daemon `/v1/code/lsp` (proxied) so remote Connection works.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

use crate::daemon::state::AppState;
use crate::grapheme_script::store::GraphemeScriptStore;
use crate::paths::medousa_data_dir;

const DEFAULT_BIND: &str = "127.0.0.1:7861";

#[derive(Debug, Default)]
pub struct CodingEngineHost {
    child: Mutex<Option<Child>>,
}

impl CodingEngineHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CodingEngineInfo {
    pub available: bool,
    pub url: String,
    pub health_url: String,
    /// Direct orchestrator WS (local workshop only). Prefer `daemon_lsp_path`.
    pub lsp_url: String,
    /// Path on the daemon to proxy LSP WebSocket (works for remote Home).
    pub daemon_lsp_path: String,
    pub workspace_root: String,
    pub workspace_root_uri: String,
    pub bind: String,
    pub message: String,
}

fn engine_bind() -> String {
    std::env::var("MEDOUSA_CODE_BIND").unwrap_or_else(|_| DEFAULT_BIND.into())
}

fn engine_http_base(bind: &str) -> String {
    format!("http://{bind}")
}

fn path_to_file_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn resolve_engine_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("MEDOUSA_CODE_BIN") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    let data_bin = medousa_data_dir().join("bin").join(binary_name());
    if data_bin.is_file() {
        return Some(data_bin);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(binary_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    which_bin(binary_name())
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "medousa-code.exe"
    } else {
        "medousa-code"
    }
}

fn which_bin(name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("/usr/local/bin").join(name));
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
    }
    if cfg!(target_os = "linux") {
        candidates.push(PathBuf::from("/usr/local/bin").join(name));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn forge_worktree_roots() -> Vec<PathBuf> {
    let forge_root = medousa_data_dir().join("forge").join("worktrees");
    let mut roots = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&forge_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Ok(inner) = std::fs::read_dir(&path)
            {
                for work in inner.flatten() {
                    let wp = work.path();
                    if wp.is_dir() {
                        roots.push(wp);
                    }
                }
            }
        }
    }
    roots
}

async fn probe_health(health_url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400))
        .build()
    else {
        return false;
    };
    client
        .get(health_url)
        .send()
        .await
        .ok()
        .is_some_and(|r| r.status().is_success())
}

/// Ensure the coding engine is running; return connection info.
pub async fn ensure_coding_engine(host: &CodingEngineHost) -> CodingEngineInfo {
    let bind = engine_bind();
    let base = engine_http_base(&bind);
    let health_url = format!("{base}/health");
    let lsp_url = format!("ws://{bind}/v1/lsp");
    let workspace_root = GraphemeScriptStore::root_dir();
    let workspace_str = workspace_root.to_string_lossy().into_owned();
    let workspace_root_uri = path_to_file_uri(&workspace_root);

    let info = |available: bool, message: String| CodingEngineInfo {
        available,
        url: base.clone(),
        health_url: health_url.clone(),
        lsp_url: lsp_url.clone(),
        daemon_lsp_path: "/v1/code/lsp".into(),
        workspace_root: workspace_str.clone(),
        workspace_root_uri: workspace_root_uri.clone(),
        bind: bind.clone(),
        message,
    };

    if probe_health(&health_url).await {
        return info(true, "coding engine reachable".into());
    }

    let Some(bin) = resolve_engine_binary() else {
        return info(
            false,
            "medousa-code binary not found — install coding-engine from Settings → Packages or build crates/medousa-code".into(),
        );
    };

    {
        let mut guard = host.child.lock().await;
        if guard.is_none() {
            let mut cmd = Command::new(&bin);
            cmd.arg("--bind")
                .arg(&bind)
                .arg("--workspace")
                .arg(&workspace_root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            for root in forge_worktree_roots() {
                cmd.arg("--allow-root").arg(root);
            }
            match cmd.spawn() {
                Ok(child) => {
                    *guard = Some(child);
                    tracing::info!(binary = %bin.display(), %bind, "spawned medousa-code");
                }
                Err(err) => {
                    return info(false, format!("failed to spawn medousa-code: {err}"));
                }
            }
        }
    }

    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if probe_health(&health_url).await {
            return info(true, "coding engine started".into());
        }
    }

    info(
        false,
        "medousa-code spawned but health check timed out".into(),
    )
}

pub fn coding_engine_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/coding-engine", get(coding_engine_info))
        .route("/v1/code/lsp", get(code_lsp_ws))
        .route("/v1/code/hover", get(code_hover))
        .route("/v1/code/definition", get(code_definition))
        .route("/v1/code/diagnostics", get(code_diagnostics))
        .route("/v1/code/symbols", get(code_symbols))
        .with_state(state)
}

pub async fn coding_engine_info(State(state): State<AppState>) -> Json<CodingEngineInfo> {
    let host = state
        .coding_engine
        .clone()
        .unwrap_or_default();
    Json(ensure_coding_engine(&host).await)
}

#[derive(Debug, serde::Deserialize)]
pub struct CodeLspQuery {
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "grapheme".into()
}

pub async fn code_lsp_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<CodeLspQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| proxy_lsp_socket(socket, state, q.language))
}

async fn proxy_lsp_socket(client: WebSocket, state: AppState, language: String) {
    let host = state
        .coding_engine
        .clone()
        .unwrap_or_default();
    let info = ensure_coding_engine(&host).await;
    if !info.available {
        tracing::warn!(message = %info.message, "coding engine unavailable for LSP proxy");
        return;
    }
    let upstream = format!(
        "{}/v1/lsp?language={}",
        info.lsp_url.trim_end_matches('/'),
        urlencoding::encode(&language)
    );
    // lsp_url is already ws://host/v1/lsp — avoid double path
    let upstream = if info.lsp_url.contains("/v1/lsp") {
        format!(
            "{}?language={}",
            info.lsp_url,
            urlencoding::encode(&language)
        )
    } else {
        upstream
    };

    let Ok((upstream_ws, _)) = connect_async(&upstream).await else {
        tracing::warn!(%upstream, "failed to connect to medousa-code LSP");
        return;
    };

    let (mut up_tx, mut up_rx) = upstream_ws.split();
    let (mut client_tx, mut client_rx) = client.split();

    let client_to_up = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_rx.next().await {
            let out = match msg {
                AxumMessage::Text(t) => TungsteniteMessage::Text(t.to_string().into()),
                AxumMessage::Binary(b) => TungsteniteMessage::Binary(b),
                AxumMessage::Ping(p) => TungsteniteMessage::Ping(p),
                AxumMessage::Pong(p) => TungsteniteMessage::Pong(p),
                AxumMessage::Close(_) => {
                    let _ = up_tx.close().await;
                    break;
                }
            };
            if up_tx.send(out).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = up_rx.next().await {
        let out = match msg {
            TungsteniteMessage::Text(t) => AxumMessage::Text(t.to_string().into()),
            TungsteniteMessage::Binary(b) => AxumMessage::Binary(b),
            TungsteniteMessage::Ping(p) => AxumMessage::Ping(p),
            TungsteniteMessage::Pong(p) => AxumMessage::Pong(p),
            TungsteniteMessage::Close(_) => break,
            TungsteniteMessage::Frame(_) => continue,
        };
        if client_tx.send(out).await.is_err() {
            break;
        }
    }
    client_to_up.abort();
}

pub async fn code_hover(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/hover", &q).await
}

pub async fn code_definition(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/definition", &q).await
}

pub async fn code_diagnostics(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/diagnostics", &q).await
}

pub async fn code_symbols(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/symbols", &q).await
}

async fn proxy_agent_get(
    state: &AppState,
    path: &str,
    q: &std::collections::HashMap<String, String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let host = state
        .coding_engine
        .clone()
        .unwrap_or_default();
    let info = ensure_coding_engine(&host).await;
    if !info.available {
        return Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            info.message,
        ));
    }
    let mut url = reqwest::Url::parse(&format!("{}{path}", info.url))
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in q {
            pairs.append_pair(k, v);
        }
    }
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let status = resp.status();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    if !status.is_success() {
        return Err((
            axum::http::StatusCode::BAD_GATEWAY,
            format!("coding engine returned {status}"),
        ));
    }
    Ok(Json(body))
}

#[allow(dead_code)]
pub fn parse_bind(bind: &str) -> Option<SocketAddr> {
    bind.parse().ok()
}

#[allow(dead_code)]
pub fn workspace_is_under(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}
