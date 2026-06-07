//! Stasis job handler: create → exec → destroy via OpenShell CLI (Sprint B4).

use std::process::Stdio;
use std::time::Duration;

use uuid::Uuid;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::runtime::job::Job;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::openshell_handoff::{
    medousa_openshell_policies_dir, probe_openshell_readyz, probe_tcp_endpoint,
    resolve_openshell_gateway_url,
};

pub const OPENSHELL_SANDBOX_RUN_JOB_TYPE: &str = "openshell.sandbox.run";

const MAX_CAPTURED_OUTPUT_BYTES: usize = 32_768;
const DEFAULT_SANDBOX_FROM: &str = "base";
const DEFAULT_EXEC_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenshellSandboxRunPayload {
    pub command: Vec<String>,
    #[serde(default)]
    pub sandbox_from: Option<String>,
    #[serde(default)]
    pub policy_template: Option<String>,
    #[serde(default = "default_destroy_on_complete")]
    pub destroy_on_complete: bool,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub skill_assets_dir: Option<String>,
    #[serde(default)]
    pub skill_upload_dest: Option<String>,
    #[serde(default)]
    pub skill_script: Option<String>,
}

fn default_destroy_on_complete() -> bool {
    true
}

impl OpenshellSandboxRunPayload {
    pub fn to_payload_ref(&self) -> StasisResult<String> {
        serde_json::to_string(self).map_err(|err| {
            StasisError::PortFailure(format!("failed to encode openshell sandbox payload: {err}"))
        })
    }
}

pub async fn register_openshell_sandbox_run_handler(
    composition: &RuntimeComposition,
) -> anyhow::Result<()> {
    let handler = OpenshellSandboxRunJobHandler;
    match composition {
        RuntimeComposition::InMemory(rt) => rt.register_handler(handler)?,
        RuntimeComposition::Surreal(rt) => rt.register_handler(handler)?,
    }
    Ok(())
}

struct OpenshellSandboxRunJobHandler;

struct CliRunResult {
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[async_trait]
impl JobHandler for OpenshellSandboxRunJobHandler {
    fn job_type(&self) -> &'static str {
        OPENSHELL_SANDBOX_RUN_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let payload: OpenshellSandboxRunPayload =
            serde_json::from_str(&job.payload_ref).map_err(|err| {
                StasisError::PortFailure(format!(
                    "invalid openshell sandbox payload for job {}: {err}",
                    job.id
                ))
            })?;

        if payload.command.is_empty() {
            return Ok(fatal_outcome(
                "openshell sandbox payload.command must be non-empty",
                None,
            ));
        }

        let gateway_url = resolve_openshell_gateway_url(None);
        if !probe_tcp_endpoint(&gateway_url, Duration::from_millis(500)) {
            return Ok(fatal_outcome(
                format!("openshell gateway not reachable at {gateway_url}"),
                Some(json!({ "gateway_url": gateway_url, "stage": "preflight" }).to_string()),
            ));
        }
        if !probe_openshell_readyz(&gateway_url) {
            return Ok(fatal_outcome(
                format!("openshell gateway /readyz failed at {gateway_url}"),
                Some(json!({ "gateway_url": gateway_url, "stage": "preflight" }).to_string()),
            ));
        }

        let sandbox_name = sandbox_name_for_job(&job.id);
        let sandbox_from = payload
            .sandbox_from
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_SANDBOX_FROM)
            .to_string();
        let policy_path = payload
            .policy_template
            .as_deref()
            .and_then(resolve_policy_template_path);

        eprintln!(
            "medousa openshell_sandbox_run job_id={} sandbox={sandbox_name} from={sandbox_from} manuscript={}",
            job.id,
            payload.manuscript_id.as_deref().unwrap_or("-"),
        );

        let create_result = tokio::task::spawn_blocking({
            let sandbox_name = sandbox_name.clone();
            let sandbox_from = sandbox_from.clone();
            let policy_path = policy_path.clone();
            let gateway_url = gateway_url.clone();
            move || run_sandbox_create(&gateway_url, &sandbox_name, &sandbox_from, policy_path.as_deref())
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("openshell create join error: {err}")))?;

        if let Err(message) = create_result {
            return Ok(fatal_outcome(
                message,
                Some(
                    json!({
                        "gateway_url": gateway_url,
                        "sandbox_name": sandbox_name,
                        "stage": "create",
                    })
                    .to_string(),
                ),
            ));
        }

        if let (Some(assets_dir), Some(upload_dest)) = (
            payload.skill_assets_dir.as_deref(),
            payload.skill_upload_dest.as_deref(),
        ) {
            let upload_result = tokio::task::spawn_blocking({
                let sandbox_name = sandbox_name.clone();
                let gateway_url = gateway_url.clone();
                let assets_dir = assets_dir.to_string();
                let upload_dest = upload_dest.to_string();
                move || run_sandbox_upload(&gateway_url, &sandbox_name, &assets_dir, &upload_dest)
            })
            .await
            .map_err(|err| StasisError::PortFailure(format!("openshell upload join error: {err}")))?;
            if let Err(message) = upload_result {
                let _ = tokio::task::spawn_blocking({
                    let sandbox_name = sandbox_name.clone();
                    let gateway_url = gateway_url.clone();
                    move || run_sandbox_delete(&gateway_url, &sandbox_name)
                })
                .await;
                return Ok(fatal_outcome(
                    message,
                    Some(
                        json!({
                            "gateway_url": gateway_url,
                            "sandbox_name": sandbox_name,
                            "stage": "upload",
                            "skill_assets_dir": assets_dir,
                            "skill_upload_dest": upload_dest,
                        })
                        .to_string(),
                    ),
                ));
            }
        }

        let timeout_secs = payload
            .timeout_secs
            .unwrap_or(DEFAULT_EXEC_TIMEOUT_SECS);
        let exec_result = tokio::task::spawn_blocking({
            let sandbox_name = sandbox_name.clone();
            let command = payload.command.clone();
            let workdir = payload.workdir.clone();
            let gateway_url = gateway_url.clone();
            move || {
                run_sandbox_exec(
                    &gateway_url,
                    &sandbox_name,
                    &command,
                    workdir.as_deref(),
                    timeout_secs,
                )
            }
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("openshell exec join error: {err}")))?;

        let destroy_result = if payload.destroy_on_complete {
            tokio::task::spawn_blocking({
                let sandbox_name = sandbox_name.clone();
                let gateway_url = gateway_url.clone();
                move || run_sandbox_delete(&gateway_url, &sandbox_name)
            })
            .await
            .ok()
        } else {
            None
        };

        let diagnostics = json!({
            "provider": "openshell-cli",
            "job_type": OPENSHELL_SANDBOX_RUN_JOB_TYPE,
            "gateway_url": gateway_url,
            "sandbox_name": sandbox_name,
            "sandbox_from": sandbox_from,
            "policy_template": payload.policy_template,
            "manuscript_id": payload.manuscript_id,
            "correlation_id": payload.correlation_id,
            "exit_code": exec_result.status_code,
            "stdout": truncate_output(&exec_result.stdout),
            "stderr": truncate_output(&exec_result.stderr),
            "destroy_on_complete": payload.destroy_on_complete,
            "destroy_ok": destroy_result.map(|value| value.is_ok()),
            "skill_script": payload.skill_script,
            "skill_upload_dest": payload.skill_upload_dest,
        })
        .to_string();

        if exec_result.status_code == Some(0) {
            Ok(JobExecutionOutcome::Success {
                sttp_output_node_id: format!("sttp:out:openshell-sandbox:{}", job.id),
                execution_id: Some(sandbox_name),
                diagnostics: Some(diagnostics),
            })
        } else {
            Ok(JobExecutionOutcome::FatalFailure {
                message: format!(
                    "openshell sandbox exec failed (exit={:?})",
                    exec_result.status_code
                ),
                execution_id: Some(sandbox_name),
                diagnostics: Some(diagnostics),
            })
        }
    }
}

fn fatal_outcome(message: impl Into<String>, diagnostics: Option<String>) -> JobExecutionOutcome {
    JobExecutionOutcome::FatalFailure {
        message: message.into(),
        execution_id: None,
        diagnostics,
    }
}

pub fn sandbox_name_for_job(job_id: &str) -> String {
    let slug: String = job_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch
            } else {
                '-'
            }
        })
        .take(24)
        .collect();
    format!("medousa-{slug}")
}

pub fn resolve_policy_template_path(template: &str) -> Option<std::path::PathBuf> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = if trimmed.ends_with(".yaml") || trimmed.ends_with(".yml") {
        medousa_openshell_policies_dir().join(trimmed)
    } else {
        medousa_openshell_policies_dir().join(format!("{trimmed}.yaml"))
    };
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

fn truncate_output(text: &str) -> String {
    if text.len() <= MAX_CAPTURED_OUTPUT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_CAPTURED_OUTPUT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated]", &text[..end])
}

fn openshell_command(gateway_url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("openshell");
    command.arg("--gateway-endpoint").arg(gateway_url);
    if gateway_url.starts_with("http://") {
        command.arg("--gateway-insecure");
    }
    if let Ok(name) = std::env::var("OPENSHELL_GATEWAY") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            command.arg("--gateway").arg(trimmed);
        }
    }
    command
}

fn run_sandbox_create(
    gateway_url: &str,
    sandbox_name: &str,
    sandbox_from: &str,
    policy_path: Option<&std::path::Path>,
) -> Result<(), String> {
    let mut command = openshell_command(gateway_url);
    command
        .arg("sandbox")
        .arg("create")
        .arg("--name")
        .arg(sandbox_name)
        .arg("--from")
        .arg(sandbox_from)
        .arg("--no-tty");
    if let Some(path) = policy_path {
        command.arg("--policy").arg(path);
    }
    run_cli_capture(&mut command, "sandbox create")
        .map(|_| ())
        .map_err(|err| format!("openshell sandbox create failed: {err}"))
}

fn run_sandbox_exec(
    gateway_url: &str,
    sandbox_name: &str,
    command_argv: &[String],
    workdir: Option<&str>,
    timeout_secs: u64,
) -> CliRunResult {
    let mut command = openshell_command(gateway_url);
    command
        .arg("sandbox")
        .arg("exec")
        .arg("--name")
        .arg(sandbox_name)
        .arg("--no-tty")
        .arg("--timeout")
        .arg(timeout_secs.to_string());
    if let Some(dir) = workdir.filter(|value| !value.trim().is_empty()) {
        command.arg("--workdir").arg(dir);
    }
    command.arg("--");
    for part in command_argv {
        command.arg(part);
    }
    run_cli_capture_allow_failure(&mut command, "sandbox exec")
}

fn run_sandbox_upload(
    gateway_url: &str,
    sandbox_name: &str,
    local_assets_dir: &str,
    dest_path: &str,
) -> Result<(), String> {
    let mut command = openshell_command(gateway_url);
    command
        .arg("sandbox")
        .arg("upload")
        .arg(sandbox_name)
        .arg(local_assets_dir)
        .arg(dest_path);
    run_cli_capture(&mut command, "sandbox upload")
        .map(|_| ())
        .map_err(|err| format!("openshell sandbox upload failed: {err}"))
}

fn run_sandbox_delete(gateway_url: &str, sandbox_name: &str) -> Result<(), String> {
    let mut command = openshell_command(gateway_url);
    command
        .arg("sandbox")
        .arg("delete")
        .arg(sandbox_name)
        .arg("--yes");
    run_cli_capture(&mut command, "sandbox delete")
        .map(|_| ())
        .map_err(|err| format!("openshell sandbox delete failed: {err}"))
}

fn run_cli_capture(command: &mut std::process::Command, label: &str) -> Result<CliRunResult, String> {
    let result = run_cli_capture_allow_failure(command, label);
    if result.status_code == Some(0) {
        return Ok(result);
    }
    Err(format!(
        "{label} exit={:?} stderr={}",
        result.status_code,
        truncate_output(&result.stderr)
    ))
}

fn run_cli_capture_allow_failure(command: &mut std::process::Command, label: &str) -> CliRunResult {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = match command.output() {
        Ok(output) => output,
        Err(err) => {
            return CliRunResult {
                status_code: None,
                stdout: String::new(),
                stderr: format!("{label} spawn error: {err}"),
            };
        }
    };
    CliRunResult {
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[derive(Debug, Clone)]
pub struct OpenshellProbeReceipt {
    pub sandbox_name: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

pub fn probe_grapheme_in_sandbox(
    sandbox_from: &str,
    policy_template: Option<&str>,
) -> Result<OpenshellProbeReceipt, String> {
    let gateway_url = resolve_openshell_gateway_url(None);
    preflight_gateway(&gateway_url)?;
    let sandbox_name = format!("medousa-probe-{}", Uuid::new_v4().simple());
    let policy_path = policy_template.and_then(resolve_policy_template_path);
    run_sandbox_create(&gateway_url, &sandbox_name, sandbox_from, policy_path.as_deref())?;
    let exec = run_sandbox_exec(
        &gateway_url,
        &sandbox_name,
        &["grapheme".to_string(), "--version".to_string()],
        Some("/sandbox"),
        120,
    );
    let _ = run_sandbox_delete(&gateway_url, &sandbox_name);
    if exec.status_code != Some(0) {
        return Err(format!(
            "grapheme probe failed exit={:?} stderr={}",
            exec.status_code,
            truncate_output(&exec.stderr)
        ));
    }
    Ok(OpenshellProbeReceipt {
        sandbox_name,
        stdout: exec.stdout,
        stderr: exec.stderr,
        exit_code: exec.status_code,
    })
}

pub fn probe_skill_script_in_sandbox(
    manuscript_id: &str,
    script_relative: &str,
    sandbox_from: &str,
    policy_template: Option<&str>,
) -> Result<OpenshellProbeReceipt, String> {
    let gateway_url = resolve_openshell_gateway_url(None);
    preflight_gateway(&gateway_url)?;
    let manuscript = crate::identity_manuscript::build_manuscript_context(manuscript_id)
        .map_err(|err| err.to_string())?;
    let payload = crate::skill_execution::build_sandbox_payload_for_skill(
        manuscript_id,
        script_relative,
        &manuscript,
        None,
    )
    .map_err(|err| err.to_string())?;
    let sandbox_name = format!("medousa-probe-{}", Uuid::new_v4().simple());
    let policy_path = policy_template
        .and_then(resolve_policy_template_path)
        .or_else(|| {
            payload
                .policy_template
                .as_deref()
                .and_then(resolve_policy_template_path)
        });
    let from = payload
        .sandbox_from
        .as_deref()
        .unwrap_or(sandbox_from);
    run_sandbox_create(&gateway_url, &sandbox_name, from, policy_path.as_deref())?;
    if let (Some(assets), Some(dest)) = (
        payload.skill_assets_dir.as_deref(),
        payload.skill_upload_dest.as_deref(),
    ) {
        run_sandbox_upload(&gateway_url, &sandbox_name, assets, dest)?;
    }
    let exec = run_sandbox_exec(
        &gateway_url,
        &sandbox_name,
        &payload.command,
        payload.workdir.as_deref(),
        payload.timeout_secs.unwrap_or(DEFAULT_EXEC_TIMEOUT_SECS),
    );
    let _ = run_sandbox_delete(&gateway_url, &sandbox_name);
    if exec.status_code != Some(0) {
        return Err(format!(
            "skill probe failed exit={:?} stderr={}",
            exec.status_code,
            truncate_output(&exec.stderr)
        ));
    }
    Ok(OpenshellProbeReceipt {
        sandbox_name,
        stdout: exec.stdout,
        stderr: exec.stderr,
        exit_code: exec.status_code,
    })
}

fn preflight_gateway(gateway_url: &str) -> Result<(), String> {
    if !probe_tcp_endpoint(gateway_url, Duration::from_millis(500)) {
        return Err(format!("openshell gateway not reachable at {gateway_url}"));
    }
    if !probe_openshell_readyz(gateway_url) {
        return Err(format!("openshell gateway /readyz failed at {gateway_url}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_name_is_slugged() {
        let name = sandbox_name_for_job("job/with spaces!");
        assert!(name.starts_with("medousa-"));
        assert!(!name.contains(' '));
        assert!(!name.contains('/'));
    }

    #[test]
    fn payload_round_trip() {
        let payload = OpenshellSandboxRunPayload {
            command: vec!["echo".to_string(), "hi".to_string()],
            sandbox_from: Some("base".to_string()),
            policy_template: Some("research-readonly".to_string()),
            destroy_on_complete: true,
            workdir: None,
            timeout_secs: Some(30),
            manuscript_id: None,
            correlation_id: None,
            skill_assets_dir: None,
            skill_upload_dest: None,
            skill_script: None,
        };
        let raw = payload.to_payload_ref().expect("encode");
        let decoded: OpenshellSandboxRunPayload =
            serde_json::from_str(&raw).expect("decode");
        assert_eq!(decoded.command, payload.command);
    }

    #[test]
    fn gateway_url_env_constant_is_stable() {
        assert_eq!(ENV_OPENSHELL_GATEWAY_URL, "MEDOUSA_OPENSHELL_GATEWAY_URL");
    }
}
