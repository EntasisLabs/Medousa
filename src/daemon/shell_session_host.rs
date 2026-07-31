//! Daemon-side host for the workshop shell session host (`medousa-session`).
//!
//! One OS PTY per `session_id` owned on the workshop; Home Terminal tabs and
//! agent coding tools attach as peers. Same authority rules as the coding
//! engine: remote Home never opens a local PTY against a foreign workshop disk.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

use crate::daemon::state::AppState;
use crate::grapheme_script::store::GraphemeScriptStore;
use crate::paths::medousa_data_dir;

const DEFAULT_BIND: &str = "127.0.0.1:7862";
const EXPECTED_API_REVISION: u32 = 1;

#[derive(Debug, Default)]
pub struct ShellSessionHost {
    child: Mutex<Option<Child>>,
}

impl ShellSessionHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ShellSessionInfo {
    pub available: bool,
    pub url: String,
    pub health_url: String,
    pub daemon_base_path: String,
    pub workspace_root: String,
    pub bind: String,
    pub message: String,
}

fn session_bind() -> String {
    std::env::var("MEDOUSA_SESSION_BIND").unwrap_or_else(|_| DEFAULT_BIND.into())
}

fn session_http_base(bind: &str) -> String {
    format!("http://{bind}")
}

fn resolve_session_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("MEDOUSA_SESSION_BIN") {
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
        "medousa-session.exe"
    } else {
        "medousa-session"
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

/// Forge worktree roots exposed for the coding tool surface (read/search/patch).
pub fn forge_worktree_roots_for_tools() -> Vec<PathBuf> {
    forge_worktree_roots()
}

#[derive(Debug, Deserialize)]
struct SessionHealth {
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
    let Ok(health) = response.json::<SessionHealth>().await else {
        return HealthProbe::Incompatible("health response uses an unknown format".into());
    };
    if health.name != "medousa-session" {
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

pub async fn ensure_shell_session_host(host: &ShellSessionHost) -> ShellSessionInfo {
    let bind = session_bind();
    let base = session_http_base(&bind);
    let health_url = format!("{base}/health");
    let workspace_root = GraphemeScriptStore::root_dir();
    let workspace_str = workspace_root.to_string_lossy().into_owned();
    let required_roots = forge_worktree_roots();

    let info = |available: bool, message: String| ShellSessionInfo {
        available,
        url: base.clone(),
        health_url: health_url.clone(),
        daemon_base_path: "/v1/sessions/shell".into(),
        workspace_root: workspace_str.clone(),
        bind: bind.clone(),
        message,
    };

    match probe_health(&health_url, &required_roots).await {
        HealthProbe::Compatible => return info(true, "session host reachable".into()),
        HealthProbe::Incompatible(reason) => {
            return info(
                false,
                format!(
                    "incompatible medousa-session is already listening on {bind}: {reason}; restart it with the current Medousa package"
                ),
            );
        }
        HealthProbe::Unreachable => {}
    }

    let Some(bin) = resolve_session_binary() else {
        return info(
            false,
            "medousa-session binary not found — install shell-session from Settings → Packages or build crates/medousa-session".into(),
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
            for root in &required_roots {
                cmd.arg("--allow-root").arg(root);
            }
            match cmd.spawn() {
                Ok(child) => {
                    *guard = Some(child);
                    tracing::info!(binary = %bin.display(), %bind, "spawned medousa-session");
                }
                Err(err) => {
                    return info(false, format!("failed to spawn medousa-session: {err}"));
                }
            }
        }
    }

    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        match probe_health(&health_url, &required_roots).await {
            HealthProbe::Compatible => return info(true, "session host started".into()),
            HealthProbe::Incompatible(reason) => {
                return info(
                    false,
                    format!("medousa-session started incompatibly: {reason}"),
                );
            }
            HealthProbe::Unreachable => {}
        }
    }

    info(false, "medousa-session spawned but health timed out".into())
}

pub fn shell_session_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/shell-sessions", get(shell_sessions_info))
        .route(
            "/v1/sessions/shell",
            get(list_sessions).post(create_session),
        )
        .route("/v1/sessions/shell/{id}", get(session_ws))
        .route(
            "/v1/sessions/shell/{id}/signal",
            axum::routing::post(signal_session),
        )
        .with_state(state)
}

pub async fn shell_sessions_info(State(state): State<AppState>) -> Json<ShellSessionInfo> {
    let host = state.shell_sessions.clone().unwrap_or_default();
    Json(ensure_shell_session_host(&host).await)
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_http(&state, "GET", "/v1/sessions/shell", None).await
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionBody {
    #[serde(default)]
    pub work_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    /// Optional Forge lease id — enables command-log staging for evidence.
    #[serde(default)]
    pub lease_id: Option<String>,
}

async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    // Forge bind: work_id → worktree cwd via Forge.
    let mut cwd = body.cwd.clone();
    let mut work_id = body.work_id.clone();
    if let Some(wid) = work_id.as_deref().filter(|s| !s.trim().is_empty()) {
        let item = state
            .forge
            .load(&medousa_forge::model::WorkId::from(wid.to_string()))
            .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
        let Some(env) = item.environment else {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                format!("work {wid} is not provisioned (no governed env)"),
            ));
        };
        cwd = Some(env.worktree.to_string_lossy().into_owned());
        work_id = Some(wid.to_string());
    }
    let payload = serde_json::json!({ "work_id": work_id, "cwd": cwd });
    let mut response = proxy_http(&state, "POST", "/v1/sessions/shell", Some(payload)).await?;

    if let (Some(lease_id), Some(wid)) = (
        body.lease_id.as_deref().filter(|s| !s.trim().is_empty()),
        work_id.as_deref(),
    ) && let Some(session_id) = response.0.get("session_id").and_then(|v| v.as_str())
    {
        let now = chrono::Utc::now();
        let lease = medousa_forge::model::ExecutionLease {
            lease_id: medousa_forge::model::LeaseId::from(lease_id.to_string()),
            generation: response
                .0
                .get("generation")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            work_id: medousa_forge::model::WorkId::from(wid.to_string()),
            attempt_id: medousa_forge::model::AttemptId::from(
                response
                    .0
                    .get("attempt_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("shell-session")
                    .to_string(),
            ),
            owner_instance_id: "medousa-session".into(),
            acquired_at: now,
            heartbeat_at: now,
            pid: None,
            process_start_marker: None,
        };
        let _ = state.forge.append_command_log(
            &lease,
            &serde_json::json!({
                "kind": "shell_session_open",
                "session_id": session_id,
                "cwd": cwd,
            }),
        );
        if let Some(obj) = response.0.as_object_mut() {
            obj.insert("forge_log_staged".into(), serde_json::Value::Bool(true));
        }
    }

    Ok(response)
}

async fn signal_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    proxy_http(
        &state,
        "POST",
        &format!("/v1/sessions/shell/{id}/signal"),
        Some(body),
    )
    .await
}

async fn proxy_http(
    state: &AppState,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let host = state.shell_sessions.clone().unwrap_or_default();
    let info = ensure_shell_session_host(&host).await;
    if !info.available {
        return Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, info.message));
    }
    let url = format!("{}{path}", info.url);
    let client = reqwest::Client::new();
    let req = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        _ => return Err((axum::http::StatusCode::BAD_REQUEST, "bad method".into())),
    };
    let req = if let Some(b) = body {
        req.json(&b)
    } else {
        req
    };
    let resp = req
        .send()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY, e.to_string()))?;
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| {
        (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("session host response read failed: {e}"),
        )
    })?;
    if !status.is_success() {
        let detail = String::from_utf8_lossy(&bytes);
        return Err((
            proxy_upstream_status(status),
            format!("session host returned {status}: {}", detail.trim()),
        ));
    }
    let value = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| {
        (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("session host returned invalid JSON: {e}"),
        )
    })?;
    Ok(Json(value))
}

fn proxy_upstream_status(status: axum::http::StatusCode) -> axum::http::StatusCode {
    if status.is_client_error() {
        status
    } else {
        axum::http::StatusCode::BAD_GATEWAY
    }
}

async fn session_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| proxy_ws(socket, state, id))
}

async fn proxy_ws(client: WebSocket, state: AppState, id: String) {
    let host = state.shell_sessions.clone().unwrap_or_default();
    let info = ensure_shell_session_host(&host).await;
    if !info.available {
        tracing::warn!(message = %info.message, "session host unavailable for WS proxy");
        return;
    }
    let ws_base = info.url.replacen("http", "ws", 1);
    let upstream = format!("{ws_base}/v1/sessions/shell/{}", urlencoding::encode(&id));
    let Ok((upstream_ws, _)) = connect_async(&upstream).await else {
        tracing::warn!(%upstream, "failed to connect to medousa-session");
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

#[allow(dead_code)]
pub fn parse_bind(bind: &str) -> Option<SocketAddr> {
    bind.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::proxy_upstream_status;
    use axum::http::StatusCode;

    #[test]
    fn session_host_client_errors_preserve_their_status() {
        assert_eq!(
            proxy_upstream_status(StatusCode::FORBIDDEN),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn session_host_server_errors_stop_at_the_gateway() {
        assert_eq!(
            proxy_upstream_status(StatusCode::INTERNAL_SERVER_ERROR),
            StatusCode::BAD_GATEWAY
        );
    }
}
