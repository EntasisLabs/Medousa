use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

use crate::channel_delivery::{
    ChannelDeliveryTarget, JobDeliveryRecord, JobDeliveryState,
};
use crate::daemon_api::{InteractiveTurnRequest, InteractiveTurnStreamEvent};
use crate::interactive_turn_runtime;
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::session::{ConversationTurn, append_turn, load_history};

use crate::turn_continuation::{TurnContinuationScope, TurnOutcome, turn_continuation_store};

use super::prompt_prep::{truncate_text_for_budget, MAX_REQUEST_PROMPT_CHARS};
use super::settings::runtime_settings_for_interactive_turn;
use super::stream_sink::AgentStreamSink;
use super::stream_sink::SharedAgentStreamSink;
use super::turn_orchestrator::{self, AssembleLocalTurnParams, PrepareTurnPromptParams};

/// Delivery registry hooks for interactive turns (mirrors ingest `channel_deliveries` pattern).
#[derive(Clone)]
pub struct InteractiveTurnDeliveryContext {
    pub turn_key: String,
    pub delivery_records: Arc<RwLock<HashMap<String, JobDeliveryRecord>>>,
    pub channel_deliveries: Arc<RwLock<HashMap<String, ChannelDeliveryTarget>>>,
    pub last_turn_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub last_turn_latency_ms: Arc<RwLock<Option<u64>>>,
    pub started: Instant,
}

impl InteractiveTurnDeliveryContext {
    pub async fn mark_complete(&self, error: Option<String>) {
        let latency_ms = self.started.elapsed().as_millis() as u64;
        let now = Utc::now();
        self.delivery_records.write().await.insert(
            self.turn_key.clone(),
            JobDeliveryRecord {
                state: JobDeliveryState::Delivered,
                delivered_at: Some(now),
                error,
                latency_ms: Some(latency_ms),
            },
        );
        *self.last_turn_at.write().await = Some(now);
        *self.last_turn_latency_ms.write().await = Some(latency_ms);
        self.channel_deliveries.write().await.remove(&self.turn_key);
    }
}

struct InteractiveTurnStreamSink {
    turn_id: String,
    session_id: String,
    stream_tx: broadcast::Sender<InteractiveTurnStreamEvent>,
    delivery: Option<InteractiveTurnDeliveryContext>,
}

#[async_trait]
impl AgentStreamSink for InteractiveTurnStreamSink {
    async fn content_chunk(&self, _turn_id: u64, delta: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::content_delta_stream_event(&self.turn_id, &delta),
        );
    }

    async fn reasoning_chunk(&self, _turn_id: u64, delta: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::reasoning_delta_stream_event(&self.turn_id, &delta),
        );
    }

    async fn agent_worker_ack(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        let assistant_turn = ConversationTurn {
            role: "assistant".to_string(),
            content: text.clone(),
            timestamp: Utc::now(),
            tool_names: tool_names.clone(),
            answer_state: None,
        };
        append_turn(&self.session_id, &assistant_turn);

        publish(
            &self.stream_tx,
            interactive_turn_runtime::worker_ack_stream_event_with_tools(
                &self.turn_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_response(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        let assistant_turn = ConversationTurn {
            role: "assistant".to_string(),
            content: text.clone(),
            timestamp: Utc::now(),
            tool_names: tool_names.clone(),
            answer_state: None,
        };
        append_turn(&self.session_id, &assistant_turn);

        publish(
            &self.stream_tx,
            interactive_turn_runtime::final_stream_event_with_tools(
                &self.turn_id,
                &text,
                tool_names,
            ),
        );

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_needs_input(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        let assistant_turn = ConversationTurn {
            role: "assistant".to_string(),
            content: text.clone(),
            timestamp: Utc::now(),
            tool_names: tool_names.clone(),
            answer_state: Some("needs_input".to_string()),
        };
        append_turn(&self.session_id, &assistant_turn);

        publish(
            &self.stream_tx,
            interactive_turn_runtime::needs_input_stream_event_with_tools(
                &self.turn_id,
                &text,
                tool_names,
            ),
        );

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(None).await;
        }
    }

    async fn agent_final_pending(&self, _turn_id: u64, text: String, tool_names: Vec<String>) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::final_pending_stream_event_with_tools(
                &self.turn_id,
                &text,
                tool_names,
            ),
        );
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::error_stream_event(&self.turn_id, &message),
        );

        if let Some(delivery) = &self.delivery {
            delivery.mark_complete(Some(message)).await;
        }
    }

    async fn notice(&self, message: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::status_stream_event(&self.turn_id, "orchestration", &message),
        );
    }

    async fn scratch_reset(&self, _turn_id: u64) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::scratch_reset_stream_event(&self.turn_id),
        );
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::status_stream_event(
                &self.turn_id,
                "tool",
                &format!("tool={tool_name} {input_summary}"),
            ),
        );
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<ArtifactReceiptMeta>,
        _output_receipt: Option<ArtifactReceiptMeta>,
    ) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::status_stream_event(
                &self.turn_id,
                "tool",
                &format!("tool_payload={tool_name}"),
            ),
        );
    }
}

fn publish(
    stream_tx: &broadcast::Sender<InteractiveTurnStreamEvent>,
    event: anyhow::Result<InteractiveTurnStreamEvent>,
) {
    if let Ok(payload) = event {
        let _ = stream_tx.send(payload);
    }
}

/// Run a full agent turn, streaming events through the provided sink.
pub async fn run_agent_turn(
    _turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    sink: SharedAgentStreamSink,
    continuation_scope: Option<TurnContinuationScope>,
) {
    let previous_scope = agent_rt.turn_scope.read().await.clone();
    if let Some(scope) = continuation_scope.clone() {
        *agent_rt.turn_scope.write().await = Some(scope);
    }

    let turn_correlation_id = continuation_scope.as_ref().map(|scope| scope.turn_correlation_id.clone());
    let outcome: Arc<RwLock<Option<TurnOutcome>>> = Arc::new(RwLock::new(None));
    let tracking_sink: SharedAgentStreamSink = Arc::new(TurnOutcomeTrackingSink {
        inner: sink,
        outcome: outcome.clone(),
    });

    run_agent_turn_inner(
        _turn_id,
        request,
        backend,
        agent_rt,
        tracking_sink,
    )
    .await;

    if let Some(correlation_id) = turn_correlation_id {
        let final_outcome = outcome
            .read()
            .await
            .unwrap_or(TurnOutcome::Error);
        let _ = turn_continuation_store()
            .mark_turn_finished(&correlation_id, final_outcome)
            .await;
    }

    *agent_rt.turn_scope.write().await = previous_scope;
}

async fn run_agent_turn_inner(
    _turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    sink: SharedAgentStreamSink,
) {

    let session_id = request.session_id.trim().to_string();
    let prompt = request.prompt.trim().to_string();
    if session_id.is_empty() || prompt.is_empty() {
        sink.agent_error(1, "session_id and prompt are required".to_string())
            .await;
        return;
    }

    let settings = runtime_settings_for_interactive_turn(backend, &request);
    let final_route = request.stage_routing.get("final_response").cloned();
    let verifier_route = request.stage_routing.get("verifier").cloned();

    if let Some(route) = final_route.as_ref() {
        sink.notice(format!(
            "◈ stage route final_response target={}:{} policy={} fallback={}",
            route.provider,
            route.model,
            route.policy_profile,
            route.fallback_chain.join(","),
        ))
        .await;
    }

    let mut conversation = load_history(&session_id);
    if request.persist_user_turn {
        let user_turn = ConversationTurn {
            role: "user".to_string(),
            content: prompt.clone(),
            timestamp: Utc::now(),
            tool_names: vec![],
            answer_state: None,
        };
        append_turn(&session_id, &user_turn);
        conversation.push(user_turn);
    }

    let manuscript_id = request
        .manuscript_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let scheduled_tool_allowlist = request
        .scheduled_tool_allowlist
        .as_ref()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| tool.trim().to_string())
                .filter(|tool| !tool.is_empty())
                .collect::<std::collections::HashSet<_>>()
        })
        .filter(|tools| !tools.is_empty())
        .or_else(|| {
            manuscript_id.and_then(|id| {
                crate::identity_manuscript::build_manuscript_context(id)
                    .ok()
                    .map(|ctx| crate::identity_manuscript::scheduled_tool_allowlist_for_manuscript(&ctx))
            })
        });

    if let Some(manuscript_id) = manuscript_id {
        sink.notice(format!("◈ manuscript_load id={manuscript_id} lane=scheduled"))
            .await;
        if let Some(allowlist) = scheduled_tool_allowlist.as_ref() {
            sink.notice(format!(
                "◈ manuscript_tools allowed={} lane=scheduled",
                allowlist.len()
            ))
            .await;
        }
    }

    let prepared = turn_orchestrator::prepare_turn_prompt(PrepareTurnPromptParams {
        session_id: &session_id,
        prompt: &prompt,
        selected_context_pack_query: None,
        settings: &settings,
        verifier_route: verifier_route.as_ref(),
        final_route: final_route.as_ref(),
        response_depth_mode: &request.response_depth_mode,
        surface: request.surface.as_ref(),
        tui_rt: agent_rt,
        manuscript_id,
    })
    .await;

    if let Some(err) = &prepared.recall_probe.error {
        sink.notice(format!("◈ cheap_recall error={err}")).await;
    } else if prepared.recall_probe.attempted {
        sink.notice(format!(
            "◈ cheap_recall retrieved={} path={} keys={}",
            prepared.recall_probe.retrieved,
            prepared
                .recall_probe
                .retrieval_path
                .as_deref()
                .unwrap_or("n/a"),
            prepared.recall_probe.node_sync_keys.len(),
        ))
        .await;
    }

    if let Some(summary) = &prepared.identity_probe.summary {
        sink.notice(format!(
            "◈ identity_context loaded summary={}",
            truncate_text_for_budget(summary, 180)
        ))
        .await;
    }

    sink.notice(format!("◈ {}", prepared.compiler_output.compiler_summary))
        .await;

    if let Some(note) = &prepared.pack_note {
        sink.notice(note.clone()).await;
    }

    let resolved_prompt = truncate_text_for_budget(&prepared.resolved_prompt, MAX_REQUEST_PROMPT_CHARS);
    let assembled = turn_orchestrator::assemble_local_turn(AssembleLocalTurnParams {
        session_id: &session_id,
        settings: &settings,
        conversation: &conversation,
        prompt: &prompt,
        persist_user_turn: request.persist_user_turn,
        prepared: &prepared,
        resolved_prompt,
        tui_rt: agent_rt,
        final_route: final_route.as_ref(),
        response_depth_mode: &request.response_depth_mode,
        turn_id: 1,
        scheduled_tool_allowlist,
    });

    if let Some(route_notice) = assembled.pipeline_selection.route_dispatch_notice {
        sink.notice(route_notice).await;
    }

    sink.notice(format!(
        "◈ activation heuristic class={} mode={} rounds={} no_tools={} reason={}",
        assembled.activation.turn_class,
        match assembled.activation.tool_call_mode {
            stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Auto => "auto",
            stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Strict => {
                "strict"
            }
        },
        assembled.activation.max_tool_rounds,
        assembled.activation.enforce_no_tools,
        assembled.activation.reason,
    ))
    .await;

    sink.notice(format!(
        "◈ turn slicing hot_turns={} cold_turns={} cold_chars={} prior_chars={}",
        assembled.prior_build.hot_turns_included,
        assembled.prior_build.cold_turns_summarized,
        assembled.prior_build.cold_summary_chars,
        assembled.prior_build.total_chars,
    ))
    .await;

    turn_orchestrator::execute_local_turn(sink, assembled.execution).await;
}

struct TurnOutcomeTrackingSink {
    inner: SharedAgentStreamSink,
    outcome: Arc<RwLock<Option<TurnOutcome>>>,
}

#[async_trait]
impl AgentStreamSink for TurnOutcomeTrackingSink {
    async fn content_chunk(&self, turn_id: u64, delta: String) {
        self.inner.content_chunk(turn_id, delta).await;
    }

    async fn reasoning_chunk(&self, turn_id: u64, delta: String) {
        self.inner.reasoning_chunk(turn_id, delta).await;
    }

    async fn agent_worker_ack(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.inner.agent_worker_ack(turn_id, text, tool_names).await;
    }

    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        *self.outcome.write().await = Some(TurnOutcome::Success);
        self.inner.agent_response(turn_id, text, tool_names).await;
    }

    async fn agent_needs_input(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        *self.outcome.write().await = Some(TurnOutcome::Success);
        self.inner.agent_needs_input(turn_id, text, tool_names).await;
    }

    async fn agent_final_pending(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.inner.agent_final_pending(turn_id, text, tool_names).await;
    }

    async fn agent_error(&self, turn_id: u64, message: String) {
        *self.outcome.write().await = Some(TurnOutcome::Error);
        self.inner.agent_error(turn_id, message).await;
    }

    async fn notice(&self, message: String) {
        self.inner.notice(message).await;
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        self.inner.tool_invoked(tool_name, input_summary).await;
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
    ) {
        self.inner
            .tool_payload(
                tool_name,
                tool_input,
                tool_output,
                input_receipt,
                output_receipt,
            )
            .await;
    }

    async fn scratch_reset(&self, turn_id: u64) {
        self.inner.scratch_reset(turn_id).await;
    }
}

/// Run a full agent turn for `POST /v1/interactive/turn`, streaming via SSE.
pub async fn run_daemon_interactive_turn(
    turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    stream_tx: broadcast::Sender<InteractiveTurnStreamEvent>,
    delivery: Option<InteractiveTurnDeliveryContext>,
    continuation_scope: Option<TurnContinuationScope>,
) {
    publish(
        &stream_tx,
        interactive_turn_runtime::status_stream_event(
            turn_id,
            "accepted",
            "interactive turn accepted; agent runtime started",
        ),
    );

    let session_id = request.session_id.trim().to_string();
    let sink: SharedAgentStreamSink = Arc::new(InteractiveTurnStreamSink {
        turn_id: turn_id.to_string(),
        session_id,
        stream_tx,
        delivery,
    });

    run_agent_turn(
        turn_id,
        request,
        backend,
        agent_rt,
        sink,
        continuation_scope,
    )
    .await;
}
