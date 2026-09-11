use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;

use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpGatewayFileConfig {
    #[serde(default)]
    pub gateway: GatewaySection,
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySection {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_daemon_policy_url")]
    pub daemon_policy_url: String,
    #[serde(default = "default_max_invoke_ms")]
    pub max_invoke_duration_ms: u64,
    #[serde(default = "default_catalog_refresh_secs")]
    pub catalog_refresh_interval_secs: u64,
    #[serde(default = "default_true")]
    pub use_mock_fallback: bool,
}

impl Default for GatewaySection {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            daemon_policy_url: default_daemon_policy_url(),
            max_invoke_duration_ms: default_max_invoke_ms(),
            catalog_refresh_interval_secs: default_catalog_refresh_secs(),
            use_mock_fallback: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub bearer_token: Option<String>,
    /// Whether a bearer token exists in the typed secret store. The value is
    /// deliberately never serialized into this configuration object.
    #[serde(default)]
    pub bearer_token_configured: bool,
    #[serde(default = "default_allowed_lanes")]
    pub allowed_lanes: Vec<String>,
    #[serde(default = "default_allowed_effects")]
    pub allowed_effect_classes: Vec<String>,
    #[serde(default)]
    pub tool_tags: HashMap<String, Vec<String>>,
    /// Tools hidden from discovery and denied at invoke time. A denylist keeps
    /// newly-added server tools enabled until the user explicitly disables one.
    #[serde(default)]
    pub disabled_tools: Vec<String>,
    #[serde(default)]
    pub use_mock: bool,
}

impl McpServerConfig {
    pub fn tool_enabled(&self, tool_name: &str) -> bool {
        !self
            .disabled_tools
            .iter()
            .any(|disabled| disabled.eq_ignore_ascii_case(tool_name))
    }
}

#[derive(Debug, Clone)]
pub struct McpGatewayFullConfig {
    pub bind: String,
    pub gateway_token: Option<String>,
    pub admin_token: Option<String>,
    pub invokes_enabled: bool,
    pub daemon_policy_url: String,
    pub policy_token: Option<String>,
    pub max_invoke_duration_ms: u64,
    pub catalog_refresh_interval_secs: u64,
    pub use_mock_fallback: bool,
    pub servers: Vec<McpServerConfig>,
}

impl McpGatewayFullConfig {
    pub fn from_env_and_args(args: &[String]) -> Self {
        let file = load_gateway_file_config();
        let mut config = Self {
            bind: file.gateway.bind.clone().trim().to_string(),
            gateway_token: super::config::resolve_mcp_gateway_token(),
            admin_token: super::config::resolve_mcp_gateway_admin_token(),
            invokes_enabled: !args.iter().any(|arg| arg == "--invokes-disabled"),
            daemon_policy_url: file.gateway.daemon_policy_url.clone(),
            policy_token: super::config::resolve_mcp_policy_token(),
            max_invoke_duration_ms: file.gateway.max_invoke_duration_ms,
            catalog_refresh_interval_secs: file.gateway.catalog_refresh_interval_secs,
            use_mock_fallback: file.gateway.use_mock_fallback,
            servers: file.servers,
        };

        if let Some(bind) = find_arg_value(args, "--bind") {
            config.bind = bind;
        }

        config
    }

    pub fn server_by_id(&self, server_id: &str) -> Option<&McpServerConfig> {
        self.servers
            .iter()
            .find(|server| server.id.eq_ignore_ascii_case(server_id))
    }

    /// Keep transports that can run without a child process or sidecar.
    pub fn remote_only(mut self) -> Self {
        self.servers.retain(|server| {
            server.use_mock
                || matches!(
                    server.transport.trim().to_ascii_lowercase().as_str(),
                    "http"
                        | "https"
                        | "streamable"
                        | "streamable-http"
                        | "streamable_http"
                        | "sse"
                        | "http-sse"
                        | "http_sse"
                )
        });
        self
    }
}

/// Validate a remote MCP endpoint before any credentials are attached.
///
/// Plain HTTP is supported for local development and unauthenticated private
/// network servers. Credentials are never sent over plaintext outside the
/// loopback interface.
pub fn validate_remote_server_url(raw: &str, has_bearer: bool) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|error| format!("Invalid remote MCP URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Remote MCP URL must use http:// or https://".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Remote MCP credentials must not be embedded in the URL".to_string());
    }
    if url.fragment().is_some() {
        return Err("Remote MCP URL must not contain a fragment".to_string());
    }

    if url.scheme() == "http" {
        let host = url
            .host_str()
            .ok_or_else(|| "Remote MCP URL must include a host".to_string())?;
        if !is_local_or_private_host(host) {
            return Err("Remote MCP servers outside the local network must use HTTPS".to_string());
        }
        if has_bearer && !is_loopback_host(host) {
            return Err(
                "Bearer and OAuth credentials require HTTPS except on loopback".to_string(),
            );
        }
    }
    Ok(url)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_local_or_private_host(host: &str) -> bool {
    if is_loopback_host(host) || host.ends_with(".local") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|address| match address {
        IpAddr::V4(address) => address.is_private() || address.is_link_local(),
        IpAddr::V6(address) => address.is_unique_local() || address.is_unicast_link_local(),
    })
}

pub fn gateway_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("mcp-gateway.toml")
}

fn load_gateway_file_config() -> McpGatewayFileConfig {
    let path = gateway_config_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return McpGatewayFileConfig {
            gateway: GatewaySection::default(),
            servers: Vec::new(),
        };
    };

    toml::from_str(&raw).unwrap_or_else(|error| {
        eprintln!(
            "medousa-mcp-gateway: failed to parse {}: {error}",
            path.display()
        );
        McpGatewayFileConfig {
            gateway: GatewaySection::default(),
            servers: Vec::new(),
        }
    })
}

fn default_bind() -> String {
    medousa_types::mcp_gateway_api::DEFAULT_MCP_GATEWAY_BIND.to_string()
}

fn default_daemon_policy_url() -> String {
    format!(
        "{}/v1/mcp/policy/evaluate",
        medousa_types::daemon_api::DEFAULT_DAEMON_URL
    )
}

fn default_max_invoke_ms() -> u64 {
    30_000
}

fn default_catalog_refresh_secs() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_allowed_lanes() -> Vec<String> {
    vec!["interactive".to_string(), "scheduled".to_string()]
}

fn default_allowed_effects() -> Vec<String> {
    vec![
        "external_read".to_string(),
        "external_write".to_string(),
        "external_side_effect".to_string(),
    ]
}

fn find_arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{McpGatewayFileConfig, validate_remote_server_url};

    #[test]
    fn tool_discovery_hints_and_disabled_tools_round_trip() {
        let raw = r#"
[[servers]]
id = "search"
title = "Search"
disabled_tools = ["delete_index"]

[servers.tool_tags]
search_web = ["web_research", "internet"]
"#;

        let parsed: McpGatewayFileConfig = toml::from_str(raw).expect("parse gateway config");
        let server = parsed.servers.first().expect("configured server");
        assert_eq!(server.disabled_tools, ["delete_index"]);
        assert_eq!(
            server.tool_tags.get("search_web"),
            Some(&vec!["web_research".to_string(), "internet".to_string()])
        );

        let encoded = toml::to_string_pretty(&parsed).expect("serialize gateway config");
        let reparsed: McpGatewayFileConfig =
            toml::from_str(&encoded).expect("reparse gateway config");
        assert_eq!(reparsed.servers[0].disabled_tools, ["delete_index"]);
        assert_eq!(
            reparsed.servers[0].tool_tags.get("search_web"),
            Some(&vec!["web_research".to_string(), "internet".to_string()])
        );
    }

    #[test]
    fn remote_urls_require_https_outside_local_networks() {
        assert!(validate_remote_server_url("https://mcp.example.com/mcp", true).is_ok());
        assert!(validate_remote_server_url("http://127.0.0.1:8000/mcp", true).is_ok());
        assert!(validate_remote_server_url("http://192.168.1.40/mcp", false).is_ok());
        assert!(validate_remote_server_url("http://192.168.1.40/mcp", true).is_err());
        assert!(validate_remote_server_url("http://mcp.example.com/mcp", false).is_err());
        assert!(validate_remote_server_url("ftp://mcp.example.com", false).is_err());
        assert!(validate_remote_server_url("https://token@mcp.example.com/mcp", false).is_err());
        assert!(validate_remote_server_url("https://mcp.example.com/mcp#tools", false).is_err());
    }
}
