//! MCP Client gateway — config, registry, HTTP handlers.

pub mod mcp_gateway;

pub use mcp_gateway::config::{
    resolve_mcp_gateway_admin_token, resolve_mcp_gateway_token, resolve_mcp_policy_token,
};
pub use mcp_gateway::connector;
pub use mcp_gateway::connector::{
    CatalogEntry, ConnectorManifest, ConnectorPlatform, InstallInput, InstallProbe, InstallReport,
    InstallStore, MOBILE_OAUTH_REDIRECT, OAuthBeginPlan, PreparedInstall, SuppliedOAuthClient,
    builtin_catalog, catalog_entry, commit_install, install_after_tools_list,
    parse_connector_manifest, plan_oauth_begin, prepare_install, probe_remote_tool_names,
};
pub use mcp_gateway::elicitation;
pub use mcp_gateway::server_config::{
    GatewaySection, McpGatewayFileConfig, McpGatewayFullConfig, McpServerConfig,
    McpServerOAuthConfig, gateway_config_path, validate_remote_server_url,
};
pub use mcp_gateway::starter_config::{
    STARTER_MCP_GATEWAY_TOML, install_starter_gateway_config_if_missing,
};
pub use mcp_gateway::{
    GatewayState, McpOAuthBeginRequest, McpOAuthBroker, McpOAuthBundleStore, McpOAuthError,
    McpPolicyEvaluator, SecureMcpOAuthBundleStore, ServerRegistry, build_router, serve,
    verify_admin_bearer, verify_gateway_bearer, verify_policy_bearer,
};
