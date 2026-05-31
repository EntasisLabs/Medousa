use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::daemon_api::{InteractiveTurnRequest, InteractiveTurnStreamEvent};
use crate::interactive_turn_runtime;
use crate::payload_receipt::ArtifactReceiptMeta;
use crate::session::{ConversationTurn, append_turn, load_history};

use super::prompt_prep::{truncate_text_for_budget, MAX_REQUEST_PROMPT_CHARS};
use super::settings::runtime_settings_for_interactive_turn;
use super::stream_sink::AgentStreamSink;
use super::stream_sink::SharedAgentStreamSink;
use super::turn_orchestrator::{self, AssembleLocalTurnParams, PrepareTurnPromptParams};

struct InteractiveTurnStreamSink {
    turn_id: String,
    session_id: String,
    stream_tx: broadcast::Sender<InteractiveTurnStreamEvent>,
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
    }

    async fn agent_error(&self, _turn_id: u64, message: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::error_stream_event(&self.turn_id, &message),
        );
    }

    async fn notice(&self, message: String) {
        publish(
            &self.stream_tx,
            interactive_turn_runtime::status_stream_event(&self.turn_id, "orchestration", &message),
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

/// Run a full agent turn for `POST /v1/interactive/turn`, streaming via SSE.
pub async fn run_daemon_interactive_turn(
    turn_id: &str,
    request: InteractiveTurnRequest,
    backend: &str,
    agent_rt: &super::runtime::MedousaAgentRuntime,
    stream_tx: broadcast::Sender<InteractiveTurnStreamEvent>,
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
    let prompt = request.prompt.trim().to_string();
    if session_id.is_empty() || prompt.is_empty() {
        publish(
            &stream_tx,
            interactive_turn_runtime::error_stream_event(turn_id, "session_id and prompt are required"),
        );
        return;
    }

    let settings = runtime_settings_for_interactive_turn(backend, &request);
    let final_route = request.stage_routing.get("final_response").cloned();
    let verifier_route = request.stage_routing.get("verifier").cloned();

    if let Some(route) = final_route.as_ref() {
        publish(
            &stream_tx,
            interactive_turn_runtime::status_stream_event(
                turn_id,
                "routing",
                &format!(
                    "final_response route target={}:{} policy={} fallback={}",
                    route.provider,
                    route.model,
                    route.policy_profile,
                    route.fallback_chain.join(",")
                ),
            ),
        );
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

    let prepared = turn_orchestrator::prepare_turn_prompt(PrepareTurnPromptParams {
        session_id: &session_id,
        prompt: &prompt,
        selected_context_pack_query: None,
        settings: &settings,
        verifier_route: verifier_route.as_ref(),
        final_route: final_route.as_ref(),
        response_depth_mode: &request.response_depth_mode,
        tui_rt: agent_rt,
    })
    .await;

    if let Some(err) = &prepared.recall_probe.error {
        publish(
            &stream_tx,
            interactive_turn_runtime::status_stream_event(
                turn_id,
                "recall",
                &format!("cheap_recall error={err}"),
            ),
        );
    } else if prepared.recall_probe.attempted {
        publish(
            &stream_tx,
            interactive_turn_runtime::status_stream_event(
                turn_id,
                "recall",
                &format!(
                    "cheap_recall retrieved={} path={} keys={}",
                    prepared.recall_probe.retrieved,
                    prepared
                        .recall_probe
                        .retrieval_path
                        .as_deref()
                        .unwrap_or("n/a"),
                    prepared.recall_probe.node_sync_keys.len(),
                ),
            ),
        );
    }

    if let Some(summary) = &prepared.identity_probe.summary {
        publish(
            &stream_tx,
            interactive_turn_runtime::status_stream_event(
                turn_id,
                "identity",
                &truncate_text_for_budget(
                    &format!("identity_context loaded summary={summary}"),
                    180,
                ),
            ),
        );
    }

    publish(
        &stream_tx,
        interactive_turn_runtime::status_stream_event(
            turn_id,
            "compiler",
            &prepared.compiler_output.compiler_summary,
        ),
    );

    if let Some(note) = &prepared.pack_note {
        publish(
            &stream_tx,
            interactive_turn_runtime::status_stream_event(turn_id, "context_pack", note),
        );
    }

    let resolved_prompt = truncate_text_for_budget(&prepared.resolved_prompt, MAX_REQUEST_PROMPT_CHARS);
    let assembled = turn_orchestrator::assemble_local_turn(AssembleLocalTurnParams {
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
    });

    if let Some(route_notice) = assembled.pipeline_selection.route_dispatch_notice {
        publish(
            &stream_tx,
            interactive_turn_runtime::status_stream_event(turn_id, "routing", &route_notice),
        );
    }

    publish(
        &stream_tx,
        interactive_turn_runtime::status_stream_event(
            turn_id,
            "activation",
            &format!(
                "class={} mode={} rounds={} no_tools={} reason={}",
                assembled.activation.turn_class,
                match assembled.activation.tool_call_mode {
                    stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Auto => {
                        "auto"
                    }
                    stasis::application::orchestration::tool_loop_pipeline::ToolCallMode::Strict => {
                        "strict"
                    }
                },
                assembled.activation.max_tool_rounds,
                assembled.activation.enforce_no_tools,
                assembled.activation.reason,
            ),
        ),
    );

    publish(
        &stream_tx,
        interactive_turn_runtime::status_stream_event(
            turn_id,
            "slicing",
            &format!(
                "hot_turns={} cold_turns={} cold_chars={} prior_chars={}",
                assembled.prior_build.hot_turns_included,
                assembled.prior_build.cold_turns_summarized,
                assembled.prior_build.cold_summary_chars,
                assembled.prior_build.total_chars,
            ),
        ),
    );

    let sink: SharedAgentStreamSink = Arc::new(InteractiveTurnStreamSink {
        turn_id: turn_id.to_string(),
        session_id: session_id.clone(),
        stream_tx: stream_tx.clone(),
    });

    turn_orchestrator::execute_local_turn(sink, assembled.execution).await;
}
