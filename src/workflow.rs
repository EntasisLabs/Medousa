//! Medousa declarative workflow registry and sequential job handler.
//!
//! Design: docs/internal/runtime-tools-roadmap.md (Phase D2)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionPipeline, PromptExecutionRequest,
};
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::application::runtime::runtime_factory::{RuntimeComposition, RuntimeFactory};
use stasis::prelude::{BackoffPolicy, NewJob};
use stasis::domain::runtime::job::Job;
use stasis::ports::outbound::runtime::workflow_engine::WorkflowEngine;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::execution_policy::{
    load_parallel_execution_settings, validate_concurrent_workflow,
};
use crate::identity_memory;
use crate::mcp_gateway_api::{McpInvokeRequest, McpTurnContext, McpTurnLane};
use crate::mcp_gateway_client::McpGatewayClient;
use crate::mcp_turn_token::mint_mcp_turn_token;
use crate::tools::validate_grapheme_source_for_schedule;
use crate::turn_continuation::{ContinuationAwaitMode, TurnContinuationScope, wire_turn_child_job};

pub use medousa_types::workflow::{WorkflowRunRequest, WorkflowStepSpec};

pub const WORKFLOW_SEQUENTIAL_JOB_TYPE: &str = "workflow.medousa.sequential";
pub const WORKFLOW_CONCURRENT_JOB_TYPE: &str = "workflow.medousa.concurrent";
pub const WORKFLOW_HANDOFF_JOB_TYPE: &str = "workflow.medousa.handoff";
pub const WORKFLOW_PAYLOAD_PREFIX: &str = "medousa:workflow:";
pub const MAX_WORKFLOW_STEPS: usize = 20;

pub fn workflow_job_type_for_strategy(strategy: &str) -> Option<&'static str> {
    match strategy.trim().to_ascii_lowercase().as_str() {
        "sequential" => Some(WORKFLOW_SEQUENTIAL_JOB_TYPE),
        "concurrent" => Some(WORKFLOW_CONCURRENT_JOB_TYPE),
        "handoff" => Some(WORKFLOW_HANDOFF_JOB_TYPE),
        _ => None,
    }
}

static WORKFLOW_REGISTRY: OnceCell<Arc<WorkflowRegistry>> = OnceCell::new();

pub fn shared_workflow_registry() -> Arc<WorkflowRegistry> {
    WORKFLOW_REGISTRY
        .get_or_init(|| Arc::new(WorkflowRegistry::default()))
        .clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Enqueued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl WorkflowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enqueued => "enqueued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedousaWorkflowPayload {
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub strategy: String,
    pub mode: String,
    pub on_failure: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub lane: String,
    pub steps: Vec<WorkflowStepSpec>,
}

pub type MedousaSequentialWorkflowPayload = MedousaWorkflowPayload;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepResult {
    pub id: String,
    pub kind: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowRecord {
    pub workflow_id: String,
    pub name: Option<String>,
    pub strategy: String,
    pub mode: String,
    pub on_failure: String,
    pub note: Option<String>,
    pub root_job_id: String,
    pub status: WorkflowStatus,
    pub created_at: DateTime<Utc>,
    pub scheduled_recurring_id: Option<String>,
    pub step_results: Vec<WorkflowStepResult>,
}

#[derive(Default)]
pub struct WorkflowRegistry {
    inner: RwLock<HashMap<String, WorkflowRecord>>,
}

impl WorkflowRegistry {
    pub async fn insert(&self, record: WorkflowRecord) {
        self.inner
            .write()
            .await
            .insert(record.workflow_id.clone(), record);
    }

    pub async fn get(&self, workflow_id: &str) -> Option<WorkflowRecord> {
        self.inner.read().await.get(workflow_id).cloned()
    }

    pub async fn update_status(
        &self,
        workflow_id: &str,
        status: WorkflowStatus,
        step_results: Vec<WorkflowStepResult>,
    ) {
        let mut guard = self.inner.write().await;
        if let Some(record) = guard.get_mut(workflow_id) {
            record.status = status;
            record.step_results = step_results;
        }
    }

    pub async fn mark_canceled(&self, workflow_id: &str) {
        let mut guard = self.inner.write().await;
        if let Some(record) = guard.get_mut(workflow_id) {
            record.status = WorkflowStatus::Canceled;
        }
    }

    pub async fn set_recurring_id(&self, workflow_id: &str, recurring_id: String) {
        let mut guard = self.inner.write().await;
        if let Some(record) = guard.get_mut(workflow_id) {
            record.scheduled_recurring_id = Some(recurring_id);
        }
    }

    pub async fn list(&self, limit: usize) -> Vec<WorkflowRecord> {
        let guard = self.inner.read().await;
        let mut records: Vec<_> = guard.values().cloned().collect();
        records.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        records.truncate(limit.clamp(1, 500));
        records
    }
}

pub fn encode_workflow_payload(payload: &MedousaWorkflowPayload) -> stasis::prelude::Result<String> {
    let raw = serde_json::to_string(payload).map_err(|error| {
        stasis::prelude::StasisError::PortFailure(format!(
            "failed to encode workflow payload: {error}"
        ))
    })?;
    Ok(format!("{WORKFLOW_PAYLOAD_PREFIX}{raw}"))
}

pub fn decode_workflow_payload(payload_ref: &str) -> stasis::prelude::Result<MedousaWorkflowPayload> {
    let raw = payload_ref
        .strip_prefix(WORKFLOW_PAYLOAD_PREFIX)
        .unwrap_or(payload_ref);
    serde_json::from_str(raw).map_err(|error| {
        stasis::prelude::StasisError::PortFailure(format!(
            "invalid workflow payload json: {error}"
        ))
    })
}

pub fn validate_workflow_request(request: &WorkflowRunRequest) -> stasis::prelude::Result<()> {
    if request.steps.is_empty() {
        return Err(stasis::prelude::StasisError::PortFailure(
            "workflow requires at least one step".to_string(),
        ));
    }
    if request.steps.len() > MAX_WORKFLOW_STEPS {
        return Err(stasis::prelude::StasisError::PortFailure(format!(
            "workflow exceeds max steps ({MAX_WORKFLOW_STEPS})"
        )));
    }
    if request.strategy != "sequential"
        && request.strategy != "concurrent"
        && request.strategy != "handoff"
    {
        return Err(stasis::prelude::StasisError::PortFailure(format!(
            "unsupported workflow strategy '{}'; supported: sequential, concurrent, handoff",
            request.strategy
        )));
    }

    let parallel_settings = load_parallel_execution_settings();
    if request.strategy == "concurrent" {
        validate_concurrent_workflow(&request.steps, &parallel_settings).map_err(|reason| {
            stasis::prelude::StasisError::PortFailure(format!(
                "concurrent workflow policy violation: {reason}"
            ))
        })?;
    }

    let mut seen_ids = HashMap::new();
    for step in &request.steps {
        let id = step.id().trim();
        if id.is_empty() {
            return Err(stasis::prelude::StasisError::PortFailure(
                "workflow step id must be non-empty".to_string(),
            ));
        }
        if seen_ids.insert(id.to_string(), ()).is_some() {
            return Err(stasis::prelude::StasisError::PortFailure(format!(
                "duplicate workflow step id '{id}'"
            )));
        }
    }

    let on_failure = request.on_failure.trim();
    if on_failure != "stop" && on_failure != "continue" {
        return Err(stasis::prelude::StasisError::PortFailure(
            "on_failure must be 'stop' or 'continue'".to_string(),
        ));
    }

    Ok(())
}

pub fn apply_step_refs(
    template: &str,
    outputs: &HashMap<String, Value>,
    handoff: Option<&Value>,
) -> String {
    let mut result = template.to_string();
    for (step_id, output) in outputs {
        let needle = format!("$steps.{step_id}.output");
        let replacement = output_to_string(output);
        result = result.replace(&needle, &replacement);
    }
    if let Some(handoff_value) = handoff {
        let handoff_json = serde_json::to_string(handoff_value)
            .unwrap_or_else(|_| handoff_value.to_string());
        result = result.replace("$handoff.context", &handoff_json);
    }
    result
}

fn output_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

pub fn new_workflow_id() -> String {
    format!("wf-{}", Uuid::new_v4().simple())
}

pub struct MedousaSequentialWorkflowHandler {
    executor: WorkflowExecutor,
}

impl MedousaSequentialWorkflowHandler {
    pub(crate) fn new(executor: WorkflowExecutor) -> Self {
        Self { executor }
    }

    pub fn with_defaults(registry: Arc<WorkflowRegistry>, prompt_pipeline: PromptExecutionPipeline) -> Self {
        Self::new(WorkflowExecutor::with_defaults(registry, prompt_pipeline))
    }
}

#[async_trait]
impl JobHandler for MedousaSequentialWorkflowHandler {
    fn job_type(&self) -> &'static str {
        WORKFLOW_SEQUENTIAL_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> stasis::prelude::Result<JobExecutionOutcome> {
        self.executor
            .execute(job, WorkflowExecutionMode::Sequential)
            .await
    }
}

pub struct MedousaConcurrentWorkflowHandler {
    executor: WorkflowExecutor,
}

impl MedousaConcurrentWorkflowHandler {
    pub(crate) fn new(executor: WorkflowExecutor) -> Self {
        Self { executor }
    }

    pub fn with_defaults(registry: Arc<WorkflowRegistry>, prompt_pipeline: PromptExecutionPipeline) -> Self {
        Self::new(WorkflowExecutor::with_defaults(registry, prompt_pipeline))
    }
}

#[async_trait]
impl JobHandler for MedousaConcurrentWorkflowHandler {
    fn job_type(&self) -> &'static str {
        WORKFLOW_CONCURRENT_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> stasis::prelude::Result<JobExecutionOutcome> {
        self.executor
            .execute(job, WorkflowExecutionMode::Concurrent)
            .await
    }
}

pub struct MedousaHandoffWorkflowHandler {
    executor: WorkflowExecutor,
}

impl MedousaHandoffWorkflowHandler {
    pub(crate) fn new(executor: WorkflowExecutor) -> Self {
        Self { executor }
    }

    pub fn with_defaults(registry: Arc<WorkflowRegistry>, prompt_pipeline: PromptExecutionPipeline) -> Self {
        Self::new(WorkflowExecutor::with_defaults(registry, prompt_pipeline))
    }
}

#[async_trait]
impl JobHandler for MedousaHandoffWorkflowHandler {
    fn job_type(&self) -> &'static str {
        WORKFLOW_HANDOFF_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> stasis::prelude::Result<JobExecutionOutcome> {
        self.executor
            .execute(job, WorkflowExecutionMode::Handoff)
            .await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkflowExecutionMode {
    Sequential,
    Concurrent,
    Handoff,
}

#[derive(Clone)]
pub(crate) struct WorkflowExecutor {
    workflow_engine: Arc<dyn WorkflowEngine>,
    prompt_pipeline: PromptExecutionPipeline,
    mcp_client: Arc<McpGatewayClient>,
    registry: Arc<WorkflowRegistry>,
}

impl WorkflowExecutor {
    fn new(
        workflow_engine: Arc<dyn WorkflowEngine>,
        prompt_pipeline: PromptExecutionPipeline,
        mcp_client: Arc<McpGatewayClient>,
        registry: Arc<WorkflowRegistry>,
    ) -> Self {
        Self {
            workflow_engine,
            prompt_pipeline,
            mcp_client,
            registry,
        }
    }

    fn with_defaults(registry: Arc<WorkflowRegistry>, prompt_pipeline: PromptExecutionPipeline) -> Self {
        Self::new(
            crate::grapheme_medousa_bridge::medousa_workflow_engine(),
            prompt_pipeline,
            Arc::new(McpGatewayClient::from_env()),
            registry,
        )
    }

    async fn execute(
        &self,
        job: &Job,
        mode: WorkflowExecutionMode,
    ) -> stasis::prelude::Result<JobExecutionOutcome> {
        let started = Instant::now();
        let payload = match decode_workflow_payload(&job.payload_ref) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(JobExecutionOutcome::FatalFailure {
                    message: error.to_string(),
                    execution_id: None,
                    diagnostics: Some(json!({ "error": error.to_string() }).to_string()),
                });
            }
        };

        self.registry
            .update_status(&payload.workflow_id, WorkflowStatus::Running, Vec::new())
            .await;

        let lane = if payload.lane == "scheduled" {
            McpTurnLane::Scheduled
        } else {
            McpTurnLane::Interactive
        };

        let step_results = match mode {
            WorkflowExecutionMode::Sequential => {
                self.run_sequential(&payload, lane).await
            }
            WorkflowExecutionMode::Concurrent => {
                self.run_concurrent(&payload, lane).await
            }
            WorkflowExecutionMode::Handoff => self.run_handoff(&payload, lane).await,
        };

        let workflow_failed = step_results.iter().any(|step| step.status == "failed");
        let final_status = if workflow_failed {
            WorkflowStatus::Failed
        } else {
            WorkflowStatus::Succeeded
        };

        self.registry
            .update_status(&payload.workflow_id, final_status, step_results.clone())
            .await;

        let duration_ms = started.elapsed().as_millis();
        let diagnostics = json!({
            "workflow_id": payload.workflow_id,
            "strategy": payload.strategy,
            "status": final_status.as_str(),
            "duration_ms": duration_ms,
            "steps": step_results,
        })
        .to_string();

        if workflow_failed {
            Ok(JobExecutionOutcome::FatalFailure {
                message: format!("workflow {} failed", payload.workflow_id),
                execution_id: Some(payload.workflow_id.clone()),
                diagnostics: Some(diagnostics),
            })
        } else {
            Ok(JobExecutionOutcome::Success {
                sttp_output_node_id: format!("sttp:workflow:{}", payload.workflow_id),
                execution_id: Some(payload.workflow_id.clone()),
                diagnostics: Some(diagnostics),
            })
        }
    }

    async fn run_sequential(
        &self,
        payload: &MedousaWorkflowPayload,
        lane: McpTurnLane,
    ) -> Vec<WorkflowStepResult> {
        let mut step_outputs: HashMap<String, Value> = HashMap::new();
        let mut step_results = Vec::new();
        let stop_on_failure = payload.on_failure == "stop";

        for step in &payload.steps {
            let result = execute_workflow_step(
                step,
                &step_outputs,
                None,
                &self.workflow_engine,
                &self.prompt_pipeline,
                &self.mcp_client,
                &payload.workflow_id,
                lane,
            )
            .await;

            let failed = result.status == "failed";
            step_results.push(result.clone());
            if let Some(output) = result.output.clone() {
                step_outputs.insert(result.id.clone(), output);
            }
            if failed && stop_on_failure {
                break;
            }
        }

        step_results
    }

    async fn run_concurrent(
        &self,
        payload: &MedousaWorkflowPayload,
        lane: McpTurnLane,
    ) -> Vec<WorkflowStepResult> {
        let empty_outputs = HashMap::new();
        let mut join_set = tokio::task::JoinSet::new();
        for step in payload.steps.clone() {
            let workflow_engine = self.workflow_engine.clone();
            let prompt_pipeline = self.prompt_pipeline.clone();
            let mcp_client = self.mcp_client.clone();
            let workflow_id = payload.workflow_id.clone();
            let prior_outputs = empty_outputs.clone();
            join_set.spawn(async move {
                execute_workflow_step(
                    &step,
                    &prior_outputs,
                    None,
                    &workflow_engine,
                    &prompt_pipeline,
                    &mcp_client,
                    &workflow_id,
                    lane,
                )
                .await
            });
        }

        let mut step_results = Vec::new();
        while let Some(joined) = join_set.join_next().await {
            match joined {
                Ok(result) => step_results.push(result),
                Err(error) => step_results.push(WorkflowStepResult {
                    id: "unknown".to_string(),
                    kind: "unknown".to_string(),
                    status: "failed".to_string(),
                    output: None,
                    error: Some(format!("concurrent step join failed: {error}")),
                }),
            }
        }

        step_results.sort_by(|left, right| left.id.cmp(&right.id));
        step_results
    }

    async fn run_handoff(
        &self,
        payload: &MedousaWorkflowPayload,
        lane: McpTurnLane,
    ) -> Vec<WorkflowStepResult> {
        let mut step_outputs: HashMap<String, Value> = HashMap::new();
        let mut handoff_context = json!({});
        let mut step_results = Vec::new();
        let stop_on_failure = payload.on_failure == "stop";

        for step in &payload.steps {
            let result = execute_workflow_step(
                step,
                &step_outputs,
                Some(&handoff_context),
                &self.workflow_engine,
                &self.prompt_pipeline,
                &self.mcp_client,
                &payload.workflow_id,
                lane,
            )
            .await;

            let failed = result.status == "failed";
            step_results.push(result.clone());
            if let Some(output) = result.output.clone() {
                step_outputs.insert(result.id.clone(), output.clone());
                if let Some(handoff_obj) = handoff_context.as_object_mut() {
                    handoff_obj.insert(result.id.clone(), output);
                }
            }
            if failed && stop_on_failure {
                break;
            }
        }

        step_results
    }
}

async fn execute_workflow_step(
    step: &WorkflowStepSpec,
    prior_outputs: &HashMap<String, Value>,
    handoff: Option<&Value>,
    workflow_engine: &Arc<dyn WorkflowEngine>,
    prompt_pipeline: &PromptExecutionPipeline,
    mcp_client: &McpGatewayClient,
    workflow_id: &str,
    lane: McpTurnLane,
) -> WorkflowStepResult {
    if let WorkflowStepSpec::ToolReplay {
        id,
        tool_name,
        input,
        requires_confirm,
        ..
    } = step
    {
        if *requires_confirm {
            return WorkflowStepResult {
                id: id.clone(),
                kind: "tool_replay".to_string(),
                status: "failed".to_string(),
                output: None,
                error: Some(
                    "tool replay step requires operator confirmation after secret redaction"
                        .to_string(),
                ),
            };
        }
        let entry = crate::tool_history_index::ToolHistoryRunEntry {
            entry_id: id.clone(),
            session_id: String::new(),
            slice_id: String::new(),
            turn_index: 0,
            tool_round: 0,
            run_id: id.clone(),
            tool_name: tool_name.clone(),
            status: "succeeded".to_string(),
            input_summary: input
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            sanitized_input: input.clone(),
            args_hash: String::new(),
            redacted: false,
            output_preview: None,
            timestamp: Utc::now(),
            session_preview: None,
        };
        let (native_step, _) = crate::tool_history_index::promote_run_to_step(&entry, id);
        if matches!(native_step, WorkflowStepSpec::ToolReplay { .. }) {
            return WorkflowStepResult {
                id: id.clone(),
                kind: "tool_replay".to_string(),
                status: "failed".to_string(),
                output: None,
                error: Some(format!(
                    "tool replay for '{tool_name}' is not executable without editing the flow step"
                )),
            };
        }
        return execute_native_workflow_step(
            &native_step,
            prior_outputs,
            handoff,
            workflow_engine,
            prompt_pipeline,
            mcp_client,
            workflow_id,
            lane,
        )
        .await;
    }

    execute_native_workflow_step(
        step,
        prior_outputs,
        handoff,
        workflow_engine,
        prompt_pipeline,
        mcp_client,
        workflow_id,
        lane,
    )
    .await
}

async fn execute_native_workflow_step(
    step: &WorkflowStepSpec,
    prior_outputs: &HashMap<String, Value>,
    handoff: Option<&Value>,
    workflow_engine: &Arc<dyn WorkflowEngine>,
    prompt_pipeline: &PromptExecutionPipeline,
    mcp_client: &McpGatewayClient,
    workflow_id: &str,
    lane: McpTurnLane,
) -> WorkflowStepResult {
    match step {
        WorkflowStepSpec::Grapheme { id, source } => {
            let rendered_source = apply_step_refs(source, prior_outputs, handoff);
            match workflow_engine
                .execute_grapheme_source(&rendered_source, None)
                .await
            {
                Ok(output) => WorkflowStepResult {
                    id: id.clone(),
                    kind: "grapheme".to_string(),
                    status: "succeeded".to_string(),
                    output: Some(json!({
                        "execution": output.execution,
                        "final_state": output.final_state,
                        "run_id": output.run_id,
                    })),
                    error: None,
                },
                Err(error) => WorkflowStepResult {
                    id: id.clone(),
                    kind: "grapheme".to_string(),
                    status: "failed".to_string(),
                    output: None,
                    error: Some(error.to_string()),
                },
            }
        }
        WorkflowStepSpec::Prompt {
            id,
            user_prompt,
            system_prompt,
        } => {
            let rendered_prompt = apply_step_refs(user_prompt, prior_outputs, handoff);
            let mut request = PromptExecutionRequest::from_user_prompt(rendered_prompt);
            if let Some(system_prompt) = system_prompt.as_deref() {
                request = request.with_system_prompt(apply_step_refs(
                    system_prompt,
                    prior_outputs,
                    handoff,
                ));
            }
            match prompt_pipeline.execute(request).await {
                Ok(response) => WorkflowStepResult {
                    id: id.clone(),
                    kind: "prompt".to_string(),
                    status: "succeeded".to_string(),
                    output: Some(json!({ "text": response.text })),
                    error: None,
                },
                Err(error) => WorkflowStepResult {
                    id: id.clone(),
                    kind: "prompt".to_string(),
                    status: "failed".to_string(),
                    output: None,
                    error: Some(error.to_string()),
                },
            }
        }
        WorkflowStepSpec::Mcp {
            id,
            server_id,
            tool_name,
            args,
            effect_class: _,
        } => {
            let turn_context = McpTurnContext {
                turn_id: format!("wf-{workflow_id}-{}", Uuid::new_v4().simple()),
                session_id: workflow_id.to_string(),
                user_id: identity_memory::resolve_identity_user_id(None),
                channel_id: identity_memory::resolve_identity_channel_id(Some(
                    if lane == McpTurnLane::Scheduled {
                        "scheduled"
                    } else {
                        "interactive"
                    },
                )),
                lane,
                policy_profile: Some(if lane == McpTurnLane::Scheduled {
                    "scheduled".to_string()
                } else {
                    "interactive".to_string()
                }),
            };
            let turn_token = match mint_mcp_turn_token(&turn_context) {
                Ok(Some(token)) => Some(token),
                Ok(None) => None,
                Err(error) => {
                    return WorkflowStepResult {
                        id: id.clone(),
                        kind: "mcp".to_string(),
                        status: "failed".to_string(),
                        output: None,
                        error: Some(error.to_string()),
                    };
                }
            };
            let invoke_request = McpInvokeRequest {
                server_id: server_id.clone(),
                tool_name: tool_name.clone(),
                input: args.clone(),
                turn_context,
                turn_token,
                operator_approval_granted: None,
            };
            match mcp_client.invoke(&invoke_request).await {
                Ok(response) => WorkflowStepResult {
                    id: id.clone(),
                    kind: "mcp".to_string(),
                    status: if response.ok { "succeeded" } else { "failed" }.to_string(),
                    output: Some(json!(response)),
                    error: if response.ok {
                        None
                    } else {
                        response.error.map(|err| err.message)
                    },
                },
                Err(error) => WorkflowStepResult {
                    id: id.clone(),
                    kind: "mcp".to_string(),
                    status: "failed".to_string(),
                    output: None,
                    error: Some(error.to_string()),
                },
            }
        }
        WorkflowStepSpec::ToolReplay { id, .. } => WorkflowStepResult {
            id: id.clone(),
            kind: "tool_replay".to_string(),
            status: "failed".to_string(),
            output: None,
            error: Some("unexpected nested tool replay step".to_string()),
        },
    }
}

pub async fn preflight_grapheme_steps(
    runtime: &stasis::prelude::RuntimeComposition,
    steps: &[WorkflowStepSpec],
) -> stasis::prelude::Result<Vec<Value>> {
    let runtime = Arc::new(runtime.clone());
    let mut results = Vec::new();
    for step in steps {
        if let WorkflowStepSpec::Grapheme { id, source } = step {
            let validation = validate_grapheme_source_for_schedule(&runtime, source).await?;
            results.push(json!({ "step_id": id, "validation": validation }));
        }
    }
    Ok(results)
}

pub struct WorkflowEnqueueContinuation<'a> {
    pub turn_scope: &'a TurnContinuationScope,
    pub tool_name: &'a str,
    pub await_mode: ContinuationAwaitMode,
}

pub async fn enqueue_workflow_job(
    runtime: &RuntimeComposition,
    payload: &MedousaWorkflowPayload,
    queue: &str,
    continuation: Option<WorkflowEnqueueContinuation<'_>>,
) -> stasis::prelude::Result<String> {
    let job_type = workflow_job_type_for_strategy(&payload.strategy).ok_or_else(|| {
        stasis::prelude::StasisError::PortFailure(format!(
            "unsupported workflow strategy '{}'",
            payload.strategy
        ))
    })?;

    let job_id = format!("wf-job-{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let mut job = NewJob {
        id: job_id.clone(),
        queue: queue.to_string(),
        job_type: job_type.to_string(),
        payload_ref: encode_workflow_payload(payload)?,
        priority: 100,
        max_attempts: 1,
        idempotency_key: format!("idem-{job_id}"),
        correlation_id: payload.workflow_id.clone(),
        causation_id: "cognition_runtime_workflow".to_string(),
        trace_id: payload.workflow_id.clone(),
        sttp_input_node_id: format!("sttp:in:workflow:{}", payload.workflow_id),
        scheduled_at: now,
        backoff_policy: BackoffPolicy::default(),
    };

    if let Some(ctx) = continuation {
        wire_turn_child_job(
            &mut job,
            ctx.turn_scope,
            ctx.tool_name,
            job_type,
            ctx.await_mode,
        )
        .await;
    }

    match runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
    }
    Ok(job_id)
}

pub async fn enqueue_sequential_workflow_job(
    runtime: &RuntimeComposition,
    payload: &MedousaWorkflowPayload,
    queue: &str,
) -> stasis::prelude::Result<String> {
    enqueue_workflow_job(runtime, payload, queue, None).await
}

pub fn attach_workflow_handler(
    builder: stasis::prelude::StasisRuntimeBuilder,
    prompt_pipeline: PromptExecutionPipeline,
    registry: Arc<WorkflowRegistry>,
) -> stasis::prelude::StasisRuntimeBuilder {
    let executor = WorkflowExecutor::with_defaults(registry.clone(), prompt_pipeline.clone());
    builder
        .with_extra_handler(MedousaSequentialWorkflowHandler::new(executor.clone()))
        .with_extra_handler(MedousaConcurrentWorkflowHandler::new(executor.clone()))
        .with_extra_handler(MedousaHandoffWorkflowHandler::new(executor))
}

/// Register Medousa workflow job handlers on an already-open runtime composition.
pub fn register_workflow_job_handlers<R>(
    registrar: &R,
    registry: Arc<WorkflowRegistry>,
    prompt_pipeline: PromptExecutionPipeline,
) -> stasis::prelude::Result<()>
where
    R: WorkflowHandlerRegistrar,
{
    let executor = WorkflowExecutor::with_defaults(registry, prompt_pipeline);
    registrar.register_handler(MedousaSequentialWorkflowHandler::new(executor.clone()))?;
    registrar.register_handler(MedousaConcurrentWorkflowHandler::new(executor.clone()))?;
    registrar.register_handler(MedousaHandoffWorkflowHandler::new(executor))
}

pub trait WorkflowHandlerRegistrar {
    fn register_handler<H: JobHandler + 'static>(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()>;
}

impl WorkflowHandlerRegistrar
    for stasis::application::runtime::in_memory_runtime::InMemoryRuntime
{
    fn register_handler<H: JobHandler + 'static>(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()> {
        self.register_handler(handler)
    }
}

impl WorkflowHandlerRegistrar for stasis::application::runtime::surreal_runtime::SurrealRuntime {
    fn register_handler<H: JobHandler + 'static>(
        &self,
        handler: H,
    ) -> stasis::prelude::Result<()> {
        self.register_handler(handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_duplicate_step_ids() {
        let request = WorkflowRunRequest {
            name: None,
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            steps: vec![
                WorkflowStepSpec::Grapheme {
                    id: "a".to_string(),
                    source: "query {}".to_string(),
                },
                WorkflowStepSpec::Prompt {
                    id: "a".to_string(),
                    user_prompt: "hello".to_string(),
                    system_prompt: None,
                },
            ],
            on_failure: "stop".to_string(),
            note: None,
            queue: None,
        };
        assert!(validate_workflow_request(&request).is_err());
    }

    #[test]
    fn apply_step_refs_substitutes_output() {
        let mut outputs = HashMap::new();
        outputs.insert("fetch".to_string(), json!("csv-data"));
        let rendered = apply_step_refs("process {{ $steps.fetch.output }}", &outputs, None);
        assert!(rendered.contains("csv-data"));
    }

    #[test]
    fn encode_decode_roundtrip() {
        let payload = MedousaWorkflowPayload {
            workflow_id: "wf-test".to_string(),
            name: Some("demo".to_string()),
            strategy: "sequential".to_string(),
            mode: "default".to_string(),
            on_failure: "stop".to_string(),
            note: None,
            lane: "interactive".to_string(),
            steps: vec![WorkflowStepSpec::Prompt {
                id: "one".to_string(),
                user_prompt: "hello".to_string(),
                system_prompt: None,
            }],
        };
        let encoded = encode_workflow_payload(&payload).expect("encode");
        let decoded = decode_workflow_payload(&encoded).expect("decode");
        assert_eq!(decoded.workflow_id, "wf-test");
        assert_eq!(decoded.steps.len(), 1);
    }

    #[test]
    fn validate_concurrent_strategy_rejects_step_refs() {
        let request = WorkflowRunRequest {
            name: None,
            strategy: "concurrent".to_string(),
            mode: "default".to_string(),
            steps: vec![WorkflowStepSpec::Prompt {
                id: "b".to_string(),
                user_prompt: "after $steps.a.output".to_string(),
                system_prompt: None,
            }],
            on_failure: "stop".to_string(),
            note: None,
            queue: None,
        };
        assert!(validate_workflow_request(&request).is_err());
    }

    #[test]
    fn workflow_job_type_maps_strategies() {
        assert_eq!(
            workflow_job_type_for_strategy("concurrent"),
            Some(WORKFLOW_CONCURRENT_JOB_TYPE)
        );
        assert_eq!(
            workflow_job_type_for_strategy("handoff"),
            Some(WORKFLOW_HANDOFF_JOB_TYPE)
        );
    }

    #[test]
    fn apply_handoff_context_substitution() {
        let handoff = json!({ "a": { "text": "hello" } });
        let rendered = apply_step_refs("ctx=$handoff.context", &HashMap::new(), Some(&handoff));
        assert!(rendered.contains("hello"));
    }
}
