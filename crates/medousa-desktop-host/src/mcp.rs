//! Settings → MCP without a renderer: the gateway's status, its servers in
//! `mcp-gateway.toml`, and restarting the local gateway so edits apply. The
//! same steps as medousa-home's Tauri commands for a desktop workshop.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use medousa_mcp_gateway::{McpGatewayFileConfig, SecureMcpOAuthBundleStore};

const DEFAULT_GATEWAY_URL: &str = "http://127.0.0.1:7420";

/// One server as Settings lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub connected: bool,
    pub tool_count: u32,
}

/// The gateway as Settings shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub gateway_url: String,
    pub reachable: bool,
    pub message: String,
    pub connected_servers: Option<u32>,
    pub catalog_entries: Option<u32>,
    pub servers: Vec<Server>,
    /// This computer owns the config; otherwise the workshop manages it.
    pub local: bool,
    pub config_path: String,
}

fn data_dir() -> PathBuf {
    crate::workshop_registry::medousa_data_dir()
}

/// Where the gateway writes its log when this app starts it.
pub fn log_path() -> PathBuf {
    data_dir().join("logs").join("mcp-gateway.log")
}

/// Whether the active workshop's MCP config lives on this computer.
pub fn uses_local_config() -> bool {
    use crate::workshop_registry::{active_workshop, load_registry};
    load_registry()
        .ok()
        .and_then(|r| active_workshop(&r).map(|w| w.kind == "local"))
        .unwrap_or(true)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn gateway_url() -> String {
    env("MEDOUSA_MCP_GATEWAY_URL").unwrap_or_else(|| DEFAULT_GATEWAY_URL.to_string())
}

fn normalize_server_id(raw: &str) -> Result<String, String> {
    let id = raw.trim().to_ascii_lowercase();
    if id.is_empty() {
        return Err("Server id is required".to_string());
    }
    if !id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(
            "Server id may only use letters, numbers, hyphens, and underscores".to_string(),
        );
    }
    Ok(id)
}

/// Reads the config, installing the starter file first if there is none and
/// moving any plaintext bearer token into secure storage. Blocking file IO.
pub fn load_config() -> Result<(McpGatewayFileConfig, PathBuf), String> {
    let path = medousa_mcp_gateway::install_starter_gateway_config_if_missing()
        .map_err(|err| err.to_string())?;
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let mut config = toml::from_str::<McpGatewayFileConfig>(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    let credentials = SecureMcpOAuthBundleStore::new(data_dir())
        .map_err(|error| format!("initialize MCP credential storage: {error}"))?;
    let mut migrated = false;
    for server in &mut config.servers {
        let Some(token) = server
            .bearer_token
            .take()
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
        else {
            continue;
        };
        credentials.save_bearer_token(&server.id, Some(&token))?;
        server.bearer_token_configured = true;
        migrated = true;
    }
    if migrated {
        save_config(&config)?;
    }
    Ok((config, path))
}

fn save_config(config: &McpGatewayFileConfig) -> Result<PathBuf, String> {
    let path = medousa_mcp_gateway::install_starter_gateway_config_if_missing()
        .map_err(|err| err.to_string())?;
    let encoded = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(&path, encoded).map_err(|err| err.to_string())?;
    Ok(path)
}

fn require_local() -> Result<(), String> {
    if uses_local_config() {
        Ok(())
    } else {
        Err("MCP configuration is managed by the selected workshop".to_string())
    }
}

/// Turns a server on or off in the config. Applies on the next restart.
/// Blocking file IO.
pub fn set_server_enabled(server_id: &str, enabled: bool) -> Result<(), String> {
    require_local()?;
    let id = normalize_server_id(server_id)?;
    let (mut config, _) = load_config()?;
    let entry = config
        .servers
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))?;
    entry.enabled = enabled;
    save_config(&config).map(|_| ())
}

/// Removes a server and its stored token. Applies on the next restart.
/// Blocking file IO.
pub fn remove_server(server_id: &str) -> Result<(), String> {
    require_local()?;
    let id = normalize_server_id(server_id)?;
    let (mut config, _) = load_config()?;
    let before = config.servers.len();
    config
        .servers
        .retain(|entry| !entry.id.eq_ignore_ascii_case(&id));
    if config.servers.len() == before {
        return Err(format!("unknown MCP server '{id}'"));
    }
    SecureMcpOAuthBundleStore::new(data_dir())
        .map_err(|error| format!("initialize MCP credential storage: {error}"))?
        .save_bearer_token(&id, None)?;
    save_config(&config).map(|_| ())
}

/// How a server is reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transport {
    #[default]
    Stdio,
    Http,
    Sse,
}

/// A server to add or replace, as medousa-home's form sends it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Upsert {
    pub id: String,
    pub title: String,
    pub transport: Transport,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    /// A new bearer token; `None` keeps the stored one.
    pub bearer_token: Option<String>,
    pub clear_bearer_token: bool,
}

/// A saved server as the form shows it, and whether it has a token.
/// Blocking file IO.
pub fn server_config(server_id: &str) -> Result<Option<(Upsert, bool)>, String> {
    let (config, _) = load_config()?;
    Ok(config
        .servers
        .iter()
        .find(|s| s.id.eq_ignore_ascii_case(server_id.trim()))
        .map(|s| {
            let transport = match s.transport.trim().to_ascii_lowercase().as_str() {
                "sse" | "http-sse" => Transport::Sse,
                "http" | "https" | "streamable" | "streamable-http" => Transport::Http,
                _ => Transport::Stdio,
            };
            (
                Upsert {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    transport,
                    command: s.command.clone().unwrap_or_default(),
                    args: s.args.clone(),
                    url: s.url.clone().unwrap_or_default(),
                    bearer_token: None,
                    clear_bearer_token: false,
                },
                s.bearer_token_configured,
            )
        }))
}

/// medousa-home's `validate_server`, keeping what an existing server had
/// that the form doesn't edit: whether it is on, tool tags and disabled
/// tools.
fn config_from(
    u: &Upsert,
    existing: Option<&medousa_mcp_gateway::McpServerConfig>,
) -> Result<medousa_mcp_gateway::McpServerConfig, String> {
    let id = normalize_server_id(&u.id)?;
    let title = u.title.trim();
    if title.is_empty() {
        return Err("Title is required".to_string());
    }
    let token = u
        .bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let (transport, command, args, url) = match u.transport {
        Transport::Http | Transport::Sse => {
            let url = u.url.trim();
            if url.is_empty() {
                return Err("URL is required for remote MCP servers".to_string());
            }
            let has_token = token.is_some()
                || (!u.clear_bearer_token
                    && existing.is_some_and(|e| e.bearer_token_configured));
            medousa_mcp_gateway::validate_remote_server_url(url, has_token)?;
            let name = if u.transport == Transport::Sse {
                "sse"
            } else {
                "http"
            };
            (name, None, Vec::new(), Some(url.to_string()))
        }
        Transport::Stdio => {
            let command = u.command.trim();
            if command.is_empty() {
                return Err("Command is required for stdio MCP servers".to_string());
            }
            let args = u
                .args
                .iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
            ("stdio", Some(command.to_string()), args, None)
        }
    };
    let remote = transport != "stdio";
    Ok(medousa_mcp_gateway::McpServerConfig {
        id,
        title: title.to_string(),
        enabled: existing.is_none_or(|e| e.enabled),
        transport: transport.to_string(),
        command,
        args,
        url,
        bearer_token: None,
        bearer_token_configured: remote
            && (token.is_some()
                || (!u.clear_bearer_token
                    && existing.is_some_and(|e| e.bearer_token_configured))),
        allowed_lanes: existing.map_or_else(
            || vec!["interactive".into(), "scheduled".into()],
            |e| e.allowed_lanes.clone(),
        ),
        allowed_effect_classes: existing.map_or_else(
            || {
                vec![
                    "external_read".into(),
                    "external_write".into(),
                    "external_side_effect".into(),
                ]
            },
            |e| e.allowed_effect_classes.clone(),
        ),
        tool_tags: existing.map(|e| e.tool_tags.clone()).unwrap_or_default(),
        disabled_tools: existing
            .map(|e| e.disabled_tools.clone())
            .unwrap_or_default(),
        use_mock: false,
    })
}

/// Adds or replaces a server, keeping its bearer token in secure storage.
/// Applies on the next restart. Blocking file IO.
pub fn upsert_server(u: &Upsert) -> Result<(), String> {
    require_local()?;
    let (mut config, _) = load_config()?;
    let id = normalize_server_id(&u.id)?;
    let at = config
        .servers
        .iter()
        .position(|s| s.id.eq_ignore_ascii_case(&id));
    let server = config_from(u, at.map(|i| &config.servers[i]))?;
    let credentials = SecureMcpOAuthBundleStore::new(data_dir())
        .map_err(|error| format!("initialize MCP credential storage: {error}"))?;
    let token = u
        .bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if server.transport == "stdio" || u.clear_bearer_token {
        credentials.save_bearer_token(&server.id, None)?;
    } else if let Some(token) = token {
        credentials.save_bearer_token(&server.id, Some(token))?;
    }
    match at {
        Some(i) => config.servers[i] = server,
        None => config.servers.push(server),
    }
    save_config(&config).map(|_| ())
}

/// One tool of a server as Settings lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    pub title: String,
    pub enabled: bool,
    /// The gateway reports it and it is on.
    pub available: bool,
    /// Words that help Medousa find it.
    pub hints: Vec<String>,
}

/// The gateway's live tool catalog.
pub async fn catalog() -> Result<medousa_types::mcp_gateway_api::McpCatalogSyncResponse, String> {
    let base = gateway_url();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client.get(format!("{}/v1/mcp/catalog", base.trim_end_matches('/')));
    if let Some(token) = env("MEDOUSA_MCP_GATEWAY_TOKEN") {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("cannot reach MCP gateway at {base}: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("MCP gateway returned HTTP {}", response.status()));
    }
    response.json().await.map_err(|err| err.to_string())
}

fn hints_for(server: &medousa_mcp_gateway::McpServerConfig, tool: &str) -> Vec<String> {
    server
        .tool_tags
        .iter()
        .find_map(|(name, hints)| name.eq_ignore_ascii_case(tool).then(|| hints.clone()))
        .unwrap_or_default()
}

/// A server's tools, after medousa-home's `mcp_gateway_list_tools`: what
/// the gateway reports, and what the config turns off or tags, by title.
pub fn merge_tools(
    server: &medousa_mcp_gateway::McpServerConfig,
    catalog: Option<&medousa_types::mcp_gateway_api::McpCatalogSyncResponse>,
) -> Vec<Tool> {
    let mut tools: Vec<Tool> = Vec::new();
    for entry in catalog
        .into_iter()
        .flat_map(|c| c.entries.iter())
        .filter(|e| e.server_id.eq_ignore_ascii_case(&server.id))
    {
        if tools.iter().any(|t| t.name.eq_ignore_ascii_case(&entry.tool_name)) {
            continue;
        }
        let enabled = server.tool_enabled(&entry.tool_name);
        tools.push(Tool {
            name: entry.tool_name.clone(),
            title: entry.title.clone(),
            enabled,
            available: entry.available && enabled,
            hints: hints_for(server, &entry.tool_name),
        });
    }
    for name in server.tool_tags.keys().chain(server.disabled_tools.iter()) {
        if tools.iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
            continue;
        }
        tools.push(Tool {
            name: name.clone(),
            title: name.clone(),
            enabled: server.tool_enabled(name),
            available: false,
            hints: hints_for(server, name),
        });
    }
    tools.sort_by(|a, b| {
        a.title
            .to_ascii_lowercase()
            .cmp(&b.title.to_ascii_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    tools
}

/// One saved server's config. Blocking file IO.
pub fn server(server_id: &str) -> Result<medousa_mcp_gateway::McpServerConfig, String> {
    let id = normalize_server_id(server_id)?;
    let (config, _) = load_config()?;
    config
        .servers
        .into_iter()
        .find(|s| s.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))
}

/// Turns one tool on or off and sets its discovery hints, as medousa-home's
/// `mcp_gateway_update_tool`. Applies on the next restart. Blocking file IO.
pub fn update_tool(
    server_id: &str,
    tool_name: &str,
    enabled: bool,
    hints: &[String],
) -> Result<String, String> {
    require_local()?;
    let id = normalize_server_id(server_id)?;
    let tool_name = tool_name.trim();
    if tool_name.is_empty() || tool_name.chars().any(char::is_control) {
        return Err("Tool name is required".to_string());
    }
    let (mut config, _) = load_config()?;
    let server = config
        .servers
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))?;
    let name = server
        .tool_tags
        .keys()
        .chain(server.disabled_tools.iter())
        .find(|existing| existing.eq_ignore_ascii_case(tool_name))
        .cloned()
        .unwrap_or_else(|| tool_name.to_string());
    server
        .tool_tags
        .retain(|n, _| !n.eq_ignore_ascii_case(tool_name));
    let mut kept: Vec<String> = Vec::new();
    for hint in hints.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
        if !kept.iter().any(|k| k.eq_ignore_ascii_case(hint)) {
            kept.push(hint.to_string());
        }
    }
    if !kept.is_empty() {
        server.tool_tags.insert(name.clone(), kept);
    }
    server
        .disabled_tools
        .retain(|d| !d.eq_ignore_ascii_case(tool_name));
    if !enabled {
        server.disabled_tools.push(name.clone());
    }
    save_config(&config)?;
    Ok(name)
}

fn servers_from_config(config: &McpGatewayFileConfig, connected: bool) -> Vec<Server> {
    config
        .servers
        .iter()
        .map(|s| Server {
            id: s.id.clone(),
            title: s.title.clone(),
            enabled: s.enabled,
            connected,
            tool_count: 0,
        })
        .collect()
}

/// The daemon's view of the gateway, with this computer's config filling in
/// servers the gateway has not reported.
pub async fn status(client: &medousa_sdk::MedousaClient) -> Status {
    let local = uses_local_config();
    let (config, config_path) = if local {
        match tokio::task::spawn_blocking(load_config).await {
            Ok(Ok((config, path))) => (Some(config), path.display().to_string()),
            _ => (None, String::new()),
        }
    } else {
        (None, String::new())
    };
    let fallback = |message: String| Status {
        gateway_url: gateway_url(),
        reachable: false,
        message,
        connected_servers: None,
        catalog_entries: None,
        servers: config
            .as_ref()
            .map(|c| servers_from_config(c, false))
            .unwrap_or_default(),
        local,
        config_path: config_path.clone(),
    };
    match client.mcp_gateway().status().await {
        Ok(daemon) => {
            let servers = if daemon.servers.is_empty() {
                config
                    .as_ref()
                    .map(|c| servers_from_config(c, daemon.reachable))
                    .unwrap_or_default()
            } else {
                daemon
                    .servers
                    .into_iter()
                    .map(|s| Server {
                        id: s.server_id,
                        title: s.title,
                        enabled: s.enabled,
                        connected: s.connected,
                        tool_count: s.tool_count,
                    })
                    .collect()
            };
            Status {
                gateway_url: daemon.gateway_url,
                reachable: daemon.reachable,
                message: daemon.message,
                connected_servers: daemon.health.as_ref().map(|h| h.connected_servers),
                catalog_entries: daemon.health.as_ref().map(|h| h.catalog_entries),
                servers,
                local,
                config_path: config_path.clone(),
            }
        }
        Err(err) => fallback(format!(
            "Workshop unavailable — cannot check the MCP gateway ({err})"
        )),
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())
}

async fn healthy(base_url: &str) -> bool {
    let Ok(client) = http_client() else {
        return false;
    };
    client
        .get(format!("{}/health", base_url.trim_end_matches('/')))
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

async fn bind_reachable(bind: &str) -> bool {
    tokio::time::timeout(
        Duration::from_millis(250),
        tokio::net::TcpStream::connect(bind),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

fn gateway_binary() -> Result<String, String> {
    use crate::workshop_runtime::{find_command_in_path, platform_binary_name, shared_bin_binary};
    if let Some(explicit) = env("MEDOUSA_MCP_GATEWAY_BIN").map(PathBuf::from)
        && explicit.exists()
    {
        return Ok(explicit.to_string_lossy().to_string());
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name(platform_binary_name("medousa_mcp_gateway"));
        if sibling.exists() {
            return Ok(sibling.to_string_lossy().to_string());
        }
    }
    if let Some(shared) = shared_bin_binary("medousa_mcp_gateway") {
        return Ok(shared.to_string_lossy().to_string());
    }
    if find_command_in_path("medousa_mcp_gateway").is_some() {
        return Ok(platform_binary_name("medousa_mcp_gateway"));
    }
    Err("Medousa couldn’t find the MCP gateway — install it from Packages, or set MEDOUSA_MCP_GATEWAY_BIN for development.".to_string())
}

fn spawn(bind: &str) -> Result<u32, String> {
    let program = gateway_binary()?;
    let log = log_path();
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|err| err.to_string())?;
    let err = out.try_clone().map_err(|err| err.to_string())?;
    let mut command = Command::new(&program);
    command
        .arg("--bind")
        .arg(bind)
        .env("MEDOUSA_DATA_DIR", data_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    // A background child must outlive this app and never open a console.
    medousa_host::detach_new_session(&mut command);
    command
        .spawn()
        .map(|child| child.id())
        .map_err(|err| format!("failed to start the MCP gateway ({program}): {err}"))
}

/// Restarts the local gateway so config edits apply, and returns a message
/// and whether it came up healthy. Only for a gateway on this computer.
pub async fn restart() -> Result<(String, bool), String> {
    // Two restarts must not stop each other's freshly started gateway.
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _lock = LOCK.lock().await;
    require_local()?;
    let (config, _) = tokio::task::spawn_blocking(load_config)
        .await
        .map_err(|e| e.to_string())??;
    let bind = config.gateway.bind.trim().to_string();
    let base_url = gateway_url();
    let address: std::net::SocketAddr = bind
        .parse()
        .map_err(|_| "Invalid MCP gateway bind address".to_string())?;
    let url = reqwest::Url::parse(&base_url).map_err(|error| error.to_string())?;
    let url_ip = url.host_str().and_then(|host| {
        host.trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .ok()
    });
    if !address.ip().is_loopback()
        || url_ip != Some(address.ip())
        || url.port_or_known_default() != Some(address.port())
    {
        return Err("MCP restart needs the gateway URL and its local bind address to match".into());
    }
    if healthy(&base_url).await {
        medousa_mcp_gateway::mcp_gateway::local_process::stop_local_gateway(&bind)
            .await
            .map_err(|error| error.to_string())?;
    }
    if bind_reachable(&bind).await {
        return Err(format!(
            "Port {bind} is open but the MCP gateway isn’t answering — check {}",
            log_path().display()
        ));
    }
    let owned = bind.clone();
    let pid = tokio::task::spawn_blocking(move || spawn(&owned))
        .await
        .map_err(|error| error.to_string())??;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut ready = false;
    while Instant::now() < deadline {
        if bind_reachable(&bind).await && healthy(&base_url).await {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok((
        if ready {
            format!("MCP gateway restarted (pid {pid})")
        } else {
            format!(
                "MCP gateway started (pid {pid}) but isn’t healthy yet — check {}",
                log_path().display()
            )
        },
        ready,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_merge_the_catalog_with_what_the_config_turns_off_or_tags() {
        let mut server = config_from(
            &Upsert {
                id: "files".into(),
                title: "Files".into(),
                command: "npx".into(),
                ..Upsert::default()
            },
            None,
        )
        .unwrap();
        server.disabled_tools = vec!["Delete".into()];
        server
            .tool_tags
            .insert("read".into(), vec!["open a file".into()]);
        server.tool_tags.insert("gone".into(), vec!["x".into()]);
        let catalog: medousa_types::mcp_gateway_api::McpCatalogSyncResponse =
            serde_json::from_value(serde_json::json!({
                "now_utc": "2026-11-01T00:00:00Z",
                "entries": [
                    {"server_id": "files", "tool_name": "read", "title": "Read file", "available": true},
                    {"server_id": "FILES", "tool_name": "delete", "title": "Delete file", "available": true},
                    {"server_id": "other", "tool_name": "x", "title": "X", "available": true}
                ]
            }))
            .unwrap();
        let tools = merge_tools(&server, Some(&catalog));
        let names: Vec<_> = tools.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(names, ["Delete file", "gone", "Read file"]);
        assert!(!tools[0].enabled && !tools[0].available);
        assert!(!tools[1].available, "only in the config");
        assert_eq!(tools[2].hints, ["open a file"]);
        assert_eq!(merge_tools(&server, None).len(), 3);
    }

    #[test]
    fn a_server_is_checked_like_medousa_home_and_keeps_what_the_form_skips() {
        let stdio = Upsert {
            id: "Files".into(),
            title: " Files ".into(),
            command: "npx".into(),
            args: vec!["-y".into(), " ".into(), "server".into()],
            ..Upsert::default()
        };
        let made = config_from(&stdio, None).unwrap();
        assert_eq!((made.id.as_str(), made.title.as_str()), ("files", "Files"));
        assert_eq!(made.args, ["-y", "server"]);
        assert!(made.enabled && made.allowed_lanes.len() == 2);

        let mut existing = made.clone();
        existing.enabled = false;
        existing.disabled_tools = vec!["delete".into()];
        let edited = config_from(&stdio, Some(&existing)).unwrap();
        assert!(!edited.enabled, "editing keeps a server off");
        assert_eq!(edited.disabled_tools, ["delete"]);

        assert!(config_from(&Upsert { command: String::new(), ..stdio.clone() }, None).is_err());
        assert!(config_from(&Upsert { title: " ".into(), ..stdio.clone() }, None).is_err());
        let remote = Upsert {
            transport: Transport::Sse,
            url: "https://mcp.example/sse".into(),
            bearer_token: Some("t".into()),
            ..stdio
        };
        let made = config_from(&remote, None).unwrap();
        assert_eq!(made.transport, "sse");
        assert!(made.bearer_token_configured && made.bearer_token.is_none());
        assert!(config_from(&Upsert { url: " ".into(), ..remote }, None).is_err());
    }

    #[test]
    fn server_ids_follow_medousa_home_rules() {
        assert_eq!(normalize_server_id(" GitHub ").unwrap(), "github");
        assert!(normalize_server_id("").is_err());
        assert!(normalize_server_id("has space").is_err());
    }

    #[test]
    fn config_servers_stand_in_until_the_gateway_reports() {
        let config: McpGatewayFileConfig = toml::from_str(
            r#"
            [[servers]]
            id = "github"
            title = "GitHub"
            enabled = true
            transport = "stdio"
            command = "github-mcp"
            "#,
        )
        .unwrap();
        let servers = servers_from_config(&config, false);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].title, "GitHub");
        assert!(!servers[0].connected);
        assert_eq!(servers[0].tool_count, 0);
    }
}
