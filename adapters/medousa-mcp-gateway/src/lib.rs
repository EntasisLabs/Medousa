//! MCP Client gateway — config, registry, HTTP handlers.

pub mod mcp_gateway;

pub use mcp_gateway::{
    build_router, serve, verify_admin_bearer, verify_gateway_bearer, verify_policy_bearer,
    GatewayState, ServerRegistry,
};
pub use mcp_gateway::config::{
    resolve_mcp_gateway_admin_token, resolve_mcp_gateway_token, resolve_mcp_policy_token,
};
pub use mcp_gateway::server_config::{gateway_config_path, McpGatewayFullConfig, McpServerConfig};
pub use mcp_gateway::starter_config::{
    install_starter_gateway_config_if_missing, STARTER_MCP_GATEWAY_TOML,
};
