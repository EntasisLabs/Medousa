use std::env;

#[derive(Clone)]
pub struct McpGatewayConfig {
    pub bind: String,
    pub gateway_token: Option<String>,
    pub admin_token: Option<String>,
    pub invokes_enabled: bool,
}

impl McpGatewayConfig {
    pub fn from_env_and_args(args: &[String]) -> Self {
        let bind = find_arg_value(args, "--bind")
            .unwrap_or_else(|| env::var("MEDOUSA_MCP_GATEWAY_BIND").unwrap_or_else(|_| {
                crate::mcp_gateway_api::DEFAULT_MCP_GATEWAY_BIND.to_string()
            }));

        Self {
            bind,
            gateway_token: resolve_mcp_gateway_token(),
            admin_token: resolve_mcp_gateway_admin_token(),
            invokes_enabled: !args.iter().any(|arg| arg == "--invokes-disabled"),
        }
    }
}

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

fn find_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
