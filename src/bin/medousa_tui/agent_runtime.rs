use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use stasis::application::orchestration::tool_loop_pipeline::ToolCallMode;
use tokio::sync::mpsc;

use medousa::{
    InteractiveTurnRequest, InteractiveTurnStreamEvent, TuiRuntime,
    turn_continuation::TurnContinuationScope,
    agent_runtime::{
        prompt_prep,
        stream_sink::AgentStreamSink,
        turn_orchestrator::{
            self, LocalTurnExecutionParams, PrepareTurnPromptParams, DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS,
            DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS, DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD,
            DEFAULT_COLD_WINDOW_TURNS, DEFAULT_HOT_WINDOW_TURNS, DEFAULT_RETRY_RUNTIME_MAX_RETRIES,
            DEFAULT_RETRY_RUNTIME_MAX_ROUNDS, MAX_COLD_WINDOW_TURNS, MAX_HOT_WINDOW_TURNS,
            MIN_COLD_WINDOW_TURNS, MIN_HOT_WINDOW_TURNS,
        },
        turn_services::{
            self, IntentContextLimits, PriorMessageLimits,
        },
    },
    events::TuiEvent,
    payload_receipt::ArtifactReceiptMeta,
};
use serde_json::Value;

use super::daemon_commands::daemon_start_interactive_turn;
use super::{ConversationTurn, TuiState};

const INTENT_CLASSIFIER_MAX_CONTEXT_TURNS: usize = 4;
const INTENT_CLASSIFIER_MAX_CONTEXT_CHARS: usize = 1400;
const INTENT_CLASSIFIER_CONTEXT_LINE_CHARS: usize = 260;

struct TuiStreamSink {
    tx: mpsc::Sender<TuiEvent>,
}

#[async_trait]
impl AgentStreamSink for TuiStreamSink {
    async fn content_chunk(&self, turn_id: u64, delta: String) {
        let _ = self
            .tx
            .send(TuiEvent::AgentChunk { turn_id, delta })
            .await;
    }

    async fn reasoning_chunk(&self, turn_id: u64, delta: String) {
        let _ = self
            .tx
            .send(TuiEvent::AgentReasoningChunk { turn_id, delta })
            .await;
    }

    async fn agent_worker_ack(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        let _ = self
            .tx
            .send(TuiEvent::AgentResponse {
                turn_id,
                text,
                tool_names,
                terminal: false,
            })
            .await;
    }

    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        let _ = self
            .tx
            .send(TuiEvent::AgentResponse {
                turn_id,
                text,
                tool_names,
                terminal: true,
            })
            .await;
    }

    async fn agent_error(&self, turn_id: u64, message: String) {
        let _ = self
            .tx
            .send(TuiEvent::AgentError { turn_id, message })
            .await;
    }

    async fn notice(&self, message: String) {
        let _ = self.tx.send(TuiEvent::UiNotice(message)).await;
    }

    async fn tool_invoked(&self, tool_name: String, input_summary: String) {
        let _ = self
            .tx
            .send(TuiEvent::ToolInvoked {
                tool_name,
                input_summary,
            })
            .await;
    }

    async fn tool_payload(
        &self,
        tool_name: String,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
    ) {
        let _ = self
            .tx
            .send(TuiEvent::ToolPayload {
                tool_name,
                tool_input,
                tool_output,
                input_receipt,
                output_receipt,
            })
            .await;
    }

    async fn scratch_reset(&self, turn_id: u64) {
        let _ = self
            .tx
            .send(TuiEvent::AgentScratchReset { turn_id })
            .await;
    }
}

pub(crate) async fn start_prompt_run(
    state: &mut TuiState,
    tui_rt: &TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
    prompt: String,
    persist_user_turn: bool,
) {
    if !state.local_runtime_only {
        match attempt_daemon_interactive_turn(state, &prompt, persist_user_turn).await {
            Ok(response) => {
                if let Some(notice) = response.daemon_notice {
                    super::push_obs(state, format!("◈ {notice}"));
                }

                if response.fallback_to_local || !response.stream_ready {
                    super::push_obs(
                        state,
                        format!(
                            "◈ interactive turn fallback local turn_id={} reason={} stream_ready={}",
                            response.turn_id,
                            response
                                .fallback_reason
                                .unwrap_or_else(|| "daemon_stream_not_ready".to_string()),
                            response.stream_ready,
                        ),
                    );
                } else {
                    super::push_obs(
                        state,
                        format!(
                            "◈ interactive turn accepted daemon turn_id={} stream={}",
                            response.turn_id, response.stream_url
                        ),
                    );
                    start_daemon_stream_prompt_run(
                        state,
                        event_tx,
                        &prompt,
                        persist_user_turn,
                        &response.turn_id,
                        &response.stream_url,
                    )
                    .await;
                    return;
                }
            }
            Err(err) => {
                super::push_obs(
                    state,
                    format!(
                        "◈ interactive turn daemon unavailable; using local runtime ({})",
                        prompt_prep::truncate_text_for_budget(&err, 180)
                    ),
                );
            }
        }
    } else {
        super::push_obs(
            state,
            "◈ local-runtime-only — using in-process agent runtime".to_string(),
        );
    }

    state.active_agent_turn_id = state.active_agent_turn_id.saturating_add(1);
    let turn_id = state.active_agent_turn_id;
    state.open_stream_turn_id = Some(turn_id);
    state.is_processing = true;
    state.auto_scroll = true;
    state.conv_scroll = state.conv_max_scroll;
    state.active_agent_stream_turn = None;
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    state.in_thinking_tag = false;
    state.stream_tag_tail.clear();
    state.received_native_reasoning = false;

    if persist_user_turn {
        let user_turn = ConversationTurn {
            role: "user".to_string(),
            content: prompt.clone(),
            timestamp: chrono::Utc::now(),
            tool_names: vec![],
            answer_state: None,
        };
        let session_id = state.session_id.clone();
        super::history_services::append_turn_daemon_first(state, &session_id, &user_turn).await;
        state.conversation.push(user_turn);
    }

    let final_route = state.stage_routing.get("final_response").cloned();
    let verifier_route = state.stage_routing.get("verifier").cloned();

    if let Some(route) = &final_route {
        super::push_obs(
            state,
            format!(
                "◈ stage route final_response target={}:{} policy={} fallback={}",
                route.provider,
                route.model,
                route.policy_profile,
                route.fallback_chain.join(","),
            ),
        );
    }
    if let Some(route) = &verifier_route {
        super::push_obs(
            state,
            format!(
                "◈ stage route verifier target={}:{} policy={} fallback={}",
                route.provider,
                route.model,
                route.policy_profile,
                route.fallback_chain.join(","),
            ),
        );
    }

    let prepared = turn_orchestrator::prepare_turn_prompt(PrepareTurnPromptParams {
        session_id: &state.session_id,
        prompt: &prompt,
        selected_context_pack_query: state.selected_context_pack_query.as_deref(),
        settings: &state.settings,
        verifier_route: verifier_route.as_ref(),
        final_route: final_route.as_ref(),
        response_depth_mode: &state.response_depth_mode,
        tui_rt,
    })
    .await;

    if let Some(err) = &prepared.recall_probe.error {
        super::push_obs(state, format!("◈ cheap_recall error={err}"));
    } else if prepared.recall_probe.attempted {
        super::push_obs(
            state,
            format!(
                "◈ cheap_recall retrieved={} path={} fallback={} fallback_reason={} keys={} snippets={}",
                prepared.recall_probe.retrieved,
                prepared.recall_probe.retrieval_path.as_deref().unwrap_or("n/a"),
                prepared.recall_probe.fallback_triggered,
                prepared.recall_probe.fallback_reason.as_deref().unwrap_or("none"),
                prepared.recall_probe.node_sync_keys.len(),
                prepared.recall_probe.snippets.len(),
            ),
        );
    }

    if let Some(err) = &prepared.identity_probe.error {
        super::push_obs(state, format!("◈ identity_context error={err}"));
    } else if let Some(summary) = &prepared.identity_probe.summary {
        super::push_obs(
            state,
            format!(
                "◈ identity_context loaded summary={}",
                prompt_prep::truncate_text_for_budget(summary, 180)
            ),
        );
    }

    state.pending_response_verified = prepared.verification_state;
    super::push_obs(state, format!("◈ {}", prepared.compiler_output.compiler_summary));

    if let Some(note) = &prepared.pack_note {
        super::push_obs(state, note.clone());
    }

    let prompt_len_before_budget = prepared.resolved_prompt.chars().count();
    let resolved_prompt = prompt_prep::truncate_text_for_budget(
        &prepared.resolved_prompt,
        prompt_prep::MAX_REQUEST_PROMPT_CHARS,
    );
    let prompt_len_after_budget = resolved_prompt.chars().count();
    if prompt_len_after_budget < prompt_len_before_budget {
        super::push_obs(
            state,
            format!(
                "◈ prompt budget applied chars={} -> {}",
                prompt_len_before_budget, prompt_len_after_budget
            ),
        );
    }

    let pipeline_selection =
        turn_services::select_pipeline_for_turn(tui_rt, final_route.as_ref(), &state.settings);
    if let Some(route_notice) = pipeline_selection.route_dispatch_notice {
        super::push_obs(state, route_notice);
    }
    let pipeline = pipeline_selection.pipeline;
    let tx = event_tx.clone();
    let prompt_preview: String = resolved_prompt.chars().take(48).collect();
    let configured_tool_call_mode = turn_services::parse_tool_call_mode(&state.settings.tool_call_mode);
    let turn_loop_settings =
        medousa::agent_runtime::TurnLoopSettings::from_runtime_settings(&state.settings);
    let activation = turn_services::decide_turn_activation(
        &prompt,
        configured_tool_call_mode,
        turn_loop_settings.configured_max_tool_rounds,
        turn_loop_settings.activation_tool_intent_max_rounds,
        turn_loop_settings.activation_short_turn_max_tool_rounds,
        state.conversation.len(),
        medousa::tui::settings::parse_usize_with_bounds(
            &state.settings.activation_direct_answer_max_prompt_chars,
            DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS,
            64,
            4000,
        ),
        medousa::tui::settings::parse_usize_with_bounds(
            &state.settings.activation_long_session_turn_threshold,
            DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD,
            8,
            500,
        ),
        medousa::tui::settings::parse_usize_with_bounds(
            &state.settings.activation_long_session_max_prompt_chars,
            DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS,
            64,
            4000,
        ),
    );
    let activation = turn_services::apply_context_compiler_activation_gate(
        activation,
        prepared.compiler_output.allow_no_tools_fallback,
    );
    let hot_window_turns = medousa::tui::settings::parse_usize_with_bounds(
        &state.settings.slice_hot_window_turns,
        DEFAULT_HOT_WINDOW_TURNS,
        MIN_HOT_WINDOW_TURNS,
        MAX_HOT_WINDOW_TURNS,
    );
    let cold_window_turns = medousa::tui::settings::parse_usize_with_bounds(
        &state.settings.slice_cold_window_turns,
        DEFAULT_COLD_WINDOW_TURNS,
        MIN_COLD_WINDOW_TURNS,
        MAX_COLD_WINDOW_TURNS,
    )
    .max(hot_window_turns);
    let prior_build = build_prior_messages(
        &state.conversation,
        &prompt,
        persist_user_turn,
        hot_window_turns,
        cold_window_turns,
    );
    super::push_obs(
        state,
        format!(
            "◈ turn_loop_limits {}",
            turn_loop_settings.operator_summary()
        ),
    );
    super::push_obs(
        state,
        format!(
            "◈ activation heuristic class={} mode={} rounds={} no_tools={} reason={}",
            activation.turn_class,
            match activation.tool_call_mode {
                ToolCallMode::Auto => "auto",
                ToolCallMode::Strict => "strict",
            },
            activation.max_tool_rounds,
            activation.enforce_no_tools,
            activation.reason,
        ),
    );
    super::push_obs(
        state,
        format!(
            "◈ turn slicing hot_turns={} cold_turns={} cold_chars={} prior_chars={}",
            prior_build.hot_turns_included,
            prior_build.cold_turns_summarized,
            prior_build.cold_summary_chars,
            prior_build.total_chars,
        ),
    );
    let prior_messages = prior_build.messages;
    let prompt_for_request = if activation.enforce_no_tools {
        format!(
            "{resolved_prompt}\n\n[MEDOUSA_TOOL_POLICY]\nmode=no_tools\ninstruction=Do not call tools for this turn unless the user explicitly requests external lookup, execution, or fresh evidence. Answer directly from current context."
        )
    } else {
        medousa::agent_runtime::turn_ledger::append_tool_loop_policy(
            &resolved_prompt,
            activation.max_tool_rounds,
        )
    };
    let retry_max_retries = medousa::tui::settings::parse_usize_with_bounds(
        &state.settings.retry_runtime_max_retries,
        DEFAULT_RETRY_RUNTIME_MAX_RETRIES,
        0,
        5,
    );
    let retry_max_rounds = medousa::tui::settings::parse_usize_with_bounds(
        &state.settings.retry_runtime_max_rounds,
        DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
        1,
        100,
    );
    let no_tools_pipeline =
        turn_services::build_prompt_pipeline_for_turn(final_route.as_ref(), &state.settings);
    let intent_classifier_recent_context = turn_services::build_intent_classifier_recent_context(
        &state.conversation,
        &prompt,
        persist_user_turn,
        INTENT_CLASSIFIER_MAX_CONTEXT_TURNS,
        INTENT_CLASSIFIER_MAX_CONTEXT_CHARS,
        IntentContextLimits {
            context_line_chars: INTENT_CLASSIFIER_CONTEXT_LINE_CHARS,
        },
    );
    let original_prompt_for_continuation = prompt.clone();
    let continuation_response_depth_mode = state.response_depth_mode.clone();
    let continuation_stage_route = final_route.clone();
    let continuation_recall_readiness = prepared.recall_readiness;
    let sink: Arc<dyn AgentStreamSink> = Arc::new(TuiStreamSink { tx: tx.clone() });
    let turn_scope = tui_rt.turn_scope.clone();
    let worker_scheduler = tui_rt.worker_scheduler.clone();
    let tool_registry = tui_rt.tool_registry.clone();
    let session_id = state.session_id.clone();
    let backend = state.settings.backend.clone();
    let provider = state.settings.provider.clone();
    let model = state.settings.model.clone();
    let base_url = (!state.settings.base_url.trim().is_empty())
        .then(|| state.settings.base_url.clone());
    let response_depth_mode = state.response_depth_mode.clone();
    let handle = tokio::spawn(async move {
        let previous_scope = turn_scope.read().await.clone();
        *turn_scope.write().await = Some(TurnContinuationScope {
            turn_correlation_id: format!("tui-turn-{turn_id}"),
            session_id: session_id.clone(),
            original_prompt: original_prompt_for_continuation.clone(),
            delivery_target: None,
            provider: provider.clone(),
            model: model.clone(),
            response_depth_mode: continuation_response_depth_mode.clone(),
        });

        turn_orchestrator::execute_local_turn(
            sink,
            LocalTurnExecutionParams {
                turn_id,
                session_id,
                backend,
                provider,
                model,
                base_url,
                response_depth_mode,
                worker_scheduler,
                tool_registry,
                turn_scope: turn_scope.clone(),
                activation,
                pipeline,
                no_tools_pipeline,
                prior_messages,
                prompt_for_request,
                original_prompt: original_prompt_for_continuation,
                intent_classifier_recent_context,
                retry_max_retries,
                retry_max_rounds,
                continuation_response_depth_mode,
                continuation_stage_route,
                continuation_recall_readiness,
                prompt_preview,
                turn_loop_settings,
            },
        )
        .await;

        *turn_scope.write().await = previous_scope;
    });

    state.active_request_task = Some(handle);
}

async fn attempt_daemon_interactive_turn(
    state: &TuiState,
    prompt: &str,
    persist_user_turn: bool,
) -> std::result::Result<medousa::InteractiveTurnResponse, String> {
    let request = InteractiveTurnRequest {
        session_id: state.session_id.clone(),
        prompt: prompt.to_string(),
        persist_user_turn,
        response_depth_mode: state.response_depth_mode.clone(),
        provider: state.settings.provider.clone(),
        model: state.settings.model.clone(),
        stage_routing: state.stage_routing.clone(),
        max_tool_rounds: Some(medousa::tui::settings::parse_usize_with_bounds(
            &state.settings.max_tool_rounds,
            10,
            1,
            50,
        )),
        retry_runtime_max_rounds: Some(medousa::tui::settings::parse_usize_with_bounds(
            &state.settings.retry_runtime_max_rounds,
            medousa::agent_runtime::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
            1,
            10,
        )),
    };

    daemon_start_interactive_turn(&state.daemon_url, &request)
        .await
        .map_err(|err| err.to_string())
}

async fn start_daemon_stream_prompt_run(
    state: &mut TuiState,
    event_tx: &mpsc::Sender<TuiEvent>,
    prompt: &str,
    persist_user_turn: bool,
    daemon_turn_id: &str,
    stream_url: &str,
) {
    state.active_agent_turn_id = state.active_agent_turn_id.saturating_add(1);
    let turn_id = state.active_agent_turn_id;
    state.open_stream_turn_id = Some(turn_id);
    state.is_processing = true;
    state.auto_scroll = true;
    state.conv_scroll = state.conv_max_scroll;
    state.active_agent_stream_turn = None;
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    state.in_thinking_tag = false;
    state.stream_tag_tail.clear();
    state.received_native_reasoning = false;
    state.pending_response_verified = None;

    if persist_user_turn {
        let user_turn = ConversationTurn {
            role: "user".to_string(),
            content: prompt.to_string(),
            timestamp: chrono::Utc::now(),
            tool_names: vec![],
            answer_state: None,
        };
        let session_id = state.session_id.clone();
        super::history_services::append_turn_daemon_first(state, &session_id, &user_turn).await;
        state.conversation.push(user_turn);
    }

    let tx = event_tx.clone();
    let stream_url = stream_url.to_string();
    let daemon_turn_id = daemon_turn_id.to_string();
    let handle = tokio::spawn(async move {
        if let Err(err) = consume_daemon_interactive_stream(&stream_url, turn_id, &tx).await {
            let _ = tx
                .send(TuiEvent::AgentError {
                    turn_id,
                    message: format!(
                        "daemon interactive stream {} failed: {}",
                        daemon_turn_id,
                        prompt_prep::truncate_text_for_budget(&err, 220)
                    ),
                })
                .await;
        }
    });

    state.active_request_task = Some(handle);
}

async fn consume_daemon_interactive_stream(
    stream_url: &str,
    turn_id: u64,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> std::result::Result<(), String> {
    let client = reqwest::Client::new();
    let response = client
        .get(stream_url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;

    let mut bytes = response.bytes_stream();
    let mut buffer = String::new();
    let mut saw_terminal = false;

    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let text = String::from_utf8_lossy(&chunk).to_string();
        buffer.push_str(&text);

        while let Some(idx) = buffer.find("\n\n") {
            let frame = buffer[..idx].to_string();
            buffer = buffer[idx + 2..].to_string();

            let Some(payload) = parse_daemon_stream_payload(&frame) else {
                continue;
            };

            if dispatch_daemon_stream_event(payload, turn_id, event_tx).await? {
                saw_terminal = true;
            }
        }
    }

    if !saw_terminal {
        return Err("stream closed without terminal event".to_string());
    }

    Ok(())
}

fn parse_daemon_stream_payload(frame: &str) -> Option<InteractiveTurnStreamEvent> {
    let data = frame
        .lines()
        .filter_map(|line| {
            if let Some(value) = line.strip_prefix("data: ") {
                Some(value)
            } else if let Some(value) = line.strip_prefix("data:") {
                Some(value.trim_start())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if data.trim().is_empty() {
        return None;
    }

    serde_json::from_str::<InteractiveTurnStreamEvent>(&data).ok()
}

async fn dispatch_daemon_stream_event(
    payload: InteractiveTurnStreamEvent,
    turn_id: u64,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> std::result::Result<bool, String> {
    match payload.event_type.as_str() {
        "content_delta" => {
            if let Some(delta) = payload.content_delta {
                event_tx
                    .send(TuiEvent::AgentChunk { turn_id, delta })
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }
        "reasoning_delta" => {
            if let Some(delta) = payload.reasoning_delta {
                event_tx
                    .send(TuiEvent::AgentReasoningChunk { turn_id, delta })
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }
        "final" => {
            let text = payload
                .final_text
                .or_else(|| {
                    if payload.message.trim().is_empty() {
                        None
                    } else {
                        Some(payload.message.clone())
                    }
                })
                .unwrap_or_else(|| "(empty daemon final response)".to_string());
            let tool_names = payload.tool_names.unwrap_or_default();
            event_tx
                .send(TuiEvent::AgentResponse {
                    turn_id,
                    text,
                    tool_names,
                    terminal: payload.terminal,
                })
                .await
                .map_err(|err| err.to_string())?;
        }
        "error" => {
            let message = if payload.message.trim().is_empty() {
                "daemon interactive stream failed".to_string()
            } else {
                payload.message
            };
            event_tx
                .send(TuiEvent::AgentError { turn_id, message })
                .await
                .map_err(|err| err.to_string())?;
        }
        _ => {
            if !payload.message.trim().is_empty() {
                event_tx
                    .send(TuiEvent::UiNotice(format!(
                        "◈ daemon interactive {} {}",
                        payload.phase, payload.message
                    )))
                    .await
                    .map_err(|err| err.to_string())?;
            }
        }
    }

    Ok(payload.terminal)
}

fn build_prior_messages(
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    hot_window_turns: usize,
    cold_window_turns: usize,
) -> turn_services::PriorMessageBuild {
    turn_services::build_prior_messages(
        turns,
        current_prompt,
        current_user_persisted,
        hot_window_turns,
        cold_window_turns,
        PriorMessageLimits {
            max_prior_total_chars: turn_orchestrator::MAX_PRIOR_TOTAL_CHARS,
            max_single_prior_message_chars: turn_orchestrator::MAX_SINGLE_PRIOR_MESSAGE_CHARS,
            hot_window_char_budget: turn_orchestrator::HOT_WINDOW_CHAR_BUDGET,
            cold_window_char_budget: turn_orchestrator::COLD_WINDOW_CHAR_BUDGET,
            cold_summary_line_chars: turn_orchestrator::COLD_SUMMARY_LINE_CHARS,
        },
    )
}

pub(crate) fn stop_active_generation(state: &mut TuiState) {
    if let Some(task) = state.active_request_task.take() {
        task.abort();
        state.is_processing = false;
        state.open_stream_turn_id = None;
        state.active_agent_stream_turn = None;
        state.pending_response_verified = None;
        state.pending_agent_chunk_delta.clear();
        state.pending_agent_chunk_count = 0;
        super::flush_thinking_buffer(state);
        super::push_obs(state, "■ generation stopped".to_string());
    }
}
