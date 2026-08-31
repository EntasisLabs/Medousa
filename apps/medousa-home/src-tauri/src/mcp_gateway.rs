//! MCP gateway config + lifecycle for Home — read/write `mcp-gateway.toml`, probe gateway HTTP.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_GATEWAY_URL: &str = "http://127.0.0.1:7420";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayConfigLoadResult {
    pub path: String,
    pub config: medousa_mcp_gateway::McpGatewayFileConfig,
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
    pub tool_tags: Option<HashMap<String, Vec<String>>>,
    #[serde(default)]
    pub disabled_tools: Option<Vec<String>>,
    #[serde(default)]
    pub use_mock: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayToolDto {
    pub tool_name: String,
    pub title: String,
    pub enabled: bool,
    pub available: bool,
    pub capability_ids: Vec<String>,
    pub discovery_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpGatewayToolsResult {
    pub tools: Vec<McpGatewayToolDto>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolUpdateRequest {
    pub server_id: String,
    pub tool_name: String,
    pub enabled: bool,
    #[serde(default)]
    pub discovery_hints: Vec<String>,
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

fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn gateway_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("mcp-gateway.log")
}

fn active_workshop_uses_local_mcp_config() -> Result<bool, String> {
    Ok(match crate::active_workshop::resolve()? {
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal => true,
        crate::active_workshop::ActiveWorkshopTarget::Transport { workshop, .. } => {
            workshop.kind == "local"
        }
    })
}

fn require_local_mcp_config() -> Result<(), String> {
    if active_workshop_uses_local_mcp_config()? {
        Ok(())
    } else {
        Err("MCP configuration is managed by the selected workshop".to_string())
    }
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

fn normalize_tool_tags(raw: HashMap<String, Vec<String>>) -> HashMap<String, Vec<String>> {
    raw.into_iter()
        .filter_map(|(tool_name, hints)| {
            let tool_name = tool_name.trim().to_string();
            if tool_name.is_empty() {
                return None;
            }
            let mut normalized = Vec::new();
            for hint in hints {
                let hint = hint.trim();
                if hint.is_empty()
                    || normalized
                        .iter()
                        .any(|existing: &String| existing.eq_ignore_ascii_case(hint))
                {
                    continue;
                }
                normalized.push(hint.to_string());
            }
            (!normalized.is_empty()).then_some((tool_name, normalized))
        })
        .collect()
}

fn normalize_disabled_tools(raw: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for tool_name in raw {
        let tool_name = tool_name.trim();
        if tool_name.is_empty()
            || normalized
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(tool_name))
        {
            continue;
        }
        normalized.push(tool_name.to_string());
    }
    normalized
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn count_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn install_starter_if_missing() -> Result<PathBuf, String> {
    medousa_mcp_gateway::install_starter_gateway_config_if_missing().map_err(|err| err.to_string())
}

fn load_file_config() -> Result<(medousa_mcp_gateway::McpGatewayFileConfig, PathBuf, bool), String>
{
    let path = install_starter_if_missing()?;
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let config = toml::from_str::<medousa_mcp_gateway::McpGatewayFileConfig>(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    Ok((config, path, true))
}

fn persist_file_config(
    config: &medousa_mcp_gateway::McpGatewayFileConfig,
) -> Result<PathBuf, String> {
    let path = install_starter_if_missing()?;
    let encoded = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(&path, encoded).map_err(|err| err.to_string())?;
    Ok(path)
}

fn persist_server(server: medousa_mcp_gateway::McpServerConfig) -> Result<PathBuf, String> {
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
    persist_file_config(&config)
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

fn normalize_transport(raw: &str) -> Result<String, String> {
    let transport = raw.trim().to_ascii_lowercase();
    match transport.as_str() {
        "stdio" | "http" | "streamable" | "streamable-http" | "sse" => Ok(transport),
        _ => Err(format!(
            "Unsupported transport '{raw}' — use stdio, http, or sse"
        )),
    }
}

fn validate_server(
    request: &McpServerUpsertRequest,
) -> Result<medousa_mcp_gateway::McpServerConfig, String> {
    let id = normalize_server_id(&request.id)?;
    let title = request.title.trim();
    if title.is_empty() {
        return Err("Title is required".to_string());
    }
    let transport = normalize_transport(&request.transport)?;
    let tool_tags = normalize_tool_tags(request.tool_tags.clone().unwrap_or_default());
    let disabled_tools =
        normalize_disabled_tools(request.disabled_tools.clone().unwrap_or_default());

    if request.use_mock {
        return Ok(medousa_mcp_gateway::McpServerConfig {
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
            tool_tags,
            disabled_tools,
            use_mock: true,
        });
    }

    if transport == "http"
        || transport == "streamable"
        || transport == "streamable-http"
        || transport == "sse"
    {
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
        return Ok(medousa_mcp_gateway::McpServerConfig {
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
            tool_tags,
            disabled_tools,
            use_mock: false,
        });
    }

    let command = request
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Command is required for stdio MCP servers (or enable mock mode)".to_string()
        })?;
    Ok(medousa_mcp_gateway::McpServerConfig {
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
        tool_tags,
        disabled_tools,
        use_mock: false,
    })
}

fn server_from_request(
    request: &McpServerUpsertRequest,
) -> Result<medousa_mcp_gateway::McpServerConfig, String> {
    let mut server = validate_server(request)?;
    if request.tool_tags.is_some() && request.disabled_tools.is_some() {
        return Ok(server);
    }

    let (config, _, _) = load_file_config()?;
    if let Some(existing) = config
        .servers
        .iter()
        .find(|entry| entry.id.eq_ignore_ascii_case(&server.id))
    {
        if request.tool_tags.is_none() {
            server.tool_tags = existing.tool_tags.clone();
        }
        if request.disabled_tools.is_none() {
            server.disabled_tools = existing.disabled_tools.clone();
        }
    }
    Ok(server)
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
        client.get(format!("{}/v1/mcp/servers", base_url.trim_end_matches('/'))),
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

async fn fetch_runtime_catalog(
    base_url: &str,
) -> Result<medousa_types::mcp_gateway_api::McpCatalogSyncResponse, String> {
    let client = http_client()?;
    let response = apply_gateway_auth(
        client.get(format!("{}/v1/mcp/catalog", base_url.trim_end_matches('/'))),
    )
    .send()
    .await
    .map_err(|err| format!("cannot reach MCP gateway at {base_url}: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("MCP gateway returned HTTP {}", response.status()));
    }
    response.json().await.map_err(|err| err.to_string())
}

async fn admin_refresh_catalog(base_url: &str) -> Result<(), String> {
    let client = http_client()?;
    let response = apply_admin_auth(client.post(format!(
        "{}/v1/admin/catalog/refresh",
        base_url.trim_end_matches('/')
    )))
    .send()
    .await
    .map_err(|err| format!("cannot reach MCP gateway at {base_url}: {err}"))?;
    if response.status().is_success() {
        return Ok(());
    }
    Err(format!(
        "catalog refresh returned HTTP {}",
        response.status()
    ))
}

async fn reindex_daemon_capabilities(
    state: &tauri::State<'_, crate::daemon::DaemonState>,
) -> Result<(), String> {
    crate::daemon::sdk::client(state)?
        .capabilities()
        .reindex()
        .await
        .map(|_| ())
        .map_err(crate::daemon::sdk::sdk_error)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn reload_embedded_mcp(
    client: &medousa::embedded_daemon::EmbeddedDaemonClient,
) -> Result<(), String> {
    let config = medousa_mcp_gateway::McpGatewayFullConfig::from_env_and_args(&[]).remote_only();
    client
        .reconfigure_mcp_gateway(config)
        .await
        .map_err(|error| format!("reload embedded MCP adapter: {error:#}"))?;
    client
        .reindex_capabilities()
        .await
        .map(|_| ())
        .map_err(|error| format!("reindex embedded MCP capabilities: {error:#}"))
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
        let sibling = current_exe.with_file_name(crate::workshop_runtime::platform_binary_name(
            "medousa_mcp_gateway",
        ));
        if sibling.exists() {
            return Ok(crate::workshop_runtime::ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }
    if let Some(shared) = crate::workshop_runtime::shared_bin_binary("medousa_mcp_gateway") {
        return Ok(crate::workshop_runtime::ComponentCommand {
            program: shared.to_string_lossy().to_string(),
            pre_args: Vec::new(),
        });
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
    command.env("MEDOUSA_DATA_DIR", medousa_data_dir());
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
    require_local_mcp_config()?;
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
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<McpGatewayStatusResult, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        let (_, path, _) = load_file_config()?;
        let (health, servers) = client
            .mcp_gateway_status()
            .await
            .map_err(|error| format!("embedded MCP adapter status: {error:#}"))?;
        return Ok(McpGatewayStatusResult {
            gateway_url: "in-process://embedded".to_string(),
            reachable: true,
            message: "Embedded MCP adapter is running".to_string(),
            health: Some(McpGatewayHealthDto {
                status: health.status,
                invokes_enabled: health.invokes_enabled,
                registered_servers: count_u32(health.registered_servers),
                connected_servers: count_u32(health.connected_servers),
                catalog_entries: count_u32(health.catalog_entries),
            }),
            servers: servers
                .servers
                .into_iter()
                .map(|server| McpServerRuntimeDto {
                    server_id: server.server_id,
                    title: server.title,
                    enabled: server.enabled,
                    connected: server.connected,
                    tool_count: count_u32(server.tool_count),
                    allowed_lanes: server.allowed_lanes,
                })
                .collect(),
            config_path: path.display().to_string(),
        });
    }

    let (config, config_path) = if active_workshop_uses_local_mcp_config()? {
        let (config, path, _) = load_file_config()?;
        (config, path.display().to_string())
    } else {
        (
            medousa_mcp_gateway::McpGatewayFileConfig {
                gateway: medousa_mcp_gateway::GatewaySection::default(),
                servers: Vec::new(),
            },
            String::new(),
        )
    };

    let status = match crate::daemon::sdk::client(&state) {
        Ok(client) => client.mcp_gateway().status().await,
        Err(err) => {
            return Ok(McpGatewayStatusResult {
                gateway_url: resolve_gateway_url(),
                reachable: false,
                message: format!("Workshop unavailable — cannot check MCP gateway status ({err})"),
                health: None,
                servers: servers_from_local_config(&config, false),
                config_path,
            });
        }
    };

    match status {
        Ok(daemon_status) => Ok(merge_daemon_gateway_status(
            daemon_status,
            &config,
            config_path,
        )),
        Err(err) => Ok(McpGatewayStatusResult {
            gateway_url: resolve_gateway_url(),
            reachable: false,
            message: format!("Workshop unavailable — cannot check MCP gateway status ({err})"),
            health: None,
            servers: servers_from_local_config(&config, false),
            config_path,
        }),
    }
}

#[tauri::command]
pub async fn mcp_gateway_list_tools(
    _state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: String,
) -> Result<McpGatewayToolsResult, String> {
    require_local_mcp_config()?;
    let id = normalize_server_id(&server_id)?;
    let (config, _, _) = load_file_config()?;
    let server = config
        .servers
        .iter()
        .find(|entry| entry.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))?;

    #[cfg(any(target_os = "ios", target_os = "android"))]
    let catalog_result = {
        let client = _embedded_state
            .client_if_active()
            .await?
            .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;
        client
            .mcp_gateway_catalog()
            .await
            .map_err(|error| format!("read embedded MCP catalog: {error:#}"))
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let catalog_result = fetch_runtime_catalog(&resolve_gateway_url()).await;

    let (catalog, message) = match catalog_result {
        Ok(catalog) => (Some(catalog), String::new()),
        Err(error) => (
            None,
            format!("Live tools are unavailable until the MCP gateway reconnects ({error})"),
        ),
    };

    let mut tools = HashMap::<String, McpGatewayToolDto>::new();
    if let Some(catalog) = catalog {
        for entry in catalog
            .entries
            .into_iter()
            .filter(|entry| entry.server_id.eq_ignore_ascii_case(&id))
        {
            let key = entry.tool_name.to_ascii_lowercase();
            tools.insert(
                key,
                McpGatewayToolDto {
                    enabled: server.tool_enabled(&entry.tool_name),
                    discovery_hints: server
                        .tool_tags
                        .iter()
                        .find_map(|(name, hints)| {
                            name.eq_ignore_ascii_case(&entry.tool_name)
                                .then_some(hints.clone())
                        })
                        .unwrap_or_default(),
                    tool_name: entry.tool_name,
                    title: entry.title,
                    available: entry.available,
                    capability_ids: entry.capability_ids,
                },
            );
        }
    }

    for tool_name in server.tool_tags.keys().chain(server.disabled_tools.iter()) {
        let key = tool_name.to_ascii_lowercase();
        tools.entry(key).or_insert_with(|| McpGatewayToolDto {
            tool_name: tool_name.clone(),
            title: tool_name.clone(),
            enabled: server.tool_enabled(tool_name),
            available: false,
            capability_ids: Vec::new(),
            discovery_hints: server
                .tool_tags
                .iter()
                .find_map(|(name, hints)| {
                    name.eq_ignore_ascii_case(tool_name)
                        .then_some(hints.clone())
                })
                .unwrap_or_default(),
        });
    }

    for tool in tools.values_mut() {
        if let Some(configured) = server
            .tool_tags
            .iter()
            .find_map(|(name, hints)| name.eq_ignore_ascii_case(&tool.tool_name).then_some(hints))
        {
            for hint in configured {
                if !tool
                    .capability_ids
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(hint))
                {
                    tool.capability_ids.push(hint.clone());
                }
            }
        }
        tool.enabled = server.tool_enabled(&tool.tool_name);
        tool.available &= tool.enabled;
    }

    let mut tools = tools.into_values().collect::<Vec<_>>();
    tools.sort_by(|left, right| {
        left.title
            .to_ascii_lowercase()
            .cmp(&right.title.to_ascii_lowercase())
            .then_with(|| left.tool_name.cmp(&right.tool_name))
    });
    Ok(McpGatewayToolsResult { tools, message })
}

#[tauri::command]
pub async fn mcp_oauth_status(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: String,
) -> Result<medousa_types::McpOAuthStatusResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .mcp_oauth_status(&server_id)
            .await
            .map_err(|error| format!("read MCP connection: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = _embedded_state;
    crate::daemon::workshop_http::get_json(&state, &format!("/v1/mcp/oauth/{}", server_id.trim()))
        .await
}

/// Run the host browser ceremony while the selected runtime remains the sole
/// owner of OAuth discovery, state validation, token exchange, and storage.
#[tauri::command]
pub async fn mcp_oauth_authorize(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::daemon::DaemonState>,
    embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: String,
) -> Result<medousa_types::CompleteMcpOAuthResponse, String> {
    let browser = crate::oauth_browser::OAuthBrowserSession::bind().await?;
    let login = mcp_oauth_begin(
        state.clone(),
        embedded_state.clone(),
        medousa_types::BeginMcpOAuthRequest {
            server_id,
            redirect_uri: browser.redirect_uri().to_string(),
            scopes: Vec::new(),
            client_metadata_url: None,
            client_id: None,
            client_secret: None,
            challenge: None,
        },
    )
    .await?;

    let callback = browser.authorize(&app, &login.authorization_url).await?;
    let result = mcp_oauth_complete(
        state,
        embedded_state,
        medousa_types::CompleteMcpOAuthRequest {
            login_id: login.login_id,
            callback_url: callback.url().to_string(),
        },
    )
    .await;
    callback.finish(&app, result.is_ok()).await;
    result
}

#[tauri::command]
pub async fn mcp_oauth_begin(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: medousa_types::BeginMcpOAuthRequest,
) -> Result<medousa_types::BeginMcpOAuthResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .begin_mcp_oauth(request)
            .await
            .map_err(|error| format!("begin MCP authorization: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = _embedded_state;
    crate::daemon::workshop_http::post_json(&state, "/v1/mcp/oauth/begin", &request).await
}

#[tauri::command]
pub async fn mcp_oauth_complete(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: medousa_types::CompleteMcpOAuthRequest,
) -> Result<medousa_types::CompleteMcpOAuthResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .complete_mcp_oauth(request)
            .await
            .map_err(|error| format!("complete MCP authorization: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = _embedded_state;
    crate::daemon::workshop_http::post_json(&state, "/v1/mcp/oauth/complete", &request).await
}

#[tauri::command]
pub async fn mcp_oauth_refresh(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: medousa_types::RefreshMcpOAuthRequest,
) -> Result<medousa_types::McpOAuthStatusResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .refresh_mcp_oauth(&request.server_id)
            .await
            .map_err(|error| format!("refresh MCP connection: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = _embedded_state;
    crate::daemon::workshop_http::post_json(&state, "/v1/mcp/oauth/refresh", &request).await
}

#[tauri::command]
pub async fn mcp_oauth_disconnect(
    state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: String,
) -> Result<medousa_types::DisconnectMcpOAuthResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .disconnect_mcp_oauth(&server_id)
            .await
            .map_err(|error| format!("disconnect MCP authorization: {error:#}"));
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = _embedded_state;
    crate::daemon::workshop_http::delete_json(
        &state,
        &format!("/v1/mcp/oauth/{}", server_id.trim()),
    )
    .await
}

fn merge_daemon_gateway_status(
    daemon_status: medousa_types::McpGatewayStatusResponse,
    config: &medousa_mcp_gateway::McpGatewayFileConfig,
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
    config: &medousa_mcp_gateway::McpGatewayFileConfig,
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
    _state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
) -> Result<McpGatewayRestartResult, String> {
    require_local_mcp_config()?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let client = _embedded_state
            .client_if_active()
            .await?
            .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;
        reload_embedded_mcp(&client).await?;
        return Ok(McpGatewayRestartResult {
            started: false,
            already_running: true,
            log_path: String::new(),
            message: "Embedded MCP catalog reloaded".to_string(),
        });
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let (result, ready) = perform_mcp_gateway_restart().await?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if ready {
        let _ = reindex_daemon_capabilities(&_state).await;
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Ok(result)
}

#[tauri::command]
pub async fn mcp_gateway_upsert_server(
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: McpServerUpsertRequest,
) -> Result<McpServerMutationResult, String> {
    require_local_mcp_config()?;
    let server = server_from_request(&request)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if server.transport == "stdio" && !server.use_mock {
        return Err("Embedded MCP supports hosted HTTP and SSE servers only".to_string());
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let embedded_client = _embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;
    let path = persist_server(server)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        reload_embedded_mcp(&embedded_client).await?;
        return Ok(McpServerMutationResult {
            ok: true,
            message: "Server saved and applied".to_string(),
            config_path: path.display().to_string(),
        });
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Ok(McpServerMutationResult {
        ok: true,
        message: "Server saved — restart the MCP gateway to apply".to_string(),
        config_path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn mcp_gateway_remove_server(
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: String,
) -> Result<McpServerMutationResult, String> {
    require_local_mcp_config()?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let embedded_client = _embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;
    let id = normalize_server_id(&server_id)?;
    let (mut config, _, _) = load_file_config()?;
    let before = config.servers.len();
    config
        .servers
        .retain(|entry| !entry.id.eq_ignore_ascii_case(&id));
    if config.servers.len() == before {
        return Err(format!("unknown MCP server '{id}'"));
    }
    let path = persist_file_config(&config)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        reload_embedded_mcp(&embedded_client).await?;
        return Ok(McpServerMutationResult {
            ok: true,
            message: "Server removed".to_string(),
            config_path: path.display().to_string(),
        });
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Ok(McpServerMutationResult {
        ok: true,
        message: "Server removed — restart the MCP gateway to apply".to_string(),
        config_path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn mcp_gateway_set_server_enabled(
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    server_id: String,
    enabled: bool,
) -> Result<McpServerMutationResult, String> {
    require_local_mcp_config()?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let embedded_client = _embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;
    let id = normalize_server_id(&server_id)?;
    let (mut config, _, _) = load_file_config()?;
    let entry = config
        .servers
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))?;
    entry.enabled = enabled;
    let path = persist_file_config(&config)?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        reload_embedded_mcp(&embedded_client).await?;
        return Ok(McpServerMutationResult {
            ok: true,
            message: if enabled {
                "Server enabled".to_string()
            } else {
                "Server disabled".to_string()
            },
            config_path: path.display().to_string(),
        });
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
pub async fn mcp_gateway_update_tool(
    _state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: McpToolUpdateRequest,
) -> Result<McpServerMutationResult, String> {
    require_local_mcp_config()?;
    let id = normalize_server_id(&request.server_id)?;
    let tool_name = request.tool_name.trim();
    if tool_name.is_empty() {
        return Err("Tool name is required".to_string());
    }
    if tool_name.chars().any(char::is_control) {
        return Err("Tool name contains unsupported control characters".to_string());
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    let embedded_client = _embedded_state
        .client_if_active()
        .await?
        .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;

    let (mut config, _, _) = load_file_config()?;
    let server = config
        .servers
        .iter_mut()
        .find(|entry| entry.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| format!("unknown MCP server '{id}'"))?;
    let canonical_name = server
        .tool_tags
        .keys()
        .find(|existing| existing.eq_ignore_ascii_case(tool_name))
        .cloned()
        .or_else(|| {
            server
                .disabled_tools
                .iter()
                .find(|existing| existing.eq_ignore_ascii_case(tool_name))
                .cloned()
        })
        .unwrap_or_else(|| tool_name.to_string());

    server
        .tool_tags
        .retain(|name, _| !name.eq_ignore_ascii_case(tool_name));
    let hints = normalize_tool_tags(HashMap::from([(
        canonical_name.clone(),
        request.discovery_hints,
    )]));
    if let Some(hints) = hints.get(&canonical_name) {
        server
            .tool_tags
            .insert(canonical_name.clone(), hints.clone());
    }

    server
        .disabled_tools
        .retain(|disabled| !disabled.eq_ignore_ascii_case(tool_name));
    if !request.enabled {
        server.disabled_tools.push(canonical_name.clone());
    }
    server.disabled_tools = normalize_disabled_tools(std::mem::take(&mut server.disabled_tools));
    let path = persist_file_config(&config)?;

    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        reload_embedded_mcp(&embedded_client).await?;
        return Ok(McpServerMutationResult {
            ok: true,
            message: format!("{} updated", canonical_name),
            config_path: path.display().to_string(),
        });
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let (_, ready) = perform_mcp_gateway_restart().await?;
        let gateway_url = resolve_gateway_url();
        let _ = admin_refresh_catalog(&gateway_url).await;
        if ready {
            let _ = reindex_daemon_capabilities(&_state).await;
        }
        Ok(McpServerMutationResult {
            ok: true,
            message: if ready {
                format!("{} updated", canonical_name)
            } else {
                format!(
                    "{} saved — the MCP gateway is still starting",
                    canonical_name
                )
            },
            config_path: path.display().to_string(),
        })
    }
}

#[tauri::command]
pub async fn mcp_gateway_apply_server(
    _state: tauri::State<'_, crate::daemon::DaemonState>,
    _embedded_state: tauri::State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    request: McpServerUpsertRequest,
) -> Result<McpGatewayTestResult, String> {
    require_local_mcp_config()?;
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let client = _embedded_state
            .client_if_active()
            .await?
            .ok_or_else(|| "MCP configuration is managed by the selected workshop".to_string())?;
        let server = server_from_request(&request)?;
        if server.transport == "stdio" && !server.use_mock {
            return Err("Embedded MCP supports hosted HTTP and SSE servers only".to_string());
        }
        persist_server(server)?;
        reload_embedded_mcp(&client).await?;
        let (_, servers) = client
            .mcp_gateway_status()
            .await
            .map_err(|error| format!("embedded MCP adapter status: {error:#}"))?;
        let id = normalize_server_id(&request.id)?;
        let runtime = servers
            .servers
            .iter()
            .find(|server| server.server_id.eq_ignore_ascii_case(&id));
        return Ok(match runtime {
            Some(runtime) => McpGatewayTestResult {
                ok: runtime.connected || request.use_mock,
                connected: runtime.connected,
                tool_count: count_u32(runtime.tool_count),
                message: if runtime.connected {
                    format!(
                        "{} connected with {} tool(s)",
                        runtime.title, runtime.tool_count
                    )
                } else {
                    format!("{} saved but did not connect", runtime.title)
                },
            },
            None => McpGatewayTestResult {
                ok: false,
                connected: false,
                tool_count: 0,
                message: "Server saved, but runtime status is unavailable".to_string(),
            },
        });
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    mcp_gateway_upsert_server(_embedded_state, request.clone()).await?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let (_, ready) = perform_mcp_gateway_restart().await?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let gateway_url = resolve_gateway_url();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _ = admin_refresh_catalog(&gateway_url).await;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if ready {
        let _ = reindex_daemon_capabilities(&_state).await;
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    tokio::time::sleep(Duration::from_millis(750)).await;

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let id = normalize_server_id(&request.id)?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let servers = fetch_runtime_servers(&gateway_url)
        .await
        .unwrap_or_default();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let runtime = servers
        .iter()
        .find(|server| server.server_id.eq_ignore_ascii_case(&id));

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Ok(McpGatewayTestResult {
        ok: false,
        connected: false,
        tool_count: 0,
        message: "Server saved and gateway restarted, but runtime status is unavailable"
            .to_string(),
    })
}
