//! MCP gateway config helpers re-exported for daemon and launcher.
pub use medousa_mcp_gateway::{
    gateway_config_path, install_starter_gateway_config_if_missing,
    resolve_mcp_gateway_admin_token, resolve_mcp_gateway_token, resolve_mcp_policy_token,
    verify_admin_bearer, verify_gateway_bearer, verify_policy_bearer, STARTER_MCP_GATEWAY_TOML,
    McpGatewayFullConfig, McpServerConfig,
};
