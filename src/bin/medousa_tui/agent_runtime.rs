use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use stasis::application::orchestration::tool_loop_pipeline::ToolCallMode;
use tokio::sync::mpsc;

use medousa::{
    InteractiveTurnRequest, TuiRuntime,
    agent_runtime::{
        prompt_prep,
        stream_sink::AgentStreamSink,
        turn_orchestrator::{
            self, DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS,
            DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS,
            DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD, DEFAULT_COLD_WINDOW_TURNS,
            DEFAULT_HOT_WINDOW_TURNS, DEFAULT_RETRY_RUNTIME_MAX_RETRIES,
            DEFAULT_RETRY_RUNTIME_MAX_ROUNDS, LocalTurnExecutionParams, MAX_COLD_WINDOW_TURNS,
            MAX_HOT_WINDOW_TURNS, MIN_COLD_WINDOW_TURNS, MIN_HOT_WINDOW_TURNS,
            PrepareTurnPromptParams,
        },
        turn_services::{self, IntentContextLimits, PriorMessageLimits},
    },
    events::TuiEvent,
    payload_receipt::ArtifactReceiptMeta,
    turn_continuation::TurnContinuationScope,
};
use medousa_sdk::{HttpTransport, MedousaClient};
use medousa_types::{
    TurnCompletionOutcomeV3, TurnStreamEnvelopeV3, TurnStreamEventV3, WorkerAckKind,
};
use serde_json::Value;

use super::daemon_commands::daemon_start_interactive_turn;
use super::{ConversationTurn, TuiState};

const INTENT_CLASSIFIER_MAX_CONTEXT_TURNS: usize = 4;
const INTENT_CLASSIFIER_MAX_CONTEXT_CHARS: usize = 1400;
const INTENT_CLASSIFIER_CONTEXT_LINE_CHARS: usize = 260;

struct TuiStreamSink {
    tx: mpsc::Sender<TuiEvent>,
    local_turn_id: u64,
    chronology: tokio::sync::Mutex<TuiChronology>,
}

struct TuiChronology {
    seq: u64,
    model_round: usize,
    next_segment: usize,
    active_segment: Option<String>,
}

impl Default for TuiChronology {
    fn default() -> Self {
        Self {
            seq: 0,
            model_round: 1,
            next_segment: 0,
            active_segment: None,
        }
    }
}

impl TuiStreamSink {
    fn new(tx: mpsc::Sender<TuiEvent>, local_turn_id: u64) -> Self {
        Self {
            tx,
            local_turn_id,
            chronology: tokio::sync::Mutex::new(TuiChronology::default()),
        }
    }

    async fn publish_locked(
        &self,
        turn_id: u64,
        state: &mut TuiChronology,
        event: TurnStreamEventV3,
    ) {
        state.seq = state.seq.saturating_add(1);
        let envelope = TurnStreamEnvelopeV3::new(
            format!("tui-local-{turn_id}"),
            state.seq,
            chrono::Utc::now(),
            event,
        )
        .expect("valid local TUI V3 envelope");
        let _ = self
            .tx
            .send(TuiEvent::TurnStreamV3 { turn_id, envelope })
            .await;
    }

    async fn publish(&self, turn_id: u64, event: TurnStreamEventV3) {
        let mut state = self.chronology.lock().await;
        self.publish_locked(turn_id, &mut state, event).await;
    }

    async fn commit_active_segment(&self, turn_id: u64, advance_round: bool) {
        let mut state = self.chronology.lock().await;
        if let Some(segment_id) = state.active_segment.take() {
            self.publish_locked(
                turn_id,
                &mut state,
                TurnStreamEventV3::AssistantTextCommitted { segment_id },
            )
            .await;
        }
        if advance_round {
            state.model_round = state.model_round.saturating_add(1);
        }
    }
}

#[async_trait]
impl AgentStreamSink for TuiStreamSink {
    async fn content_chunk(&self, turn_id: u64, delta: String) {
        if delta.is_empty() {
            return;
        }
        let mut state = self.chronology.lock().await;
        if state.active_segment.is_none() {
            state.next_segment = state.next_segment.saturating_add(1);
            let segment_id = format!("tui-local-{turn_id}:text:{}", state.next_segment);
            state.active_segment = Some(segment_id.clone());
            let model_round = state.model_round;
            self.publish_locked(
                turn_id,
                &mut state,
                TurnStreamEventV3::AssistantTextStarted {
                    segment_id,
                    model_round,
                },
            )
            .await;
        }
        let segment_id = state
            .active_segment
            .clone()
            .expect("active local TUI segment");
        self.publish_locked(
            turn_id,
            &mut state,
            TurnStreamEventV3::ContentAppend {
                segment_id,
                text: delta,
            },
        )
        .await;
    }

    async fn reasoning_chunk(&self, turn_id: u64, delta: String) {
        self.publish(turn_id, TurnStreamEventV3::ReasoningAppend { text: delta })
            .await;
    }

    async fn agent_worker_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        self.publish(
            turn_id,
            TurnStreamEventV3::WorkerAck {
                ack_kind: WorkerAckKind::Worker,
                text,
                tool_names,
                work_id,
            },
        )
        .await;
    }

    async fn agent_workshop_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        self.publish(
            turn_id,
            TurnStreamEventV3::WorkerAck {
                ack_kind: WorkerAckKind::Workshop,
                text,
                tool_names,
                work_id,
            },
        )
        .await;
    }

    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.commit_active_segment(turn_id, false).await;
        self.publish(
            turn_id,
            TurnStreamEventV3::TurnCompleted {
                outcome: TurnCompletionOutcomeV3::Completed,
                aggregate_text: text,
                tool_names,
                operator_message: None,
                debug_message: None,
            },
        )
        .await;
    }

    async fn agent_needs_input(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.commit_active_segment(turn_id, false).await;
        self.publish(
            turn_id,
            TurnStreamEventV3::TurnCompleted {
                outcome: TurnCompletionOutcomeV3::NeedsInput,
                aggregate_text: text,
                tool_names,
                operator_message: None,
                debug_message: None,
            },
        )
        .await;
    }

    async fn agent_turn_progress(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        self.publish(
            turn_id,
            TurnStreamEventV3::Progress {
                message,
                tool_names,
            },
        )
        .await;
    }

    async fn agent_turn_checkpoint(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.commit_active_segment(turn_id, false).await;
        self.publish(
            turn_id,
            TurnStreamEventV3::TurnCompleted {
                outcome: TurnCompletionOutcomeV3::Checkpointed,
                aggregate_text: text,
                tool_names,
                operator_message: None,
                debug_message: None,
            },
        )
        .await;
    }

    async fn agent_error(&self, turn_id: u64, message: String) {
        let failure = medousa::turn_failure::TurnFailure::from_debug(&message);
        self.publish(
            turn_id,
            TurnStreamEventV3::Error {
                operator_message: failure.operator_message.clone(),
                debug_message: Some(message),
            },
        )
        .await;
        self.commit_active_segment(turn_id, false).await;
        self.publish(
            turn_id,
            TurnStreamEventV3::TurnCompleted {
                outcome: TurnCompletionOutcomeV3::Failed,
                aggregate_text: String::new(),
                tool_names: Vec::new(),
                operator_message: Some(failure.operator_message),
                debug_message: None,
            },
        )
        .await;
    }

    async fn model_receipt(&self, turn_id: u64, provider: String, model: String) {
        self.publish(turn_id, TurnStreamEventV3::ModelReceipt { provider, model })
            .await;
    }

    async fn notice(&self, message: String) {
        let _ = self.tx.send(TuiEvent::UiNotice(message)).await;
    }

    async fn tool_run_started(
        &self,
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        input_params: Vec<medousa_types::daemon_api::ToolInputParam>,
        tool_round: usize,
    ) {
        self.publish(
            self.local_turn_id,
            TurnStreamEventV3::ToolStarted {
                tool_run_id,
                tool_name,
                input_summary,
                input_params,
                tool_round,
            },
        )
        .await;
    }

    async fn tool_run_finished(
        &self,
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        output_summary: Option<String>,
        _tool_input: Value,
        _tool_output: Value,
        _input_receipt: Option<ArtifactReceiptMeta>,
        _output_receipt: Option<ArtifactReceiptMeta>,
        tool_round: usize,
    ) {
        self.publish(
            self.local_turn_id,
            TurnStreamEventV3::ToolFinished {
                tool_run_id,
                tool_name,
                status,
                input_summary,
                input_params: Vec::new(),
                output_summary,
                tool_round,
                artifact_refs: Vec::new(),
            },
        )
        .await;
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

    async fn model_response_completed_with_text(
        &self,
        turn_id: u64,
        _model_round: usize,
        response_text: Option<String>,
    ) {
        let needs_fallback = self.chronology.lock().await.active_segment.is_none();
        if needs_fallback && let Some(text) = response_text.filter(|text| !text.trim().is_empty()) {
            self.content_chunk(turn_id, text).await;
        }
        self.commit_active_segment(turn_id, true).await;
    }

    async fn reset_streamed_markdown(&self) {
        self.commit_active_segment(self.local_turn_id, false).await;
    }

    async fn turn_budget_approval_required(
        &self,
        turn_id: u64,
        request_id: String,
        rounds_executed: usize,
        max_tool_rounds: usize,
        requested_rounds: usize,
        reason: String,
        progress_summary: Option<String>,
    ) {
        self.publish(
            turn_id,
            TurnStreamEventV3::BudgetApprovalRequired {
                request_id,
                rounds_executed,
                max_tool_rounds,
                requested_rounds,
                reason,
                progress_summary,
            },
        )
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
    if state.is_processing {
        super::push_obs(
            state,
            "⚠ this pane is already running a turn (Ctrl+G to stop)".to_string(),
        );
        return;
    }
    if super::workspace_runtime::live_stream_count(state)
        >= super::workspace_runtime::MAX_LIVE_STREAMS
    {
        super::push_obs(
            state,
            "⚠ live stream cap reached (max 4) — wait or stop a turn".to_string(),
        );
        return;
    }
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
    state.turn_parts.reset();
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    state.in_thinking_tag = false;
    state.stream_tag_tail.clear();
    state.received_native_reasoning = false;
    super::workspace_runtime::register_stream_turn(state, turn_id);

    if persist_user_turn {
        let user_turn = medousa::turn_parts::user_conversation_turn(prompt.clone());
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

    let tui_surface = medousa::TurnSurfaceContext::tui();
    let identity_user_id =
        medousa::identity_memory::resolve_tool_identity_user_id(&state.session_id, false);
    let agent_mode =
        medousa::agent_runtime::resolve_agent_mode(medousa::daemon_api::AgentModeId::General)
            .expect("General agent mode is always available");
    let prepared = turn_orchestrator::prepare_turn_prompt(PrepareTurnPromptParams {
        agent_mode,
        mode_context_appendix: None,
        session_id: &state.session_id,
        prompt: &prompt,
        selected_context_pack_query: state.selected_context_pack_query.as_deref(),
        settings: &state.settings,
        verifier_route: verifier_route.as_ref(),
        final_route: final_route.as_ref(),
        response_depth_mode: &state.response_depth_mode,
        surface: Some(&tui_surface),
        tui_rt,
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
        voice_preset_id: None,
        voice_appendix: None,
        identity_user_id: &identity_user_id,
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
                prepared
                    .recall_probe
                    .retrieval_path
                    .as_deref()
                    .unwrap_or("n/a"),
                prepared.recall_probe.fallback_triggered,
                prepared
                    .recall_probe
                    .fallback_reason
                    .as_deref()
                    .unwrap_or("none"),
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
    super::push_obs(
        state,
        format!("◈ {}", prepared.compiler_output.compiler_summary),
    );

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
    let configured_tool_call_mode =
        turn_services::parse_tool_call_mode(&state.settings.tool_call_mode);
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
        tui_rt.tool_catalog.as_ref(),
        &state.session_id,
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
    let current_turn_user_message =
        medousa::media_vision::TurnMediaVisionPlan::empty().build_user_message(&prompt_for_request);
    let retry_max_retries = medousa::tui::settings::parse_usize_with_bounds(
        &state.settings.retry_runtime_max_retries,
        DEFAULT_RETRY_RUNTIME_MAX_RETRIES,
        medousa::agent_runtime::RETRY_LIMIT_MIN,
        medousa::agent_runtime::RETRY_LIMIT_MAX,
    );
    let retry_max_rounds = medousa::tui::settings::parse_usize_with_bounds(
        &state.settings.retry_runtime_max_rounds,
        DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
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
    let handoff_vibe_signature = prepared.handoff_vibe_signature.clone();
    let handoff_model_avec = prepared.handoff_model_avec;
    let host_continuity_bundle = Some(
        medousa::agent_runtime::worker_continuity::build_host_continuity_bundle(
            &prepared,
            &state.conversation,
            None,
        ),
    );
    let session_scratch_seed =
        medousa::turn_slice::session_scratch_seed_from_history(&state.conversation, &prompt);
    let sink: Arc<dyn AgentStreamSink> = Arc::new(TuiStreamSink::new(tx.clone(), turn_id));
    let turn_scope = medousa::agent_runtime::execution_context::TurnScopeAccess::default();
    let execution_registry = tui_rt.execution_registry.clone();
    let worker_scheduler = tui_rt.worker_scheduler.clone();
    let tool_registry = tui_rt.tool_registry.clone();
    let client_registry = tui_rt.client_registry.clone();
    let identity_memory_store = Some(tui_rt.identity_memory_store.clone()
        as std::sync::Arc<
            dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore,
        >);
    let session_id = state.session_id.clone();
    let backend = state.settings.backend.clone();
    let provider = state.settings.provider.clone();
    let model = state.settings.model.clone();
    let base_url =
        (!state.settings.base_url.trim().is_empty()).then(|| state.settings.base_url.clone());
    let inference_targets = vec![medousa::inference_profiles::InferenceTarget {
        provider: provider.clone(),
        model: model.clone(),
        base_url: base_url.clone(),
    }];
    let response_depth_mode = state.response_depth_mode.clone();
    let reasoning_effort = state.reasoning_effort.clone();
    let handle = tokio::spawn(async move {
        let scope = TurnContinuationScope {
            turn_correlation_id: format!("tui-turn-{turn_id}"),
            session_id: session_id.clone(),
            identity_user_id: Some(medousa::user_profiles::resolve_workshop_identity_user_id()),
            original_prompt: original_prompt_for_continuation.clone(),
            delivery_target: None,
            provider: provider.clone(),
            model: model.clone(),
            response_depth_mode: continuation_response_depth_mode.clone(),
            supports_ui_artifacts: false,
            supports_liquid_markdown: false,
            supports_browser_host: false,
            channel_surface: Some("tui".to_string()),
        };
        let execution =
            match medousa::agent_runtime::execution_context::TurnExecutionContext::from_scope(
                format!("tui-turn-{turn_id}"),
                medousa::request_principal::RequestPrincipal::local_app(
                    std::sync::Arc::from("medousa-tui"),
                    medousa::request_principal::TransportClass::Loopback,
                ),
                tokio_util::sync::CancellationToken::new(),
                std::time::Instant::now() + std::time::Duration::from_secs(2 * 60 * 60),
                scope,
            ) {
                Ok(execution) => execution,
                Err(error) => {
                    sink.agent_error(turn_id, format!("turn admission failed: {error}"))
                        .await;
                    return;
                }
            };
        let execution_lease = match execution_registry.admit(execution) {
            Ok(lease) => lease,
            Err(error) => {
                sink.agent_error(turn_id, format!("turn admission failed: {error}"))
                    .await;
                return;
            }
        };
        let execution_context = execution_lease.context().clone();

        medousa::agent_runtime::execution_context::with_turn_execution_context(
            execution_context,
            turn_orchestrator::execute_local_turn(
                sink.clone(),
                LocalTurnExecutionParams {
                    agent_mode,
                    turn_id,
                    session_id,
                    backend,
                    provider,
                    model,
                    base_url,
                    response_depth_mode,
                    reasoning_effort,
                    worker_scheduler,
                    tool_registry,
                    client_registry,
                    identity_memory_store,
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
                    handoff_vibe_signature,
                    handoff_model_avec,
                    host_continuity_bundle,
                    session_scratch_seed,
                    current_turn_user_message,
                    inference_profile_kind: medousa::inference_profiles::InferenceProfileKind::Main,
                    inference_targets,
                    supports_ui_artifacts: false,
                    supports_liquid_markdown: false,
                    supports_browser_host: false,
                    round_context_provider: None,
                    evidence_undertaking_id: None,
                    compact_evidence_receipt_sink: None,
                    active_turn_checkpoint_sink: None,
                    active_turn_resume: None,
                },
            ),
        )
        .await;
        drop(execution_lease);
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
        agent_mode: None,
        code_context: None,
        code_project_setup_authorized: false,
        persist_user_turn,
        response_depth_mode: state.response_depth_mode.clone(),
        reasoning_effort: state.reasoning_effort.clone(),
        provider: state.settings.provider.clone(),
        model: state.settings.model.clone(),
        stage_routing: state.stage_routing.clone(),
        surface: Some(medousa::TurnSurfaceContext::tui()),
        host_context: None,
        // The daemon reads the persisted General-mode setting. Leaving this
        // unset also lets Coder select its independent 100-round default.
        max_tool_rounds: None,
        retry_runtime_max_rounds: Some(medousa::tui::settings::parse_usize_with_bounds(
            &state.settings.retry_runtime_max_rounds,
            medousa::agent_runtime::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
            medousa::agent_runtime::ROUND_LIMIT_MIN,
            medousa::agent_runtime::ROUND_LIMIT_MAX,
        )),
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
        scheduled_tool_allowlist: None,
        voice_preset_id: None,
        voice_appendix: None,
        media_refs: Vec::new(),
        identity_user_id: None,
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
    state.turn_parts.reset();
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    state.in_thinking_tag = false;
    state.stream_tag_tail.clear();
    state.received_native_reasoning = false;
    state.pending_response_verified = None;
    super::workspace_runtime::register_stream_turn(state, turn_id);

    if persist_user_turn {
        let user_turn = medousa::turn_parts::user_conversation_turn(prompt.to_string());
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
    let client = medousa::local_daemon_auth::async_client(
        stream_url,
        medousa_local_credential::TUI_LOCAL_NAME,
    )
    .map_err(|err| err.to_string())?;
    let sdk =
        MedousaClient::with_transport(Arc::new(HttpTransport::with_client(client)), stream_url);
    let interactive = sdk.interactive();
    let mut events = interactive.stream_reconnecting_v3(stream_url);
    let mut saw_terminal = false;

    while let Some(payload) = events.next().await {
        let payload = payload.map_err(|err| err.to_string())?;
        if dispatch_daemon_stream_event(payload, turn_id, event_tx).await? {
            saw_terminal = true;
        }
    }

    if !saw_terminal {
        return Err("stream closed without terminal event".to_string());
    }

    Ok(())
}

async fn dispatch_daemon_stream_event(
    payload: TurnStreamEnvelopeV3,
    turn_id: u64,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> std::result::Result<bool, String> {
    let terminal = payload.event.is_terminal();
    event_tx
        .send(TuiEvent::TurnStreamV3 {
            turn_id,
            envelope: payload,
        })
        .await
        .map_err(|err| err.to_string())?;

    Ok(terminal)
}

fn build_prior_messages(
    tool_catalog: &medousa::typed_tools::ToolCatalog,
    session_id: &str,
    turns: &[ConversationTurn],
    current_prompt: &str,
    current_user_persisted: bool,
    hot_window_turns: usize,
    cold_window_turns: usize,
) -> turn_services::PriorMessageBuild {
    turn_services::build_prior_messages(
        tool_catalog,
        session_id,
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
    let turn_id = state.open_stream_turn_id;
    if let Some(task) = state.active_request_task.take() {
        task.abort();
        state.is_processing = false;
        state.open_stream_turn_id = None;
        state.active_agent_stream_turn = None;
        state.pending_response_verified = None;
        state.pending_agent_chunk_delta.clear();
        state.pending_agent_chunk_count = 0;
        if let Some(tid) = turn_id {
            super::workspace_runtime::clear_stream_turn(state, tid);
        }
        state.session_tasks.remove(&state.session_id);
        super::flush_thinking_buffer(state);
        super::push_obs(state, "■ generation stopped".to_string());
    }
}

#[cfg(test)]
mod stream_v3_tests {
    use super::*;

    #[tokio::test]
    async fn typed_completion_dispatches_one_native_terminal_fact() {
        let envelope = TurnStreamEnvelopeV3::new(
            "daemon-turn-1",
            1,
            chrono::Utc::now(),
            TurnStreamEventV3::TurnCompleted {
                outcome: medousa_types::TurnCompletionOutcomeV3::Completed,
                aggregate_text: "done".to_string(),
                tool_names: vec!["search".to_string()],
                operator_message: None,
                debug_message: None,
            },
        )
        .expect("v3 envelope");
        let (event_tx, mut event_rx) = mpsc::channel(1);

        let terminal = dispatch_daemon_stream_event(envelope, 7, &event_tx)
            .await
            .expect("dispatch");

        assert!(terminal);
        assert!(matches!(
            event_rx.recv().await,
            Some(TuiEvent::TurnStreamV3 {
                turn_id: 7,
                envelope: TurnStreamEnvelopeV3 {
                    event: TurnStreamEventV3::TurnCompleted { aggregate_text, .. },
                    ..
                },
            }) if aggregate_text == "done"
        ));
    }

    #[tokio::test]
    async fn local_sink_emits_response_tools_response_as_native_v3_facts() {
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let sink = TuiStreamSink::new(event_tx, 7);

        sink.content_chunk(7, "Let me check.".into()).await;
        sink.model_response_completed_with_text(7, 1, None).await;
        sink.tool_run_started("run-1".into(), "search".into(), "query".into(), vec![], 1)
            .await;
        sink.tool_run_finished(
            "run-1".into(),
            "search".into(),
            "failed".into(),
            "query".into(),
            Some("offline".into()),
            Value::Null,
            Value::Null,
            None,
            None,
            1,
        )
        .await;
        sink.content_chunk(7, "I recovered.".into()).await;
        sink.agent_response(
            7,
            "Let me check.\n\nI recovered.".into(),
            vec!["search".into()],
        )
        .await;

        let mut envelopes = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            let TuiEvent::TurnStreamV3 { turn_id, envelope } = event else {
                panic!("expected native V3 event");
            };
            assert_eq!(turn_id, 7);
            envelopes.push(envelope);
        }

        assert_eq!(envelopes.len(), 9);
        assert_eq!(
            envelopes.iter().map(|item| item.seq).collect::<Vec<_>>(),
            (1..=9).collect::<Vec<_>>()
        );
        assert!(matches!(
            &envelopes[0].event,
            TurnStreamEventV3::AssistantTextStarted { model_round: 1, .. }
        ));
        assert!(matches!(
            &envelopes[2].event,
            TurnStreamEventV3::AssistantTextCommitted { .. }
        ));
        assert!(matches!(
            &envelopes[3].event,
            TurnStreamEventV3::ToolStarted { .. }
        ));
        assert!(matches!(
            &envelopes[4].event,
            TurnStreamEventV3::ToolFinished { status, .. } if status == "failed"
        ));
        assert!(matches!(
            &envelopes[5].event,
            TurnStreamEventV3::AssistantTextStarted { model_round: 2, .. }
        ));
        assert!(matches!(
            &envelopes[8].event,
            TurnStreamEventV3::TurnCompleted { .. }
        ));
    }
}
