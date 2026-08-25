//! Home-side terminal transport.
//!
//! The session host owns the PTY and xterm.js owns terminal emulation. This
//! module is deliberately only a message bridge: one async task owns each
//! websocket, preserving the order of stdin/resize control messages while
//! forwarding raw PTY output to the matching webview attachment.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{Notify, mpsc};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::AUTHORIZATION},
    },
};
use tokio_util::sync::CancellationToken;

use crate::daemon::DaemonState;
use crate::daemon::sdk::client;

static NEXT_ATTACH_ID: AtomicU64 = AtomicU64::new(1);

pub type TerminalRegistry = Arc<Mutex<HashMap<u64, TerminalHandle>>>;

pub struct TerminalHandle {
    outbound: mpsc::UnboundedSender<TerminalClientFrame>,
    ready: Arc<Notify>,
    cancel: CancellationToken,
}

enum TerminalClientFrame {
    Stdin(Vec<u8>),
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalSessionSummary {
    pub session_id: String,
    pub cwd: String,
    pub root_kind: String,
    pub work_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub available: bool,
    pub url: String,
    pub daemon_base_path: String,
    pub workspace_root: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalAttachResponse {
    pub attach_id: u64,
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize)]
struct TerminalOutput {
    attach_id: u64,
    data: String,
}

#[derive(Clone, Debug, Serialize)]
struct TerminalStatus {
    attach_id: u64,
    connected: bool,
    message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct TerminalResizeAck {
    attach_id: u64,
    cols: u16,
    rows: u16,
}

#[derive(Clone, Debug, Serialize)]
struct TerminalProtocolError {
    attach_id: u64,
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalCreateInput {
    pub work_id: Option<String>,
    pub cwd: Option<String>,
    pub lease_id: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

fn ws_url_for(daemon_url: &str, path: &str) -> String {
    let base = daemon_url.trim_end_matches('/').replacen("http", "ws", 1);
    format!("{base}{path}")
}

pub(crate) fn authenticated_ws_request(
    path: &str,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, String> {
    let config = crate::active_workshop::transport_config()?;
    ws_request_with_bearer(
        &config.lan_base,
        path,
        config.session_token.as_deref(),
    )
}

fn ws_request_with_bearer(
    daemon_url: &str,
    path: &str,
    token: Option<&str>,
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, String> {
    let url = ws_url_for(daemon_url, path);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|error| error.to_string())?;
    if let Some(token) = token {
        let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| "workshop credential cannot be used for WebSocket authentication")?;
        value.set_sensitive(true);
        request.headers_mut().insert(AUTHORIZATION, value);
    }
    Ok(request)
}

async fn daemon_get<T: serde::de::DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    client(state)?
        .http()
        .get(path)
        .await
        .map_err(|e| e.to_string())
}

async fn daemon_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    client(state)?
        .http()
        .post(path, body)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_info(state: State<'_, DaemonState>) -> Result<TerminalInfo, String> {
    daemon_get(&state, "/v1/shell-sessions").await
}

#[tauri::command]
pub async fn terminal_sessions(
    state: State<'_, DaemonState>,
) -> Result<Vec<TerminalSessionSummary>, String> {
    let value: serde_json::Value = daemon_get(&state, "/v1/sessions/shell").await?;
    Ok(value
        .get("sessions")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn terminal_interrupt(
    state: State<'_, DaemonState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    daemon_post(
        &state,
        &format!("/v1/sessions/shell/{session_id}/signal"),
        &serde_json::json!({ "signal": "interrupt" }),
    )
    .await
}

#[tauri::command]
pub async fn terminal_create(
    state: State<'_, DaemonState>,
    input: TerminalCreateInput,
) -> Result<serde_json::Value, String> {
    daemon_post(
        &state,
        "/v1/sessions/shell",
        &serde_json::json!({
            "work_id": input.work_id,
            "cwd": input.cwd,
            "lease_id": input.lease_id,
            "cols": input.cols.unwrap_or(80),
            "rows": input.rows.unwrap_or(24),
        }),
    )
    .await
}

#[tauri::command]
pub async fn terminal_attach(
    app: AppHandle,
    _state: State<'_, DaemonState>,
    registry: State<'_, TerminalRegistry>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<TerminalAttachResponse, String> {
    let request = authenticated_ws_request(
        &format!("/v1/sessions/shell/{}", urlencoding::encode(&session_id)),
    )?;
    let (websocket, _) = connect_async(request)
        .await
        .map_err(|error| error.to_string())?;

    let attach_id = NEXT_ATTACH_ID.fetch_add(1, Ordering::SeqCst);
    let (outbound, outbound_rx) = mpsc::unbounded_channel();
    let ready = Arc::new(Notify::new());
    let cancel = CancellationToken::new();

    registry
        .lock()
        .map_err(|_| "terminal registry lock")?
        .insert(
            attach_id,
            TerminalHandle {
                outbound,
                ready: Arc::clone(&ready),
                cancel: cancel.clone(),
            },
        );

    tauri::async_runtime::spawn(run_terminal_transport(
        attach_id,
        app,
        websocket,
        outbound_rx,
        ready,
        cancel,
        cols,
        rows,
    ));

    Ok(TerminalAttachResponse {
        attach_id,
        session_id,
    })
}

async fn run_terminal_transport<S>(
    attach_id: u64,
    app: AppHandle,
    websocket: tokio_tungstenite::WebSocketStream<S>,
    mut outbound: mpsc::UnboundedReceiver<TerminalClientFrame>,
    ready: Arc<Notify>,
    cancel: CancellationToken,
    initial_cols: u16,
    initial_rows: u16,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::select! {
        _ = ready.notified() => {}
        _ = cancel.cancelled() => return,
    }

    let (mut ws_tx, mut ws_rx) = websocket.split();
    let initial_resize = serde_json::json!({
        "type": "resize",
        "cols": initial_cols.max(2),
        "rows": initial_rows.max(1),
    });
    if let Err(error) = ws_tx
        .send(Message::Text(initial_resize.to_string().into()))
        .await
    {
        let _ = app.emit(
            "terminal-status",
            TerminalStatus {
                attach_id,
                connected: false,
                message: Some(error.to_string()),
            },
        );
        return;
    }
    let _ = app.emit(
        "terminal-status",
        TerminalStatus {
            attach_id,
            connected: true,
            message: None,
        },
    );

    let disconnect_message = loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = ws_tx.close().await;
                return;
            }
            command = outbound.recv() => {
                let Some(command) = command else {
                    let _ = ws_tx.close().await;
                    return;
                };
                let frame = match command {
                    TerminalClientFrame::Stdin(bytes) => serde_json::json!({
                        "type": "stdin",
                        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                    }),
                    TerminalClientFrame::Resize { cols, rows } => serde_json::json!({
                        "type": "resize",
                        "cols": cols,
                        "rows": rows,
                    }),
                };
                if let Err(error) = ws_tx.send(Message::Text(frame.to_string().into())).await {
                    break Some(error.to_string());
                }
            }
            message = ws_rx.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                            match value.get("type").and_then(|value| value.as_str()) {
                                Some("stdout") => {
                                    if let Some(data) =
                                        value.get("data").and_then(|value| value.as_str())
                                    {
                                        let _ = app.emit(
                                            "terminal-output",
                                            TerminalOutput {
                                                attach_id,
                                                data: data.to_string(),
                                            },
                                        );
                                    }
                                }
                                Some("resize") => {
                                    if let (Some(cols), Some(rows)) = (
                                        value.get("cols").and_then(|value| value.as_u64()),
                                        value.get("rows").and_then(|value| value.as_u64()),
                                    ) {
                                        let _ = app.emit(
                                            "terminal-resize",
                                            TerminalResizeAck {
                                                attach_id,
                                                cols: cols.min(u16::MAX as u64) as u16,
                                                rows: rows.min(u16::MAX as u64) as u16,
                                            },
                                        );
                                    }
                                }
                                Some("error") => {
                                    let _ = app.emit(
                                        "terminal-error",
                                        TerminalProtocolError {
                                            attach_id,
                                            code: value
                                                .get("code")
                                                .and_then(|value| value.as_str())
                                                .unwrap_or("terminal_protocol_error")
                                                .to_string(),
                                            message: value
                                                .get("message")
                                                .and_then(|value| value.as_str())
                                                .unwrap_or("terminal protocol error")
                                                .to_string(),
                                        },
                                    );
                                }
                                Some("exit") => {
                                    let code = value
                                        .get("exit_code")
                                        .and_then(|value| value.as_i64());
                                    break Some(code.map_or_else(
                                        || "Task process exited".to_string(),
                                        |code| format!("Task process exited with code {code}"),
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        let _ = app.emit(
                            "terminal-output",
                            TerminalOutput {
                                attach_id,
                                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                            },
                        );
                    }
                    Some(Ok(Message::Close(frame))) => {
                        break frame.map(|frame| frame.reason.to_string());
                    }
                    Some(Err(error)) => break Some(error.to_string()),
                    None => break None,
                    _ => {}
                }
            }
        }
    };

    let _ = app.emit(
        "terminal-status",
        TerminalStatus {
            attach_id,
            connected: false,
            message: disconnect_message,
        },
    );
}

#[tauri::command]
pub async fn terminal_ready(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
) -> Result<(), String> {
    let handles = registry.lock().map_err(|_| "terminal registry lock")?;
    let handle = handles
        .get(&attach_id)
        .ok_or_else(|| "unknown attach_id".to_string())?;
    handle.ready.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn terminal_write(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
    data: String,
) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())?;
    send_frame(&registry, attach_id, TerminalClientFrame::Stdin(bytes))
}

#[tauri::command]
pub async fn terminal_resize(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if cols == 0 || rows == 0 {
        return Ok(());
    }
    send_frame(
        &registry,
        attach_id,
        TerminalClientFrame::Resize { cols, rows },
    )
}

fn send_frame(
    registry: &State<'_, TerminalRegistry>,
    attach_id: u64,
    frame: TerminalClientFrame,
) -> Result<(), String> {
    let handles = registry.lock().map_err(|_| "terminal registry lock")?;
    let handle = handles
        .get(&attach_id)
        .ok_or_else(|| "unknown attach_id".to_string())?;
    handle
        .outbound
        .send(frame)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn terminal_detach(
    registry: State<'_, TerminalRegistry>,
    attach_id: u64,
) -> Result<(), String> {
    if let Some(handle) = registry
        .lock()
        .map_err(|_| "terminal registry lock")?
        .remove(&attach_id)
    {
        handle.cancel.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ws_request_with_bearer;
    use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

    #[test]
    fn protected_websocket_credential_stays_in_the_authorization_header() {
        let request = ws_request_with_bearer(
            "http://127.0.0.1:7419",
            "/v1/sessions/shell/session-1",
            Some("home-secret"),
        )
        .expect("authenticated websocket request");
        assert_eq!(
            request.uri(),
            "ws://127.0.0.1:7419/v1/sessions/shell/session-1"
        );
        assert_eq!(
            request.headers().get(AUTHORIZATION).unwrap(),
            "Bearer home-secret"
        );
        assert!(!request.uri().to_string().contains("home-secret"));
    }
}
