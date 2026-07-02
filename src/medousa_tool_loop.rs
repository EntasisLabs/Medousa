//! Medousa tool loop with policy-coherent parallel tool-call batches.

use std::sync::Arc;

use genai::chat::{ChatMessage, ChatRequest, ToolResponse};
use serde_json::Value;
use tokio::sync::mpsc;

use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline, PromptExecutionRequest,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolInvocation, ToolLoopExecutionRequest, ToolLoopExecutionResponse,
};
use stasis::application::orchestration::tool_registry::ToolRegistry;
use stasis::domain::errors::{Result, StasisError};
use stasis::ports::outbound::ai_chat_client::StreamDelta;

use crate::agent_runtime::turn_completion::{ToolLoopCompletionGate, collect_tool_names};
use crate::agent_runtime::turn_completion_fsm::{
    decide_after_tools_text_round, decide_no_tool_debt_text_round, resolve_interim_continue_cap,
    AfterToolsRoundContext, ContinueReason, NoToolDebtRoundContext, TurnRoundAction,
};
use crate::agent_runtime::turn_context::{
    HostTurnContext, TurnScratchpad, publish_host_handoff_snapshot,
    push_turn_scratch_message_with_budget,
};
use crate::agent_runtime::turn_ledger::{
    TurnLoopAwareness, TurnLoopDiscipline, ledger_tool_names, persist_ledger_record,
    push_turn_control_message, record_finalized, record_fsm_continue, record_stuck,
    record_tool_round, stuck_turn_user_message, TURN_CONTROL_PREFIX,
};
use crate::execution_policy::{load_parallel_execution_settings, parallel_tool_batch_allowed};
use crate::turn_budget_request::{
    turn_budget_request_store, BudgetResolution, CreateTurnBudgetRequest,
};
use crate::turn_control_tools::{
    finish_turn_from_invocations, begin_work_note_from_invocations,
    checkpoint_turn_from_invocations,
    is_begin_work_tool_name, is_checkpoint_turn_tool_name, is_finish_turn_tool_name,
    is_prepare_final_tool_name, is_request_more_rounds_tool_name, is_update_user_tool_name,
    request_more_rounds_from_invocations, terminal_text_for_fsm_end,
    turn_progress_message_from_invocations, workshop_entered_from_invocations,
    COGNITION_TURN_BEGIN_WORK, COGNITION_TURN_CHECKPOINT, COGNITION_TURN_FINISH,
};

const DEFAULT_MAX_TOOL_ROUNDS: usize = 10;

#[derive(Clone)]
pub struct MedousaToolLoopPipeline {
    prompt_pipeline: PromptExecutionPipeline,
    tool_registry: Arc<dyn ToolRegistry>,
}

#[derive(Clone)]
struct ToolLoopSharedInputs {
    user_prompt: Arc<str>,
    system_prompt: Option<Arc<str>>,
    context: Arc<PromptExecutionContext>,
    selected_tool_name: Arc<str>,
    tool_input: Arc<Value>,
    tool_call_mode: ToolCallMode,
}

impl ToolLoopSharedInputs {
    fn context_clone(&self) -> PromptExecutionContext {
        (*self.context).clone()
    }

    fn selected_tool_name(&self) -> &str {
        &self.selected_tool_name
    }
}

impl MedousaToolLoopPipeline {
    pub fn new(
        prompt_pipeline: PromptExecutionPipeline,
        tool_registry: Arc<dyn ToolRegistry>,
    ) -> Self {
        Self {
            prompt_pipeline,
            tool_registry,
        }
    }

    pub async fn execute(
        &self,
        request: ToolLoopExecutionRequest,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_with_defaults(request, Vec::new(), None).await
    }

    pub async fn execute_with_prior_messages(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_with_defaults(request, prior_messages, None).await
    }

    pub async fn execute_with_stream(
        &self,
        request: ToolLoopExecutionRequest,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_with_defaults(request, Vec::new(), chunk_tx).await
    }

    pub async fn execute_with_stream_prior_messages(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_with_defaults(request, prior_messages, chunk_tx)
            .await
    }

    pub async fn execute_with_stream_prior_messages_max_rounds(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
        max_tool_rounds: usize,
        completion_gate: Option<&mut ToolLoopCompletionGate<'_>>,
        current_turn_user_message: Option<ChatMessage>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_internal(
            request,
            prior_messages,
            chunk_tx,
            max_tool_rounds,
            completion_gate,
            current_turn_user_message,
        )
        .await
    }

    async fn execute_with_defaults(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_internal(request, prior_messages, chunk_tx, DEFAULT_MAX_TOOL_ROUNDS, None, None)
            .await
    }

    async fn execute_internal(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
        max_tool_rounds: usize,
        mut completion_gate: Option<&mut ToolLoopCompletionGate<'_>>,
        current_turn_user_message: Option<ChatMessage>,
    ) -> Result<ToolLoopExecutionResponse> {
        let ToolLoopExecutionRequest {
            user_prompt,
            system_prompt,
            context,
            tool_name,
            tool_input,
            tool_call_mode,
        } = request;

        let mut effective_max_tool_rounds = max_tool_rounds.max(1);
        let shared_inputs = ToolLoopSharedInputs {
            user_prompt: Arc::<str>::from(user_prompt),
            system_prompt: system_prompt.map(Arc::<str>::from),
            context: Arc::new(context),
            selected_tool_name: Arc::<str>::from(tool_name),
            tool_input: Arc::new(tool_input),
            tool_call_mode,
        };
        let has_selected_tool = !shared_inputs.selected_tool_name().trim().is_empty();
        let parallel_settings = load_parallel_execution_settings();

        let user_message = current_turn_user_message
            .unwrap_or_else(|| ChatMessage::user(shared_inputs.user_prompt.to_string()));
        let mut turn_ctx = HostTurnContext::new_with_user_message(prior_messages, user_message);
        if let Some(gate) = completion_gate.as_ref() {
            if let Some(seed) = gate.initial_worker_scratch.as_ref() {
                turn_ctx.scratchpad = seed.clone();
            }
        }

        let mut tools = self.tool_registry.list_tools().await?;
        if has_selected_tool {
            let selected_sanitized = sanitize_tool_name_for_model(shared_inputs.selected_tool_name());
            let selected_prefix = format!("{selected_sanitized}_");
            tools.retain(|tool| {
                let name = tool.name.as_str();
                name == shared_inputs.selected_tool_name()
                    || name == selected_sanitized
                    || name.starts_with(&selected_prefix)
            });
        }

        let mut invocations = Vec::new();
        let mut should_use_legacy_fallback = false;
        let mut fallback_draft_text: Option<String> = None;
        let mut rounds_executed = 0usize;
        let mut pending_final_answer = false;
        let streaming_enabled = chunk_tx.is_some();
        let max_text_only_stuck = completion_gate
            .as_ref()
            .map(|gate| gate.max_text_only_stuck_continues)
            .unwrap_or_else(|| {
                crate::agent_runtime::turn_ledger::resolve_max_text_only_stuck_continues(
                    effective_max_tool_rounds,
                )
            });
        let mut discipline =
            TurnLoopDiscipline::with_max_text_only_stuck_continues(max_text_only_stuck);
        let mut loop_awareness = TurnLoopAwareness::default();
        // Per-turn budget for bounded interim auto-continues (short non-tool notes
        // that should not end the turn). Capped low; the stuck discipline +
        // max_tool_rounds fuse below are the hard safety net.
        let interim_continue_cap = resolve_interim_continue_cap(effective_max_tool_rounds);
        let mut interim_continues_used = 0usize;

        if !tools.is_empty() {
            while rounds_executed < effective_max_tool_rounds {
                rounds_executed += 1;
                if let Some(gate) = completion_gate.as_ref() {
                    if let Some(work_id) = gate.cancel_poll_work_id.as_deref() {
                        if crate::agent_runtime::turn_worker::turn_worker_store()
                            .is_work_cancelled(work_id)
                        {
                            return Ok(ToolLoopExecutionResponse {
                                text: String::new(),
                                metadata: shared_inputs.context_clone(),
                                tool_name: String::new(),
                                tool_output: Value::Null,
                                tool_invocations: invocations,
                                rounds_executed,
                                termination_reason: "workshop_cancelled".to_string(),
                            });
                        }
                    }
                    if let Some(work_id) = gate.steer_poll_work_id.as_deref() {
                        let steers = crate::agent_runtime::turn_worker::turn_worker_store()
                            .drain_steer_messages(work_id);
                        for steer in steers {
                            push_turn_control_message(
                                &mut turn_ctx.tool_lane.messages,
                                &format!(
                                    "[MEDOUSA_WORKSHOP_STEER]\n{}",
                                    steer.text.trim()
                                ),
                            );
                        }
                    }
                }
                if rounds_executed > 1 {
                    if let Some(gate) = completion_gate.as_ref() {
                        gate.reset_scratch(streaming_enabled).await;
                    }
                }
                let tool_rounds_remaining =
                    effective_max_tool_rounds.saturating_sub(rounds_executed);
                turn_ctx.scratchpad.on_tool_round_start(rounds_executed);
                push_turn_control_message(
                    &mut turn_ctx.tool_lane.messages,
                    &loop_awareness.loop_budget_message(tool_rounds_remaining),
                );
                push_turn_scratch_message_with_budget(
                    &mut turn_ctx.tool_lane.messages,
                    &turn_ctx.scratchpad,
                    tool_rounds_remaining,
                );
                sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                let messages = turn_ctx
                    .build_model_messages(shared_inputs.system_prompt.as_deref());
                let chat_request = ChatRequest::new(messages).with_tools(tools.clone());
                let mut response = match chunk_tx {
                    Some(tx) => {
                        self.prompt_pipeline
                            .complete_chat_stream(
                                chat_request.clone(),
                                shared_inputs.context_clone(),
                                Some(tx),
                            )
                            .await?
                            .response
                    }
                    None => {
                        self.prompt_pipeline
                            .complete_chat(chat_request.clone(), shared_inputs.context_clone())
                            .await?
                            .response
                    }
                };
                let mut maybe_text = response
                    .first_text()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let mut tool_calls = response.clone().into_tool_calls();

                // Some providers stream assistant text but omit tool_calls from the stream
                // capture; retry once without streaming before treating text as final.
                if tool_calls.is_empty()
                    && invocations.is_empty()
                    && !has_selected_tool
                    && chunk_tx.is_some()
                    && maybe_text.is_some()
                {
                    response = self
                        .prompt_pipeline
                        .complete_chat(chat_request, shared_inputs.context_clone())
                        .await?
                        .response;
                    maybe_text = response
                        .first_text()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    tool_calls = response.clone().into_tool_calls();
                }

                if tool_calls.is_empty() {
                    if invocations.is_empty() && has_selected_tool {
                        if shared_inputs.tool_call_mode == ToolCallMode::Strict {
                            return Err(StasisError::PortFailure(
                                "policy violation: strict tool-call mode expected model tool call but none was returned"
                                    .to_string(),
                            ));
                        }

                        should_use_legacy_fallback = true;
                        fallback_draft_text = maybe_text;
                        break;
                    }

                    if !invocations.is_empty() || maybe_text.is_some() {
                        let text = maybe_text.unwrap_or_default();
                        let workshop_lane = completion_gate
                            .as_ref()
                            .map(|gate| gate.skip_avec_ritual_check)
                            .unwrap_or(false);
                        let host_scheduler_lane = completion_gate
                            .as_ref()
                            .map(|gate| gate.host_scheduler_lane)
                            .unwrap_or(false);
                        let action = if invocations.is_empty() {
                            decide_no_tool_debt_text_round(&NoToolDebtRoundContext {
                                draft_text: text.clone(),
                                pending_final_answer,
                                rounds_executed,
                                max_tool_rounds: effective_max_tool_rounds,
                                interim_continues_used,
                                interim_continue_cap,
                                host_scheduler_lane,
                            })
                        } else {
                            decide_after_tools_text_round(&AfterToolsRoundContext {
                                draft_text: text.clone(),
                                pending_final_answer,
                                rounds_executed,
                                max_tool_rounds: effective_max_tool_rounds,
                                invocations: &invocations,
                                workshop_lane,
                                interim_continues_used,
                                interim_continue_cap,
                                host_scheduler_lane,
                            })
                        };

                        match action {
                            TurnRoundAction::EndTurn { termination_reason } => {
                                let text = terminal_text_for_fsm_end(termination_reason, text);
                                let tools = if invocations.is_empty() {
                                    Vec::new()
                                } else {
                                    collect_tool_names(&invocations)
                                };
                                if let Some(gate) = completion_gate.as_ref() {
                                    persist_ledger_record(
                                        gate.session_id.as_deref(),
                                        &record_finalized(
                                            gate.stream_turn_id,
                                            termination_reason,
                                            rounds_executed,
                                            &tools,
                                        ),
                                    );
                                }
                                let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                                    tool_name: shared_inputs.selected_tool_name().to_string(),
                                    tool_input: (*shared_inputs.tool_input).clone(),
                                    tool_output: Value::Null,
                                });
                                return Ok(ToolLoopExecutionResponse {
                                    text,
                                    metadata: shared_inputs.context_clone(),
                                    tool_name: last.tool_name,
                                    tool_output: last.tool_output,
                                    tool_invocations: invocations,
                                    rounds_executed,
                                    termination_reason: termination_reason.to_string(),
                                });
                            }
                            TurnRoundAction::ContinueLoop {
                                reason,
                                control_message,
                                missing_tools,
                            } => {
                                if matches!(
                                    reason,
                                    ContinueReason::InterimProse | ContinueReason::ExtendedProse
                                ) {
                                    interim_continues_used += 1;
                                }
                                if let Some(response) = apply_fsm_continue_loop(
                                    &text,
                                    reason,
                                    &control_message,
                                    &missing_tools,
                                    &invocations,
                                    &mut turn_ctx,
                                    &mut loop_awareness,
                                    &mut discipline,
                                    tool_rounds_remaining,
                                    completion_gate.as_deref_mut(),
                                    &shared_inputs,
                                    rounds_executed,
                                    effective_max_tool_rounds,
                                )
                                .await?
                                {
                                    return Ok(response);
                                }
                                continue;
                            }
                        }
                    } else {
                        return Err(StasisError::PortFailure(
                            "chat response was empty after tool loop".to_string(),
                        ));
                    }
                }

                if pending_final_answer
                    && tool_calls.iter().any(|call| {
                        !is_prepare_final_tool_name(&call.fn_name)
                            && !is_begin_work_tool_name(&call.fn_name)
                            && !is_update_user_tool_name(&call.fn_name)
                            && !is_checkpoint_turn_tool_name(&call.fn_name)
                            && !is_finish_turn_tool_name(&call.fn_name)
                            && !is_request_more_rounds_tool_name(&call.fn_name)
                    })
                {
                    pending_final_answer = false;
                }

                turn_ctx.tool_lane.messages.push(ChatMessage::from(tool_calls.clone()));

                let invocations_before = invocations.len();
                let batch: Vec<(String, Value)> = tool_calls
                    .iter()
                    .map(|call| (call.fn_name.clone(), call.fn_arguments.clone()))
                    .collect();

                let use_parallel = parallel_tool_batch_allowed(&batch, &parallel_settings).is_ok();

                let mut prepare_final_in_batch = false;
                let round_tool_names: Vec<String> = tool_calls
                    .iter()
                    .map(|call| call.fn_name.clone())
                    .collect();

                if use_parallel && tool_calls.len() > 1 {
                    let mut join_set = tokio::task::JoinSet::new();
                    for call in tool_calls.clone() {
                        let run_id = crate::agent_runtime::tool_stream::new_tool_run_id();
                        if let Some(gate) = completion_gate.as_ref() {
                            if let Some(sink) = gate.sink.as_ref() {
                                crate::agent_runtime::tool_stream::emit_tool_run_started(
                                    sink,
                                    &run_id,
                                    &call.fn_name,
                                    &call.fn_arguments,
                                    rounds_executed,
                                )
                                .await;
                            }
                        }
                        let registry = self.tool_registry.clone();
                        let run_id_spawn = run_id.clone();
                        join_set.spawn(async move {
                            let output = registry
                                .invoke_tool(&call.fn_name, call.fn_arguments.clone())
                                .await;
                            (call, output, run_id_spawn)
                        });
                    }

                    while let Some(joined) = join_set.join_next().await {
                        let (call, output, run_id) = match joined {
                            Ok(pair) => pair,
                            Err(error) => {
                                if let Some(gate) = completion_gate.as_ref() {
                                    if let Some(sink) = gate.sink.as_ref() {
                                        sink.notice(format!(
                                            "◈ parallel_tool_join_failed: {error}"
                                        ))
                                        .await;
                                    }
                                }
                                continue;
                            }
                        };
                        let tool_output = tool_output_from_invoke(output);
                        let tool_output_text = tool_output.to_string();
                        turn_ctx.tool_lane.messages.push(ChatMessage::from(ToolResponse::new(
                            call.call_id,
                            tool_output_text,
                        )));
                        if is_prepare_final_tool_name(&call.fn_name) {
                            prepare_final_in_batch = true;
                        }
                        invocations.push(ToolInvocation {
                            tool_name: call.fn_name.clone(),
                            tool_input: call.fn_arguments.clone(),
                            tool_output: tool_output.clone(),
                        });
                        if let Some(gate) = completion_gate.as_ref() {
                            if let Some(sink) = gate.sink.as_ref() {
                                let safe_input = crate::settings_guard::redact_json_value(
                                    &call.fn_arguments,
                                );
                                let safe_output =
                                    crate::settings_guard::redact_json_value(&tool_output);
                                crate::agent_runtime::tool_stream::emit_tool_run_finished(
                                    sink,
                                    &run_id,
                                    rounds_executed,
                                    invocations.last().expect("invocation"),
                                    crate::payload_receipt::receipt_meta(
                                        &safe_input,
                                        crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                                    ),
                                    crate::payload_receipt::receipt_meta(
                                        &safe_output,
                                        crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                                    ),
                                )
                                .await;
                            }
                        }
                    }
                } else {
                    for call in tool_calls {
                        let run_id = crate::agent_runtime::tool_stream::new_tool_run_id();
                        if let Some(gate) = completion_gate.as_ref() {
                            if let Some(sink) = gate.sink.as_ref() {
                                crate::agent_runtime::tool_stream::emit_tool_run_started(
                                    sink,
                                    &run_id,
                                    &call.fn_name,
                                    &call.fn_arguments,
                                    rounds_executed,
                                )
                                .await;
                            }
                        }
                        let tool_output = tool_output_from_invoke(
                            self.tool_registry
                                .invoke_tool(&call.fn_name, call.fn_arguments.clone())
                                .await,
                        );

                        if is_prepare_final_tool_name(&call.fn_name) {
                            prepare_final_in_batch = true;
                        }

                        let tool_output_text = tool_output.to_string();
                        turn_ctx.tool_lane.messages.push(ChatMessage::from(ToolResponse::new(
                            call.call_id,
                            tool_output_text,
                        )));
                        invocations.push(ToolInvocation {
                            tool_name: call.fn_name.clone(),
                            tool_input: call.fn_arguments.clone(),
                            tool_output: tool_output.clone(),
                        });
                        if let Some(gate) = completion_gate.as_ref() {
                            if let Some(sink) = gate.sink.as_ref() {
                                let safe_input = crate::settings_guard::redact_json_value(
                                    &call.fn_arguments,
                                );
                                let safe_output =
                                    crate::settings_guard::redact_json_value(&tool_output);
                                crate::agent_runtime::tool_stream::emit_tool_run_finished(
                                    sink,
                                    &run_id,
                                    rounds_executed,
                                    invocations.last().expect("invocation"),
                                    crate::payload_receipt::receipt_meta(
                                        &safe_input,
                                        crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                                    ),
                                    crate::payload_receipt::receipt_meta(
                                        &safe_output,
                                        crate::payload_receipt::DEFAULT_MAX_INLINE_BYTES,
                                    ),
                                )
                                .await;
                            }
                        }
                    }
                }

                let round_invocations = &invocations[invocations_before..];
                turn_ctx
                    .scratchpad
                    .record_round_digest_from_invocations(round_invocations);
                sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                if let Some(gate) = completion_gate.as_ref() {
                    let parent_for_handoff = gate
                        .handoff_parent_user_prompt
                        .as_deref()
                        .unwrap_or(shared_inputs.user_prompt.as_ref());
                    publish_host_handoff_snapshot(
                        gate.session_id.as_deref(),
                        gate.stream_turn_id,
                        gate.parent_turn_correlation_id.clone(),
                        parent_for_handoff,
                        &turn_ctx.scratchpad,
                        gate.host_handoff_slot.as_ref(),
                        gate.handoff_vibe_signature.clone(),
                        gate.handoff_model_avec,
                        gate.handoff_continuity_bundle.clone(),
                    )
                    .await;
                }

                if let Some(progress_message) =
                    turn_progress_message_from_invocations(round_invocations)
                {
                    if let Some(gate) = completion_gate.as_ref() {
                        if let Some(sink) = gate.sink.as_ref() {
                            sink.agent_turn_progress(
                                gate.stream_turn_id,
                                progress_message,
                                round_tool_names.clone(),
                            )
                            .await;
                        }
                    }
                }
                if let Some(note) = begin_work_note_from_invocations(round_invocations) {
                    turn_ctx.scratchpad.push_working_note(note);
                }

                if prepare_final_in_batch {
                    let workshop_lane = completion_gate
                        .as_ref()
                        .map(|gate| gate.skip_avec_ritual_check)
                        .unwrap_or(false);
                    if workshop_lane {
                        pending_final_answer = true;
                        turn_ctx.scratchpad.phase =
                            crate::agent_runtime::turn_context::TurnScratchPhase::Finalize;
                    } else if let Some(gate) = completion_gate.as_ref() {
                        if let Some(sink) = gate.sink.as_ref() {
                            sink.agent_turn_progress(
                                gate.stream_turn_id,
                                "Wrapping up your answer…".to_string(),
                                round_tool_names.clone(),
                            )
                            .await;
                        }
                    }
                }

                discipline.on_tool_round();
                if let Some(gate) = completion_gate.as_ref() {
                    persist_ledger_record(
                        gate.session_id.as_deref(),
                        &record_tool_round(
                            gate.stream_turn_id,
                            rounds_executed,
                            &round_tool_names,
                            &turn_ctx.scratchpad,
                        ),
                    );
                }

                if let Some(payload) = request_more_rounds_from_invocations(&invocations) {
                    if let Some(gate) = completion_gate.as_ref() {
                        if !gate.require_operator_budget_gate {
                            let headroom = gate
                                .tool_round_budget_ceiling
                                .saturating_sub(effective_max_tool_rounds);
                            let granted = payload
                                .requested_rounds
                                .max(1)
                                .min(headroom);
                            if granted > 0 {
                                effective_max_tool_rounds =
                                    effective_max_tool_rounds.saturating_add(granted);
                                push_turn_control_message(
                                    &mut turn_ctx.tool_lane.messages,
                                    &format!(
                                        "{TURN_CONTROL_PREFIX}\nRuntime extended tool budget by +{granted} (now {effective_max_tool_rounds}). Continue the task."
                                    ),
                                );
                                discipline.on_tool_round();
                                continue;
                            }
                        }
                    }
                    if let Some(gate) = completion_gate.as_ref() {
                        let create_result = turn_budget_request_store()
                            .create_and_register_wait(CreateTurnBudgetRequest {
                                turn_correlation_id: gate.parent_turn_correlation_id.clone(),
                                stream_turn_id: gate.stream_turn_id,
                                session_id: gate.session_id.clone(),
                                channel: gate.channel.clone(),
                                rounds_executed,
                                max_tool_rounds: effective_max_tool_rounds,
                                requested_rounds: payload.requested_rounds,
                                reason: payload.reason.clone(),
                                progress_summary: payload.progress_summary.clone(),
                            })
                            .await;
                        match create_result {
                            Ok((request_id, rx)) => {
                                if let Some(sink) = gate.sink.as_ref() {
                                    sink.turn_budget_approval_required(
                                        gate.stream_turn_id,
                                        request_id.clone(),
                                        rounds_executed,
                                        effective_max_tool_rounds,
                                        payload.requested_rounds,
                                        payload.reason.clone(),
                                        payload.progress_summary.clone(),
                                    )
                                    .await;
                                    sink.notice(format!(
                                        "◈ turn_budget_request id={request_id} at {rounds_executed}/{effective_max_tool_rounds} requested=+{}",
                                        payload.requested_rounds
                                    ))
                                    .await;
                                }
                                if let Some(stored) = gate.delivery_target.as_ref() {
                                    let target = crate::channel_delivery::ChannelDeliveryTarget::from(stored);
                                    let notify_payload =
                                        crate::turn_budget_notify::TurnBudgetNotifyPayload {
                                            request_id: request_id.clone(),
                                            rounds_executed,
                                            max_tool_rounds: effective_max_tool_rounds,
                                            requested_rounds: payload.requested_rounds,
                                            reason: payload.reason.clone(),
                                            progress_summary: payload.progress_summary.clone(),
                                        };
                                    tokio::spawn(async move {
                                        let client = reqwest::Client::new();
                                        if let Err(err) =
                                            crate::turn_budget_notify::notify_turn_budget_approval_required(
                                                &client,
                                                &target,
                                                notify_payload,
                                            )
                                            .await
                                        {
                                            eprintln!("turn budget channel notify failed: {err:#}");
                                        }
                                    });
                                }
                                let resolution = turn_budget_request_store()
                                    .wait_for_resolution(&request_id, rx)
                                    .await;
                                match resolution {
                                    BudgetResolution::Approved { granted_rounds } => {
                                        effective_max_tool_rounds = effective_max_tool_rounds
                                            .saturating_add(granted_rounds)
                                            .min(
                                                crate::turn_budget_request::ABSOLUTE_MAX_TOOL_ROUNDS,
                                            );
                                        push_turn_control_message(
                                            &mut turn_ctx.tool_lane.messages,
                                            &format!(
                                                "{TURN_CONTROL_PREFIX}\nOperator approved +{granted_rounds} tool rounds (budget now {effective_max_tool_rounds}). Continue the task."
                                            ),
                                        );
                                    }
                                    BudgetResolution::Denied => {
                                        push_turn_control_message(
                                            &mut turn_ctx.tool_lane.messages,
                                            &format!(
                                                "{TURN_CONTROL_PREFIX}\nOperator denied extra tool rounds. Wrap up with cognition_turn_finish, one clarifying question, or best-effort answer now."
                                            ),
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                push_turn_control_message(
                                    &mut turn_ctx.tool_lane.messages,
                                    &format!(
                                        "{TURN_CONTROL_PREFIX}\nExtra rounds unavailable: {err}. Finish with cognition_turn_finish or best effort."
                                    ),
                                );
                            }
                        }
                    }
                    continue;
                }

                if let Some(message) = finish_turn_from_invocations(&invocations) {
                    if let Some(gate) = completion_gate.as_ref() {
                        let tools = collect_tool_names(&invocations);
                        persist_ledger_record(
                            gate.session_id.as_deref(),
                            &record_finalized(
                                gate.stream_turn_id,
                                "cognition_turn_finish",
                                rounds_executed,
                                &tools,
                            ),
                        );
                    }
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: COGNITION_TURN_FINISH.to_string(),
                        tool_input: Value::Null,
                        tool_output: Value::Null,
                    });
                    return Ok(ToolLoopExecutionResponse {
                        text: message,
                        metadata: shared_inputs.context_clone(),
                        tool_name: last.tool_name,
                        tool_output: last.tool_output,
                        tool_invocations: invocations,
                        rounds_executed,
                        termination_reason: "cognition_turn_finish".to_string(),
                    });
                }

                if let Some(message) = checkpoint_turn_from_invocations(&invocations) {
                    if let Some(gate) = completion_gate.as_ref() {
                        let tools = collect_tool_names(&invocations);
                        persist_ledger_record(
                            gate.session_id.as_deref(),
                            &record_finalized(
                                gate.stream_turn_id,
                                "cognition_turn_checkpoint",
                                rounds_executed,
                                &tools,
                            ),
                        );
                    }
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: COGNITION_TURN_CHECKPOINT.to_string(),
                        tool_input: Value::Null,
                        tool_output: Value::Null,
                    });
                    return Ok(ToolLoopExecutionResponse {
                        text: message,
                        metadata: shared_inputs.context_clone(),
                        tool_name: last.tool_name,
                        tool_output: last.tool_output,
                        tool_invocations: invocations,
                        rounds_executed,
                        termination_reason: "cognition_turn_checkpoint".to_string(),
                    });
                }

                if let Some((work_id, ack)) =
                    workshop_entered_from_invocations(&invocations)
                {
                    let intent = invocations
                        .iter()
                        .find(|i| is_begin_work_tool_name(&i.tool_name))
                        .and_then(|i| i.tool_input.get("intent"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("general");
                    turn_ctx.scratchpad.set_delegate(&work_id, intent);
                    sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                    if let Some(gate) = completion_gate.as_ref() {
                        let parent_corr = gate
                            .parent_turn_correlation_id
                            .as_deref()
                            .unwrap_or("-");
                        let digest = turn_ctx.scratchpad.digest_hash();
                        persist_ledger_record(
                            gate.session_id.as_deref(),
                            &crate::agent_runtime::turn_ledger::TurnLedgerRecord {
                                timestamp: chrono::Utc::now(),
                                stream_turn_id: gate.stream_turn_id,
                                kind: crate::agent_runtime::turn_ledger::TurnLedgerEventKind::WorkDelegated,
                                detail: format!(
                                    "host_turn_ended workshop_entered work_id={work_id} intent={intent} parent_turn_correlation_id={parent_corr} scratch_digest={digest}"
                                ),
                                tools_invoked: ledger_tool_names(&invocations),
                                missing_tools: Vec::new(),
                                rounds_executed,
                                scratch: Some(turn_ctx.scratchpad.clone()),
                                active_profile_id: None,
                            },
                        );
                    }
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: COGNITION_TURN_BEGIN_WORK.to_string(),
                        tool_input: Value::Null,
                        tool_output: Value::Null,
                    });
                    return Ok(ToolLoopExecutionResponse {
                        text: ack,
                        metadata: shared_inputs.context_clone(),
                        tool_name: last.tool_name,
                        tool_output: last.tool_output,
                        tool_invocations: invocations,
                        rounds_executed,
                        termination_reason: "workshop_entered".to_string(),
                    });
                }

                if let Some((work_id, ack)) =
                    crate::agent_runtime::turn_worker_tools::worker_spawn_from_invocations(
                        &invocations,
                    )
                {
                    let intent = invocations
                        .iter()
                        .find(|i| i.tool_name == "cognition_spawn_turn_worker")
                        .and_then(|i| i.tool_input.get("intent"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("general");
                    turn_ctx.scratchpad.set_delegate(&work_id, intent);
                    sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                    if let Some(gate) = completion_gate.as_ref() {
                        let parent_corr = gate
                            .parent_turn_correlation_id
                            .as_deref()
                            .unwrap_or("-");
                        let digest = turn_ctx.scratchpad.digest_hash();
                        persist_ledger_record(
                            gate.session_id.as_deref(),
                            &crate::agent_runtime::turn_ledger::TurnLedgerRecord {
                                timestamp: chrono::Utc::now(),
                                stream_turn_id: gate.stream_turn_id,
                                kind: crate::agent_runtime::turn_ledger::TurnLedgerEventKind::WorkDelegated,
                                detail: format!(
                                    "host_turn_ended work_id={work_id} intent={intent} parent_turn_correlation_id={parent_corr} scratch_digest={digest}"
                                ),
                                tools_invoked: ledger_tool_names(&invocations),
                                missing_tools: Vec::new(),
                                rounds_executed,
                                scratch: Some(turn_ctx.scratchpad.clone()),
                                active_profile_id: None,
                            },
                        );
                    }
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: "cognition_spawn_turn_worker".to_string(),
                        tool_input: Value::Null,
                        tool_output: Value::Null,
                    });
                    return Ok(ToolLoopExecutionResponse {
                        text: ack,
                        metadata: shared_inputs.context_clone(),
                        tool_name: last.tool_name,
                        tool_output: last.tool_output,
                        tool_invocations: invocations,
                        rounds_executed,
                        termination_reason: "worker_spawned".to_string(),
                    });
                }
            }

            if !should_use_legacy_fallback {
                return Err(StasisError::PortFailure(format!(
                    "tool loop exceeded max rounds ({effective_max_tool_rounds}) without final response"
                )));
            }
        }

        if !should_use_legacy_fallback {
            return Err(StasisError::PortFailure(
                "no matching tools available for tool loop execution".to_string(),
            ));
        }

        let draft_text = if let Some(text) = fallback_draft_text {
            text
        } else {
            let mut first_request =
                PromptExecutionRequest::from_user_prompt(shared_inputs.user_prompt.to_string())
                    .with_context(shared_inputs.context_clone());
            if let Some(system_prompt) = shared_inputs.system_prompt.as_ref() {
                first_request = first_request.with_system_prompt(system_prompt.to_string());
            }
            self.prompt_pipeline.execute(first_request).await?.text
        };
        let tool_output = tool_output_from_invoke(
            self.tool_registry
                .invoke_tool(
                    shared_inputs.selected_tool_name(),
                    (*shared_inputs.tool_input).clone(),
                )
                .await,
        );

        let synthesis_prompt = build_fallback_synthesis_prompt(
            &shared_inputs.user_prompt,
            &draft_text,
            shared_inputs.selected_tool_name(),
            &tool_output,
        );

        let mut final_request = PromptExecutionRequest::from_user_prompt(synthesis_prompt)
            .with_context(shared_inputs.context_clone());
        if let Some(system_prompt) = shared_inputs.system_prompt.as_ref() {
            final_request = final_request.with_system_prompt(system_prompt.to_string());
        }

        let final_response = self.prompt_pipeline.execute(final_request).await?;

        let fallback_invocation = ToolInvocation {
            tool_name: shared_inputs.selected_tool_name().to_string(),
            tool_input: (*shared_inputs.tool_input).clone(),
            tool_output: tool_output.clone(),
        };

        Ok(ToolLoopExecutionResponse {
            text: final_response.text,
            metadata: final_response.metadata,
            tool_name: shared_inputs.selected_tool_name().to_string(),
            tool_output,
            tool_invocations: vec![fallback_invocation],
            rounds_executed,
            termination_reason: "legacy_fallback_no_model_tool_call".to_string(),
        })
    }
}

async fn apply_fsm_continue_loop(
    text: &str,
    continue_reason: ContinueReason,
    control_message: &str,
    missing_tools: &[String],
    invocations: &[ToolInvocation],
    turn_ctx: &mut HostTurnContext,
    loop_awareness: &mut TurnLoopAwareness,
    discipline: &mut TurnLoopDiscipline,
    tool_rounds_remaining: usize,
    mut completion_gate: Option<&mut ToolLoopCompletionGate<'_>>,
    shared_inputs: &ToolLoopSharedInputs,
    rounds_executed: usize,
    max_tool_rounds: usize,
) -> Result<Option<ToolLoopExecutionResponse>> {
    if !missing_tools.is_empty() {
        turn_ctx.scratchpad.set_open_gaps(missing_tools);
    }
    if let Some(gate) = completion_gate.as_mut() {
        let tools_invoked = if invocations.is_empty() {
            Vec::new()
        } else {
            ledger_tool_names(invocations)
        };
        persist_ledger_record(
            gate.session_id.as_deref(),
            &record_fsm_continue(
                gate.stream_turn_id,
                continue_reason,
                control_message,
                missing_tools,
                rounds_executed,
                &tools_invoked,
                &turn_ctx.scratchpad,
            ),
        );
        if let Some(slot) = gate.scratch_out.as_mut() {
            **slot = Some(turn_ctx.scratchpad.clone());
        }
    }
    if !text.trim().is_empty() {
        loop_awareness.record_user_response(text);
    }
    // Interim prose: surface the short note to the principal as a non-terminal
    // progress line so they SEE it, AND append it to `tool_lane.messages` so the
    // model retains continuity into the next round. Previously this prose was dropped
    // (to avoid self-dialogue), but combined with the scratch_reset that wipes the
    // draft it gave the model "amnesia" — it would forget what it just did and redo
    // finished work. Preserving the note keeps memory intact; the turn-control nudge
    // below still steers it to a tool / cognition_turn_finish, and the bounded
    // interim_continue_cap prevents the loop from spinning.
    if matches!(
        continue_reason,
        ContinueReason::InterimProse | ContinueReason::ExtendedProse
    ) && !text.trim().is_empty()
    {
        if let Some(gate) = completion_gate.as_ref() {
            if let Some(sink) = gate.sink.as_ref() {
                sink.agent_turn_progress(
                    gate.stream_turn_id,
                    text.trim().to_string(),
                    ledger_tool_names(invocations),
                )
                .await;
            }
        }
        turn_ctx
            .tool_lane
            .messages
            .push(ChatMessage::assistant(text.trim().to_string()));
    }
    push_turn_control_message(
        &mut turn_ctx.tool_lane.messages,
        &loop_awareness.wrap_control_body(tool_rounds_remaining, control_message),
    );
    push_turn_scratch_message_with_budget(
        &mut turn_ctx.tool_lane.messages,
        &turn_ctx.scratchpad,
        tool_rounds_remaining,
    );
    sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
    if discipline.on_text_only_continue(invocations.len()) {
        if let Some(gate) = completion_gate.as_ref() {
            return Ok(Some(
                finish_stuck_turn(shared_inputs, invocations.to_vec(), rounds_executed, gate).await?,
            ));
        }
        let text_only_limit = completion_gate
            .as_ref()
            .map(|gate| gate.max_text_only_stuck_continues)
            .unwrap_or_else(|| {
                crate::agent_runtime::turn_ledger::resolve_max_text_only_stuck_continues(
                    max_tool_rounds,
                )
            });
        return Ok(Some(finish_stuck_turn_response(
            shared_inputs,
            invocations.to_vec(),
            rounds_executed,
            text_only_limit,
            max_tool_rounds,
        )?));
    }
    Ok(None)
}

async fn finish_stuck_turn(
    shared_inputs: &ToolLoopSharedInputs,
    invocations: Vec<ToolInvocation>,
    rounds_executed: usize,
    gate: &ToolLoopCompletionGate<'_>,
) -> Result<ToolLoopExecutionResponse> {
    let tools = ledger_tool_names(&invocations);
    persist_ledger_record(
        gate.session_id.as_deref(),
        &record_stuck(
            gate.stream_turn_id,
            rounds_executed,
            &tools,
            gate.max_text_only_stuck_continues,
        ),
    );
    if let Some(sink) = gate.sink.as_ref() {
        sink.notice(format!(
            "◈ turn loop stuck: {} text-only continues without new tools (max_tool_rounds={})",
            gate.max_text_only_stuck_continues, gate.max_tool_rounds
        ))
        .await;
    }
    finish_stuck_turn_response(
        shared_inputs,
        invocations,
        rounds_executed,
        gate.max_text_only_stuck_continues,
        gate.max_tool_rounds,
    )
}

fn finish_stuck_turn_response(
    shared_inputs: &ToolLoopSharedInputs,
    invocations: Vec<ToolInvocation>,
    rounds_executed: usize,
    text_only_limit: usize,
    max_tool_rounds: usize,
) -> Result<ToolLoopExecutionResponse> {
    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
        tool_name: shared_inputs.selected_tool_name().to_string(),
        tool_input: (*shared_inputs.tool_input).clone(),
        tool_output: Value::Null,
    });
    Ok(ToolLoopExecutionResponse {
        text: stuck_turn_user_message(text_only_limit, max_tool_rounds, rounds_executed),
        metadata: shared_inputs.context_clone(),
        tool_name: last.tool_name,
        tool_output: last.tool_output,
        tool_invocations: invocations,
        rounds_executed,
        termination_reason: "stuck_text_only_continue".to_string(),
    })
}

/// Map tool-registry failures into JSON receipts so the model can recover in-loop.
fn tool_output_from_invoke(result: Result<Value>) -> Value {
    match result {
        Ok(value) => value,
        Err(err) => recoverable_tool_error_value(&err.to_string()),
    }
}

fn sync_scratch_snapshot(
    gate: Option<&mut ToolLoopCompletionGate<'_>>,
    scratch: &TurnScratchpad,
) {
    if let Some(gate) = gate {
        if let Some(slot) = gate.scratch_out.as_mut() {
            **slot = Some(scratch.clone());
        }
    }
}

fn recoverable_tool_error_value(message: &str) -> Value {
    serde_json::json!({
        "ok": false,
        "error": message,
        "recoverable": true,
        "hint": "Read the error, fix arguments or choose another allowed tool, retry once if policy allows; delegate via cognition_spawn_turn_worker when the host profile blocks direct execution."
    })
}

fn build_fallback_synthesis_prompt(
    user_prompt: &str,
    draft_text: &str,
    tool_name: &str,
    tool_output: &Value,
) -> String {
    let tool_output_text = tool_output.to_string();
    let mut prompt = String::with_capacity(
        user_prompt.len() + draft_text.len() + tool_name.len() + tool_output_text.len() + 128,
    );
    prompt.push_str("User request:\n");
    prompt.push_str(user_prompt);
    prompt.push_str("\n\nDraft analysis:\n");
    prompt.push_str(draft_text);
    prompt.push_str("\n\nTool '");
    prompt.push_str(tool_name);
    prompt.push_str("' output JSON:\n");
    prompt.push_str(&tool_output_text);
    prompt.push_str("\n\nProduce final answer grounded in the tool output.");
    prompt
}

#[cfg(test)]
mod tests {
    use crate::turn_control_tools::finish_turn_from_invocations;
    use super::{recoverable_tool_error_value, tool_output_from_invoke};
    use stasis::domain::errors::StasisError;

    #[test]
    fn tool_invoke_failure_becomes_recoverable_receipt() {
        let out = tool_output_from_invoke(Err(StasisError::PortFailure(
            "tool not allowed in this turn profile: cognition_mcp_invoke".to_string(),
        )));
        assert_eq!(out["ok"], false);
        assert_eq!(out["recoverable"], true);
        assert!(out["error"]
            .as_str()
            .unwrap()
            .contains("cognition_mcp_invoke"));
    }

    #[test]
    fn recoverable_tool_error_has_hint() {
        let out = recoverable_tool_error_value("boom");
        assert_eq!(out["error"], "boom");
        assert!(out["hint"].as_str().unwrap().contains("spawn_turn_worker"));
    }

    #[test]
    fn celebratory_preamble_after_tools_continues_extended() {
        use crate::agent_runtime::turn_completion_fsm::{
            decide_after_tools_text_round, AfterToolsRoundContext, ContinueReason,
            TurnRoundAction,
        };
        use crate::turn_text_heuristics::is_extended_prose;
        use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
        let preamble = "Yesss! Let's do this — I'll pull up the current context, check what's \
                          resonating in memory, and calibrate to a focused AVEC posture. Boom — \
                          focused preset pulled. Let me lock it in and then call cognition_turn_finish \
                          once the full calibration summary is ready for you to read.";
        assert!(is_extended_prose(preamble));
        let invocations = vec![
            ToolInvocation {
                tool_name: "cognition_memory_moods".to_string(),
                tool_input: serde_json::json!(null),
                tool_output: serde_json::json!(null),
            },
            ToolInvocation {
                tool_name: "cognition_memory_calibrate".to_string(),
                tool_input: serde_json::json!(null),
                tool_output: serde_json::json!(null),
            },
        ];
        let action = decide_after_tools_text_round(&AfterToolsRoundContext {
            draft_text: preamble.to_string(),
            pending_final_answer: false,
            rounds_executed: 3,
            max_tool_rounds: 10,
            invocations: &invocations,
            workshop_lane: false,
            interim_continues_used: 0,
            interim_continue_cap: 2,
            host_scheduler_lane: false,
        });
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ExtendedProse,
                ..
            }
        ));
    }

    #[test]
    fn prose_requires_finish_substitutes_stub_for_terminal_body() {
        use crate::turn_control_tools::{terminal_text_for_fsm_end, PROSE_REQUIRES_FINISH_STUB};
        let text = terminal_text_for_fsm_end(
            "prose_requires_finish",
            "I'll summarize everything next.".to_string(),
        );
        assert_eq!(text, PROSE_REQUIRES_FINISH_STUB);
        assert_eq!(
            terminal_text_for_fsm_end("clarifying_question", "Which repo?".to_string()),
            "Which repo?"
        );
    }

    #[test]
    fn checkpoint_turn_from_invocations_is_detected_for_loop_exit() {
        use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
        let invocations = vec![ToolInvocation {
            tool_name: "cognition_turn_checkpoint".to_string(),
            tool_input: serde_json::json!({"message": "Here is progress so far."}),
            tool_output: serde_json::json!({"ok": true, "checkpoint_turn": true}),
        }];
        assert_eq!(
            crate::turn_control_tools::checkpoint_turn_from_invocations(&invocations).as_deref(),
            Some("Here is progress so far.")
        );
    }

    #[test]
    fn finish_turn_from_invocations_is_detected_for_loop_exit() {
        use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
        let invocations = vec![ToolInvocation {
            tool_name: "cognition_turn_finish".to_string(),
            tool_input: serde_json::json!({"message": "Final answer ready."}),
            tool_output: serde_json::json!({"ok": true, "finish_turn": true}),
        }];
        assert_eq!(
            finish_turn_from_invocations(&invocations).as_deref(),
            Some("Final answer ready.")
        );
    }

    #[test]
    fn no_tool_debt_fuses_at_max_rounds() {
        use crate::agent_runtime::turn_completion_fsm::{
            decide_no_tool_debt_text_round, NoToolDebtRoundContext, TurnRoundAction,
        };
        let action = decide_no_tool_debt_text_round(&NoToolDebtRoundContext {
            draft_text: "Let me check.".to_string(),
            pending_final_answer: false,
            rounds_executed: 10,
            max_tool_rounds: 10,
            interim_continues_used: 0,
            interim_continue_cap: 2,
            host_scheduler_lane: false,
        });
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "max_rounds_fuse"
            }
        ));
    }
}

fn sanitize_tool_name_for_model(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.to_string()
    }
}
