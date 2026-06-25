//! Spawn `medousa_local` as a detached sidecar process (CLI / desktop app).

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use medousa_types::local::{LocalEngineStatus, DEFAULT_LOCAL_ENGINE_BIND};

use crate::{
    detach_new_session, is_bind_reachable, resolve_medousa_local_binary,
};

pub fn medousa_local_binary_available() -> bool {
    resolve_medousa_local_binary().is_ok()
}

pub async fn spawn_and_wait_recommended(
    bind: Option<String>,
) -> Result<LocalEngineStatus, String> {
    spawn_and_wait(bind, None).await
}

pub async fn spawn_and_wait(
    bind: Option<String>,
    model_id: Option<String>,
) -> Result<LocalEngineStatus, String> {
    if !medousa_local_binary_available() {
        return Err("Offline brain package is not installed (medousa_local missing)".to_string());
    }
    let bind = bind.unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());
    if is_bind_reachable(&bind) {
        return Ok(LocalEngineStatus {
            feature_enabled: true,
            loaded: true,
            base_url: format!("http://{bind}/v1"),
            bind: Some(bind),
            model_repo: None,
            model_alias: None,
            inference_backend: None,
            message: "Local engine already running (medousa_local)".to_string(),
        });
    }
    spawn_medousa_local(bind.clone(), model_id)?;
    let ready = wait_local_engine_ready(&bind, Duration::from_secs(600)).await;
    if !ready {
        return Err(format!(
            "medousa_local did not become ready on {bind} — check {}",
            local_engine_log_path().display()
        ));
    }
    Ok(LocalEngineStatus {
        feature_enabled: true,
        loaded: true,
        base_url: format!("http://{bind}/v1"),
        bind: Some(bind),
        model_repo: None,
        model_alias: None,
        inference_backend: None,
        message: "Local Gemma engine ready via medousa_local".to_string(),
    })
}

pub fn spawn_medousa_local_recommended(bind: Option<String>) -> Result<std::process::Child, String> {
    let bind = bind.unwrap_or_else(|| DEFAULT_LOCAL_ENGINE_BIND.to_string());
    spawn_medousa_local(bind, None)
}

pub fn spawn_medousa_local(
    bind: String,
    model_id: Option<String>,
) -> Result<std::process::Child, String> {
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
    command.arg("--bind").arg(&bind);
    if let Some(model_id) = model_id {
        command.arg("--model-id").arg(model_id);
    } else {
        command.arg("--load-recommended");
    }
    if let Ok(data_dir) = std::env::var("MEDOUSA_DATA_DIR") {
        command.env("MEDOUSA_DATA_DIR", data_dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));
    detach_new_session(&mut command);
    command
        .spawn()
        .map_err(|err| format!("failed to spawn medousa_local ({}): {err}", binary.display()))
}

pub async fn wait_local_engine_ready(bind: &str, timeout: Duration) -> bool {
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
    std::env::var("MEDOUSA_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("medousa")
        })
        .join("logs")
        .join("medousa_local.log")
}
