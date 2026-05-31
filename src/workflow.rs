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

use crate::identity_memory;
use crate::mcp_gateway_api::{McpInvokeRequest, McpTurnContext, McpTurnLane};
use crate::mcp_gateway_client::McpGatewayClient;
use crate::mcp_turn_token::mint_mcp_turn_token;
use crate::tools::validate_grapheme_source_for_schedule;

pub const WORKFLOW_SEQUENTIAL_JOB_TYPE: &str = "workflow.medousa.sequential";
pub const WORKFLOW_PAYLOAD_PREFIX: &str = "medousa:workflow:";
pub const MAX_WORKFLOW_STEPS: usize = 20;

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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStepSpec {
    Grapheme {
        id: String,
        source: String,
    },
    Prompt {
        id: String,
        user_prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
    },
    Mcp {
        id: String,
        server_id: String,
        tool_name: String,
        #[serde(default)]
        args: Value,
    },
}

impl WorkflowStepSpec {
    pub fn id(&self) -> &str {
        match self {
            Self::Grapheme { id, .. } | Self::Prompt { id, .. } | Self::Mcp { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    pub steps: Vec<WorkflowStepSpec>,
    #[serde(default = "default_on_failure")]
    pub on_failure: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
}

fn default_strategy() -> String {
    "sequential".to_string()
}

fn default_mode() -> String {
    "default".to_string()
}

fn default_on_failure() -> String {
    "stop".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedousaSequentialWorkflowPayload {
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
}

pub fn encode_workflow_payload(payload: &MedousaSequentialWorkflowPayload) -> stasis::prelude::Result<String> {
    let raw = serde_json::to_string(payload).map_err(|error| {
        stasis::prelude::StasisError::PortFailure(format!(
            "failed to encode workflow payload: {error}"
        ))
    })?;
    Ok(format!("{WORKFLOW_PAYLOAD_PREFIX}{raw}"))
}

pub fn decode_workflow_payload(payload_ref: &str) -> stasis::prelude::Result<MedousaSequentialWorkflowPayload> {
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
    if request.strategy != "sequential" {
        return Err(stasis::prelude::StasisError::PortFailure(format!(
            "unsupported workflow strategy '{}'; v1 supports sequential only",
            request.strategy
        )));
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

pub fn apply_step_refs(template: &str, outputs: &HashMap<String, Value>) -> String {
    let mut result = template.to_string();
    for (step_id, output) in outputs {
        let needle = format!("$steps.{step_id}.output");
        let replacement = output_to_string(output);
        result = result.replace(&needle, &replacement);
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
    workflow_engine: Arc<dyn WorkflowEngine>,
    prompt_pipeline: PromptExecutionPipeline,
    mcp_client: Arc<McpGatewayClient>,
    registry: Arc<WorkflowRegistry>,
}

impl MedousaSequentialWorkflowHandler {
    pub fn new(
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

    pub fn with_defaults(registry: Arc<WorkflowRegistry>, prompt_pipeline: PromptExecutionPipeline) -> Self {
        Self::new(
            RuntimeFactory::default_workflow_engine(),
            prompt_pipeline,
            Arc::new(McpGatewayClient::from_env()),
            registry,
        )
    }
}

#[async_trait]
impl JobHandler for MedousaSequentialWorkflowHandler {
    fn job_type(&self) -> &'static str {
        WORKFLOW_SEQUENTIAL_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> stasis::prelude::Result<JobExecutionOutcome> {
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

        let mut step_outputs: HashMap<String, Value> = HashMap::new();
        let mut step_results = Vec::new();
        let stop_on_failure = payload.on_failure == "stop";

        for step in &payload.steps {
            let result = execute_workflow_step(
                step,
                &step_outputs,
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
}

async fn execute_workflow_step(
    step: &WorkflowStepSpec,
    prior_outputs: &HashMap<String, Value>,
    workflow_engine: &Arc<dyn WorkflowEngine>,
    prompt_pipeline: &PromptExecutionPipeline,
    mcp_client: &McpGatewayClient,
    workflow_id: &str,
    lane: McpTurnLane,
) -> WorkflowStepResult {
    match step {
        WorkflowStepSpec::Grapheme { id, source } => {
            let rendered_source = apply_step_refs(source, prior_outputs);
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
            let rendered_prompt = apply_step_refs(user_prompt, prior_outputs);
            let mut request = PromptExecutionRequest::from_user_prompt(rendered_prompt);
            if let Some(system_prompt) = system_prompt.as_deref() {
                request = request.with_system_prompt(apply_step_refs(system_prompt, prior_outputs));
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

pub async fn enqueue_sequential_workflow_job(
    runtime: &RuntimeComposition,
    payload: &MedousaSequentialWorkflowPayload,
    queue: &str,
) -> stasis::prelude::Result<String> {
    let job_id = format!("wf-job-{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let job = NewJob {
        id: job_id.clone(),
        queue: queue.to_string(),
        job_type: WORKFLOW_SEQUENTIAL_JOB_TYPE.to_string(),
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

    match runtime {
        RuntimeComposition::InMemory(rt) => rt.enqueue(job).await?,
        RuntimeComposition::Surreal(rt) => rt.enqueue(job).await?,
    }
    Ok(job_id)
}

pub fn attach_workflow_handler(
    builder: stasis::prelude::StasisRuntimeBuilder,
    prompt_pipeline: PromptExecutionPipeline,
    registry: Arc<WorkflowRegistry>,
) -> stasis::prelude::StasisRuntimeBuilder {
    builder.with_extra_handler(MedousaSequentialWorkflowHandler::with_defaults(
        registry,
        prompt_pipeline,
    ))
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
        let rendered = apply_step_refs("process {{ $steps.fetch.output }}", &outputs);
        assert!(rendered.contains("csv-data"));
    }

    #[test]
    fn encode_decode_roundtrip() {
        let payload = MedousaSequentialWorkflowPayload {
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
}
