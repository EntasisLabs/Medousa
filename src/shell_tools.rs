//! Cognition tools for Medousa OS-native shell (`shell.run` Grapheme module).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::shell_grapheme::synthesize_shell_run_source;
use crate::shell_sandbox::{ShellSandboxStatus, probe_shell_sandbox, shell_agent_tools_enabled};
use crate::tools::run_grapheme_via_runtime;
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
    Ok(())
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
        if let Some(invocation) = crate::work_environment_tools::EnvironmentToolInvocation::active(
            COGNITION_SHELL_STATUS,
        ) {
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

pub struct CognitionShellRunTool {
    runtime: Arc<RuntimeComposition>,
}

impl CognitionShellRunTool {
    pub fn new(runtime: Arc<RuntimeComposition>) -> Self {
        Self { runtime }
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
        })
    }
}

impl ShellRunInput {
    fn into_grapheme_args(self) -> Value {
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
    /// Run a command in Medousa's OS-native sandbox via the Grapheme shell.run module. Prefer argv or a short command; network is denied by default. Power users can call shell.run directly inside Grapheme scripts.
    async fn invoke_typed(&self, input: ShellRunInput) -> stasis::prelude::Result<ShellRunOutput> {
        ensure_shell_agent_tools_enabled()?;
        if let Some(invocation) = crate::work_environment_tools::EnvironmentToolInvocation::active(
            COGNITION_SHELL_RUN,
        ) {
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
                (Some(argv), _) if !argv.is_empty() => {
                    (argv[0].clone(), argv[1..].to_vec())
                }
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
                input.timeout_ms.unwrap_or(30_000),
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
        let args = input.into_grapheme_args();
        let source = synthesize_shell_run_source(&args).map_err(StasisError::PortFailure)?;
        let result = run_grapheme_via_runtime(&self.runtime, &source, COGNITION_SHELL_RUN).await?;

        // Surface shell.run fields when the grapheme diagnostics carry final_state.
        let shell = extract_shell_result(&result);
        Ok(ShellRunOutput {
            mode: "grapheme_shell_run".to_string(),
            source,
            runtime: ExternalJson::new(result),
            shell: ExternalJson::new(shell),
        })
    }
}

fn extract_shell_result(runtime_result: &Value) -> Value {
    let diagnostics = runtime_result
        .get("diagnostics")
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(final_state) = diagnostics.get("final_state") {
        return final_state.clone();
    }
    if let Some(execution) = diagnostics.get("execution") {
        return execution.clone();
    }
    diagnostics
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
