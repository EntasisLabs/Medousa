//! Lifecycle host for the colocated `medousa-computer` driver sidecar.
//!
//! The child only receives already-admitted, bounded requests over private
//! stdin/stdout. It exposes no listener and cannot mint world authority.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use medousa_computer_bridge::{
    COMPUTER_DRIVER_PROTOCOL_VERSION, ComputerActionReceipt, ComputerActionRequest,
    ComputerDriverPreflight, ComputerDriverRequest, ComputerDriverRequestEnvelope,
    ComputerDriverResponse, ComputerDriverResponseEnvelope, ComputerDriverResponseResult,
    ComputerObservation, ComputerObservationRequest, MAX_COMPUTER_DRIVER_MESSAGE_BYTES,
};
use medousa_world::{WorldDriverId, WorldDriverRegistration};
#[cfg(target_os = "macos")]
use medousa_world::{
    WorldDriverCapability, WorldDriverKind, WorldDriverTransport, WorldOwnership,
    WorldSurfaceKind,
};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::computer_driver::{
    ComputerDriver, ComputerDriverActionError, ComputerDriverBroker,
};
use crate::paths::medousa_data_dir;

#[cfg(target_os = "macos")]
const MACOS_DRIVER_ID: &str = "driver:computer:macos:accessibility";

#[derive(Debug, Clone, Serialize)]
pub struct ComputerDriverDoctorReport {
    pub supported: bool,
    pub installed: bool,
    pub binary: Option<String>,
    pub preflight: Option<ComputerDriverPreflight>,
    pub error: Option<String>,
}

/// Probe the optional native driver without requesting platform permissions.
pub fn collect_computer_driver_doctor_report() -> ComputerDriverDoctorReport {
    let Some(registration) = native_registration() else {
        return ComputerDriverDoctorReport {
            supported: false,
            installed: false,
            binary: None,
            preflight: None,
            error: None,
        };
    };
    let supported = true;
    let Some(binary) = resolve_computer_binary() else {
        return ComputerDriverDoctorReport {
            supported,
            installed: false,
            binary: None,
            preflight: None,
            error: None,
        };
    };
    let binary_label = binary.display().to_string();
    match run_doctor_preflight(&binary) {
        Ok(output) if output.status.success() => {
            match serde_json::from_slice::<ComputerDriverPreflight>(&output.stdout) {
                Ok(preflight)
                    if preflight.protocol_version == COMPUTER_DRIVER_PROTOCOL_VERSION
                        && preflight.driver_id == registration.driver_id =>
                {
                    ComputerDriverDoctorReport {
                        supported,
                        installed: true,
                        binary: Some(binary_label),
                        preflight: Some(preflight),
                        error: None,
                    }
                }
                Ok(_) => ComputerDriverDoctorReport {
                    supported,
                    installed: true,
                    binary: Some(binary_label),
                    preflight: None,
                    error: Some(
                        "native computer preflight reported an incompatible identity".to_string(),
                    ),
                },
                Err(error) => ComputerDriverDoctorReport {
                    supported,
                    installed: true,
                    binary: Some(binary_label),
                    preflight: None,
                    error: Some(format!("decode native computer preflight: {error}")),
                },
            }
        }
        Ok(output) => ComputerDriverDoctorReport {
            supported,
            installed: true,
            binary: Some(binary_label),
            preflight: None,
            error: Some(format!(
                "native computer preflight exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        },
        Err(error) => ComputerDriverDoctorReport {
            supported,
            installed: true,
            binary: Some(binary_label),
            preflight: None,
            error: Some(error),
        },
    }
}

fn run_doctor_preflight(binary: &PathBuf) -> Result<std::process::Output, String> {
    let mut command = std::process::Command::new(binary);
    command
        .arg("preflight")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    medousa_host::hide_subprocess_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("run native computer preflight: {error}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|error| format!("read native computer preflight: {error}"))?;
                if output.stdout.len() > MAX_COMPUTER_DRIVER_MESSAGE_BYTES
                    || output.stderr.len() > MAX_COMPUTER_DRIVER_MESSAGE_BYTES
                {
                    return Err("native computer preflight exceeded its output limit".to_string());
                }
                return Ok(output);
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("native computer preflight timed out".to_string());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("inspect native computer preflight: {error}"));
            }
        }
    }
}

pub async fn register_native_computer_driver(
    broker: &ComputerDriverBroker,
) -> Result<Option<WorldDriverId>, String> {
    let Some(registration) = native_registration() else {
        return Ok(None);
    };
    let Some(binary) = resolve_computer_binary() else {
        return Ok(None);
    };
    let driver_id = registration.driver_id.clone();
    broker
        .register(Arc::new(SidecarComputerDriver::new(binary, registration)))
        .await?;
    Ok(Some(driver_id))
}

fn native_registration() -> Option<WorldDriverRegistration> {
    #[cfg(target_os = "macos")]
    {
        Some(WorldDriverRegistration {
            driver_id: WorldDriverId::new(MACOS_DRIVER_ID),
            kind: WorldDriverKind::NativeDesktop,
            surface: WorldSurfaceKind::Desktop,
            ownership: WorldOwnership::Attached,
            transport: WorldDriverTransport::LocalSidecar,
            capabilities: [
                WorldDriverCapability::SemanticObservation,
                WorldDriverCapability::Interaction,
            ]
                .into_iter()
                .collect(),
            display_name: Some("macOS Accessibility".to_string()),
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn resolve_computer_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("MEDOUSA_COMPUTER_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let candidate = directory.join(binary_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let data_binary = medousa_data_dir().join("bin").join(binary_name());
    if data_binary.is_file() {
        return Some(data_binary);
    }
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("/usr/local/bin").join(binary_name()));
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(binary_name()));
    }
    if cfg!(target_os = "linux") {
        candidates.push(PathBuf::from("/usr/local/bin").join(binary_name()));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|path| path.join(binary_name())));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "medousa-computer.exe"
    } else {
        "medousa-computer"
    }
}

struct SidecarComputerDriver {
    binary: PathBuf,
    registration: WorldDriverRegistration,
    process: Mutex<Option<ComputerSidecarProcess>>,
}

impl SidecarComputerDriver {
    fn new(binary: PathBuf, registration: WorldDriverRegistration) -> Self {
        Self {
            binary,
            registration,
            process: Mutex::new(None),
        }
    }

    async fn request(
        &self,
        request: ComputerDriverRequest,
    ) -> Result<ComputerDriverResponseResult, String> {
        let request_id = format!("computer:{}", Uuid::new_v4());
        let envelope = ComputerDriverRequestEnvelope::new(request_id.clone(), request);
        let encoded = serde_json::to_vec(&envelope)
            .map_err(|error| format!("serialize computer sidecar request: {error}"))?;
        if encoded.len() > MAX_COMPUTER_DRIVER_MESSAGE_BYTES {
            return Err("computer sidecar request exceeded its framing limit".to_string());
        }

        let mut process = self.process.lock().await;
        for attempt in 0..2 {
            if process.is_none() {
                *process = Some(ComputerSidecarProcess::spawn(&self.binary)?);
            }
            let exchange = process
                .as_mut()
                .expect("computer sidecar process")
                .exchange(&encoded);
            let response = match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                exchange,
            )
            .await
            .map_err(|_| "native computer sidecar request timed out".to_string())
            .and_then(|response| response)
            {
                Ok(response) => response,
                Err(error) if attempt == 0 => {
                    stop_process(&mut process);
                    tracing::warn!(%error, "restarting native computer sidecar after transport failure");
                    continue;
                }
                Err(error) => {
                    stop_process(&mut process);
                    return Err(error);
                }
            };
            if response.protocol_version != COMPUTER_DRIVER_PROTOCOL_VERSION {
                stop_process(&mut process);
                return Err(format!(
                    "computer sidecar protocol {} is unsupported",
                    response.protocol_version
                ));
            }
            if response.request_id != request_id {
                stop_process(&mut process);
                return Err("computer sidecar response crossed request identities".to_string());
            }
            return match response.response {
                ComputerDriverResponse::Success { result } => Ok(*result),
                ComputerDriverResponse::Error { error } => {
                    Err(format!("{}: {}", error.code, error.message))
                }
            };
        }
        Err("computer sidecar request exhausted its retry boundary".to_string())
    }

    async fn action_request(
        &self,
        request: ComputerActionRequest,
    ) -> Result<ComputerActionReceipt, ComputerDriverActionError> {
        let expected = request.clone();
        let request_id = format!("computer:{}", Uuid::new_v4());
        let envelope = ComputerDriverRequestEnvelope::new(
            request_id.clone(),
            ComputerDriverRequest::Act { request },
        );
        let encoded = serde_json::to_vec(&envelope).map_err(|error| {
            ComputerDriverActionError::failed(format!(
                "serialize computer sidecar action: {error}"
            ))
        })?;
        if encoded.len() > MAX_COMPUTER_DRIVER_MESSAGE_BYTES {
            return Err(ComputerDriverActionError::failed(
                "computer sidecar action exceeded its framing limit",
            ));
        }

        let mut process = self.process.lock().await;
        if process.is_none() {
            *process = Some(
                ComputerSidecarProcess::spawn(&self.binary)
                    .map_err(ComputerDriverActionError::failed)?,
            );
        }
        let exchange = process
            .as_mut()
            .expect("computer sidecar process")
            .exchange(&encoded);
        let response = match tokio::time::timeout(std::time::Duration::from_secs(10), exchange)
            .await
            .map_err(|_| "native computer sidecar action timed out".to_string())
            .and_then(|response| response)
        {
            Ok(response) => response,
            Err(error) => {
                stop_process(&mut process);
                return Err(ComputerDriverActionError::indeterminate(format!(
                    "{error}; the semantic action was not retried because its outcome is unknown"
                )));
            }
        };
        if response.protocol_version != COMPUTER_DRIVER_PROTOCOL_VERSION
            || response.request_id != request_id
        {
            stop_process(&mut process);
            return Err(ComputerDriverActionError::indeterminate(
                "native computer sidecar returned an incompatible action acknowledgement",
            ));
        }
        match response.response {
            ComputerDriverResponse::Success { result } => match *result {
                ComputerDriverResponseResult::Action { receipt } => {
                    if let Err(error) =
                        receipt.validate_for(&self.registration.driver_id, &expected)
                    {
                        stop_process(&mut process);
                        return Err(ComputerDriverActionError::indeterminate(error));
                    }
                    Ok(receipt)
                }
                _ => {
                    stop_process(&mut process);
                    Err(ComputerDriverActionError::indeterminate(
                        "native computer sidecar returned the wrong result for an action",
                    ))
                }
            },
            ComputerDriverResponse::Error { error } => Err(ComputerDriverActionError::failed(
                format!("{}: {}", error.code, error.message),
            )),
        }
    }
}

#[async_trait]
impl ComputerDriver for SidecarComputerDriver {
    fn registration(&self) -> WorldDriverRegistration {
        self.registration.clone()
    }

    async fn preflight(&self) -> Result<ComputerDriverPreflight, String> {
        match self.request(ComputerDriverRequest::Preflight).await? {
            ComputerDriverResponseResult::Preflight { report } => Ok(report),
            ComputerDriverResponseResult::Observation { .. }
            | ComputerDriverResponseResult::Action { .. } => {
                Err("computer sidecar returned an observation for preflight".to_string())
            }
        }
    }

    async fn observe(
        &self,
        request: ComputerObservationRequest,
    ) -> Result<ComputerObservation, String> {
        match self
            .request(ComputerDriverRequest::Observe { request })
            .await?
        {
            ComputerDriverResponseResult::Observation { observation } => Ok(observation),
            ComputerDriverResponseResult::Preflight { .. }
            | ComputerDriverResponseResult::Action { .. } => {
                Err("computer sidecar returned preflight for an observation".to_string())
            }
        }
    }

    async fn act(
        &self,
        request: ComputerActionRequest,
    ) -> Result<ComputerActionReceipt, ComputerDriverActionError> {
        self.action_request(request).await
    }
}

fn stop_process(process: &mut Option<ComputerSidecarProcess>) {
    if let Some(mut child) = process.take() {
        let _ = child.child.start_kill();
    }
}

struct ComputerSidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ComputerSidecarProcess {
    fn spawn(binary: &PathBuf) -> Result<Self, String> {
        let mut command = Command::new(binary);
        command
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        medousa_host::hide_tokio_subprocess_window(&mut command);
        let mut child = command.spawn().map_err(|error| {
            format!(
                "spawn native computer sidecar {}: {error}",
                binary.display()
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "native computer sidecar stdin is unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "native computer sidecar stdout is unavailable".to_string())?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    async fn exchange(
        &mut self,
        encoded: &[u8],
    ) -> Result<ComputerDriverResponseEnvelope, String> {
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| format!("inspect native computer sidecar: {error}"))?
        {
            return Err(format!("native computer sidecar exited with {status}"));
        }
        self.stdin
            .write_all(encoded)
            .await
            .map_err(|error| format!("write native computer sidecar request: {error}"))?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|error| format!("write native computer sidecar request: {error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("flush native computer sidecar request: {error}"))?;
        let line = read_bounded_line(&mut self.stdout).await?;
        serde_json::from_slice(&line)
            .map_err(|error| format!("decode native computer sidecar response: {error}"))
    }
}

async fn read_bounded_line(reader: &mut BufReader<ChildStdout>) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .await
            .map_err(|error| format!("read native computer sidecar response: {error}"))?;
        if buffer.is_empty() {
            return Err("native computer sidecar closed its response stream".to_string());
        }
        let consumed = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        let ended = buffer[consumed - 1] == b'\n';
        let content_len = consumed - usize::from(ended);
        if line.len().saturating_add(content_len) > MAX_COMPUTER_DRIVER_MESSAGE_BYTES {
            return Err("native computer sidecar response exceeded its framing limit".to_string());
        }
        line.extend_from_slice(&buffer[..content_len]);
        reader.consume(consumed);
        if ended {
            return Ok(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_registration_is_platform_exact() {
        let registration = native_registration();
        #[cfg(target_os = "macos")]
        {
            let registration = registration.expect("macOS registration");
            assert_eq!(registration.driver_id.as_str(), MACOS_DRIVER_ID);
            assert_eq!(registration.transport, WorldDriverTransport::LocalSidecar);
            assert_eq!(registration.kind, WorldDriverKind::NativeDesktop);
        }
        #[cfg(not(target_os = "macos"))]
        assert!(registration.is_none());
    }
}
