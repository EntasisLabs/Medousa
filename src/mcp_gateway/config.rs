use std::env;

pub use super::server_config::McpGatewayFullConfig;

pub fn resolve_mcp_gateway_token() -> Option<String> {
    env::var("MEDOUSA_MCP_GATEWAY_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_mcp_gateway_admin_token() -> Option<String> {
    env::var("MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_mcp_policy_token() -> Option<String> {
    env::var("MEDOUSA_MCP_POLICY_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

impl McpGatewayFullConfig {
    pub fn from_env_and_args_legacy(args: &[String]) -> Self {
        Self::from_env_and_args(args)
    }
}
