//! WebSocket bridge between Home CodeMirror LSP client and in-process `grapheme-lsp`.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::grapheme_script::store::GraphemeScriptStore;

#[derive(Debug, Clone, Serialize)]
pub struct GraphemeLspWorkspaceResponse {
    pub root_path: String,
    pub root_uri: String,
    pub scripts_dir: String,
}

pub fn lsp_workspace_response() -> GraphemeLspWorkspaceResponse {
    let root = GraphemeScriptStore::root_dir();
    let root_path = root.to_string_lossy().into_owned();
    let scripts_dir = root.join("scripts").to_string_lossy().into_owned();
    GraphemeLspWorkspaceResponse {
        root_uri: path_to_file_uri(&root),
        root_path,
        scripts_dir,
    }
}

pub async fn get_lsp_workspace() -> Json<GraphemeLspWorkspaceResponse> {
    Json(lsp_workspace_response())
}

pub async fn grapheme_lsp_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_lsp_socket)
}

async fn handle_lsp_socket(socket: WebSocket) {
    let (client_to_lsp, lsp_stdin) = tokio::io::duplex(1024 * 1024);
    let (lsp_stdout, lsp_to_client) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(grapheme_lsp::run_server(lsp_stdin, lsp_stdout));

    let (_, mut lsp_stdin_writer) = tokio::io::split(client_to_lsp);
    let (mut lsp_stdout_reader, _) = tokio::io::split(lsp_to_client);

    let (mut ws_tx, mut ws_rx) = socket.split();

    let read_lsp = tokio::spawn(async move {
        loop {
            match read_lsp_message(&mut lsp_stdout_reader).await {
                Ok(Some(body)) => {
                    if ws_tx.send(Message::Text(body.into())).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });

    while let Some(msg) = ws_rx.next().await {
        let Ok(msg) = msg else { break };
        match msg {
            Message::Text(text) => {
                if write_lsp_message(&mut lsp_stdin_writer, &text).await.is_err() {
                    break;
                }
            }
            Message::Binary(bytes) => {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    if write_lsp_message(&mut lsp_stdin_writer, text).await.is_err() {
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    read_lsp.abort();
}

async fn read_lsp_message(
    reader: &mut tokio::io::ReadHalf<tokio::io::DuplexStream>,
) -> std::io::Result<Option<String>> {
    let mut header = Vec::with_capacity(64);
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).await?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > 8192 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "LSP header too large",
            ));
        }
    }

    let header_str = String::from_utf8_lossy(&header);
    let content_length = header_str
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length:").map(str::trim))
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
        })?;

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await?;
    Ok(Some(String::from_utf8(body).map_err(|err| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, err)
    })?))
}

async fn write_lsp_message(
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    body: &str,
) -> std::io::Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await
}

pub fn path_to_file_uri(path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut normalized = canonical.to_string_lossy().replace('\\', "/");
    if !normalized.starts_with('/') {
        normalized = format!("/{normalized}");
    }
    format!("file://{normalized}")
}
