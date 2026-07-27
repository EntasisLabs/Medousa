//! ACP client bones for hot-swappable agentic runtimes.
//!
//! The **daemon** owns this library and exposes `/v1/agents` via the Medousa SDK.
//! Clients never speak ACP directly.
//!
//! Stub session + Cursor/Codex process adapters (spawn when binary exists;
//! real `session/new` → `session/prompt` → `session/update` pump; stub fallback).

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
    /// Reply to an inbound `session/request_permission` JSON-RPC id.
    async fn respond_permission(
        &self,
        session: &AcpSessionId,
        request_id: &str,
        approved: bool,
    ) -> Result<()>;
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

    async fn respond_permission(
        &self,
        _session: &AcpSessionId,
        _request_id: &str,
        _approved: bool,
    ) -> Result<()> {
        Ok(())
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
    /// ACP wire session id from `session/new`.
    acp_session_id: Option<String>,
    cwd: Option<String>,
    /// Outstanding `session/prompt` JSON-RPC id waiting for stopReason.
    pending_prompt_id: Option<u64>,
    /// Permission JSON-RPC id → allow option ids (first is preferred).
    permission_allow_options: HashMap<String, Vec<String>>,
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
                    if let Err(err) = handshake_session(&mut proc).await {
                        tracing::warn!(error = %err, "ACP handshake failed; using stub");
                        let _ = proc.child.kill().await;
                    } else {
                        self.processes.lock().await.insert(id.0.clone(), proc);
                        return Ok(id);
                    }
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
                if let Err(err) = send_prompt(proc, text).await {
                    proc.queue.push(AcpEvent::Error {
                        message: format!("ACP prompt failed: {err}"),
                    });
                    proc.queue.push(AcpEvent::Done);
                }
                return Ok(());
            }
        }
        self.stub.prompt(session, text).await
    }

    async fn cancel(&self, session: &AcpSessionId) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(mut proc) = guard.remove(&session.0) {
                proc.cancelled = true;
                if let Some(acp_id) = proc.acp_session_id.clone() {
                    let _ = write_line(
                        &mut proc,
                        &json!({
                            "jsonrpc": "2.0",
                            "method": "session/cancel",
                            "params": { "sessionId": acp_id }
                        }),
                    )
                    .await;
                }
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
                return drain_stdout(proc).await;
            }
        }
        self.stub.next_event(session).await
    }

    async fn respond_permission(
        &self,
        session: &AcpSessionId,
        request_id: &str,
        approved: bool,
    ) -> Result<()> {
        {
            let mut guard = self.processes.lock().await;
            if let Some(proc) = guard.get_mut(&session.0) {
                return send_permission_response(proc, request_id, approved).await;
            }
        }
        self.stub
            .respond_permission(session, request_id, approved)
            .await
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
        acp_session_id: None,
        cwd: config.cwd.clone(),
        pending_prompt_id: None,
        permission_allow_options: HashMap::new(),
    })
}

async fn handshake_session(proc: &mut ProcessSession) -> Result<()> {
    send_initialize(proc).await?;
    send_session_new(proc).await?;
    Ok(())
}

async fn send_initialize(proc: &mut ProcessSession) -> Result<()> {
    let id = alloc_id(proc);
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": {
                "name": "medousa-acp-client",
                "version": env!("CARGO_PKG_VERSION")
            },
            "clientCapabilities": {
                "fs": { "readTextFile": true },
                "permission": true
            }
        }
    });
    write_line(proc, &req).await?;
    // Drain until we see the initialize response (or timeout).
    for _ in 0..20 {
        let Some(line) = read_line_timeout(proc, 2).await? else {
            break;
        };
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
                return Ok(());
            }
            // Buffer early notifications while waiting.
            if let Some(ev) = map_inbound_line(proc, &value) {
                proc.queue.push(ev);
            }
        }
    }
    Ok(())
}

async fn send_session_new(proc: &mut ProcessSession) -> Result<()> {
    let id = alloc_id(proc);
    let cwd = proc
        .cwd
        .clone()
        .or_else(|| std::env::current_dir().ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| ".".into());
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/new",
        "params": {
            "cwd": cwd,
            "mcpServers": []
        }
    });
    write_line(proc, &req).await?;
    for _ in 0..20 {
        let Some(line) = read_line_timeout(proc, 3).await? else {
            break;
        };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(|v| v.as_u64()) == Some(id) {
            let session_id = value
                .pointer("/result/sessionId")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            proc.acp_session_id = session_id;
            if proc.acp_session_id.is_none() {
                bail!("session/new response missing sessionId");
            }
            return Ok(());
        }
        if let Some(ev) = map_inbound_line(proc, &value) {
            proc.queue.push(ev);
        }
    }
    bail!("timed out waiting for session/new")
}

async fn send_prompt(proc: &mut ProcessSession, text: &str) -> Result<()> {
    let Some(session_id) = proc.acp_session_id.clone() else {
        bail!("ACP session has no sessionId — handshake incomplete");
    };
    let id = alloc_id(proc);
    proc.pending_prompt_id = Some(id);
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{ "type": "text", "text": text }]
        }
    });
    write_line(proc, &req).await
}

async fn send_permission_response(
    proc: &mut ProcessSession,
    request_id: &str,
    approved: bool,
) -> Result<()> {
    let rpc_id: Value = if let Ok(n) = request_id.parse::<u64>() {
        json!(n)
    } else if let Ok(n) = request_id.parse::<i64>() {
        json!(n)
    } else {
        json!(request_id)
    };
    let result = if approved {
        let option_id = proc
            .permission_allow_options
            .get(request_id)
            .and_then(|opts| opts.first())
            .cloned()
            .unwrap_or_else(|| "allow-once".to_string());
        json!({
            "outcome": {
                "outcome": "selected",
                "optionId": option_id
            }
        })
    } else {
        json!({ "outcome": { "outcome": "cancelled" } })
    };
    proc.permission_allow_options.remove(request_id);
    write_line(
        proc,
        &json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result
        }),
    )
    .await
}

fn alloc_id(proc: &mut ProcessSession) -> u64 {
    let id = proc.next_id;
    proc.next_id = proc.next_id.saturating_add(1);
    id
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

async fn drain_stdout(proc: &mut ProcessSession) -> Result<Option<AcpEvent>> {
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(80),
        proc.lines.next_line(),
    )
    .await;
    let Ok(Ok(Some(line))) = result else {
        return Ok(None);
    };
    let value: Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(map_inbound_line(proc, &value))
}

fn map_inbound_line(proc: &mut ProcessSession, value: &Value) -> Option<AcpEvent> {
    // JSON-RPC response to session/prompt → turn complete.
    if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
        if proc.pending_prompt_id == Some(id) {
            proc.pending_prompt_id = None;
            if let Some(err) = value.get("error") {
                let message = err
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ACP prompt error")
                    .to_string();
                // Emit Error first; Done follows on the next poll.
                proc.queue.push(AcpEvent::Done);
                return Some(AcpEvent::Error { message });
            }
            // Terminal: session/prompt completed (stopReason present or implied).
            let _stop = value
                .pointer("/result/stopReason")
                .and_then(|v| v.as_str())
                .unwrap_or("end_turn");
            return Some(AcpEvent::Done);
        }
        // Other RPC responses (initialize/session/new) — ignore here.
        return None;
    }

    let method = value.get("method").and_then(|m| m.as_str())?;
    if method == "session/request_permission" || method.ends_with("request_permission") {
        let rpc_id = value
            .get("id")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| format!("perm-{}", uuid_v4_lite()));
        let allow_ids: Vec<String> = value
            .pointer("/params/options")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|opt| {
                let kind = opt.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let id = opt.get("optionId").and_then(|v| v.as_str())?;
                if kind.starts_with("allow") || id.contains("allow") {
                    Some(id.to_string())
                } else {
                    None
                }
            })
            .collect();
        proc.permission_allow_options
            .insert(rpc_id.clone(), allow_ids);
        let title = value
            .pointer("/params/toolCall/title")
            .or_else(|| value.pointer("/params/toolCall/toolCallId"))
            .and_then(|v| v.as_str())
            .unwrap_or("tool call");
        let summary = format!("Allow ACP action: {title}?");
        return Some(AcpEvent::PermissionRequest {
            id: rpc_id,
            summary,
        });
    }

    if method == "session/update" {
        let update = value.pointer("/params/update")?;
        let kind = update
            .get("sessionUpdate")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match kind {
            "agent_message_chunk" | "agent_thought_chunk" | "user_message_chunk" => {
                let text = update
                    .pointer("/content/text")
                    .or_else(|| update.pointer("/content/content/text"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                if text.is_empty() {
                    return None;
                }
                return Some(AcpEvent::MessageDelta { text });
            }
            "tool_call" | "tool_call_update" => {
                let id = update
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string();
                let name = update
                    .get("title")
                    .or_else(|| update.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string();
                return Some(AcpEvent::ToolCall {
                    id,
                    name,
                    input: update.clone(),
                });
            }
            "state_update" => {
                let state = update.get("state").and_then(|v| v.as_str()).unwrap_or("");
                if state == "idle" {
                    return Some(AcpEvent::Done);
                }
            }
            _ => {}
        }
    }
    None
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
