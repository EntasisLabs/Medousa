//! Durable recurring jobs that run a full Medousa agent turn (tool loop) per tick.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use stasis::application::runtime::in_memory_runtime::{JobExecutionOutcome, JobHandler};
use stasis::domain::runtime::job::Job;
use stasis::ports::outbound::memory::memory_models::MemoryStoreRequest;
use stasis::prelude::{Result as StasisResult, RuntimeComposition, StasisError};
use tokio::sync::Mutex;

use crate::agent_runtime::{run_agent_turn, stream_sink::AgentStreamSink};
use crate::daemon_api::InteractiveTurnRequest;
use crate::engine_context::{
    EngineExecutionLane, compile_default_lane_prompt, default_policy_profile_for_lane,
};
use crate::identity_manuscript::{
    build_manuscript_context, manuscript_wants_locus_store_on_complete,
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
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub scheduled_tool_allowlist: Vec<String>,
    #[serde(default)]
    pub max_tool_rounds: Option<usize>,
}

impl RecurringAgentTurnJobPayload {
    pub fn to_payload_ref(&self) -> StasisResult<String> {
        serde_json::to_string(self).map_err(|err| {
            StasisError::PortFailure(format!("failed to encode recurring agent-turn payload: {err}"))
        })
    }
}

pub fn manuscript_id_from_recurring_payload(
    job_type: &str,
    payload_template_ref: &str,
) -> Option<String> {
    if job_type != RECURRING_AGENT_TURN_JOB_TYPE {
        return None;
    }
    serde_json::from_str::<RecurringAgentTurnJobPayload>(payload_template_ref)
        .ok()?
        .manuscript_id
        .filter(|value| !value.trim().is_empty())
}

pub fn build_recurring_agent_turn_payload(
    prompt: &str,
    session_id: &str,
    system_prompt: Option<String>,
    policy_profile: Option<String>,
    model_hint: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    manuscript_id: Option<String>,
    scheduled_tool_allowlist: Vec<String>,
    max_tool_rounds: Option<usize>,
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
        manuscript_id,
        scheduled_tool_allowlist,
        max_tool_rounds,
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

        let manuscript_id = payload.manuscript_id.clone();
        eprintln!(
            "medousa recurring_tick job_id={} manuscript={} session_id={}",
            job.id,
            manuscript_id.as_deref().unwrap_or("-"),
            payload.session_id,
        );

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
            session_id: payload.session_id.clone(),
            prompt: payload.user_prompt.clone(),
            persist_user_turn: false,
            response_depth_mode: payload
                .response_depth_mode
                .clone()
                .unwrap_or_else(|| "standard".to_string()),
            provider,
            model,
            stage_routing,
            surface: None,
            max_tool_rounds: payload.max_tool_rounds,
            retry_runtime_max_rounds: None,
            manuscript_id: manuscript_id.clone(),
            additional_manuscript_ids: None,
            suggested_capability_ids: None,
            scheduled_tool_allowlist: if payload.scheduled_tool_allowlist.is_empty() {
                None
            } else {
                Some(payload.scheduled_tool_allowlist.clone())
            },
            media_refs: Vec::new(),
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

        if let Some(manuscript_id) = manuscript_id.as_deref() {
            if let Ok(manuscript) = build_manuscript_context(manuscript_id) {
                if manuscript_wants_locus_store_on_complete(&manuscript) {
                    if let Err(err) =
                        store_manuscript_brief_to_locus(self.agent.as_ref(), &manuscript, &text)
                            .await
                    {
                        eprintln!(
                            "medousa recurring_locus_store manuscript={manuscript_id} error={err}"
                        );
                    }
                }
            }
        }

        let diagnostics = json!({
            "provider": "medousa-agent-runtime",
            "status": "success",
            "job_type": RECURRING_AGENT_TURN_JOB_TYPE,
            "output_text": text,
            "policy_profile": payload.policy_profile,
            "model_hint": payload.model_hint,
            "manuscript_id": manuscript_id,
        })
        .to_string();

        Ok(JobExecutionOutcome::Success {
            sttp_output_node_id: format!("sttp:out:recurring-agent:{}", job.id),
            execution_id: None,
            diagnostics: Some(diagnostics),
        })
    }
}

async fn store_manuscript_brief_to_locus(
    agent: &TuiRuntime,
    manuscript: &crate::identity_manuscript::ManuscriptContext,
    brief_text: &str,
) -> anyhow::Result<()> {
    let session_id = manuscript
        .locus_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("manuscript locus session_id is missing"))?;
    let summary = brief_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let summary = if summary.is_empty() {
        format!("{} brief", manuscript.name)
    } else {
        summary
    };
    let node = build_scheduled_brief_sttp_node(session_id, &manuscript.id, &summary);
    let response = agent
        .memory_writer
        .store_context(&MemoryStoreRequest {
            session_id: session_id.to_string(),
            raw_node: node,
        })
        .await?;
    if !response.valid {
        anyhow::bail!(
            response
                .validation_error
                .unwrap_or_else(|| "locus store rejected brief node".to_string())
        );
    }
    eprintln!(
        "medousa recurring_locus_store manuscript={} session_id={} node_id={}",
        manuscript.id, session_id, response.node_id
    );
    Ok(())
}

fn build_scheduled_brief_sttp_node(session_id: &str, manuscript_id: &str, summary: &str) -> String {
    let timestamp = Utc::now().to_rfc3339();
    let escaped_summary = summary.replace('"', "\\\"");
    format!(
        "⊕⟨ ⏣0{{ trigger: recurring_tick, response_format: temporal_node, origin_session: \"{session_id}\", compression_depth: 1, parent_node: null, prime: {{ attractor_config: {{ stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85 }}, context_summary: \"{escaped_summary}\", relevant_tier: raw, retrieval_budget: 8 }} }} ⟩\n\
         ⦿⟨ ⏣0{{ timestamp: \"{timestamp}\", tier: raw, session_id: \"{session_id}\", schema_version: \"sttp-1.0\", user_avec: {{ stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 }}, model_avec: {{ stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 }} }} ⟩\n\
         ◈⟨ ⏣0{{ focus(.99): \"{manuscript_id} brief\", decision(.96): {{ summary(.95): \"{escaped_summary}\" }} }} ⟩\n\
         ⍉⟨ ⏣0{{ rho: 0.95, kappa: 0.94, psi: 2.93, compression_avec: {{ stability: 0.90, friction: 0.20, logic: 0.98, autonomy: 0.85, psi: 2.93 }} }} ⟩"
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_roundtrips_manuscript_metadata() {
        let payload = build_recurring_agent_turn_payload(
            "Produce today's brief.",
            "recurring-morning-brief",
            None,
            None,
            None,
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
            Some("morning-brief".to_string()),
            vec![
                "cognition_identity_recall".to_string(),
                "cognition_memory_context".to_string(),
            ],
            Some(8),
        );
        let encoded = payload.to_payload_ref().expect("encode");
        let decoded: RecurringAgentTurnJobPayload =
            serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded.manuscript_id.as_deref(), Some("morning-brief"));
        assert_eq!(decoded.scheduled_tool_allowlist.len(), 2);
        assert_eq!(
            manuscript_id_from_recurring_payload(RECURRING_AGENT_TURN_JOB_TYPE, &encoded)
                .as_deref(),
            Some("morning-brief")
        );
    }

    #[test]
    fn manuscript_id_extractor_ignores_other_job_types() {
        assert!(manuscript_id_from_recurring_payload(
            "workflow.stasis.prompt",
            r#"{"manuscript_id":"x"}"#
        )
        .is_none());
    }
}
