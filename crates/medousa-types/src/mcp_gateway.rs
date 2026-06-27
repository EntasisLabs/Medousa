//! MCP gateway status contract (daemon → Home / mobile clients).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayHealthSnapshot {
    pub status: String,
    pub invokes_enabled: bool,
    pub registered_servers: u32,
    pub connected_servers: u32,
    pub catalog_entries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayServerRuntime {
    pub server_id: String,
    pub title: String,
    pub enabled: bool,
    pub connected: bool,
    pub tool_count: u32,
    pub allowed_lanes: Vec<String>,
}

/// Daemon-proxied MCP gateway liveness + server runtime snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayStatusResponse {
    pub gateway_url: String,
    pub reachable: bool,
    pub message: String,
    pub health: Option<McpGatewayHealthSnapshot>,
    pub servers: Vec<McpGatewayServerRuntime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_response_uses_camel_case() {
        let json = serde_json::json!({
            "gatewayUrl": "http://127.0.0.1:7420",
            "reachable": true,
            "message": "ok",
            "health": {
                "status": "ok",
                "invokesEnabled": true,
                "registeredServers": 2,
                "connectedServers": 1,
                "catalogEntries": 5
            },
            "servers": []
        });
        let parsed: McpGatewayStatusResponse = serde_json::from_value(json).expect("parse");
        assert!(parsed.reachable);
        assert_eq!(parsed.health.as_ref().map(|h| h.catalog_entries), Some(5));
    }
}
