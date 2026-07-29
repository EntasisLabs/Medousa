//! Opt-in coding domain cognition tools (Medousa is not a default coding agent).
//!
//! These tools unlock only when a session surface opts in (manuscript / Forge
//! work bind / Settings) — they are never in the default interactive palette.
//! `code_read` / `code_search` / `code_apply_patch` are rooted at the scripts
//! library or an explicit `root` under the workshop; `shell_session_*` drive
//! the workshop-owned PTY sessions on the daemon.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, StasisError};

pub const COGNITION_CODE_READ: &str = "cognition_code_read";
pub const COGNITION_CODE_SEARCH: &str = "cognition_code_search";
pub const COGNITION_CODE_APPLY_PATCH: &str = "cognition_code_apply_patch";
pub const COGNITION_SHELL_SESSION_STATUS: &str = "cognition_shell_session_status";
pub const COGNITION_SHELL_SESSION_RUN: &str = "cognition_shell_session_run";
pub const COGNITION_SHELL_SESSION_INTERRUPT: &str = "cognition_shell_session_interrupt";

pub const CODING_COGNITION_TOOLS: &[&str] = &[
    COGNITION_CODE_READ,
    COGNITION_CODE_SEARCH,
    COGNITION_CODE_APPLY_PATCH,
    COGNITION_SHELL_SESSION_STATUS,
    COGNITION_SHELL_SESSION_RUN,
    COGNITION_SHELL_SESSION_INTERRUPT,
];

pub fn is_coding_cognition_tool(name: &str) -> bool {
    CODING_COGNITION_TOOLS.contains(&name)
}

fn daemon_base() -> String {
    std::env::var("MEDOUSA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8741".into())
}

fn allowed_roots() -> Vec<PathBuf> {
    let mut roots = vec![crate::grapheme_script::store::GraphemeScriptStore::root_dir()];
    roots.extend(crate::daemon::shell_session_host::forge_worktree_roots_for_tools());
    roots
}

fn resolve_root(root: Option<&str>) -> StasisResult<PathBuf> {
    let base = match root.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => PathBuf::from(raw),
        None => crate::grapheme_script::store::GraphemeScriptStore::root_dir(),
    };
    let canon = base.canonicalize().unwrap_or(base);
    let allowed = allowed_roots();
    if !allowed.iter().any(|r| canon.starts_with(r)) {
        return Err(StasisError::PortFailure(format!(
            "root not under allowed workshop roots: {}",
            canon.display()
        )));
    }
    Ok(canon)
}

fn resolve_path(root: &Path, rel: &str) -> StasisResult<PathBuf> {
    let path = if Path::new(rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        root.join(rel)
    };
    let canon = path.canonicalize().unwrap_or(path);
    if !canon.starts_with(root) {
        return Err(StasisError::PortFailure(format!(
            "path escapes root: {}",
            canon.display()
        )));
    }
    Ok(canon)
}

async fn daemon_post(path: &str, body: Value) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}{path}",
        daemon_base().trim_end_matches('/')
    );
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("daemon session proxy: {e}")))?;
    let status = resp.status();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    if !status.is_success() {
        return Err(StasisError::PortFailure(format!("daemon {status}: {value}")));
    }
    Ok(value)
}

async fn daemon_get(path: &str) -> StasisResult<Value> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}{path}",
        daemon_base().trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| StasisError::PortFailure(format!("daemon session proxy: {e}")))?;
    let status = resp.status();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| StasisError::PortFailure(e.to_string()))?;
    if !status.is_success() {
        return Err(StasisError::PortFailure(format!("daemon {status}: {value}")));
    }
    Ok(value)
}

fn root_and_path(input: &Value) -> StasisResult<(PathBuf, PathBuf)> {
    let root = resolve_root(input.get("root").and_then(|v| v.as_str()))?;
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| StasisError::PortFailure("path is required".into()))?;
    let resolved = resolve_path(&root, path)?;
    Ok((root, resolved))
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

pub struct CognitionCodeReadTool;
pub struct CognitionCodeSearchTool;
pub struct CognitionCodeApplyPatchTool;
pub struct CognitionShellSessionStatusTool;
pub struct CognitionShellSessionRunTool;
pub struct CognitionShellSessionInterruptTool;

#[async_trait]
impl StasisTool for CognitionCodeReadTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_READ
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Read a text file under the scripts root or a Forge worktree. Coding domain only — opt-in via manuscript / work_id / Settings.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or root-relative file path" },
                "root": { "type": "string", "description": "Optional explicit root (default: scripts library)" }
            },
            "required": ["path"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let (root, path) = root_and_path(&input)?;
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StasisError::PortFailure(format!("read {}: {e}", path.display())))?;
        Ok(json!({
            "ok": true,
            "path": path.display().to_string(),
            "root": root.display().to_string(),
            "bytes": content.len(),
            "content": content,
        }))
    }
}

#[async_trait]
impl StasisTool for CognitionCodeSearchTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_SEARCH
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Search for a substring under the scripts root or a Forge worktree. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "root": { "type": "string" },
                "max_results": { "type": "integer" }
            },
            "required": ["query"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StasisError::PortFailure("query is required".into()))?;
        let root = resolve_root(input.get("root").and_then(|v| v.as_str()))?;
        let max = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .clamp(1, 500) as usize;

        let mut results = Vec::new();
        search_dir(&root, &root, query, max, &mut results)
            .map_err(|e| StasisError::PortFailure(e.to_string()))?;
        Ok(json!({ "ok": true, "root": root.display().to_string(), "query": query, "results": results }))
    }
}

fn search_dir(
    dir: &Path,
    root: &Path,
    query: &str,
    max: usize,
    out: &mut Vec<Value>,
) -> std::io::Result<()> {
    if out.len() >= max {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            search_dir(&path, root, query, max, out)?;
        } else if path.is_file() {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.contains(query) {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                let lines: Vec<usize> = content
                    .lines()
                    .enumerate()
                    .filter(|(_, l)| l.contains(query))
                    .map(|(i, _)| i + 1)
                    .take(5)
                    .collect();
                out.push(json!({
                    "path": rel.display().to_string(),
                    "lines": lines,
                }));
                if out.len() >= max {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

#[async_trait]
impl StasisTool for CognitionCodeApplyPatchTool {
    fn name(&self) -> &'static str {
        COGNITION_CODE_APPLY_PATCH
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Write full content or replace an exact snippet in a file under the session root / Forge worktree. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "root": { "type": "string" },
                "content": { "type": "string", "description": "Full file content (write)" },
                "find": { "type": "string", "description": "Exact snippet to replace" },
                "replace": { "type": "string", "description": "Replacement for `find`" }
            },
            "required": ["path"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let (root, path) = root_and_path(&input)?;
        if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| StasisError::PortFailure(e.to_string()))?;
            }
            tokio::fs::write(&path, content)
                .await
                .map_err(|e| StasisError::PortFailure(format!("write {}: {e}", path.display())))?;
            return Ok(json!({
                "ok": true,
                "mode": "write",
                "path": path.display().to_string(),
                "root": root.display().to_string(),
                "bytes": content.len(),
            }));
        }
        let find = input.get("find").and_then(|v| v.as_str());
        let replace = input.get("replace").and_then(|v| v.as_str());
        let (find, replace) = match (find, replace) {
            (Some(f), Some(r)) => (f, r),
            _ => {
                return Err(StasisError::PortFailure(
                    "provide `content` (write) or `find` + `replace` (patch)".into(),
                ));
            }
        };
        let existing = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StasisError::PortFailure(format!("read {}: {e}", path.display())))?;
        if !existing.contains(find) {
            return Err(StasisError::PortFailure(
                "find snippet not present in file".into(),
            ));
        }
        let next = existing.replacen(find, replace, 1);
        tokio::fs::write(&path, &next)
            .await
            .map_err(|e| StasisError::PortFailure(format!("write {}: {e}", path.display())))?;
        Ok(json!({
            "ok": true,
            "mode": "patch",
            "path": path.display().to_string(),
            "root": root.display().to_string(),
            "bytes": next.len(),
        }))
    }
}

#[async_trait]
impl StasisTool for CognitionShellSessionStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_SESSION_STATUS
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "List or create a workshop shell session (PTY). Coding domain only — sessions are shared by Home Terminal tabs and agents.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "work_id": { "type": "string", "description": "Optional Forge work id — session cwd binds to the worktree" },
                "create": { "type": "boolean", "description": "Create a session (default: list only)" }
            }
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let create = input
            .get("create")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let work_id = input
            .get("work_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        if create {
            daemon_post(
                "/v1/sessions/shell",
                json!({ "work_id": work_id, "cwd": Value::Null }),
            )
            .await
        } else {
            daemon_get("/v1/sessions/shell").await
        }
    }
}

#[async_trait]
impl StasisTool for CognitionShellSessionRunTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_SESSION_RUN
    }
    fn description(&self) -> Option<&'static str> {
        Some(
            "Write a command (or raw input) into a workshop shell session. Streams output for `wait_ms`. Coding domain only.",
        )
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Existing session id" },
                "work_id": { "type": "string", "description": "Create/bind a session for this Forge work id" },
                "command": { "type": "string", "description": "Command line to run (newline appended)" },
                "input": { "type": "string", "description": "Raw bytes to write (base64 not required)" },
                "wait_ms": { "type": "integer", "description": "How long to stream output (default 1500, max 15000)" }
            }
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = match input
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            Some(id) => id.to_string(),
            None => {
                let work_id = input
                    .get("work_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty());
                let created = daemon_post(
                    "/v1/sessions/shell",
                    json!({ "work_id": work_id, "cwd": Value::Null }),
                )
                .await?;
                created
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        StasisError::PortFailure("daemon did not return session_id".into())
                    })?
            }
        };
        let payload = if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
            format!("{cmd}\n")
        } else if let Some(raw) = input.get("input").and_then(|v| v.as_str()) {
            raw.to_string()
        } else {
            return Err(StasisError::PortFailure(
                "provide `command` or `input`".into(),
            ));
        };
        let wait_ms = input
            .get("wait_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(1500)
            .clamp(100, 15_000);

        let output = stream_session_input(&session_id, payload.as_bytes(), wait_ms).await?;
        Ok(json!({
            "ok": true,
            "session_id": session_id,
            "output": output,
        }))
    }
}

async fn stream_session_input(
    session_id: &str,
    input: &[u8],
    wait_ms: u64,
) -> StasisResult<String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let base = daemon_base().replacen("http", "ws", 1);
    let url = format!(
        "{}/v1/sessions/shell/{}",
        base.trim_end_matches('/'),
        urlencoding::encode(session_id)
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| StasisError::PortFailure(format!("session ws connect: {e}")))?;
    let frame = serde_json::json!({
        "type": "stdin",
        "data": base64::engine::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            input
        )
    })
    .to_string();
    ws.send(Message::Text(frame.into()))
        .await
        .map_err(|e| StasisError::PortFailure(format!("session ws send: {e}")))?;

    let mut output = String::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                if let Ok(v) = serde_json::from_str::<Value>(&text)
                    && v.get("type").and_then(|t| t.as_str()) == Some("stdout")
                    && let Some(data) = v.get("data").and_then(|d| d.as_str())
                    && let Ok(bytes) = base64::engine::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        data,
                    )
                {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                output.push_str(&String::from_utf8_lossy(&bytes));
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
            Ok(Some(Err(_))) => break,
            Ok(Some(Ok(_))) => {}
            Err(_) => break,
        }
    }
    let _ = ws.close(None).await;
    Ok(output)
}

#[async_trait]
impl StasisTool for CognitionShellSessionInterruptTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_SESSION_INTERRUPT
    }
    fn description(&self) -> Option<&'static str> {
        Some("Send SIGINT to a workshop shell session. Coding domain only.")
    }
    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" }
            },
            "required": ["session_id"]
        }))
    }
    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| StasisError::PortFailure("session_id is required".into()))?;
        daemon_post(
            &format!("/v1/sessions/shell/{session_id}/signal"),
            json!({ "signal": "interrupt" }),
        )
        .await
    }
}

pub fn register_coding_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionCodeReadTool)?;
    registry.register_tool(CognitionCodeSearchTool)?;
    registry.register_tool(CognitionCodeApplyPatchTool)?;
    registry.register_tool(CognitionShellSessionStatusTool)?;
    registry.register_tool(CognitionShellSessionRunTool)?;
    registry.register_tool(CognitionShellSessionInterruptTool)?;
    Ok(())
}
