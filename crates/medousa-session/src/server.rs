//! HTTP/WS surface for the session host.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;

use crate::session::{SessionId, SessionManager, SessionMeta, SessionRootKind};
use crate::{ENGINE_API_REVISION, ENGINE_NAME, ENGINE_VERSION};

#[derive(Clone)]
pub struct SessionHostConfig {
    pub bind: SocketAddr,
    pub workspace_root: PathBuf,
    /// Additional allowed cwd roots. The workspace root is always allowed.
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

    fn effective_allowed_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.config.workspace_root.clone()];
        for root in &self.config.allowed_roots {
            if !roots.contains(root) {
                roots.push(root.clone());
            }
        }
        roots
    }

    pub fn cwd_allowed(&self, path: &std::path::Path) -> bool {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.effective_allowed_roots()
            .iter()
            .any(|root| canon.starts_with(root))
    }
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub name: String,
    pub version: String,
    pub api_revision: u32,
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
    /// When present, host this command directly in the PTY instead of spawning a shell.
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub ok: bool,
    pub session_id: String,
    pub cwd: String,
    pub root_kind: String,
    pub work_id: Option<String>,
    pub argv: Vec<String>,
    pub ws_path: String,
    pub cols: u16,
    pub rows: u16,
    pub message: String,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

pub fn app(state: SessionHostState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route(
            "/v1/sessions/shell",
            get(list_sessions).post(create_session),
        )
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
        api_revision: ENGINE_API_REVISION,
        uptime_secs: state.started.elapsed().as_secs(),
        active_sessions: state.manager.list().await.len(),
        workspace_root: state.config.workspace_root.display().to_string(),
        allowed_roots: state
            .effective_allowed_roots()
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
        "argv": m.argv,
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
    let argv = body.argv.unwrap_or_default();
    if argv.len() > 64
        || argv
            .iter()
            .any(|value| value.is_empty() || value.len() > 16 * 1024)
        || argv.iter().map(String::len).sum::<usize>() > 64 * 1024
    {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "argv is empty or exceeds the hosted-command limit".into(),
        ));
    }
    let session = state
        .manager
        .create_command_with_size(
            root_kind,
            Some(cwd),
            body.work_id.clone(),
            argv.clone(),
            body.cols,
            body.rows,
        )
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let size = session
        .size()
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
        argv,
        ws_path: format!("/v1/sessions/shell/{}", session.meta.session_id),
        cols: size.cols,
        rows: size.rows,
        message: "session created".into(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Stdin { data: String },
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Default, Deserialize)]
struct SessionAttachQuery {
    /// Resume after a previously consumed output chunk.
    #[serde(default)]
    after_sequence: Option<u64>,
    /// `tail` skips retained history and starts at the current output watermark.
    #[serde(default)]
    replay: Option<String>,
}

struct AttachReplay {
    chunks: Vec<crate::session::OutputChunk>,
    last_sequence: u64,
    oldest_sequence: Option<u64>,
    replay_truncated: bool,
}

fn attach_replay(
    snapshot: Vec<crate::session::OutputChunk>,
    query: &SessionAttachQuery,
) -> AttachReplay {
    let oldest_sequence = snapshot.first().map(|chunk| chunk.sequence);
    let latest_sequence = snapshot.last().map_or(0, |chunk| chunk.sequence);

    if let Some(requested) = query.after_sequence {
        // A cursor from a replaced session host must not suppress all future output.
        let cursor_reset = requested > latest_sequence;
        let baseline = requested.min(latest_sequence);
        let replay_truncated = cursor_reset
            || oldest_sequence.is_some_and(|oldest| baseline.saturating_add(1) < oldest);
        let chunks = snapshot
            .into_iter()
            .filter(|chunk| chunk.sequence > baseline)
            .collect::<Vec<_>>();
        let last_sequence = chunks.last().map_or(baseline, |chunk| chunk.sequence);
        return AttachReplay {
            chunks,
            last_sequence,
            oldest_sequence,
            replay_truncated,
        };
    }

    if query.replay.as_deref() == Some("tail") {
        return AttachReplay {
            chunks: Vec::new(),
            last_sequence: latest_sequence,
            oldest_sequence,
            replay_truncated: false,
        };
    }

    AttachReplay {
        last_sequence: latest_sequence,
        chunks: snapshot,
        oldest_sequence,
        replay_truncated: false,
    }
}

async fn session_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<SessionHostState>>,
    Path(id): Path<String>,
    Query(query): Query<SessionAttachQuery>,
) -> impl IntoResponse {
    let session_id = SessionId(id);
    let session = state.manager.get(&session_id).await;
    ws.on_upgrade(move |socket| handle_ws(socket, session, query))
}

async fn handle_ws(
    socket: WebSocket,
    session: Option<Arc<crate::session::Session>>,
    query: SessionAttachQuery,
) {
    let Some(session) = session else {
        return;
    };
    session.touch().await;
    let mut output_rx = session.output.subscribe();
    let mut exit_poll = tokio::time::interval(std::time::Duration::from_millis(50));
    let (mut ws_tx, mut ws_rx) = socket.split();

    let replay = attach_replay(session.output_snapshot(), &query);
    let mut last_sequence = replay.last_sequence;
    for chunk in replay.chunks {
        if send_output_chunk(&mut ws_tx, &chunk).await.is_err() {
            return;
        }
    }
    if send_attach_ready(
        &mut ws_tx,
        last_sequence,
        replay.oldest_sequence,
        replay.replay_truncated,
    )
    .await
    .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            output = output_rx.recv() => match output {
                Ok(chunk) if chunk.sequence > last_sequence => {
                    last_sequence = chunk.sequence;
                    if send_output_chunk(&mut ws_tx, &chunk).await.is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let snapshot = session.output_snapshot();
                    let oldest_sequence = snapshot.first().map(|chunk| chunk.sequence);
                    if oldest_sequence
                        .is_some_and(|oldest| last_sequence.saturating_add(1) < oldest)
                        && send_output_gap(&mut ws_tx, last_sequence, oldest_sequence).await.is_err()
                    {
                        return;
                    }
                    for chunk in snapshot {
                        if chunk.sequence > last_sequence {
                            last_sequence = chunk.sequence;
                            if send_output_chunk(&mut ws_tx, &chunk).await.is_err() {
                                return;
                            }
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            message = ws_rx.next() => {
                let Some(Ok(message)) = message else {
                    break;
                };
                session.touch().await;
                let parsed = match &message {
                    Message::Text(text) => serde_json::from_str::<ClientFrame>(text).ok(),
                    Message::Binary(bytes) => {
                        let _ = session.write(bytes);
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
                    Some(ClientFrame::Resize { cols, rows }) => {
                        match session.resize(cols, rows) {
                            Ok(size) => {
                                if send_resize_ack(&mut ws_tx, size.cols, size.rows).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                if send_protocol_error(&mut ws_tx, "resize_failed", &error.to_string())
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                    None => {}
                }
            }
            _ = exit_poll.tick(), if session.exited() => {
                // The child wait may win the race with the PTY reader. Replay
                // the final retained chunks before publishing terminal state.
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                for chunk in session.output_snapshot() {
                    if chunk.sequence > last_sequence {
                        last_sequence = chunk.sequence;
                        if send_output_chunk(&mut ws_tx, &chunk).await.is_err() {
                            return;
                        }
                    }
                }
                let _ = send_exit(&mut ws_tx, session.exit_code()).await;
                break;
            }
        }
    }
}

async fn send_output_chunk(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    chunk: &crate::session::OutputChunk,
) -> Result<(), axum::Error> {
    let frame = json!({
        "type": "stdout",
        "sequence": chunk.sequence,
        "data": base64::engine::general_purpose::STANDARD.encode(&chunk.bytes)
    })
    .to_string();
    ws_tx.send(Message::Text(frame.into())).await
}

async fn send_attach_ready(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    sequence: u64,
    oldest_sequence: Option<u64>,
    replay_truncated: bool,
) -> Result<(), axum::Error> {
    let frame = json!({
        "type": "ready",
        "sequence": sequence,
        "oldest_sequence": oldest_sequence,
        "replay_truncated": replay_truncated,
    })
    .to_string();
    ws_tx.send(Message::Text(frame.into())).await
}

async fn send_exit(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    exit_code: Option<i32>,
) -> Result<(), axum::Error> {
    ws_tx
        .send(Message::Text(
            json!({ "type": "exit", "exit_code": exit_code })
                .to_string()
                .into(),
        ))
        .await
}

async fn send_output_gap(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    after_sequence: u64,
    oldest_sequence: Option<u64>,
) -> Result<(), axum::Error> {
    let frame = json!({
        "type": "output_gap",
        "after_sequence": after_sequence,
        "oldest_sequence": oldest_sequence,
    })
    .to_string();
    ws_tx.send(Message::Text(frame.into())).await
}

async fn send_resize_ack(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    cols: u16,
    rows: u16,
) -> Result<(), axum::Error> {
    let frame = json!({
        "type": "resize",
        "cols": cols,
        "rows": rows
    })
    .to_string();
    ws_tx.send(Message::Text(frame.into())).await
}

async fn send_protocol_error(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    code: &str,
    message: &str,
) -> Result<(), axum::Error> {
    let frame = json!({
        "type": "error",
        "code": code,
        "message": message
    })
    .to_string();
    ws_tx.send(Message::Text(frame.into())).await
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
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            "session not found".into(),
        ));
    };
    match body.signal.as_str() {
        "interrupt" | "sigint" => session.signal_interrupt(),
        "kill" => session.kill(),
        "destroy" => {
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

#[cfg(test)]
mod tests {
    use super::{SessionAttachQuery, SessionHostConfig, SessionHostState, attach_replay};
    use crate::session::OutputChunk;
    use std::path::PathBuf;

    fn state() -> SessionHostState {
        SessionHostState::new(SessionHostConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workspace_root: PathBuf::from("medousa/scripts"),
            allowed_roots: vec![PathBuf::from("medousa/forge/worktrees")],
        })
    }

    #[test]
    fn workspace_and_future_forge_worktrees_are_allowed() {
        let state = state();
        assert!(state.cwd_allowed(std::path::Path::new("medousa/scripts")));
        assert!(state.cwd_allowed(std::path::Path::new(
            "medousa/forge/worktrees/repo/work-new"
        )));
        assert!(!state.cwd_allowed(std::path::Path::new("other")));
    }

    fn chunks(sequences: impl IntoIterator<Item = u64>) -> Vec<OutputChunk> {
        sequences
            .into_iter()
            .map(|sequence| OutputChunk {
                sequence,
                bytes: sequence.to_string().into_bytes(),
            })
            .collect()
    }

    #[test]
    fn human_attach_replays_retained_history() {
        let replay = attach_replay(chunks([3, 4, 5]), &SessionAttachQuery::default());
        assert_eq!(
            replay
                .chunks
                .iter()
                .map(|chunk| chunk.sequence)
                .collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
        assert_eq!(replay.last_sequence, 5);
        assert!(!replay.replay_truncated);
    }

    #[test]
    fn agent_tail_attach_skips_retained_history() {
        let replay = attach_replay(
            chunks([3, 4, 5]),
            &SessionAttachQuery {
                replay: Some("tail".into()),
                ..Default::default()
            },
        );
        assert!(replay.chunks.is_empty());
        assert_eq!(replay.last_sequence, 5);
        assert!(!replay.replay_truncated);
    }

    #[test]
    fn agent_cursor_replays_only_unconsumed_chunks() {
        let replay = attach_replay(
            chunks([3, 4, 5]),
            &SessionAttachQuery {
                after_sequence: Some(3),
                ..Default::default()
            },
        );
        assert_eq!(
            replay
                .chunks
                .iter()
                .map(|chunk| chunk.sequence)
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(replay.last_sequence, 5);
        assert!(!replay.replay_truncated);
    }

    #[test]
    fn stale_agent_cursor_reports_a_history_gap() {
        let replay = attach_replay(
            chunks([8, 9, 10]),
            &SessionAttachQuery {
                after_sequence: Some(3),
                ..Default::default()
            },
        );
        assert_eq!(
            replay
                .chunks
                .iter()
                .map(|chunk| chunk.sequence)
                .collect::<Vec<_>>(),
            vec![8, 9, 10]
        );
        assert!(replay.replay_truncated);
    }

    #[test]
    fn cursor_from_a_replaced_host_resets_to_the_current_watermark() {
        let replay = attach_replay(
            chunks([8, 9, 10]),
            &SessionAttachQuery {
                after_sequence: Some(42),
                ..Default::default()
            },
        );
        assert!(replay.chunks.is_empty());
        assert_eq!(replay.last_sequence, 10);
        assert!(replay.replay_truncated);
    }
}
