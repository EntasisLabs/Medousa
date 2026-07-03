//! Per-workshop process management: slim `medousa_daemon` (:7419) and optional `medousa_local` brain (:7421).

use crate::medousa_paths::{load_tui_defaults_summary, tui_defaults_path};
use crate::workshop_registry::{WorkshopRegistry, WorkshopServer, PERSONAL_WORKSHOP_ID};
use reqwest::Client;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_LOCAL_BIND: &str = "127.0.0.1:7419";
const PUBLIC_LOCAL_BIND: &str = "0.0.0.0:7419";
const DEFAULT_BACKEND: &str = "surreal-mem";
const LOCAL_PORT_START: u16 = 7419;
const LOCAL_PORT_END: u16 = 7499;

pub(crate) struct ComponentCommand {
    pub program: String,
    pub pre_args: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineEnsureResult {
    pub ok: bool,
    pub already_running: bool,
    pub started: bool,
    pub pid: Option<u32>,
    pub url: String,
    pub bind: String,
    pub data_dir: String,
    pub log_path: String,
    pub message: String,
}

pub fn public_local_bind() -> &'static str {
    PUBLIC_LOCAL_BIND
}

pub(crate) fn resolve_backend() -> String {
    if let Ok(raw) = fs::read_to_string(tui_defaults_path()) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(backend) = json.get("backend").and_then(|value| value.as_str()) {
                let trimmed = backend.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    DEFAULT_BACKEND.to_string()
}

pub(crate) fn platform_binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub(crate) fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let names = if cfg!(windows) {
        vec![platform_binary_name(command), command.to_string()]
    } else {
        vec![command.to_string()]
    };
    std::env::split_paths(&path_var).find_map(|dir| {
        names
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

pub(crate) fn resolve_daemon_binary() -> Result<ComponentCommand, String> {
    if let Ok(explicit) = std::env::var("MEDOUSA_MEDOUSA_DAEMON_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.exists() {
            return Ok(ComponentCommand {
                program: path.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        let sibling = current_exe.with_file_name(platform_binary_name("medousa_daemon"));
        if sibling.exists() {
            return Ok(ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if find_command_in_path("medousa_daemon").is_some() {
        return Ok(ComponentCommand {
            program: platform_binary_name("medousa_daemon"),
            pre_args: Vec::new(),
        });
    }

    Err(
        "Medousa could not start — the app bundle may be incomplete. Reinstall Medousa, or set MEDOUSA_MEDOUSA_DAEMON_BIN for development.".to_string(),
    )
}

pub(crate) fn resolve_local_binary() -> Result<ComponentCommand, String> {
    if let Ok(explicit) = std::env::var("MEDOUSA_MEDOUSA_LOCAL_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.exists() {
            return Ok(ComponentCommand {
                program: path.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        let sibling = current_exe.with_file_name(platform_binary_name("medousa_local"));
        if sibling.exists() {
            return Ok(ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if find_command_in_path("medousa_local").is_some() {
        return Ok(ComponentCommand {
            program: platform_binary_name("medousa_local"),
            pre_args: Vec::new(),
        });
    }

    Err(
        "Offline brain is not installed. Run Medousa Installer and add the Offline brain package."
            .to_string(),
    )
}

pub(crate) fn local_brain_installed() -> bool {
    resolve_local_binary().is_ok()
}

pub(crate) fn is_bind_reachable(bind: &str) -> bool {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        return medousa_host::is_bind_reachable(bind);
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        use std::net::{TcpStream, ToSocketAddrs};
        if let Ok(mut addrs) = bind.to_socket_addrs() {
            if let Some(addr) = addrs.next() {
                return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
            }
        }
        false
    }
}

pub(crate) fn should_load_private_brain(explicit: bool) -> bool {
    if explicit {
        return true;
    }
    load_tui_defaults_summary()
        .provider
        .as_deref()
        .map(str::trim)
        .is_some_and(|provider| provider.eq_ignore_ascii_case("medousa-local"))
}

fn apply_daemon_messaging_env(command: &mut Command) {
    if let Ok(Some(token)) = crate::messaging::secrets::load_secret_value("telegram_bot_token") {
        command.env("MEDOUSA_TELEGRAM_BOT_TOKEN", token);
    }
    if let Ok(Some(token)) = crate::messaging::secrets::load_secret_value("discord_bot_token") {
        command.env("MEDOUSA_DISCORD_BOT_TOKEN", token);
    }
    if let Ok(Some(token)) = crate::messaging::secrets::load_secret_value("slack_bot_token") {
        command.env("MEDOUSA_SLACK_BOT_TOKEN", token);
    }
    if let Ok(Some(token)) = crate::messaging::secrets::load_secret_value("slack_app_token") {
        command.env("MEDOUSA_SLACK_APP_TOKEN", token);
    }
}

fn apply_daemon_apns_env(command: &mut Command) {
    const KEYS: &[&str] = &[
        "MEDOUSA_APNS_TEAM_ID",
        "MEDOUSA_APNS_KEY_ID",
        "MEDOUSA_APNS_KEY_PATH",
        "MEDOUSA_APNS_BUNDLE_ID",
        "MEDOUSA_APNS_SANDBOX",
    ];
    for key in KEYS {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                command.env(key, value);
            }
        }
    }
}

#[cfg(unix)]
fn detach_new_session(command: &mut Command) {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        medousa_host::detach_new_session(command);
        return;
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        use std::io;
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
}

#[cfg(not(unix))]
fn detach_new_session(_command: &mut Command) {}

pub fn engine_runtime_dir(workshop_id: &str) -> PathBuf {
    crate::paths::medousa_data_dir()
        .join("engines")
        .join(workshop_id)
}

pub fn daemon_pid_path(workshop_id: &str) -> PathBuf {
    engine_runtime_dir(workshop_id).join("daemon.pid")
}

pub fn daemon_log_path(workshop_id: &str) -> PathBuf {
    engine_runtime_dir(workshop_id).join("daemon.log")
}

pub fn local_brain_pid_path(workshop_id: &str) -> PathBuf {
    engine_runtime_dir(workshop_id).join("local.pid")
}

pub fn local_brain_log_path(workshop_id: &str) -> PathBuf {
    engine_runtime_dir(workshop_id).join("local.log")
}

pub const DEFAULT_LOCAL_BRAIN_BIND: &str = "127.0.0.1:7421";

fn legacy_daemon_pid_path() -> PathBuf {
    crate::paths::medousa_data_dir().join("daemon.pid")
}

fn legacy_daemon_log_path() -> PathBuf {
    crate::paths::medousa_data_dir().join("logs").join("daemon.log")
}

pub fn url_from_bind(bind: &str) -> String {
    format!("http://{}", bind.trim())
}

pub fn parse_bind_port(bind: &str) -> Option<u16> {
    bind.rsplit(':').next()?.parse().ok()
}

pub fn resolve_workshop_bind(workshop: &WorkshopServer) -> String {
    workshop
        .bind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_LOCAL_BIND.to_string())
}

pub fn resolve_workshop_data_dir(workshop: &WorkshopServer) -> PathBuf {
    if let Some(raw) = workshop.data_dir.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        return PathBuf::from(raw);
    }
    crate::paths::medousa_data_dir()
}

pub fn resolve_workshop_url(workshop: &WorkshopServer) -> String {
    if workshop.kind == "local" {
        url_from_bind(&resolve_workshop_bind(workshop))
    } else {
        workshop.url.trim().trim_end_matches('/').to_string()
    }
}

pub fn allocate_local_bind(registry: &WorkshopRegistry) -> Result<String, String> {
    let mut used_ports = HashSet::new();
    for workshop in registry
        .workshops
        .iter()
        .filter(|entry| entry.kind == "local")
    {
        if let Some(port) = parse_bind_port(&resolve_workshop_bind(workshop)) {
            used_ports.insert(port);
        }
    }

    for port in LOCAL_PORT_START..=LOCAL_PORT_END {
        if used_ports.contains(&port) {
            continue;
        }
        let bind = format!("127.0.0.1:{port}");
        if is_bind_reachable(&bind) {
            continue;
        }
        return Ok(bind);
    }

    Err(format!(
        "No free local engine ports in {LOCAL_PORT_START}–{LOCAL_PORT_END} — remove a local workshop or stop another engine."
    ))
}

pub fn validate_engine_data_dir(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Engine data folder is required".to_string());
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err("Engine data folder must be an absolute path".to_string());
    }
    Ok(path)
}

pub fn backfill_local_workshop_fields(registry: &mut WorkshopRegistry) {
    for workshop in &mut registry.workshops {
        if workshop.kind != "local" {
            continue;
        }
        if workshop.bind.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_none() {
            workshop.bind = Some(DEFAULT_LOCAL_BIND.to_string());
        }
        if workshop.id == PERSONAL_WORKSHOP_ID {
            workshop.data_dir = None;
        }
        workshop.url = url_from_bind(&resolve_workshop_bind(workshop));
    }
}

fn write_daemon_pid(workshop_id: &str, pid: u32) -> Result<(), String> {
    let path = daemon_pid_path(workshop_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, pid.to_string()).map_err(|err| err.to_string())
}

fn clear_daemon_pid(workshop_id: &str) {
    let _ = fs::remove_file(daemon_pid_path(workshop_id));
}

fn read_daemon_pid(workshop_id: &str) -> Option<u32> {
    let paths = if workshop_id == PERSONAL_WORKSHOP_ID {
        vec![daemon_pid_path(workshop_id), legacy_daemon_pid_path()]
    } else {
        vec![daemon_pid_path(workshop_id)]
    };
    for path in paths {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                return Some(pid);
            }
        }
    }
    None
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

pub fn stop_local_engine(workshop_id: &str) {
    stop_local_brain(workshop_id);
    if let Some(pid) = read_daemon_pid(workshop_id) {
        if is_process_alive(pid) {
            #[cfg(unix)]
            {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
    }
    clear_daemon_pid(workshop_id);
    if workshop_id == PERSONAL_WORKSHOP_ID {
        let _ = fs::remove_file(legacy_daemon_pid_path());
    }
}

pub fn spawn_local_engine(
    workshop_id: &str,
    bind: &str,
    data_dir: &Path,
    private_brain: bool,
) -> Result<(u32, PathBuf), String> {
    fs::create_dir_all(data_dir).map_err(|err| err.to_string())?;

    let daemon = resolve_daemon_binary()?;
    let log_path = if workshop_id == PERSONAL_WORKSHOP_ID && !engine_runtime_dir(workshop_id).exists()
    {
        legacy_daemon_log_path()
    } else {
        daemon_log_path(workshop_id)
    };
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|err| err.to_string())?;

    let summary = load_tui_defaults_summary();
    let mut command = Command::new(&daemon.program);
    command.args(&daemon.pre_args);
    command.arg("--backend").arg(resolve_backend());
    command.arg("--bind").arg(bind);
    if let Some(provider) = summary.provider.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        command.arg("--provider").arg(provider);
    }
    if let Some(model) = summary.model.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        command.arg("--model").arg(model);
    }
    if private_brain {
        command.env(
            "MEDOUSA_LOCAL_ENGINE_BIND",
            DEFAULT_LOCAL_BRAIN_BIND,
        );
    }
    command.env(
        "MEDOUSA_DATA_DIR",
        data_dir.to_string_lossy().to_string(),
    );
    apply_daemon_messaging_env(&mut command);
    apply_daemon_apns_env(&mut command);
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));
    detach_new_session(&mut command);

    let child = command.spawn().map_err(|err| {
        format!(
            "Failed to spawn medousa_daemon ({}): {err}",
            daemon.program
        )
    })?;
    let pid = child.id();
    write_daemon_pid(workshop_id, pid)?;
    Ok((pid, log_path))
}

fn write_local_brain_pid(workshop_id: &str, pid: u32) -> Result<(), String> {
    let path = local_brain_pid_path(workshop_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, pid.to_string()).map_err(|err| err.to_string())
}

fn clear_local_brain_pid(workshop_id: &str) {
    let _ = fs::remove_file(local_brain_pid_path(workshop_id));
}

fn read_local_brain_pid(workshop_id: &str) -> Option<u32> {
    fs::read_to_string(local_brain_pid_path(workshop_id))
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
}

pub fn stop_local_brain(workshop_id: &str) {
    if let Some(pid) = read_local_brain_pid(workshop_id) {
        if is_process_alive(pid) {
            #[cfg(unix)]
            {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
    }
    clear_local_brain_pid(workshop_id);
}

pub fn spawn_local_brain(
    workshop_id: &str,
    data_dir: &Path,
    model_id: Option<&str>,
) -> Result<(u32, PathBuf), String> {
    if is_bind_reachable(DEFAULT_LOCAL_BRAIN_BIND) {
        return Ok((
            read_local_brain_pid(workshop_id).unwrap_or(0),
            local_brain_log_path(workshop_id),
        ));
    }

    let local = resolve_local_binary()?;
    let log_path = local_brain_log_path(workshop_id);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|err| err.to_string())?;

    let mut command = Command::new(&local.program);
    command.args(&local.pre_args);
    command
        .arg("--bind")
        .arg(DEFAULT_LOCAL_BRAIN_BIND)
        .env("MEDOUSA_DATA_DIR", data_dir.to_string_lossy().to_string());
    match model_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(model_id) => {
            command.arg("--model-id").arg(model_id);
        }
        None => {
            command.arg("--load-recommended");
        }
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));
    detach_new_session(&mut command);

    let child = command.spawn().map_err(|err| {
        format!(
            "Failed to spawn medousa_local ({}): {err}",
            local.program
        )
    })?;
    let pid = child.id();
    write_local_brain_pid(workshop_id, pid)?;
    Ok((pid, log_path))
}

async fn local_brain_http_ready() -> bool {
    is_bind_reachable(DEFAULT_LOCAL_BRAIN_BIND)
}

pub async fn ensure_local_brain(
    workshop_id: &str,
    data_dir: &Path,
    model_id: Option<&str>,
) -> Result<bool, String> {
    if !local_brain_installed() {
        return Ok(false);
    }
    if local_brain_http_ready().await {
        return Ok(true);
    }
    spawn_local_brain(workshop_id, data_dir, model_id)?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(600) {
        if local_brain_http_ready().await {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
        "Offline brain did not become ready — check {}",
        local_brain_log_path(workshop_id).display()
    ))
}

async fn daemon_http_healthy(base_url: &str) -> bool {
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    client
        .get(url)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub async fn wait_engine_healthy(
    base_url: &str,
    timeout_seconds: u64,
    poll_ms: u64,
) -> Result<(bool, u32), String> {
    let timeout = Duration::from_secs(timeout_seconds.max(1));
    let poll = Duration::from_millis(poll_ms.clamp(250, 10_000));
    let started = Instant::now();
    let mut attempts = 0u32;

    while started.elapsed() < timeout {
        attempts += 1;
        if daemon_http_healthy(base_url).await {
            return Ok((true, attempts));
        }
        tokio::time::sleep(poll).await;
    }
    Ok((false, attempts))
}

pub async fn ensure_local_engine(
    workshop: &WorkshopServer,
    private_brain: bool,
) -> Result<LocalEngineEnsureResult, String> {
    if workshop.kind != "local" {
        return Ok(LocalEngineEnsureResult {
            ok: true,
            already_running: true,
            started: false,
            pid: None,
            url: resolve_workshop_url(workshop),
            bind: resolve_workshop_bind(workshop),
            data_dir: resolve_workshop_data_dir(workshop).display().to_string(),
            log_path: String::new(),
            message: "Remote workshop — no local engine spawn".to_string(),
        });
    }

    let bind = resolve_workshop_bind(workshop);
    let url = url_from_bind(&bind);
    let data_dir = resolve_workshop_data_dir(workshop);
    let log_path = daemon_log_path(&workshop.id);

    if daemon_http_healthy(&url).await {
        if private_brain && local_brain_installed() {
            let _ = ensure_local_brain(&workshop.id, &data_dir, None).await;
        }
        let message = format!("Engine already running at {bind}");
        return Ok(LocalEngineEnsureResult {
            ok: true,
            already_running: true,
            started: false,
            pid: read_daemon_pid(&workshop.id),
            url,
            bind,
            data_dir: data_dir.display().to_string(),
            log_path: log_path.display().to_string(),
            message,
        });
    }

    if is_bind_reachable(&bind) {
        let (ok, _) = wait_engine_healthy(&url, 45, 500).await?;
        if ok {
            let message = format!("Engine is ready at {bind}");
            return Ok(LocalEngineEnsureResult {
                ok: true,
                already_running: true,
                started: false,
                pid: read_daemon_pid(&workshop.id),
                url,
                bind,
                data_dir: data_dir.display().to_string(),
                log_path: log_path.display().to_string(),
                message,
            });
        }
        stop_local_engine(&workshop.id);
        tokio::time::sleep(Duration::from_millis(750)).await;
    }

    let (pid, log_path) = spawn_local_engine(&workshop.id, &bind, &data_dir, private_brain)?;
    let (ok, _) = wait_engine_healthy(&url, 45, 500).await?;
    if ok && private_brain && local_brain_installed() {
        let _ = ensure_local_brain(&workshop.id, &data_dir, None).await;
    }
    Ok(LocalEngineEnsureResult {
        ok,
        already_running: false,
        started: true,
        pid: Some(pid),
        url,
        bind,
        data_dir: data_dir.display().to_string(),
        log_path: log_path.display().to_string(),
        message: if ok {
            format!("Started local engine (pid {pid})")
        } else {
            format!(
                "Engine did not become ready — check {}",
                log_path.display()
            )
        },
    })
}

pub async fn ensure_active_local_engine(
    registry: &WorkshopRegistry,
    private_brain: bool,
) -> Result<LocalEngineEnsureResult, String> {
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == registry.active_workshop_id)
        .ok_or_else(|| "Active workshop not found".to_string())?;
    ensure_local_engine(workshop, private_brain).await
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDiagnosis {
    pub issue: String,
    pub title: String,
    pub message: String,
    pub log_path: Option<String>,
    pub lock_path: Option<String>,
    pub bind: Option<String>,
    pub can_clear_lock: bool,
    pub can_restart: bool,
}

fn surreal_kv_lock_path(backend: &str, data_dir: &Path) -> Option<PathBuf> {
    let trimmed = backend.trim();
    let kv_root = if trimmed.eq_ignore_ascii_case("surreal-kv") {
        Some(data_dir.join("runtime.surrealkv"))
    } else if let Some(rest) = trimmed.strip_prefix("surreal-kv:") {
        let path = rest.trim();
        if path.is_empty() {
            Some(data_dir.join("runtime.surrealkv"))
        } else {
            Some(PathBuf::from(path))
        }
    } else {
        None
    }?;
    Some(kv_root.join("LOCK"))
}

pub async fn diagnose_local_engine(workshop: &WorkshopServer) -> EngineDiagnosis {
    let bind = resolve_workshop_bind(workshop);
    let url = url_from_bind(&bind);
    let log_path = daemon_log_path(&workshop.id);
    let log_path_display = log_path.display().to_string();
    let bind_display = bind.clone();

    if workshop.kind != "local" {
        return EngineDiagnosis {
            issue: "remote".to_string(),
            title: "Remote workshop".to_string(),
            message: "This workshop runs on another device. Open Connection settings and check the address.".to_string(),
            log_path: None,
            lock_path: None,
            bind: Some(bind_display),
            can_clear_lock: false,
            can_restart: false,
        };
    }

    if resolve_daemon_binary().is_err() {
        return EngineDiagnosis {
            issue: "binary_missing".to_string(),
            title: "Medousa engine missing".to_string(),
            message: "The app couldn't find its engine files. Try reinstalling Medousa.".to_string(),
            log_path: Some(log_path_display),
            lock_path: None,
            bind: Some(bind_display),
            can_clear_lock: false,
            can_restart: false,
        };
    }

    let healthy = daemon_http_healthy(&url).await;
    if healthy {
        return EngineDiagnosis {
            issue: "ok".to_string(),
            title: "Medousa is running".to_string(),
            message: format!("Engine is ready at {bind}"),
            log_path: Some(log_path_display),
            lock_path: None,
            bind: Some(bind_display),
            can_clear_lock: false,
            can_restart: false,
        };
    }

    let data_dir = resolve_workshop_data_dir(workshop);
    let backend = resolve_backend();
    let lock_path = surreal_kv_lock_path(&backend, &data_dir);
    let lock_exists = lock_path
        .as_ref()
        .is_some_and(|path| path.exists());
    let lock_display = lock_path.as_ref().map(|path| path.display().to_string());

    let pid = read_daemon_pid(&workshop.id);
    let pid_alive = pid.is_some_and(is_process_alive);
    let bind_up = is_bind_reachable(&bind);

    if lock_exists && !pid_alive {
        return EngineDiagnosis {
            issue: "stale_lock".to_string(),
            title: "Medousa didn't shut down cleanly".to_string(),
            message: "A leftover lock file is blocking startup. Tap Fix and we'll clear it, then start Medousa again.".to_string(),
            log_path: Some(log_path_display),
            lock_path: lock_display,
            bind: Some(bind_display),
            can_clear_lock: true,
            can_restart: true,
        };
    }

    if bind_up && !healthy {
        return EngineDiagnosis {
            issue: "port_blocked".to_string(),
            title: "Something else is using the engine port".to_string(),
            message: format!(
                "Port {} is busy but Medousa isn't responding. Restarting usually fixes this — or another app may be using that port.",
                parse_bind_port(&bind).unwrap_or(7419)
            ),
            log_path: Some(log_path_display),
            lock_path: lock_display,
            bind: Some(bind_display),
            can_clear_lock: false,
            can_restart: true,
        };
    }

    if pid_alive && !healthy {
        return EngineDiagnosis {
            issue: "wedged".to_string(),
            title: "Medousa is stuck starting up".to_string(),
            message: "The engine process is running but not responding. Try restarting Medousa.".to_string(),
            log_path: Some(log_path_display),
            lock_path: lock_display,
            bind: Some(bind_display),
            can_clear_lock: false,
            can_restart: true,
        };
    }

    EngineDiagnosis {
        issue: "not_running".to_string(),
        title: "Medousa isn't running".to_string(),
        message: "Start Medousa on this computer to chat. Your notes and settings stay on this machine.".to_string(),
        log_path: Some(log_path_display),
        lock_path: lock_display,
        bind: Some(bind_display),
        can_clear_lock: false,
        can_restart: true,
    }
}

pub fn clear_stale_engine_lock(workshop: &WorkshopServer) -> Result<(), String> {
    if workshop.kind != "local" {
        return Err("Only local workshops have engine locks".to_string());
    }
    let data_dir = resolve_workshop_data_dir(workshop);
    let backend = resolve_backend();
    let Some(lock_path) = surreal_kv_lock_path(&backend, &data_dir) else {
        return Err("This workshop doesn't use on-disk storage locks".to_string());
    };
    if !lock_path.exists() {
        return Ok(());
    }
    let pid = read_daemon_pid(&workshop.id);
    if pid.is_some_and(is_process_alive) {
        return Err("Medousa is still running — stop it before clearing the lock".to_string());
    }
    fs::remove_file(&lock_path).map_err(|err| {
        format!(
            "Couldn't remove the lock file ({}): {err}",
            lock_path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_from_bind_formats_http() {
        assert_eq!(url_from_bind("127.0.0.1:7420"), "http://127.0.0.1:7420");
    }

    #[test]
    fn parse_bind_port_reads_trailing_port() {
        assert_eq!(parse_bind_port("127.0.0.1:7419"), Some(7419));
    }
}
