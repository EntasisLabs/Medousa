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
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

use crate::daemon::state::AppState;
use crate::grapheme_script::store::GraphemeScriptStore;
use crate::paths::medousa_data_dir;

const DEFAULT_BIND: &str = "127.0.0.1:7861";
const EXPECTED_API_REVISION: u32 = 1;

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
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(binary_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let data_bin = medousa_data_dir().join("bin").join(binary_name());
    if data_bin.is_file() {
        return Some(data_bin);
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
    vec![forge_root]
}

#[derive(Debug, Deserialize)]
struct EngineHealth {
    name: String,
    api_revision: Option<u32>,
    allowed_roots: Vec<PathBuf>,
}

enum HealthProbe {
    Compatible,
    Unreachable,
    Incompatible(String),
}

async fn probe_health(health_url: &str, required_roots: &[PathBuf]) -> HealthProbe {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400))
        .build()
    else {
        return HealthProbe::Unreachable;
    };
    let Ok(response) = client.get(health_url).send().await else {
        return HealthProbe::Unreachable;
    };
    if !response.status().is_success() {
        return HealthProbe::Unreachable;
    }
    let Ok(health) = response.json::<EngineHealth>().await else {
        return HealthProbe::Incompatible("health response uses an unknown format".into());
    };
    if health.name != "medousa-code" {
        return HealthProbe::Incompatible(format!("unexpected service {}", health.name));
    }
    if health.api_revision != Some(EXPECTED_API_REVISION) {
        return HealthProbe::Incompatible(format!(
            "API revision {:?}; expected {EXPECTED_API_REVISION}",
            health.api_revision
        ));
    }
    if let Some(missing) = required_roots.iter().find(|required| {
        !health
            .allowed_roots
            .iter()
            .any(|allowed| required.starts_with(allowed))
    }) {
        return HealthProbe::Incompatible(format!(
            "workspace root {} is not allowed",
            missing.display()
        ));
    }
    HealthProbe::Compatible
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
    let required_roots = forge_worktree_roots();

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

    match probe_health(&health_url, &required_roots).await {
        HealthProbe::Compatible => return info(true, "coding engine reachable".into()),
        HealthProbe::Incompatible(reason) => {
            return info(
                false,
                format!(
                    "incompatible medousa-code is already listening on {bind}: {reason}; restart it with the current Medousa package"
                ),
            );
        }
        HealthProbe::Unreachable => {}
    }

    let Some(bin) = resolve_engine_binary() else {
        return info(
            false,
            "medousa-code binary not found — install coding-engine from Settings → Packages or build crates/medousa-code".into(),
        );
    };

    {
        let mut guard = host.child.lock().await;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::warn!(%status, "previous medousa-code process exited");
                    *guard = None;
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "failed to inspect medousa-code process");
                    *guard = None;
                }
            }
        }
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
            for root in &required_roots {
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
        match probe_health(&health_url, &required_roots).await {
            HealthProbe::Compatible => return info(true, "coding engine started".into()),
            HealthProbe::Incompatible(reason) => {
                return info(
                    false,
                    format!("medousa-code started incompatibly: {reason}"),
                );
            }
            HealthProbe::Unreachable => {}
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
        .route(
            "/v1/code/workspace-diagnostics",
            get(code_workspace_diagnostics),
        )
        .route("/v1/code/symbols", get(code_symbols))
        .route("/v1/code/workspace-symbols", get(code_workspace_symbols))
        .route("/v1/code/capabilities", get(code_capabilities))
        .route("/v1/code/conventions", get(code_conventions))
        .route("/v1/code/language-root", get(code_language_root))
        .route("/v1/code/request", post(code_request))
        .with_state(state)
}

pub async fn coding_engine_info(State(state): State<AppState>) -> Json<CodingEngineInfo> {
    let host = state.coding_engine.clone().unwrap_or_default();
    Json(ensure_coding_engine(&host).await)
}

#[derive(Debug, serde::Deserialize)]
pub struct CodeLspQuery {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub work_id: Option<String>,
    #[serde(default)]
    pub document_uri: Option<String>,
}

fn default_language() -> String {
    "grapheme".into()
}

pub async fn code_lsp_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<CodeLspQuery>,
) -> axum::response::Response {
    let host = state.coding_engine.clone().unwrap_or_default();
    let info = ensure_coding_engine(&host).await;
    if !info.available {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, info.message).into_response();
    }
    ws.on_upgrade(move |socket| {
        proxy_lsp_socket(socket, state, q.language, q.work_id, q.document_uri)
    })
    .into_response()
}

async fn proxy_lsp_socket(
    client: WebSocket,
    state: AppState,
    language: String,
    work_id: Option<String>,
    document_uri: Option<String>,
) {
    let host = state.coding_engine.clone().unwrap_or_default();
    let info = ensure_coding_engine(&host).await;
    if !info.available {
        tracing::warn!(message = %info.message, "coding engine unavailable for LSP proxy");
        return;
    }
    let workspace_root = work_id.as_deref().and_then(|raw| {
        let id = medousa_forge::model::WorkId::from(raw.trim().to_owned());
        state
            .forge
            .load(&id)
            .ok()
            .and_then(|item| item.environment.map(|env| env.worktree))
    });
    if work_id.is_some() && workspace_root.is_none() {
        tracing::warn!(
            work_id,
            "rejected LSP request for unknown or unprepared undertaking"
        );
        return;
    }
    let upstream = lsp_upstream_url(
        &info.lsp_url,
        &language,
        workspace_root.as_deref(),
        document_uri.as_deref(),
    );

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

fn lsp_upstream_url(
    lsp_url: &str,
    language: &str,
    workspace_root: Option<&Path>,
    document_uri: Option<&str>,
) -> String {
    let base = if lsp_url.contains("/v1/lsp") {
        lsp_url.trim_end_matches('/').to_string()
    } else {
        format!("{}/v1/lsp", lsp_url.trim_end_matches('/'))
    };
    let mut query = format!("language={}", urlencoding::encode(language));
    if let Some(root) = workspace_root {
        query.push_str("&workspace_root=");
        query.push_str(&urlencoding::encode(&root.to_string_lossy()));
    }
    if let Some(uri) = document_uri.filter(|uri| !uri.trim().is_empty()) {
        query.push_str("&document_uri=");
        query.push_str(&urlencoding::encode(uri));
    }
    format!("{base}?{query}")
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

pub async fn code_workspace_diagnostics(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/workspace-diagnostics", &q).await
}

pub async fn code_workspace_symbols(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/workspace-symbols", &q).await
}

pub async fn code_capabilities(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/capabilities", &q).await
}

pub async fn code_conventions(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/conventions", &q).await
}

pub async fn code_language_root(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_get(&state, "/v1/code/language-root", &q).await
}

pub async fn code_request(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_agent_post(&state, "/v1/code/request", body).await
}

async fn proxy_agent_get(
    state: &AppState,
    path: &str,
    q: &std::collections::HashMap<String, String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let host = state.coding_engine.clone().unwrap_or_default();
    let info = ensure_coding_engine(&host).await;
    if !info.available {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, info.message));
    }
    let mut forwarded = q.clone();
    // Workshop paths are daemon authority, never caller authority.
    forwarded.remove("workspace_root");
    if let Some(work_id) = forwarded.remove("work_id") {
        let id = medousa_forge::model::WorkId::from(work_id.trim().to_owned());
        let root = state
            .forge
            .load(&id)
            .ok()
            .and_then(|item| item.environment.map(|env| env.worktree))
            .ok_or((
                axum::http::StatusCode::CONFLICT,
                "unknown or unprepared undertaking".to_string(),
            ))?;
        forwarded.insert("workspace_root".into(), root.to_string_lossy().into_owned());
    }
    let mut url = reqwest::Url::parse(&format!("{}{path}", info.url))
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in &forwarded {
            pairs.append_pair(k, v);
        }
    }
    let mut last_error = None;
    for attempt in 0..2 {
        let response = reqwest::Client::new().get(url.clone()).send().await;
        let result = match response {
            Ok(response) => decode_upstream_response(response).await,
            Err(err) => Err((axum::http::StatusCode::BAD_GATEWAY, err.to_string())),
        };
        match result {
            Ok(body) => return Ok(body),
            Err(error) if attempt == 0 && error.0 == axum::http::StatusCode::BAD_GATEWAY => {
                tracing::warn!(path, error = %error.1, "retrying idempotent coding engine request");
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or((
        axum::http::StatusCode::BAD_GATEWAY,
        "coding engine request failed".into(),
    )))
}

async fn proxy_agent_post(
    state: &AppState,
    path: &str,
    mut body: serde_json::Value,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let host = state.coding_engine.clone().unwrap_or_default();
    let info = ensure_coding_engine(&host).await;
    if !info.available {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, info.message));
    }
    let work_id = body.as_object_mut().and_then(|object| {
        object.remove("workspace_root");
        object.remove("work_id")
    });
    if let Some(work_id) = work_id.and_then(|value| value.as_str().map(str::to_owned)) {
        let id = medousa_forge::model::WorkId::from(work_id.trim().to_owned());
        let root = state
            .forge
            .load(&id)
            .ok()
            .and_then(|item| item.environment.map(|env| env.worktree))
            .ok_or((
                axum::http::StatusCode::CONFLICT,
                "unknown or unprepared undertaking".to_string(),
            ))?;
        if let Some(object) = body.as_object_mut() {
            object.insert(
                "workspace_root".into(),
                serde_json::Value::String(root.to_string_lossy().into_owned()),
            );
        }
    }
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{path}", info.url))
        .json(&body)
        .send()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    decode_upstream_response(resp).await
}

async fn decode_upstream_response(
    response: reqwest::Response,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let status = response.status();
    let bytes = response.bytes().await.map_err(|e| {
        (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("coding engine response read failed: {e}"),
        )
    })?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&bytes);
        return Err((
            proxy_upstream_status(status),
            format!("coding engine returned {status}: {}", detail.trim()),
        ));
    }
    let body = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| {
        (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("coding engine returned invalid JSON: {e}"),
        )
    })?;
    Ok(Json(body))
}

fn proxy_upstream_status(status: axum::http::StatusCode) -> axum::http::StatusCode {
    if status.is_client_error() {
        status
    } else {
        axum::http::StatusCode::BAD_GATEWAY
    }
}

#[allow(dead_code)]
pub fn parse_bind(bind: &str) -> Option<SocketAddr> {
    bind.parse().ok()
}

#[allow(dead_code)]
pub fn workspace_is_under(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::{lsp_upstream_url, proxy_upstream_status};
    use axum::http::StatusCode;
    use std::path::Path;

    #[test]
    fn coding_engine_client_errors_preserve_their_status() {
        assert_eq!(
            proxy_upstream_status(StatusCode::BAD_REQUEST),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            proxy_upstream_status(StatusCode::NOT_FOUND),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn coding_engine_server_errors_stop_at_the_gateway() {
        assert_eq!(
            proxy_upstream_status(StatusCode::INTERNAL_SERVER_ERROR),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn lsp_proxy_forwards_the_document_and_daemon_owned_root() {
        let upstream = lsp_upstream_url(
            "ws://127.0.0.1:7861/v1/lsp",
            "typescript",
            Some(Path::new("/work trees/project")),
            Some("file:///work%20trees/project/packages/app/src/main.ts"),
        );
        let parsed = reqwest::Url::parse(&upstream).unwrap();
        let query = parsed
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(parsed.path(), "/v1/lsp");
        assert_eq!(query.get("language").unwrap(), "typescript");
        assert_eq!(query.get("workspace_root").unwrap(), "/work trees/project");
        assert_eq!(
            query.get("document_uri").unwrap(),
            "file:///work%20trees/project/packages/app/src/main.ts"
        );
    }
}
