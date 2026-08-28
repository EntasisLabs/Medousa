//! Cognition tools for OpenShell sandbox handoff (Sprint B5).

use std::sync::Arc;

use chrono::Utc;
use schemars::JsonSchema;
use schemars::schema::{ArrayValidation, InstanceType, Schema, SchemaObject, SingleOrVec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::events::TuiEvent;
use crate::identity_manuscript::build_manuscript_context;
use crate::openshell_handoff::collect_openshell_doctor_report;
use crate::openshell_sandbox_run::{OPENSHELL_SANDBOX_RUN_JOB_TYPE, OpenshellSandboxRunPayload};
use crate::runtime_composition_ext::RuntimeCompositionExt;
use crate::runtime_job_spec::ToolJobSpec;
use crate::semantic_values::RequiredContent;
use crate::turn_continuation::{
    ContinuationAwaitMode, continuation_tool_metadata, wire_turn_child_job,
};
use crate::typed_tools::{CompatOption, ExternalJson, ToolId, medousa_tool};

pub const COGNITION_OPENSHELL_STATUS: &str = "cognition_openshell_status";
pub const COGNITION_OPENSHELL_REQUEST_SECRET: &str = "cognition_openshell_request_secret";
pub const COGNITION_OPENSHELL_SANDBOX_RUN: &str = "cognition_openshell_sandbox_run";

const COGNITION_OPENSHELL_STATUS_ID: ToolId = ToolId::new(COGNITION_OPENSHELL_STATUS);
const COGNITION_OPENSHELL_REQUEST_SECRET_ID: ToolId =
    ToolId::new(COGNITION_OPENSHELL_REQUEST_SECRET);
const COGNITION_OPENSHELL_SANDBOX_RUN_ID: ToolId = ToolId::new(COGNITION_OPENSHELL_SANDBOX_RUN);

pub const OPENSHELL_COGNITION_TOOLS: &[&str] = &[
    COGNITION_OPENSHELL_STATUS,
    COGNITION_OPENSHELL_REQUEST_SECRET,
    COGNITION_OPENSHELL_SANDBOX_RUN,
];

pub fn is_openshell_cognition_tool(name: &str) -> bool {
    crate::tool_names::is_openshell_cognition_tool(name)
}

pub fn register_openshell_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionOpenshellStatusTool)?;
    registry.register_typed_tool(CognitionOpenshellRequestSecretTool::new(turn_scope.clone()))?;
    registry.register_typed_tool(CognitionOpenshellSandboxRunTool::new(
        runtime, event_tx, turn_scope,
    ))?;
    Ok(())
}

async fn probe_openshell_providers_v2() -> Result<bool, String> {
    tokio::task::spawn_blocking(crate::openshell_sandbox_run::openshell_providers_v2_enabled)
        .await
        .map_err(|err| format!("OpenShell Providers v2 probe task failed: {err}"))?
}

async fn probe_openshell_provider_profile(
    provider_type: &str,
    credential_key: &str,
) -> Result<(), String> {
    let provider_type = provider_type.to_string();
    let credential_key = credential_key.to_string();
    tokio::task::spawn_blocking(move || {
        crate::openshell_sandbox_run::validate_openshell_provider_profile(
            &provider_type,
            &credential_key,
        )
    })
    .await
    .map_err(|err| format!("OpenShell provider profile probe task failed: {err}"))?
}

pub struct CognitionOpenshellRequestSecretTool {
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionOpenshellRequestSecretTool {
    pub fn new(turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess) -> Self {
        Self { turn_scope }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenshellRequestSecretInput {
    /// OpenShell provider profile id, such as `github` or `openai`.
    provider_type: String,
    /// Environment variable expected by that provider, such as `GITHUB_TOKEN`.
    credential_key: String,
    /// Short trusted-UI label describing the credential.
    label: String,
    /// Why this turn needs the credential and what it will do with it.
    reason: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OpenshellRequestSecretOutput {
    Granted {
        grant_id: String,
        provider_type: String,
        credential_key: String,
        usage_hint: String,
    },
    Denied {
        reason: String,
    },
    Rejected {
        reason: String,
        policy_message: String,
    },
}

#[medousa_tool(id = COGNITION_OPENSHELL_REQUEST_SECRET_ID)]
impl CognitionOpenshellRequestSecretTool {
    /// Request a credential through Medousa's secret-entry UI and return a one-use grant for `cognition_openshell_sandbox_run.secret_grant_ids`.
    async fn invoke_typed(
        &self,
        input: OpenshellRequestSecretInput,
    ) -> stasis::prelude::Result<OpenshellRequestSecretOutput> {
        let Some(scope) =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await
        else {
            return Ok(OpenshellRequestSecretOutput::Rejected {
                reason: "missing_turn_context".to_string(),
                policy_message: "secure credential requests require an active interactive turn"
                    .to_string(),
            });
        };
        if !scope.supports_ui_artifacts {
            return Ok(OpenshellRequestSecretOutput::Rejected {
                reason: "unsupported_surface".to_string(),
                policy_message:
                    "secure credential entry is available only on a trusted Medousa UI surface"
                        .to_string(),
            });
        }
        let Some(sink) = crate::engine_adapters::active_tool_sink().await else {
            return Ok(OpenshellRequestSecretOutput::Rejected {
                reason: "missing_trusted_prompt_channel".to_string(),
                policy_message: "the active turn cannot publish a trusted credential prompt"
                    .to_string(),
            });
        };

        let report = tokio::task::spawn_blocking(collect_openshell_doctor_report)
            .await
            .map_err(|err| {
                StasisError::PortFailure(format!("openshell preflight join error: {err}"))
            })?;
        if !report.readyz_ok || !report.cli_installed {
            return Ok(OpenshellRequestSecretOutput::Rejected {
                reason: "openshell_unavailable".to_string(),
                policy_message: format!(
                    "OpenShell CLI and gateway must be ready at {} before storing a credential",
                    report.gateway_url
                ),
            });
        }
        match probe_openshell_providers_v2().await {
            Ok(true) => {}
            Ok(false) => {
                return Ok(OpenshellRequestSecretOutput::Rejected {
                    reason: "providers_v2_disabled".to_string(),
                    policy_message: "secure credential handoff requires OpenShell Providers v2; enable providers_v2_enabled on the workshop gateway".to_string(),
                });
            }
            Err(error) => {
                return Ok(OpenshellRequestSecretOutput::Rejected {
                    reason: "providers_v2_unverified".to_string(),
                    policy_message: error,
                });
            }
        }
        if let Err(error) =
            probe_openshell_provider_profile(&input.provider_type, &input.credential_key).await
        {
            return Ok(OpenshellRequestSecretOutput::Rejected {
                reason: "invalid_provider_profile".to_string(),
                policy_message: error,
            });
        }

        let record = crate::agent_secret_request::agent_secret_request_store()
            .create(crate::agent_secret_request::CreateAgentSecretRequest {
                turn_id: scope.turn_correlation_id,
                session_id: scope.session_id,
                provider_type: input.provider_type.clone(),
                credential_key: input.credential_key.clone(),
                backend: medousa_types::AgentSecretRequestBackend::OpenshellProvider,
                allowed_hosts: Vec::new(),
                label: input.label.clone(),
                reason: input.reason.clone(),
            })
            .map_err(StasisError::PortFailure)?;

        sink.emit(medousa_engine::ToolSinkEvent::SecretRequest {
            request_id: record.request_id.clone(),
            label: record.label,
            reason: record.reason,
            provider_type: record.provider_type.clone(),
            credential_key: record.credential_key.clone(),
            backend: "openshell_provider".to_string(),
            allowed_hosts: record.allowed_hosts,
        })
        .await;

        match crate::agent_secret_request::agent_secret_request_store()
            .wait_for_resolution(&record.request_id)
            .await
            .map_err(StasisError::PortFailure)?
        {
            crate::agent_secret_request::SecretRequestResolution::Granted { grant_id } => {
                Ok(OpenshellRequestSecretOutput::Granted {
                    grant_id,
                    provider_type: record.provider_type,
                    credential_key: record.credential_key,
                    usage_hint: "Pass this grant_id once in cognition_openshell_sandbox_run.secret_grant_ids. Do not print it or ask for the credential value."
                        .to_string(),
                })
            }
            crate::agent_secret_request::SecretRequestResolution::Denied => {
                Ok(OpenshellRequestSecretOutput::Denied {
                    reason: "the user denied or did not complete secure credential entry"
                        .to_string(),
                })
            }
        }
    }
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
    pub providers_v2_enabled: Option<bool>,
}

#[medousa_tool(id = COGNITION_OPENSHELL_STATUS_ID)]
impl CognitionOpenshellStatusTool {
    /// Report local OpenShell gateway health, including TCP, /readyz, CLI binaries, Podman socket, and templates.
    async fn invoke_typed(
        &self,
        _input: OpenshellStatusInput,
    ) -> stasis::prelude::Result<OpenshellStatusOutput> {
        let (report, providers_v2_enabled) = tokio::task::spawn_blocking(|| {
            let report = collect_openshell_doctor_report();
            let providers_v2_enabled = if report.readyz_ok && report.cli_installed {
                crate::openshell_sandbox_run::openshell_providers_v2_enabled().ok()
            } else {
                None
            };
            (report, providers_v2_enabled)
        })
        .await
        .map_err(|err| StasisError::PortFailure(format!("openshell status join error: {err}")))?;
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
            providers_v2_enabled,
        })
    }
}

pub struct CognitionOpenshellSandboxRunTool {
    runtime: Arc<RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
}

impl CognitionOpenshellSandboxRunTool {
    pub fn new(
        runtime: Arc<RuntimeComposition>,
        event_tx: mpsc::Sender<TuiEvent>,
        turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    manuscript_id: CompatOption<String>,
    /// OpenShell --from source (default base or manuscript spec)
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    sandbox_from: CompatOption<String>,
    /// Policy template id under ~/.config/medousa/openshell-policies/
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    policy_template: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    workdir: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "i64",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    timeout_secs: CompatOption<u64>,
    #[serde(default)]
    #[schemars(with = "bool", default = "default_destroy_on_complete")]
    destroy_on_complete: CompatOption<bool>,
    /// Relative script path in imported skill assets (e.g. scripts/run.sh). Requires manuscript_id.
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    skill_script: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    correlation_id: CompatOption<String>,
    /// Opaque one-use grants returned by cognition_openshell_request_secret.
    #[serde(default)]
    secret_grant_ids: Vec<String>,
}

#[derive(Debug)]
struct OpenshellSandboxRunCommand {
    command: Option<OpenshellCommandInput>,
    manuscript_id: Option<String>,
    sandbox_from: Option<String>,
    policy_template: Option<String>,
    workdir: Option<String>,
    timeout_secs: Option<u64>,
    destroy_on_complete: bool,
    skill_script: Option<String>,
    correlation_id: Option<String>,
    secret_grant_ids: Vec<String>,
}

fn optional_openshell_text(value: CompatOption<String>) -> Option<String> {
    value.into_option().and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

impl TryFrom<OpenshellSandboxRunInput> for OpenshellSandboxRunCommand {
    type Error = StasisError;

    fn try_from(input: OpenshellSandboxRunInput) -> Result<Self, Self::Error> {
        Ok(Self {
            command: input.command,
            manuscript_id: optional_openshell_text(input.manuscript_id),
            sandbox_from: optional_openshell_text(input.sandbox_from),
            policy_template: optional_openshell_text(input.policy_template),
            workdir: optional_openshell_text(input.workdir),
            timeout_secs: input.timeout_secs.into_option(),
            destroy_on_complete: input
                .destroy_on_complete
                .into_option()
                .unwrap_or_else(default_destroy_on_complete),
            skill_script: optional_openshell_text(input.skill_script),
            correlation_id: optional_openshell_text(input.correlation_id),
            secret_grant_ids: input.secret_grant_ids,
        })
    }
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
    /// Run a command or imported skill script in a disposable OpenShell sandbox. Pass command or skill_script with manuscript_id.
    async fn invoke_typed(
        &self,
        input: OpenshellSandboxRunInput,
    ) -> stasis::prelude::Result<OpenshellSandboxRunOutput> {
        let command = OpenshellSandboxRunCommand::try_from(input)?;
        let manuscript_id = command.manuscript_id.clone();

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

        let sandbox_from = command.sandbox_from.clone().or(sandbox_from);
        let policy_template = command.policy_template.clone().or(policy_template);

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
        if !command.secret_grant_ids.is_empty() {
            match probe_openshell_providers_v2().await {
                Ok(true) => {}
                Ok(false) => {
                    return Ok(OpenshellSandboxRunOutput::Rejected {
                        status: "rejected".to_string(),
                        reason: "providers_v2_disabled".to_string(),
                        policy_message:
                            "secret grants require OpenShell Providers v2 on the workshop gateway"
                                .to_string(),
                        gateway_url: Some(report.gateway_url),
                    });
                }
                Err(error) => {
                    return Ok(OpenshellSandboxRunOutput::Rejected {
                        status: "rejected".to_string(),
                        reason: "providers_v2_unverified".to_string(),
                        policy_message: error,
                        gateway_url: Some(report.gateway_url),
                    });
                }
            }
        }

        let destroy_on_complete = command.destroy_on_complete;
        let workdir = command.workdir.clone();
        let timeout_secs = command.timeout_secs;
        let correlation_id = command.correlation_id.clone();
        let skill_script = command.skill_script.clone();

        let mut payload = if let (Some(manuscript_id), Some(script)) =
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
            let input_command = parse_command_argv(command.command.as_ref())?;
            OpenshellSandboxRunPayload {
                command: input_command,
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
                providers: Vec::new(),
            }
        };
        let scope =
            crate::agent_runtime::execution_context::turn_continuation_scope(&self.turn_scope)
                .await;
        if !command.secret_grant_ids.is_empty() {
            let Some(scope) = scope.as_ref() else {
                return Ok(OpenshellSandboxRunOutput::Rejected {
                    status: "rejected".to_string(),
                    reason: "missing_turn_context".to_string(),
                    policy_message: "secret grants may only be used from their active chat session"
                        .to_string(),
                    gateway_url: None,
                });
            };
            payload.providers = crate::agent_secret_request::agent_secret_request_store()
                .consume_openshell_grants(&command.secret_grant_ids, &scope.session_id)
                .map_err(StasisError::PortFailure)?;
        }
        let payload_ref = payload.to_payload_ref()?;

        let job_id = format!("openshell-{}", Uuid::new_v4().simple());
        let now = Utc::now();
        let mut job = ToolJobSpec::new(
            job_id.clone(),
            "default",
            OPENSHELL_SANDBOX_RUN_JOB_TYPE,
            payload_ref,
            "cognition_openshell",
            now,
        )
        .build();

        if let Some(scope) = scope.as_ref() {
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

        let continuation = scope.map(|scope| {
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
        assert!(is_openshell_cognition_tool(
            COGNITION_OPENSHELL_REQUEST_SECRET
        ));
        assert!(is_openshell_cognition_tool(COGNITION_OPENSHELL_SANDBOX_RUN));
        assert!(!is_openshell_cognition_tool("cognition_memory_query"));
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

    #[test]
    fn sandbox_run_command_normalizes_optional_inputs_once() {
        let input: OpenshellSandboxRunInput = serde_json::from_value(serde_json::json!({
            "command": "echo hi",
            "manuscript_id": " specialist ",
            "sandbox_from": 42,
            "policy_template": " policy-a ",
            "workdir": " /workspace ",
            "timeout_secs": "120",
            "destroy_on_complete": "true",
            "skill_script": " scripts/run.sh ",
            "correlation_id": " correlation-1 ",
            "secret_grant_ids": ["sgrant-one"],
        }))
        .expect("sandbox input");
        let command = OpenshellSandboxRunCommand::try_from(input).expect("sandbox command");

        assert_eq!(command.manuscript_id.as_deref(), Some("specialist"));
        assert_eq!(command.sandbox_from, None);
        assert_eq!(command.policy_template.as_deref(), Some("policy-a"));
        assert_eq!(command.workdir.as_deref(), Some("/workspace"));
        assert_eq!(command.timeout_secs, None);
        assert!(command.destroy_on_complete);
        assert_eq!(command.skill_script.as_deref(), Some("scripts/run.sh"));
        assert_eq!(command.correlation_id.as_deref(), Some("correlation-1"));
        assert_eq!(command.secret_grant_ids, vec!["sgrant-one"]);
    }
}
