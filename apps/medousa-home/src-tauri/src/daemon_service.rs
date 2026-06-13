use crate::daemon::types::DEFAULT_DAEMON_URL;
use crate::medousa_paths::{load_tui_defaults_summary, tui_defaults_path};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_BIND: &str = "127.0.0.1:7419";
const PUBLIC_BIND: &str = "0.0.0.0:7419";
const DEFAULT_BACKEND: &str = "surreal-mem";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStartResult {
    pub started: bool,
    pub already_running: bool,
    pub pid: Option<u32>,
    pub log_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWaitHealthResult {
    pub ok: bool,
    pub message: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWaitHealthRequest {
    #[serde(default = "default_wait_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}

fn default_wait_seconds() -> u64 {
    30
}

fn default_poll_ms() -> u64 {
    2000
}

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn daemon_log_path() -> PathBuf {
    medousa_data_dir().join("logs").join("daemon.log")
}

fn daemon_pid_path() -> PathBuf {
    medousa_data_dir().join("daemon.pid")
}

fn write_daemon_pid(pid: u32) -> Result<(), String> {
    if let Some(parent) = daemon_pid_path().parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(daemon_pid_path(), pid.to_string()).map_err(|err| err.to_string())
}

fn clear_daemon_pid() {
    let _ = fs::remove_file(daemon_pid_path());
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
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

pub fn stop_daemon_process() {
    if let Ok(raw) = fs::read_to_string(daemon_pid_path()) {
        if let Ok(pid) = raw.trim().parse::<u32>() {
            if is_process_alive(pid) {
                #[cfg(unix)]
                {
                    use std::process::Command;
                    let _ = Command::new("kill").arg(pid.to_string()).status();
                }
            }
        }
    }
    clear_daemon_pid();
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

pub(crate) struct ComponentCommand {
    pub program: String,
    pub pre_args: Vec<String>,
}

pub(crate) fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|path| path.join(command))
        .find(|candidate| candidate.exists())
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
        let sibling = current_exe.with_file_name("medousa_daemon");
        if sibling.exists() {
            return Ok(ComponentCommand {
                program: sibling.to_string_lossy().to_string(),
                pre_args: Vec::new(),
            });
        }
    }

    if find_command_in_path("medousa_daemon").is_some() {
        return Ok(ComponentCommand {
            program: "medousa_daemon".to_string(),
            pre_args: Vec::new(),
        });
    }

    Err(
        "Medousa could not start — the app bundle may be incomplete. Reinstall Medousa, or set MEDOUSA_MEDOUSA_DAEMON_BIN for development.".to_string(),
    )
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

#[cfg(not(unix))]
fn detach_new_session(_command: &mut Command) {}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStartRequest {
    /// Load the private Gemma brain when the engine starts (`medousa_daemon --local-engine`).
    #[serde(default)]
    pub private_brain: bool,
    /// Bind on all interfaces for phone pairing (`0.0.0.0:7419`). Falls back to saved prefs.
    #[serde(default)]
    pub public_bind: Option<bool>,
}

fn resolve_bind_address(public_bind: bool) -> &'static str {
    if public_bind {
        PUBLIC_BIND
    } else {
        DEFAULT_BIND
    }
}

fn resolve_public_bind(request: Option<&DaemonStartRequest>) -> bool {
    if let Some(explicit) = request.and_then(|value| value.public_bind) {
        return explicit;
    }
    crate::connection_prefs::load_connection_prefs().public_bind
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

fn spawn_daemon_background(backend: &str, bind: &str, private_brain: bool) -> Result<(u32, PathBuf), String> {
    let daemon = resolve_daemon_binary()?;
    let log_path = daemon_log_path();
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
    command.arg("--backend").arg(backend);
    command.arg("--bind").arg(bind);
    if let Some(provider) = summary.provider.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        command.arg("--provider").arg(provider);
    }
    if let Some(model) = summary.model.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        command.arg("--model").arg(model);
    }
    if private_brain {
        command.arg("--local-engine");
    }
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
    let _ = write_daemon_pid(pid);
    Ok((pid, log_path))
}

#[tauri::command]
pub async fn daemon_start(request: Option<DaemonStartRequest>) -> Result<DaemonStartResult, String> {
    let request = request.unwrap_or(DaemonStartRequest {
        private_brain: false,
        public_bind: None,
    });
    let private_brain = should_load_private_brain(request.private_brain);
    let public_bind = resolve_public_bind(Some(&request));
    let bind = resolve_bind_address(public_bind);
    let base_url = DEFAULT_DAEMON_URL;
    if daemon_http_healthy(base_url).await {
        return Ok(DaemonStartResult {
            started: false,
            already_running: true,
            pid: None,
            log_path: daemon_log_path().to_string_lossy().to_string(),
            message: format!("Medousa Engine already running at {base_url}"),
        });
    }

    if is_bind_reachable(bind) {
        return Err(format!(
            "Port {bind} is open but the engine is not responding. Try restarting from Settings → Connection."
        ));
    }

    let backend = resolve_backend();
    let (pid, log_path) = spawn_daemon_background(&backend, bind, private_brain)?;

    Ok(DaemonStartResult {
        started: true,
        already_running: false,
        pid: Some(pid),
        log_path: log_path.to_string_lossy().to_string(),
        message: if private_brain {
            format!("Starting Medousa Engine with your private brain (pid {pid})")
        } else if public_bind {
            format!("Starting Medousa Engine on your network (pid {pid})")
        } else {
            format!("Starting Medousa Engine (pid {pid})")
        },
    })
}

#[tauri::command]
pub async fn daemon_restart(request: Option<DaemonStartRequest>) -> Result<DaemonStartResult, String> {
    stop_daemon_process();
    tokio::time::sleep(Duration::from_millis(750)).await;
    daemon_start(request).await
}

#[tauri::command]
pub async fn daemon_wait_healthy(
    request: Option<DaemonWaitHealthRequest>,
) -> Result<DaemonWaitHealthResult, String> {
    let request = request.unwrap_or(DaemonWaitHealthRequest {
        timeout_seconds: default_wait_seconds(),
        poll_ms: default_poll_ms(),
    });
    let base_url = DEFAULT_DAEMON_URL;
    let timeout = Duration::from_secs(request.timeout_seconds.max(1));
    let poll = Duration::from_millis(request.poll_ms.clamp(250, 10_000));
    let started = Instant::now();
    let mut attempts = 0u32;

    while started.elapsed() < timeout {
        attempts += 1;
        if daemon_http_healthy(base_url).await {
            return Ok(DaemonWaitHealthResult {
                ok: true,
                message: format!("Medousa Engine is ready at {base_url}"),
                attempts,
            });
        }
        tokio::time::sleep(poll).await;
    }

    Ok(DaemonWaitHealthResult {
        ok: false,
        message: format!(
            "Medousa Engine did not become ready within {}s — check {}",
            request.timeout_seconds,
            daemon_log_path().display()
        ),
        attempts,
    })
}
