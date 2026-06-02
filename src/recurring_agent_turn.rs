//! Durable recurring jobs that run a full Medousa agent turn (tool loop) per tick.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::runtime::job::Job;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::Mutex;

use crate::agent_runtime::{run_agent_turn, stream_sink::AgentStreamSink};
use crate::daemon_api::InteractiveTurnRequest;
use crate::engine_context::{
    EngineExecutionLane, compile_default_lane_prompt, default_policy_profile_for_lane,
};
use crate::stage_routing::StageRoutingMatrix;
use crate::tools::TuiRuntime;

pub const RECURRING_AGENT_TURN_JOB_TYPE: &str = "workflow.medousa.recurring_agent_turn";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringAgentTurnJobPayload {
    pub user_prompt: String,
    pub system_prompt: Option<String>,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
    pub session_id: String,
    pub response_depth_mode: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

impl RecurringAgentTurnJobPayload {
    pub fn to_payload_ref(&self) -> StasisResult<String> {
        serde_json::to_string(self).map_err(|err| {
            StasisError::PortFailure(format!("failed to encode recurring agent-turn payload: {err}"))
        })
    }
}

pub fn build_recurring_agent_turn_payload(
    prompt: &str,
    session_id: &str,
    system_prompt: Option<String>,
    policy_profile: Option<String>,
    model_hint: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> RecurringAgentTurnJobPayload {
    let compiled = compile_default_lane_prompt(EngineExecutionLane::Scheduled, prompt);
    RecurringAgentTurnJobPayload {
        user_prompt: compiled,
        system_prompt,
        policy_profile: policy_profile.or_else(|| {
            Some(default_policy_profile_for_lane(EngineExecutionLane::Scheduled).to_string())
        }),
        model_hint,
        session_id: session_id.to_string(),
        response_depth_mode: Some("standard".to_string()),
        provider,
        model,
    }
}

pub async fn register_recurring_agent_turn_handler(
    composition: &RuntimeComposition,
    agent: Arc<TuiRuntime>,
    backend: String,
) -> anyhow::Result<()> {
    let handler = RecurringAgentTurnJobHandler { agent, backend };
    match composition {
        RuntimeComposition::InMemory(rt) => rt.register_handler(handler)?,
        RuntimeComposition::Surreal(rt) => rt.register_handler(handler)?,
    }
    Ok(())
}

struct RecurringAgentTurnJobHandler {
    agent: Arc<TuiRuntime>,
    backend: String,
}

struct CapturingAgentSink {
    output: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl AgentStreamSink for CapturingAgentSink {
    async fn content_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn reasoning_chunk(&self, _turn_id: u64, _delta: String) {}

    async fn agent_response(&self, _turn_id: u64, text: String, _tool_names: Vec<String>) {
        *self.output.lock().await = Some(text);
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        *self.output.lock().await = Some(format!("recurring agent turn failed: {message}"));
    }

    async fn notice(&self, _message: String) {}

    async fn tool_invoked(&self, _tool_name: String, _input_summary: String) {}

    async fn tool_payload(
        &self,
        _tool_name: String,
        _tool_input: serde_json::Value,
        _tool_output: serde_json::Value,
        _input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        _output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
    ) {
    }
}

#[async_trait]
impl JobHandler for RecurringAgentTurnJobHandler {
    fn job_type(&self) -> &'static str {
        RECURRING_AGENT_TURN_JOB_TYPE
    }

    async fn execute(&self, job: &Job) -> StasisResult<JobExecutionOutcome> {
        let payload: RecurringAgentTurnJobPayload =
            serde_json::from_str(&job.payload_ref).map_err(|err| {
                StasisError::PortFailure(format!(
                    "invalid recurring agent-turn payload for job {}: {err}",
                    job.id
                ))
            })?;

        if payload.user_prompt.trim().is_empty() {
            return Ok(fatal_outcome(
                "recurring agent-turn payload.user_prompt must be non-empty",
            ));
        }

        let provider = payload
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| crate::resolve_llm_provider(Some(value)))
            .unwrap_or_else(|| crate::resolve_llm_provider(None));
        let model = payload
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| crate::resolve_llm_model(Some(value)))
            .unwrap_or_else(|| crate::resolve_llm_model(None));

        let stage_routing = StageRoutingMatrix::default_for(&provider, &model);
        let request = InteractiveTurnRequest {
            session_id: payload.session_id,
            prompt: payload.user_prompt,
            persist_user_turn: false,
            response_depth_mode: payload
                .response_depth_mode
                .unwrap_or_else(|| "standard".to_string()),
            provider,
            model,
            stage_routing,
            max_tool_rounds: None,
            retry_runtime_max_rounds: None,
        };

        let output = Arc::new(Mutex::new(None));
        let sink: Arc<dyn AgentStreamSink> = Arc::new(CapturingAgentSink {
            output: output.clone(),
        });

        run_agent_turn(
            &job.id,
            request,
            &self.backend,
            self.agent.as_ref(),
            sink,
            None,
        )
        .await;

        let text = output.lock().await.clone().unwrap_or_else(|| {
            "recurring agent turn completed without assistant text".to_string()
        });

        let diagnostics = json!({
            "provider": "medousa-agent-runtime",
            "status": "success",
            "job_type": RECURRING_AGENT_TURN_JOB_TYPE,
            "output_text": text,
            "policy_profile": payload.policy_profile,
            "model_hint": payload.model_hint,
        })
        .to_string();

        Ok(JobExecutionOutcome::Success {
            sttp_output_node_id: format!("sttp:out:recurring-agent:{}", job.id),
            execution_id: None,
            diagnostics: Some(diagnostics),
        })
    }
}

fn fatal_outcome(message: &str) -> JobExecutionOutcome {
    JobExecutionOutcome::FatalFailure {
        message: message.to_string(),
        execution_id: None,
        diagnostics: Some(
            json!({
                "provider": "medousa-agent-runtime",
                "status": "failure",
                "output_text": message,
            })
            .to_string(),
        ),
    }
}
