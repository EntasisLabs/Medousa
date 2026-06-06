//! Cognition tools for OpenShell sandbox handoff (Sprint B5).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::runtime::job::{BackoffPolicy, NewJob};
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::identity_manuscript::build_manuscript_context;
use crate::openshell_handoff::collect_openshell_doctor_report;
use crate::openshell_sandbox_run::{
    OpenshellSandboxRunPayload, OPENSHELL_SANDBOX_RUN_JOB_TYPE,
};
use crate::turn_continuation::{
    ContinuationAwaitMode, TurnContinuationScope, continuation_tool_metadata, wire_turn_child_job,
};

pub const COGNITION_OPENSHELL_STATUS: &str = "cognition_openshell_status";
pub const COGNITION_OPENSHELL_SANDBOX_RUN: &str = "cognition_openshell_sandbox_run";

pub const OPENSHELL_COGNITION_TOOLS: &[&str] = &[
    COGNITION_OPENSHELL_STATUS,
    COGNITION_OPENSHELL_SANDBOX_RUN,
];

pub fn is_openshell_cognition_tool(name: &str) -> bool {
    name.starts_with("cognition_openshell_")
}

pub fn register_openshell_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_tool(CognitionOpenshellStatusTool)?;
    registry.register_tool(CognitionOpenshellSandboxRunTool::new(
        runtime,
        event_tx,
        turn_scope,
    ))?;
    Ok(())
}

pub struct CognitionOpenshellStatusTool;

#[async_trait]
impl StasisTool for CognitionOpenshellStatusTool {
    fn name(&self) -> &'static str {
        COGNITION_OPENSHELL_STATUS
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Probe local OpenShell gateway health (TCP, /readyz, CLI binaries, Podman socket, policy templates). \
             Read-only — does not create sandboxes.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {}
        }))
    }

    async fn invoke(&self, _input: Value) -> StasisResult<Value> {
        let report = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| StasisError::PortFailure(format!("openshell status join error: {err}")))?;
        Ok(json!({
            "gateway_url": report.gateway_url,
            "gateway_reachable": report.gateway_reachable,
            "readyz_ok": report.readyz_ok,
            "cli_installed": report.cli_installed,
            "cli_version": report.cli_version,
            "gateway_binary": report.gateway_binary.map(|path| path.display().to_string()),
            "sandbox_binary": report.sandbox_binary.map(|path| path.display().to_string()),
            "podman_socket": report.podman_socket.display().to_string(),
            "podman_socket_active": report.podman_socket_active,
            "active_gateway_name": report.active_gateway_name,
            "policy_templates_dir": report.policy_templates_dir.display().to_string(),
            "policy_template_count": report.policy_template_count,
        }))
    }
}

pub struct CognitionOpenshellSandboxRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
}

impl CognitionOpenshellSandboxRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
    ) -> Self {
        Self {
            runtime,
            event_tx,
            turn_scope,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionOpenshellSandboxRunTool {
    fn name(&self) -> &'static str {
        COGNITION_OPENSHELL_SANDBOX_RUN
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Enqueue a durable OpenShell sandbox job (create → exec → destroy). \
             Requires gateway healthy and manuscript spec.openshell.enabled when manuscript_id is set. \
             Worker lane primary; not available on scheduled lane unless spec.openshell.allow_scheduled=true.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "command": {
                    "description": "Argv to run inside the sandbox (string or string array)",
                    "oneOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                },
                "manuscript_id": {
                    "type": "string",
                    "description": "Optional manuscript for policy_template/sandbox_from defaults"
                },
                "sandbox_from": {
                    "type": "string",
                    "description": "OpenShell --from source (default base or manuscript spec)"
                },
                "policy_template": {
                    "type": "string",
                    "description": "Policy template id under ~/.config/medousa/openshell-policies/"
                },
                "workdir": { "type": "string" },
                "timeout_secs": { "type": "integer" },
                "destroy_on_complete": { "type": "boolean", "default": true },
                "correlation_id": { "type": "string" }
            },
            "required": ["command"]
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let command = parse_command_argv(&input)?;
        let manuscript_id = input
            .get("manuscript_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let (sandbox_from, policy_template) = if let Some(id) = manuscript_id.as_deref() {
            let manuscript = build_manuscript_context(id)
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            if !manuscript.openshell_enabled {
                return Ok(json!({
                    "status": "rejected",
                    "reason": "openshell_not_enabled",
                    "policy_message": format!(
                        "manuscript '{id}' does not have spec.openshell.enabled=true"
                    ),
                }));
            }
            (
                manuscript.openshell_sandbox_from.clone(),
                manuscript.openshell_policy_template.clone(),
            )
        } else {
            (None, None)
        };

        let sandbox_from = input
            .get("sandbox_from")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(sandbox_from);
        let policy_template = input
            .get("policy_template")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(policy_template);

        if policy_template.is_none() {
            return Ok(json!({
                "status": "rejected",
                "reason": "missing_policy_template",
                "policy_message": "policy_template is required (directly or via manuscript spec.openshell.policy_template)",
            }));
        }

        let template = policy_template.as_deref().unwrap_or_default();
        if crate::openshell_sandbox_run::resolve_policy_template_path(template).is_none() {
            return Ok(json!({
                "status": "rejected",
                "reason": "policy_template_missing",
                "policy_message": format!(
                    "policy template '{template}' not found under ~/.config/medousa/openshell-policies/"
                ),
            }));
        }

        let report = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| StasisError::PortFailure(format!("openshell preflight join error: {err}")))?;
        if !report.readyz_ok {
            return Ok(json!({
                "status": "rejected",
                "reason": "gateway_unhealthy",
                "policy_message": format!(
                    "OpenShell gateway not ready at {} (run medousa doctor)",
                    report.gateway_url
                ),
                "gateway_url": report.gateway_url,
            }));
        }

        let destroy_on_complete = input
            .get("destroy_on_complete")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        let workdir = input
            .get("workdir")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|value| value.as_u64());
        let correlation_id = input
            .get("correlation_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let payload = OpenshellSandboxRunPayload {
            command,
            sandbox_from,
            policy_template,
            destroy_on_complete,
            workdir,
            timeout_secs,
            manuscript_id: manuscript_id.clone(),
            correlation_id,
        };
        let payload_ref = payload.to_payload_ref()?;

        let job_id = format!("openshell-{}", Uuid::new_v4().simple());
        let now = Utc::now();
        let mut job = NewJob {
            id: job_id.clone(),
            queue: "default".to_string(),
            job_type: OPENSHELL_SANDBOX_RUN_JOB_TYPE.to_string(),
            payload_ref,
            priority: 100,
            max_attempts: 1,
            idempotency_key: format!("idem-{job_id}"),
            correlation_id: job_id.clone(),
            causation_id: "cognition_openshell".to_string(),
            trace_id: job_id.clone(),
            sttp_input_node_id: "sttp:in:openshell:sandbox_run".to_string(),
            scheduled_at: now,
            backoff_policy: BackoffPolicy::default(),
        };

        if let Some(scope) = self.turn_scope.read().await.clone() {
            wire_turn_child_job(
                &mut job,
                &scope,
                self.name(),
                OPENSHELL_SANDBOX_RUN_JOB_TYPE,
                ContinuationAwaitMode::Async,
            )
            .await;
        }

        match &*self.runtime {
            RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
            RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
        }

        let _ = self
            .event_tx
            .send(TuiEvent::JobEnqueued {
                job_id: job_id.clone(),
                job_type: OPENSHELL_SANDBOX_RUN_JOB_TYPE.to_string(),
            })
            .await;

        let mut response = json!({
            "job_id": job_id,
            "status": "enqueued",
            "job_type": OPENSHELL_SANDBOX_RUN_JOB_TYPE,
            "manuscript_id": manuscript_id,
            "policy_template": payload.policy_template,
        });
        if let Some(scope) = self.turn_scope.read().await.clone() {
            if let Some(obj) = response.as_object_mut() {
                obj.insert(
                    "continuation".to_string(),
                    continuation_tool_metadata(
                        &scope,
                        &job_id,
                        ContinuationAwaitMode::Async,
                    ),
                );
            }
        }
        Ok(response)
    }
}

fn parse_command_argv(input: &Value) -> StasisResult<Vec<String>> {
    let command_value = input.get("command").ok_or_else(|| {
        StasisError::PortFailure(
            "cognition_openshell_sandbox_run: command is required".to_string(),
        )
    })?;
    if let Some(text) = command_value.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(StasisError::PortFailure(
                "cognition_openshell_sandbox_run: command must be non-empty".to_string(),
            ));
        }
        return Ok(vec!["sh".to_string(), "-lc".to_string(), trimmed.to_string()]);
    }
    if let Some(parts) = command_value.as_array() {
        let argv: Vec<String> = parts
            .iter()
            .filter_map(|value| value.as_str().map(str::trim).filter(|part| !part.is_empty()))
            .map(str::to_string)
            .collect();
        if argv.is_empty() {
            return Err(StasisError::PortFailure(
                "cognition_openshell_sandbox_run: command array must be non-empty".to_string(),
            ));
        }
        return Ok(argv);
    }
    Err(StasisError::PortFailure(
        "cognition_openshell_sandbox_run: command must be a string or string array".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openshell_tool_names_are_stable() {
        assert!(is_openshell_cognition_tool(COGNITION_OPENSHELL_STATUS));
        assert!(is_openshell_cognition_tool(COGNITION_OPENSHELL_SANDBOX_RUN));
        assert!(!is_openshell_cognition_tool("cognition_memory_recall"));
    }

    #[test]
    fn parse_string_command_wraps_shell() {
        let argv = parse_command_argv(&json!({ "command": "echo hi" })).expect("parse");
        assert_eq!(argv, vec!["sh", "-lc", "echo hi"]);
    }

    #[test]
    fn parse_array_command() {
        let argv = parse_command_argv(&json!({ "command": ["echo", "hi"] })).expect("parse");
        assert_eq!(argv, vec!["echo", "hi"]);
    }
}
