//! Medousa MCP server bones.
//!
//! Exposes Medousa *space* (vault / calendar / artifacts) to external agentic
//! runtimes. Does **not** expose spawn/turn/worker orchestration.
//!
//! Full workshop I/O lands later; this crate ships the protocol surface + allowlist.

use serde_json::{Value, json};

/// Tools that must never be registered on this server.
pub const DENIED_TOOL_PREFIXES: &[&str] = &[
    "cognition_spawn",
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

/// Allowlisted space tools for 0.4.0 bones.
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
            name: "vault_write",
            title: "Write vault note",
            description: "Write or create a vault markdown note by relative path.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
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
                "properties": {}
            }),
        },
        ToolSpec {
            name: "artifacts_fetch",
            title: "Fetch artifact",
            description: "Fetch an artifact by id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"]
            }),
        },
    ]
}

pub fn is_denied_tool(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
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

/// Stub call handler — real vault/calendar I/O wires to daemon services later.
pub fn call_tool_stub(name: &str, _arguments: &Value) -> Result<Value, String> {
    if is_denied_tool(name) {
        return Err(format!("tool '{name}' is denied on medousa_mcp_server"));
    }
    if space_tools().iter().all(|t| t.name != name) {
        return Err(format!("unknown tool '{name}'"));
    }
    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!(
                "medousa_mcp_server bones: '{name}' is registered. Workshop I/O not wired yet."
            )
        }],
        "isError": false
    }))
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
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool_stub(name, &args) {
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
    fn lists_space_tools_and_denies_spawn() {
        let listed = tools_list_result();
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"vault_read"));
        assert!(names.contains(&"calendar_list"));
        assert!(!names.iter().any(|n| n.contains("spawn")));
        assert!(is_denied_tool("cognition_spawn_turn_worker"));
        assert!(call_tool_stub("cognition_spawn_turn_worker", &json!({})).is_err());
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
}
