//! Heartbeat agent turn — proactive operator message via MedousaAgentRuntime.
//!
//! Phase 5: replace ad-hoc heartbeat summary strings with policy-guided agent output.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::engine_context::default_policy_profile_for_lane;
use crate::session_mapping::build_interactive_turn_request_for_ingest;
use crate::stage_routing::StageRoutingMatrix;

use super::runtime::MedousaAgentRuntime;
use super::stream_sink::AgentStreamSink;

/// Runtime snapshot passed into the heartbeat agent turn prompt.
#[derive(Debug, Clone)]
pub struct HeartbeatRuntimeSnapshot {
    pub significance: f32,
    pub reason: String,
    pub failed_jobs: usize,
    pub dead_letter_jobs: usize,
    pub pending_outbox_events: usize,
    pub materialized_jobs: usize,
    pub processed_job: Option<String>,
    pub published_events: usize,
}

pub fn heartbeat_agent_turn_enabled() -> bool {
    std::env::var("MEDOUSA_HEARTBEAT_AGENT_TURN_ENABLED")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn heartbeat_policy_doc_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("HEARTBEAT.md")
}

pub fn load_heartbeat_policy_doc() -> Option<String> {
    let path = heartbeat_policy_doc_path();
    std::fs::read_to_string(path).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn build_heartbeat_turn_prompt(snapshot: &HeartbeatRuntimeSnapshot) -> String {
    let policy_doc = load_heartbeat_policy_doc().unwrap_or_else(|| {
        "Review runtime health signals. If action is needed, recommend concrete next steps \
         using read-only observability tools (jobs list, delivery status). \
         Do not register recurring jobs or enqueue durable work on heartbeat lane. \
         Keep the message concise and operator-friendly."
            .to_string()
    });

    let heartbeat_surface = crate::daemon_api::TurnSurfaceContext {
        channel_surface: Some("heartbeat".to_string()),
        channel_id: None,
        user_id: None,
        supports_ui_artifacts: false,
        supports_liquid_markdown: false,
        supports_browser_host: false,
    };
    let ambient = super::ambient_context::build_ambient_context(
        super::ambient_context::AmbientContextInput {
            session_id: "heartbeat",
            surface: Some(&heartbeat_surface),
            channel_policy: None,
        },
    );

    format!(
        "You are running a scheduled heartbeat check for the Medousa operator.\n\n\
         {}\n\n\
         ## HEARTBEAT policy\n{policy_doc}\n\n\
         ## Runtime signals (now)\n\
         significance={:.2}\n\
         reason={}\n\
         failed_jobs={}\n\
         dead_letter_jobs={}\n\
         pending_outbox_events={}\n\
         materialized_jobs={}\n\
         processed_job={}\n\
         published_events={}\n\n\
         Write a brief proactive status message for the operator. \
         Use read-only tools only if you need fresh queue stats.",
        ambient.appendix,
        snapshot.significance,
        snapshot.reason,
        snapshot.failed_jobs,
        snapshot.dead_letter_jobs,
        snapshot.pending_outbox_events,
        snapshot.materialized_jobs,
        snapshot.processed_job.as_deref().unwrap_or("none"),
        snapshot.published_events,
    )
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
        *self.output.lock().await = Some(format!("heartbeat agent turn failed: {message}"));
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

/// Run a heartbeat agent turn and return the assistant message text, if any.
pub async fn run_heartbeat_agent_turn(
    snapshot: &HeartbeatRuntimeSnapshot,
    backend: &str,
    provider: &str,
    model: &str,
    response_depth_mode: &str,
    agent_rt: &MedousaAgentRuntime,
) -> Option<String> {
    let prompt = build_heartbeat_turn_prompt(snapshot);
    let session_id = format!(
        "daemon-heartbeat-{}",
        default_policy_profile_for_lane(crate::engine_context::EngineExecutionLane::Heartbeat)
    );
    let mut request = build_interactive_turn_request_for_ingest(
        &session_id,
        prompt,
        provider,
        model,
        response_depth_mode,
        crate::reasoning_effort::REASONING_EFFORT_DEFAULT,
        None,
        None,
        None,
        None,
    );
    request.persist_user_turn = false;
    request.stage_routing = StageRoutingMatrix::default_for(provider, model);

    let output = Arc::new(Mutex::new(None));
    let sink: Arc<dyn AgentStreamSink> = Arc::new(CapturingAgentSink {
        output: output.clone(),
    });

    let identity_user_id = crate::user_profiles::resolve_workshop_identity_user_id();
    let scope = crate::turn_continuation::TurnContinuationScope {
        turn_correlation_id: "heartbeat-agent-turn".to_string(),
        session_id: session_id.clone(),
        identity_user_id: Some(identity_user_id.clone()),
        original_prompt: request.prompt.clone(),
        delivery_target: None,
        provider: request.provider.clone(),
        model: request.model.clone(),
        response_depth_mode: request.response_depth_mode.clone(),
        supports_ui_artifacts: false,
        supports_liquid_markdown: false,
        supports_browser_host: false,
        channel_surface: Some("heartbeat".to_string()),
    };
    let execution = super::execution_context::TurnExecutionContext::from_scope(
        "heartbeat-agent-turn",
        crate::request_principal::RequestPrincipal::continuation(identity_user_id),
        tokio_util::sync::CancellationToken::new(),
        std::time::Instant::now() + std::time::Duration::from_secs(2 * 60 * 60),
        scope,
    )
    .ok()?;
    let execution_lease = agent_rt.execution_registry.admit(execution).ok()?;
    let execution_context = execution_lease.context().clone();

    super::run_agent_turn(
        "heartbeat-agent-turn",
        request,
        backend,
        agent_rt,
        sink,
        None,
        execution_context,
        None,
        None,
    )
    .await;

    drop(execution_lease);

    output.lock().await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_prompt_includes_signals() {
        let prompt = build_heartbeat_turn_prompt(&HeartbeatRuntimeSnapshot {
            significance: 0.82,
            reason: "dead_letter_detected dead_letter_jobs=1".to_string(),
            failed_jobs: 2,
            dead_letter_jobs: 1,
            pending_outbox_events: 5,
            materialized_jobs: 0,
            processed_job: None,
            published_events: 0,
        });
        assert!(prompt.contains("dead_letter_jobs=1"));
        assert!(prompt.contains("HEARTBEAT policy"));
    }
}
