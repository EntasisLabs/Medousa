//! Authenticated native WebSocket bridge for the Code editor's LSP transport.
//!
//! WebView WebSockets cannot attach the workshop bearer credential. Keep the
//! credential native and forward only LSP payloads across Tauri IPC.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{Notify, mpsc};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

use crate::daemon::DaemonState;

static NEXT_ATTACH_ID: AtomicU64 = AtomicU64::new(1);

pub type CodeLspTransportRegistry = Arc<Mutex<HashMap<u64, CodeLspTransportHandle>>>;

pub struct CodeLspTransportHandle {
    outbound: mpsc::UnboundedSender<String>,
    ready: Arc<Notify>,
    cancel: CancellationToken,
}

#[derive(Debug, Serialize)]
pub struct CodeLspAttachResponse {
    pub attach_id: u64,
}

#[derive(Clone, Debug, Serialize)]
struct CodeLspMessage {
    attach_id: u64,
    data: String,
}

#[derive(Clone, Debug, Serialize)]
struct CodeLspStatus {
    attach_id: u64,
    connected: bool,
    code: u16,
    reason: String,
    clean: bool,
}

fn validate_lsp_path(path: &str) -> Result<(), String> {
    let route = path.split('?').next().unwrap_or(path);
    if matches!(route, "/v1/code/lsp" | "/v1/grapheme/lsp") {
        return Ok(());
    }
    Err("unsupported Code language WebSocket path".into())
}

#[tauri::command]
pub async fn code_lsp_attach(
    app: AppHandle,
    _state: State<'_, DaemonState>,
    registry: State<'_, CodeLspTransportRegistry>,
    path: String,
) -> Result<CodeLspAttachResponse, String> {
    validate_lsp_path(&path)?;
    let request = crate::terminal::authenticated_ws_request(&path)?;
    let (websocket, _) = connect_async(request)
        .await
        .map_err(|error| error.to_string())?;

    let attach_id = NEXT_ATTACH_ID.fetch_add(1, Ordering::SeqCst);
    let (outbound, outbound_rx) = mpsc::unbounded_channel();
    let ready = Arc::new(Notify::new());
    let cancel = CancellationToken::new();
    registry
        .lock()
        .map_err(|_| "Code LSP transport registry lock")?
        .insert(
            attach_id,
            CodeLspTransportHandle {
                outbound,
                ready: Arc::clone(&ready),
                cancel: cancel.clone(),
            },
        );
    tauri::async_runtime::spawn(run_transport(
        attach_id,
        app,
        websocket,
        outbound_rx,
        ready,
        cancel,
    ));
    Ok(CodeLspAttachResponse { attach_id })
}

async fn run_transport<S>(
    attach_id: u64,
    app: AppHandle,
    websocket: tokio_tungstenite::WebSocketStream<S>,
    mut outbound: mpsc::UnboundedReceiver<String>,
    ready: Arc<Notify>,
    cancel: CancellationToken,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::select! {
        _ = ready.notified() => {}
        _ = cancel.cancelled() => return,
    }
    let (mut ws_tx, mut ws_rx) = websocket.split();
    let _ = app.emit(
        "code-lsp-status",
        CodeLspStatus {
            attach_id,
            connected: true,
            code: 0,
            reason: String::new(),
            clean: true,
        },
    );

    let (code, reason, clean) = loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = ws_tx.close().await;
                return;
            }
            message = outbound.recv() => {
                let Some(message) = message else {
                    let _ = ws_tx.close().await;
                    return;
                };
                if let Err(error) = ws_tx.send(Message::Text(message.into())).await {
                    break (1006, error.to_string(), false);
                }
            }
            message = ws_rx.next() => {
                match message {
                    Some(Ok(Message::Text(data))) => {
                        let _ = app.emit(
                            "code-lsp-message",
                            CodeLspMessage { attach_id, data: data.to_string() },
                        );
                    }
                    Some(Ok(Message::Binary(data))) => match String::from_utf8(data.to_vec()) {
                        Ok(data) => {
                            let _ = app.emit(
                                "code-lsp-message",
                                CodeLspMessage { attach_id, data },
                            );
                        }
                        Err(_) => break (1003, "language server sent non-text data".into(), false),
                    },
                    Some(Ok(Message::Close(frame))) => {
                        let (code, reason) = frame
                            .map(|frame| (u16::from(frame.code), frame.reason.to_string()))
                            .unwrap_or((1000, String::new()));
                        break (code, reason, true);
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(error) = ws_tx.send(Message::Pong(payload)).await {
                            break (1006, error.to_string(), false);
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => break (1006, error.to_string(), false),
                    None => break (1006, "language server connection closed".into(), false),
                }
            }
        }
    };

    let _ = app.emit(
        "code-lsp-status",
        CodeLspStatus {
            attach_id,
            connected: false,
            code,
            reason,
            clean,
        },
    );
}

#[tauri::command]
pub fn code_lsp_ready(
    registry: State<'_, CodeLspTransportRegistry>,
    attach_id: u64,
) -> Result<(), String> {
    let registry = registry
        .lock()
        .map_err(|_| "Code LSP transport registry lock")?;
    let handle = registry
        .get(&attach_id)
        .ok_or_else(|| "Code LSP transport is not attached".to_string())?;
    handle.ready.notify_one();
    Ok(())
}

#[tauri::command]
pub fn code_lsp_send(
    registry: State<'_, CodeLspTransportRegistry>,
    attach_id: u64,
    message: String,
) -> Result<(), String> {
    let registry = registry
        .lock()
        .map_err(|_| "Code LSP transport registry lock")?;
    let handle = registry
        .get(&attach_id)
        .ok_or_else(|| "Code LSP transport is not attached".to_string())?;
    handle
        .outbound
        .send(message)
        .map_err(|_| "Code LSP transport is closed".to_string())
}

#[tauri::command]
pub fn code_lsp_detach(
    registry: State<'_, CodeLspTransportRegistry>,
    attach_id: u64,
) -> Result<(), String> {
    if let Some(handle) = registry
        .lock()
        .map_err(|_| "Code LSP transport registry lock")?
        .remove(&attach_id)
    {
        handle.cancel.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_lsp_path;

    #[test]
    fn only_declared_language_websockets_can_use_the_native_bridge() {
        assert!(validate_lsp_path("/v1/code/lsp?language=rust").is_ok());
        assert!(validate_lsp_path("/v1/grapheme/lsp").is_ok());
        assert!(validate_lsp_path("/v1/sessions/shell/example").is_err());
        assert!(validate_lsp_path("https://example.com/v1/code/lsp").is_err());
    }
}
