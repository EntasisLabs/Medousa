//! Language-server backends: in-process Grapheme and external stdio LSPs.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

use crate::registry::{ServerKind, ServerLaunchSpec};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("failed to spawn language server: {0}")]
    Spawn(String),
    #[error("language server IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("language server closed")]
    Closed,
}

#[async_trait]
pub trait LanguageServerBackend: Send + Sync {
    async fn write_message(&self, json_body: &str) -> Result<(), BackendError>;
    async fn read_message(&self) -> Result<String, BackendError>;
    async fn shutdown(&self);
}

/// Spawn a backend for the given launch spec.
pub async fn spawn_backend(
    spec: &ServerLaunchSpec,
) -> Result<Arc<dyn LanguageServerBackend>, BackendError> {
    match &spec.kind {
        ServerKind::Grapheme => Ok(Arc::new(GraphemeBackend::spawn())),
        ServerKind::Stdio { command } => {
            Ok(Arc::new(StdioBackend::spawn(command, &spec.args).await?))
        }
    }
}

fn data_dir_bin() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("MEDOUSA_DATA_DIR") {
        return Some(PathBuf::from(explicit).join("bin"));
    }
    let base = dirs::data_local_dir()?.join("medousa").join("bin");
    Some(base)
}

fn resolve_command(command: &str) -> PathBuf {
    let as_path = PathBuf::from(command);
    if as_path.is_absolute() && as_path.is_file() {
        return as_path;
    }
    if let Some(bin) = data_dir_bin() {
        let candidate = bin.join(command);
        if candidate.is_file() {
            return candidate;
        }
        #[cfg(windows)]
        {
            let exe = bin.join(format!("{command}.exe"));
            if exe.is_file() {
                return exe;
            }
        }
    }
    as_path
}

async fn write_lsp_framed<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    json_body: &str,
) -> Result<(), BackendError> {
    let header = format!("Content-Length: {}\r\n\r\n", json_body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(json_body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn read_lsp_framed<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
) -> Result<String, BackendError> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(BackendError::Closed);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                rest.trim()
                    .parse()
                    .map_err(|e| BackendError::Spawn(format!("bad Content-Length: {e}")))?,
            );
        }
    }
    let len = content_length.ok_or_else(|| BackendError::Spawn("missing Content-Length".into()))?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    String::from_utf8(buf).map_err(|e| BackendError::Spawn(format!("invalid utf8: {e}")))
}

pub struct GraphemeBackend {
    stdin: Mutex<tokio::io::WriteHalf<tokio::io::DuplexStream>>,
    stdout: Mutex<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
}

impl GraphemeBackend {
    pub fn spawn() -> Self {
        let (client_to_lsp, lsp_stdin) = tokio::io::duplex(1024 * 1024);
        let (lsp_stdout, lsp_to_client) = tokio::io::duplex(1024 * 1024);
        tokio::spawn(grapheme_lsp::run_server(lsp_stdin, lsp_stdout));
        let (_client_read, client_write) = tokio::io::split(client_to_lsp);
        let (server_read, _unused_write) = tokio::io::split(lsp_to_client);
        drop(_client_read);
        drop(_unused_write);
        Self {
            stdin: Mutex::new(client_write),
            stdout: Mutex::new(BufReader::new(server_read)),
        }
    }
}

#[async_trait]
impl LanguageServerBackend for GraphemeBackend {
    async fn write_message(&self, json_body: &str) -> Result<(), BackendError> {
        let mut stdin = self.stdin.lock().await;
        write_lsp_framed(&mut *stdin, json_body).await
    }

    async fn read_message(&self) -> Result<String, BackendError> {
        let mut stdout = self.stdout.lock().await;
        read_lsp_framed(&mut *stdout).await
    }

    async fn shutdown(&self) {
        // Duplex drops when Arc released; grapheme task ends on EOF.
    }
}

pub struct StdioBackend {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
}

impl StdioBackend {
    pub async fn spawn(command: &str, args: &[String]) -> Result<Self, BackendError> {
        let resolved = resolve_command(command);
        let mut child = Command::new(&resolved)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| BackendError::Spawn(format!("{}: {e}", resolved.display())))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BackendError::Spawn("missing stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::Spawn("missing stdout".into()))?;
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
        })
    }
}

#[async_trait]
impl LanguageServerBackend for StdioBackend {
    async fn write_message(&self, json_body: &str) -> Result<(), BackendError> {
        let mut stdin = self.stdin.lock().await;
        write_lsp_framed(&mut *stdin, json_body).await
    }

    async fn read_message(&self) -> Result<String, BackendError> {
        let mut stdout = self.stdout.lock().await;
        read_lsp_framed(&mut *stdout).await
    }

    async fn shutdown(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }
}
