//! Portable, destination-executed Coder work.
//!
//! The serialized task contains semantic intent and immutable identities only.
//! Source paths, credentials, process handles, and container ids remain behind
//! their owning daemon adapters. The destination revalidates its own grant at
//! every tool boundary and Stasis persists the result before checkpointing.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use medousa_forge::model::{ChangeStatus, ChangedFile, WorkPolicy};
use medousa_runtime::{
    WorkEnvironmentBinding, WorkEnvironmentError, WorkEnvironmentNetworkPolicy, WorkEnvironmentSpec,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use crate::peer_execution_policy::{
    PeerNetworkPolicy, TASK_EXECUTION_GRANT_SCHEMA_VERSION, TaskExecutionGrant,
};

pub const PORTABLE_CODER_TASK_SCHEMA_VERSION: u32 = 1;
pub const PORTABLE_CODER_RESULT_SCHEMA_VERSION: u32 = 1;
const MAX_PORTABLE_PROMPT_BYTES: usize = 256 * 1024;
const MAX_PORTABLE_TOOL_ROUNDS: usize = 100;
const MAX_PORTABLE_CHANGED_FILES: usize = 2_048;
const MAX_PORTABLE_SCANNED_FILE_BYTES: u64 = 1024 * 1024;
const MAX_PORTABLE_SCANNED_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENVIRONMENT_COMMAND_OUTPUT_BYTES: u64 = 1024 * 1024;

fn default_response_depth_mode() -> String {
    "standard".to_string()
}

fn default_max_tool_rounds() -> usize {
    24
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableCoderTask {
    pub schema_version: u32,
    /// Replay-stable identity for this exact handoff or retry.
    pub operation_id: String,
    /// Forge undertaking identity on the origin daemon.
    pub work_id: String,
    pub parent_session_id: String,
    pub correlation_id: String,
    pub project_id: String,
    /// Opaque policy identity for `/workspace`; never a host path.
    pub root_ref: String,
    pub expected_base_oid: String,
    pub expected_checkpoint_oid: String,
    pub prompt: String,
    pub provider: String,
    pub model: String,
    #[serde(default = "default_response_depth_mode")]
    pub response_depth_mode: String,
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: usize,
    #[serde(default)]
    pub work_policy: WorkPolicy,
    pub requested_tool_names: Vec<String>,
    pub requested_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
    /// Added only by the authenticated destination after its policy admits the
    /// source-signed request. The source cannot manufacture this authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_execution_grant: Option<TaskExecutionGrant>,
}

impl PortableCoderTask {
    pub fn validate(
        &self,
        spec: &WorkEnvironmentSpec,
        now: DateTime<Utc>,
        require_destination_grant: bool,
    ) -> Result<(), WorkEnvironmentError> {
        if self.schema_version != PORTABLE_CODER_TASK_SCHEMA_VERSION {
            return invalid(format!(
                "unsupported portable Coder task schema_version={}",
                self.schema_version
            ));
        }
        for (label, value) in [
            ("operation_id", self.operation_id.as_str()),
            ("work_id", self.work_id.as_str()),
            ("parent_session_id", self.parent_session_id.as_str()),
            ("correlation_id", self.correlation_id.as_str()),
            ("project_id", self.project_id.as_str()),
            ("root_ref", self.root_ref.as_str()),
            ("provider", self.provider.as_str()),
            ("model", self.model.as_str()),
            ("response_depth_mode", self.response_depth_mode.as_str()),
        ] {
            validate_text_identity(label, value)?;
        }
        if self.root_ref.starts_with(['/', '\\'])
            || self.root_ref.contains("/../")
            || self.root_ref.contains("\\..\\")
        {
            return invalid("portable root_ref must be an opaque logical reference");
        }
        validate_git_oid("expected_base_oid", &self.expected_base_oid)?;
        validate_git_oid("expected_checkpoint_oid", &self.expected_checkpoint_oid)?;
        if self.prompt.trim().is_empty() || self.prompt.len() > MAX_PORTABLE_PROMPT_BYTES {
            return invalid(format!(
                "portable Coder prompt must be between one and {MAX_PORTABLE_PROMPT_BYTES} bytes"
            ));
        }
        if !(1..=MAX_PORTABLE_TOOL_ROUNDS).contains(&self.max_tool_rounds) {
            return invalid(format!(
                "portable Coder max_tool_rounds must be between 1 and {MAX_PORTABLE_TOOL_ROUNDS}"
            ));
        }
        if self.requested_at > now + chrono::Duration::minutes(5)
            || self.deadline_at <= now
            || self.deadline_at <= self.requested_at
        {
            return invalid("portable Coder request timestamps are invalid or expired");
        }
        if !self.work_policy.checkpoint_capture_all || !self.work_policy.checkpoint_secret_scan {
            return invalid(
                "portable Coder requires complete checkpoint capture and secret scanning",
            );
        }
        if spec.repository.repository_id != self.project_id
            || spec.base_commit != self.expected_base_oid
        {
            return invalid(
                "portable Coder project or base commit does not match the work environment",
            );
        }
        if spec.checkpoint_ref.is_none() {
            return invalid("portable Coder requires an immutable input checkpoint");
        }
        let requested = validated_unique_tools(&self.requested_tool_names)?;
        let ceiling = crate::agent_runtime::coder_tools::portable_coder_tool_names();
        if let Some(denied) = requested.iter().find(|name| !ceiling.contains(*name)) {
            return invalid(format!(
                "tool is outside the portable Coder contract: {denied}"
            ));
        }

        let Some(grant) = self.task_execution_grant.as_ref() else {
            return if require_destination_grant {
                invalid("remote portable Coder task is missing its destination grant")
            } else {
                Ok(())
            };
        };
        validate_grant(self, spec, grant, now, &requested)
    }

    pub fn effective_tool_names(&self) -> Vec<String> {
        let granted = self.task_execution_grant.as_ref().map(|grant| {
            grant
                .effective_tool_names
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
        });
        self.requested_tool_names
            .iter()
            .filter(|name| {
                granted
                    .as_ref()
                    .is_none_or(|granted| granted.contains(*name))
            })
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableCoderResult {
    pub schema_version: u32,
    pub operation_id: String,
    pub work_id: String,
    pub destination_runtime_id: String,
    pub project_id: String,
    pub input_checkpoint_oid: String,
    pub response_text: String,
    pub tool_names: Vec<String>,
    pub changed_files: Vec<ChangedFile>,
    pub termination_reason: String,
    pub workspace_state_digest: String,
    pub evidence_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    pub completed_at: DateTime<Utc>,
}

impl PortableCoderResult {
    pub fn validate(&self) -> Result<(), WorkEnvironmentError> {
        if self.schema_version != PORTABLE_CODER_RESULT_SCHEMA_VERSION {
            return invalid(format!(
                "unsupported portable Coder result schema_version={}",
                self.schema_version
            ));
        }
        for (label, value) in [
            ("operation_id", self.operation_id.as_str()),
            ("work_id", self.work_id.as_str()),
            (
                "destination_runtime_id",
                self.destination_runtime_id.as_str(),
            ),
            ("project_id", self.project_id.as_str()),
            ("termination_reason", self.termination_reason.as_str()),
        ] {
            validate_text_identity(label, value)?;
        }
        validate_git_oid("input_checkpoint_oid", &self.input_checkpoint_oid)?;
        validate_sha256("workspace_state_digest", &self.workspace_state_digest)?;
        validate_sha256("evidence_digest", &self.evidence_digest)?;
        let tools = validated_unique_tools(&self.tool_names)?;
        let ceiling = crate::agent_runtime::coder_tools::portable_coder_tool_names();
        if let Some(denied) = tools.iter().find(|name| !ceiling.contains(*name)) {
            return invalid(format!(
                "portable Coder result reports a tool outside its contract: {denied}"
            ));
        }
        if self.changed_files.len() > MAX_PORTABLE_CHANGED_FILES {
            return invalid("portable Coder result contains too many changed files");
        }
        Ok(())
    }
}

#[async_trait]
pub trait PortableCoderRunner: Send + Sync {
    async fn run(
        &self,
        task: &PortableCoderTask,
        binding: WorkEnvironmentBinding,
        cancellation: CancellationToken,
        deadline: Instant,
    ) -> Result<PortableCoderResult, WorkEnvironmentError>;
}

fn validate_grant(
    task: &PortableCoderTask,
    spec: &WorkEnvironmentSpec,
    grant: &TaskExecutionGrant,
    now: DateTime<Utc>,
    requested: &BTreeSet<String>,
) -> Result<(), WorkEnvironmentError> {
    let secrets = spec.secret_refs.iter().cloned().collect::<BTreeSet<_>>();
    let granted_secrets = grant
        .authorized_secret_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let effective = grant
        .effective_tool_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if grant.schema_version != TASK_EXECUTION_GRANT_SCHEMA_VERSION
        || grant.worker_intent != "coder"
        || !grant.work_environment_materialization
        || grant.work_id != task.work_id
        || grant.correlation_id != task.correlation_id
        || grant.parent_session_id != task.parent_session_id
        || grant.project_id.as_deref() != Some(task.project_id.as_str())
        || grant.authorized_root_ref.as_deref() != Some(task.root_ref.as_str())
        || grant.network_policy != requested_network_policy(&spec.network_policy)
        || granted_secrets != secrets
        || !requested.is_subset(&effective)
        || grant.issued_at > now + chrono::Duration::minutes(5)
        || grant.expires_at <= now
        || grant.expires_at > task.deadline_at
    {
        return Err(WorkEnvironmentError::AdmissionDenied(
            "portable Coder destination grant does not match the admitted task".to_string(),
        ));
    }
    Ok(())
}

pub fn requested_network_policy(policy: &WorkEnvironmentNetworkPolicy) -> PeerNetworkPolicy {
    match policy {
        WorkEnvironmentNetworkPolicy::Deny => PeerNetworkPolicy::Deny,
        WorkEnvironmentNetworkPolicy::AllowList { .. } => PeerNetworkPolicy::WebOnly,
    }
}

fn validated_unique_tools(values: &[String]) -> Result<BTreeSet<String>, WorkEnvironmentError> {
    if values.is_empty() || values.len() > 64 {
        return invalid("portable Coder requested tool count is invalid");
    }
    let mut unique = BTreeSet::new();
    for value in values {
        validate_text_identity("requested tool name", value)?;
        if !unique.insert(value.clone()) {
            return invalid(format!("duplicate portable Coder tool: {value}"));
        }
    }
    Ok(unique)
}

fn validate_text_identity(label: &str, value: &str) -> Result<(), WorkEnvironmentError> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > 512
        || value.chars().any(char::is_control)
    {
        return invalid(format!("portable Coder {label} is invalid"));
    }
    Ok(())
}

fn validate_git_oid(label: &str, value: &str) -> Result<(), WorkEnvironmentError> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid(format!(
            "portable Coder {label} must be an immutable Git oid"
        ));
    }
    Ok(())
}

fn validate_sha256(label: &str, value: &str) -> Result<(), WorkEnvironmentError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return invalid(format!("portable Coder {label} must use sha256"));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid(format!("portable Coder {label} must use sha256"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, WorkEnvironmentError> {
    Err(WorkEnvironmentError::InvalidSpec(message.into()))
}

#[cfg(feature = "full-daemon")]
mod daemon_runner {
    use std::collections::BTreeMap;

    use medousa_runtime::{
        RuntimePorts, ToolLoopCompletionGate, TurnCompletionProfile, WorkEnvironmentExecRequest,
        WorkEnvironmentExecResult,
    };
    use serde_json::{Value, json};
    use stasis::application::orchestration::prompt_pipeline::PromptExecutionContext;
    use stasis::application::orchestration::tool_loop_pipeline::{
        ToolCallMode, ToolLoopExecutionRequest,
    };

    use super::*;
    use crate::agent_runtime::coder_tools::{CoderExecutionGuard, PortableCoderToolRegistry};
    use crate::agent_runtime::execution_context::{
        ProviderRoute, SurfaceCapabilities, TurnExecutionContext, with_turn_execution_context,
    };
    use crate::agent_runtime::turn_worker::{
        TurnWorkerIntent, worker_system_prompt_for_parent_mode,
    };
    use crate::request_principal::RequestPrincipal;
    use crate::session_storage::SessionId;
    use crate::turn_scope::TurnContinuationScope;

    pub struct AgentPortableCoderRunner {
        agent: Arc<crate::tools::TuiRuntime>,
        execution_policies: Arc<crate::peer_execution_policy::PeerExecutionPolicyStore>,
        destination_runtime_id: String,
    }

    impl AgentPortableCoderRunner {
        pub fn new(
            agent: Arc<crate::tools::TuiRuntime>,
            execution_policies: Arc<crate::peer_execution_policy::PeerExecutionPolicyStore>,
            destination_runtime_id: impl Into<String>,
        ) -> Self {
            Self {
                agent,
                execution_policies,
                destination_runtime_id: destination_runtime_id.into(),
            }
        }
    }

    struct PortableGrantGuard {
        execution_policies: Arc<crate::peer_execution_policy::PeerExecutionPolicyStore>,
        grant: TaskExecutionGrant,
    }

    impl CoderExecutionGuard for PortableGrantGuard {
        fn verify(&self) -> stasis::prelude::Result<()> {
            match self.execution_policies.coder_grant_is_active(&self.grant) {
                Ok(true) => Ok(()),
                Ok(false) => Err(stasis::prelude::StasisError::PortFailure(
                    "portable Coder authority was revoked by the destination workshop".to_string(),
                )),
                Err(error) => Err(stasis::prelude::StasisError::PortFailure(format!(
                    "portable Coder grant revalidation failed: {error}"
                ))),
            }
        }
    }

    #[async_trait]
    impl PortableCoderRunner for AgentPortableCoderRunner {
        async fn run(
            &self,
            task: &PortableCoderTask,
            binding: WorkEnvironmentBinding,
            cancellation: CancellationToken,
            deadline: Instant,
        ) -> Result<PortableCoderResult, WorkEnvironmentError> {
            task.validate_for_runner()?;
            if let Some(grant) = task.task_execution_grant.as_ref()
                && grant.destination_runtime_id != self.destination_runtime_id
            {
                return Err(WorkEnvironmentError::AdmissionDenied(
                    "portable Coder grant names another destination runtime".to_string(),
                ));
            }
            verify_head(&binding, task, "input-head").await?;

            let guard = task.task_execution_grant.as_ref().map(|grant| {
                Arc::new(PortableGrantGuard {
                    execution_policies: Arc::clone(&self.execution_policies),
                    grant: grant.clone(),
                }) as Arc<dyn CoderExecutionGuard>
            });
            if let Some(guard) = guard.as_ref() {
                guard.verify().map_err(tool_error)?;
            }
            let tools = Arc::new(
                PortableCoderToolRegistry::new(
                    Arc::clone(&self.agent.tool_registry),
                    Arc::clone(&self.agent.tool_catalog),
                    task.work_policy.clone(),
                    task.effective_tool_names(),
                    guard,
                )
                .map_err(tool_error)?,
            );

            crate::workshop_env::apply_provider_llm_env(&task.provider);
            let pipeline = crate::tui::runtime_services::build_tool_loop_pipeline_for_target(
                &task.provider,
                &task.model,
                crate::resolve_llm_base_url(Some(&task.provider), None).as_deref(),
                tools,
            );
            let scope = TurnContinuationScope {
                turn_correlation_id: task.correlation_id.clone(),
                session_id: task.parent_session_id.clone(),
                identity_user_id: None,
                original_prompt: task.prompt.clone(),
                delivery_target: None,
                provider: task.provider.clone(),
                model: task.model.clone(),
                response_depth_mode: task.response_depth_mode.clone(),
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
                channel_surface: Some("portable_coder".to_string()),
            };
            let session_id = SessionId::parse(&task.parent_session_id).map_err(|error| {
                WorkEnvironmentError::InvalidSpec(format!(
                    "portable Coder parent session is invalid: {error}"
                ))
            })?;
            let context = Arc::new(
                TurnExecutionContext::new(
                    task.operation_id.clone(),
                    task.correlation_id.clone(),
                    session_id,
                    RequestPrincipal::worker(format!("portable:workflow:{}", task.work_id)),
                    ProviderRoute::new(task.provider.clone(), task.model.clone()),
                    SurfaceCapabilities::default(),
                    cancellation,
                    deadline,
                    scope,
                )
                .with_work_environment(binding.clone()),
            );
            let mut system_prompt = worker_system_prompt_for_parent_mode(
                &task.parent_session_id,
                TurnWorkerIntent::Coder,
                None,
                false,
                false,
                Some("coder"),
            );
            system_prompt.push_str(&portable_prompt_appendix(task));
            let request = ToolLoopExecutionRequest {
                user_prompt: task.prompt.clone(),
                system_prompt: Some(system_prompt),
                context: PromptExecutionContext {
                    trace_id: Some(task.operation_id.clone()),
                    correlation_id: Some(task.correlation_id.clone()),
                    policy_profile: Some("portable_coder".to_string()),
                    model_hint: Some(task.model.clone()),
                    reasoning_effort: Some(task.response_depth_mode.clone()),
                },
                tool_name: String::new(),
                tool_input: Value::Null,
                tool_call_mode: ToolCallMode::Auto,
            };
            let mut gate = ToolLoopCompletionGate::new_for_execution(
                stable_stream_id(&task.operation_id),
                RuntimePorts::new(),
                task.max_tool_rounds,
            );
            gate.skip_avec_ritual_check = true;
            gate.completion_profile = TurnCompletionProfile::WorkerSynthesis;
            gate.parent_turn_correlation_id = Some(task.correlation_id.clone());

            let response = with_turn_execution_context(context, async {
                pipeline
                    .execute_with_stream_prior_messages_max_rounds(
                        request,
                        Vec::new(),
                        None,
                        task.max_tool_rounds,
                        Some(&mut gate),
                        None,
                    )
                    .await
            })
            .await
            .map_err(tool_error)?;

            verify_head(&binding, task, "output-head").await?;
            let (changed_files, status_bytes) = validate_workspace_changes(&binding, task).await?;
            let workspace_state_digest = sha256(&status_bytes);
            let tool_names = response
                .tool_invocations
                .iter()
                .map(|invocation| invocation.tool_name.clone())
                .collect::<Vec<_>>();
            let evidence = serde_json::to_vec(&json!({
                "schema_version": 1,
                "operation_id": &task.operation_id,
                "work_id": &task.work_id,
                "project_id": &task.project_id,
                "input_checkpoint_oid": &task.expected_checkpoint_oid,
                "destination_runtime_id": &self.destination_runtime_id,
                "response_text": &response.text,
                "tool_invocations": &response.tool_invocations,
                "termination_reason": &response.termination_reason,
                "changed_files": &changed_files,
                "workspace_state_digest": &workspace_state_digest,
                "grant_id": task.task_execution_grant.as_ref().map(|grant| grant.grant_id.as_str()),
            }))
            .map_err(|error| WorkEnvironmentError::Adapter(error.to_string()))?;
            let result = PortableCoderResult {
                schema_version: PORTABLE_CODER_RESULT_SCHEMA_VERSION,
                operation_id: task.operation_id.clone(),
                work_id: task.work_id.clone(),
                destination_runtime_id: self.destination_runtime_id.clone(),
                project_id: task.project_id.clone(),
                input_checkpoint_oid: task.expected_checkpoint_oid.clone(),
                response_text: response.text,
                tool_names,
                changed_files,
                termination_reason: response.termination_reason,
                workspace_state_digest,
                evidence_digest: sha256(&evidence),
                grant_id: task
                    .task_execution_grant
                    .as_ref()
                    .map(|grant| grant.grant_id.clone()),
                completed_at: Utc::now(),
            };
            result.validate()?;
            Ok(result)
        }
    }

    impl PortableCoderTask {
        fn validate_for_runner(&self) -> Result<(), WorkEnvironmentError> {
            if let Some(grant) = &self.task_execution_grant {
                if !self
                    .effective_tool_names()
                    .iter()
                    .any(|name| name == crate::public_api::COGNITION_TURN)
                {
                    return Err(WorkEnvironmentError::AdmissionDenied(
                        "portable Coder task must retain structural turn authority".to_string(),
                    ));
                }
                if grant.expires_at <= Utc::now() {
                    return Err(WorkEnvironmentError::AdmissionDenied(
                        "portable Coder destination grant expired".to_string(),
                    ));
                }
            }
            Ok(())
        }
    }

    fn portable_prompt_appendix(task: &PortableCoderTask) -> String {
        format!(
            "\n\n[PORTABLE_CODER]\nworkspace=/workspace\nproject_id={}\ninput_checkpoint={}\n\
             Git authority, checkpointing, publication, and transport are runtime-owned. Never run \
             Git commands that mutate refs, the index, branches, or HEAD. Read/search with \
             cognition_store_read action=code.read|code.search; write with cognition_store_write \
             action=code.write and its digest precondition. Use file:///workspace/... for code \
             intelligence. Finish with cognition_turn only after verification and then give the \
             user a concise result.",
            task.project_id, task.expected_checkpoint_oid,
        )
    }

    async fn verify_head(
        binding: &WorkEnvironmentBinding,
        task: &PortableCoderTask,
        boundary: &str,
    ) -> Result<(), WorkEnvironmentError> {
        let result = environment_exec(
            binding,
            &format!("{}:{boundary}", task.operation_id),
            "git",
            vec!["rev-parse".to_string(), "HEAD".to_string()],
            64 * 1024,
        )
        .await?;
        ensure_exec_success(boundary, &result)?;
        if result.stdout.trim() != task.expected_checkpoint_oid {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "portable Coder {boundary} changed immutable HEAD: expected {}, found {}",
                task.expected_checkpoint_oid,
                result.stdout.trim()
            )));
        }
        Ok(())
    }

    async fn validate_workspace_changes(
        binding: &WorkEnvironmentBinding,
        task: &PortableCoderTask,
    ) -> Result<(Vec<ChangedFile>, Vec<u8>), WorkEnvironmentError> {
        let status = environment_exec(
            binding,
            &format!("{}:final-status", task.operation_id),
            "git",
            vec![
                "status".to_string(),
                "--porcelain=v1".to_string(),
                "-z".to_string(),
                "--untracked-files=all".to_string(),
            ],
            MAX_ENVIRONMENT_COMMAND_OUTPUT_BYTES,
        )
        .await?;
        ensure_exec_success("final status", &status)?;
        if status.output_truncated {
            return Err(WorkEnvironmentError::AdmissionDenied(
                "portable Coder changed-file inventory exceeded its bound".to_string(),
            ));
        }
        let status_bytes = status.stdout.as_bytes().to_vec();
        let mut changed = parse_porcelain(&status.stdout)?;
        if changed.len() > MAX_PORTABLE_CHANGED_FILES {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "portable Coder changed more than {MAX_PORTABLE_CHANGED_FILES} files"
            )));
        }
        if changed
            .iter()
            .any(|file| file.status == ChangeStatus::Unmerged)
        {
            return Err(WorkEnvironmentError::AdmissionDenied(
                "portable Coder left unmerged files".to_string(),
            ));
        }
        let violations = medousa_forge::policy::evaluate_paths(&task.work_policy, &changed)
            .map_err(|error| WorkEnvironmentError::AdmissionDenied(error.to_string()))?;
        if let Some(first) = violations.first() {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "portable Coder changed {} path(s) outside policy (first: {} — {})",
                violations.len(),
                first.path,
                first.rule
            )));
        }
        let exclusions = medousa_forge::policy::capture_exclusions(&task.work_policy, &changed)
            .map_err(|error| WorkEnvironmentError::AdmissionDenied(error.to_string()))?;
        if let Some(first) = exclusions.first() {
            return Err(WorkEnvironmentError::AdmissionDenied(format!(
                "portable Coder changed a path excluded from durable capture: {first}"
            )));
        }

        let per_file_limit = match task.work_policy.checkpoint_max_file_bytes {
            0 => MAX_PORTABLE_SCANNED_FILE_BYTES,
            configured => configured.min(MAX_PORTABLE_SCANNED_FILE_BYTES),
        };
        let total_limit = match task.work_policy.checkpoint_max_total_bytes {
            0 => MAX_PORTABLE_SCANNED_TOTAL_BYTES,
            configured => configured.min(MAX_PORTABLE_SCANNED_TOTAL_BYTES),
        };
        let mut total = 0_u64;
        for (index, file) in changed.iter_mut().enumerate() {
            if file.status == ChangeStatus::Deleted {
                continue;
            }
            let probe = environment_exec_with_args(
                binding,
                &format!("{}:probe:{index}", task.operation_id),
                "/bin/sh",
                vec![
                    "-c".to_string(),
                    "if [ -L \"$1\" ]; then echo symlink; elif [ -d \"$1\" ]; then echo directory; elif [ -f \"$1\" ]; then printf 'file '; wc -c < \"$1\"; elif [ -e \"$1\" ]; then echo unsupported; else echo missing; fi".to_string(),
                    "sh".to_string(),
                    format!("./{}", file.path),
                ],
                1024,
            )
            .await?;
            ensure_exec_success("changed-file probe", &probe)?;
            let probe = probe.stdout.trim();
            if probe == "missing" {
                continue;
            }
            let Some(raw_size) = probe.strip_prefix("file ") else {
                return Err(WorkEnvironmentError::AdmissionDenied(format!(
                    "portable Coder output contains an unsafe {probe}: {}",
                    file.path
                )));
            };
            let size = raw_size.trim().parse::<u64>().map_err(|_| {
                WorkEnvironmentError::Adapter("invalid changed-file size probe".to_string())
            })?;
            file.byte_size = Some(size);
            total = total.saturating_add(size);
            if size > per_file_limit || total > total_limit {
                return Err(WorkEnvironmentError::AdmissionDenied(format!(
                    "portable Coder output exceeds durable capture limits at {}",
                    file.path
                )));
            }
            if task.work_policy.checkpoint_secret_scan {
                let content = environment_exec_with_args(
                    binding,
                    &format!("{}:scan:{index}", task.operation_id),
                    "/bin/sh",
                    vec![
                        "-c".to_string(),
                        "cat -- \"$1\"".to_string(),
                        "sh".to_string(),
                        format!("./{}", file.path),
                    ],
                    size.max(1),
                )
                .await?;
                ensure_exec_success("changed-file secret scan", &content)?;
                if content.output_truncated {
                    return Err(WorkEnvironmentError::AdmissionDenied(format!(
                        "portable Coder could not completely scan {}",
                        file.path
                    )));
                }
                if let Some(pattern) =
                    medousa_forge::policy::secret_pattern_in_bytes(content.stdout.as_bytes())
                {
                    return Err(WorkEnvironmentError::AdmissionDenied(format!(
                        "portable Coder output contains likely secret {pattern} in {}",
                        file.path
                    )));
                }
            }
        }
        Ok((changed, status_bytes))
    }

    async fn environment_exec(
        binding: &WorkEnvironmentBinding,
        idempotency_key: &str,
        program: &str,
        args: Vec<String>,
        max_output_bytes: u64,
    ) -> Result<WorkEnvironmentExecResult, WorkEnvironmentError> {
        environment_exec_with_args(binding, idempotency_key, program, args, max_output_bytes).await
    }

    async fn environment_exec_with_args(
        binding: &WorkEnvironmentBinding,
        idempotency_key: &str,
        program: &str,
        args: Vec<String>,
        max_output_bytes: u64,
    ) -> Result<WorkEnvironmentExecResult, WorkEnvironmentError> {
        binding
            .port
            .exec(
                &binding.handle,
                WorkEnvironmentExecRequest {
                    idempotency_key: idempotency_key.to_string(),
                    program: program.to_string(),
                    args,
                    working_directory: Some(
                        medousa_runtime::WORK_ENVIRONMENT_WORKSPACE_ROOT.to_string(),
                    ),
                    environment: BTreeMap::new(),
                    stdin: None,
                    timeout_seconds: 30,
                    max_output_bytes: max_output_bytes
                        .clamp(1, MAX_ENVIRONMENT_COMMAND_OUTPUT_BYTES),
                },
                &binding.fence,
            )
            .await
    }

    fn ensure_exec_success(
        operation: &str,
        result: &WorkEnvironmentExecResult,
    ) -> Result<(), WorkEnvironmentError> {
        if result.exit_code == Some(0) {
            return Ok(());
        }
        let detail = if result.stderr.trim().is_empty() {
            format!("exit code {:?}", result.exit_code)
        } else {
            result.stderr.trim().to_string()
        };
        Err(WorkEnvironmentError::Adapter(format!(
            "portable Coder {operation} failed: {detail}"
        )))
    }

    fn parse_porcelain(output: &str) -> Result<Vec<ChangedFile>, WorkEnvironmentError> {
        let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
        let mut changed = Vec::new();
        while let Some(entry) = entries.next() {
            let bytes = entry.as_bytes();
            if bytes.len() < 4 || bytes[2] != b' ' {
                return Err(WorkEnvironmentError::Adapter(
                    "invalid Git porcelain entry in portable Coder output".to_string(),
                ));
            }
            let status = &entry[..2];
            let path = portable_relative_path(&entry[3..])?;
            let is_unmerged = status.contains('U')
                || matches!(status, "AA" | "DD")
                || (status.starts_with('A') && status.ends_with('D'))
                || (status.starts_with('D') && status.ends_with('A'));
            let change_status = if is_unmerged {
                ChangeStatus::Unmerged
            } else if status == "??" {
                ChangeStatus::Untracked
            } else if status.contains('R') {
                ChangeStatus::Renamed
            } else if status.contains('C') {
                ChangeStatus::Copied
            } else if status.contains('A') {
                ChangeStatus::Added
            } else if status.contains('D') {
                ChangeStatus::Deleted
            } else if status.contains('T') {
                ChangeStatus::TypeChanged
            } else {
                ChangeStatus::Modified
            };
            let old_path = if matches!(change_status, ChangeStatus::Renamed | ChangeStatus::Copied)
            {
                entries.next().map(portable_relative_path).transpose()?
            } else {
                None
            };
            changed.push(ChangedFile {
                path,
                status: change_status,
                old_path,
                is_binary: false,
                byte_size: None,
            });
        }
        Ok(changed)
    }

    fn portable_relative_path(raw: &str) -> Result<String, WorkEnvironmentError> {
        let path = std::path::Path::new(raw);
        if raw.is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(WorkEnvironmentError::AdmissionDenied(
                "portable Coder output path escapes /workspace".to_string(),
            ));
        }
        Ok(path.to_string_lossy().replace('\\', "/"))
    }

    fn stable_stream_id(operation_id: &str) -> u64 {
        let digest = Sha256::digest(operation_id.as_bytes());
        u64::from_be_bytes(digest[..8].try_into().expect("sha256 prefix"))
    }

    fn sha256(bytes: &[u8]) -> String {
        format!("sha256:{:x}", Sha256::digest(bytes))
    }

    fn tool_error(error: stasis::prelude::StasisError) -> WorkEnvironmentError {
        WorkEnvironmentError::Adapter(error.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_porcelain_without_accepting_paths_outside_workspace() {
            let changed = parse_porcelain(" M src/lib.rs\0?? notes.txt\0").unwrap();
            assert_eq!(changed.len(), 2);
            assert_eq!(changed[0].path, "src/lib.rs");
            assert_eq!(changed[1].status, ChangeStatus::Untracked);
            assert!(parse_porcelain(" M ../outside\0").is_err());
        }
    }
}

#[cfg(feature = "full-daemon")]
pub use daemon_runner::AgentPortableCoderRunner;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use medousa_runtime::{
        WorkEnvironmentCheckpoint, WorkEnvironmentFence, WorkEnvironmentId, WorkEnvironmentImage,
        WorkEnvironmentRepository, WorkEnvironmentRequirements, WorkEnvironmentRetention,
        WorkspaceId,
    };
    use stasis::domain::runtime::blob_descriptor::BlobDescriptor;
    use stasis::domain::runtime::provenance::{ContentDigest, ProvenanceRef};
    use stasis::domain::runtime::resource_lease::FencingToken;

    use super::*;

    fn spec() -> WorkEnvironmentSpec {
        let descriptor = BlobDescriptor {
            digest: ContentDigest::sha256_bytes(b"manifest"),
            size_bytes: 8,
            media_type: Some("application/json".to_string()),
            transfer_hint: None,
        };
        WorkEnvironmentSpec {
            environment_id: WorkEnvironmentId::parse("portable-task").unwrap(),
            workspace_id: WorkspaceId::parse("portable-task").unwrap(),
            repository: WorkEnvironmentRepository {
                repository_id: "repo-a".to_string(),
                authorized_origin: "medousa://repo-a".to_string(),
            },
            base_commit: "1".repeat(40),
            image: WorkEnvironmentImage {
                reference: "example.invalid/medousa/coder".to_string(),
                digest: ContentDigest::sha256_bytes(b"image"),
                platform: "linux/amd64".to_string(),
            },
            checkpoint_ref: Some(WorkEnvironmentCheckpoint {
                provenance: ProvenanceRef::cas(descriptor.digest.clone()),
                manifest: descriptor,
            }),
            requirements: WorkEnvironmentRequirements::default(),
            mounts: Vec::new(),
            network_policy: WorkEnvironmentNetworkPolicy::Deny,
            secret_refs: Vec::new(),
            fence: WorkEnvironmentFence {
                stasis_attempt: FencingToken(1),
                forge_environment_generation: Some(2),
                forge_execution_generation: Some(3),
            },
            publication: None,
            retention: WorkEnvironmentRetention::Delete,
        }
    }

    fn task(now: DateTime<Utc>) -> PortableCoderTask {
        PortableCoderTask {
            schema_version: PORTABLE_CODER_TASK_SCHEMA_VERSION,
            operation_id: "portable-op".to_string(),
            work_id: "work-a".to_string(),
            parent_session_id: "session-a".to_string(),
            correlation_id: "correlation-a".to_string(),
            project_id: "repo-a".to_string(),
            root_ref: "workspace:repo-a".to_string(),
            expected_base_oid: "1".repeat(40),
            expected_checkpoint_oid: "2".repeat(40),
            prompt: "Fix the failing test and verify it.".to_string(),
            provider: "openai".to_string(),
            model: "test-model".to_string(),
            response_depth_mode: "standard".to_string(),
            max_tool_rounds: 12,
            work_policy: WorkPolicy::default(),
            requested_tool_names: vec![
                crate::public_api::COGNITION_TURN.to_string(),
                crate::public_api::COGNITION_STORE_READ.to_string(),
            ],
            requested_at: now,
            deadline_at: now + chrono::Duration::minutes(5),
            task_execution_grant: None,
        }
    }

    #[test]
    fn portable_task_requires_checkpoint_and_never_serializes_host_paths() {
        let now = Utc::now();
        let spec = spec();
        let task = task(now);
        task.validate(&spec, now, false).unwrap();
        let encoded = serde_json::to_string(&task).unwrap();
        assert!(!encoded.contains("/Users/"));
        assert!(!encoded.contains("container"));

        let mut missing = spec;
        missing.checkpoint_ref = None;
        assert!(task.validate(&missing, now, false).is_err());
    }

    #[test]
    fn remote_portable_task_requires_an_exact_destination_grant() {
        let now = Utc::now();
        let spec = spec();
        let mut task = task(now);
        assert!(task.validate(&spec, now, true).is_err());
        task.task_execution_grant = Some(TaskExecutionGrant {
            schema_version: TASK_EXECUTION_GRANT_SCHEMA_VERSION,
            grant_id: "grant-a".to_string(),
            peer_device_id: "peer-a".to_string(),
            peer_pairing_id: "pair-a".to_string(),
            origin_runtime_id: "peer-a".to_string(),
            destination_runtime_id: "runtime-b".to_string(),
            parent_session_id: task.parent_session_id.clone(),
            bot_id: None,
            work_id: task.work_id.clone(),
            correlation_id: task.correlation_id.clone(),
            worker_intent: "coder".to_string(),
            project_id: Some(task.project_id.clone()),
            work_environment_materialization: true,
            authorized_root_ref: Some(task.root_ref.clone()),
            authorized_secret_refs: Vec::new(),
            policy_revision: 1,
            policy_source: crate::peer_execution_policy::PeerExecutionPolicySource::Stored,
            requested_tool_domains: vec!["code".to_string(), "turn".to_string()],
            effective_tool_domains: vec!["code".to_string(), "turn".to_string()],
            requested_tool_names: task.requested_tool_names.clone(),
            effective_tool_names: task.requested_tool_names.clone(),
            network_policy: PeerNetworkPolicy::Deny,
            issued_at: now,
            expires_at: task.deadline_at,
        });
        task.validate(&spec, now, true).unwrap();

        let mut wrong_secrets = spec;
        wrong_secrets.secret_refs = BTreeSet::from(["secret-a".to_string()])
            .into_iter()
            .collect();
        assert!(task.validate(&wrong_secrets, now, true).is_err());
    }
}
