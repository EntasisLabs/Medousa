use std::sync::Arc;

use genai::chat::{ChatMessage, ChatRequest};
use serde_json::Value;
use tokio::sync::mpsc;

use medousa::{
    TuiRuntime,
    engine_context::{
        ContextCompilerInput, EngineExecutionLane, RecallReadiness, compile_context_prompt,
        lane_execution_budget,
    },
    events::TuiEvent,
};
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolInvocation, ToolLoopExecutionRequest,
};
use stasis::infrastructure::llm::genai_chat_client::GenaiChatClient;
use stasis::ports::outbound::ai_chat_client::{AiChatClient, StreamDelta};
use stasis::prelude::MemoryRecallRequest;

use super::{ConversationTurn, TuiState};

const MAX_REQUEST_PROMPT_CHARS: usize = 48_000;
const MAX_PRIOR_TOTAL_CHARS: usize = 24_000;
const MAX_SINGLE_PRIOR_MESSAGE_CHARS: usize = 4_000;
const DEFAULT_HOT_WINDOW_TURNS: usize = 8;
const MIN_HOT_WINDOW_TURNS: usize = 2;
const MAX_HOT_WINDOW_TURNS: usize = 32;
const DEFAULT_COLD_WINDOW_TURNS: usize = 24;
const MIN_COLD_WINDOW_TURNS: usize = 4;
const MAX_COLD_WINDOW_TURNS: usize = 128;
const HOT_WINDOW_CHAR_BUDGET: usize = 14_000;
const COLD_WINDOW_CHAR_BUDGET: usize = 8_000;
const COLD_SUMMARY_LINE_CHARS: usize = 240;
const DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS: usize = 320;
const DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD: usize = 28;
const DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS: usize = 420;
const DEFAULT_RETRY_RUNTIME_MAX_RETRIES: usize = 1;
const DEFAULT_RETRY_RUNTIME_MAX_ROUNDS: usize = 3;
const CONTINUATION_TRIGGER_TOOL_OUTPUT_CHARS: usize = 8_000;
const CONTINUATION_TRIGGER_STDOUT_CHARS: usize = 4_000;
const CONTINUATION_MAX_DRAFT_CHARS: usize = 6_000;
const CONTINUATION_MAX_TOOL_OUTPUT_CHARS: usize = 2_000;
const CONTINUATION_MAX_TOOL_SUMMARIES: usize = 6;
const CONTINUATION_MAX_ROUNDS: usize = 4;
const INTENT_CLASSIFIER_MAX_PROMPT_CHARS: usize = 900;
const INTENT_CLASSIFIER_MAX_CONTEXT_TURNS: usize = 4;
const INTENT_CLASSIFIER_MAX_CONTEXT_CHARS: usize = 1400;
const INTENT_CLASSIFIER_CONTEXT_LINE_CHARS: usize = 260;
const INTENT_CLASSIFIER_CONFIDENCE_LOW: f32 = 0.45;
const INTENT_CLASSIFIER_CONFIDENCE_CONVERSATIONAL: f32 = 0.55;
const INTENT_CLASSIFIER_CONFIDENCE_TOOL_REQUIRED: f32 = 0.60;
const CHEAP_RECALL_LIMIT: usize = 4;
const CHEAP_RECALL_QUERY_MAX_CHARS: usize = 280;
const CHEAP_RECALL_MAX_KEYS: usize = 6;

#[derive(Debug, Clone)]
struct ContextPackQuality {
    citation_coverage: f32,
    avg_support_strength: f32,
    supported_claim_ratio: f32,
    confidence_score: f32,
    is_usable: bool,
}

#[derive(Debug, Clone)]
struct TurnActivationDecision {
    turn_class: &'static str,
    tool_call_mode: ToolCallMode,
    max_tool_rounds: usize,
    enforce_no_tools: bool,
    reason: &'static str,
}

#[derive(Debug, Clone)]
struct IntentClassification {
    intent: String,
    confidence: f32,
    reason: String,
}

#[derive(Debug, Clone, Default)]
struct CheapRecallProbe {
    attempted: bool,
    retrieved: usize,
    retrieval_path: Option<String>,
    fallback_triggered: bool,
    fallback_reason: Option<String>,
    node_sync_keys: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct PriorMessageBuild {
    messages: Vec<ChatMessage>,
    hot_turns_included: usize,
    cold_turns_summarized: usize,
    cold_summary_chars: usize,
    total_chars: usize,
}

#[derive(Debug, Clone, Default)]
struct TurnOrchestrationState {
    calls_total: usize,
    classifier_calls: usize,
    tool_loop_calls: usize,
    prompt_only_calls: usize,
    continuations: usize,
    retries: usize,
    loop_guard_tripped: bool,
    final_mode: String,
}

#[derive(Debug, Clone)]
struct TurnBudget {
    max_llm_calls_total: usize,
    max_tool_loop_calls: usize,
    max_prompt_only_calls: usize,
    max_classifier_calls: usize,
    max_retries: usize,
    max_continuations: usize,
}

pub(crate) async fn start_prompt_run(
    state: &mut TuiState,
    tui_rt: &TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
    prompt: String,
    persist_user_turn: bool,
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

    if persist_user_turn {
        let user_turn = ConversationTurn {
            role: "user".to_string(),
            content: prompt.clone(),
            timestamp: chrono::Utc::now(),
            tool_names: vec![],
            answer_state: None,
        };
        super::append_turn(&state.session_id, &user_turn);
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

    let verifier_policy =
        verifier_policy_from_settings_and_route(&state.settings, verifier_route.as_ref());
    let (mut resolved_prompt, pack_note, verification_state) = resolve_prompt_with_context_pack(
        &state.session_id,
        &prompt,
        state.selected_context_pack_query.as_deref(),
        &verifier_policy,
    );

    let recall_probe = cheap_memory_recall_probe(tui_rt, &state.session_id, &prompt).await;
    if let Some(err) = &recall_probe.error {
        super::push_obs(state, format!("◈ cheap_recall error={err}"));
    } else if recall_probe.attempted {
        super::push_obs(
            state,
            format!(
                "◈ cheap_recall retrieved={} path={} fallback={} fallback_reason={} keys={}",
                recall_probe.retrieved,
                recall_probe.retrieval_path.as_deref().unwrap_or("n/a"),
                recall_probe.fallback_triggered,
                recall_probe.fallback_reason.as_deref().unwrap_or("none"),
                recall_probe.node_sync_keys.len(),
            ),
        );
    }

    resolved_prompt = append_memory_recall_hint(&resolved_prompt, &recall_probe);
    state.pending_response_verified = Some(verification_state.unwrap_or(false));
    let recall_readiness = if verification_state.unwrap_or(false) || recall_probe.retrieved > 0 {
        RecallReadiness::Verified
    } else if verification_state == Some(false) || recall_probe.attempted {
        RecallReadiness::Unverified
    } else {
        RecallReadiness::Missing
    };
    let compiler_output = compile_context_prompt(ContextCompilerInput {
        lane: EngineExecutionLane::Interactive,
        user_prompt: &resolved_prompt,
        response_depth_mode: &state.response_depth_mode,
        stage_route: final_route.as_ref(),
        recall_readiness,
    });
    resolved_prompt = compiler_output.compiled_prompt;
    let allow_no_tools_fallback = compiler_output.allow_no_tools_fallback;
    super::push_obs(state, format!("◈ {}", compiler_output.compiler_summary));

    if let Some(note) = pack_note {
        super::push_obs(state, note);
    }

    let prompt_len_before_budget = resolved_prompt.chars().count();
    resolved_prompt = truncate_text_for_budget(&resolved_prompt, MAX_REQUEST_PROMPT_CHARS);
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

    let pipeline = if let Some(route) = &final_route {
        let route_base_url = route_base_url(route, &state.settings);
        super::push_obs(
            state,
            format!(
                "◈ stage route dispatch final_response target={}:{} base_url={}",
                route.provider,
                route.model,
                route_base_url
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("(auto)"),
            ),
        );
        tui_rt.tool_loop_pipeline_for_target(
            &route.provider,
            &route.model,
            route_base_url.as_deref(),
        )
    } else {
        tui_rt.tool_loop_pipeline.clone()
    };
    let tx = event_tx.clone();
    let prompt_preview: String = resolved_prompt.chars().take(48).collect();
    let configured_tool_call_mode = parse_tool_call_mode(&state.settings.tool_call_mode);
    let configured_max_tool_rounds =
        super::parse_usize_with_bounds(&state.settings.max_tool_rounds, 10, 1, 50);
    let activation = decide_turn_activation(
        &prompt,
        configured_tool_call_mode,
        configured_max_tool_rounds,
        state.conversation.len(),
        super::parse_usize_with_bounds(
            &state.settings.activation_direct_answer_max_prompt_chars,
            DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS,
            64,
            4000,
        ),
        super::parse_usize_with_bounds(
            &state.settings.activation_long_session_turn_threshold,
            DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD,
            8,
            500,
        ),
        super::parse_usize_with_bounds(
            &state.settings.activation_long_session_max_prompt_chars,
            DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS,
            64,
            4000,
        ),
    );
    let activation = apply_context_compiler_activation_gate(activation, allow_no_tools_fallback);
    let hot_window_turns = super::parse_usize_with_bounds(
        &state.settings.slice_hot_window_turns,
        DEFAULT_HOT_WINDOW_TURNS,
        MIN_HOT_WINDOW_TURNS,
        MAX_HOT_WINDOW_TURNS,
    );
    let cold_window_turns = super::parse_usize_with_bounds(
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
        resolved_prompt
    };
    let retry_max_retries = super::parse_usize_with_bounds(
        &state.settings.retry_runtime_max_retries,
        DEFAULT_RETRY_RUNTIME_MAX_RETRIES,
        0,
        5,
    );
    let retry_max_rounds = super::parse_usize_with_bounds(
        &state.settings.retry_runtime_max_rounds,
        DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
        1,
        10,
    );
    let no_tools_pipeline = build_prompt_pipeline_for_turn(final_route.as_ref(), &state.settings);
    let intent_classifier_recent_context = build_intent_classifier_recent_context(
        &state.conversation,
        &prompt,
        persist_user_turn,
        INTENT_CLASSIFIER_MAX_CONTEXT_TURNS,
        INTENT_CLASSIFIER_MAX_CONTEXT_CHARS,
    );
    let original_prompt_for_continuation = prompt.clone();
    let turn_budget = turn_budget_for_lane(EngineExecutionLane::Interactive);
    let handle = tokio::spawn(async move {
        let mut orchestration_state = TurnOrchestrationState {
            final_mode: "unknown".to_string(),
            ..TurnOrchestrationState::default()
        };

        let mut activation = activation;
        if should_invoke_intent_classifier(&activation) {
            if try_consume_classifier_budget(&tx, &mut orchestration_state, &turn_budget).await {
                let classification = classify_turn_intent_with_model(
                    &no_tools_pipeline,
                    &prompt,
                    &intent_classifier_recent_context,
                )
                .await;
                if let Some(classification) = classification {
                    let _ = tx
                        .send(TuiEvent::UiNotice(format!(
                            "◈ intent classifier intent={} confidence={:.2} reason={}",
                            classification.intent, classification.confidence, classification.reason
                        )))
                        .await;

                    activation = apply_intent_classifier_override(activation, &classification);
                    let _ = tx
                        .send(TuiEvent::UiNotice(format!(
                            "◈ activation final class={} mode={} rounds={} no_tools={} reason={}",
                            activation.turn_class,
                            match activation.tool_call_mode {
                                ToolCallMode::Auto => "auto",
                                ToolCallMode::Strict => "strict",
                            },
                            activation.max_tool_rounds,
                            activation.enforce_no_tools,
                            activation.reason,
                        )))
                        .await;
                } else {
                    let _ = tx
                        .send(TuiEvent::UiNotice(
                            "◈ intent classifier skipped: no parseable result; using heuristic"
                                .to_string(),
                        ))
                        .await;
                }
            } else {
                orchestration_state.final_mode = "classifier_budget_denied".to_string();
            }
        }

        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel::<StreamDelta>();
        let chunk_event_tx = tx.clone();
        tokio::spawn(async move {
            while let Some(delta) = chunk_rx.recv().await {
                let event = match delta {
                    StreamDelta::Content(delta) => TuiEvent::AgentChunk { turn_id, delta },
                    StreamDelta::Reasoning(delta) => {
                        TuiEvent::AgentReasoningChunk { turn_id, delta }
                    }
                    StreamDelta::ThoughtSignature(delta) => {
                        TuiEvent::AgentReasoningChunk { turn_id, delta }
                    }
                };
                if chunk_event_tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        let _ = tx
            .send(TuiEvent::ToolInvoked {
                tool_name: "llm.chat".to_string(),
                input_summary: prompt_preview,
            })
            .await;

        if activation.enforce_no_tools {
            let mut messages = Vec::with_capacity(prior_messages.len() + 2);
            messages.push(ChatMessage::system(super::SYSTEM_PROMPT.to_string()));
            messages.extend(prior_messages);
            messages.push(ChatMessage::user(prompt_for_request));

            if !try_consume_prompt_only_budget(&tx, &mut orchestration_state, &turn_budget).await {
                orchestration_state.final_mode = "prompt_only_budget_denied".to_string();
                let _ = tx
                    .send(TuiEvent::AgentError {
                        turn_id,
                        message: "turn budget exhausted before prompt-only execution".to_string(),
                    })
                    .await;
                emit_orchestration_summary(&tx, &orchestration_state).await;
                return;
            }
            orchestration_state.final_mode = "prompt_only".to_string();

            let _ = tx
                .send(TuiEvent::UiNotice(
                    "◈ fallback_mode=prompt_only retry_count=0 retry_reason=none".to_string(),
                ))
                .await;

            match no_tools_pipeline
                .complete_chat_stream(
                    ChatRequest::new(messages),
                    PromptExecutionContext::default(),
                    Some(&chunk_tx),
                )
                .await
            {
                Ok(completion) => {
                    let final_text = completion
                        .response
                        .into_first_text()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| {
                            "I do not have enough information to answer confidently without tools for this turn."
                                .to_string()
                        });
                    let _ = tx
                        .send(TuiEvent::AgentResponse {
                            turn_id,
                            text: final_text,
                            tool_names: Vec::new(),
                        })
                        .await;
                    emit_orchestration_summary(&tx, &orchestration_state).await;
                }
                Err(err) => {
                    let _ = tx
                        .send(TuiEvent::AgentError {
                            turn_id,
                            message: err.to_string(),
                        })
                        .await;
                    emit_orchestration_summary(&tx, &orchestration_state).await;
                }
            }
            return;
        }

        let request = ToolLoopExecutionRequest {
            user_prompt: prompt_for_request,
            system_prompt: Some(super::SYSTEM_PROMPT.to_string()),
            context: PromptExecutionContext::default(),
            tool_name: String::new(),
            tool_input: Value::Null,
            tool_call_mode: activation.tool_call_mode,
        };
        if !try_consume_tool_loop_budget(&tx, &mut orchestration_state, &turn_budget).await {
            orchestration_state.final_mode = "tool_loop_budget_denied".to_string();
            let _ = tx
                .send(TuiEvent::AgentError {
                    turn_id,
                    message: "turn budget exhausted before tool-loop execution".to_string(),
                })
                .await;
            emit_orchestration_summary(&tx, &orchestration_state).await;
            return;
        }
        orchestration_state.final_mode = "tool_loop".to_string();
        let first_attempt = pipeline
            .execute_with_stream_prior_messages_max_rounds(
                request.clone(),
                prior_messages.clone(),
                Some(&chunk_tx),
                activation.max_tool_rounds,
            )
            .await;

        match first_attempt {
            Ok(response) => {
                let _ = tx
                    .send(TuiEvent::UiNotice(
                        "◈ fallback_mode=tool_loop retry_count=0 retry_reason=none".to_string(),
                    ))
                    .await;
                emit_tool_payload_events(&tx, &response.tool_invocations).await;

                let mut combined_invocations = response.tool_invocations.clone();
                let mut final_text = response.text;
                if should_run_continuation(&combined_invocations) {
                    if let Some(continuation_prompt) = build_continuation_prompt(
                        &original_prompt_for_continuation,
                        &final_text,
                        &combined_invocations,
                    ) {
                        let _ = tx
                            .send(TuiEvent::UiNotice(
                                "◈ continuation synthesis: refining draft with chunked tool context".to_string(),
                            ))
                            .await;

                        let _ = tx
                            .send(TuiEvent::ToolInvoked {
                                tool_name: "llm.chat".to_string(),
                                input_summary: "continuation synthesis".to_string(),
                            })
                            .await;

                        let continuation_request = ToolLoopExecutionRequest {
                            user_prompt: continuation_prompt,
                            system_prompt: Some(super::SYSTEM_PROMPT.to_string()),
                            context: PromptExecutionContext::default(),
                            tool_name: String::new(),
                            tool_input: Value::Null,
                            tool_call_mode: ToolCallMode::Auto,
                        };
                        let continuation_prior_messages = build_continuation_prior_messages(
                            &original_prompt_for_continuation,
                            &final_text,
                        );

                        if try_consume_continuation_budget(
                            &tx,
                            &mut orchestration_state,
                            &turn_budget,
                        )
                        .await
                        {
                            orchestration_state.final_mode =
                                "tool_loop_with_continuation".to_string();

                            match pipeline
                                .execute_with_stream_prior_messages_max_rounds(
                                    continuation_request,
                                    continuation_prior_messages,
                                    Some(&chunk_tx),
                                    activation
                                        .max_tool_rounds
                                        .min(CONTINUATION_MAX_ROUNDS)
                                        .max(1),
                                )
                                .await
                            {
                                Ok(continuation_response) => {
                                    emit_tool_payload_events(
                                        &tx,
                                        &continuation_response.tool_invocations,
                                    )
                                    .await;
                                    final_text = continuation_response.text;
                                    combined_invocations
                                        .extend(continuation_response.tool_invocations);
                                }
                                Err(err) => {
                                    let _ = tx
                                        .send(TuiEvent::UiNotice(format!(
                                            "⚠ continuation synthesis skipped: {err}"
                                        )))
                                        .await;
                                }
                            }
                        } else {
                            let _ = tx
                                .send(TuiEvent::UiNotice(
                                    "◈ continuation synthesis skipped: turn budget limit"
                                        .to_string(),
                                ))
                                .await;
                        }
                    }
                }

                let tool_names = collect_tool_names(&combined_invocations);
                let _ = tx
                    .send(TuiEvent::ToolInvoked {
                        tool_name: "llm.chat".to_string(),
                        input_summary: format!(
                            "done  {} token(s)",
                            final_text.split_whitespace().count()
                        ),
                    })
                    .await;
                let _ = tx
                    .send(TuiEvent::AgentResponse {
                        turn_id,
                        text: final_text,
                        tool_names,
                    })
                    .await;
                emit_orchestration_summary(&tx, &orchestration_state).await;
            }
            Err(err) => {
                let err_text = err.to_string();
                if let Some(reason) = retryable_runtime_reason(&err_text) {
                    let retry_rounds = activation.max_tool_rounds.min(retry_max_rounds).max(1);
                    let mut last_err = err_text;
                    let mut retry_count = 0usize;
                    while retry_count < retry_max_retries {
                        retry_count = retry_count.saturating_add(1);
                        let _ = tx
                            .send(TuiEvent::UiNotice(format!(
                                "◈ retry_policy retry_count={} retry_reason={} fallback_mode=tool_loop rounds={}",
                                retry_count, reason, retry_rounds
                            )))
                            .await;

                        if !try_consume_retry_budget(&tx, &mut orchestration_state, &turn_budget)
                            .await
                        {
                            orchestration_state.final_mode =
                                "tool_loop_retry_budget_denied".to_string();
                            let _ = tx
                                .send(TuiEvent::AgentError {
                                    turn_id,
                                    message: "turn budget exhausted before retry".to_string(),
                                })
                                .await;
                            emit_orchestration_summary(&tx, &orchestration_state).await;
                            return;
                        }
                        orchestration_state.final_mode = "tool_loop_retry".to_string();

                        match pipeline
                            .execute_with_stream_prior_messages_max_rounds(
                                request.clone(),
                                prior_messages.clone(),
                                Some(&chunk_tx),
                                retry_rounds,
                            )
                            .await
                        {
                            Ok(response) => {
                                emit_tool_payload_events(&tx, &response.tool_invocations).await;
                                let tool_names = collect_tool_names(&response.tool_invocations);
                                let _ = tx
                                    .send(TuiEvent::AgentResponse {
                                        turn_id,
                                        text: response.text,
                                        tool_names,
                                    })
                                    .await;
                                orchestration_state.final_mode =
                                    "tool_loop_retry_success".to_string();
                                emit_orchestration_summary(&tx, &orchestration_state).await;
                                return;
                            }
                            Err(retry_err) => {
                                last_err = format!("{}", retry_err);
                            }
                        }
                    }
                    let _ = tx
                        .send(TuiEvent::AgentError {
                            turn_id,
                            message: format!("{} (retry exhausted: {})", reason, last_err),
                        })
                        .await;
                    orchestration_state.final_mode = "tool_loop_retry_exhausted".to_string();
                    emit_orchestration_summary(&tx, &orchestration_state).await;
                } else {
                    let _ = tx
                        .send(TuiEvent::UiNotice(
                            "◈ retry_policy retry_count=0 retry_reason=not_runtime".to_string(),
                        ))
                        .await;
                    let _ = tx
                        .send(TuiEvent::AgentError {
                            turn_id,
                            message: err_text,
                        })
                        .await;
                    orchestration_state.final_mode = "tool_loop_error_non_retryable".to_string();
                    emit_orchestration_summary(&tx, &orchestration_state).await;
                }
            }
        }
    });

    state.active_request_task = Some(handle);
}

fn build_prior_messages(
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    hot_window_turns: usize,
    cold_window_turns: usize,
) -> PriorMessageBuild {
    let mut selected: Vec<&ConversationTurn> = turns.iter().collect();

    if current_user_persisted {
        if let Some(last) = selected.last() {
            if last.role == "user" && last.content.trim() == current_prompt.trim() {
                selected.pop();
            }
        }
    }

    let mut accepted: Vec<ChatMessage> = Vec::new();
    let mut total_chars = 0usize;

    let hot_turns: Vec<&ConversationTurn> = selected
        .iter()
        .rev()
        .take(hot_window_turns)
        .copied()
        .collect();
    let cold_turns: Vec<&ConversationTurn> = selected
        .iter()
        .rev()
        .skip(hot_window_turns)
        .take(cold_window_turns)
        .copied()
        .collect();

    let mut hot_remaining = HOT_WINDOW_CHAR_BUDGET.min(MAX_PRIOR_TOTAL_CHARS);
    for turn in hot_turns {
        if hot_remaining == 0 {
            break;
        }

        let bounded = truncate_text_for_budget(&turn.content, MAX_SINGLE_PRIOR_MESSAGE_CHARS);
        let bounded = truncate_text_for_budget(&bounded, hot_remaining);
        if bounded.trim().is_empty() {
            continue;
        }

        let bounded_chars = bounded.chars().count();
        hot_remaining = hot_remaining.saturating_sub(bounded_chars);
        total_chars = total_chars.saturating_add(bounded_chars);
        match turn.role.as_str() {
            "user" => accepted.push(ChatMessage::user(bounded)),
            "assistant" | "agent" => accepted.push(ChatMessage::assistant(bounded)),
            _ => {}
        }
    }

    let cold_lines = cold_turns
        .iter()
        .rev()
        .filter_map(|turn| match turn.role.as_str() {
            "user" | "assistant" | "agent" => {
                let line = truncate_text_for_budget(&turn.content, COLD_SUMMARY_LINE_CHARS);
                if line.trim().is_empty() {
                    None
                } else {
                    let role = if turn.role == "agent" {
                        "assistant"
                    } else {
                        turn.role.as_str()
                    };
                    Some(format!("{}: {}", role, line.replace('\n', " ")))
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    let cold_summary = if cold_lines.is_empty() {
        String::new()
    } else {
        let cold_budget =
            COLD_WINDOW_CHAR_BUDGET.min(MAX_PRIOR_TOTAL_CHARS.saturating_sub(total_chars));
        truncate_text_for_budget(
            &format!("[MEDOUSA_COLD_HISTORY_SUMMARY]\n{}", cold_lines.join("\n")),
            cold_budget,
        )
    };
    let cold_summary_chars = cold_summary.chars().count();
    if !cold_summary.trim().is_empty() {
        total_chars = total_chars.saturating_add(cold_summary_chars);
        accepted.push(ChatMessage::assistant(cold_summary));
    }

    accepted.reverse();
    PriorMessageBuild {
        messages: accepted,
        hot_turns_included: selected.len().min(hot_window_turns),
        cold_turns_summarized: selected
            .len()
            .saturating_sub(hot_window_turns)
            .min(cold_window_turns),
        cold_summary_chars,
        total_chars,
    }
}

fn decide_turn_activation(
    prompt: &str,
    configured_mode: ToolCallMode,
    configured_rounds: usize,
    turn_count: usize,
    direct_answer_max_prompt_chars: usize,
    long_session_turn_threshold: usize,
    long_session_max_prompt_chars: usize,
) -> TurnActivationDecision {
    let prompt_trimmed = prompt.trim();
    let prompt_lower = prompt_trimmed.to_ascii_lowercase();
    let prompt_chars = prompt_trimmed.chars().count();

    let tool_intent = contains_tool_intent(&prompt_lower);
    let direct_answer_intent = contains_direct_answer_intent(&prompt_lower);

    if tool_intent {
        return TurnActivationDecision {
            turn_class: "c",
            tool_call_mode: ToolCallMode::Auto,
            max_tool_rounds: configured_rounds.min(12).max(2),
            enforce_no_tools: false,
            reason: "tool_intent_detected",
        };
    }

    if direct_answer_intent && prompt_chars < direct_answer_max_prompt_chars {
        return TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: 1,
            enforce_no_tools: true,
            reason: "direct_answer_short_prompt",
        };
    }

    if turn_count > long_session_turn_threshold && prompt_chars < long_session_max_prompt_chars {
        return TurnActivationDecision {
            turn_class: "b",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: 1,
            enforce_no_tools: true,
            reason: "long_session_short_turn",
        };
    }

    TurnActivationDecision {
        turn_class: "b",
        tool_call_mode: configured_mode,
        max_tool_rounds: configured_rounds,
        enforce_no_tools: false,
        reason: "configured_default",
    }
}

fn should_invoke_intent_classifier(activation: &TurnActivationDecision) -> bool {
    activation.reason == "configured_default"
}

fn apply_context_compiler_activation_gate(
    base: TurnActivationDecision,
    allow_no_tools_fallback: bool,
) -> TurnActivationDecision {
    if base.enforce_no_tools && !allow_no_tools_fallback {
        return TurnActivationDecision {
            turn_class: "b",
            tool_call_mode: ToolCallMode::Auto,
            max_tool_rounds: base.max_tool_rounds.max(2),
            enforce_no_tools: false,
            reason: "cheap_recall_first_no_verified_context",
        };
    }

    base
}

async fn classify_turn_intent_with_model(
    pipeline: &PromptExecutionPipeline,
    prompt: &str,
    recent_context: &str,
) -> Option<IntentClassification> {
    let bounded_prompt = truncate_text_for_budget(prompt, INTENT_CLASSIFIER_MAX_PROMPT_CHARS);
    let bounded_context =
        truncate_text_for_budget(recent_context, INTENT_CLASSIFIER_MAX_CONTEXT_CHARS);
    let messages = vec![
        ChatMessage::system(
            "You are an intent router. Classify intent for tool routing using CURRENT_USER_MESSAGE plus RECENT_CONTEXT. RECENT_CONTEXT is only local grounding for short follow-ups (acknowledgements, confirmations, pivots); do not treat old unresolved tasks as active unless CURRENT_USER_MESSAGE explicitly re-requests them. Return strict JSON only with fields: intent, confidence, reason. intent must be one of: conversational, tool_required, clarify, mixed.".to_string(),
        ),
        ChatMessage::user(format!(
            "RECENT_CONTEXT:\n{}\n\nCURRENT_USER_MESSAGE:\n{}\n\nClassify whether this turn should use tools now.",
            if bounded_context.trim().is_empty() {
                "(none)"
            } else {
                bounded_context.as_str()
            },
            bounded_prompt,
        )),
    ];

    let completion = pipeline
        .complete_chat_stream(
            ChatRequest::new(messages),
            PromptExecutionContext::default(),
            None,
        )
        .await
        .ok()?;

    let raw = completion
        .response
        .into_first_text()
        .map(|value| value.trim().to_string())?;

    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let intent = parsed
        .get("intent")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())?;
    let confidence = parsed
        .get("confidence")
        .and_then(|value| value.as_f64())
        .map(|value| value as f32)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let reason = parsed
        .get("reason")
        .and_then(|value| value.as_str())
        .map(|value| truncate_text_for_budget(value, 120))
        .unwrap_or_else(|| "none".to_string());

    Some(IntentClassification {
        intent,
        confidence,
        reason,
    })
}

fn apply_intent_classifier_override(
    base: TurnActivationDecision,
    classification: &IntentClassification,
) -> TurnActivationDecision {
    if classification.confidence < INTENT_CLASSIFIER_CONFIDENCE_LOW {
        return TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: 1,
            enforce_no_tools: true,
            reason: "classifier_low_confidence_bias_no_tools",
        };
    }

    match classification.intent.as_str() {
        "conversational"
            if classification.confidence >= INTENT_CLASSIFIER_CONFIDENCE_CONVERSATIONAL =>
        {
            TurnActivationDecision {
                turn_class: "a",
                tool_call_mode: ToolCallMode::Strict,
                max_tool_rounds: 1,
                enforce_no_tools: true,
                reason: "classifier_conversational",
            }
        }
        "clarify" => TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: 1,
            enforce_no_tools: true,
            reason: "classifier_clarify",
        },
        "tool_required"
            if classification.confidence >= INTENT_CLASSIFIER_CONFIDENCE_TOOL_REQUIRED =>
        {
            TurnActivationDecision {
                turn_class: "c",
                tool_call_mode: ToolCallMode::Auto,
                max_tool_rounds: base.max_tool_rounds.max(2),
                enforce_no_tools: false,
                reason: "classifier_tool_required",
            }
        }
        "mixed" => TurnActivationDecision {
            reason: "classifier_mixed_keep_default",
            ..base
        },
        _ => base,
    }
}

fn retryable_runtime_reason(err_text: &str) -> Option<&'static str> {
    let text = err_text.to_ascii_lowercase();
    if text.contains("timeout") || text.contains("timed out") {
        return Some("timeout");
    }
    if text.contains("queue") && (text.contains("busy") || text.contains("full")) {
        return Some("queue_busy");
    }
    if text.contains("connection")
        || text.contains("transport")
        || text.contains("temporar")
        || text.contains("unavailable")
        || text.contains("5xx")
    {
        return Some("transient_runtime");
    }
    None
}

fn build_prompt_pipeline_for_turn(
    final_route: Option<&medousa::stage_routing::StageRoute>,
    settings: &super::RuntimeSettings,
) -> PromptExecutionPipeline {
    let (provider, model, base_url) = match final_route {
        Some(route) => (
            route.provider.clone(),
            route.model.clone(),
            route_base_url(route, settings),
        ),
        None => {
            let base = settings.base_url.trim();
            (
                settings.provider.clone(),
                settings.model.clone(),
                if base.is_empty() {
                    None
                } else {
                    Some(base.to_string())
                },
            )
        }
    };

    let chat_client: Arc<dyn AiChatClient> = Arc::new(
        GenaiChatClient::from_provider_model_with_base_url(
            Some(&provider),
            &model,
            base_url.as_deref(),
        ),
    );
    PromptExecutionPipeline::new(chat_client)
}

fn build_intent_classifier_recent_context(
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    max_turns: usize,
    max_chars: usize,
) -> String {
    let mut selected: Vec<&ConversationTurn> = turns.iter().collect();

    if current_user_persisted {
        if let Some(last) = selected.last() {
            if last.role == "user" && last.content.trim() == current_prompt.trim() {
                selected.pop();
            }
        }
    }

    let mut lines = Vec::new();
    let mut total_chars = 0usize;
    for turn in selected.iter().rev().take(max_turns).rev() {
        let role = match turn.role.as_str() {
            "user" => "user",
            "assistant" | "agent" => "assistant",
            _ => continue,
        };

        let text = truncate_text_for_budget(&turn.content, INTENT_CLASSIFIER_CONTEXT_LINE_CHARS)
            .replace('\n', " ");
        if text.trim().is_empty() {
            continue;
        }

        let line = format!("{}: {}", role, text);
        let line_chars = line.chars().count();
        if total_chars.saturating_add(line_chars) > max_chars {
            break;
        }
        total_chars = total_chars.saturating_add(line_chars);
        lines.push(line);
    }

    lines.join("\n")
}

fn contains_tool_intent(prompt_lower: &str) -> bool {
    [
        "search", "look up", "lookup", "run ", "execute", "query", "fetch", "verify", "evidence",
        "grapheme", "tool", "call", "api", "latest",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn contains_direct_answer_intent(prompt_lower: &str) -> bool {
    [
        "explain",
        "summarize",
        "rephrase",
        "clarify",
        "what does",
        "how does",
        "why",
        "help me understand",
        "give me",
        "draft",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

async fn emit_tool_payload_events(tx: &mpsc::Sender<TuiEvent>, invocations: &[ToolInvocation]) {
    for invocation in invocations {
        let safe_input = medousa::settings_guard::redact_json_value(&invocation.tool_input);
        let safe_output = medousa::settings_guard::redact_json_value(&invocation.tool_output);
        let _ = tx
            .send(TuiEvent::ToolPayload {
                tool_name: invocation.tool_name.clone(),
                tool_input: invocation.tool_input.clone(),
                tool_output: invocation.tool_output.clone(),
                input_receipt: medousa::payload_receipt::receipt_meta(
                    &safe_input,
                    medousa::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                ),
                output_receipt: medousa::payload_receipt::receipt_meta(
                    &safe_output,
                    medousa::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                ),
            })
            .await;
    }
}

fn should_run_continuation(invocations: &[ToolInvocation]) -> bool {
    for invocation in invocations {
        let output_chars = invocation.tool_output.to_string().chars().count();
        if output_chars >= CONTINUATION_TRIGGER_TOOL_OUTPUT_CHARS {
            return true;
        }

        let stdout_chars = invocation
            .tool_output
            .get("stdout")
            .and_then(|value| value.as_str())
            .map(|value| value.chars().count())
            .unwrap_or(0);
        if stdout_chars >= CONTINUATION_TRIGGER_STDOUT_CHARS {
            return true;
        }

        if invocation
            .tool_name
            .to_ascii_lowercase()
            .contains("grapheme")
            && output_chars >= 2000
        {
            return true;
        }
    }
    false
}

fn build_continuation_prompt(
    original_prompt: &str,
    draft_text: &str,
    invocations: &[ToolInvocation],
) -> Option<String> {
    if invocations.is_empty() {
        return None;
    }

    let summaries = invocations
        .iter()
        .take(CONTINUATION_MAX_TOOL_SUMMARIES)
        .map(|invocation| {
            let safe_output = medousa::settings_guard::redact_json_value(&invocation.tool_output);
            let rendered_output = truncate_text_for_budget(
                &safe_output.to_string(),
                CONTINUATION_MAX_TOOL_OUTPUT_CHARS,
            );
            format!(
                "- tool={} output={} ",
                invocation.tool_name, rendered_output
            )
        })
        .collect::<Vec<_>>();

    if summaries.is_empty() {
        return None;
    }

    let draft = truncate_text_for_budget(draft_text, CONTINUATION_MAX_DRAFT_CHARS);
    let user_request = truncate_text_for_budget(original_prompt, 3000);
    let prompt = format!(
        "You have an initial draft answer plus additional tool context that may have arrived in chunks. Rewrite one coherent final answer that integrates the tool evidence. Preserve substantiated details, remove contradictions, and mark uncertainty explicitly. Prefer concise structure with clear takeaways.\n\n[USER_REQUEST]\n{user_request}\n\n[DRAFT_ANSWER]\n{draft}\n\n[ADDITIONAL_TOOL_CONTEXT]\n{}\n\nReturn only the final answer body.",
        summaries.join("\n")
    );

    Some(truncate_text_for_budget(&prompt, MAX_REQUEST_PROMPT_CHARS))
}

fn build_continuation_prior_messages(original_prompt: &str, draft_text: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::user(truncate_text_for_budget(original_prompt, 2000)),
        ChatMessage::assistant(truncate_text_for_budget(draft_text, 4000)),
    ]
}

fn collect_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    let mut names = Vec::new();
    for invocation in invocations {
        if !names
            .iter()
            .any(|existing| existing == &invocation.tool_name)
        {
            names.push(invocation.tool_name.clone());
        }
    }
    names
}

fn turn_budget_for_lane(lane: EngineExecutionLane) -> TurnBudget {
    let lane_budget = lane_execution_budget(lane);
    TurnBudget {
        max_llm_calls_total: lane_budget.max_llm_calls_total,
        max_tool_loop_calls: lane_budget.max_tool_loop_calls,
        max_prompt_only_calls: lane_budget.max_prompt_only_calls,
        max_classifier_calls: lane_budget.max_classifier_calls,
        max_retries: lane_budget.max_retries,
        max_continuations: lane_budget.max_continuations,
    }
}

async fn try_consume_classifier_budget(
    tx: &mpsc::Sender<TuiEvent>,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.classifier_calls >= budget.max_classifier_calls {
        return emit_budget_deny(
            tx,
            state,
            "classifier",
            "max_classifier_calls",
            state.classifier_calls,
            budget.max_classifier_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            tx,
            state,
            "classifier",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.classifier_calls = state.classifier_calls.saturating_add(1);
    true
}

async fn try_consume_prompt_only_budget(
    tx: &mpsc::Sender<TuiEvent>,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.prompt_only_calls >= budget.max_prompt_only_calls {
        return emit_budget_deny(
            tx,
            state,
            "prompt_only",
            "max_prompt_only_calls",
            state.prompt_only_calls,
            budget.max_prompt_only_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            tx,
            state,
            "prompt_only",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.prompt_only_calls = state.prompt_only_calls.saturating_add(1);
    true
}

async fn try_consume_tool_loop_budget(
    tx: &mpsc::Sender<TuiEvent>,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.tool_loop_calls >= budget.max_tool_loop_calls {
        return emit_budget_deny(
            tx,
            state,
            "tool_loop",
            "max_tool_loop_calls",
            state.tool_loop_calls,
            budget.max_tool_loop_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            tx,
            state,
            "tool_loop",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.tool_loop_calls = state.tool_loop_calls.saturating_add(1);
    true
}

async fn try_consume_continuation_budget(
    tx: &mpsc::Sender<TuiEvent>,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.continuations >= budget.max_continuations {
        return emit_budget_deny(
            tx,
            state,
            "continuation",
            "max_continuations",
            state.continuations,
            budget.max_continuations,
        )
        .await;
    }
    if state.tool_loop_calls >= budget.max_tool_loop_calls {
        return emit_budget_deny(
            tx,
            state,
            "continuation",
            "max_tool_loop_calls",
            state.tool_loop_calls,
            budget.max_tool_loop_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            tx,
            state,
            "continuation",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.tool_loop_calls = state.tool_loop_calls.saturating_add(1);
    state.continuations = state.continuations.saturating_add(1);
    true
}

async fn try_consume_retry_budget(
    tx: &mpsc::Sender<TuiEvent>,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.retries >= budget.max_retries {
        return emit_budget_deny(
            tx,
            state,
            "retry",
            "max_retries",
            state.retries,
            budget.max_retries,
        )
        .await;
    }
    if state.tool_loop_calls >= budget.max_tool_loop_calls {
        return emit_budget_deny(
            tx,
            state,
            "retry",
            "max_tool_loop_calls",
            state.tool_loop_calls,
            budget.max_tool_loop_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            tx,
            state,
            "retry",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.tool_loop_calls = state.tool_loop_calls.saturating_add(1);
    state.retries = state.retries.saturating_add(1);
    true
}

async fn emit_budget_deny(
    tx: &mpsc::Sender<TuiEvent>,
    state: &mut TurnOrchestrationState,
    stage: &str,
    reason: &str,
    used: usize,
    limit: usize,
) -> bool {
    state.loop_guard_tripped = true;
    let _ = tx
        .send(TuiEvent::UiNotice(format!(
            "◈ budget_deny stage={} reason={} used={} limit={}",
            stage, reason, used, limit
        )))
        .await;
    false
}

async fn emit_orchestration_summary(tx: &mpsc::Sender<TuiEvent>, state: &TurnOrchestrationState) {
    let _ = tx
        .send(TuiEvent::UiNotice(format!(
            "◈ orchestration_summary calls_total={} classifier_calls={} tool_loop_calls={} prompt_only_calls={} continuations={} retries={} loop_guard_tripped={} final_mode={}",
            state.calls_total,
            state.classifier_calls,
            state.tool_loop_calls,
            state.prompt_only_calls,
            state.continuations,
            state.retries,
            state.loop_guard_tripped,
            state.final_mode,
        )))
        .await;
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

fn parse_tool_call_mode(value: &str) -> ToolCallMode {
    if value.trim().eq_ignore_ascii_case("strict") {
        ToolCallMode::Strict
    } else {
        ToolCallMode::Auto
    }
}

fn resolve_prompt_with_context_pack(
    session_id: &str,
    prompt: &str,
    pack_query: Option<&str>,
    policy: &medousa::verifier::VerificationPolicy,
) -> (String, Option<String>, Option<bool>) {
    let selector = pack_query.unwrap_or("last");
    let Some(pack) = medousa::context_pack::find_context_pack(session_id, Some(selector)) else {
        return (prompt.to_string(), None, None);
    };

    let (prompt_with_pack, quality, report) = build_prompt_with_context_pack(prompt, &pack, policy);
    let verification_id = medousa::verification_store::persist_verification(
        session_id,
        selector,
        "prompt_injection",
        policy,
        &report,
    )
    .ok()
    .map(|record| record.verification_id);

    let verification_suffix = verification_id
        .map(|id| format!(" verification={id}"))
        .unwrap_or_default();
    let note = if quality.is_usable {
        format!(
            "◈ context pack verified {} selector={} artifact={} claims={} chunks={} coverage={:.2} avg_support={:.2} support_ratio={:.2} confidence={:.2}{}",
            pack.pack_id,
            selector,
            pack.artifact_id,
            pack.selected_claims.len(),
            pack.selected_chunk_refs.len(),
            quality.citation_coverage,
            quality.avg_support_strength,
            quality.supported_claim_ratio,
            quality.confidence_score,
            verification_suffix,
        )
    } else {
        format!(
            "◈ context pack verification failed {} selector={} artifact={} coverage={:.2} avg_support={:.2} support_ratio={:.2} confidence={:.2}{}",
            pack.pack_id,
            selector,
            pack.artifact_id,
            quality.citation_coverage,
            quality.avg_support_strength,
            quality.supported_claim_ratio,
            quality.confidence_score,
            verification_suffix,
        )
    };

    (prompt_with_pack, Some(note), Some(quality.is_usable))
}

async fn cheap_memory_recall_probe(
    tui_rt: &TuiRuntime,
    session_id: &str,
    prompt: &str,
) -> CheapRecallProbe {
    let query_text = truncate_text_for_budget(prompt, CHEAP_RECALL_QUERY_MAX_CHARS)
        .trim()
        .to_string();
    if query_text.is_empty() {
        return CheapRecallProbe::default();
    }

    let mut request = MemoryRecallRequest {
        query_text: Some(query_text),
        limit: CHEAP_RECALL_LIMIT,
        ..Default::default()
    };
    request.scope.session_ids = Some(vec![session_id.to_string()]);

    match tui_rt.memory_reader.recall(&request).await {
        Ok(response) => CheapRecallProbe {
            attempted: true,
            retrieved: response.retrieved,
            retrieval_path: response.retrieval_path,
            fallback_triggered: response.fallback_triggered,
            fallback_reason: response.fallback_reason,
            node_sync_keys: response
                .node_sync_keys
                .into_iter()
                .take(CHEAP_RECALL_MAX_KEYS)
                .collect(),
            error: None,
        },
        Err(err) => CheapRecallProbe {
            attempted: true,
            error: Some(err.to_string()),
            ..Default::default()
        },
    }
}

fn append_memory_recall_hint(prompt: &str, recall: &CheapRecallProbe) -> String {
    if !recall.attempted {
        return prompt.to_string();
    }

    let keys = if recall.node_sync_keys.is_empty() {
        "none".to_string()
    } else {
        recall.node_sync_keys.join(",")
    };
    let status = if recall.retrieved > 0 { "hit" } else { "miss" };
    let fallback_reason = recall
        .fallback_reason
        .as_deref()
        .unwrap_or("none")
        .replace('\n', " ");

    format!(
        "{prompt}\n\n[MEDOUSA_MEMORY_RECALL]\nstatus={status}\nretrieved={}\nretrieval_path={}\nfallback_triggered={}\nfallback_reason={}\nnode_sync_keys={}",
        recall.retrieved,
        recall.retrieval_path.as_deref().unwrap_or("none"),
        recall.fallback_triggered,
        truncate_text_for_budget(&fallback_reason, 200),
        keys,
    )
}

fn build_prompt_with_context_pack(
    prompt: &str,
    pack: &medousa::context_pack::ContextPack,
    policy: &medousa::verifier::VerificationPolicy,
) -> (
    String,
    ContextPackQuality,
    medousa::verifier::VerificationReport,
) {
    let report = medousa::verifier::verify_context_pack(pack, policy);
    let quality = ContextPackQuality {
        citation_coverage: report.citation_coverage,
        avg_support_strength: report.avg_support_strength,
        supported_claim_ratio: report.supported_claim_ratio,
        confidence_score: report.confidence_score,
        is_usable: report.is_verified,
    };

    if !quality.is_usable {
        let fallback = format!(
            "{prompt}\n\n[MEDOUSA_CONTEXT_PACK]\nstatus=verification_failed\npack_id={}\nartifact_id={}\ncitation_coverage={:.2}\navg_support={:.2}\nsupported_claim_ratio={:.2}\nconfidence={:.2}\npolicy=Treat context pack claims as non-authoritative. If evidence is needed, call tools or request fresher data.",
            pack.pack_id,
            pack.artifact_id,
            quality.citation_coverage,
            quality.avg_support_strength,
            quality.supported_claim_ratio,
            quality.confidence_score,
        );
        return (fallback, quality, report);
    }

    let claim_lines = pack
        .selected_claims
        .iter()
        .take(8)
        .map(|claim| {
            let refs = if claim.supporting_chunk_node_ids.is_empty() {
                "none".to_string()
            } else {
                claim
                    .supporting_chunk_node_ids
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let statement = truncate_text_for_budget(&claim.statement, 360);
            format!(
                "- [{}] strength={:.2} refs={} {}",
                claim.claim_id, claim.support_strength, refs, statement
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let chunk_lines = pack
        .selected_chunk_refs
        .iter()
        .take(8)
        .map(|chunk| {
            format!(
                "- {} tokens={} hash={}",
                chunk.node_id, chunk.token_estimate, chunk.hash64
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let augmented = format!(
        "{prompt}\n\n[MEDOUSA_CONTEXT_PACK]\nstatus=verified\npack_id={}\nartifact_id={}\ntoken_estimate={}\ncitation_coverage={:.2}\navg_support={:.2}\nsupported_claim_ratio={:.2}\nconfidence={:.2}\nclaims:\n{}\nchunks:\n{}",
        pack.pack_id,
        pack.artifact_id,
        pack.total_token_estimate,
        quality.citation_coverage,
        quality.avg_support_strength,
        quality.supported_claim_ratio,
        quality.confidence_score,
        claim_lines,
        chunk_lines,
    );

    (augmented, quality, report)
}

fn truncate_text_for_budget(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    if max_chars <= 12 {
        return text.chars().take(max_chars).collect();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 5);
    let head_part = text.chars().take(head).collect::<String>();
    let tail_part = text
        .chars()
        .skip(total_chars.saturating_sub(tail))
        .collect::<String>();
    format!("{head_part}\n...\n{tail_part}")
}

pub(crate) fn verifier_policy_from_settings_and_route(
    settings: &super::RuntimeSettings,
    verifier_route: Option<&medousa::stage_routing::StageRoute>,
) -> medousa::verifier::VerificationPolicy {
    let mut policy = medousa::verifier::VerificationPolicy {
        min_citation_coverage: super::parse_f32_with_bounds(
            &settings.verifier_min_citation_coverage,
            0.60,
            0.0,
            1.0,
        ),
        min_avg_support_strength: super::parse_f32_with_bounds(
            &settings.verifier_min_avg_support_strength,
            0.70,
            0.0,
            1.0,
        ),
        min_supported_claim_ratio: super::parse_f32_with_bounds(
            &settings.verifier_min_supported_claim_ratio,
            0.60,
            0.0,
            1.0,
        ),
        min_claim_support_strength: super::parse_f32_with_bounds(
            &settings.verifier_min_claim_support_strength,
            0.65,
            0.0,
            1.0,
        ),
    };

    if let Some(route) = verifier_route {
        apply_verifier_policy_profile(&mut policy, &route.policy_profile);
    }

    policy
}

fn apply_verifier_policy_profile(
    policy: &mut medousa::verifier::VerificationPolicy,
    policy_profile: &str,
) {
    match policy_profile.trim().to_ascii_lowercase().as_str() {
        "strict" => {
            policy.min_citation_coverage = policy.min_citation_coverage.max(0.70);
            policy.min_avg_support_strength = policy.min_avg_support_strength.max(0.75);
            policy.min_supported_claim_ratio = policy.min_supported_claim_ratio.max(0.70);
            policy.min_claim_support_strength = policy.min_claim_support_strength.max(0.72);
        }
        "analytical" => {
            policy.min_citation_coverage = policy.min_citation_coverage.max(0.65);
            policy.min_avg_support_strength = policy.min_avg_support_strength.max(0.78);
            policy.min_supported_claim_ratio = policy.min_supported_claim_ratio.max(0.62);
            policy.min_claim_support_strength = policy.min_claim_support_strength.max(0.76);
        }
        "fast" => {
            policy.min_citation_coverage = policy.min_citation_coverage.min(0.50);
            policy.min_avg_support_strength = policy.min_avg_support_strength.min(0.55);
            policy.min_supported_claim_ratio = policy.min_supported_claim_ratio.min(0.50);
            policy.min_claim_support_strength = policy.min_claim_support_strength.min(0.52);
        }
        _ => {}
    }
}

fn route_base_url(
    route: &medousa::stage_routing::StageRoute,
    settings: &super::RuntimeSettings,
) -> Option<String> {
    if route
        .provider
        .eq_ignore_ascii_case(settings.provider.trim())
    {
        let candidate = settings.base_url.trim();
        if !candidate.is_empty() {
            return Some(candidate.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        ToolCallMode, ToolInvocation, apply_context_compiler_activation_gate,
        build_intent_classifier_recent_context, build_prior_messages,
        build_prompt_with_context_pack, decide_turn_activation, should_run_continuation,
        verifier_policy_from_settings_and_route,
    };
    use chrono::Utc;
    use medousa::artifact_chunking::SttpChunkNodeRef;
    use medousa::artifact_extraction::EvidenceClaim;
    use medousa::context_pack::{ContextPack, ContextPackBudgetProfile};

    fn sample_pack() -> ContextPack {
        ContextPack {
            pack_id: "pack:test:1".to_string(),
            session_id: "session-1".to_string(),
            artifact_id: "artifact-1".to_string(),
            created_at_utc: Utc::now(),
            budget_profile: ContextPackBudgetProfile {
                max_tokens: 3200,
                max_claims: 6,
                max_chunks: 12,
            },
            selected_claims: vec![EvidenceClaim {
                claim_id: "claim-1".to_string(),
                statement: "The payload contains two result entries.".to_string(),
                supporting_chunk_node_ids: vec!["sttp:artifact-1:chunk:0".to_string()],
                support_strength: 0.88,
            }],
            selected_chunk_refs: vec![SttpChunkNodeRef {
                node_id: "sttp:artifact-1:chunk:0".to_string(),
                chunk_id: "artifact-1:chunk:0".to_string(),
                sequence: 0,
                token_estimate: 120,
                hash64: "abc123".to_string(),
            }],
            total_token_estimate: 120,
        }
    }

    #[test]
    fn prompt_includes_pack_when_quality_is_usable() {
        let pack = sample_pack();
        let policy = medousa::verifier::VerificationPolicy::default();
        let (prompt, quality, _) =
            build_prompt_with_context_pack("Summarize latest run", &pack, &policy);
        assert!(quality.is_usable);
        assert!(prompt.contains("[MEDOUSA_CONTEXT_PACK]"));
        assert!(prompt.contains("status=verified"));
        assert!(prompt.contains("claims:"));
    }

    #[test]
    fn quality_rejects_low_coverage_pack() {
        let mut pack = sample_pack();
        pack.selected_claims[0].supporting_chunk_node_ids.clear();
        pack.selected_claims[0].support_strength = 0.40;

        let policy = medousa::verifier::VerificationPolicy::default();
        let (prompt, quality, _) =
            build_prompt_with_context_pack("Summarize latest run", &pack, &policy);
        assert!(!quality.is_usable);
        assert!(prompt.contains("status=verification_failed"));
    }

    #[test]
    fn derives_policy_from_settings_values() {
        let settings = super::super::RuntimeSettings {
            backend: "in-memory".to_string(),
            theme_id: "medousa-default".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: String::new(),
            env_overrides: String::new(),
            api_key: String::new(),
            allowed_modules: String::new(),
            tool_call_mode: "auto".to_string(),
            max_tool_rounds: "10".to_string(),
            thinking_capture: "true".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "3".to_string(),
            verifier_min_citation_coverage: "0.55".to_string(),
            verifier_min_avg_support_strength: "0.66".to_string(),
            verifier_min_supported_claim_ratio: "0.77".to_string(),
            verifier_min_claim_support_strength: "0.88".to_string(),
        };

        let policy = verifier_policy_from_settings_and_route(&settings, None);
        assert!((policy.min_citation_coverage - 0.55).abs() < 0.001);
        assert!((policy.min_avg_support_strength - 0.66).abs() < 0.001);
        assert!((policy.min_supported_claim_ratio - 0.77).abs() < 0.001);
        assert!((policy.min_claim_support_strength - 0.88).abs() < 0.001);
    }

    #[test]
    fn strict_route_profile_tightens_verifier_policy() {
        let settings = super::super::RuntimeSettings {
            backend: "in-memory".to_string(),
            theme_id: "medousa-default".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            base_url: String::new(),
            env_overrides: String::new(),
            api_key: String::new(),
            allowed_modules: String::new(),
            tool_call_mode: "auto".to_string(),
            max_tool_rounds: "10".to_string(),
            thinking_capture: "true".to_string(),
            thinking_max_lines: "300".to_string(),
            activation_direct_answer_max_prompt_chars: "320".to_string(),
            activation_long_session_turn_threshold: "28".to_string(),
            activation_long_session_max_prompt_chars: "420".to_string(),
            slice_hot_window_turns: "8".to_string(),
            slice_cold_window_turns: "24".to_string(),
            retry_runtime_max_retries: "1".to_string(),
            retry_runtime_max_rounds: "3".to_string(),
            verifier_min_citation_coverage: "0.55".to_string(),
            verifier_min_avg_support_strength: "0.66".to_string(),
            verifier_min_supported_claim_ratio: "0.57".to_string(),
            verifier_min_claim_support_strength: "0.61".to_string(),
        };
        let route = medousa::stage_routing::StageRoute {
            role: "verifier".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            policy_profile: "strict".to_string(),
            fallback_chain: vec!["verifier".to_string()],
        };

        let policy = verifier_policy_from_settings_and_route(&settings, Some(&route));
        assert!((policy.min_citation_coverage - 0.70).abs() < 0.001);
        assert!((policy.min_avg_support_strength - 0.75).abs() < 0.001);
        assert!((policy.min_supported_claim_ratio - 0.70).abs() < 0.001);
        assert!((policy.min_claim_support_strength - 0.72).abs() < 0.001);
    }

    #[test]
    fn continuation_trigger_detects_large_stdout_payload() {
        let invocations = vec![
            ToolInvocation {
                tool_name: "cognition.grapheme.run".to_string(),
                tool_input: serde_json::json!({"script": "noop"}),
                tool_output: serde_json::json!({
                    "stdout": "x".repeat(4500)
                }),
            },
        ];

        assert!(should_run_continuation(&invocations));
    }

    #[test]
    fn activation_policy_prefers_no_tools_for_short_explanations() {
        let policy = decide_turn_activation(
            "Explain what this config means",
            ToolCallMode::Auto,
            10,
            4,
            320,
            28,
            420,
        );
        assert!(policy.enforce_no_tools);
        assert_eq!(policy.max_tool_rounds, 1);
    }

    #[test]
    fn activation_policy_prefers_tools_for_lookup_intent() {
        let policy = decide_turn_activation(
            "Search latest runtime failures and verify evidence",
            ToolCallMode::Strict,
            3,
            8,
            320,
            28,
            420,
        );
        assert!(!policy.enforce_no_tools);
        assert_eq!(policy.tool_call_mode, ToolCallMode::Auto);
    }

    #[test]
    fn activation_gate_blocks_no_tools_when_recall_not_verified() {
        let base = decide_turn_activation(
            "Explain what this config means",
            ToolCallMode::Auto,
            10,
            4,
            320,
            28,
            420,
        );
        assert!(base.enforce_no_tools);

        let gated = apply_context_compiler_activation_gate(base, false);
        assert!(!gated.enforce_no_tools);
        assert_eq!(gated.tool_call_mode, ToolCallMode::Auto);
        assert_eq!(gated.reason, "cheap_recall_first_no_verified_context");
    }

    #[test]
    fn prior_messages_include_cold_history_summary() {
        let mut turns = Vec::new();
        for idx in 0..18 {
            turns.push(super::super::ConversationTurn {
                role: if idx % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: format!("turn-{idx}-{}", "x".repeat(120)),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            });
        }

        let built = build_prior_messages(&turns, "new prompt", false, 8, 24);
        assert!(built.hot_turns_included > 0);
        assert!(built.cold_turns_summarized > 0);
        assert!(built.total_chars > 0);
    }

    #[test]
    fn prior_messages_include_agent_role_as_assistant() {
        let turns = vec![
            super::super::ConversationTurn {
                role: "user".to_string(),
                content: "hello".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
            super::super::ConversationTurn {
                role: "agent".to_string(),
                content: "hi there".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
        ];

        let built = build_prior_messages(&turns, "new prompt", false, 8, 24);
        let has_assistant = built
            .messages
            .iter()
            .any(|message| matches!(message.role, genai::chat::ChatRole::Assistant));
        assert!(has_assistant);
    }

    #[test]
    fn classifier_recent_context_excludes_current_persisted_user_turn() {
        let turns = vec![
            super::super::ConversationTurn {
                role: "user".to_string(),
                content: "earlier question".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
            super::super::ConversationTurn {
                role: "agent".to_string(),
                content: "earlier answer".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
            super::super::ConversationTurn {
                role: "user".to_string(),
                content: "thanks".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
        ];

        let context = build_intent_classifier_recent_context(&turns, "thanks", true, 4, 1400);
        assert!(context.contains("user: earlier question"));
        assert!(context.contains("assistant: earlier answer"));
        assert!(!context.contains("user: thanks"));
    }

    #[test]
    fn classifier_recent_context_normalizes_agent_role() {
        let turns = vec![
            super::super::ConversationTurn {
                role: "agent".to_string(),
                content: "done".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
            super::super::ConversationTurn {
                role: "user".to_string(),
                content: "ok".to_string(),
                timestamp: Utc::now(),
                tool_names: Vec::new(),
                answer_state: None,
            },
        ];

        let context = build_intent_classifier_recent_context(&turns, "new", false, 4, 1400);
        assert!(context.contains("assistant: done"));
        assert!(!context.contains("agent: done"));
    }
}
