//! HTTP/WS surface for the session host.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::session::{SessionId, SessionManager, SessionMeta, SessionRootKind};
use crate::{ENGINE_NAME, ENGINE_VERSION};

#[derive(Clone)]
pub struct SessionHostConfig {
    pub bind: SocketAddr,
    pub workspace_root: PathBuf,
    /// Allowed cwd roots (scripts, forge worktrees). Empty = workspace_root only.
    pub allowed_roots: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct SessionHostState {
    pub config: SessionHostConfig,
    pub manager: Arc<SessionManager>,
    pub started: Instant,
}

impl SessionHostState {
    pub fn new(config: SessionHostConfig) -> Self {
        Self {
            manager: SessionManager::new(config.workspace_root.clone()),
            config,
            started: Instant::now(),
        }
    }

    pub fn cwd_allowed(&self, path: &std::path::Path) -> bool {
        let roots = if self.config.allowed_roots.is_empty() {
            vec![self.config.workspace_root.clone()]
        } else {
            self.config.allowed_roots.clone()
        };
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        roots.iter().any(|root| canon.starts_with(root))
    }
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub name: String,
    pub version: String,
    pub uptime_secs: u64,
    pub active_sessions: usize,
    pub workspace_root: String,
    pub allowed_roots: Vec<String>,
}

#[derive(Deserialize)]
pub struct CreateSessionBody {
    #[serde(default)]
    pub work_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub ok: bool,
    pub session_id: String,
    pub cwd: String,
    pub root_kind: String,
    pub work_id: Option<String>,
    pub ws_path: String,
    pub message: String,
}

pub fn app(state: SessionHostState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/sessions/shell", get(list_sessions).post(create_session))
        .route("/v1/sessions/shell/{id}", get(session_ws))
        .route("/v1/sessions/shell/{id}/signal", post(signal_session))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state))
}

pub async fn serve(state: SessionHostState) -> anyhow::Result<()> {
    let addr = state.config.bind;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "medousa-session host listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}

async fn health(State(state): State<Arc<SessionHostState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        name: ENGINE_NAME.into(),
        version: ENGINE_VERSION.into(),
        uptime_secs: state.started.elapsed().as_secs(),
        active_sessions: state.manager.list().await.len(),
        workspace_root: state.config.workspace_root.display().to_string(),
        allowed_roots: state
            .config
            .allowed_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    })
}

fn meta_json(m: &SessionMeta) -> Value {
    json!({
        "session_id": m.session_id.as_str(),
        "cwd": m.cwd.display().to_string(),
        "root_kind": match m.root_kind {
            SessionRootKind::Scripts => "scripts",
            SessionRootKind::Forge => "forge",
        },
        "work_id": m.work_id,
    })
}

async fn list_sessions(State(state): State<Arc<SessionHostState>>) -> Json<Value> {
    let sessions: Vec<Value> = state.manager.list().await.iter().map(meta_json).collect();
    Json(json!({ "ok": true, "sessions": sessions }))
}

async fn create_session(
    State(state): State<Arc<SessionHostState>>,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<CreateSessionResponse>, (axum::http::StatusCode, String)> {
    let cwd = body.cwd.as_deref().map(PathBuf::from);
    let root_kind = if body.work_id.is_some() {
        SessionRootKind::Forge
    } else {
        SessionRootKind::Scripts
    };
    let cwd = cwd.unwrap_or_else(|| state.config.workspace_root.clone());
    if !state.cwd_allowed(&cwd) {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            format!("cwd not allowed: {}", cwd.display()),
        ));
    }
    let session = state
        .manager
        .create(root_kind, Some(cwd), body.work_id.clone())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(CreateSessionResponse {
        ok: true,
        session_id: session.meta.session_id.as_str().to_string(),
        cwd: session.meta.cwd.display().to_string(),
        root_kind: match session.meta.root_kind {
            SessionRootKind::Scripts => "scripts".into(),
            SessionRootKind::Forge => "forge".into(),
        },
        work_id: session.meta.work_id.clone(),
        ws_path: format!("/v1/sessions/shell/{}", session.meta.session_id),
        message: "session created".into(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Stdin { data: String },
    Resize { cols: u16, rows: u16 },
}

async fn session_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<SessionHostState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session_id = SessionId(id);
    let session = state.manager.get(&session_id).await;
    ws.on_upgrade(move |socket| handle_ws(socket, session))
}

async fn handle_ws(socket: WebSocket, session: Option<Arc<crate::session::Session>>) {
    let Some(session) = session else {
        return;
    };
    session.touch().await;
    let mut output_rx = session.output.subscribe();
    let (mut ws_tx, mut ws_rx) = socket.split();

    let fanout = tokio::spawn(async move {
        while let Ok(bytes) = output_rx.recv().await {
            let frame = json!({
                "type": "stdout",
                "data": base64::engine::general_purpose::STANDARD.encode(&bytes)
            })
            .to_string();
            if ws_tx.send(Message::Text(frame.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        session.touch().await;
        let parsed = match &msg {
            Message::Text(t) => serde_json::from_str::<ClientFrame>(t).ok(),
            Message::Binary(b) => {
                let _ = session.write(b);
                None
            }
            Message::Close(_) => break,
            _ => None,
        };
        match parsed {
            Some(ClientFrame::Stdin { data }) => {
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&data) else {
                    continue;
                };
                let _ = session.write(&bytes);
            }
            Some(ClientFrame::Resize { cols, rows }) => session.resize(cols, rows),
            None => {}
        }
    }
    fanout.abort();
}

#[derive(Deserialize)]
pub struct SignalBody {
    #[serde(default = "default_signal")]
    pub signal: String,
}

fn default_signal() -> String {
    "interrupt".into()
}

async fn signal_session(
    State(state): State<Arc<SessionHostState>>,
    Path(id): Path<String>,
    Json(body): Json<SignalBody>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let session_id = SessionId(id);
    let Some(session) = state.manager.get(&session_id).await else {
        return Err((axum::http::StatusCode::NOT_FOUND, "session not found".into()));
    };
    match body.signal.as_str() {
        "interrupt" | "sigint" => session.signal_interrupt(),
        "kill" | "destroy" => {
            state.manager.destroy(&session_id).await;
        }
        other => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                format!("unknown signal: {other}"),
            ));
        }
    }
    Ok(Json(json!({ "ok": true })))
}
