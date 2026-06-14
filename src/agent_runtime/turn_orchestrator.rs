use std::sync::Arc;

use genai::chat::{ChatMessage, ChatRequest};
use serde_json::Value;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use crate::medousa_tool_loop::MedousaToolLoopPipeline;
use stasis::application::orchestration::tool_loop_pipeline::{ToolCallMode, ToolInvocation, ToolLoopExecutionRequest};
use stasis::ports::outbound::ai_chat_client::StreamDelta;

use crate::channel_delivery;
use crate::engine_context::{EngineExecutionLane, RecallReadiness};
use stasis::ports::outbound::memory::memory_models::MemoryAvecState;
use crate::session::ConversationTurn;
use crate::daemon_api::TurnSurfaceContext;
use crate::stage_routing::StageRoute;
use crate::tools::TuiRuntime;
use crate::tui::settings::{
    RuntimeSettings, OPERATOR_RETRY_LIMIT_MAX, OPERATOR_RETRY_LIMIT_MIN, OPERATOR_ROUND_LIMIT_MAX,
    OPERATOR_ROUND_LIMIT_MIN, parse_usize_with_bounds,
};

use super::continuation::{
    build_continuation_prior_messages, build_continuation_prompt, collect_tool_names,
    should_run_continuation,
};
use super::prompt_prep::{
    CheapRecallProbe, IdentityContextProbe, append_identity_context_hint,
    append_manuscript_hint, append_memory_recall_hint, append_suggested_capabilities_hint,
    cheap_memory_recall_probe,
    compile_interactive_context_prompt,
    channel_policy_probe, derive_recall_readiness, identity_context_probe,
    resolve_prompt_with_context_pack,
    truncate_text_for_budget, verifier_policy_from_settings_and_route, MAX_REQUEST_PROMPT_CHARS,
};
use super::stream_sink::SharedAgentStreamSink;
use super::system_prompt::DEFAULT_SYSTEM_PROMPT;
use super::turn_budget::{
    emit_orchestration_summary, try_consume_classifier_budget, try_consume_continuation_budget,
    try_consume_prompt_only_budget, try_consume_retry_budget, try_consume_tool_loop_budget,
    turn_budget_for_lane, TurnBudget, TurnOrchestrationState,
};
use super::turn_completion::ToolLoopCompletionGate;
use super::turn_ledger::append_tool_loop_policy;
use super::turn_context::TurnScratchpad;
use super::turn_loop_settings::TurnLoopSettings;
use super::turn_worker::{
    ActiveWorkerBusSession, WorkerRuntimeContext, apply_host_profile_to_activation,
    host_route_notice, pipeline_for_turn_profile, resolve_host_turn_profile,
    system_prompt_for_host_profile,
};
use crate::turn_continuation::StoredDeliveryTarget;
use crate::turn_slice::session_scratch_seed_from_history;
use super::turn_services::{
    self, IntentContextLimits, PriorMessageBuild, PriorMessageLimits, SelectedTurnPipeline,
    TurnActivationDecision,
};

pub const MAX_PRIOR_TOTAL_CHARS: usize = 24_000;
pub const MAX_SINGLE_PRIOR_MESSAGE_CHARS: usize = 4_000;
pub const DEFAULT_HOT_WINDOW_TURNS: usize = 8;
pub const MIN_HOT_WINDOW_TURNS: usize = 2;
pub const MAX_HOT_WINDOW_TURNS: usize = 32;
pub const DEFAULT_COLD_WINDOW_TURNS: usize = 24;
pub const MIN_COLD_WINDOW_TURNS: usize = 4;
pub const MAX_COLD_WINDOW_TURNS: usize = 128;
pub const HOT_WINDOW_CHAR_BUDGET: usize = 14_000;
pub const COLD_WINDOW_CHAR_BUDGET: usize = 8_000;
pub const COLD_SUMMARY_LINE_CHARS: usize = 240;
pub const DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS: usize = 320;
pub const DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD: usize = 28;
pub const DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS: usize = 420;
pub const DEFAULT_RETRY_RUNTIME_MAX_RETRIES: usize = 1;
pub const DEFAULT_RETRY_RUNTIME_MAX_ROUNDS: usize = 10;
const INTENT_CLASSIFIER_MAX_PROMPT_CHARS: usize = 900;
const INTENT_CLASSIFIER_MAX_CONTEXT_TURNS: usize = 4;
const INTENT_CLASSIFIER_MAX_CONTEXT_CHARS: usize = 1400;
const INTENT_CLASSIFIER_CONTEXT_LINE_CHARS: usize = 260;
const INTENT_CLASSIFIER_CONFIDENCE_LOW: f32 = 0.45;
const INTENT_CLASSIFIER_CONFIDENCE_CONVERSATIONAL: f32 = 0.55;
const INTENT_CLASSIFIER_CONFIDENCE_TOOL_REQUIRED: f32 = 0.60;

#[derive(Debug, Clone)]
pub struct IntentClassification {
    pub intent: String,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct PreparedTurnPrompt {
    pub resolved_prompt: String,
    pub pack_note: Option<String>,
    pub verification_state: Option<bool>,
    pub recall_probe: CheapRecallProbe,
    pub identity_probe: IdentityContextProbe,
    pub recall_readiness: RecallReadiness,
    pub compiler_output: crate::engine_context::ContextCompilerOutput,
    pub handoff_vibe_signature: String,
    pub handoff_model_avec: MemoryAvecState,
    pub ambient_appendix: String,
}

pub struct PrepareTurnPromptParams<'a> {
    pub session_id: &'a str,
    pub prompt: &'a str,
    pub selected_context_pack_query: Option<&'a str>,
    pub settings: &'a RuntimeSettings,
    pub verifier_route: Option<&'a StageRoute>,
    pub final_route: Option<&'a StageRoute>,
    pub response_depth_mode: &'a str,
    pub surface: Option<&'a TurnSurfaceContext>,
    pub tui_rt: &'a TuiRuntime,
    pub manuscript_id: Option<&'a str>,
    pub additional_manuscript_ids: Option<&'a [String]>,
    pub suggested_capability_ids: Option<&'a [String]>,
}

pub async fn prepare_turn_prompt(params: PrepareTurnPromptParams<'_>) -> PreparedTurnPrompt {
    let verifier_policy =
        verifier_policy_from_settings_and_route(params.settings, params.verifier_route);
    let (mut resolved_prompt, pack_note, verification_state) = resolve_prompt_with_context_pack(
        params.session_id,
        params.prompt,
        params.selected_context_pack_query,
        &verifier_policy,
    );

    let recall_probe =
        cheap_memory_recall_probe(params.tui_rt, params.session_id, params.prompt).await;
    let manuscript_ctx = params
        .manuscript_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|id| crate::identity_manuscript::build_manuscript_context(id).ok());
    let identity_probe = identity_context_probe(
        params.tui_rt,
        params.final_route.map(|route| route.policy_profile.as_str()),
        Some(params.prompt),
        manuscript_ctx.as_ref(),
    )
    .await;
    let channel_policy = channel_policy_probe(
        params.tui_rt,
        params.final_route.map(|route| route.policy_profile.as_str()),
    )
    .await;

    resolved_prompt = append_memory_recall_hint(&resolved_prompt, &recall_probe);
    resolved_prompt = append_manuscript_hint(&resolved_prompt, manuscript_ctx.as_ref());
    if let Some(ids) = params.additional_manuscript_ids {
        for id in ids {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                continue;
            }
            if Some(trimmed) == params.manuscript_id {
                continue;
            }
            if let Ok(ctx) = crate::identity_manuscript::build_manuscript_context(trimmed) {
                resolved_prompt = append_manuscript_hint(&resolved_prompt, Some(&ctx));
            }
        }
    }
    if let Some(ids) = params.suggested_capability_ids {
        resolved_prompt = append_suggested_capabilities_hint(&resolved_prompt, ids);
    }
    resolved_prompt = append_identity_context_hint(&resolved_prompt, &identity_probe);
    resolved_prompt =
        crate::agent_runtime::turn_worker::append_active_workers_hint(
            &resolved_prompt,
            params.session_id,
        );
    let recall_readiness = derive_recall_readiness(
        verification_state,
        recall_probe.attempted,
        recall_probe.retrieved,
        identity_probe.summary.is_some(),
    );
    let compiler_output = compile_interactive_context_prompt(
        &resolved_prompt,
        params.response_depth_mode,
        params.final_route,
        recall_readiness,
    );
    resolved_prompt = compiler_output.compiled_prompt.clone();
    let ambient_block = super::ambient_context::build_ambient_context(
        super::ambient_context::AmbientContextInput {
            session_id: params.session_id,
            surface: params.surface,
            channel_policy: Some(&channel_policy),
        },
    );
    let ambient_appendix = ambient_block.appendix.clone();
    resolved_prompt = format!("{resolved_prompt}\n\n{}", ambient_block.appendix);

    let handoff_model_avec = super::vibe_signature::default_handoff_model_avec();
    let handoff_vibe_signature = super::vibe_signature::derive_vibe_signature(
        params.session_id,
        params.surface,
        Some(&channel_policy),
        &handoff_model_avec,
    );

    PreparedTurnPrompt {
        resolved_prompt,
        pack_note,
        verification_state,
        recall_probe,
        identity_probe,
        recall_readiness,
        compiler_output,
        handoff_vibe_signature,
        handoff_model_avec,
        ambient_appendix,
    }
}

pub struct LocalTurnExecutionParams {
    pub turn_id: u64,
    pub session_id: String,
    pub backend: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub response_depth_mode: String,
    pub worker_scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
    pub tool_registry: Arc<dyn stasis::application::orchestration::tool_registry::ToolRegistry>,
    pub identity_memory_store:
        Option<Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>>,
    pub turn_scope: Arc<tokio::sync::RwLock<Option<crate::turn_continuation::TurnContinuationScope>>>,
    pub activation: TurnActivationDecision,
    pub pipeline: MedousaToolLoopPipeline,
    pub no_tools_pipeline: PromptExecutionPipeline,
    pub prior_messages: Vec<ChatMessage>,
    pub prompt_for_request: String,
    pub original_prompt: String,
    pub intent_classifier_recent_context: String,
    pub retry_max_retries: usize,
    pub retry_max_rounds: usize,
    pub continuation_response_depth_mode: String,
    pub continuation_stage_route: Option<StageRoute>,
    pub continuation_recall_readiness: RecallReadiness,
    pub prompt_preview: String,
    pub turn_loop_settings: TurnLoopSettings,
    pub handoff_vibe_signature: String,
    pub handoff_model_avec: MemoryAvecState,
    pub host_continuity_bundle: Option<super::worker_continuity::HostContinuityBundle>,
    pub session_scratch_seed: TurnScratchpad,
    pub current_turn_user_message: ChatMessage,
}

pub struct AssembleLocalTurnParams<'a> {
    pub session_id: &'a str,
    pub settings: &'a RuntimeSettings,
    pub conversation: &'a [ConversationTurn],
    pub prompt: &'a str,
    pub persist_user_turn: bool,
    pub prepared: &'a PreparedTurnPrompt,
    pub resolved_prompt: String,
    pub tui_rt: &'a TuiRuntime,
    pub final_route: Option<&'a StageRoute>,
    pub response_depth_mode: &'a str,
    pub turn_id: u64,
    pub scheduled_tool_allowlist: Option<std::collections::HashSet<String>>,
    pub media_refs: Vec<crate::daemon_api::MediaRef>,
    pub vision_plan: crate::media_vision::TurnMediaVisionPlan,
}

pub struct AssembledLocalTurn {
    pub execution: LocalTurnExecutionParams,
    pub pipeline_selection: SelectedTurnPipeline,
    pub activation: TurnActivationDecision,
    pub prior_build: PriorMessageBuild,
}

pub fn assemble_local_turn(params: AssembleLocalTurnParams<'_>) -> AssembledLocalTurn {
    let configured_tool_call_mode = turn_services::parse_tool_call_mode(&params.settings.tool_call_mode);
    let turn_loop_settings = TurnLoopSettings::from_runtime_settings(params.settings);
    let activation = turn_services::decide_turn_activation(
        params.prompt,
        configured_tool_call_mode,
        turn_loop_settings.configured_max_tool_rounds,
        turn_loop_settings.activation_tool_intent_max_rounds,
        turn_loop_settings.activation_short_turn_max_tool_rounds,
        params.conversation.len(),
        parse_usize_with_bounds(
            &params.settings.activation_direct_answer_max_prompt_chars,
            DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS,
            64,
            4000,
        ),
        parse_usize_with_bounds(
            &params.settings.activation_long_session_turn_threshold,
            DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD,
            8,
            500,
        ),
        parse_usize_with_bounds(
            &params.settings.activation_long_session_max_prompt_chars,
            DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS,
            64,
            4000,
        ),
    );
    let activation = turn_services::apply_context_compiler_activation_gate(
        activation,
        params.prepared.compiler_output.allow_no_tools_fallback,
    );

    let hot_window_turns = parse_usize_with_bounds(
        &params.settings.slice_hot_window_turns,
        DEFAULT_HOT_WINDOW_TURNS,
        MIN_HOT_WINDOW_TURNS,
        MAX_HOT_WINDOW_TURNS,
    );
    let cold_window_turns = parse_usize_with_bounds(
        &params.settings.slice_cold_window_turns,
        DEFAULT_COLD_WINDOW_TURNS,
        MIN_COLD_WINDOW_TURNS,
        MAX_COLD_WINDOW_TURNS,
    )
    .max(hot_window_turns);

    let prior_build = turn_services::build_prior_messages(
        params.session_id,
        params.conversation,
        params.prompt,
        params.persist_user_turn,
        hot_window_turns,
        cold_window_turns,
        PriorMessageLimits {
            max_prior_total_chars: MAX_PRIOR_TOTAL_CHARS,
            max_single_prior_message_chars: MAX_SINGLE_PRIOR_MESSAGE_CHARS,
            hot_window_char_budget: HOT_WINDOW_CHAR_BUDGET,
            cold_window_char_budget: COLD_WINDOW_CHAR_BUDGET,
            cold_summary_line_chars: COLD_SUMMARY_LINE_CHARS,
        },
    );

    let prompt_for_request = if activation.enforce_no_tools {
        format!(
            "{}\n\n[MEDOUSA_TOOL_POLICY]\nmode=no_tools\ninstruction=Do not call tools for this turn unless the user explicitly requests external lookup, execution, or fresh evidence. Answer directly from current context.",
            params.resolved_prompt
        )
    } else {
        append_tool_loop_policy(&params.resolved_prompt, activation.max_tool_rounds)
    };
    let current_turn_user_message = params
        .vision_plan
        .build_user_message(&prompt_for_request);

    let pipeline_selection = turn_services::select_pipeline_for_turn_with_allowlist(
        params.tui_rt,
        params.final_route,
        params.settings,
        params.scheduled_tool_allowlist.clone(),
    );

    AssembledLocalTurn {
        execution: LocalTurnExecutionParams {
            turn_id: params.turn_id,
            session_id: params.session_id.to_string(),
            backend: params.settings.backend.clone(),
            provider: params.settings.provider.clone(),
            model: params.settings.model.clone(),
            base_url: (!params.settings.base_url.trim().is_empty())
                .then(|| params.settings.base_url.clone()),
            response_depth_mode: params.response_depth_mode.to_string(),
            worker_scheduler: params.tui_rt.worker_scheduler.clone(),
            tool_registry: params.tui_rt.tool_registry.clone(),
            identity_memory_store: Some(
                params.tui_rt.identity_memory_store.clone()
                    as Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>,
            ),
            turn_scope: params.tui_rt.turn_scope.clone(),
            activation: activation.clone(),
            pipeline: pipeline_selection.pipeline.clone(),
            no_tools_pipeline: turn_services::build_prompt_pipeline_for_turn(
                params.final_route,
                params.settings,
            ),
            prior_messages: prior_build.messages.clone(),
            prompt_for_request,
            original_prompt: params.prompt.to_string(),
            intent_classifier_recent_context: turn_services::build_intent_classifier_recent_context(
                params.conversation,
                params.prompt,
                params.persist_user_turn,
                INTENT_CLASSIFIER_MAX_CONTEXT_TURNS,
                INTENT_CLASSIFIER_MAX_CONTEXT_CHARS,
                IntentContextLimits {
                    context_line_chars: INTENT_CLASSIFIER_CONTEXT_LINE_CHARS,
                },
            ),
            retry_max_retries: parse_usize_with_bounds(
                &params.settings.retry_runtime_max_retries,
                DEFAULT_RETRY_RUNTIME_MAX_RETRIES,
                OPERATOR_RETRY_LIMIT_MIN,
                OPERATOR_RETRY_LIMIT_MAX,
            ),
            retry_max_rounds: parse_usize_with_bounds(
                &params.settings.retry_runtime_max_rounds,
                DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
                OPERATOR_ROUND_LIMIT_MIN,
                OPERATOR_ROUND_LIMIT_MAX,
            ),
            continuation_response_depth_mode: params.response_depth_mode.to_string(),
            continuation_stage_route: params.final_route.cloned(),
            continuation_recall_readiness: params.prepared.recall_readiness,
            prompt_preview: params.resolved_prompt.chars().take(48).collect(),
            turn_loop_settings,
            handoff_vibe_signature: params.prepared.handoff_vibe_signature.clone(),
            handoff_model_avec: params.prepared.handoff_model_avec,
            host_continuity_bundle: Some(super::worker_continuity::build_host_continuity_bundle(
                params.prepared,
                params.conversation,
                None,
            )),
            session_scratch_seed: session_scratch_seed_from_history(
                params.conversation,
                params.prompt,
            ),
            current_turn_user_message,
        },
        pipeline_selection,
        activation: activation.clone(),
        prior_build,
    }
}

pub fn should_invoke_intent_classifier(activation: &TurnActivationDecision) -> bool {
    activation.reason == "configured_default"
}

pub async fn classify_turn_intent_with_model(
    pipeline: &PromptExecutionPipeline,
    prompt: &str,
    recent_context: &str,
) -> Option<IntentClassification> {
    let bounded_prompt = truncate_text_for_budget(prompt, INTENT_CLASSIFIER_MAX_PROMPT_CHARS);
    let bounded_context =
        truncate_text_for_budget(recent_context, INTENT_CLASSIFIER_MAX_CONTEXT_CHARS);
    let messages = vec![
        ChatMessage::system(
            "Intent routing for tool-loop turns. Classify CURRENT_USER_MESSAGE with RECENT_CONTEXT as local grounding only. Return strict JSON: intent, confidence, reason. intent ∈ conversational | tool_required | clarify | mixed. Use clarify when the principal should get one direct question instead of tools (vague goal, missing target, ambiguous scope).".to_string(),
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

pub fn apply_intent_classifier_override(
    base: TurnActivationDecision,
    classification: &IntentClassification,
    classifier_restricted_max_tool_rounds: usize,
) -> TurnActivationDecision {
    let restricted = classifier_restricted_max_tool_rounds.max(1);
    if classification.confidence < INTENT_CLASSIFIER_CONFIDENCE_LOW {
        return TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: restricted,
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
                max_tool_rounds: restricted,
                enforce_no_tools: true,
                reason: "classifier_conversational",
            }
        }
        "clarify" => TurnActivationDecision {
            turn_class: "a",
            tool_call_mode: ToolCallMode::Strict,
            max_tool_rounds: restricted,
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

fn build_tool_loop_failure_explanation_prompt(
    original_prompt: &str,
    runtime_error: &str,
    scratch: Option<&TurnScratchpad>,
) -> String {
    let scratch_block = scratch
        .map(|s| {
            format!(
                "\n\nTURN_SCRATCHPAD (tool-loop working memory, no raw tool transcript):\n{}",
                s.format_control_body(0)
            )
        })
        .unwrap_or_default();
    format!(
        "[MEDOUSA_TURN_RUNTIME]\n\
         The interactive tool loop ended without a complete user-facing answer.\n\n\
         RUNTIME_ERROR:\n{runtime_error}\n\n\
         ORIGINAL_USER_MESSAGE:\n{original_prompt}{scratch_block}\n\n\
         Write one clear user-facing message: explain what happened in plain language, what was \
         attempted if you can infer it from the error, and what the user can try next (retry, \
         clarify, adjust settings, simpler request, or ask them for specific missing details). \
         Do not invent tool results or claim success you did not achieve. \
         This is a final explanation pass only — do not call tools."
    )
}

fn fallback_failure_explanation_text(runtime_error: &str) -> String {
    format!(
        "I couldn't finish that turn cleanly. Technical detail: {}. \
         You can retry, simplify the request, or share any missing context (session id, paths, etc.).",
        truncate_text_for_budget(runtime_error, 800)
    )
}

async fn deliver_tool_loop_failure_explanation(
    sink: &SharedAgentStreamSink,
    turn_id: u64,
    no_tools_pipeline: &PromptExecutionPipeline,
    chunk_tx: &tokio::sync::mpsc::UnboundedSender<StreamDelta>,
    original_prompt: &str,
    runtime_error: &str,
    prior_messages: Vec<ChatMessage>,
    scratch: Option<&TurnScratchpad>,
    host_bus: bool,
    suggested_intent: Option<&str>,
    orchestration_state: &mut TurnOrchestrationState,
    turn_budget: &TurnBudget,
) {
    let _ = try_consume_prompt_only_budget(sink, orchestration_state, turn_budget).await;
    orchestration_state.final_mode = "tool_loop_failure_explanation".to_string();

    sink.notice(
        "◈ fallback_mode=runtime_failure_explanation retry_count=0 (no tools)".to_string(),
    )
    .await;

    let explanation_prompt = truncate_text_for_budget(
        &build_tool_loop_failure_explanation_prompt(original_prompt, runtime_error, scratch),
        MAX_REQUEST_PROMPT_CHARS,
    );

    let mut messages = Vec::with_capacity(prior_messages.len() + 2);
    messages.push(ChatMessage::system(system_prompt_for_host_profile(
        DEFAULT_SYSTEM_PROMPT,
        host_bus,
        suggested_intent,
    )));
    messages.extend(prior_messages);
    messages.push(ChatMessage::user(explanation_prompt));

    sink.tool_invoked(
        "llm.chat".to_string(),
        "runtime failure explanation (no tools)".to_string(),
    )
    .await;

    let final_text = match no_tools_pipeline
        .complete_chat_stream(
            ChatRequest::new(messages),
            PromptExecutionContext::default(),
            Some(chunk_tx),
        )
        .await
    {
        Ok(completion) => completion
            .response
            .into_first_text()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| fallback_failure_explanation_text(runtime_error)),
        Err(err) => {
            sink.notice(format!(
                "⚠ runtime failure explanation LLM failed: {err}; using fallback text"
            ))
            .await;
            fallback_failure_explanation_text(runtime_error)
        }
    };

    sink.agent_response(turn_id, final_text, Vec::new()).await;
}

pub fn retryable_runtime_reason(err_text: &str) -> Option<&'static str> {
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

pub async fn emit_tool_payload_events(
    sink: &SharedAgentStreamSink,
    invocations: &[ToolInvocation],
) {
    for invocation in invocations {
        let safe_input = crate::settings_guard::redact_json_value(&invocation.tool_input);
        let safe_output = crate::settings_guard::redact_json_value(&invocation.tool_output);
        sink.tool_payload(
            invocation.tool_name.clone(),
            invocation.tool_input.clone(),
            invocation.tool_output.clone(),
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

async fn stage_scratch_for_persist(
    sink: &SharedAgentStreamSink,
    scratch: &Option<TurnScratchpad>,
) {
    if let Some(scratch) = scratch.clone() {
        sink.stage_persist_scratch(scratch).await;
    }
}

fn host_tool_round_budget_ceiling(settings: &TurnLoopSettings, loop_max_rounds: usize) -> usize {
    settings
        .effective_host_bus_max_tool_rounds()
        .max(loop_max_rounds)
}

fn require_operator_budget_gate() -> bool {
    matches!(
        std::env::var("MEDOUSA_TURN_BUDGET_OPERATOR_GATE")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

pub async fn execute_local_turn(sink: SharedAgentStreamSink, params: LocalTurnExecutionParams) {
    let LocalTurnExecutionParams {
        turn_id,
        session_id,
        backend,
        provider,
        model,
        base_url,
        response_depth_mode,
        worker_scheduler,
        tool_registry,
        identity_memory_store,
        turn_scope,
        mut activation,
        pipeline: default_pipeline,
        no_tools_pipeline,
        prior_messages,
        prompt_for_request,
        original_prompt,
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
        mut host_continuity_bundle,
        session_scratch_seed,
        current_turn_user_message,
    } = params;

    sink.notice(format!(
        "◈ turn_loop_limits {}",
        turn_loop_settings.operator_summary()
    ))
    .await;

    let host_profile = resolve_host_turn_profile(
        &original_prompt,
        activation.max_tool_rounds,
        turn_loop_settings.effective_host_bus_max_tool_rounds(),
        turn_loop_settings.effective_host_bus_env_mode(),
    );
    activation = apply_host_profile_to_activation(activation, &host_profile);
    sink.notice(format!(
        "◈ activation effective rounds={} (after host bus; configured_max={})",
        activation.max_tool_rounds, turn_loop_settings.configured_max_tool_rounds
    ))
    .await;
    let host_bus = host_profile.host_bus_active;
    let suggested_intent = host_profile
        .route
        .suggested_worker_intent()
        .map(|i| i.as_str());
    sink.notice(host_route_notice(&host_profile)).await;

    worker_scheduler
        .set_runtime_context(WorkerRuntimeContext {
            tool_registry: tool_registry.clone(),
            identity_memory_store: identity_memory_store.clone(),
            provider: provider.clone(),
            model: model.clone(),
            base_url: base_url.clone(),
        })
        .await;

    let scope_snapshot = turn_scope.read().await.clone();
    if let Some(bundle) = host_continuity_bundle.as_mut() {
        bundle.parent_turn_correlation_id = scope_snapshot
            .as_ref()
            .map(|scope| scope.turn_correlation_id.clone());
        sink.notice(format!(
            "◈ worker_continuity {}",
            bundle.log_summary()
        ))
        .await;
    }
    let handoff_continuity_bundle = host_continuity_bundle.clone();
    let host_handoff_slot = Arc::new(tokio::sync::RwLock::new(None));
    worker_scheduler
        .set_bus_session(ActiveWorkerBusSession {
            sink: sink.clone(),
            stream_turn_id: turn_id,
            session_id: session_id.clone(),
            backend: backend.clone(),
            parent_user_prompt: original_prompt.clone(),
            provider: provider.clone(),
            model: model.clone(),
            response_depth_mode: response_depth_mode.clone(),
            parent_turn_correlation_id: scope_snapshot
                .as_ref()
                .map(|scope| scope.turn_correlation_id.clone()),
            delivery_target: scope_snapshot
                .as_ref()
                .and_then(|scope| scope.delivery_target.as_ref())
                .map(StoredDeliveryTarget::from),
            host_handoff_slot: host_handoff_slot.clone(),
            host_continuity_bundle,
            configured_max_tool_rounds: turn_loop_settings.configured_max_tool_rounds,
        })
        .await;

    let pipeline = if host_bus {
        pipeline_for_turn_profile(
            tool_registry.clone(),
            &provider,
            &model,
            base_url.as_deref(),
            true,
            Some(session_id.as_str()),
        )
    } else {
        default_pipeline
    };

    let turn_budget = turn_budget_for_lane(EngineExecutionLane::Interactive);
    let mut orchestration_state = TurnOrchestrationState {
        final_mode: "unknown".to_string(),
        ..TurnOrchestrationState::default()
    };

    if should_invoke_intent_classifier(&activation) {
        if try_consume_classifier_budget(&sink, &mut orchestration_state, &turn_budget).await {
            let classification = classify_turn_intent_with_model(
                &no_tools_pipeline,
                &original_prompt,
                &intent_classifier_recent_context,
            )
            .await;
            if let Some(classification) = classification {
                sink.notice(format!(
                    "◈ intent classifier intent={} confidence={:.2} reason={}",
                    classification.intent, classification.confidence, classification.reason
                ))
                .await;

                activation = apply_intent_classifier_override(
                    activation,
                    &classification,
                    turn_loop_settings.classifier_restricted_max_tool_rounds,
                );
                sink.notice(format!(
                    "◈ activation final class={} mode={} rounds={} no_tools={} reason={}",
                    activation.turn_class,
                    match activation.tool_call_mode {
                        ToolCallMode::Auto => "auto",
                        ToolCallMode::Strict => "strict",
                    },
                    activation.max_tool_rounds,
                    activation.enforce_no_tools,
                    activation.reason,
                ))
                .await;
            } else {
                sink.notice(
                    "◈ intent classifier skipped: no parseable result; using heuristic"
                        .to_string(),
                )
                .await;
            }
        } else {
            orchestration_state.final_mode = "classifier_budget_denied".to_string();
        }
    }

    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::unbounded_channel::<StreamDelta>();
    let chunk_sink = sink.clone();
    tokio::spawn(async move {
        while let Some(delta) = chunk_rx.recv().await {
            match delta {
                StreamDelta::Content(delta) => chunk_sink.content_chunk(turn_id, delta).await,
                StreamDelta::Reasoning(delta) | StreamDelta::ThoughtSignature(delta) => {
                    chunk_sink.reasoning_chunk(turn_id, delta).await
                }
            }
        }
    });

    sink.tool_invoked("llm.chat".to_string(), prompt_preview)
        .await;

    if activation.enforce_no_tools {
        let mut messages = Vec::with_capacity(prior_messages.len() + 2);
        messages.push(ChatMessage::system(system_prompt_for_host_profile(
            DEFAULT_SYSTEM_PROMPT,
            host_bus,
            suggested_intent,
        )));
        messages.extend(prior_messages);
        messages.push(current_turn_user_message.clone());

        if !try_consume_prompt_only_budget(&sink, &mut orchestration_state, &turn_budget).await {
            orchestration_state.final_mode = "prompt_only_budget_denied".to_string();
            sink.agent_error(
                turn_id,
                "turn budget exhausted before prompt-only execution".to_string(),
            )
            .await;
            emit_orchestration_summary(&sink, &orchestration_state).await;
            return;
        }
        orchestration_state.final_mode = "prompt_only".to_string();

        sink.notice(
            "◈ fallback_mode=prompt_only retry_count=0 retry_reason=none".to_string(),
        )
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
                super::turn_delivery::deliver_agent_turn_outcome(
                    &sink,
                    turn_id,
                    final_text,
                    Vec::new(),
                    super::turn_delivery::AgentTurnDeliveryHint {
                        activation_reason: activation.reason,
                    },
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            }
            Err(err) => {
                sink.agent_error(turn_id, err.to_string()).await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            }
        }
        return;
    }

    let request = ToolLoopExecutionRequest {
        user_prompt: prompt_for_request,
        system_prompt: Some(system_prompt_for_host_profile(
            DEFAULT_SYSTEM_PROMPT,
            host_bus,
            suggested_intent,
        )),
        context: PromptExecutionContext::default(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: activation.tool_call_mode,
    };
    if !try_consume_tool_loop_budget(&sink, &mut orchestration_state, &turn_budget).await {
        orchestration_state.final_mode = "tool_loop_budget_denied".to_string();
        sink.agent_error(
            turn_id,
            "turn budget exhausted before tool-loop execution".to_string(),
        )
        .await;
        emit_orchestration_summary(&sink, &orchestration_state).await;
        return;
    }
    orchestration_state.final_mode = "tool_loop".to_string();
    let ledger_session_id = (!session_id.trim().is_empty()).then(|| session_id.clone());
    let parent_turn_correlation_id = scope_snapshot
        .as_ref()
        .map(|scope| scope.turn_correlation_id.clone());
    let origin_channel = scope_snapshot
        .as_ref()
        .and_then(|scope| scope.delivery_target.as_ref().map(|target| target.channel.clone()))
        .or_else(|| Some("interactive".to_string()));
    let origin_delivery_target = scope_snapshot
        .as_ref()
        .and_then(|scope| scope.delivery_target.as_ref())
        .map(StoredDeliveryTarget::from);
    let mut last_tool_scratch: Option<TurnScratchpad> = None;
    let first_attempt = {
        let loop_max_rounds = activation.max_tool_rounds.max(1);
        let mut completion_gate = ToolLoopCompletionGate {
            stream_turn_id: turn_id,
            session_id: ledger_session_id.clone(),
            sink: Some(sink.clone()),
            orchestration: Some(&mut orchestration_state),
            budget: Some(&turn_budget),
            max_tool_rounds: loop_max_rounds,
            max_text_only_stuck_continues: turn_loop_settings.max_text_only_stuck_continues,
            scratch_out: Some(&mut last_tool_scratch),
            host_handoff_slot: Some(host_handoff_slot.clone()),
            parent_turn_correlation_id: parent_turn_correlation_id.clone(),
            initial_worker_scratch: Some(session_scratch_seed.clone()),
            handoff_parent_user_prompt: Some(original_prompt.clone()),
            handoff_vibe_signature: Some(handoff_vibe_signature.clone()),
            handoff_model_avec: Some(handoff_model_avec),
            handoff_continuity_bundle: handoff_continuity_bundle.clone(),
            skip_avec_ritual_check: false,
            channel: origin_channel.clone(),
            delivery_target: origin_delivery_target.clone(),
            tool_round_budget_ceiling: host_tool_round_budget_ceiling(
                &turn_loop_settings,
                loop_max_rounds,
            ),
            require_operator_budget_gate: require_operator_budget_gate(),
        };
        pipeline
            .execute_with_stream_prior_messages_max_rounds(
                request.clone(),
                prior_messages.clone(),
                Some(&chunk_tx),
                loop_max_rounds,
                Some(&mut completion_gate),
                Some(current_turn_user_message.clone()),
            )
            .await
    };

    match first_attempt {
        Ok(response) => {
            sink.notice(
                "◈ fallback_mode=tool_loop retry_count=0 retry_reason=none".to_string(),
            )
            .await;
            let mut combined_invocations = response.tool_invocations.clone();
            let mut final_text = response.text;
            if response.termination_reason == "worker_spawned" {
                let tool_names = collect_tool_names(&combined_invocations);
                let work_id = crate::agent_runtime::turn_worker_tools::worker_spawn_from_invocations(
                    &combined_invocations,
                )
                .map(|(id, _)| id);
                stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                sink.agent_worker_ack(turn_id, final_text, tool_names, work_id)
                    .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            if response.termination_reason == "cognition_turn_checkpoint" {
                let tool_names = collect_tool_names(&combined_invocations);
                sink.tool_invoked(
                    "llm.chat".to_string(),
                    format!(
                        "checkpoint  {} token(s)",
                        final_text.split_whitespace().count()
                    ),
                )
                .await;
                stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                super::turn_delivery::deliver_agent_turn_checkpoint(
                    &sink,
                    turn_id,
                    final_text,
                    tool_names,
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            if should_run_continuation(&combined_invocations)
                && !crate::channel_delivery::is_principal_interactive_channel(
                    origin_channel.as_deref().unwrap_or(channel_delivery::CHANNEL_INTERACTIVE),
                )
            {
                if let Some(continuation_prompt) = build_continuation_prompt(
                    &original_prompt,
                    &final_text,
                    &combined_invocations,
                ) {
                    let continuation_compiler_output = compile_interactive_context_prompt(
                        &continuation_prompt,
                        &continuation_response_depth_mode,
                        continuation_stage_route.as_ref(),
                        continuation_recall_readiness,
                    );
                    let continuation_compiled_prompt = truncate_text_for_budget(
                        &continuation_compiler_output.compiled_prompt,
                        MAX_REQUEST_PROMPT_CHARS,
                    );
                    sink.notice(
                        "◈ continuation synthesis: refining draft with chunked tool context"
                            .to_string(),
                    )
                    .await;
                    sink.notice(format!(
                        "◈ {}",
                        continuation_compiler_output.compiler_summary
                    ))
                    .await;

                    sink.tool_invoked(
                        "llm.chat".to_string(),
                        "continuation synthesis".to_string(),
                    )
                    .await;

                    let continuation_request = ToolLoopExecutionRequest {
                        user_prompt: continuation_compiled_prompt,
                        system_prompt: Some(DEFAULT_SYSTEM_PROMPT.to_string()),
                        context: PromptExecutionContext::default(),
                        tool_name: String::new(),
                        tool_input: Value::Null,
                        tool_call_mode: ToolCallMode::Auto,
                    };
                    let continuation_prior_messages =
                        build_continuation_prior_messages(&original_prompt, &final_text);

                    if try_consume_continuation_budget(&sink, &mut orchestration_state, &turn_budget)
                        .await
                    {
                        orchestration_state.final_mode = "tool_loop_with_continuation".to_string();

                        let continuation_result = {
                            let continuation_max_rounds = activation
                                .max_tool_rounds
                                .min(turn_loop_settings.continuation_max_tool_rounds)
                                .max(1);
                            let mut continuation_gate = ToolLoopCompletionGate {
                                stream_turn_id: turn_id,
                                session_id: ledger_session_id.clone(),
                                sink: Some(sink.clone()),
                                orchestration: Some(&mut orchestration_state),
                                budget: Some(&turn_budget),
                                max_tool_rounds: continuation_max_rounds,
                                max_text_only_stuck_continues: turn_loop_settings
                                    .max_text_only_stuck_continues,
                                scratch_out: Some(&mut last_tool_scratch),
                                host_handoff_slot: Some(host_handoff_slot.clone()),
                                parent_turn_correlation_id: parent_turn_correlation_id.clone(),
                                initial_worker_scratch: Some(session_scratch_seed.clone()),
                                handoff_parent_user_prompt: Some(original_prompt.clone()),
                                handoff_vibe_signature: Some(handoff_vibe_signature.clone()),
                                handoff_model_avec: Some(handoff_model_avec),
                                handoff_continuity_bundle: handoff_continuity_bundle.clone(),
                                skip_avec_ritual_check: false,
                                channel: origin_channel.clone(),
                                delivery_target: origin_delivery_target.clone(),
                                tool_round_budget_ceiling: host_tool_round_budget_ceiling(
                                    &turn_loop_settings,
                                    continuation_max_rounds,
                                ),
                                require_operator_budget_gate: require_operator_budget_gate(),
                            };
                            pipeline
                                .execute_with_stream_prior_messages_max_rounds(
                                    continuation_request,
                                    continuation_prior_messages,
                                    Some(&chunk_tx),
                                    continuation_max_rounds,
                                    Some(&mut continuation_gate),
                                    None,
                                )
                                .await
                        };

                        match continuation_result
                        {
                            Ok(continuation_response) => {
                                final_text = continuation_response.text;
                                combined_invocations.extend(continuation_response.tool_invocations);
                            }
                            Err(err) => {
                                sink.notice(format!("⚠ continuation synthesis skipped: {err}"))
                                    .await;
                            }
                        }
                    } else {
                        sink.notice(
                            "◈ continuation synthesis skipped: turn budget limit".to_string(),
                        )
                        .await;
                    }
                }
            }

            let profile = super::presentation::presentation_profile_for_channel(
                origin_channel.as_deref().unwrap_or(channel_delivery::CHANNEL_INTERACTIVE),
            );
            super::presentation::maybe_append_tools_to_canonical_body(
                &mut final_text,
                &combined_invocations,
                profile,
            );
            let tool_names = collect_tool_names(&combined_invocations);
            sink.tool_invoked(
                "llm.chat".to_string(),
                format!(
                    "done  {} token(s)",
                    final_text.split_whitespace().count()
                ),
            )
            .await;
            stage_scratch_for_persist(&sink, &last_tool_scratch).await;
            super::turn_delivery::deliver_agent_turn_outcome(
                &sink,
                turn_id,
                final_text,
                tool_names,
                super::turn_delivery::AgentTurnDeliveryHint {
                    activation_reason: activation.reason,
                },
            )
            .await;
            emit_orchestration_summary(&sink, &orchestration_state).await;
        }
        Err(err) => {
            let err_text = err.to_string();
            if let Some(reason) = retryable_runtime_reason(&err_text) {
                // Retry uses the same tool-round budget as the primary loop unless the
                // operator explicitly set a lower retry_runtime_max_rounds cap.
                let retry_rounds = activation
                    .max_tool_rounds
                    .min(retry_max_rounds.max(activation.max_tool_rounds))
                    .max(1);
                let mut last_err = err_text;
                let mut retry_count = 0usize;
                while retry_count < retry_max_retries {
                    retry_count = retry_count.saturating_add(1);
                    sink.notice(format!(
                        "◈ retry_policy retry_count={} retry_reason={} fallback_mode=tool_loop rounds={}",
                        retry_count, reason, retry_rounds
                    ))
                    .await;

                    if !try_consume_retry_budget(&sink, &mut orchestration_state, &turn_budget)
                        .await
                    {
                        orchestration_state.final_mode = "tool_loop_retry_budget_denied".to_string();
                        sink.agent_error(
                            turn_id,
                            "turn budget exhausted before retry".to_string(),
                        )
                        .await;
                        emit_orchestration_summary(&sink, &orchestration_state).await;
                        return;
                    }
                    orchestration_state.final_mode = "tool_loop_retry".to_string();

                    let retry_result = {
                        let mut retry_gate = ToolLoopCompletionGate {
                            stream_turn_id: turn_id,
                            session_id: ledger_session_id.clone(),
                            sink: Some(sink.clone()),
                            orchestration: Some(&mut orchestration_state),
                            budget: Some(&turn_budget),
                            max_tool_rounds: retry_rounds,
                            max_text_only_stuck_continues: turn_loop_settings
                                .max_text_only_stuck_continues,
                            scratch_out: Some(&mut last_tool_scratch),
                            host_handoff_slot: Some(host_handoff_slot.clone()),
                            parent_turn_correlation_id: parent_turn_correlation_id.clone(),
                            initial_worker_scratch: Some(session_scratch_seed.clone()),
                            handoff_parent_user_prompt: Some(original_prompt.clone()),
                            handoff_vibe_signature: Some(handoff_vibe_signature.clone()),
                            handoff_model_avec: Some(handoff_model_avec),
                            handoff_continuity_bundle: handoff_continuity_bundle.clone(),
                            skip_avec_ritual_check: false,
                            channel: origin_channel.clone(),
                            delivery_target: origin_delivery_target.clone(),
                            tool_round_budget_ceiling: host_tool_round_budget_ceiling(
                                &turn_loop_settings,
                                retry_rounds,
                            ),
                            require_operator_budget_gate: require_operator_budget_gate(),
                        };
                        pipeline
                            .execute_with_stream_prior_messages_max_rounds(
                                request.clone(),
                                prior_messages.clone(),
                                Some(&chunk_tx),
                                retry_rounds,
                                Some(&mut retry_gate),
                                None,
                            )
                            .await
                    };

                    match retry_result
                    {
                        Ok(response) => {
                            let tool_names = collect_tool_names(&response.tool_invocations);
                            stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                            super::turn_delivery::deliver_agent_turn_outcome(
                                &sink,
                                turn_id,
                                response.text,
                                tool_names,
                                super::turn_delivery::AgentTurnDeliveryHint {
                                    activation_reason: activation.reason,
                                },
                            )
                            .await;
                            orchestration_state.final_mode = "tool_loop_retry_success".to_string();
                            emit_orchestration_summary(&sink, &orchestration_state).await;
                            return;
                        }
                        Err(retry_err) => {
                            last_err = format!("{}", retry_err);
                        }
                    }
                }
                orchestration_state.final_mode = "tool_loop_retry_exhausted".to_string();
                deliver_tool_loop_failure_explanation(
                    &sink,
                    turn_id,
                    &no_tools_pipeline,
                    &chunk_tx,
                    &original_prompt,
                    &format!("{reason} (retry exhausted: {last_err})"),
                    prior_messages.clone(),
                    last_tool_scratch.as_ref(),
                    host_bus,
                    suggested_intent,
                    &mut orchestration_state,
                    &turn_budget,
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            } else {
                sink.notice(
                    "◈ retry_policy retry_count=0 retry_reason=not_runtime".to_string(),
                )
                .await;
                orchestration_state.final_mode = "tool_loop_error_non_retryable".to_string();
                deliver_tool_loop_failure_explanation(
                    &sink,
                    turn_id,
                    &no_tools_pipeline,
                    &chunk_tx,
                    &original_prompt,
                    &err_text,
                    prior_messages.clone(),
                    last_tool_scratch.as_ref(),
                    host_bus,
                    suggested_intent,
                    &mut orchestration_state,
                    &turn_budget,
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            }
        }
    }

    worker_scheduler.clear_bus_session().await;
}
