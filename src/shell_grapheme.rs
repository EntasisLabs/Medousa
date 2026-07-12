//! Grapheme `shell.run` MirV1 host module — OS-native sandboxed command execution.

use grapheme_runtime::host::{CapabilityCall, HostCallError};
use grapheme_runtime::{EffectKind, ExportedOp, ModuleAbi, ModuleManifest, ResourceLimits};
use serde_json::Value;

use crate::shell_sandbox::{ShellRunRequest, probe_shell_sandbox, run_sandboxed};

pub const SHELL_MODULE: &str = "shell";

pub fn register_shell_host_module(registry: &mut grapheme_runtime::ModuleRegistry) {
    registry.register_host_module(ModuleManifest {
        module_id: SHELL_MODULE.to_string(),
        version: "0.1.0".to_string(),
        abi: ModuleAbi::MirV1,
        entrypoint: "shell.host".to_string(),
        exported_ops: vec![
            ExportedOp {
                op: "run".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Io,
            },
            ExportedOp {
                op: "status".to_string(),
                input_schema_ref: None,
                output_schema_ref: None,
                effect: EffectKind::Pure,
            },
        ],
        // Enforced in-handler via ShellPermissionProfile (network/writable roots).
        required_capabilities: vec![],
        limits: ResourceLimits {
            max_cpu_ms: 60_000,
            max_memory_mb: 512,
            max_io_bytes: 32 * 1024 * 1024,
            max_network_calls: 0,
        },
    });
}

/// Capability interceptor fragment for `shell.*` host calls.
pub fn intercept_shell_call(call: &CapabilityCall) -> Option<Result<Value, HostCallError>> {
    if !is_shell_call(call) {
        return None;
    }
    let op = resolve_shell_op(call);
    match op.as_str() {
        "run" => Some(handle_shell_run(&call.args)),
        "status" => Some(Ok(serde_json::to_value(probe_shell_sandbox()).unwrap_or(Value::Null))),
        other => Some(Err(HostCallError::Fatal(format!(
            "unsupported shell op '{other}' (expected run or status)"
        )))),
    }
}

fn is_shell_call(call: &CapabilityCall) -> bool {
    let module = call
        .module
        .as_deref()
        .or_else(|| call.capability.split('.').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    module == SHELL_MODULE
}

fn resolve_shell_op(call: &CapabilityCall) -> String {
    let raw = if call.op.contains('.') {
        call.op.rsplit('.').next().unwrap_or(&call.op)
    } else if call.capability.contains('.') {
        call.capability.rsplit('.').next().unwrap_or(&call.op)
    } else {
        call.op.as_str()
    };
    raw.trim().to_ascii_lowercase()
}

fn handle_shell_run(args: &Value) -> Result<Value, HostCallError> {
    let request = ShellRunRequest::from_args(args).map_err(HostCallError::Fatal)?;
    let result = run_sandboxed(&request).map_err(HostCallError::Fatal)?;
    Ok(result.to_json())
}

/// Build a tiny Grapheme query that invokes `shell.run` (used by cognition_shell_run).
pub fn synthesize_shell_run_source(args: &Value) -> Result<String, String> {
    let request = ShellRunRequest::from_args(args)?;
    let command_or_argv = if let Some(command) = args
        .get("command")
        .or_else(|| args.get("cmd"))
        .and_then(Value::as_str)
    {
        format!("command: \"{}\"", escape_grapheme_literal(command))
    } else {
        let items = request
            .argv
            .iter()
            .map(|part| format!("\"{}\"", escape_grapheme_literal(part)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("argv: [{items}]")
    };

    let cwd = request
        .profile
        .cwd
        .as_ref()
        .map(|path| {
            format!(
                ",\n    cwd: \"{}\"",
                escape_grapheme_literal(&path.display().to_string())
            )
        })
        .unwrap_or_default();

    let writable = if request.profile.writable_roots.is_empty() {
        String::new()
    } else {
        let items = request
            .profile
            .writable_roots
            .iter()
            .map(|path| format!("\"{}\"", escape_grapheme_literal(&path.display().to_string())))
            .collect::<Vec<_>>()
            .join(", ");
        format!(",\n    writable_roots: [{items}]")
    };

    Ok(format!(
        r#"import core from "grapheme/core"
query ShellRun {{
  shell.run(
    {command_or_argv}{cwd}{writable},
    network: {},
    timeout_ms: {}
  ) {{ exit_code stdout stderr backend sandboxed timed_out duration_ms warning }}
}}"#,
        if request.profile.network {
            "true"
        } else {
            "false"
        },
        request.profile.timeout_ms,
    ))
}

fn escape_grapheme_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn synthesizes_command_query() {
        let source = synthesize_shell_run_source(&json!({
            "command": "echo hi",
            "network": false,
        }))
        .expect("synth");
        assert!(source.contains("shell.run("));
        assert!(source.contains("command: \"echo hi\""));
        assert!(source.contains("network: false"));
    }

    #[test]
    fn intercept_run_echo() {
        let call = CapabilityCall {
            module: Some("shell".to_string()),
            op: "run".to_string(),
            capability: "shell.run".to_string(),
            arg_count: 1,
            args: json!({ "command": "echo shell-module-ok" }),
            step_index: 0,
        };
        let result = intercept_shell_call(&call)
            .expect("intercepted")
            .expect("ok");
        assert!(
            result
                .get("stdout")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains("shell-module-ok")
                || result.get("exit_code").and_then(Value::as_i64) == Some(0),
            "{result}"
        );
    }
}
