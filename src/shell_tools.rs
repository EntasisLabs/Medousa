//! Cognition tools for Medousa OS-native shell.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::shell_sandbox::{ShellSandboxStatus, probe_shell_sandbox, shell_agent_tools_enabled};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

pub const COGNITION_SHELL_STATUS: &str = "cognition_shell_status";
pub const COGNITION_SHELL_RUN: &str = "cognition_shell_run";

const COGNITION_SHELL_STATUS_ID: ToolId = ToolId::new(COGNITION_SHELL_STATUS);
const COGNITION_SHELL_RUN_ID: ToolId = ToolId::new(COGNITION_SHELL_RUN);

pub use crate::tool_names::SHELL_COGNITION_TOOLS;

pub fn is_shell_cognition_tool(name: &str) -> bool {
    crate::tool_names::is_shell_cognition_tool(name)
}

fn ensure_shell_agent_tools_enabled() -> StasisResult<()> {
    ensure_shell_agent_tools_enabled_flag(shell_agent_tools_enabled())
}

fn ensure_shell_agent_tools_enabled_flag(enabled: bool) -> StasisResult<()> {
    if enabled {
        return Ok(());
    }
    Err(StasisError::PortFailure(
        "Shell agent tools are disabled. Enable them in Settings → Shell before calling cognition_shell_*."
            .to_string(),
    ))
}

pub fn register_shell_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionShellStatusTool)?;
    registry.register_typed_tool(CognitionShellRunTool::new(runtime))?;
    registry.register_typed_tool(CognitionProcessStatusTool)?;
    Ok(())
}

const COGNITION_PROCESS_STATUS_ID: ToolId = ToolId::new("cognition_process_status");

pub struct CognitionProcessStatusTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProcessStatusInput {
    handle: String,
}

#[medousa_tool(id = COGNITION_PROCESS_STATUS_ID)]
impl CognitionProcessStatusTool {
    /// Read one supervised background process. The first check returns status. Further checks are refused while a completion wake is already registered; finish the turn and wait for process_completed or timed_out.
    async fn invoke_typed(
        &self,
        input: ProcessStatusInput,
    ) -> stasis::prelude::Result<ExternalJson> {
        ensure_shell_agent_tools_enabled()?;
        crate::process_supervisor::check(&input.handle)
            .map(ExternalJson::new)
            .map_err(StasisError::PortFailure)
    }
}

fn background_grace_ms(input: &ShellRunInput) -> Option<u64> {
    if input.foreground == Some(true)
        || input.background == Some(true)
        || input.timeout_ms.is_some()
    {
        return None;
    }
    input.background_after_ms.or_else(|| {
        std::env::var("MEDOUSA_SHELL_BACKGROUND_AFTER_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
    })
}

async fn maybe_background(input: &ShellRunInput) -> stasis::prelude::Result<Option<Value>> {
    let explicit = input.background == Some(true);
    let grace = background_grace_ms(input);
    if !explicit && grace.is_none() {
        return Ok(None);
    }
    if crate::work_environment_tools::EnvironmentToolInvocation::active(COGNITION_SHELL_RUN)
        .is_some()
    {
        return Err(StasisError::PortFailure(
            "background wakes are for the native shell. This bound work environment runs in the turn. Allowed action: omit background, or run the command on the native shell."
                .into(),
        ));
    }
    let turn = crate::agent_runtime::execution_context::active_turn_execution_context()
        .ok_or_else(|| StasisError::PortFailure("shell requires an admitted turn".into()))?;
    let argv = match (input.argv.clone(), input.command.clone()) {
        (Some(argv), _) if !argv.is_empty() => argv,
        (_, Some(command)) if !command.trim().is_empty() => {
            vec!["/bin/sh".into(), "-lc".into(), command]
        }
        _ => {
            return Err(StasisError::PortFailure(
                "provide command or a non-empty argv".into(),
            ));
        }
    };
    let deadline = input
        .deadline_ms
        .or(if explicit { input.timeout_ms } else { None });
    let value =
        crate::process_supervisor::launch_detached(crate::process_supervisor::LaunchRequest {
            session_id: turn.session_id().as_str().to_string(),
            profile_id: turn.principal().profile_id().unwrap_or("user").to_string(),
            home_session: input.home_session.clone(),
            argv,
            cwd: input.cwd.as_deref().map(std::path::PathBuf::from),
            deadline: deadline.map(std::time::Duration::from_millis),
            background_after: if explicit {
                None
            } else {
                grace.map(std::time::Duration::from_millis)
            },
        })
        .await
        .map_err(StasisError::PortFailure)?;
    Ok(Some(value))
}

pub struct CognitionShellStatusTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellStatusInput {}

#[medousa_tool(id = COGNITION_SHELL_STATUS_ID)]
impl CognitionShellStatusTool {
    /// Probe Medousa OS-native shell sandbox readiness (Seatbelt / bubblewrap / systemd-run). Does not execute commands.
    async fn invoke_typed(
        &self,
        _input: ShellStatusInput,
    ) -> stasis::prelude::Result<ShellSandboxStatus> {
        ensure_shell_agent_tools_enabled()?;
        if let Some(invocation) =
            crate::work_environment_tools::EnvironmentToolInvocation::active(COGNITION_SHELL_STATUS)
        {
            let (ready, phase) = crate::work_environment_tools::status(&invocation).await?;
            return Ok(ShellSandboxStatus {
                os: "oci".to_string(),
                backend: "work_environment".to_string(),
                ready,
                sandboxed: true,
                detail: format!(
                    "daemon-owned environment {} is {phase:?}",
                    invocation.binding().handle.environment_id()
                ),
            });
        }
        let status = tokio::task::spawn_blocking(probe_shell_sandbox)
            .await
            .map_err(|err| StasisError::PortFailure(format!("shell status join error: {err}")))?;
        Ok(status)
    }
}

pub struct CognitionShellRunTool {}

impl CognitionShellRunTool {
    pub fn new(_runtime: Arc<RuntimeComposition>) -> Self {
        Self {}
    }
}

#[derive(Debug, JsonSchema)]
pub struct ShellRunInput {
    /// Shell command string (wrapped in sh -c / cmd /C)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    /// Explicit argv (preferred over command when possible)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    argv: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    writable_roots: Option<Vec<String>>,
    /// Allow network (default false)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    network: Option<bool>,
    /// Optional command execution deadline. Omitting it leaves lifetime to the owning turn; output observation yields independently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    allowed_binaries: Option<Vec<String>>,
    #[schemars(skip)]
    readonly_roots: Option<Vec<String>>,
    #[schemars(skip)]
    max_output_bytes: Option<u64>,
    /// Return a handle immediately and wake this session when the process exits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    background: Option<bool>,
    /// If the command is still running after this many milliseconds, detach it.
    /// Explicit timeout_ms keeps the foreground kill behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64", skip_serializing_if = "Option::is_none")]
    background_after_ms: Option<u64>,
    /// Stay in the foreground even when auto-background is configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    foreground: Option<bool>,
    /// Optional deadline for a detached process. Wakes the session with timed_out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64", skip_serializing_if = "Option::is_none")]
    deadline_ms: Option<u64>,
    /// Coordinator session that should receive a remote process wake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    home_session: Option<String>,
}

fn compatible_path_list(value: Option<&Value>) -> Option<Vec<String>> {
    let value = value?;
    if let Some(items) = value.as_array() {
        return Some(
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        );
    }
    value.as_str().map(|value| vec![value.to_string()])
}

impl<'de> Deserialize<'de> for ShellRunInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let string = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        let string_list = |key: &str| {
            value.get(key).and_then(Value::as_array).map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
        };
        Ok(Self {
            command: string("command").or_else(|| string("cmd")),
            argv: string_list("argv"),
            cwd: string("cwd"),
            writable_roots: compatible_path_list(
                value
                    .get("writable_roots")
                    .or_else(|| value.get("writable")),
            ),
            network: value.get("network").and_then(Value::as_bool),
            timeout_ms: value
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .or_else(|| value.get("timeout").and_then(Value::as_u64)),
            allowed_binaries: string_list("allowed_binaries").or_else(|| string_list("binaries")),
            readonly_roots: compatible_path_list(
                value
                    .get("readonly_roots")
                    .or_else(|| value.get("readonly")),
            ),
            max_output_bytes: value.get("max_output_bytes").and_then(Value::as_u64),
            background: value.get("background").and_then(Value::as_bool),
            background_after_ms: value.get("background_after_ms").and_then(Value::as_u64),
            foreground: value.get("foreground").and_then(Value::as_bool),
            deadline_ms: value.get("deadline_ms").and_then(Value::as_u64),
            home_session: string("home_session"),
        })
    }
}

impl ShellRunInput {
    fn into_shell_args(self) -> Value {
        let mut args = Map::new();
        if let Some(value) = self.command {
            args.insert("command".to_string(), Value::String(value));
        }
        if let Some(value) = self.argv {
            args.insert(
                "argv".to_string(),
                Value::Array(value.into_iter().map(Value::String).collect()),
            );
        }
        if let Some(value) = self.cwd {
            args.insert("cwd".to_string(), Value::String(value));
        }
        if let Some(value) = self.writable_roots {
            args.insert(
                "writable_roots".to_string(),
                Value::Array(value.into_iter().map(Value::String).collect()),
            );
        }
        if let Some(value) = self.network {
            args.insert("network".to_string(), Value::Bool(value));
        }
        if let Some(value) = self.timeout_ms {
            args.insert("timeout_ms".to_string(), Value::from(value));
        }
        if let Some(value) = self.allowed_binaries {
            args.insert(
                "allowed_binaries".to_string(),
                Value::Array(value.into_iter().map(Value::String).collect()),
            );
        }
        if let Some(value) = self.readonly_roots {
            args.insert(
                "readonly_roots".to_string(),
                Value::Array(value.into_iter().map(Value::String).collect()),
            );
        }
        if let Some(value) = self.max_output_bytes {
            args.insert("max_output_bytes".to_string(), Value::from(value));
        }
        Value::Object(args)
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ShellRunOutput {
    mode: String,
    source: String,
    runtime: ExternalJson,
    shell: ExternalJson,
}

#[medousa_tool(id = COGNITION_SHELL_RUN_ID)]
impl CognitionShellRunTool {
    /// Run a command in Medousa's OS-native sandbox and terminal. Prefer argv or a short command; network is denied by default. Power users can call shell.run directly inside Grapheme scripts.
    async fn invoke_typed(&self, input: ShellRunInput) -> stasis::prelude::Result<ShellRunOutput> {
        ensure_shell_agent_tools_enabled()?;
        if let Some(invocation) =
            crate::work_environment_tools::EnvironmentToolInvocation::active(COGNITION_SHELL_RUN)
        {
            if input.background == Some(true) || background_grace_ms(&input).is_some() {
                return Err(StasisError::PortFailure(
                    "background wakes are for the native shell. This bound work environment runs in the turn. Allowed action: omit background, or run the command on the native shell."
                        .into(),
                ));
            }
            if input.network == Some(true) {
                return Err(StasisError::PortFailure(
                    "the bound work environment denies network access".to_string(),
                ));
            }
            for root in input
                .writable_roots
                .iter()
                .flatten()
                .chain(input.readonly_roots.iter().flatten())
            {
                crate::work_environment_tools::workspace_directory(Some(root))?;
            }
            let (program, args) = match (input.argv.as_ref(), input.command.as_ref()) {
                (Some(argv), _) if !argv.is_empty() => (argv[0].clone(), argv[1..].to_vec()),
                (_, Some(command)) if !command.trim().is_empty() => (
                    "/bin/sh".to_string(),
                    vec!["-lc".to_string(), command.clone()],
                ),
                _ => {
                    return Err(StasisError::PortFailure(
                        "provide command or a non-empty argv".to_string(),
                    ));
                }
            };
            if let Some(allowed) = input.allowed_binaries.as_ref()
                && !allowed.is_empty()
            {
                let basename = std::path::Path::new(&program)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(program.as_str());
                if !allowed.iter().any(|allowed| allowed == basename) {
                    return Err(StasisError::PortFailure(format!(
                        "program is outside allowed_binaries: {basename}"
                    )));
                }
            }
            let result = crate::work_environment_tools::shell_exec(
                &invocation,
                program,
                args,
                input.cwd.as_deref(),
                None,
                input.timeout_ms,
                input.max_output_bytes.unwrap_or(256 * 1024),
            )
            .await?;
            let shell = json!({
                "execution_id": result.execution_id,
                "exit_code": result.exit_code,
                "stdout": result.stdout,
                "stderr": result.stderr,
                "succeeded": result.exit_code == Some(0),
                "output_truncated": result.output_truncated,
            });
            return Ok(ShellRunOutput {
                mode: "work_environment_exec".to_string(),
                source: String::new(),
                runtime: ExternalJson::new(json!({
                    "environment_id": invocation.binding().handle.environment_id(),
                    "fenced": true,
                })),
                shell: ExternalJson::new(shell),
            });
        }
        if let Some(shell) = maybe_background(&input).await? {
            return Ok(ShellRunOutput {
                mode: "background_process".to_string(),
                source: String::new(),
                runtime: ExternalJson::new(json!({
                    "transport": "supervised_process",
                    "wake": "process_completed",
                })),
                shell: ExternalJson::new(shell),
            });
        }
        let shell = crate::native_shell::run(input.into_shell_args()).await?;
        Ok(ShellRunOutput {
            mode: "native_shell_run".to_string(),
            source: String::new(),
            runtime: ExternalJson::new(
                json!({"transport": if shell.get("session_id").is_some() { "native_terminal" } else { "native_process" }}),
            ),
            shell: ExternalJson::new(shell),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_tools_gate_rejects_when_disabled() {
        let err = ensure_shell_agent_tools_enabled_flag(false).expect_err("denied");
        let message = err.to_string();
        assert!(
            message.contains("Settings → Shell"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn agent_tools_gate_allows_when_enabled() {
        ensure_shell_agent_tools_enabled_flag(true).expect("allowed");
    }
}
