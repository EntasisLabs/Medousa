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

use crate::agent_runtime::turn_completion::{
    ToolLoopCompletionGate, TurnCompletionDecision, build_turn_completion_docket,
    collect_tool_names, resolve_turn_completion,
};
use crate::agent_runtime::turn_ledger::{
    TurnLoopDiscipline, developer_message_for_gatekeeper_continue,
    developer_message_for_heuristic_interim_continue, ledger_tool_names, persist_ledger_record,
    push_turn_control_message, record_finalized, record_from_gatekeeper_continue,
    record_stuck, record_tool_round, stuck_turn_user_message,
};
use crate::execution_policy::{load_parallel_execution_settings, parallel_tool_batch_allowed};
use crate::turn_control_tools::is_prepare_final_tool_name;
pub(crate) use crate::turn_text_heuristics::{
    should_finalize_on_text_only_response, termination_reason_for_text_only_finalize,
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
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_internal(
            request,
            prior_messages,
            chunk_tx,
            max_tool_rounds,
            completion_gate,
        )
        .await
    }

    async fn execute_with_defaults(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
    ) -> Result<ToolLoopExecutionResponse> {
        self.execute_internal(request, prior_messages, chunk_tx, DEFAULT_MAX_TOOL_ROUNDS, None)
            .await
    }

    async fn execute_internal(
        &self,
        request: ToolLoopExecutionRequest,
        prior_messages: Vec<ChatMessage>,
        chunk_tx: Option<&mpsc::UnboundedSender<StreamDelta>>,
        max_tool_rounds: usize,
        mut completion_gate: Option<&mut ToolLoopCompletionGate<'_>>,
    ) -> Result<ToolLoopExecutionResponse> {
        let ToolLoopExecutionRequest {
            user_prompt,
            system_prompt,
            context,
            tool_name,
            tool_input,
            tool_call_mode,
        } = request;

        let max_tool_rounds = max_tool_rounds.max(1);
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

        let mut messages = Vec::with_capacity(2 + prior_messages.len());
        if let Some(system_prompt) = shared_inputs.system_prompt.as_ref() {
            messages.push(ChatMessage::system(system_prompt.to_string()));
        }
        messages.extend(prior_messages);
        messages.push(ChatMessage::user(shared_inputs.user_prompt.to_string()));

        let mut tools = self.tool_registry.list_tools().await?;
        if has_selected_tool {
            let selected_sanitized = sanitize_tool_name_for_model(shared_inputs.selected_tool_name());
            let selected_prefix = format!("{selected_sanitized}_");
            tools.retain(|tool| {
                tool.name == shared_inputs.selected_tool_name()
                    || tool.name == selected_sanitized
                    || tool.name.starts_with(&selected_prefix)
            });
        }

        let mut invocations = Vec::new();
        let mut should_use_legacy_fallback = false;
        let mut fallback_draft_text: Option<String> = None;
        let mut rounds_executed = 0usize;
        let mut pending_final_answer = false;
        let mut last_streamed_draft: Option<String> = None;
        let streaming_enabled = chunk_tx.is_some();
        let mut discipline = TurnLoopDiscipline::default();

        if !tools.is_empty() {
            for _ in 0..max_tool_rounds {
                rounds_executed += 1;
                let chat_request = ChatRequest::new(messages.clone()).with_tools(tools.clone());
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

                    if let Some(text) = maybe_text {
                        let heuristic_would_finalize = should_finalize_on_text_only_response(
                            has_selected_tool,
                            invocations.len(),
                            &text,
                            pending_final_answer,
                            rounds_executed,
                            max_tool_rounds,
                        );

                        if heuristic_would_finalize {
                            if let Some(gate) = completion_gate.as_mut() {
                                let docket = build_turn_completion_docket(
                                    shared_inputs.user_prompt.as_ref(),
                                    &text,
                                    &invocations,
                                    pending_final_answer,
                                    rounds_executed,
                                    max_tool_rounds,
                                    true,
                                    last_streamed_draft.as_deref(),
                                );
                                let sink = gate.sink.clone();
                                let orchestration = gate.orchestration.as_deref_mut();
                                let budget = gate.budget;
                                let verdict = resolve_turn_completion(
                                    &self.prompt_pipeline,
                                    &docket,
                                    sink.as_ref(),
                                    orchestration,
                                    budget,
                                )
                                .await;

                                if verdict.decision == TurnCompletionDecision::Continue {
                                    let tools = ledger_tool_names(&invocations);
                                    let record = record_from_gatekeeper_continue(
                                        gate.stream_turn_id,
                                        &verdict,
                                        rounds_executed,
                                        &tools,
                                    );
                                    persist_ledger_record(
                                        gate.session_id.as_deref(),
                                        &record,
                                    );
                                    push_turn_control_message(
                                        &mut messages,
                                        &developer_message_for_gatekeeper_continue(&verdict),
                                    );
                                    if discipline.on_text_only_continue(invocations.len()) {
                                        return finish_stuck_turn(
                                            &shared_inputs,
                                            invocations,
                                            rounds_executed,
                                            gate,
                                        )
                                        .await;
                                    }
                                    gate.reset_scratch(streaming_enabled).await;
                                    last_streamed_draft = Some(text);
                                    continue;
                                }

                                let tools = collect_tool_names(&invocations);
                                persist_ledger_record(
                                    gate.session_id.as_deref(),
                                    &record_finalized(
                                        gate.stream_turn_id,
                                        "gatekeeper_finalize",
                                        rounds_executed,
                                        &tools,
                                    ),
                                );

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
                                    termination_reason: format!(
                                        "gatekeeper_{}",
                                        verdict.source
                                    ),
                                });
                            }

                            let last = invocations.last().cloned().unwrap_or(ToolInvocation {
                                tool_name: shared_inputs.selected_tool_name().to_string(),
                                tool_input: (*shared_inputs.tool_input).clone(),
                                tool_output: Value::Null,
                            });

                            let termination_reason = termination_reason_for_text_only_finalize(
                                pending_final_answer,
                                rounds_executed,
                                max_tool_rounds,
                            )
                            .to_string();

                            if let Some(gate) = completion_gate.as_ref() {
                                let tools = collect_tool_names(&invocations);
                                persist_ledger_record(
                                    gate.session_id.as_deref(),
                                    &record_finalized(
                                        gate.stream_turn_id,
                                        &termination_reason,
                                        rounds_executed,
                                        &tools,
                                    ),
                                );
                            }

                            return Ok(ToolLoopExecutionResponse {
                                text,
                                metadata: shared_inputs.context_clone(),
                                tool_name: last.tool_name,
                                tool_output: last.tool_output,
                                tool_invocations: invocations,
                                rounds_executed,
                                termination_reason,
                            });
                        }

                        if let Some(gate) = completion_gate.as_ref() {
                            gate.reset_scratch(streaming_enabled).await;
                        }
                        push_turn_control_message(
                            &mut messages,
                            developer_message_for_heuristic_interim_continue(),
                        );
                        if discipline.on_text_only_continue(invocations.len()) {
                            if let Some(gate) = completion_gate.as_ref() {
                                return finish_stuck_turn(
                                    &shared_inputs,
                                    invocations,
                                    rounds_executed,
                                    gate,
                                )
                                .await;
                            }
                            return finish_stuck_turn_response(
                                &shared_inputs,
                                invocations,
                                rounds_executed,
                            );
                        }
                        last_streamed_draft = Some(text);
                        continue;
                    }

                    return Err(StasisError::PortFailure(
                        "chat response was empty after tool loop".to_string(),
                    ));
                }

                if pending_final_answer
                    && tool_calls
                        .iter()
                        .any(|call| !is_prepare_final_tool_name(&call.fn_name))
                {
                    pending_final_answer = false;
                }

                messages.push(ChatMessage::from(tool_calls.clone()));

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
                        let registry = self.tool_registry.clone();
                        join_set.spawn(async move {
                            let output = registry
                                .invoke_tool(&call.fn_name, call.fn_arguments.clone())
                                .await;
                            (call, output)
                        });
                    }

                    while let Some(joined) = join_set.join_next().await {
                        let (call, output) = joined.map_err(|error| {
                            StasisError::PortFailure(format!(
                                "parallel tool batch join failed: {error}"
                            ))
                        })?;
                        let tool_output = output?;
                        let tool_output_text = tool_output.to_string();
                        messages.push(ChatMessage::from(ToolResponse::new(
                            call.call_id,
                            tool_output_text,
                        )));
                        if is_prepare_final_tool_name(&call.fn_name) {
                            prepare_final_in_batch = true;
                        }
                        invocations.push(ToolInvocation {
                            tool_name: call.fn_name,
                            tool_input: call.fn_arguments,
                            tool_output,
                        });
                    }
                } else {
                    for call in tool_calls {
                        let tool_output = self
                            .tool_registry
                            .invoke_tool(&call.fn_name, call.fn_arguments.clone())
                            .await?;

                        if is_prepare_final_tool_name(&call.fn_name) {
                            prepare_final_in_batch = true;
                        }

                        let tool_output_text = tool_output.to_string();
                        messages.push(ChatMessage::from(ToolResponse::new(
                            call.call_id,
                            tool_output_text,
                        )));
                        invocations.push(ToolInvocation {
                            tool_name: call.fn_name,
                            tool_input: call.fn_arguments,
                            tool_output,
                        });
                    }
                }

                if prepare_final_in_batch {
                    pending_final_answer = true;
                }

                discipline.on_tool_round();
                if let Some(gate) = completion_gate.as_ref() {
                    persist_ledger_record(
                        gate.session_id.as_deref(),
                        &record_tool_round(
                            gate.stream_turn_id,
                            rounds_executed,
                            &round_tool_names,
                        ),
                    );
                }

                if let Some((work_id, ack)) =
                    crate::agent_runtime::turn_worker_tools::worker_spawn_from_invocations(
                        &invocations,
                    )
                {
                    if let Some(gate) = completion_gate.as_ref() {
                        persist_ledger_record(
                            gate.session_id.as_deref(),
                            &crate::agent_runtime::turn_ledger::TurnLedgerRecord {
                                timestamp: chrono::Utc::now(),
                                stream_turn_id: gate.stream_turn_id,
                                kind: crate::agent_runtime::turn_ledger::TurnLedgerEventKind::WorkDelegated,
                                detail: format!("host_turn_ended work_id={work_id}"),
                                tools_invoked: ledger_tool_names(&invocations),
                                missing_tools: Vec::new(),
                                rounds_executed,
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
                    "tool loop exceeded max rounds ({max_tool_rounds}) without final response"
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
        let tool_output = self
            .tool_registry
            .invoke_tool(
                shared_inputs.selected_tool_name(),
                (*shared_inputs.tool_input).clone(),
            )
            .await?;

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

async fn finish_stuck_turn(
    shared_inputs: &ToolLoopSharedInputs,
    invocations: Vec<ToolInvocation>,
    rounds_executed: usize,
    gate: &ToolLoopCompletionGate<'_>,
) -> Result<ToolLoopExecutionResponse> {
    let tools = ledger_tool_names(&invocations);
    persist_ledger_record(
        gate.session_id.as_deref(),
        &record_stuck(gate.stream_turn_id, rounds_executed, &tools),
    );
    if let Some(sink) = gate.sink.as_ref() {
        sink.notice(format!(
            "◈ turn loop stuck: {} text-only continues without new tools",
            crate::agent_runtime::turn_ledger::MAX_TEXT_ONLY_STUCK_CONTINUES
        ))
        .await;
    }
    finish_stuck_turn_response(shared_inputs, invocations, rounds_executed)
}

fn finish_stuck_turn_response(
    shared_inputs: &ToolLoopSharedInputs,
    invocations: Vec<ToolInvocation>,
    rounds_executed: usize,
) -> Result<ToolLoopExecutionResponse> {
    let last = invocations.last().cloned().unwrap_or(ToolInvocation {
        tool_name: shared_inputs.selected_tool_name().to_string(),
        tool_input: (*shared_inputs.tool_input).clone(),
        tool_output: Value::Null,
    });
    Ok(ToolLoopExecutionResponse {
        text: stuck_turn_user_message(crate::agent_runtime::turn_ledger::MAX_TEXT_ONLY_STUCK_CONTINUES),
        metadata: shared_inputs.context_clone(),
        tool_name: last.tool_name,
        tool_output: last.tool_output,
        tool_invocations: invocations,
        rounds_executed,
        termination_reason: "stuck_text_only_continue".to_string(),
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
    use super::{should_finalize_on_text_only_response, termination_reason_for_text_only_finalize};
    use crate::turn_text_heuristics::{
        looks_like_interim_status, looks_like_substantive_final_answer,
    };

    #[test]
    fn interim_status_before_first_tool_continues() {
        assert!(looks_like_interim_status("Let me check that for you."));
        assert!(!should_finalize_on_text_only_response(
            false,
            0,
            "Let me check that for you.",
            false,
            1,
            10
        ));
    }

    #[test]
    fn interim_status_between_tools_continues() {
        assert!(looks_like_interim_status("Stored."));
        assert!(!should_finalize_on_text_only_response(
            false, 3, "Stored.", false, 4, 10
        ));
    }

    #[test]
    fn substantive_answer_after_tools_finalizes() {
        let answer = "Your memory profile shows stability at 0.95 and three recent nodes about \
                      the ingester roadmap. I stored the update in Locus.";
        assert!(looks_like_substantive_final_answer(answer));
        assert!(should_finalize_on_text_only_response(
            false, 2, answer, false, 3, 10
        ));
    }

    #[test]
    fn celebratory_preamble_with_let_me_does_not_finalize_after_tools() {
        let preamble = "Yesss! Let's do this — I'll pull up the current context, check what's \
                          resonating in memory, and calibrate to a focused AVEC posture. Boom — \
                          focused preset pulled. Let me lock it in.";
        assert!(looks_like_interim_status(preamble));
        assert!(!looks_like_substantive_final_answer(preamble));
        assert!(!should_finalize_on_text_only_response(
            false, 2, preamble, false, 3, 10
        ));
    }

    #[test]
    fn prepare_final_flag_finalizes_on_next_text() {
        assert!(should_finalize_on_text_only_response(
            false,
            1,
            "Here is your answer.",
            true,
            2,
            10
        ));
        assert!(!should_finalize_on_text_only_response(
            false, 1, "Stored.", false, 2, 10
        ));
    }

    #[test]
    fn termination_reason_reflects_finalize_path() {
        assert_eq!(
            termination_reason_for_text_only_finalize(true, 2, 10),
            "prepare_final_then_text"
        );
        assert_eq!(
            termination_reason_for_text_only_finalize(false, 10, 10),
            "max_rounds_fuse"
        );
        assert_eq!(
            termination_reason_for_text_only_finalize(false, 3, 10),
            "heuristic_substantive"
        );
    }

    #[test]
    fn round_budget_is_safety_fuse_only() {
        assert!(should_finalize_on_text_only_response(
            false,
            2,
            "Let me check.",
            false,
            10,
            10
        ));
        assert!(!should_finalize_on_text_only_response(
            false,
            2,
            "Let me check.",
            false,
            3,
            10
        ));
    }

    #[test]
    fn before_tools_never_finalizes_on_text_even_on_last_round() {
        assert!(!should_finalize_on_text_only_response(
            false,
            0,
            "Let me check.",
            false,
            10,
            10
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
