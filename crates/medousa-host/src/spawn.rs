//! Spawn `medousa_local` as a detached sidecar process (CLI / desktop app).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use medousa_types::local::{
    DEFAULT_LOCAL_ENGINE_BIND, LOCAL_WORKER_STATUS_PATH, LocalEngineStatus, LocalWorkerStatus,
};

use crate::{detach_new_session, is_bind_reachable, resolve_medousa_local_binary};

pub fn medousa_local_binary_available() -> bool {
    resolve_medousa_local_binary().is_ok()
}

pub async fn spawn_and_wait_recommended(bind: Option<String>) -> Result<LocalEngineStatus, String> {
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
    if let Ok(worker) = probe_local_worker(&bind) {
        return validate_worker_model(engine_status(&bind, worker, true), model_id.as_deref());
    }
    if is_bind_reachable(&bind) {
        return Err(format!(
            "{bind} is occupied, but it is not a compatible Medousa local worker"
        ));
    }
    let child = spawn_medousa_local(bind.clone(), model_id.clone())?;
    let pid = child.id();
    let Some(worker) = wait_local_worker_ready(&bind, Duration::from_secs(600)).await else {
        let _ = crate::request_process_stop_by_pid(pid);
        return Err(format!(
            "medousa_local did not become ready on {bind} — check {}",
            local_engine_log_path().display()
        ));
    };
    if worker.pid != pid {
        let _ = crate::request_process_stop_by_pid(pid);
        return Err(format!(
            "worker generation mismatch on {bind}: spawned pid {pid}, handshake reported pid {}",
            worker.pid
        ));
    }
    validate_worker_model(engine_status(&bind, worker, false), model_id.as_deref())
}

fn engine_status(
    bind: &str,
    worker: LocalWorkerStatus,
    already_running: bool,
) -> LocalEngineStatus {
    LocalEngineStatus {
        feature_enabled: true,
        loaded: true,
        phase: worker.phase.clone(),
        base_url: format!("http://{bind}/v1"),
        bind: Some(bind.to_string()),
        model_repo: Some(worker.model_repo.clone()),
        model_alias: Some(worker.model_alias.clone()),
        inference_backend: None,
        worker: Some(worker),
        message: if already_running {
            "Compatible local worker already running".to_string()
        } else {
            "Local worker ready".to_string()
        },
    }
}

fn validate_worker_model(
    status: LocalEngineStatus,
    requested_model: Option<&str>,
) -> Result<LocalEngineStatus, String> {
    if let Some(requested) = requested_model
        && status.model_alias.as_deref() != Some(requested)
    {
        return Err(format!(
            "local worker has model {}, but {requested} was requested",
            status.model_alias.as_deref().unwrap_or("unknown")
        ));
    }
    Ok(status)
}

pub fn spawn_medousa_local_recommended(
    bind: Option<String>,
) -> Result<std::process::Child, String> {
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
    command.spawn().map_err(|err| {
        format!(
            "failed to spawn medousa_local ({}): {err}",
            binary.display()
        )
    })
}

pub async fn wait_local_engine_ready(bind: &str, timeout: Duration) -> bool {
    wait_local_worker_ready(bind, timeout).await.is_some()
}

pub async fn wait_local_worker_ready(bind: &str, timeout: Duration) -> Option<LocalWorkerStatus> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Ok(status) = probe_local_worker(bind) {
            return Some(status);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    None
}

pub async fn stop_local_worker(bind: &str, timeout: Duration) -> Result<bool, String> {
    let worker = match probe_local_worker(bind) {
        Ok(worker) => worker,
        Err(_) if !is_bind_reachable(bind) => return Ok(false),
        Err(error) => return Err(error),
    };
    let confirmed = probe_local_worker(bind)?;
    if confirmed.generation_id != worker.generation_id || confirmed.pid != worker.pid {
        return Err("local worker generation changed before shutdown".to_string());
    }
    if !crate::request_process_stop_by_pid(worker.pid) && crate::is_process_alive(worker.pid) {
        return Err(format!("failed to stop local worker pid {}", worker.pid));
    }
    let started = Instant::now();
    while started.elapsed() < timeout {
        if !crate::is_process_alive(worker.pid) {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !crate::force_process_stop_by_pid(worker.pid) && crate::is_process_alive(worker.pid) {
        return Err(format!(
            "local worker pid {} did not stop within {} seconds",
            worker.pid,
            timeout.as_secs()
        ));
    }
    let forced_started = Instant::now();
    while forced_started.elapsed() < Duration::from_secs(2) {
        if !crate::is_process_alive(worker.pid) {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "local worker pid {} remained alive after forced termination",
        worker.pid
    ))
}

pub fn probe_local_worker(bind: &str) -> Result<LocalWorkerStatus, String> {
    use std::net::ToSocketAddrs;

    let address = bind
        .to_socket_addrs()
        .map_err(|error| format!("invalid local worker bind {bind}: {error}"))?
        .next()
        .ok_or_else(|| format!("local worker bind {bind} resolved no addresses"))?;
    let timeout = Duration::from_millis(500);
    let mut stream = std::net::TcpStream::connect_timeout(&address, timeout)
        .map_err(|error| format!("local worker connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    write!(
        stream,
        "GET {LOCAL_WORKER_STATUS_PATH} HTTP/1.1\r\nHost: {bind}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| format!("local worker status write failed: {error}"))?;
    let mut response = Vec::new();
    stream
        .take(64 * 1024)
        .read_to_end(&mut response)
        .map_err(|error| format!("local worker status read failed: {error}"))?;
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "local worker returned an invalid HTTP response".to_string())?;
    let headers = std::str::from_utf8(&response[..header_end])
        .map_err(|_| "local worker returned non-UTF-8 HTTP headers".to_string())?;
    if !headers
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("HTTP/1.1 200 ") || line.starts_with("HTTP/1.0 200 "))
    {
        return Err("local worker status endpoint did not return HTTP 200".to_string());
    }
    let status: LocalWorkerStatus = serde_json::from_slice(&response[header_end + 4..])
        .map_err(|error| format!("invalid local worker handshake: {error}"))?;
    if !status.is_compatible_ready() {
        return Err("local worker handshake is incompatible or not ready".to_string());
    }
    Ok(status)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn serve_status(body: String) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let bind = listener.local_addr().unwrap().to_string();
        let task = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..read])
                    .starts_with("GET /_medousa/status HTTP/1.1")
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        (bind, task)
    }

    fn status_json(protocol_version: u32, phase: &str) -> String {
        format!(
            r#"{{"protocolVersion":{protocol_version},"generationId":"generation-1","pid":42,"startedAt":"2026-08-02T00:00:00Z","phase":"{phase}","modelRepo":"google/model","modelAlias":"model","artifactDigest":"sha256:artifact","recipeRevision":"mir-recipe-v1:recipe","binaryDigest":"sha256:binary","runtimeName":"mistral.rs","runtimeVersion":"0.8.1","compiledBackends":["cpu"]}}"#
        )
    }

    #[test]
    fn probe_requires_a_versioned_ready_worker_handshake() {
        let (bind, task) = serve_status(status_json(1, "ready"));
        let status = probe_local_worker(&bind).unwrap();
        task.join().unwrap();
        assert_eq!(status.generation_id, "generation-1");
        assert_eq!(status.model_alias, "model");
        assert!(status.is_compatible_ready());
    }

    #[test]
    fn probe_rejects_incompatible_protocols() {
        let (bind, task) = serve_status(status_json(999, "ready"));
        let error = probe_local_worker(&bind).unwrap_err();
        task.join().unwrap();
        assert!(error.contains("incompatible"));
    }

    #[test]
    fn probe_rejects_loading_as_ready() {
        let (bind, task) = serve_status(status_json(1, "loading"));
        let error = probe_local_worker(&bind).unwrap_err();
        task.join().unwrap();
        assert!(error.contains("not ready"));
    }
}
