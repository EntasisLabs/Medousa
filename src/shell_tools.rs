//! Cognition tools for Medousa OS-native shell (`shell.run` Grapheme module).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};

use crate::shell_grapheme::synthesize_shell_run_source;
use crate::shell_sandbox::probe_shell_sandbox;
use crate::tools::run_grapheme_via_runtime;

pub const COGNITION_SHELL_STATUS: &str = "cognition_shell_status";
pub const COGNITION_SHELL_RUN: &str = "cognition_shell_run";

pub const SHELL_COGNITION_TOOLS: &[&str] = &[COGNITION_SHELL_STATUS, COGNITION_SHELL_RUN];

pub fn is_shell_cognition_tool(name: &str) -> bool {
    name.starts_with("cognition_shell_")
}

pub fn register_shell_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    runtime: Arc<RuntimeComposition>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionShellStatusTool)?;
    registry.register_tool(CognitionShellRunTool::new(runtime))?;
    Ok(())
}

pub struct CognitionShellStatusTool;

#[async_trait]
impl StasisTool for CognitionShellStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_STATUS
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Probe Medousa OS-native shell sandbox readiness (Seatbelt / bubblewrap / systemd-run). \
             Does not execute commands.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn invoke(&self, _input: Value) -> StasisResult<Value> {
        let status = tokio::task::spawn_blocking(probe_shell_sandbox)
            .await
            .map_err(|err| StasisError::PortFailure(format!("shell status join error: {err}")))?;
        Ok(serde_json::to_value(status).unwrap_or(Value::Null))
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

#[async_trait]
impl StasisTool for CognitionShellRunTool {
    fn name(&self) -> &'static str {
        COGNITION_SHELL_RUN
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Run a command in Medousa's OS-native sandbox via the Grapheme shell.run module. \
             Prefer argv or a short command; network is denied by default. \
             Power users can call shell.run directly inside Grapheme scripts.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command string (wrapped in sh -c / cmd /C)"
                },
                "argv": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Explicit argv (preferred over command when possible)"
                },
                "cwd": { "type": "string" },
                "writable_roots": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "network": {
                    "type": "boolean",
                    "description": "Allow network (default false)"
                },
                "timeout_ms": { "type": "integer" },
                "allowed_binaries": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let source = synthesize_shell_run_source(&input)
            .map_err(StasisError::PortFailure)?;
        let result = run_grapheme_via_runtime(&self.runtime, &source, COGNITION_SHELL_RUN).await?;

        // Surface shell.run fields when the grapheme diagnostics carry final_state.
        let shell = extract_shell_result(&result);
        Ok(json!({
            "mode": "grapheme_shell_run",
            "source": source,
            "runtime": result,
            "shell": shell,
        }))
    }
}

fn extract_shell_result(runtime_result: &Value) -> Value {
    let diagnostics = runtime_result.get("diagnostics").cloned().unwrap_or(Value::Null);
    if let Some(final_state) = diagnostics.get("final_state") {
        return final_state.clone();
    }
    if let Some(execution) = diagnostics.get("execution") {
        return execution.clone();
    }
    diagnostics
}
