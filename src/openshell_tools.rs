//! Cognition tools for OpenShell sandbox handoff (Sprint B5).

use std::sync::Arc;

use chrono::Utc;
use schemars::JsonSchema;
use schemars::schema::{ArrayValidation, InstanceType, Schema, SchemaObject, SingleOrVec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::identity_manuscript::build_manuscript_context;
use crate::openshell_handoff::collect_openshell_doctor_report;
use crate::openshell_sandbox_run::{OPENSHELL_SANDBOX_RUN_JOB_TYPE, OpenshellSandboxRunPayload};
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::runtime_job_spec::ToolJobSpec;
use crate::semantic_values::RequiredContent;
use crate::turn_continuation::{
    ContinuationAwaitMode, TurnContinuationScope, continuation_tool_metadata, wire_turn_child_job,
};
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

pub const COGNITION_OPENSHELL_STATUS: &str = "cognition_openshell_status";
pub const COGNITION_OPENSHELL_SANDBOX_RUN: &str = "cognition_openshell_sandbox_run";

const COGNITION_OPENSHELL_STATUS_ID: ToolId = ToolId::new(COGNITION_OPENSHELL_STATUS);
const COGNITION_OPENSHELL_SANDBOX_RUN_ID: ToolId = ToolId::new(COGNITION_OPENSHELL_SANDBOX_RUN);

pub const OPENSHELL_COGNITION_TOOLS: &[&str] =
    &[COGNITION_OPENSHELL_STATUS, COGNITION_OPENSHELL_SANDBOX_RUN];

pub fn is_openshell_cognition_tool(name: &str) -> bool {
    name.starts_with("cognition_openshell_")
}

pub fn register_openshell_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: Arc<RwLock<Option<TurnContinuationScope>>>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionOpenshellStatusTool)?;
    registry.register_typed_tool(CognitionOpenshellSandboxRunTool::new(
        runtime, event_tx, turn_scope,
    ))?;
    Ok(())
}

pub struct CognitionOpenshellStatusTool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenshellStatusInput {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct OpenshellStatusOutput {
    pub gateway_url: String,
    pub gateway_reachable: bool,
    pub readyz_ok: bool,
    pub cli_installed: bool,
    pub cli_version: Option<String>,
    pub gateway_binary: Option<String>,
    pub sandbox_binary: Option<String>,
    pub podman_socket: String,
    pub podman_socket_active: bool,
    pub active_gateway_name: Option<String>,
    pub policy_templates_dir: String,
    pub policy_template_count: usize,
}

#[medousa_tool(id = COGNITION_OPENSHELL_STATUS_ID)]
impl CognitionOpenshellStatusTool {
    /// Probe local OpenShell gateway health (TCP, /readyz, CLI binaries, Podman socket, policy templates). Read-only — does not create sandboxes.
    async fn invoke_typed(
        &self,
        _input: OpenshellStatusInput,
    ) -> stasis::prelude::Result<OpenshellStatusOutput> {
        let report = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| {
                StasisError::PortFailure(format!("openshell status join error: {err}"))
            })?;
        Ok(OpenshellStatusOutput {
            gateway_url: report.gateway_url,
            gateway_reachable: report.gateway_reachable,
            readyz_ok: report.readyz_ok,
            cli_installed: report.cli_installed,
            cli_version: report.cli_version,
            gateway_binary: report.gateway_binary.map(|path| path.display().to_string()),
            sandbox_binary: report.sandbox_binary.map(|path| path.display().to_string()),
            podman_socket: report.podman_socket.display().to_string(),
            podman_socket_active: report.podman_socket_active,
            active_gateway_name: report.active_gateway_name,
            policy_templates_dir: report.policy_templates_dir.display().to_string(),
            policy_template_count: report.policy_template_count,
        })
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

#[derive(Debug)]
pub enum OpenshellCommandInput {
    Text(String),
    Argv(Vec<String>),
    Invalid,
}

impl<'de> Deserialize<'de> for OpenshellCommandInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if let Some(text) = value.as_str() {
            return Ok(Self::Text(text.to_string()));
        }
        if let Some(items) = value.as_array() {
            return Ok(Self::Argv(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ));
        }
        Ok(Self::Invalid)
    }
}

impl JsonSchema for OpenshellCommandInput {
    fn schema_name() -> String {
        "OpenshellCommandInput".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        let string = Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            ..SchemaObject::default()
        });
        let array = Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::Array.into()),
            array: Some(Box::new(ArrayValidation {
                items: Some(SingleOrVec::Single(Box::new(Schema::Object(
                    SchemaObject {
                        instance_type: Some(InstanceType::String.into()),
                        ..SchemaObject::default()
                    },
                )))),
                ..ArrayValidation::default()
            })),
            ..SchemaObject::default()
        });
        Schema::Object(SchemaObject {
            subschemas: Some(Box::new(schemars::schema::SubschemaValidation {
                one_of: Some(vec![string, array]),
                ..schemars::schema::SubschemaValidation::default()
            })),
            ..SchemaObject::default()
        })
    }
}

fn default_destroy_on_complete() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenshellSandboxRunInput {
    /// Argv to run inside the sandbox (string or string array)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "OpenshellCommandInput",
        skip_serializing_if = "Option::is_none"
    )]
    command: Option<OpenshellCommandInput>,
    /// Optional manuscript for policy_template/sandbox_from defaults
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    manuscript_id: Option<String>,
    /// OpenShell --from source (default base or manuscript spec)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    sandbox_from: Option<String>,
    /// Policy template id under ~/.config/medousa/openshell-policies/
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    policy_template: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    workdir: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_u64"
    )]
    #[schemars(with = "i64", skip_serializing_if = "Option::is_none")]
    timeout_secs: Option<u64>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
    )]
    #[schemars(with = "bool", default = "default_destroy_on_complete")]
    destroy_on_complete: Option<bool>,
    /// Relative script path in imported skill assets (e.g. scripts/run.sh). Requires manuscript_id.
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    skill_script: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum OpenshellSandboxRunOutput {
    Enqueued {
        job_id: String,
        status: String,
        job_type: String,
        manuscript_id: Option<String>,
        policy_template: Option<String>,
        skill_script: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        continuation: Option<ExternalJson>,
    },
    Rejected {
        status: String,
        reason: String,
        policy_message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        gateway_url: Option<String>,
    },
}

#[medousa_tool(id = COGNITION_OPENSHELL_SANDBOX_RUN_ID)]
impl CognitionOpenshellSandboxRunTool {
    /// Enqueue a durable OpenShell sandbox job (create → upload skill assets → exec → destroy). Pass command OR skill_script+manuscript_id for imported skill assets. Requires gateway healthy and manuscript spec.openshell.enabled when manuscript_id is set. Worker lane primary; not available on scheduled lane unless spec.openshell.allow_scheduled=true.
    async fn invoke_typed(
        &self,
        input: OpenshellSandboxRunInput,
    ) -> stasis::prelude::Result<OpenshellSandboxRunOutput> {
        let manuscript_id = input
            .manuscript_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let (sandbox_from, policy_template) = if let Some(id) = manuscript_id.as_deref() {
            let manuscript = build_manuscript_context(id)
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            if !manuscript.openshell_enabled {
                return Ok(OpenshellSandboxRunOutput::Rejected {
                    status: "rejected".to_string(),
                    reason: "openshell_not_enabled".to_string(),
                    policy_message: format!(
                        "manuscript '{id}' does not have spec.openshell.enabled=true"
                    ),
                    gateway_url: None,
                });
            }
            (
                manuscript.openshell_sandbox_from.clone(),
                manuscript.openshell_policy_template.clone(),
            )
        } else {
            (None, None)
        };

        let sandbox_from = input
            .sandbox_from
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(sandbox_from);
        let policy_template = input
            .policy_template
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(policy_template);

        if policy_template.is_none() {
            return Ok(OpenshellSandboxRunOutput::Rejected {
                status: "rejected".to_string(),
                reason: "missing_policy_template".to_string(),
                policy_message: "policy_template is required (directly or via manuscript spec.openshell.policy_template)".to_string(),
                gateway_url: None,
            });
        }

        let template = policy_template.as_deref().unwrap_or_default();
        if crate::openshell_sandbox_run::resolve_policy_template_path(template).is_none() {
            return Ok(OpenshellSandboxRunOutput::Rejected {
                status: "rejected".to_string(),
                reason: "policy_template_missing".to_string(),
                policy_message: format!(
                    "policy template '{template}' not found under ~/.config/medousa/openshell-policies/"
                ),
                gateway_url: None,
            });
        }

        let report = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| {
                StasisError::PortFailure(format!("openshell preflight join error: {err}"))
            })?;
        if !report.readyz_ok {
            return Ok(OpenshellSandboxRunOutput::Rejected {
                status: "rejected".to_string(),
                reason: "gateway_unhealthy".to_string(),
                policy_message: format!(
                    "OpenShell gateway not ready at {} (run medousa doctor)",
                    report.gateway_url
                ),
                gateway_url: Some(report.gateway_url),
            });
        }

        let destroy_on_complete = input.destroy_on_complete.unwrap_or(true);
        let workdir = input
            .workdir
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let timeout_secs = input.timeout_secs;
        let correlation_id = input
            .correlation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let skill_script = input
            .skill_script
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let payload = if let (Some(manuscript_id), Some(script)) =
            (manuscript_id.as_deref(), skill_script.as_deref())
        {
            let manuscript = build_manuscript_context(manuscript_id)
                .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            let mut skill_payload = crate::skill_execution::build_sandbox_payload_for_skill(
                manuscript_id,
                script,
                &manuscript,
                correlation_id.clone(),
            )
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
            skill_payload.destroy_on_complete = destroy_on_complete;
            if let Some(secs) = timeout_secs {
                skill_payload.timeout_secs = Some(secs);
            }
            skill_payload
        } else {
            let command = parse_command_argv(input.command.as_ref())?;
            OpenshellSandboxRunPayload {
                command,
                sandbox_from,
                policy_template,
                destroy_on_complete,
                workdir,
                timeout_secs,
                manuscript_id: manuscript_id.clone(),
                correlation_id,
                skill_assets_dir: None,
                skill_upload_dest: None,
                skill_script: None,
            }
        };
        let payload_ref = payload.to_payload_ref()?;

        let job_id = format!("openshell-{}", Uuid::new_v4().simple());
        let now = Utc::now();
        let mut job = ToolJobSpec::new(
            job_id.clone(),
            "default",
            OPENSHELL_SANDBOX_RUN_JOB_TYPE,
            payload_ref,
            "cognition_openshell",
            "sttp:in:openshell:sandbox_run",
            now,
        )
        .build();

        if let Some(scope) = self.turn_scope.read().await.clone() {
            wire_turn_child_job(
                &mut job,
                &scope,
                COGNITION_OPENSHELL_SANDBOX_RUN,
                OPENSHELL_SANDBOX_RUN_JOB_TYPE,
                ContinuationAwaitMode::Async,
            )
            .await;
        }

        self.runtime.enqueue_job(job).await?;

        let _ = self
            .event_tx
            .send(TuiEvent::JobEnqueued {
                job_id: job_id.clone(),
                job_type: OPENSHELL_SANDBOX_RUN_JOB_TYPE.to_string(),
            })
            .await;

        let continuation = self.turn_scope.read().await.clone().map(|scope| {
            ExternalJson::new(continuation_tool_metadata(
                &scope,
                &job_id,
                ContinuationAwaitMode::Async,
            ))
        });
        Ok(OpenshellSandboxRunOutput::Enqueued {
            job_id,
            status: "enqueued".to_string(),
            job_type: OPENSHELL_SANDBOX_RUN_JOB_TYPE.to_string(),
            manuscript_id,
            policy_template: payload.policy_template,
            skill_script: payload.skill_script,
            continuation,
        })
    }
}

fn parse_command_argv(command: Option<&OpenshellCommandInput>) -> StasisResult<Vec<String>> {
    let command = command.ok_or_else(|| {
        StasisError::PortFailure("cognition_openshell_sandbox_run: command is required".to_string())
    })?;
    if let OpenshellCommandInput::Text(text) = command {
        let content = RequiredContent::new(text.clone()).map_err(|_| {
            StasisError::PortFailure(
                "cognition_openshell_sandbox_run: command must be non-empty".to_string(),
            )
        })?;
        return Ok(vec![
            "sh".to_string(),
            "-lc".to_string(),
            content.into_string(),
        ]);
    }
    if let OpenshellCommandInput::Argv(parts) = command {
        let argv: Vec<String> = parts
            .iter()
            .filter(|part| !part.trim().is_empty())
            .cloned()
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
        let command = OpenshellCommandInput::Text("echo hi".to_string());
        let argv = parse_command_argv(Some(&command)).expect("parse");
        assert_eq!(argv, vec!["sh", "-lc", "echo hi"]);
    }

    #[test]
    fn parse_string_command_preserves_surrounding_content() {
        let command = OpenshellCommandInput::Text("  echo hi  \n".to_string());
        let argv = parse_command_argv(Some(&command)).expect("parse");
        assert_eq!(argv[2], "  echo hi  \n");
    }

    #[test]
    fn parse_blank_string_command_rejects_whitespace_only() {
        let command = OpenshellCommandInput::Text(" \n\t".to_string());
        let error = parse_command_argv(Some(&command)).expect_err("blank command should fail");
        assert!(error.to_string().contains("command must be non-empty"));
    }

    #[test]
    fn parse_array_command() {
        let command = OpenshellCommandInput::Argv(vec!["echo".to_string(), "hi".to_string()]);
        let argv = parse_command_argv(Some(&command)).expect("parse");
        assert_eq!(argv, vec!["echo", "hi"]);
    }

    #[test]
    fn parse_array_command_preserves_valid_argument_bytes() {
        let command = OpenshellCommandInput::Argv(vec![
            " echo ".to_string(),
            "hi".to_string(),
            "   ".to_string(),
        ]);
        let argv = parse_command_argv(Some(&command)).expect("parse");
        assert_eq!(argv, vec![" echo ", "hi"]);
    }
}
