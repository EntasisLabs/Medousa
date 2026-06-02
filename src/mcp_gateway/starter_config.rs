//! Starter `mcp-gateway.toml` for setup wizard and docs.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::gateway_config_path;

/// Example gateway config written by `medousa setup` when no file exists yet.
pub const STARTER_MCP_GATEWAY_TOML: &str = r#"# Medousa MCP Client gateway
# Guide: docs/mcp-gateway-setup.md

[gateway]
bind = "127.0.0.1:7420"
daemon_policy_url = "http://127.0.0.1:7419/v1/mcp/policy/evaluate"
use_mock_fallback = true
# max_invoke_duration_ms = 30000
# catalog_refresh_interval_secs = 300

# Optional auth (export before starting gateway + daemon):
# export MEDOUSA_MCP_GATEWAY_TOKEN="shared-secret-for-daemon-to-gateway"
# export MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN="admin-secret"
# export MEDOUSA_MCP_POLICY_TOKEN="policy-secret"
# export MEDOUSA_MCP_TURN_TOKEN_SECRET="turn-hmac-secret"

# ── Mock servers (no external MCP binary; good for doctor + TUI smoke tests) ──

[[servers]]
id = "notion"
title = "Notion MCP (mock)"
enabled = true
transport = "stdio"
use_mock = true
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write", "external_side_effect"]

[[servers]]
id = "gmail"
title = "Gmail MCP (mock)"
enabled = true
transport = "stdio"
use_mock = true
allowed_lanes = ["interactive", "scheduled"]
allowed_effect_classes = ["external_read", "external_write", "external_side_effect"]

# ── Real stdio server example (disabled until you install the MCP package) ──
# Copy, set enabled = true, and fill command/args. Gateway spawns this process per catalog refresh.

# [[servers]]
# id = "filesystem"
# title = "Filesystem MCP"
# enabled = false
# transport = "stdio"
# command = "npx"
# args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/you/projects"]
# allowed_lanes = ["interactive", "scheduled"]
# allowed_effect_classes = ["external_read", "external_write"]
# [servers.tool_tags]
# "list_directory" = ["files", "read"]

# [[servers]]
# id = "fetch"
# title = "Fetch MCP"
# enabled = false
# transport = "stdio"
# command = "uvx"
# args = ["mcp-server-fetch"]
# allowed_lanes = ["interactive"]
# allowed_effect_classes = ["external_read"]
"#;

/// Install starter config if missing. Returns the config path (existing or new).
pub fn install_starter_gateway_config_if_missing() -> Result<PathBuf> {
    let path = gateway_config_path();
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create MCP gateway config directory {}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, STARTER_MCP_GATEWAY_TOML).with_context(|| {
        format!(
            "failed to write MCP gateway config {}",
            path.display()
        )
    })?;
    Ok(path)
}
