use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
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
    #[serde(default = "default_allowed_lanes")]
    pub allowed_lanes: Vec<String>,
    #[serde(default = "default_allowed_effects")]
    pub allowed_effect_classes: Vec<String>,
    #[serde(default)]
    pub tool_tags: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub use_mock: bool,
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
            bind: file
                .gateway
                .bind
                .clone()
                .trim()
                .to_string(),
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
        eprintln!("medousa-mcp-gateway: failed to parse {}: {error}", path.display());
        McpGatewayFileConfig {
            gateway: GatewaySection::default(),
            servers: Vec::new(),
        }
    })
}

fn default_bind() -> String {
    crate::mcp_gateway_api::DEFAULT_MCP_GATEWAY_BIND.to_string()
}

fn default_daemon_policy_url() -> String {
    format!(
        "{}/v1/mcp/policy/evaluate",
        crate::daemon_api::DEFAULT_DAEMON_URL
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

pub fn load_gateway_file_for_test(raw: &str) -> Result<McpGatewayFileConfig> {
    Ok(toml::from_str(raw).context("invalid gateway toml")?)
}
