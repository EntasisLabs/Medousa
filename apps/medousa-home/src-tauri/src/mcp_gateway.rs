//! MCP gateway config + lifecycle for Home — read/write `mcp-gateway.toml`, probe gateway HTTP.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_GATEWAY_URL: &str = "http://127.0.0.1:7420";
const DEFAULT_GATEWAY_BIND: &str = "127.0.0.1:7420";

const STARTER_MCP_GATEWAY_TOML: &str = r#"# Medousa MCP Client gateway
# Guide: docs/mcp-gateway-setup.md

[gateway]
bind = "127.0.0.1:7420"
daemon_policy_url = "http://127.0.0.1:7419/v1/mcp/policy/evaluate"
use_mock_fallback = true

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
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayFileConfig {
    #[serde(default)]
    pub gateway: GatewaySection,
    #[serde(default)]
    pub servers: Vec<McpServerConfigDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct McpServerConfigDto {
    pub id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "bearer_token")]
    pub bearer_token: Option<String>,
    #[serde(default = "default_allowed_lanes")]
    pub allowed_lanes: Vec<String>,
    #[serde(default = "default_allowed_effects")]
    pub allowed_effect_classes: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tool_tags: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub use_mock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayConfigLoadResult {
    pub path: String,
    pub config: McpGatewayFileConfig,
    pub file_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayHealthDto {
    pub status: String,
    pub invokes_enabled: bool,
    pub registered_servers: u32,
    pub connected_servers: u32,
    pub catalog_entries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRuntimeDto {
    pub server_id: String,
    pub title: String,
    pub enabled: bool,
    pub connected: bool,
    pub tool_count: u32,
    pub allowed_lanes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayStatusResult {
    pub gateway_url: String,
    pub reachable: bool,
    pub message: String,
    pub health: Option<McpGatewayHealthDto>,
    pub servers: Vec<McpServerRuntimeDto>,
    pub config_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayRestartResult {
    pub started: bool,
    pub already_running: bool,
    pub log_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerUpsertRequest {
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
    #[serde(default)]
    pub use_mock: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerMutationResult {
    pub ok: bool,
    pub message: String,
    pub config_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayTestResult {
    pub ok: bool,
    pub message: String,
    pub connected: bool,
    pub tool_count: u32,
}

fn gateway_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("mcp-gateway.toml")
}

fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn gateway_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("mcp-gateway.log")
}

fn resolve_gateway_url() -> String {
    std::env::var("MEDOUSA_MCP_GATEWAY_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_GATEWAY_URL.to_string())
}

fn resolve_gateway_token() -> Option<String> {
    std::env::var("MEDOUSA_MCP_GATEWAY_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn resolve_admin_token() -> Option<String> {
    std::env::var("MEDOUSA_MCP_GATEWAY_ADMIN_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_bind() -> String {
    DEFAULT_GATEWAY_BIND.to_string()
}

fn default_daemon_policy_url() -> String {
    "http://127.0.0.1:7419/v1/mcp/policy/evaluate".to_string()
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

fn install_starter_if_missing() -> Result<PathBuf, String> {
    let path = gateway_config_path();
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&path, STARTER_MCP_GATEWAY_TOML).map_err(|err| err.to_string())?;
    Ok(path)
}

fn load_file_config() -> Result<(McpGatewayFileConfig, PathBuf, bool), String> {
    let path = install_starter_if_missing()?;
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let config = toml::from_str::<McpGatewayFileConfig>(&raw).map_err(|err| {
        format!("failed to parse {}: {err}", path.display())
    })?;
    Ok((config, path, true))
}

fn persist_file_config(config: &McpGatewayFileConfig) -> Result<PathBuf, String> {
    let path = install_starter_if_missing()?;
    let encoded = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(&path, encoded).map_err(|err| err.to_string())?;
    Ok(path)
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
        return Err("Server id may only use letters, numbers, hyphens, and underscores".to_string());
    }
    Ok(id)
}

fn normalize_transport(raw: &str) -> Result<String, String> {
    let transport = raw.trim().to_ascii_lowercase();
    match transport.as_str() {
        "stdio" | "http" | "streamable" | "streamable-http" | "sse" => Ok(transport),
        _ => Err(format!(
            "Unsupported transport '{raw}' — use stdio, http, or sse"
        )),
    }
}

fn validate_server(request: &McpServerUpsertRequest) -> Result<McpServerConfigDto, String> {
    let id = normalize_server_id(&request.id)?;
    let title = request.title.trim();
    if title.is_empty() {
        return Err("Title is required".to_string());
    }
    let transport = normalize_transport(&request.transport)?;

    if request.use_mock {
        return Ok(McpServerConfigDto {
            id,
            title: title.to_string(),
            enabled: request.enabled,
            transport: "stdio".to_string(),
            command: None,
            args: Vec::new(),
            url: None,
            bearer_token: None,
            allowed_lanes: default_allowed_lanes(),
            allowed_effect_classes: default_allowed_effects(),
            tool_tags: HashMap::new(),
            use_mock: true,
        });
    }

    if transport == "http" || transport == "streamable" || transport == "streamable-http" || transport == "sse" {
        let url = request
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "URL is required for remote MCP servers".to_string())?;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("Remote MCP URL must start with http:// or https://".to_string());
        }
        let bearer_token = request
            .bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        return Ok(McpServerConfigDto {
            id,
            title: title.to_string(),
            enabled: request.enabled,
            transport: if transport == "sse" {
                "sse".to_string()
            } else {
                "http".to_string()
            },
            command: None,
            args: Vec::new(),
            url: Some(url.to_string()),
            bearer_token,
            allowed_lanes: default_allowed_lanes(),
            allowed_effect_classes: default_allowed_effects(),
            tool_tags: HashMap::new(),
            use_mock: false,
        });
    }

    let command = request
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Command is required for stdio MCP servers (or enable mock mode)".to_string())?;
    Ok(McpServerConfigDto {
        id,
        title: title.to_string(),
        enabled: request.enabled,
        transport: "stdio".to_string(),
        command: Some(command.to_string()),
        args: request
            .args
            .iter()
            .map(|arg| arg.trim().to_string())
            .filter(|arg| !arg.is_empty())
            .collect(),
        url: None,
        bearer_token: None,
        allowed_lanes: default_allowed_lanes(),
        allowed_effect_classes: default_allowed_effects(),
        tool_tags: HashMap::new(),
        use_mock: false,
    })
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())
}

fn apply_gateway_auth(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(token) = resolve_gateway_token() {
        request.bearer_auth(token)
    } else {
        request
    }
}

fn apply_admin_auth(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(token) = resolve_admin_token() {
        request.bearer_auth(token)
    } else {
        request
    }
}

async fn gateway_http_healthy(base_url: &str) -> bool {
    let Ok(client) = http_client() else {
        return false;
    };
    client
        .get(format!("{}/health", base_url.trim_end_matches('/')))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

async fn fetch_runtime_servers(base_url: &str) -> Result<Vec<McpServerRuntimeDto>, String> {
    let client = http_client()?;
    let response = apply_gateway_auth(
        client.get(format!(
            "{}/v1/mcp/servers",
            base_url.trim_end_matches('/')
        )),
    )
    .send()
    .await
    .map_err(|err| format!("cannot reach MCP gateway at {base_url}: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("MCP gateway returned HTTP {}", response.status()));
    }
    #[derive(Debug, Deserialize)]
    struct ServersPayload {
        servers: Vec<RuntimeServer>,
    }
    #[derive(Debug, Deserialize)]
    struct RuntimeServer {
        server_id: String,
        title: String,
        enabled: bool,
        connected: bool,
        tool_count: u32,
        allowed_lanes: Vec<String>,
    }
    let payload = response
        .json::<ServersPayload>()
        .await
        .map_err(|err| err.to_string())?;
    Ok(payload
        .servers
        .into_iter()
        .map(|server| McpServerRuntimeDto {
            server_id: server.server_id,
            title: server.title,
            enabled: server.enabled,
            connected: server.connected,
            tool_count: server.tool_count,
            allowed_lanes: server.allowed_lanes,
        })
        .collect())
}

async fn admin_refresh_catalog(base_url: &str) -> Result<(), String> {
    let client = http_client()?;
    let response = apply_admin_auth(
        client.post(format!(
            "{}/v1/admin/catalog/refresh",
            base_url.trim_end_matches('/')
        )),
    )
    .send()
    .await
    .map_err(|err| format!("cannot reach MCP gateway at {base_url}: {err}"))?;
    if response.status().is_success() {
        return Ok(());
    }
    Err(format!("catalog refresh returned HTTP {}", response.status()))
}

async fn reindex_daemon_capabilities(state: &tauri::State<'_, crate::daemon::DaemonState>) -> Result<(), String> {
    crate::daemon::sdk::client(state)
        .capabilities()
        .reindex()
        .await
        .map(|_| ())
        .map_err(crate::daemon::sdk::sdk_error)
}

fn bind_port(bind: &str) -> Option<u16> {
    bind.rsplit(':').next()?.parse().ok()
}

fn is_bind_reachable(bind: &str) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    if let Ok(mut addrs) = bind.to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
        }
    }
    false
}

#[cfg(unix)]
fn kill_process_on_port(port: u16) {
    let output = Command::new("lsof")
        .args(["-ti", &format!(":{port}")])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let pids = String::from_utf8_lossy(&output.stdout);
            for pid in pids.lines().map(str::trim).filter(|line| !line.is_empty()) {
                let _ = Command::new("kill").arg(pid).status();
            }
        }
    }
}

#[cfg(not(unix))]
fn kill_process_on_port(_port: u16) {}

fn resolve_gateway_binary() -> Result<crate::workshop_runtime::ComponentCommand, String> {
    if let Ok(explicit) = std::env::var("MEDOUSA_MCP_GATEWAY_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.exists() {
            return Ok(crate::workshop_runtime::ComponentCommand {
                program: path.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }
    if let Ok(current_exe) = std::env::current_exe() {
        let sibling = current_exe.with_file_name(crate::workshop_runtime::platform_binary_name("medousa_mcp_gateway"));
        if sibling.exists() {
            return Ok(crate::workshop_runtime::ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }
    if crate::workshop_runtime::find_command_in_path("medousa_mcp_gateway").is_some() {
        return Ok(crate::workshop_runtime::ComponentCommand {
            program: crate::workshop_runtime::platform_binary_name("medousa_mcp_gateway"),
            pre_args: Vec::new(),
        });
    }
    Err(
        "Medousa could not start the MCP gateway — reinstall Medousa or set MEDOUSA_MCP_GATEWAY_BIN for development.".to_string(),
    )
}

#[cfg(unix)]
fn detach_new_session(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
fn detach_new_session(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn detach_new_session(_command: &mut Command) {}

fn spawn_gateway_background(bind: &str) -> Result<(u32, PathBuf), String> {
    let gateway = resolve_gateway_binary()?;
    let log_path = gateway_log_path();
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|err| err.to_string())?;

    let mut command = Command::new(&gateway.program);
    command.args(&gateway.pre_args);
    command.arg("--bind").arg(bind);
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));
    detach_new_session(&mut command);

    let child = command
        .spawn()
        .map_err(|err| format!("failed to spawn MCP gateway ({}): {err}", gateway.program))?;
    Ok((child.id(), log_path))
}

async fn wait_for_gateway(bind: &str, timeout_seconds: u64) -> bool {
    let base_url = resolve_gateway_url();
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
    while Instant::now() < deadline {
        if is_bind_reachable(bind) && gateway_http_healthy(&base_url).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

#[tauri::command]
pub async fn mcp_gateway_load_config() -> Result<McpGatewayConfigLoadResult, String> {
    let (config, path, file_exists) = load_file_config()?;
    Ok(McpGatewayConfigLoadResult {
        path: path.display().to_string(),
        config,
        file_exists,
    })
}

#[tauri::command]
pub async fn mcp_gateway_status(
    state: tauri::State<'_, crate::daemon::DaemonState>,
) -> Result<McpGatewayStatusResult, String> {
    let (config, path, _) = load_file_config()?;
    let config_path = path.display().to_string();

    match crate::daemon::sdk::client(&state)
        .mcp_gateway()
        .status()
        .await
    {
        Ok(daemon_status) => Ok(merge_daemon_gateway_status(daemon_status, &config, config_path)),
        Err(err) => Ok(McpGatewayStatusResult {
            gateway_url: resolve_gateway_url(),
            reachable: false,
            message: format!(
                "Workshop unavailable — cannot check MCP gateway status ({err})"
            ),
            health: None,
            servers: servers_from_local_config(&config, false),
            config_path,
        }),
    }
}

fn merge_daemon_gateway_status(
    daemon_status: medousa_types::McpGatewayStatusResponse,
    config: &McpGatewayFileConfig,
    config_path: String,
) -> McpGatewayStatusResult {
    let servers = if daemon_status.servers.is_empty() {
        servers_from_local_config(config, daemon_status.reachable)
    } else {
        daemon_status
            .servers
            .into_iter()
            .map(|server| McpServerRuntimeDto {
                server_id: server.server_id,
                title: server.title,
                enabled: server.enabled,
                connected: server.connected,
                tool_count: server.tool_count,
                allowed_lanes: server.allowed_lanes,
            })
            .collect()
    };

    McpGatewayStatusResult {
        gateway_url: daemon_status.gateway_url,
        reachable: daemon_status.reachable,
        message: if daemon_status.reachable {
            daemon_status.message
        } else {
            format!(
                "{} — start it after adding servers (log: {})",
                daemon_status.message,
                gateway_log_path().display()
            )
        },
        health: daemon_status.health.map(|health| McpGatewayHealthDto {
            status: health.status,
            invokes_enabled: health.invokes_enabled,
            registered_servers: health.registered_servers,
            connected_servers: health.connected_servers,
            catalog_entries: health.catalog_entries,
        }),
        servers,
        config_path,
    }
}

fn servers_from_local_config(
    config: &McpGatewayFileConfig,
    connected: bool,
) -> Vec<McpServerRuntimeDto> {
    config
        .servers
        .iter()
        .map(|server| McpServerRuntimeDto {
            server_id: server.id.clone(),
            title: server.title.clone(),
            enabled: server.enabled,
            connected,
            tool_count: 0,
            allowed_lanes: server.allowed_lanes.clone(),
        })
        .collect()
}

async fn perform_mcp_gateway_restart() -> Result<(McpGatewayRestartResult, bool), String> {
    let (config, _, _) = load_file_config()?;
    let bind = config.gateway.bind.trim();
    let log_path = gateway_log_path();
    let base_url = resolve_gateway_url();

    if gateway_http_healthy(&base_url).await {
        if let Some(port) = bind_port(bind) {
            kill_process_on_port(port);
            tokio::time::sleep(Duration::from_millis(750)).await;
        }
    }

    if gateway_http_healthy(&base_url).await {
        return Ok((
            McpGatewayRestartResult {
                started: false,
                already_running: true,
                log_path: log_path.display().to_string(),
                message: format!("MCP gateway already running at {base_url}"),
            },
            true,
        ));
    }

    if is_bind_reachable(bind) {
        return Err(format!(
            "Port {bind} is open but the MCP gateway is not responding — check {}",
            log_path.display()
        ));
    }

    let (pid, log_path) = spawn_gateway_background(bind)?;
    let ready = wait_for_gateway(bind, 15).await;
    Ok((
        McpGatewayRestartResult {
            started: true,
            already_running: false,
            log_path: log_path.display().to_string(),
            message: if ready {
                format!("MCP gateway restarted (pid {pid})")
            } else {
                format!(
                    "MCP gateway started (pid {pid}) but is not healthy yet — check {}",
                    log_path.display()
                )
            },
        },
        ready,
    ))
}

#[tauri::command]
pub async fn mcp_gateway_restart(
    state: tauri::State<'_, crate::daemon::DaemonState>,
) -> Result<McpGatewayRestartResult, String> {
    let (result, ready) = perform_mcp_gateway_restart().await?;
    if ready {
        let _ = reindex_daemon_capabilities(&state).await;
    }
    Ok(result)
}

#[tauri::command]
pub async fn mcp_gateway_upsert_server(
    request: McpServerUpsertRequest,
) -> Result<McpServerMutationResult, String> {
    let server = validate_server(&request)?;
    let (mut config, _, _) = load_file_config()?;
    if let Some(existing) = config
        .servers
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(&server.id))
    {
        *existing = server;
    } else {
        config.servers.push(server);
    }
    let path = persist_file_config(&config)?;
    Ok(McpServerMutationResult {
        ok: true,
        message: "Server saved — restart the MCP gateway to apply".to_string(),
        config_path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn mcp_gateway_remove_server(server_id: String) -> Result<McpServerMutationResult, String> {
    let id = normalize_server_id(&server_id)?;
    let (mut config, _, _) = load_file_config()?;
    let before = config.servers.len();
    config.servers.retain(|entry| !entry.id.eq_ignore_ascii_case(&id));
    if config.servers.len() == before {
        return Err(format!("unknown MCP server '{id}'"));
    }
    let path = persist_file_config(&config)?;
    Ok(McpServerMutationResult {
        ok: true,
        message: "Server removed — restart the MCP gateway to apply".to_string(),
        config_path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn mcp_gateway_set_server_enabled(
    server_id: String,
    enabled: bool,
) -> Result<McpServerMutationResult, String> {
    let id = normalize_server_id(&server_id)?;
    let (mut config, _, _) = load_file_config()?;
    let entry = config
        .servers
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))?;
    entry.enabled = enabled;
    let path = persist_file_config(&config)?;
    Ok(McpServerMutationResult {
        ok: true,
        message: if enabled {
            "Server enabled — restart the MCP gateway to apply".to_string()
        } else {
            "Server disabled — restart the MCP gateway to apply".to_string()
        },
        config_path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn mcp_gateway_apply_server(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    request: McpServerUpsertRequest,
) -> Result<McpGatewayTestResult, String> {
    mcp_gateway_upsert_server(request.clone()).await?;
    let (_, ready) = perform_mcp_gateway_restart().await?;
    let gateway_url = resolve_gateway_url();
    let _ = admin_refresh_catalog(&gateway_url).await;
    if ready {
        let _ = reindex_daemon_capabilities(&state).await;
    }
    tokio::time::sleep(Duration::from_millis(750)).await;

    let id = normalize_server_id(&request.id)?;
    let servers = fetch_runtime_servers(&gateway_url).await.unwrap_or_default();
    let runtime = servers
        .iter()
        .find(|server| server.server_id.eq_ignore_ascii_case(&id));

    if let Some(runtime) = runtime {
        return Ok(McpGatewayTestResult {
            ok: runtime.connected || request.use_mock,
            connected: runtime.connected,
            tool_count: runtime.tool_count,
            message: if runtime.connected {
                format!(
                    "{} connected with {} tool(s)",
                    runtime.title, runtime.tool_count
                )
            } else if request.use_mock {
                "Mock server registered — tools appear after catalog refresh".to_string()
            } else {
                format!(
                    "{} saved but not connected — check URL, auth token, transport, and {}",
                    runtime.title,
                    gateway_log_path().display()
                )
            },
        });
    }

    Ok(McpGatewayTestResult {
        ok: false,
        connected: false,
        tool_count: 0,
        message: "Server saved and gateway restarted, but runtime status is unavailable".to_string(),
    })
}
