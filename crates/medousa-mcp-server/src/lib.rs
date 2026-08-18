//! Medousa MCP server — expose workshop *space* to external agent runtimes.
//!
//! Read-oriented vault / calendar / artifacts tools talk to the local daemon.
//! Writes and orchestration tools are denied.

mod daemon;

use daemon::DaemonClient;
use serde_json::{Value, json};
use std::sync::OnceLock;

/// Tools that must never be registered on this server.
pub const DENIED_TOOL_PREFIXES: &[&str] = &[
    "cognition_spawn",
    "cognition_workshop",
    "cognition_turn",
    "interactive_turn",
    "host_orchestrat",
    "openshell",
];

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Allowlisted space tools (aligned with `mcp_export_allowlist` — no vault_write).
pub fn space_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "vault_list",
            title: "List vault notes",
            description: "List notes under the bound workshop vault (paths only).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prefix": { "type": "string", "description": "Optional path prefix" }
                }
            }),
        },
        ToolSpec {
            name: "vault_read",
            title: "Read vault note",
            description: "Read a vault markdown note by relative path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "vault_search",
            title: "Search vault",
            description: "Grep/search vault note bodies.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "calendar_list",
            title: "List calendar events",
            description: "List calendar events in a time range for the bound workshop.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" }
                }
            }),
        },
        ToolSpec {
            name: "artifacts_list",
            title: "List artifacts",
            description: "List workshop artifacts.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "limit": { "type": "integer" },
                    "query": { "type": "string" }
                }
            }),
        },
        ToolSpec {
            name: "artifacts_fetch",
            title: "Fetch artifact",
            description: "Fetch an artifact by id (optional session_id).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "session_id": { "type": "string" }
                },
                "required": ["id"]
            }),
        },
    ]
}

pub fn is_denied_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "vault_write" {
        return true;
    }
    DENIED_TOOL_PREFIXES
        .iter()
        .any(|p| lower.starts_with(p) || lower.contains(p))
}

pub fn tools_list_result() -> Value {
    let tools: Vec<Value> = space_tools()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "inputSchema": t.input_schema,
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn tool_text(text: impl Into<String>, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": is_error
    })
}

fn daemon() -> Result<&'static DaemonClient, String> {
    static CLIENT: OnceLock<Result<DaemonClient, String>> = OnceLock::new();
    match CLIENT.get_or_init(DaemonClient::from_env) {
        Ok(client) => Ok(client),
        Err(err) => Err(err.clone()),
    }
}

/// Dispatch an allowlisted tool against the local daemon (or fail closed).
pub fn call_tool(name: &str, arguments: &Value) -> Result<Value, String> {
    if is_denied_tool(name) {
        return Err(format!("tool '{name}' is denied on medousa_mcp_server"));
    }
    if space_tools().iter().all(|t| t.name != name) {
        return Err(format!("unknown tool '{name}'"));
    }

    match name {
        "vault_list" => {
            let prefix = arguments.get("prefix").and_then(|v| v.as_str());
            let payload = daemon()?.vault_list(prefix)?;
            let text =
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
            Ok(tool_text(text, false))
        }
        "vault_read" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "path is required".to_string())?;
            let payload = daemon()?.vault_read(path)?;
            let body = payload
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| payload.get("body").and_then(|v| v.as_str()))
                .or_else(|| payload.get("markdown").and_then(|v| v.as_str()));
            if let Some(body) = body {
                Ok(tool_text(body.to_string(), false))
            } else {
                let text =
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
                Ok(tool_text(text, false))
            }
        }
        "vault_search" => {
            let query = arguments
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "query is required".to_string())?;
            let payload = daemon()?.vault_search(query)?;
            let text =
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
            Ok(tool_text(text, false))
        }
        "calendar_list" => {
            let from = arguments.get("from").and_then(|v| v.as_str());
            let to = arguments.get("to").and_then(|v| v.as_str());
            let payload = daemon()?.calendar_list(from, to)?;
            let text =
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
            Ok(tool_text(text, false))
        }
        "artifacts_list" => {
            let session_id = arguments.get("session_id").and_then(|v| v.as_str());
            let query = arguments.get("query").and_then(|v| v.as_str());
            let limit = arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let payload = daemon()?.artifacts_list(session_id, limit, query)?;
            let text =
                serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
            Ok(tool_text(text, false))
        }
        "artifacts_fetch" => {
            let id = arguments
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id is required".to_string())?;
            let session_id = arguments.get("session_id").and_then(|v| v.as_str());
            let payload = daemon()?.artifacts_fetch(id, session_id)?;
            let body = payload.get("body").and_then(|v| v.as_str());
            if let Some(body) = body {
                Ok(tool_text(body.to_string(), false))
            } else {
                let text =
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
                Ok(tool_text(text, false))
            }
        }
        _ => Err(format!("unknown tool '{name}'")),
    }
}

/// Backward-compatible name used by older tests/call sites.
pub fn call_tool_stub(name: &str, arguments: &Value) -> Result<Value, String> {
    call_tool(name, arguments)
}

pub fn handle_jsonrpc(request: &Value) -> Option<Value> {
    let method = request.get("method")?.as_str()?;
    let id = request.get("id").cloned();

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "medousa-mcp-server",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "notifications/initialized" | "initialized" => None,
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": tools_list_result()
        })),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(json!({}));
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args) {
                Ok(result) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                })),
                Err(msg) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32000, "message": msg }
                })),
            }
        }
        "ping" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("method not found: {method}")
            }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_space_tools_without_write_or_spawn() {
        let listed = tools_list_result();
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"vault_read"));
        assert!(names.contains(&"vault_search"));
        assert!(names.contains(&"calendar_list"));
        assert!(!names.contains(&"vault_write"));
        assert!(!names.iter().any(|n| n.contains("spawn")));
        assert!(is_denied_tool("cognition_spawn_turn_worker"));
        assert!(is_denied_tool("cognition_workshop_mutate"));
        assert!(is_denied_tool("vault_write"));
        assert!(call_tool("cognition_spawn_turn_worker", &json!({})).is_err());
        assert!(call_tool("cognition_workshop_mutate", &json!({})).is_err());
        assert!(call_tool("vault_write", &json!({"path":"x","content":"y"})).is_err());
    }

    #[test]
    fn initialize_handshake() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let res = handle_jsonrpc(&req).unwrap();
        assert_eq!(res["result"]["serverInfo"]["name"], "medousa-mcp-server");
    }

    #[test]
    fn calendar_and_artifacts_tools_are_registered() {
        let listed = tools_list_result();
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"calendar_list"));
        assert!(names.contains(&"artifacts_list"));
        assert!(names.contains(&"artifacts_fetch"));
    }
}
