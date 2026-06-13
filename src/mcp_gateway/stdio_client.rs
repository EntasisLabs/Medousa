//! Minimal MCP JSON-RPC client over stdio (initialize, tools/list, tools/call).

use std::process::Stdio;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::{Duration, timeout};

#[derive(Debug, Clone)]
pub struct McpToolDefinition {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
}

pub struct StdioMcpSession {
    child: Child,
    stdin: ChildStdin,
    lines: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
    request_timeout: Duration,
}

impl StdioMcpSession {
    pub async fn spawn(command: &str, args: &[String], request_timeout: Duration) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn MCP server command '{command}'"))?;

        let stdin = child
            .stdin
            .take()
            .context("MCP server stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("MCP server stdout unavailable")?;

        let mut session = Self {
            child,
            stdin,
            lines: BufReader::new(stdout),
            next_id: 1,
            request_timeout,
        };
        session.initialize().await?;
        Ok(session)
    }

    async fn initialize(&mut self) -> Result<()> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "medousa-mcp-gateway",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        self.send_request(&request).await?;
        let _ = self.read_response_for_id(id).await?;

        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.send_request(&initialized).await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        });
        self.send_request(&request).await?;
        let response = self.read_response_for_id(id).await?;
        parse_tool_list(&response)
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });
        self.send_request(&request).await?;
        let response = self.read_response_for_id(id).await?;
        if let Some(error) = response.get("error") {
            bail!("MCP tools/call error: {error}");
        }
        Ok(response
            .get("result")
            .cloned()
            .unwrap_or_else(|| json!({})))
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn send_request(&mut self, payload: &Value) -> Result<()> {
        let line = serde_json::to_string(payload)?;
        timeout(
            self.request_timeout,
            async {
                self.stdin.write_all(line.as_bytes()).await?;
                self.stdin.write_all(b"\n").await?;
                self.stdin.flush().await?;
                Ok::<(), std::io::Error>(())
            },
        )
        .await
        .context("MCP request timed out")??;
        Ok(())
    }

    async fn read_response_for_id(&mut self, id: u64) -> Result<Value> {
        timeout(self.request_timeout, self.read_response_for_id_inner(id))
            .await
            .context("MCP response timed out")?
    }

    async fn read_response_for_id_inner(&mut self, id: u64) -> Result<Value> {
        loop {
            let mut line = String::new();
            let read = self.lines.read_line(&mut line).await?;
            if read == 0 {
                bail!("MCP server closed stdout before response");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value =
                serde_json::from_str(trimmed).context("invalid JSON line from MCP server")?;
            if message.get("method").is_some() {
                continue;
            }
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    bail!("MCP error: {error}");
                }
                return Ok(message);
            }
        }
    }
}

impl Drop for StdioMcpSession {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub(crate) fn parse_tool_list(response: &Value) -> Result<Vec<McpToolDefinition>> {
    let tools = response
        .get("result")
        .and_then(|value| value.get("tools"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(tools
        .into_iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.to_string();
            let title = tool
                .get("title")
                .or_else(|| tool.get("name"))
                .and_then(|value| value.as_str())
                .unwrap_or(name.as_str())
                .to_string();
            Some(McpToolDefinition {
                name,
                title,
                description: tool
                    .get("description")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                input_schema: tool.get("inputSchema").cloned(),
            })
        })
        .collect())
}
