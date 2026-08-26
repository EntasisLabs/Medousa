//! Portable Medousa tool loop with policy-coherent parallel tool-call batches.

use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use genai::chat::{ChatMessage, ChatRequest, ChatRole, ContentPart, MessageContent, ToolResponse};
use medousa_engine::TurnScratchpad;
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

use crate::checkpoint::{
    ActiveTurnCheckpointStatus, ActiveTurnCounters, CheckpointToolInvocation,
    OutstandingTurnBoundary, SafeCheckpointBoundary, TOOL_ROUND_BUDGET_EXHAUSTED_REASON,
    ToolLoopCheckpointState,
};
use crate::completion_fsm::{
    AfterToolsRoundContext, ContinueReason, NoToolDebtRoundContext, TurnCompletionProfile,
    TurnRoundAction, decide_after_tools_text_round, decide_no_tool_debt_text_round,
};
use crate::execution_boundary::{
    TurnExecutionBoundaryError, active_turn_execution_boundary, await_turn_boundary,
    with_turn_execution_boundary,
};
use crate::execution_policy::{
    ParallelExecutionSettings, ParallelExecutionSettingsProvider, parallel_tool_batch_allowed,
};
use crate::loop_gate::{
    DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS, ToolLoopCompletionGate, collect_tool_names,
};
use crate::loop_state::{
    TURN_CONTROL_PREFIX, TurnLedgerEventKind, TurnLedgerRecord, TurnLoopAwareness,
    TurnLoopDiscipline, ledger_tool_names, push_turn_control_message, record_finalized,
    record_fsm_continue, record_stuck, record_tool_round, resolve_max_text_only_stuck_continues,
    stuck_turn_user_message,
};
use crate::perception::ToolPerceptionGovernor;
use crate::ports::{
    ToolRunFinish, ToolRunStart, TurnBudgetApprovalRequest, TurnBudgetApprovalResolution,
};
use crate::turn_context::{
    HostTurnContext, push_turn_scratch_message_with_budget, record_round_digest_from_invocations,
};
use crate::turn_control::{
    ABSOLUTE_MAX_TOOL_ROUNDS, COGNITION_TURN, COGNITION_WORKSHOP_MUTATE,
    begin_work_note_from_invocations, checkpoint_turn_from_invocations,
    finish_turn_from_invocations, is_begin_work_tool_name, is_terminal_turn_tool_name,
    is_workshop_spawn_call, request_input_from_invocations, request_more_rounds_from_invocations,
    terminal_text_for_fsm_end, turn_progress_message_from_invocations,
    worker_spawn_from_invocations, workshop_entered_from_invocations,
};

const DEFAULT_MAX_TOOL_ROUNDS: usize = DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS;

fn turn_boundary_failure(operation: &str, error: TurnExecutionBoundaryError) -> StasisError {
    StasisError::PortFailure(format!("{error} during {operation}"))
}

async fn await_turn_result<F, T>(operation: &str, future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    await_turn_boundary(future)
        .await
        .map_err(|error| turn_boundary_failure(operation, error))?
}

#[derive(Clone)]
pub struct MedousaToolLoopPipeline {
    prompt_pipeline: PromptExecutionPipeline,
    tool_registry: Arc<dyn ToolRegistry>,
    parallel_execution_settings_provider: Arc<dyn ParallelExecutionSettingsProvider>,
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
            parallel_execution_settings_provider: Arc::new(ParallelExecutionSettings::default),
        }
    }

    pub fn with_parallel_execution_settings(mut self, settings: ParallelExecutionSettings) -> Self {
        self.parallel_execution_settings_provider = Arc::new(move || settings.clone());
        self
    }

    pub fn with_parallel_execution_settings_provider(
        mut self,
        provider: Arc<dyn ParallelExecutionSettingsProvider>,
    ) -> Self {
        self.parallel_execution_settings_provider = provider;
        self
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
        self.execute_with_defaults(request, prior_messages, None)
            .await
    }

    pub async fn execute_with_stream(
        &self,
        request: ToolLoopExecutionRequest,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_with_defaults(request, Vec::new(), chunk_tx)
            .await
    }

    pub async fn execute_with_stream_prior_messages(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_with_defaults(request, prior_messages, chunk_tx)
            .await
    }

    pub async fn execute_with_stream_prior_messages_max_rounds(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
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
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_internal(
            request,
            prior_messages,
            chunk_tx,
            DEFAULT_MAX_TOOL_ROUNDS,
            None,
            None,
        )
        .await
    }

    async fn execute_internal(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
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
        let parallel_settings = self.parallel_execution_settings_provider.load();

        let user_message = current_turn_user_message
            .unwrap_or_else(|| ChatMessage::user(shared_inputs.user_prompt.to_string()));
        let mut turn_ctx =
            HostTurnContext::new_with_user_message(prior_messages, user_message.clone());
        let resume_state = completion_gate
            .as_mut()
            .and_then(|gate| gate.active_turn_resume.take());
        if let Some(resume) = resume_state.as_ref() {
            turn_ctx.user_lane_prefix = resume.transcript.user_lane_prefix.clone();
            if turn_ctx.user_lane_prefix.is_empty() {
                turn_ctx
                    .user_lane_prefix
                    .push(ChatMessage::user(shared_inputs.user_prompt.to_string()));
            }
            turn_ctx.tool_lane.messages = resume.transcript.tool_lane_messages.clone();
            if resume.append_current_user_message {
                turn_ctx.tool_lane.messages.push(user_message);
            }
            turn_ctx.scratchpad = resume.scratch.clone();
        } else if let Some(gate) = completion_gate.as_ref()
            && let Some(seed) = gate.initial_worker_scratch.as_ref()
        {
            turn_ctx.scratchpad = seed.clone();
        }

        let mut tools =
            await_turn_result("tool catalog lookup", self.tool_registry.list_tools()).await?;
        if has_selected_tool {
            let selected_sanitized =
                sanitize_tool_name_for_model(shared_inputs.selected_tool_name());
            let selected_prefix = format!("{selected_sanitized}_");
            tools.retain(|tool| {
                let name = tool.name.as_str();
                name == shared_inputs.selected_tool_name()
                    || name == selected_sanitized
                    || name.starts_with(&selected_prefix)
            });
        }

        let mut invocations = resume_state
            .as_ref()
            .map(|resume| {
                resume
                    .invocations
                    .clone()
                    .into_iter()
                    .map(CheckpointToolInvocation::into_runtime)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut should_use_legacy_fallback = false;
        let mut fallback_draft_text: Option<String> = None;
        let mut rounds_executed = resume_state
            .as_ref()
            .filter(|resume| resume.restore_turn_budget)
            .map(|resume| resume.counters.model_rounds_executed)
            .unwrap_or(0);
        if let Some(resume) = resume_state
            .as_ref()
            .filter(|resume| resume.restore_turn_budget)
            && resume.counters.max_tool_rounds > 0
        {
            effective_max_tool_rounds = resume.counters.max_tool_rounds;
        }
        let mut tool_batches_completed = resume_state
            .as_ref()
            .filter(|resume| resume.restore_turn_budget)
            .map(|resume| resume.counters.tool_batches_completed)
            .unwrap_or(0);
        let max_text_only_stuck = completion_gate
            .as_ref()
            .map(|gate| gate.max_text_only_stuck_continues)
            .unwrap_or_else(|| resolve_max_text_only_stuck_continues(effective_max_tool_rounds));
        let mut discipline =
            TurnLoopDiscipline::with_max_text_only_stuck_continues(max_text_only_stuck);
        let mut loop_awareness = TurnLoopAwareness::default();
        if let Some(resume) = resume_state
            .as_ref()
            .filter(|resume| resume.restore_turn_budget)
        {
            discipline.restore_checkpoint_state(
                resume.counters.text_only_continues_without_new_tools,
                resume.counters.invocations_at_last_text_continue,
            );
            loop_awareness.restore(
                resume.counters.user_responses_sent,
                resume.counters.last_response_preview.clone(),
            );
        }
        let completion_profile = completion_gate
            .as_ref()
            .map(|gate| gate.completion_profile)
            .unwrap_or(TurnCompletionProfile::ForegroundPrincipal);
        let perception_evidence = completion_gate
            .as_ref()
            .and_then(|gate| gate.runtime_ports.perception_evidence());
        let mut perception_governor = ToolPerceptionGovernor::new(perception_evidence);

        // Every durable boundary snapshots the same complete state vector. Keep
        // that capture centralized so new counters cannot drift between paths.
        macro_rules! persist_checkpoint {
            ($boundary:expr, $status:expr, $reason:expr, $outstanding:expr, $tools:expr, $call_ids:expr $(,)?) => {{
                persist_loop_checkpoint(
                    completion_gate.as_deref(),
                    $boundary,
                    $status,
                    $reason,
                    $outstanding,
                    &turn_ctx,
                    &invocations,
                    rounds_executed,
                    effective_max_tool_rounds,
                    tool_batches_completed,
                    &discipline,
                    &loop_awareness,
                    $tools,
                    $call_ids,
                )
            }};
        }

        persist_checkpoint!(
            SafeCheckpointBoundary::TurnStarted,
            ActiveTurnCheckpointStatus::Active,
            None,
            None,
            &[],
            &[],
        );

        if !tools.is_empty() {
            while rounds_executed < effective_max_tool_rounds {
                rounds_executed += 1;
                if let Some(gate) = completion_gate.as_ref() {
                    if let Some(work_id) = gate.cancel_poll_work_id.as_deref() {
                        let control = gate.runtime_ports.delegation_control().ok_or_else(|| {
                            StasisError::PortFailure(
                                "worker cancellation polling requires a delegation-control port"
                                    .to_string(),
                            )
                        })?;
                        if control.is_cancelled(work_id) {
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
                        let control = gate.runtime_ports.delegation_control().ok_or_else(|| {
                            StasisError::PortFailure(
                                "worker steer polling requires a delegation-control port"
                                    .to_string(),
                            )
                        })?;
                        let steers = control.drain_steer_messages(work_id);
                        for steer in steers {
                            let speaker = steer
                                .speaker_profile_id
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(|value| format!(" speaker={value}"))
                                .unwrap_or_default();
                            push_turn_control_message(
                                &mut turn_ctx.tool_lane.messages,
                                &format!(
                                    "[MEDOUSA_WORKSHOP_STEER{speaker}]\n{}",
                                    steer.text.trim()
                                ),
                            );
                        }
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
                let mut messages =
                    turn_ctx.build_model_messages(shared_inputs.system_prompt.as_deref());
                ensure_assistant_tool_turn_reasoning(&mut messages);
                let chat_request = ChatRequest::new(messages).with_tools(tools.clone());
                let response = match chunk_tx {
                    Some(tx) => {
                        match await_turn_result(
                            "streaming model completion",
                            complete_chat_stream_once(
                                &self.prompt_pipeline,
                                chat_request.clone(),
                                shared_inputs.context_clone(),
                                Some(tx),
                            ),
                        )
                        .await?
                        {
                            ChatCompletionOutcome::Ok(response) => *response,
                            ChatCompletionOutcome::MalformedToolJson => {
                                complete_model_response(
                                    completion_gate.as_deref(),
                                    rounds_executed,
                                )
                                .await;
                                inject_malformed_tool_json_guidance(
                                    &mut turn_ctx.tool_lane.messages,
                                    completion_gate.as_deref(),
                                )
                                .await;
                                discipline.on_tool_round();
                                persist_checkpoint!(
                                    SafeCheckpointBoundary::ModelResponseCompleted,
                                    ActiveTurnCheckpointStatus::Active,
                                    None,
                                    None,
                                    &[],
                                    &[],
                                );
                                continue;
                            }
                        }
                    }
                    None => {
                        match await_turn_result(
                            "model completion",
                            complete_chat_once(
                                &self.prompt_pipeline,
                                chat_request.clone(),
                                shared_inputs.context_clone(),
                            ),
                        )
                        .await?
                        {
                            ChatCompletionOutcome::Ok(response) => *response,
                            ChatCompletionOutcome::MalformedToolJson => {
                                complete_model_response(
                                    completion_gate.as_deref(),
                                    rounds_executed,
                                )
                                .await;
                                inject_malformed_tool_json_guidance(
                                    &mut turn_ctx.tool_lane.messages,
                                    completion_gate.as_deref(),
                                )
                                .await;
                                discipline.on_tool_round();
                                persist_checkpoint!(
                                    SafeCheckpointBoundary::ModelResponseCompleted,
                                    ActiveTurnCheckpointStatus::Active,
                                    None,
                                    None,
                                    &[],
                                    &[],
                                );
                                continue;
                            }
                        }
                    }
                };
                complete_model_response(completion_gate.as_deref(), rounds_executed).await;
                let maybe_text = response
                    .first_text()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let reasoning_content = response.reasoning_content.clone();
                let assistant_content = response.content.clone();
                let tool_calls = response.into_tool_calls();

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

                        let action = if invocations.is_empty() {
                            decide_no_tool_debt_text_round(&NoToolDebtRoundContext {
                                draft_text: text.clone(),
                                completion_profile,
                            })
                        } else {
                            decide_after_tools_text_round(&AfterToolsRoundContext {
                                draft_text: text.clone(),
                                rounds_executed,
                                max_tool_rounds: effective_max_tool_rounds,
                                completion_profile,
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
                                    persist_gate_ledger(
                                        gate,
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
                                persist_checkpoint!(
                                    SafeCheckpointBoundary::Terminal,
                                    ActiveTurnCheckpointStatus::Completed,
                                    Some(termination_reason),
                                    None,
                                    &[],
                                    &[],
                                );
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
                                    persist_checkpoint!(
                                        SafeCheckpointBoundary::Terminal,
                                        ActiveTurnCheckpointStatus::Completed,
                                        Some(&response.termination_reason),
                                        None,
                                        &[],
                                        &[],
                                    );
                                    return Ok(response);
                                }
                                persist_checkpoint!(
                                    SafeCheckpointBoundary::ModelResponseCompleted,
                                    ActiveTurnCheckpointStatus::Active,
                                    None,
                                    None,
                                    &[],
                                    &[],
                                );
                                continue;
                            }
                        }
                    } else {
                        return Err(StasisError::PortFailure(
                            "chat response was empty after tool loop".to_string(),
                        ));
                    }
                }

                let terminal_calls_in_batch = tool_calls
                    .iter()
                    .filter(|call| is_terminal_turn_tool_name(&call.fn_name, &call.fn_arguments))
                    .count();
                let mixed_terminal_batch =
                    terminal_calls_in_batch > 0 && terminal_calls_in_batch < tool_calls.len();
                turn_ctx
                    .tool_lane
                    .messages
                    .push(assistant_tool_round_message(
                        assistant_content,
                        reasoning_content,
                    ));

                let invocations_before = invocations.len();
                let batch: Vec<(String, Value)> = tool_calls
                    .iter()
                    .map(|call| (call.fn_name.clone(), call.fn_arguments.clone()))
                    .collect();

                let use_parallel = parallel_tool_batch_allowed(&batch, &parallel_settings).is_ok();
                let model_result_budget =
                    perception_governor.result_budget_for_batch(tool_calls.len());

                let round_tool_names: Vec<String> =
                    tool_calls.iter().map(|call| call.fn_name.clone()).collect();
                let round_provider_call_ids: Vec<String> =
                    tool_calls.iter().map(|call| call.call_id.clone()).collect();
                let round_tool_calls = tool_calls.clone();
                let mut completed_provider_call_ids = HashSet::new();

                if use_parallel && tool_calls.len() > 1 {
                    let mut join_set = tokio::task::JoinSet::new();
                    for call in tool_calls.clone() {
                        let tool_run_id = start_tool_run(
                            completion_gate.as_deref(),
                            &call.fn_name,
                            &call.fn_arguments,
                            rounds_executed,
                        )
                        .await;
                        let registry = self.tool_registry.clone();
                        let execution_boundary =
                            active_turn_execution_boundary().ok_or_else(|| {
                                turn_boundary_failure(
                                    "parallel tool invocation",
                                    TurnExecutionBoundaryError::MissingContext,
                                )
                            })?;
                        join_set.spawn(async move {
                            let output = with_turn_execution_boundary(execution_boundary, async {
                                await_turn_boundary(
                                    registry.invoke_tool(&call.fn_name, call.fn_arguments.clone()),
                                )
                                .await
                            })
                            .await;
                            (call, output, tool_run_id)
                        });
                    }

                    while let Some(joined) = join_set.join_next().await {
                        let (call, output, tool_run_id) = match joined {
                            Ok(pair) => pair,
                            Err(error) => {
                                if let Some(gate) = completion_gate.as_ref()
                                    && let Some(presentation) =
                                        gate.runtime_ports.turn_presentation()
                                {
                                    presentation
                                        .notice(format!("◈ parallel_tool_join_failed: {error}"))
                                        .await;
                                }
                                continue;
                            }
                        };
                        let output = output.map_err(|error| {
                            turn_boundary_failure("parallel tool invocation", error)
                        })?;
                        let tool_output = tool_output_from_invoke(output);
                        let tool_output_text = perception_governor
                            .observe_for_call(
                                &call.fn_name,
                                Some(&call.call_id),
                                &tool_output,
                                model_result_budget,
                            )
                            .to_string();
                        turn_ctx
                            .tool_lane
                            .messages
                            .push(ChatMessage::from(ToolResponse::new(
                                call.call_id.clone(),
                                tool_output_text,
                            )));
                        completed_provider_call_ids.insert(call.call_id.clone());
                        invocations.push(ToolInvocation {
                            tool_name: call.fn_name.clone(),
                            tool_input: call.fn_arguments.clone(),
                            tool_output: tool_output.clone(),
                        });
                        finish_tool_run(
                            completion_gate.as_deref(),
                            tool_run_id.as_deref(),
                            rounds_executed,
                            invocations.last().expect("invocation"),
                        )
                        .await;
                    }
                } else {
                    for call in tool_calls {
                        let tool_run_id = start_tool_run(
                            completion_gate.as_deref(),
                            &call.fn_name,
                            &call.fn_arguments,
                            rounds_executed,
                        )
                        .await;
                        let output = await_turn_boundary(
                            self.tool_registry
                                .invoke_tool(&call.fn_name, call.fn_arguments.clone()),
                        )
                        .await
                        .map_err(|error| turn_boundary_failure("tool invocation", error))?;
                        let tool_output = tool_output_from_invoke(output);

                        let tool_output_text = perception_governor
                            .observe_for_call(
                                &call.fn_name,
                                Some(&call.call_id),
                                &tool_output,
                                model_result_budget,
                            )
                            .to_string();
                        turn_ctx
                            .tool_lane
                            .messages
                            .push(ChatMessage::from(ToolResponse::new(
                                call.call_id.clone(),
                                tool_output_text,
                            )));
                        completed_provider_call_ids.insert(call.call_id.clone());
                        invocations.push(ToolInvocation {
                            tool_name: call.fn_name.clone(),
                            tool_input: call.fn_arguments.clone(),
                            tool_output: tool_output.clone(),
                        });
                        finish_tool_run(
                            completion_gate.as_deref(),
                            tool_run_id.as_deref(),
                            rounds_executed,
                            invocations.last().expect("invocation"),
                        )
                        .await;
                    }
                }

                // A panicked/cancelled parallel task must still close its provider
                // call id before this transcript is eligible for persistence. The
                // synthetic receipt is evidence of uncertainty; it is never a
                // replay of the missing side effect.
                for call in round_tool_calls
                    .iter()
                    .filter(|call| !completed_provider_call_ids.contains(call.call_id.as_str()))
                {
                    let tool_output = serde_json::json!({
                        "ok": false,
                        "error": "parallel tool task ended without a result",
                        "recovery": "effect is uncertain; inspect activity and governed environment before retrying",
                    });
                    turn_ctx.tool_lane.messages.push(ChatMessage::from(
                        ToolResponse::from_tool_call(call, tool_output.to_string()),
                    ));
                    invocations.push(ToolInvocation {
                        tool_name: call.fn_name.clone(),
                        tool_input: call.fn_arguments.clone(),
                        tool_output,
                    });
                }

                let round_invocations = &invocations[invocations_before..];
                tool_batches_completed = tool_batches_completed.saturating_add(1);
                record_round_digest_from_invocations(&mut turn_ctx.scratchpad, round_invocations);
                sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                if let Some(provider) = completion_gate
                    .as_ref()
                    .and_then(|gate| gate.round_context_provider.as_ref())
                    && let Some(context) = provider.context_for_next_round()?
                {
                    let context = perception_governor.observe_round_context(&context);
                    turn_ctx
                        .tool_lane
                        .messages
                        .push(ChatMessage::system(context));
                }
                let perception_metrics = perception_governor.take_round_metrics();
                if perception_metrics.has_governor_activity()
                    && let Some(gate) = completion_gate.as_ref()
                    && let Some(presentation) = gate.runtime_ports.turn_presentation()
                {
                    presentation
                        .notice(perception_metrics.telemetry_line(rounds_executed))
                        .await;
                }
                // A mode-owned registry may reveal a narrower or wider model-visible
                // subset between rounds, but it cannot change its authority superset.
                if !has_selected_tool {
                    tools =
                        await_turn_result("tool catalog refresh", self.tool_registry.list_tools())
                            .await?;
                }
                if let Some(handoff) = completion_gate
                    .as_ref()
                    .and_then(|gate| gate.runtime_ports.host_handoff())
                {
                    handoff.publish(turn_ctx.scratchpad.clone()).await;
                }

                if let Some(progress_message) =
                    turn_progress_message_from_invocations(round_invocations)
                    && let Some(gate) = completion_gate.as_ref()
                    && let Some(presentation) = gate.runtime_ports.turn_presentation()
                {
                    presentation
                        .turn_progress(
                            gate.stream_turn_id,
                            progress_message,
                            round_tool_names.clone(),
                        )
                        .await;
                }
                if let Some(note) = begin_work_note_from_invocations(round_invocations) {
                    turn_ctx.scratchpad.push_working_note(note);
                }

                discipline.on_tool_round();
                if let Some(gate) = completion_gate.as_ref() {
                    persist_gate_ledger(
                        gate,
                        &record_tool_round(
                            gate.stream_turn_id,
                            rounds_executed,
                            &round_tool_names,
                            &turn_ctx.scratchpad,
                        ),
                    );
                }

                persist_checkpoint!(
                    SafeCheckpointBoundary::ToolBatchCompleted,
                    ActiveTurnCheckpointStatus::Active,
                    None,
                    None,
                    &round_tool_names,
                    &round_provider_call_ids,
                );

                if let Some(payload) = request_more_rounds_from_invocations(round_invocations) {
                    if let Some(gate) = completion_gate.as_ref()
                        && !gate.require_operator_budget_gate
                    {
                        let extension_ceiling = gate
                            .hard_tool_round_ceiling
                            .unwrap_or(gate.tool_round_budget_ceiling);
                        let headroom = extension_ceiling.saturating_sub(effective_max_tool_rounds);
                        let granted = payload.requested_rounds.max(1).min(headroom);
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
                            persist_checkpoint!(
                                SafeCheckpointBoundary::ToolBatchCompleted,
                                ActiveTurnCheckpointStatus::Active,
                                None,
                                None,
                                &round_tool_names,
                                &round_provider_call_ids,
                            );
                            continue;
                        }
                    }
                    if let Some(gate) = completion_gate.as_ref() {
                        if gate
                            .hard_tool_round_ceiling
                            .is_some_and(|ceiling| effective_max_tool_rounds >= ceiling)
                        {
                            push_turn_control_message(
                                &mut turn_ctx.tool_lane.messages,
                                &format!(
                                    "{TURN_CONTROL_PREFIX}\nThis mode's hard ceiling is {effective_max_tool_rounds} model rounds; extra rounds cannot be granted. Continue within the remaining budget and checkpoint before the limit if needed."
                                ),
                            );
                            continue;
                        }
                        let create_result = match gate.runtime_ports.budget_approval() {
                            Some(approval) => {
                                approval
                                    .begin(TurnBudgetApprovalRequest {
                                        rounds_executed,
                                        max_tool_rounds: effective_max_tool_rounds,
                                        requested_rounds: payload.requested_rounds,
                                        reason: payload.reason.clone(),
                                        progress_summary: payload.progress_summary.clone(),
                                    })
                                    .await
                            }
                            None => Err("operator budget approval port unavailable".to_string()),
                        };
                        match create_result {
                            Ok(pending) => {
                                let request_id = pending.request_id.clone();
                                persist_checkpoint!(
                                    SafeCheckpointBoundary::AwaitingApproval,
                                    ActiveTurnCheckpointStatus::Active,
                                    None,
                                    Some(OutstandingTurnBoundary::BudgetApproval {
                                        request_id,
                                        requested_rounds: payload.requested_rounds,
                                    }),
                                    &round_tool_names,
                                    &round_provider_call_ids,
                                );
                                match pending.resolve().await {
                                    TurnBudgetApprovalResolution::Approved { granted_rounds } => {
                                        effective_max_tool_rounds = effective_max_tool_rounds
                                            .saturating_add(granted_rounds)
                                            .min(ABSOLUTE_MAX_TOOL_ROUNDS)
                                            .min(
                                                gate.hard_tool_round_ceiling.unwrap_or(usize::MAX),
                                            );
                                        push_turn_control_message(
                                            &mut turn_ctx.tool_lane.messages,
                                            &format!(
                                                "{TURN_CONTROL_PREFIX}\nOperator approved +{granted_rounds} tool rounds (budget now {effective_max_tool_rounds}). Continue the task."
                                            ),
                                        );
                                    }
                                    TurnBudgetApprovalResolution::Denied => {
                                        push_turn_control_message(
                                            &mut turn_ctx.tool_lane.messages,
                                            &format!(
                                                "{TURN_CONTROL_PREFIX}\nOperator denied extra tool rounds. Wrap up with cognition_turn action=turn.finish, one clarifying question, or best-effort answer now."
                                            ),
                                        );
                                    }
                                }
                                persist_checkpoint!(
                                    SafeCheckpointBoundary::ToolBatchCompleted,
                                    ActiveTurnCheckpointStatus::Active,
                                    None,
                                    None,
                                    &round_tool_names,
                                    &round_provider_call_ids,
                                );
                            }
                            Err(err) => {
                                push_turn_control_message(
                                    &mut turn_ctx.tool_lane.messages,
                                    &format!(
                                        "{TURN_CONTROL_PREFIX}\nExtra rounds unavailable: {err}. Finish with cognition_turn action=turn.finish or best effort."
                                    ),
                                );
                            }
                        }
                    }
                    continue;
                }

                if !mixed_terminal_batch
                    && let Some(message) = finish_turn_from_invocations(round_invocations)
                {
                    // The response's chronological prose is authoritative.
                    // turn.finish.message exists only for providers that cannot
                    // emit prose and a tool call in the same response.
                    let message = maybe_text.clone().unwrap_or(message);
                    if let Some(gate) = completion_gate.as_ref() {
                        let tools = collect_tool_names(&invocations);
                        persist_gate_ledger(
                            gate,
                            &record_finalized(
                                gate.stream_turn_id,
                                "cognition_turn_finish",
                                rounds_executed,
                                &tools,
                            ),
                        );
                    }
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: COGNITION_TURN.to_string(),
                        tool_input: serde_json::json!({ "action": "turn.finish" }),
                        tool_output: Value::Null,
                    });
                    persist_checkpoint!(
                        SafeCheckpointBoundary::Terminal,
                        ActiveTurnCheckpointStatus::Completed,
                        Some("cognition_turn_finish"),
                        None,
                        &round_tool_names,
                        &round_provider_call_ids,
                    );
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

                if !mixed_terminal_batch
                    && let Some(message) = request_input_from_invocations(round_invocations)
                {
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: COGNITION_TURN.to_string(),
                        tool_input: serde_json::json!({ "action": "turn.request_input" }),
                        tool_output: Value::Null,
                    });
                    persist_checkpoint!(
                        SafeCheckpointBoundary::AwaitingUser,
                        ActiveTurnCheckpointStatus::AwaitingUser,
                        Some("cognition_turn_request_input"),
                        Some(OutstandingTurnBoundary::UserInput {
                            reason: "model explicitly requested principal input".into(),
                        }),
                        &round_tool_names,
                        &round_provider_call_ids,
                    );
                    return Ok(ToolLoopExecutionResponse {
                        text: maybe_text.clone().unwrap_or(message),
                        metadata: shared_inputs.context_clone(),
                        tool_name: last.tool_name,
                        tool_output: last.tool_output,
                        tool_invocations: invocations,
                        rounds_executed,
                        termination_reason: "cognition_turn_request_input".to_string(),
                    });
                }

                if !mixed_terminal_batch
                    && let Some(message) = checkpoint_turn_from_invocations(round_invocations)
                {
                    if let Some(gate) = completion_gate.as_ref() {
                        let tools = collect_tool_names(&invocations);
                        persist_gate_ledger(
                            gate,
                            &record_finalized(
                                gate.stream_turn_id,
                                "cognition_turn_checkpoint",
                                rounds_executed,
                                &tools,
                            ),
                        );
                    }
                    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                        tool_name: COGNITION_TURN.to_string(),
                        tool_input: serde_json::json!({ "action": "turn.checkpoint" }),
                        tool_output: Value::Null,
                    });
                    persist_checkpoint!(
                        SafeCheckpointBoundary::AwaitingUser,
                        ActiveTurnCheckpointStatus::AwaitingUser,
                        Some("cognition_turn_checkpoint"),
                        Some(OutstandingTurnBoundary::UserInput {
                            reason: "model requested a principal continuation boundary".into(),
                        }),
                        &round_tool_names,
                        &round_provider_call_ids,
                    );
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

                if mixed_terminal_batch {
                    push_turn_control_message(
                        &mut turn_ctx.tool_lane.messages,
                        "[MEDOUSA_TURN_CONTROL]\nTerminal control cannot share a response with ordinary actions. Ordinary actions ran and their receipts were preserved; the premature terminal was ignored. Continue ActiveWork, then emit exactly one typed terminal outcome in its own response.",
                    );
                }

                if let Some((work_id, ack)) = workshop_entered_from_invocations(round_invocations) {
                    let intent = invocations
                        .iter()
                        .find(|i| is_begin_work_tool_name(&i.tool_name, &i.tool_input))
                        .and_then(|i| i.tool_input.get("intent"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("general");
                    turn_ctx.scratchpad.set_delegate(&work_id, intent);
                    sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                    if let Some(gate) = completion_gate.as_ref() {
                        let parent_corr = gate.parent_turn_correlation_id.as_deref().unwrap_or("-");
                        let digest = turn_ctx.scratchpad.digest_hash();
                        persist_gate_ledger(
                            gate,
                            &TurnLedgerRecord {
                                timestamp: chrono::Utc::now(),
                                stream_turn_id: gate.stream_turn_id,
                                kind: TurnLedgerEventKind::WorkDelegated,
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
                        tool_name: COGNITION_TURN.to_string(),
                        tool_input: serde_json::json!({ "action": "turn.begin_work" }),
                        tool_output: Value::Null,
                    });
                    persist_checkpoint!(
                        SafeCheckpointBoundary::Terminal,
                        ActiveTurnCheckpointStatus::Completed,
                        Some("workshop_entered"),
                        None,
                        &round_tool_names,
                        &round_provider_call_ids,
                    );
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

                if let Some((work_id, ack)) = worker_spawn_from_invocations(round_invocations) {
                    let intent = invocations
                        .iter()
                        .find(|i| is_workshop_spawn_call(&i.tool_name, &i.tool_input))
                        .and_then(|i| i.tool_input.get("intent"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("general");
                    turn_ctx.scratchpad.set_delegate(&work_id, intent);
                    sync_scratch_snapshot(completion_gate.as_deref_mut(), &turn_ctx.scratchpad);
                    if let Some(gate) = completion_gate.as_ref() {
                        let parent_corr = gate.parent_turn_correlation_id.as_deref().unwrap_or("-");
                        let digest = turn_ctx.scratchpad.digest_hash();
                        persist_gate_ledger(
                            gate,
                            &TurnLedgerRecord {
                                timestamp: chrono::Utc::now(),
                                stream_turn_id: gate.stream_turn_id,
                                kind: TurnLedgerEventKind::WorkDelegated,
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
                        tool_name: COGNITION_WORKSHOP_MUTATE.to_string(),
                        tool_input: Value::Null,
                        tool_output: Value::Null,
                    });
                    persist_checkpoint!(
                        SafeCheckpointBoundary::Terminal,
                        ActiveTurnCheckpointStatus::Completed,
                        Some("worker_spawned"),
                        None,
                        &round_tool_names,
                        &round_provider_call_ids,
                    );
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
                let checkpoint_persisted = persist_checkpoint!(
                    SafeCheckpointBoundary::BudgetExhausted,
                    ActiveTurnCheckpointStatus::BudgetExhausted,
                    Some(TOOL_ROUND_BUDGET_EXHAUSTED_REASON),
                    Some(OutstandingTurnBoundary::UserInput {
                        reason: "Coder reached its model/tool round ceiling".into(),
                    }),
                    &[],
                    &[],
                );
                let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                    tool_name: shared_inputs.selected_tool_name().to_string(),
                    tool_input: (*shared_inputs.tool_input).clone(),
                    tool_output: Value::Null,
                });
                return Ok(ToolLoopExecutionResponse {
                    text: tool_round_budget_exhausted_message(
                        rounds_executed,
                        effective_max_tool_rounds,
                        &turn_ctx.scratchpad,
                        checkpoint_persisted,
                    ),
                    metadata: shared_inputs.context_clone(),
                    tool_name: last.tool_name,
                    tool_output: last.tool_output,
                    tool_invocations: invocations,
                    rounds_executed,
                    termination_reason: TOOL_ROUND_BUDGET_EXHAUSTED_REASON.to_string(),
                });
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
            await_turn_result(
                "fallback draft completion",
                self.prompt_pipeline.execute(first_request),
            )
            .await?
            .text
        };
        let tool_result = await_turn_boundary(self.tool_registry.invoke_tool(
            shared_inputs.selected_tool_name(),
            (*shared_inputs.tool_input).clone(),
        ))
        .await
        .map_err(|error| turn_boundary_failure("fallback tool invocation", error))?;
        let tool_output = tool_output_from_invoke(tool_result);

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

        let final_response = await_turn_result(
            "fallback synthesis completion",
            self.prompt_pipeline.execute(final_request),
        )
        .await?;

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

#[allow(clippy::too_many_arguments)]
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
        persist_gate_ledger(
            gate,
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
        // Chronological prose is already visible to the principal. Keep the
        // same committed segment in model context so the next ActiveWork round
        // continues from what it actually said.
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
                finish_stuck_turn(shared_inputs, invocations.to_vec(), rounds_executed, gate)
                    .await?,
            ));
        }
        let text_only_limit = completion_gate
            .as_ref()
            .map(|gate| gate.max_text_only_stuck_continues)
            .unwrap_or_else(|| resolve_max_text_only_stuck_continues(max_tool_rounds));
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
    persist_gate_ledger(
        gate,
        &record_stuck(
            gate.stream_turn_id,
            rounds_executed,
            &tools,
            gate.max_text_only_stuck_continues,
        ),
    );
    if let Some(presentation) = gate.runtime_ports.turn_presentation() {
        presentation
            .notice(format!(
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

async fn start_tool_run(
    gate: Option<&ToolLoopCompletionGate<'_>>,
    tool_name: &str,
    tool_input: &Value,
    tool_round: usize,
) -> Option<String> {
    let events = gate?.runtime_ports.tool_run_events()?;
    Some(
        events
            .started(ToolRunStart {
                tool_name: tool_name.to_string(),
                tool_input: tool_input.clone(),
                tool_round,
            })
            .await,
    )
}

async fn complete_model_response(gate: Option<&ToolLoopCompletionGate<'_>>, model_round: usize) {
    let Some(events) = gate.and_then(|gate| gate.runtime_ports.model_response_events()) else {
        return;
    };
    events
        .completed(crate::ports::ModelResponseCompleted { model_round })
        .await;
}

async fn finish_tool_run(
    gate: Option<&ToolLoopCompletionGate<'_>>,
    tool_run_id: Option<&str>,
    tool_round: usize,
    invocation: &ToolInvocation,
) {
    let Some(events) = gate.and_then(|gate| gate.runtime_ports.tool_run_events()) else {
        return;
    };
    let Some(tool_run_id) = tool_run_id else {
        return;
    };
    events
        .finished(ToolRunFinish {
            tool_run_id: tool_run_id.to_string(),
            tool_round,
            invocation: invocation.clone(),
        })
        .await;
}

fn persist_gate_ledger(gate: &ToolLoopCompletionGate<'_>, record: &TurnLedgerRecord) {
    if let Some(sink) = gate.runtime_ports.ledger_sink() {
        sink.persist(record);
    }
}

fn sync_scratch_snapshot(gate: Option<&mut ToolLoopCompletionGate<'_>>, scratch: &TurnScratchpad) {
    if let Some(gate) = gate
        && let Some(slot) = gate.scratch_out.as_mut()
    {
        **slot = Some(scratch.clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn persist_loop_checkpoint(
    gate: Option<&ToolLoopCompletionGate<'_>>,
    boundary: SafeCheckpointBoundary,
    status: ActiveTurnCheckpointStatus,
    termination_reason: Option<&str>,
    outstanding_boundary: Option<OutstandingTurnBoundary>,
    turn_ctx: &HostTurnContext,
    invocations: &[ToolInvocation],
    rounds_executed: usize,
    max_tool_rounds: usize,
    tool_batches_completed: usize,
    discipline: &TurnLoopDiscipline,
    awareness: &TurnLoopAwareness,
    tool_names: &[String],
    provider_call_ids: &[String],
) -> bool {
    let Some(gate) = gate else {
        return false;
    };
    let Some(sink) = gate.active_turn_checkpoint_sink.as_ref() else {
        return false;
    };
    let (text_only_continues_without_new_tools, invocations_at_last_text_continue) =
        discipline.checkpoint_state();
    let (user_responses_sent, last_response_preview) = awareness.checkpoint_state();
    let orchestration = gate.orchestration.as_deref().cloned();
    let retry_count = orchestration
        .as_ref()
        .map(|state| state.retries)
        .unwrap_or(0);
    let state = ToolLoopCheckpointState {
        boundary,
        status,
        counters: ActiveTurnCounters {
            model_rounds_executed: rounds_executed,
            max_tool_rounds,
            tool_batches_completed,
            text_only_continues_without_new_tools,
            invocations_at_last_text_continue,
            user_responses_sent,
            last_response_preview,
            retry_count,
            orchestration,
        },
        user_lane_prefix: turn_ctx.user_lane_prefix.clone(),
        tool_lane_messages: turn_ctx.tool_lane.messages.clone(),
        invocations: invocations
            .iter()
            .map(CheckpointToolInvocation::from_runtime)
            .collect(),
        scratch: turn_ctx.scratchpad.clone(),
        outstanding_boundary,
        tool_names: tool_names.to_vec(),
        provider_call_ids: provider_call_ids.to_vec(),
        termination_reason: termination_reason.map(str::to_string),
    };
    match sink.persist_boundary(state) {
        Ok(()) => true,
        Err(err) => {
            tracing::warn!(
                error = %err,
                ?boundary,
                "failed to persist safe Coder turn boundary"
            );
            false
        }
    }
}

fn tool_round_budget_exhausted_message(
    rounds_executed: usize,
    max_tool_rounds: usize,
    scratch: &TurnScratchpad,
    checkpoint_persisted: bool,
) -> String {
    let goal = scratch.goal.trim();
    let goal_line = if goal.is_empty() {
        String::new()
    } else {
        format!(
            " Current goal: {}",
            goal.chars().take(240).collect::<String>()
        )
    };
    let recovery = if checkpoint_persisted {
        "Completed tool results and durable turn state were checkpointed; send a follow-up to continue with a fresh turn budget."
    } else {
        "A durable turn checkpoint could not be confirmed; send a follow-up to continue, and re-verify current workspace state before repeating any side effect."
    };
    format!(
        "I reached the turn's tool-round limit ({rounds_executed}/{max_tool_rounds}) and stopped without replaying any uncertain action.{goal_line} {recovery}"
    )
}

fn recoverable_tool_error_value(message: &str) -> Value {
    serde_json::json!({
        "ok": false,
        "error": message,
        "recoverable": true,
        "hint": "Read the error, fix arguments or choose another allowed tool, retry once if policy allows; delegate via cognition_workshop_mutate action=workshop.spawn when the host profile blocks direct execution."
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

fn is_serde_json_completion_error(err: &StasisError) -> bool {
    err.to_string().contains("Serde JSON error")
}

/// Replay an assistant tool-call turn, including thinking-mode `reasoning_content`.
///
/// DeepSeek V4 (and Kimi) require that field on later requests once the original
/// turn used tools. Rebuilding from `ChatMessage::from(tool_calls)` alone drops
/// both the CoT and any assistant preamble text. Empty string is intentional when
/// no CoT was returned — the field must still be present on tool-call turns.
fn assistant_tool_round_message(
    content: MessageContent,
    reasoning_content: Option<String>,
) -> ChatMessage {
    let reasoning = reasoning_content
        .filter(|text| !text.is_empty())
        .or_else(|| {
            content
                .joined_reasoning_content()
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_default();
    let parts: Vec<ContentPart> = content
        .parts()
        .iter()
        .filter(|part| !part.is_reasoning_content())
        .cloned()
        .collect();
    ChatMessage::assistant(MessageContent::from_parts(parts))
        .with_reasoning_content(Some(reasoning))
}

/// Providers that require thinking-mode CoT on tool turns (DeepSeek, Kimi) 400 if
/// an assistant tool-call message omits `reasoning_content` entirely. Keep the
/// field present on every such message before the request leaves the tool loop.
fn ensure_assistant_tool_turn_reasoning(messages: &mut [ChatMessage]) {
    for message in messages.iter_mut() {
        if message.role != ChatRole::Assistant {
            continue;
        }
        let parts = message.content.parts();
        if !parts.iter().any(ContentPart::is_tool_call) {
            continue;
        }
        if parts.iter().any(ContentPart::is_reasoning_content) {
            continue;
        }
        message
            .content
            .push(ContentPart::ReasoningContent(String::new()));
    }
}

/// Outcome of a chat completion that may have hit malformed provider tool-call JSON.
enum ChatCompletionOutcome {
    Ok(Box<genai::chat::ChatResponse>),
    /// Provider returned unparseable tool-call arguments after one silent retry.
    /// Caller must inject guidance and continue the tool loop — never fail the turn.
    MalformedToolJson,
}

const MALFORMED_TOOL_JSON_GUIDANCE: &str = "\
Your previous tool call could not be parsed (malformed JSON in the tool arguments). \
Do NOT apologize to the principal and do NOT ask them to retry or simplify. \
Self-correct: re-emit the tool call with valid JSON. Prefer small atomic calls \
(e.g. cognition_ui_build: verb=begin, then set_prose/add_section/add_card one at a time; \
each response returns handles + next[]). Fix brackets/commas and continue.";

async fn inject_malformed_tool_json_guidance(
    messages: &mut Vec<ChatMessage>,
    gate: Option<&ToolLoopCompletionGate<'_>>,
) {
    if let Some(gate) = gate
        && let Some(presentation) = gate.runtime_ports.turn_presentation()
    {
        presentation
            .notice(
                "◈ model_tool_json_guidance malformed tool-call JSON — coaching model to self-correct"
                    .to_string(),
            )
            .await;
    }
    push_turn_control_message(messages, MALFORMED_TOOL_JSON_GUIDANCE);
}

/// Complete chat once. A malformed tool call returns control to the loop so the
/// model can self-correct with the failed round represented in turn state.
async fn complete_chat_once(
    pipeline: &PromptExecutionPipeline,
    request: ChatRequest,
    context: PromptExecutionContext,
) -> Result<ChatCompletionOutcome> {
    match pipeline.complete_chat(request, context).await {
        Ok(completion) => Ok(ChatCompletionOutcome::Ok(Box::new(completion.response))),
        Err(err) if is_serde_json_completion_error(&err) => {
            Ok(ChatCompletionOutcome::MalformedToolJson)
        }
        Err(err) => Err(err),
    }
}

async fn complete_chat_stream_once(
    pipeline: &PromptExecutionPipeline,
    request: ChatRequest,
    context: PromptExecutionContext,
    chunk_tx: Option<&mpsc::Sender<StreamDelta>>,
) -> Result<ChatCompletionOutcome> {
    match pipeline
        .complete_chat_stream(request, context, chunk_tx)
        .await
    {
        Ok(completion) => Ok(ChatCompletionOutcome::Ok(Box::new(completion.response))),
        Err(err) if is_serde_json_completion_error(&err) => {
            Ok(ChatCompletionOutcome::MalformedToolJson)
        }
        Err(err) => Err(err),
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

#[cfg(test)]
mod tests {
    use super::{
        MALFORMED_TOOL_JSON_GUIDANCE, assistant_tool_round_message,
        ensure_assistant_tool_turn_reasoning, is_serde_json_completion_error,
        recoverable_tool_error_value, tool_output_from_invoke, tool_round_budget_exhausted_message,
    };
    use crate::completion_fsm::TurnCompletionProfile;
    use crate::turn_control::finish_turn_from_invocations;
    use genai::chat::{ChatMessage, ContentPart, MessageContent, ToolCall};
    use serde_json::json;
    use stasis::domain::errors::StasisError;

    #[test]
    fn detects_serde_json_completion_errors() {
        let err = StasisError::PortFailure(
            "genai chat completion failed for model 'openai::gpt-4o': Serde JSON error: expected ',' or ']' at line 1 column 4816".to_string(),
        );
        assert!(is_serde_json_completion_error(&err));
        let other = StasisError::PortFailure("timeout".to_string());
        assert!(!is_serde_json_completion_error(&other));
    }

    #[test]
    fn malformed_tool_json_guidance_coaches_self_correct() {
        assert!(MALFORMED_TOOL_JSON_GUIDANCE.contains("Do NOT apologize"));
        assert!(MALFORMED_TOOL_JSON_GUIDANCE.contains("Self-correct"));
        assert!(MALFORMED_TOOL_JSON_GUIDANCE.contains("cognition_ui_build"));
        assert!(MALFORMED_TOOL_JSON_GUIDANCE.contains("do NOT ask them to retry"));
    }

    #[test]
    fn assistant_tool_round_replays_reasoning_content() {
        let message = assistant_tool_round_message(
            MessageContent::from_parts(vec![
                ContentPart::Text("checking status".into()),
                ContentPart::ToolCall(ToolCall {
                    call_id: "call-1".into(),
                    fn_name: "cognition_workshop_query".into(),
                    fn_arguments: json!({ "action": "workshop.status" }),
                    thought_signatures: None,
                }),
            ]),
            Some("I should check worker status first.".into()),
        );
        assert_eq!(message.content.first_text(), Some("checking status"));
        assert!(message.content.contains_tool_call());
        assert_eq!(
            message.content.joined_reasoning_content().as_deref(),
            Some("I should check worker status first.")
        );
        assert!(
            message
                .content
                .parts()
                .iter()
                .any(|part| matches!(part, ContentPart::ReasoningContent(_)))
        );
    }

    #[test]
    fn assistant_tool_round_keeps_empty_reasoning_field() {
        let message = assistant_tool_round_message(
            MessageContent::from_parts(vec![ContentPart::ToolCall(ToolCall {
                call_id: "call-empty".into(),
                fn_name: "cognition_workshop_query".into(),
                fn_arguments: json!({ "action": "workshop.status" }),
                thought_signatures: None,
            })]),
            None,
        );
        assert_eq!(
            message.content.joined_reasoning_content().as_deref(),
            Some("")
        );
    }

    #[test]
    fn ensure_tool_turn_reasoning_fills_missing_field() {
        let mut messages = vec![ChatMessage::from(vec![ToolCall {
            call_id: "call-missing".into(),
            fn_name: "cognition_workshop_query".into(),
            fn_arguments: json!({ "action": "workshop.status" }),
            thought_signatures: None,
        }])];
        assert!(messages[0].content.joined_reasoning_content().is_none());
        ensure_assistant_tool_turn_reasoning(&mut messages);
        assert_eq!(
            messages[0].content.joined_reasoning_content().as_deref(),
            Some("")
        );
    }

    #[test]
    fn tool_invoke_failure_becomes_recoverable_receipt() {
        let out = tool_output_from_invoke(Err(StasisError::PortFailure(
            "tool not allowed in this turn profile: cognition_mcp_invoke".to_string(),
        )));
        assert_eq!(out["ok"], false);
        assert_eq!(out["recoverable"], true);
        assert!(
            out["error"]
                .as_str()
                .unwrap()
                .contains("cognition_mcp_invoke")
        );
    }

    #[test]
    fn recoverable_tool_error_has_hint() {
        let out = recoverable_tool_error_value("boom");
        assert_eq!(out["error"], "boom");
        assert!(
            out["hint"]
                .as_str()
                .unwrap()
                .contains("cognition_workshop_mutate action=workshop.spawn")
        );
    }

    #[test]
    fn foreground_prose_after_tools_keeps_active_work_running() {
        use crate::completion_fsm::{
            AfterToolsRoundContext, ContinueReason, TurnRoundAction, decide_after_tools_text_round,
        };
        let preamble = "Yesss! Let's do this — I'll pull up the current context, check what's \
                          resonating in memory, and calibrate to a focused AVEC posture. Boom — \
                          focused preset pulled. Let me lock it in and then call cognition_turn_finish \
                          once the full calibration summary is ready for you to read.";
        let action = decide_after_tools_text_round(&AfterToolsRoundContext {
            draft_text: preamble.to_string(),
            rounds_executed: 3,
            max_tool_rounds: 10,
            completion_profile: TurnCompletionProfile::ForegroundPrincipal,
        });
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ActiveWork,
                ..
            }
        ));
    }

    #[test]
    fn terminal_fsm_text_is_never_semantically_rewritten() {
        use crate::turn_control::terminal_text_for_fsm_end;
        let text = terminal_text_for_fsm_end(
            "prose_requires_finish",
            "I'll summarize everything next.".to_string(),
        );
        assert_eq!(text, "I'll summarize everything next.");
        assert_eq!(
            terminal_text_for_fsm_end(
                "prose_requires_finish",
                "Here is the complete answer after tool work.".to_string(),
            ),
            "Here is the complete answer after tool work."
        );
        assert_eq!(
            terminal_text_for_fsm_end("clarifying_question", "Which repo?".to_string()),
            "Which repo?"
        );
    }

    #[test]
    fn checkpoint_turn_from_invocations_is_detected_for_loop_exit() {
        use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
        let invocations = vec![ToolInvocation {
            tool_name: crate::turn_control::COGNITION_TURN.to_string(),
            tool_input: serde_json::json!({"action": "turn.checkpoint", "message": "Here is progress so far."}),
            tool_output: serde_json::json!({"ok": true, "checkpoint_turn": true}),
        }];
        assert_eq!(
            crate::turn_control::checkpoint_turn_from_invocations(&invocations).as_deref(),
            Some("Here is progress so far.")
        );
    }

    #[test]
    fn finish_turn_from_invocations_is_detected_for_loop_exit() {
        use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;
        let invocations = vec![ToolInvocation {
            tool_name: crate::turn_control::COGNITION_TURN.to_string(),
            tool_input: serde_json::json!({"action": "turn.finish", "message": "Final answer ready."}),
            tool_output: serde_json::json!({"ok": true, "finish_turn": true}),
        }];
        assert_eq!(
            finish_turn_from_invocations(&invocations).as_deref(),
            Some("Final answer ready.")
        );
    }

    #[test]
    fn direct_prose_ends_even_at_the_round_limit() {
        use crate::completion_fsm::{
            NoToolDebtRoundContext, TurnRoundAction, decide_no_tool_debt_text_round,
        };
        let action = decide_no_tool_debt_text_round(&NoToolDebtRoundContext {
            draft_text: "Let me check.".to_string(),
            completion_profile: TurnCompletionProfile::ForegroundPrincipal,
        });
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "direct_prose"
            }
        ));
    }

    #[test]
    fn budget_exhaustion_status_is_truthful_and_actionable() {
        let scratch = medousa_engine::TurnScratchpad::from_user_prompt("Implement exact recovery");
        let message = tool_round_budget_exhausted_message(100, 100, &scratch, true);
        assert!(message.contains("100/100"));
        assert!(message.contains("without replaying any uncertain action"));
        assert!(message.contains("fresh turn budget"));
        assert!(message.contains("Implement exact recovery"));

        let unconfirmed = tool_round_budget_exhausted_message(100, 100, &scratch, false);
        assert!(unconfirmed.contains("could not be confirmed"));
        assert!(unconfirmed.contains("re-verify current workspace state"));
        assert!(!unconfirmed.contains("were checkpointed"));
    }
}
