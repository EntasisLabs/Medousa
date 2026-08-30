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
use zeroize::Zeroizing;

use crate::openshell_handoff::{
    medousa_openshell_policies_dir, probe_openshell_readyz, probe_tcp_endpoint,
    resolve_openshell_cli_binary, resolve_openshell_gateway_url,
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
    /// OpenShell provider names resolved from opaque, session-bound grants.
    /// Names are non-secret; credential values never enter the job payload.
    #[serde(default)]
    pub providers: Vec<String>,
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

struct SecretCliRunResult {
    status_code: Option<i32>,
    stderr: Zeroizing<String>,
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
            let providers = payload.providers.clone();
            move || {
                run_sandbox_create(
                    &gateway_url,
                    &sandbox_name,
                    &sandbox_from,
                    policy_path.as_deref(),
                    &providers,
                )
            }
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
            .map_err(|err| {
                StasisError::PortFailure(format!("openshell upload join error: {err}"))
            })?;
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

        let timeout_secs = payload.timeout_secs.unwrap_or(DEFAULT_EXEC_TIMEOUT_SECS);
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
            "providers": payload.providers,
        })
        .to_string();

        if exec_result.status_code == Some(0) {
            Ok(JobExecutionOutcome::Success {
                output_provenance: None,
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
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
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
    if path.is_file() { Some(path) } else { None }
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
    let fallback = if cfg!(windows) {
        "openshell.exe"
    } else {
        "openshell"
    };
    let mut command = std::process::Command::new(
        resolve_openshell_cli_binary().unwrap_or_else(|| std::path::PathBuf::from(fallback)),
    );
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

pub fn openshell_providers_v2_enabled() -> Result<bool, String> {
    let gateway_url = resolve_openshell_gateway_url(None);
    let mut command = openshell_command(&gateway_url);
    command
        .arg("settings")
        .arg("get")
        .arg("--global")
        .arg("--json");
    let result = run_cli_capture(&mut command, "settings get --global")?;
    parse_openshell_providers_v2_settings(&result.stdout)
}

pub fn validate_openshell_provider_profile(
    provider_type: &str,
    credential_key: &str,
) -> Result<(), String> {
    let gateway_url = resolve_openshell_gateway_url(None);
    let mut command = openshell_command(&gateway_url);
    command
        .arg("provider")
        .arg("profile")
        .arg("export")
        .arg(provider_type)
        .arg("--output")
        .arg("json");
    let result = run_cli_capture(&mut command, "provider profile export")?;
    validate_openshell_provider_profile_json(&result.stdout, provider_type, credential_key)
}

fn validate_openshell_provider_profile_json(
    raw: &str,
    provider_type: &str,
    credential_key: &str,
) -> Result<(), String> {
    let profile: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| format!("invalid OpenShell provider profile JSON: {err}"))?;
    let key_declared = profile
        .get("credentials")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|credentials| {
            credentials.iter().any(|credential| {
                credential
                    .get("env_vars")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|env_vars| {
                        env_vars
                            .iter()
                            .any(|value| value.as_str() == Some(credential_key))
                    })
            })
        });
    if !key_declared {
        return Err(format!(
            "credential key {credential_key} is not declared by OpenShell provider profile {provider_type}"
        ));
    }
    let has_endpoints = profile
        .get("endpoints")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|endpoints| !endpoints.is_empty());
    if !has_endpoints {
        return Err(format!(
            "OpenShell provider profile {provider_type} has no endpoint credential binding"
        ));
    }
    Ok(())
}

fn parse_openshell_providers_v2_settings(raw: &str) -> Result<bool, String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| format!("invalid OpenShell global settings JSON: {err}"))?;
    let Some(setting) = value
        .get("settings")
        .and_then(|settings| settings.get("providers_v2_enabled"))
    else {
        return Ok(false);
    };
    if let Some(enabled) = setting.as_bool() {
        return Ok(enabled);
    }
    match setting.as_str().map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err("OpenShell providers_v2_enabled setting is not boolean".to_string()),
    }
}

/// Create an OpenShell provider without placing credential material in argv.
/// The CLI reads the bare credential key from its child-only environment and
/// sends it to the gateway; the owned value is zeroized when this call exits.
pub fn provision_openshell_provider(
    provider_name: &str,
    provider_type: &str,
    credential_key: &str,
    secret: Zeroizing<String>,
) -> Result<(), String> {
    let gateway_url = resolve_openshell_gateway_url(None);
    if secret.trim().is_empty() {
        return Err("credential value must not be empty".to_string());
    }
    if secret.as_bytes().contains(&0) {
        return Err("credential value contains an unsupported NUL byte".to_string());
    }
    if !openshell_providers_v2_enabled()? {
        return Err(
            "OpenShell Providers v2 must be enabled before storing agent credentials".to_string(),
        );
    }
    validate_openshell_provider_profile(provider_type, credential_key)?;

    let mut command = openshell_command(&gateway_url);
    command
        .arg("provider")
        .arg("create")
        .arg("--name")
        .arg(provider_name)
        .arg("--type")
        .arg(provider_type)
        // Bare-key form: OpenShell reads the value from the environment.
        .arg("--credential")
        .arg(credential_key)
        .env(credential_key, secret.as_str());

    let result = run_secret_cli_capture(&mut command, "provider create");
    if result.status_code == Some(0) {
        return Ok(());
    }
    // Defense in depth if a future CLI version echoes environment material.
    let stderr = truncate_output(&result.stderr.replace(secret.as_str(), "[REDACTED]"));
    Err(format!(
        "OpenShell provider creation failed (exit={:?}): {stderr}",
        result.status_code
    ))
}

/// Roll back a provider that was created after its waiting turn was cancelled,
/// denied, or expired.
pub fn delete_openshell_provider(provider_name: &str) -> Result<(), String> {
    let gateway_url = resolve_openshell_gateway_url(None);
    let mut command = openshell_command(&gateway_url);
    command.arg("provider").arg("delete").arg(provider_name);
    run_cli_capture(&mut command, "provider delete")
        .map(|_| ())
        .map_err(|err| format!("OpenShell provider delete failed: {err}"))
}

fn run_sandbox_create(
    gateway_url: &str,
    sandbox_name: &str,
    sandbox_from: &str,
    policy_path: Option<&std::path::Path>,
    providers: &[String],
) -> Result<(), String> {
    let mut command = openshell_command(gateway_url);
    command
        .arg("sandbox")
        .arg("create")
        .arg("--name")
        .arg(sandbox_name)
        .arg("--from")
        .arg(sandbox_from)
        // Medousa attaches only explicit, operator-issued grants. Never let
        // OpenShell discover unrelated daemon-host credentials implicitly.
        .arg("--no-auto-providers")
        .arg("--no-tty");
    for provider in providers {
        command.arg("--provider").arg(provider);
    }
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
    let assets_path = std::path::Path::new(local_assets_dir);
    let assets_root = crate::store_root::StoreRoot::open_nofollow(assets_path)
        .map_err(|error| format!("open skill upload root: {error}"))?;
    let mut command = openshell_command(gateway_url);
    assets_root
        .configure_command_current_dir(&mut command, assets_path)
        .map_err(|error| format!("pin skill upload root: {error}"))?;
    command
        .arg("sandbox")
        .arg("upload")
        .arg(sandbox_name)
        .arg(".")
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

fn run_cli_capture(
    command: &mut std::process::Command,
    label: &str,
) -> Result<CliRunResult, String> {
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

fn run_secret_cli_capture(command: &mut std::process::Command, label: &str) -> SecretCliRunResult {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let output = match command.output() {
        Ok(output) => output,
        Err(err) => {
            return SecretCliRunResult {
                status_code: None,
                stderr: Zeroizing::new(format!("{label} spawn error: {err}")),
            };
        }
    };
    let stderr_bytes = Zeroizing::new(output.stderr);
    SecretCliRunResult {
        status_code: output.status.code(),
        stderr: Zeroizing::new(String::from_utf8_lossy(&stderr_bytes).into_owned()),
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
    run_sandbox_create(
        &gateway_url,
        &sandbox_name,
        sandbox_from,
        policy_path.as_deref(),
        &[],
    )?;
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
    let from = payload.sandbox_from.as_deref().unwrap_or(sandbox_from);
    run_sandbox_create(
        &gateway_url,
        &sandbox_name,
        from,
        policy_path.as_deref(),
        &payload.providers,
    )?;
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
    use crate::openshell_handoff::ENV_OPENSHELL_GATEWAY_URL;

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
            providers: Vec::new(),
        };
        let raw = payload.to_payload_ref().expect("encode");
        let decoded: OpenshellSandboxRunPayload = serde_json::from_str(&raw).expect("decode");
        assert_eq!(decoded.command, payload.command);
    }

    #[test]
    fn gateway_url_env_constant_is_stable() {
        assert_eq!(ENV_OPENSHELL_GATEWAY_URL, "MEDOUSA_OPENSHELL_GATEWAY_URL");
    }

    #[test]
    fn providers_v2_setting_is_parsed_fail_closed() {
        assert!(
            parse_openshell_providers_v2_settings(
                r#"{"settings":{"providers_v2_enabled":"true"}}"#
            )
            .unwrap()
        );
        assert!(!parse_openshell_providers_v2_settings(r#"{"settings":{}}"#).unwrap());
        assert!(
            parse_openshell_providers_v2_settings(
                r#"{"settings":{"providers_v2_enabled":"maybe"}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn provider_profile_must_bind_the_requested_key_and_an_endpoint() {
        let profile = r#"{
            "credentials": [{"name":"token","env_vars":["GITHUB_TOKEN","GH_TOKEN"]}],
            "endpoints": [{"host":"api.github.com","port":443}]
        }"#;
        assert!(
            validate_openshell_provider_profile_json(profile, "github", "GITHUB_TOKEN").is_ok()
        );
        assert!(
            validate_openshell_provider_profile_json(profile, "github", "OPENAI_API_KEY")
                .unwrap_err()
                .contains("not declared")
        );
        assert!(
            validate_openshell_provider_profile_json(
                r#"{"credentials":[{"env_vars":["GITHUB_TOKEN"]}],"endpoints":[]}"#,
                "github",
                "GITHUB_TOKEN"
            )
            .unwrap_err()
            .contains("no endpoint")
        );
    }
}
