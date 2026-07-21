//! ACP client bones for hot-swappable agentic runtimes.
//!
//! The **daemon** owns this library and exposes `/v1/agents` via the Medousa SDK.
//! Clients never speak ACP directly.
//!
//! 0.4.0: stub session + Cursor/Codex process adapters (spawn when binary exists;
//! fall back to stub bridge events when ACP wire is unavailable).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex as AsyncMutex;

/// Built-in runtime ids for 0.4.0 bones QA.
pub const RUNTIME_CURSOR: &str = "cursor";
pub const RUNTIME_CODEX: &str = "codex";
pub const RUNTIME_MEDOUSA: &str = "medousa";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRuntimeKind {
    Medousa,
    Cursor,
    Codex,
}

impl AgentRuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Medousa => RUNTIME_MEDOUSA,
            Self::Cursor => RUNTIME_CURSOR,
            Self::Codex => RUNTIME_CODEX,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "medousa" | "native" => Some(Self::Medousa),
            "cursor" => Some(Self::Cursor),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpAgentConfig {
    pub kind: AgentRuntimeKind,
    /// Command to spawn (e.g. `agent`, `codex`). Empty = unset.
    pub command: String,
    pub args: Vec<String>,
    /// Working directory hint (workshop root).
    pub cwd: Option<String>,
}

impl AcpAgentConfig {
    pub fn cursor_default() -> Self {
        Self {
            kind: AgentRuntimeKind::Cursor,
            command: std::env::var("MEDOUSA_ACP_CURSOR_COMMAND")
                .unwrap_or_else(|_| "agent".into()),
            args: env_args("MEDOUSA_ACP_CURSOR_ARGS", &["acp"]),
            cwd: None,
        }
    }

    pub fn codex_default() -> Self {
        Self {
            kind: AgentRuntimeKind::Codex,
            command: std::env::var("MEDOUSA_ACP_CODEX_COMMAND")
                .unwrap_or_else(|_| "codex".into()),
            args: env_args("MEDOUSA_ACP_CODEX_ARGS", &["acp"]),
            cwd: None,
        }
    }
}

fn env_args(key: &str, default: &[&str]) -> Vec<String> {
    if let Ok(raw) = std::env::var(key) {
        let parts: Vec<String> = raw
            .split_whitespace()
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect();
        if !parts.is_empty() {
            return parts;
        }
    }
    default.iter().map(|s| (*s).to_string()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpSessionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpEvent {
    MessageDelta { text: String },
    MessageDone { text: String },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    PermissionRequest {
        id: String,
        summary: String,
    },
    Error { message: String },
    Done,
}

#[async_trait]
pub trait AcpClient: Send + Sync {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId>;
    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()>;
    async fn cancel(&self, session: &AcpSessionId) -> Result<()>;
    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>>;
}

struct StubSessionState {
    queue: Vec<AcpEvent>,
    cancelled: bool,
}

/// Placeholder client — proves wiring without requiring Cursor/Codex installed.
pub struct StubAcpClient {
    sessions: Mutex<HashMap<String, StubSessionState>>,
}

impl Default for StubAcpClient {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl StubAcpClient {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AcpClient for StubAcpClient {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId> {
        if matches!(config.kind, AgentRuntimeKind::Medousa) {
            bail!("use native Medousa turn path for medousa runtime");
        }
        let id = AcpSessionId(format!(
            "stub-{}-{}",
            config.kind.as_str(),
            uuid_v4_lite()
        ));
        let mut guard = self.sessions.lock().expect("stub sessions");
        guard.insert(
            id.0.clone(),
            StubSessionState {
                queue: Vec::new(),
                cancelled: false,
            },
        );
        Ok(id)
    }

    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()> {
        let mut guard = self.sessions.lock().expect("stub sessions");
        let state = guard
            .get_mut(&session.0)
            .ok_or_else(|| anyhow::anyhow!("unknown stub session {}", session.0))?;
        if state.cancelled {
            bail!("session cancelled");
        }
        let preview: String = text.chars().take(120).collect();
        state.queue.push(AcpEvent::MessageDelta {
            text: format!("[stub {}] ", session.0),
        });
        state.queue.push(AcpEvent::MessageDone {
            text: format!(
                "[medousa-acp-client] stub runtime acknowledged prompt ({} chars): {preview}",
                text.len()
            ),
        });
        // Demo permission pause once per prompt when MEDOUSA_ACP_STUB_PERMISSION=1
        if std::env::var("MEDOUSA_ACP_STUB_PERMISSION")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            state.queue.push(AcpEvent::PermissionRequest {
                id: format!("perm-{}", uuid_v4_lite()),
                summary: "Stub ACP permission: allow demo tool call?".into(),
            });
        }
        state.queue.push(AcpEvent::Done);
        Ok(())
    }

    async fn cancel(&self, session: &AcpSessionId) -> Result<()> {
        let mut guard = self.sessions.lock().expect("stub sessions");
        if let Some(state) = guard.get_mut(&session.0) {
            state.cancelled = true;
            state.queue.clear();
            state.queue.push(AcpEvent::Error {
                message: "cancelled".into(),
            });
            state.queue.push(AcpEvent::Done);
        }
        Ok(())
    }

    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>> {
        let mut guard = self.sessions.lock().expect("stub sessions");
        let state = guard
            .get_mut(&session.0)
            .ok_or_else(|| anyhow::anyhow!("unknown stub session {}", session.0))?;
        Ok(if state.queue.is_empty() {
            None
        } else {
            Some(state.queue.remove(0))
        })
    }
}

fn uuid_v4_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// Resolve a configured external runtime (Cursor / Codex only in 0.4.0).
pub fn external_runtime_config(kind: AgentRuntimeKind) -> Result<AcpAgentConfig> {
    match kind {
        AgentRuntimeKind::Cursor => Ok(AcpAgentConfig::cursor_default()),
        AgentRuntimeKind::Codex => Ok(AcpAgentConfig::codex_default()),
        AgentRuntimeKind::Medousa => {
            bail!("medousa runtime is not an ACP external agent")
        }
    }
}

/// True when `command` appears resolvable on PATH (or is an absolute existing file).
pub fn command_available(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() || trimmed.contains('/') {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(trimmed);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

pub fn runtime_availability(kind: AgentRuntimeKind) -> (bool, Option<String>, Option<String>) {
    match kind {
        AgentRuntimeKind::Medousa => (
            true,
            None,
            Some("Use /v1/turns for native Medousa agent loop".into()),
        ),
        AgentRuntimeKind::Cursor | AgentRuntimeKind::Codex => {
            let cfg = match external_runtime_config(kind) {
                Ok(c) => c,
                Err(err) => return (false, None, Some(err.to_string())),
            };
            let available = command_available(&cfg.command);
            let detail = if available {
                Some(format!("command '{}' found on PATH", cfg.command))
            } else {
                Some(format!(
                    "command '{}' not found — sessions use stub bridge until installed",
                    cfg.command
                ))
            };
            (true, Some(cfg.command), detail)
        }
    }
}

struct ProcessSession {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<tokio::process::ChildStdout>>,
    next_id: u64,
    queue: Vec<AcpEvent>,
    cancelled: bool,
}

/// Spawns Cursor/Codex ACP stdio when the binary exists; otherwise behaves like [`StubAcpClient`].
pub struct ExternalAcpClient {
    stub: StubAcpClient,
    processes: AsyncMutex<HashMap<String, ProcessSession>>,
}

impl Default for ExternalAcpClient {
    fn default() -> Self {
        Self {
            stub: StubAcpClient::new(),
            processes: AsyncMutex::new(HashMap::new()),
        }
    }
}

impl ExternalAcpClient {
    pub fn new() -> Self {
        Self::default()
    }

    fn prefer_process() -> bool {
        !std::env::var("MEDOUSA_ACP_FORCE_STUB")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
}

#[async_trait]
impl AcpClient for ExternalAcpClient {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId> {
        if matches!(config.kind, AgentRuntimeKind::Medousa) {
            bail!("use native Medousa turn path for medousa runtime");
        }
        if Self::prefer_process() && command_available(&config.command) {
            match spawn_acp_process(config).await {
                Ok(mut proc) => {
                    let id = AcpSessionId(format!(
                        "acp-{}-{}",
                        config.kind.as_str(),
                        uuid_v4_lite()
                    ));
                    // Best-effort initialize; queue bridge note either way.
                    let _ = send_initialize(&mut proc).await;
                    proc.queue.push(AcpEvent::MessageDone {
                        text: format!(
                            "[medousa-acp-client] spawned {} ({}) — ACP session ready (bones)",
                            config.command, config.kind.as_str()
                        ),
                    });
                    self.processes.lock().await.insert(id.0.clone(), proc);
                    return Ok(id);
                }
                Err(err) => {
                    tracing::warn!(error = %err, "ACP process spawn failed; using stub");
                }
            }
        }
        self.stub.create_session(config).await
    }

    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(proc) = guard.get_mut(&session.0) {
                if proc.cancelled {
                    bail!("session cancelled");
                }
                match send_prompt(proc, text).await {
                    Ok(()) => {
                        proc.queue.push(AcpEvent::MessageDone {
                            text: format!(
                                "[medousa-acp-client] prompt delivered to process ({} chars)",
                                text.len()
                            ),
                        });
                        proc.queue.push(AcpEvent::Done);
                        return Ok(());
                    }
                    Err(err) => {
                        proc.queue.push(AcpEvent::Error {
                            message: format!("ACP prompt failed: {err}"),
                        });
                        proc.queue.push(AcpEvent::Done);
                        return Ok(());
                    }
                }
            }
        }
        self.stub.prompt(session, text).await
    }

    async fn cancel(&self, session: &AcpSessionId) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(mut proc) = guard.remove(&session.0) {
                proc.cancelled = true;
                let _ = proc.child.kill().await;
                return Ok(());
            }
        }
        self.stub.cancel(session).await
    }

    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(proc) = guard.get_mut(&session.0) {
                if !proc.queue.is_empty() {
                    return Ok(Some(proc.queue.remove(0)));
                }
                // Non-blocking poll of stdout for notifications
                if let Ok(Some(ev)) = try_read_notification(proc).await {
                    return Ok(Some(ev));
                }
                return Ok(None);
            }
        }
        self.stub.next_event(session).await
    }
}

async fn spawn_acp_process(config: &AcpAgentConfig) -> Result<ProcessSession> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &config.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn()?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdout"))?;
    Ok(ProcessSession {
        child,
        stdin,
        lines: BufReader::new(stdout).lines(),
        next_id: 1,
        queue: Vec::new(),
        cancelled: false,
    })
}

async fn send_initialize(proc: &mut ProcessSession) -> Result<()> {
    let id = proc.next_id;
    proc.next_id += 1;
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "0.1.0",
            "clientInfo": {
                "name": "medousa-acp-client",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    });
    write_line(proc, &req).await?;
    let _ = read_line_timeout(proc, 2).await;
    Ok(())
}

async fn send_prompt(proc: &mut ProcessSession, text: &str) -> Result<()> {
    let id = proc.next_id;
    proc.next_id += 1;
    // Prefer session/prompt; also try prompt for looser adapters.
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/prompt",
        "params": { "prompt": text }
    });
    write_line(proc, &req).await
}

async fn write_line(proc: &mut ProcessSession, value: &Value) -> Result<()> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    proc.stdin.write_all(line.as_bytes()).await?;
    proc.stdin.flush().await?;
    Ok(())
}

async fn read_line_timeout(proc: &mut ProcessSession, secs: u64) -> Result<Option<String>> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(secs),
        proc.lines.next_line(),
    )
    .await;
    match result {
        Ok(Ok(line)) => Ok(line),
        Ok(Err(err)) => Err(err.into()),
        Err(_) => Ok(None),
    }
}

async fn try_read_notification(proc: &mut ProcessSession) -> Result<Option<AcpEvent>> {
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        proc.lines.next_line(),
    )
    .await;
    let Ok(Ok(Some(line))) = result else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(&line)?;
    if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
        if method.contains("permission") {
            let id = value
                .pointer("/params/id")
                .and_then(|v| v.as_str())
                .unwrap_or("permission")
                .to_string();
            let summary = value
                .pointer("/params/summary")
                .or_else(|| value.pointer("/params/message"))
                .and_then(|v| v.as_str())
                .unwrap_or("ACP permission requested")
                .to_string();
            return Ok(Some(AcpEvent::PermissionRequest { id, summary }));
        }
        if method.contains("message") || method.contains("update") {
            if let Some(text) = value
                .pointer("/params/text")
                .or_else(|| value.pointer("/params/content"))
                .and_then(|v| v.as_str())
            {
                return Ok(Some(AcpEvent::MessageDelta {
                    text: text.to_string(),
                }));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_session_roundtrip() {
        let client = StubAcpClient::new();
        let cfg = AcpAgentConfig::cursor_default();
        let session = client.create_session(&cfg).await.unwrap();
        assert!(session.0.starts_with("stub-cursor-"));
        client.prompt(&session, "hello").await.unwrap();
        let first = client.next_event(&session).await.unwrap();
        assert!(matches!(first, Some(AcpEvent::MessageDelta { .. })));
        let second = client.next_event(&session).await.unwrap();
        assert!(matches!(second, Some(AcpEvent::MessageDone { .. })));
    }

    #[test]
    fn parses_runtime_kinds() {
        assert_eq!(
            AgentRuntimeKind::parse("Cursor"),
            Some(AgentRuntimeKind::Cursor)
        );
        assert_eq!(
            AgentRuntimeKind::parse("codex"),
            Some(AgentRuntimeKind::Codex)
        );
        assert!(external_runtime_config(AgentRuntimeKind::Medousa).is_err());
    }

    #[tokio::test]
    async fn external_falls_back_to_stub() {
        let client = ExternalAcpClient::new();
        let mut cfg = AcpAgentConfig::cursor_default();
        cfg.command = "medousa-acp-missing-binary-xyz".into();
        let session = client.create_session(&cfg).await.unwrap();
        assert!(session.0.starts_with("stub-"));
    }
}
