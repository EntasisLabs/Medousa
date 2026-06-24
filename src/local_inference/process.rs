use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use tokio::sync::Mutex;

use super::engine::{LocalEngineConfig, LocalEngineStatus, DEFAULT_LOCAL_ENGINE_BASE_URL, DEFAULT_LOCAL_ENGINE_BIND};
use super::hardware::{build_hardware_profile, probe_hardware, read_hardware_profile};
use super::{builtin_catalog, config_from_catalog_entry, load_recommended_engine, MODEL_STORE};

static EXTERNAL_PID: Lazy<Mutex<Option<u32>>> = Lazy::new(|| Mutex::new(None));

pub fn medousa_local_binary_available() -> bool {
    resolve_medousa_local_binary().is_ok()
}

pub fn resolve_medousa_local_binary() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("MEDOUSA_MEDOUSA_LOCAL_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "MEDOUSA_MEDOUSA_LOCAL_BIN points to missing file: {}",
            path.display()
        ));
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let sibling = parent.join(binary_name());
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }

    if let Some(path) = find_command_in_path(binary_name()) {
        return Ok(path);
    }

    Err(
        "medousa_local binary not found — install the Offline brain package or set MEDOUSA_MEDOUSA_LOCAL_BIN"
            .to_string(),
    )
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "medousa_local.exe"
    } else {
        "medousa_local"
    }
}

fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

pub fn is_bind_reachable(bind: &str) -> bool {
    if let Ok(mut addrs) = bind.to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
        }
    }
    false
}

async fn wait_bind_reachable(bind: &str, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if is_bind_reachable(bind) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

fn local_engine_log_path() -> PathBuf {
    crate::paths::medousa_data_dir()
        .join("logs")
        .join("medousa_local.log")
}

#[cfg(unix)]
fn detach_new_session(command: &mut Command) {
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

#[cfg(not(unix))]
fn detach_new_session(_command: &mut Command) {}

fn spawn_medousa_local_process(config: &LocalEngineConfig) -> Result<u32, String> {
    let binary = resolve_medousa_local_binary()?;
    let log_path = local_engine_log_path();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let log_file_err = log_file.try_clone().map_err(|err| err.to_string())?;

    let mut command = Command::new(&binary);
    command
        .arg("--bind")
        .arg(&config.bind)
        .arg("--model-repo")
        .arg(&config.model_repo)
        .arg("--model-alias")
        .arg(&config.model_alias);
    if let Some(from_uqff) = &config.from_uqff {
        command.arg("--from-uqff").arg(from_uqff);
    }
    if let Some(in_situ_quant) = &config.in_situ_quant {
        command.arg("--in-situ-quant").arg(in_situ_quant);
    }
    if config.cpu_only {
        command.env("MEDOUSA_LOCAL_ENGINE_CPU", "1");
    }
    if let Ok(data_dir) = std::env::var("MEDOUSA_DATA_DIR") {
        command.env("MEDOUSA_DATA_DIR", data_dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));
    detach_new_session(&mut command);

    let child = command
        .spawn()
        .map_err(|err| format!("failed to spawn medousa_local ({}): {err}", binary.display()))?;
    Ok(child.id())
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

pub async fn stop_external_local_engine() {
    if let Some(pid) = EXTERNAL_PID.lock().await.take() {
        if is_process_alive(pid) {
            #[cfg(unix)]
            {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
            #[cfg(windows)]
            {
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .status();
            }
        }
    }
}

pub async fn spawn_external_local_engine(config: LocalEngineConfig) -> Result<LocalEngineStatus, String> {
    stop_external_local_engine().await;

    if !medousa_local_binary_available() {
        return Err(
            "Offline brain package is not installed (medousa_local missing)".to_string(),
        );
    }

    let bind = config.bind.trim().to_string();
    let pid = spawn_medousa_local_process(&config)?;
    *EXTERNAL_PID.lock().await = Some(pid);

    let ready = wait_bind_reachable(&bind, Duration::from_secs(600)).await;
    if !ready {
        stop_external_local_engine().await;
        return Err(format!(
            "medousa_local did not become ready on {bind} — check {}",
            local_engine_log_path().display()
        ));
    }

    let profile = read_hardware_profile()
        .unwrap_or_else(|| build_hardware_profile(probe_hardware()));
    let device = super::backends::resolve_inference_device(&profile.probe);

    Ok(LocalEngineStatus {
        feature_enabled: true,
        loaded: true,
        base_url: format!("http://{bind}/v1"),
        bind: Some(bind),
        model_repo: Some(config.model_repo),
        model_alias: Some(config.model_alias),
        inference_backend: Some(device.as_str().to_string()),
        message: format!("Local Gemma engine ready via medousa_local ({})", device.label()),
    })
}

pub async fn spawn_external_recommended(bind: Option<String>) -> Result<LocalEngineStatus, String> {
    let profile = read_hardware_profile()
        .unwrap_or_else(|| build_hardware_profile(probe_hardware()));
    let entry = super::catalog::recommended_model_for_tier(profile.tier)
        .ok_or_else(|| "no recommended model for hardware tier".to_string())?;
    let config = config_from_catalog_entry(&entry, bind);
    spawn_external_local_engine(config).await
}

pub async fn load_external_engine(
    model_id: Option<&str>,
    bind: Option<String>,
) -> Result<LocalEngineStatus, String> {
    if let Some(model_id) = model_id.map(str::trim).filter(|value| !value.is_empty()) {
        let catalog = builtin_catalog();
        let entry = catalog
            .models
            .iter()
            .find(|entry| entry.id == model_id)
            .cloned()
            .ok_or_else(|| format!("unknown catalog model id: {model_id}"))?;
        if !MODEL_STORE.is_installed(model_id).await {
            return Err(format!(
                "model {model_id} is not installed — download it first"
            ));
        }
        let config = config_from_catalog_entry(&entry, bind);
        spawn_external_local_engine(config).await
    } else {
        spawn_external_recommended(bind).await
    }
}

pub async fn external_engine_status() -> LocalEngineStatus {
    let bind = std::env::var("MEDOUSA_LOCAL_ENGINE_BIND")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());

    if is_bind_reachable(&bind) {
        return LocalEngineStatus {
            feature_enabled: medousa_local_binary_available(),
            loaded: true,
            base_url: format!("http://{bind}/v1"),
            bind: Some(bind),
            model_repo: None,
            model_alias: None,
            inference_backend: None,
            message: "Local engine running (medousa_local)".to_string(),
        };
    }

    LocalEngineStatus {
        feature_enabled: medousa_local_binary_available(),
        loaded: false,
        base_url: DEFAULT_LOCAL_ENGINE_BASE_URL.to_string(),
        bind: None,
        model_repo: None,
        model_alias: None,
        inference_backend: None,
        message: if medousa_local_binary_available() {
            "Local engine not loaded".to_string()
        } else {
            "Offline brain package not installed".to_string()
        },
    }
}

pub fn local_brain_package_path(install_root: &Path) -> PathBuf {
    install_root.join("packages").join("local-brain")
}
