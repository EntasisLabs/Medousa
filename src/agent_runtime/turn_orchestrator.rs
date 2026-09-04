use std::sync::Arc;

use genai::chat::{ChatMessage, ChatRequest};
use medousa_runtime::MedousaToolLoopPipeline;
use serde_json::Value;
use stasis::application::orchestration::prompt_pipeline::{
    PromptExecutionContext, PromptExecutionPipeline,
};
use stasis::application::orchestration::tool_loop_pipeline::{
    ToolCallMode, ToolInvocation, ToolLoopExecutionRequest,
};
#[cfg(test)]
use stasis::ports::outbound::ai_chat_client::StreamDelta;

use crate::channel_delivery;
use crate::daemon_api::TurnSurfaceContext;
use crate::engine_context::{EngineExecutionLane, RecallReadiness};
use crate::session::ConversationTurn;
use crate::stage_routing::StageRoute;
use crate::tools::TuiRuntime;
use crate::tui::settings::{
    OPERATOR_RETRY_LIMIT_MAX, OPERATOR_RETRY_LIMIT_MIN, OPERATOR_ROUND_LIMIT_MAX,
    OPERATOR_ROUND_LIMIT_MIN, RuntimeSettings, parse_usize_with_bounds,
};
use stasis::ports::outbound::memory::memory_models::MemoryAvecState;

use super::continuation::{
    build_continuation_prior_messages, build_continuation_prompt, collect_tool_names,
    should_run_continuation,
};
use super::prompt_prep::{
    CheapRecallProbe, IdentityContextProbe, MAX_REQUEST_PROMPT_CHARS, append_identity_context_hint,
    append_manuscript_hint, append_memory_recall_hint, append_suggested_capabilities_hint,
    append_voice_preset_hint, channel_policy_probe, cheap_memory_recall_probe,
    compile_interactive_context_prompt, derive_recall_readiness, identity_context_probe,
    resolve_prompt_with_context_pack, truncate_text_for_budget,
    verifier_policy_from_settings_and_route,
};
use super::stream_sink::SharedAgentStreamSink;
use super::turn_budget::{
    TurnOrchestrationState, emit_orchestration_summary, try_consume_classifier_budget,
    try_consume_continuation_budget, try_consume_prompt_only_budget, try_consume_retry_budget,
    try_consume_tool_loop_budget, turn_budget_for_lane,
};
use super::turn_completion::ToolLoopCompletionGateConfig;
use super::turn_context::TurnScratchpad;
use super::turn_context::scratch_seed_for_tool_loop;
use super::turn_ledger::append_tool_loop_policy;
use super::turn_loop_settings::TurnLoopSettings;
use super::turn_services::{
    self, IntentContextLimits, PriorMessageBuild, PriorMessageLimits, SelectedTurnPipeline,
    TurnActivationDecision,
};
use super::turn_worker::{
    ActiveWorkerBusSession, WorkerRuntimeContext, apply_host_profile_to_activation,
    host_route_notice, pipeline_for_turn_profile, resolve_host_turn_profile,
};
use crate::turn_continuation::StoredDeliveryTarget;
use crate::turn_slice::session_scratch_seed_from_history;

#[cfg(test)]
use super::provider_stream::{
    PROVIDER_STREAM_BYTE_CAPACITY as STREAM_BRIDGE_BYTE_CAPACITY,
    PROVIDER_STREAM_MESSAGE_CAPACITY as STREAM_BRIDGE_MESSAGE_CAPACITY,
    ProviderStreamReport as AttemptStreamReport,
};
use super::provider_stream::{ProviderStreamBridge as TurnStreamBridge, fail_on_stream_overflow};
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
    pub agent_mode: super::modes::ResolvedAgentMode,
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
    pub agent_mode: super::modes::ResolvedAgentMode,
    /// Immutable, daemon-compiled context for the selected mode.
    pub mode_context_appendix: Option<&'a str>,
    pub session_id: &'a str,
    /// Stable memory continuity scope. This differs from `session_id` only for
    /// a daemon-resolved Bot turn.
    pub memory_session_id: &'a str,
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
    /// Immutable Bot identity/job snapshot captured at turn admission.
    pub bot_profile_appendix: Option<&'a str>,
    pub suggested_capability_ids: Option<&'a [String]>,
    pub voice_preset_id: Option<&'a str>,
    pub voice_appendix: Option<&'a str>,
    /// Resolved identity principal for turn-start digest and channel policy (active profile on daemon).
    pub identity_user_id: &'a str,
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
        cheap_memory_recall_probe(params.tui_rt, params.memory_session_id, params.prompt).await;
    let manuscript_ctx = params
        .manuscript_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|id| crate::identity_manuscript::build_manuscript_context(id).ok());
    let identity_probe = identity_context_probe(
        params.tui_rt,
        params
            .final_route
            .map(|route| route.policy_profile.as_str()),
        Some(params.prompt),
        manuscript_ctx.as_ref(),
        params.identity_user_id,
    )
    .await;
    let channel_policy = channel_policy_probe(
        params.tui_rt,
        params
            .final_route
            .map(|route| route.policy_profile.as_str()),
        params.identity_user_id,
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
    if let Some(bot_profile_appendix) = params
        .bot_profile_appendix
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        resolved_prompt = format!("{resolved_prompt}\n\n{bot_profile_appendix}");
    }
    if let Some(ids) = params.suggested_capability_ids {
        resolved_prompt = append_suggested_capabilities_hint(&resolved_prompt, ids);
    }
    resolved_prompt = append_voice_preset_hint(
        &resolved_prompt,
        params.voice_preset_id,
        params.voice_appendix,
    );
    resolved_prompt = append_identity_context_hint(&resolved_prompt, &identity_probe);
    resolved_prompt = crate::agent_runtime::turn_worker::append_active_workers_hint(
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
    let environment_extras =
        super::ambient_context::build_environment_ambient_extras(params.session_id).await;
    let ambient_appendix = if environment_extras.is_empty() {
        ambient_block.appendix.clone()
    } else {
        format!("{}\n\n{environment_extras}", ambient_block.appendix)
    };
    resolved_prompt = format!("{resolved_prompt}\n\n{ambient_appendix}");
    if let Some(mode_context_appendix) = params.mode_context_appendix {
        resolved_prompt = format!("{resolved_prompt}\n\n{mode_context_appendix}");
    }

    let handoff_model_avec = super::vibe_signature::default_handoff_model_avec();
    let handoff_vibe_signature = super::vibe_signature::derive_vibe_signature(
        params.session_id,
        params.surface,
        Some(&channel_policy),
        &handoff_model_avec,
    );

    PreparedTurnPrompt {
        agent_mode: params.agent_mode,
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
    pub agent_mode: super::modes::ResolvedAgentMode,
    pub turn_id: u64,
    pub session_id: String,
    pub backend: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub response_depth_mode: String,
    pub reasoning_effort: String,
    pub worker_scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
    pub tool_registry: Arc<dyn stasis::application::orchestration::tool_registry::ToolRegistry>,
    pub client_registry: crate::client_tools::ClientRegistry,
    pub identity_memory_store: Option<
        Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>,
    >,
    pub turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
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
    pub inference_profile_kind: crate::inference_profiles::InferenceProfileKind,
    pub inference_targets: Vec<crate::inference_profiles::InferenceTarget>,
    pub supports_ui_artifacts: bool,
    pub supports_liquid_markdown: bool,
    pub supports_browser_host: bool,
    pub round_context_provider: Option<Arc<dyn super::turn_context::ToolRoundContextProvider>>,
    pub evidence_undertaking_id: Option<String>,
    pub compact_evidence_receipt_sink:
        Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
    pub active_turn_checkpoint_sink:
        Option<Arc<dyn super::coder_turn_checkpoint::ActiveTurnCheckpointSink>>,
    pub active_turn_resume: Option<super::coder_turn_checkpoint::ActiveTurnResumeState>,
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
    pub tool_registry_override:
        Option<Arc<dyn stasis::application::orchestration::tool_registry::ToolRegistry>>,
    pub final_route: Option<&'a StageRoute>,
    pub response_depth_mode: &'a str,
    pub reasoning_effort: &'a str,
    pub max_tool_rounds_override: Option<usize>,
    pub turn_id: u64,
    pub scheduled_tool_allowlist: Option<std::collections::HashSet<String>>,
    pub media_refs: Vec<crate::daemon_api::MediaRef>,
    pub vision_plan: crate::media_vision::TurnMediaVisionPlan,
    pub inference_profile_kind: crate::inference_profiles::InferenceProfileKind,
    pub inference_targets: Vec<crate::inference_profiles::InferenceTarget>,
    pub surface: Option<crate::daemon_api::TurnSurfaceContext>,
    pub round_context_provider: Option<Arc<dyn super::turn_context::ToolRoundContextProvider>>,
    pub evidence_undertaking_id: Option<String>,
    pub compact_evidence_receipt_sink:
        Option<Arc<dyn super::coder_evidence::CompactEvidenceReceiptSink>>,
    pub active_turn_checkpoint_sink:
        Option<Arc<dyn super::coder_turn_checkpoint::ActiveTurnCheckpointSink>>,
    pub active_turn_resume: Option<super::coder_turn_checkpoint::ActiveTurnResumeState>,
}

pub struct AssembledLocalTurn {
    pub execution: LocalTurnExecutionParams,
    pub pipeline_selection: SelectedTurnPipeline,
    pub activation: TurnActivationDecision,
    pub prior_build: PriorMessageBuild,
}

pub fn assemble_local_turn(params: AssembleLocalTurnParams<'_>) -> AssembledLocalTurn {
    let configured_tool_call_mode =
        turn_services::parse_tool_call_mode(&params.settings.tool_call_mode);
    let mut turn_loop_settings = TurnLoopSettings::from_runtime_settings(params.settings);
    if params.prepared.agent_mode.id == crate::daemon_api::AgentModeId::Coder {
        turn_loop_settings.configured_max_tool_rounds =
            super::turn_loop_settings::coder_max_tool_rounds(params.max_tool_rounds_override);
    }
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
    let mut activation = turn_services::apply_context_compiler_activation_gate(
        activation,
        params.prepared.compiler_output.allow_no_tools_fallback,
    );
    if params.prepared.agent_mode.id == crate::daemon_api::AgentModeId::Coder {
        activation.turn_class = "coder_foreground";
        activation.enforce_no_tools = false;
        activation.max_tool_rounds = turn_loop_settings.configured_max_tool_rounds;
        activation.reason = "coder_mode_requires_tool_capable_foreground_loop";
    }

    let (hot_window_turns, cold_window_turns, prior_limits) =
        if let Some(limits) = crate::agent_mode_context::context_limits_for_mode(
            params.prepared.agent_mode.id,
        ) {
            (
                limits.hot_window_turns,
                limits.cold_window_turns,
                PriorMessageLimits {
                    max_prior_total_chars: limits.max_prior_total_chars,
                    max_single_prior_message_chars: limits.max_single_prior_message_chars,
                    hot_window_char_budget: limits.hot_window_char_budget,
                    cold_window_char_budget: limits.cold_window_char_budget,
                    cold_summary_line_chars: limits.cold_summary_line_chars,
                    include_auxiliary_history: limits.include_auxiliary_history,
                },
            )
        } else {
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
            (
                hot_window_turns,
                cold_window_turns,
                PriorMessageLimits {
                    max_prior_total_chars: MAX_PRIOR_TOTAL_CHARS,
                    max_single_prior_message_chars: MAX_SINGLE_PRIOR_MESSAGE_CHARS,
                    hot_window_char_budget: HOT_WINDOW_CHAR_BUDGET,
                    cold_window_char_budget: COLD_WINDOW_CHAR_BUDGET,
                    cold_summary_line_chars: COLD_SUMMARY_LINE_CHARS,
                    include_auxiliary_history: true,
                },
            )
        };

    let prior_build = turn_services::build_prior_messages(
        params.tui_rt.tool_catalog.as_ref(),
        params.session_id,
        params.conversation,
        params.prompt,
        params.persist_user_turn,
        hot_window_turns,
        cold_window_turns,
        prior_limits,
    );

    let prompt_for_request = if activation.enforce_no_tools {
        format!(
            "{}\n\n[MEDOUSA_HUD]\ntool_surface=none",
            params.resolved_prompt
        )
    } else {
        append_tool_loop_policy(&params.resolved_prompt, activation.max_tool_rounds)
    };
    let current_turn_user_message = params.vision_plan.build_user_message(&prompt_for_request);

    let effective_tool_registry = params
        .tool_registry_override
        .clone()
        .unwrap_or_else(|| params.tui_rt.tool_registry.clone());
    let pipeline_selection = turn_services::select_pipeline_for_turn_with_registry_and_allowlist(
        effective_tool_registry.clone(),
        params.final_route,
        params.settings,
        params.scheduled_tool_allowlist.clone(),
    );

    let primary_inference_target = params.inference_targets.first();
    let execution_provider = primary_inference_target
        .map(|target| target.provider.clone())
        .unwrap_or_else(|| params.settings.provider.clone());
    let execution_model = primary_inference_target
        .map(|target| target.model.clone())
        .unwrap_or_else(|| params.settings.model.clone());
    let execution_base_url = primary_inference_target
        .and_then(|target| target.base_url.clone())
        .or_else(|| {
            (!params.settings.base_url.trim().is_empty()).then(|| params.settings.base_url.clone())
        });
    let no_tools_pipeline = turn_services::build_prompt_pipeline_for_target(
        &execution_provider,
        &execution_model,
        execution_base_url.as_deref(),
    );

    AssembledLocalTurn {
        execution: LocalTurnExecutionParams {
            agent_mode: params.prepared.agent_mode,
            turn_id: params.turn_id,
            session_id: params.session_id.to_string(),
            backend: params.settings.backend.clone(),
            provider: execution_provider,
            model: execution_model,
            base_url: execution_base_url,
            response_depth_mode: params.response_depth_mode.to_string(),
            reasoning_effort: params.reasoning_effort.to_string(),
            worker_scheduler: params.tui_rt.worker_scheduler.clone(),
            tool_registry: effective_tool_registry,
            client_registry: params.tui_rt.client_registry.clone(),
            identity_memory_store: Some(params.tui_rt.identity_memory_store.clone()
                as Arc<
                    dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore,
                >),
            turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess::default(),
            activation: activation.clone(),
            pipeline: pipeline_selection.pipeline.clone(),
            no_tools_pipeline,
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
            inference_profile_kind: params.inference_profile_kind,
            inference_targets: params.inference_targets,
            supports_ui_artifacts: crate::ui_present_tools::surface_supports_ui_artifacts(
                params.surface.as_ref(),
            ),
            supports_liquid_markdown: params
                .surface
                .as_ref()
                .is_some_and(|surface| surface.supports_liquid_markdown),
            supports_browser_host: crate::browser_tools::surface_supports_browser_host(
                params.surface.as_ref(),
            ),
            round_context_provider: params.round_context_provider.clone(),
            evidence_undertaking_id: params.evidence_undertaking_id.clone(),
            compact_evidence_receipt_sink: params.compact_evidence_receipt_sink.clone(),
            active_turn_checkpoint_sink: params.active_turn_checkpoint_sink.clone(),
            active_turn_resume: params.active_turn_resume.clone(),
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

    let completion = super::execution_context::await_turn_boundary(pipeline.complete_chat_stream(
        ChatRequest::new(messages),
        PromptExecutionContext::default(),
        None,
    ))
    .await
    .ok()?
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

async fn deliver_turn_failure(
    sink: &SharedAgentStreamSink,
    turn_id: u64,
    runtime_error: &str,
    orchestration_state: &mut TurnOrchestrationState,
) {
    let failure = crate::turn_failure::TurnFailure::from_debug(runtime_error);
    orchestration_state.final_mode = "turn_failed".to_string();
    tracing::info!(
        target: "medousa::turn",
        turn_id,
        category = failure.category_label(),
        retryable = failure.retryable,
        final_mode = %orchestration_state.final_mode,
        "turn_failed"
    );
    sink.notice(format!(
        "◈ turn_failed category={} retryable={}",
        failure.category_label(),
        failure.retryable
    ))
    .await;
    sink.agent_error(turn_id, failure.debug_message.clone())
        .await;
}

pub fn retryable_runtime_reason(err_text: &str) -> Option<&'static str> {
    let text = err_text.to_ascii_lowercase();
    if text.contains(super::coder_turn_checkpoint::TOOL_ROUND_BUDGET_EXHAUSTED_REASON)
        || text.contains("tool loop exceeded max rounds")
    {
        return None;
    }
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

fn legacy_retryable_runtime_reason(err_text: &str, is_coder_turn: bool) -> Option<&'static str> {
    if is_coder_turn {
        None
    } else {
        retryable_runtime_reason(err_text)
    }
}

fn mark_active_turn_checkpoint(
    checkpoint: Option<&Arc<dyn super::coder_turn_checkpoint::ActiveTurnCheckpointSink>>,
    status: super::coder_turn_checkpoint::ActiveTurnCheckpointStatus,
    boundary: super::coder_turn_checkpoint::SafeCheckpointBoundary,
    reason: &str,
    orchestration: &TurnOrchestrationState,
) {
    let Some(checkpoint) = checkpoint else {
        return;
    };
    if let Err(err) = checkpoint.mark_status(status, boundary, Some(reason), Some(orchestration)) {
        tracing::warn!(error = %err, ?status, ?boundary, "failed to update Coder checkpoint status");
    }
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

async fn stage_scratch_for_persist(sink: &SharedAgentStreamSink, scratch: &Option<TurnScratchpad>) {
    if let Some(scratch) = scratch.clone()
        && let Ok(value) = serde_json::to_value(scratch)
    {
        sink.stage_persist_scratch(value).await;
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
    super::turn_worker::with_worker_parent_scope(execute_local_turn_inner(sink, params)).await;
}

async fn execute_local_turn_inner(sink: SharedAgentStreamSink, params: LocalTurnExecutionParams) {
    let LocalTurnExecutionParams {
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
        inference_profile_kind,
        inference_targets,
        supports_ui_artifacts,
        supports_liquid_markdown,
        supports_browser_host,
        round_context_provider,
        evidence_undertaking_id,
        compact_evidence_receipt_sink,
        active_turn_checkpoint_sink,
        active_turn_resume,
    } = params;

    let is_coder_turn = agent_mode.id == crate::daemon_api::AgentModeId::Coder;
    let mut pending_active_turn_resume = active_turn_resume;
    let has_active_turn_resume = pending_active_turn_resume.is_some();
    let restores_interrupted_budget = pending_active_turn_resume
        .as_ref()
        .is_some_and(|resume| resume.restore_turn_budget);
    let resume_has_consumed_tool_loop = pending_active_turn_resume
        .as_ref()
        .filter(|resume| resume.restore_turn_budget)
        .and_then(|resume| resume.counters.orchestration.as_ref())
        .is_some_and(|state| state.tool_loop_calls > 0);

    let capability_required =
        if inference_profile_kind == crate::inference_profiles::InferenceProfileKind::Vision {
            crate::inference_router::CapabilityRequirement::Vision
        } else {
            crate::inference_router::CapabilityRequirement::None
        };

    let prompt_ctx =
        crate::reasoning_effort::prompt_execution_context(&model, Some(&reasoning_effort));

    sink.notice(format!(
        "◈ turn_loop_limits {}",
        turn_loop_settings.operator_summary()
    ))
    .await;
    if restores_interrupted_budget && let Some(resume) = pending_active_turn_resume.as_ref() {
        sink.notice(format!(
            "◈ coder_exact_resume source_turn={} rounds={}/{} tool_batches={} retries={}",
            resume.source_daemon_turn_id,
            resume.counters.model_rounds_executed,
            resume.counters.max_tool_rounds,
            resume.counters.tool_batches_completed,
            resume.counters.retry_count,
        ))
        .await;
    }

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
    let host_bus = if agent_mode.id == crate::daemon_api::AgentModeId::Coder {
        false
    } else {
        host_profile.host_bus_active
    };
    let completion_profile = agent_mode.completion_profile;
    sink.notice(host_route_notice(&host_profile)).await;

    let scope_snapshot = super::execution_context::turn_continuation_scope(&turn_scope).await;
    if let Some(bundle) = host_continuity_bundle.as_mut() {
        bundle.parent_turn_correlation_id = scope_snapshot
            .as_ref()
            .map(|scope| scope.turn_correlation_id.clone());
        sink.notice(format!("◈ worker_continuity {}", bundle.log_summary()))
            .await;
    }
    let handoff_continuity_bundle = host_continuity_bundle.clone();
    let host_handoff_slot = Arc::new(tokio::sync::RwLock::new(None));
    let worker_runtime = WorkerRuntimeContext {
        tool_registry: tool_registry.clone(),
        client_registry: client_registry.clone(),
        identity_memory_store: identity_memory_store.clone(),
        provider: provider.clone(),
        model: model.clone(),
        base_url: base_url.clone(),
        turn_scope: turn_scope.clone(),
    };
    let worker_bus = ActiveWorkerBusSession {
        sink: sink.clone(),
        stream_turn_id: turn_id,
        session_id: session_id.clone(),
        identity_user_id: scope_snapshot
            .as_ref()
            .and_then(|scope| scope.identity_user_id.clone()),
        backend: backend.clone(),
        parent_user_prompt: original_prompt.clone(),
        provider: provider.clone(),
        model: model.clone(),
        response_depth_mode: response_depth_mode.clone(),
        parent_turn_correlation_id: scope_snapshot
            .as_ref()
            .map(|scope| scope.turn_correlation_id.clone()),
        parent_runtime_id: worker_scheduler.execution_runtime_id(),
        delivery_target: scope_snapshot
            .as_ref()
            .and_then(|scope| scope.delivery_target.as_ref())
            .map(StoredDeliveryTarget::from),
        host_handoff_slot: host_handoff_slot.clone(),
        host_continuity_bundle,
        configured_max_tool_rounds: turn_loop_settings.configured_max_tool_rounds,
        supports_ui_artifacts,
        supports_liquid_markdown,
        supports_browser_host,
        parent_agent_mode: Some(agent_mode.id.as_str().to_string()),
        parent_code_work_id: evidence_undertaking_id.clone(),
    };
    let _worker_parent_lease = match worker_scheduler.register_parent(worker_runtime, worker_bus) {
        Ok(lease) => lease,
        Err(error) => {
            sink.agent_error(turn_id, format!("worker parent admission failed: {error}"))
                .await;
            return;
        }
    };

    let pipeline = if host_bus {
        pipeline_for_turn_profile(
            tool_registry.clone(),
            &provider,
            &model,
            base_url.as_deref(),
            true,
            Some(session_id.as_str()),
            supports_ui_artifacts,
            supports_browser_host,
            scope_snapshot
                .as_ref()
                .and_then(|scope| scope.channel_surface.as_deref()),
            client_registry.clone(),
        )
    } else {
        default_pipeline
    };

    let turn_budget = turn_budget_for_lane(EngineExecutionLane::Interactive);
    let mut orchestration_state = pending_active_turn_resume
        .as_ref()
        .filter(|resume| resume.restore_turn_budget)
        .and_then(|resume| resume.counters.orchestration.clone())
        .unwrap_or_default();
    if orchestration_state.final_mode.trim().is_empty() {
        orchestration_state.final_mode = "unknown".to_string();
    }

    if has_active_turn_resume {
        // A durable Coder boundary must pass through the checkpoint-aware loop
        // so it can either continue or become terminal. A fresh classifier
        // decision must not divert restored state into the prompt-only lane.
        activation.enforce_no_tools = false;
    } else if should_invoke_intent_classifier(&activation) {
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
                    "◈ intent classifier skipped: no parseable result; using heuristic".to_string(),
                )
                .await;
            }
        } else {
            orchestration_state.final_mode = "classifier_budget_denied".to_string();
        }
    }

    let mut stream_bridge = TurnStreamBridge::new(sink.clone(), turn_id);

    sink.tool_invoked("llm.chat".to_string(), prompt_preview)
        .await;

    if activation.enforce_no_tools {
        let mut messages = Vec::with_capacity(prior_messages.len() + 2);
        messages.push(ChatMessage::system(super::modes::system_prompt_for_mode(
            &agent_mode,
        )));
        messages.extend(prior_messages);
        messages.push(current_turn_user_message.clone());

        if !try_consume_prompt_only_budget(&sink, &mut orchestration_state, &turn_budget).await {
            orchestration_state.final_mode = "prompt_only_budget_denied".to_string();
            mark_active_turn_checkpoint(
                active_turn_checkpoint_sink.as_ref(),
                super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::BudgetExhausted,
                super::coder_turn_checkpoint::SafeCheckpointBoundary::BudgetExhausted,
                "turn budget exhausted before prompt-only execution",
                &orchestration_state,
            );
            stream_bridge.drain().await;
            sink.agent_error(
                turn_id,
                "turn budget exhausted before prompt-only execution".to_string(),
            )
            .await;
            emit_orchestration_summary(&sink, &orchestration_state).await;
            return;
        }
        orchestration_state.final_mode = "prompt_only".to_string();

        sink.notice("◈ fallback_mode=prompt_only retry_count=0 retry_reason=none".to_string())
            .await;

        let prompt_stream = stream_bridge.attempt();
        let prompt_only_result = match super::execution_context::await_turn_boundary(
            no_tools_pipeline.complete_chat_stream(
                ChatRequest::new(messages),
                prompt_ctx.clone(),
                Some(prompt_stream.sender()),
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(error) => Err(stasis::domain::errors::StasisError::PortFailure(format!(
                "{error} during prompt-only model completion"
            ))),
        };
        let prompt_only_result =
            fail_on_stream_overflow(prompt_only_result, prompt_stream.finish().await);
        stream_bridge.drain().await;
        match prompt_only_result {
            Ok(completion) => {
                sink.model_receipt(turn_id, provider.clone(), model.clone())
                    .await;
                let final_text = completion
                    .response
                    .into_first_text()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| {
                        "I do not have enough information to answer confidently without tools for this turn."
                            .to_string()
                    });
                mark_active_turn_checkpoint(
                    active_turn_checkpoint_sink.as_ref(),
                    super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::Completed,
                    super::coder_turn_checkpoint::SafeCheckpointBoundary::Terminal,
                    "prompt_only",
                    &orchestration_state,
                );
                super::turn_delivery::deliver_agent_turn_outcome(
                    &sink,
                    turn_id,
                    final_text,
                    Vec::new(),
                    super::turn_delivery::AgentTurnDeliveryHint {
                        activation_reason: activation.reason,
                        termination_reason: None,
                    },
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            }
            Err(err) => {
                mark_active_turn_checkpoint(
                    active_turn_checkpoint_sink.as_ref(),
                    super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::RecoverableFailure,
                    super::coder_turn_checkpoint::SafeCheckpointBoundary::RecoverableFailure,
                    &err.to_string(),
                    &orchestration_state,
                );
                sink.agent_error(turn_id, err.to_string()).await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            }
        }
        return;
    }

    let request = ToolLoopExecutionRequest {
        user_prompt: prompt_for_request,
        system_prompt: Some(super::modes::system_prompt_for_mode(&agent_mode)),
        context: prompt_ctx.clone(),
        tool_name: String::new(),
        tool_input: Value::Null,
        tool_call_mode: activation.tool_call_mode,
    };
    if !resume_has_consumed_tool_loop
        && !try_consume_tool_loop_budget(&sink, &mut orchestration_state, &turn_budget).await
    {
        orchestration_state.final_mode = "tool_loop_budget_denied".to_string();
        if let Some(checkpoint) = active_turn_checkpoint_sink.as_ref()
            && let Err(err) = checkpoint.mark_status(
                super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::BudgetExhausted,
                super::coder_turn_checkpoint::SafeCheckpointBoundary::BudgetExhausted,
                Some("turn budget exhausted before tool-loop execution"),
                Some(&orchestration_state),
            )
        {
            tracing::warn!(error = %err, "failed to checkpoint denied Coder turn budget");
        }
        stream_bridge.drain().await;
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
        .and_then(|scope| {
            scope
                .delivery_target
                .as_ref()
                .map(|target| target.channel.clone())
        })
        .or_else(|| Some("interactive".to_string()));
    let origin_delivery_target = scope_snapshot
        .as_ref()
        .and_then(|scope| scope.delivery_target.as_ref())
        .map(StoredDeliveryTarget::from);
    let hard_tool_round_ceiling =
        (agent_mode.id == crate::daemon_api::AgentModeId::Coder).then(|| {
            pending_active_turn_resume
                .as_ref()
                .filter(|resume| resume.restore_turn_budget)
                .map(|resume| resume.counters.max_tool_rounds)
                .filter(|rounds| *rounds > 0)
                .unwrap_or(turn_loop_settings.configured_max_tool_rounds)
                .min(super::turn_loop_settings::DEFAULT_CODER_MAX_TOOL_ROUNDS)
        });
    let runtime_ports = medousa_runtime::RuntimePorts::new()
        .with_optional_ledger_sink(super::turn_ledger::session_turn_ledger_sink(
            ledger_session_id.as_deref(),
        ))
        .with_tool_run_events(Arc::new(super::tool_stream::DaemonToolRunEventPort::new(
            sink.clone(),
        )))
        .with_turn_presentation(Arc::new(
            super::turn_presentation::DaemonTurnPresentationPort::new(sink.clone()),
        ))
        .with_budget_approval(Arc::new(
            crate::turn_budget_request::DaemonTurnBudgetApprovalPort::new(
                parent_turn_correlation_id.clone(),
                turn_id,
                ledger_session_id.clone(),
                origin_channel.clone(),
                origin_delivery_target.clone(),
                Some(sink.clone()),
            ),
        ))
        .with_host_handoff(Arc::new(super::turn_context::DaemonHostHandoffPort::new(
            ledger_session_id.clone(),
            turn_id,
            parent_turn_correlation_id.clone(),
            original_prompt.clone(),
            host_handoff_slot.clone(),
            Some(handoff_vibe_signature.clone()),
            Some(handoff_model_avec),
            handoff_continuity_bundle.clone(),
        )));
    let runtime_ports = match evidence_undertaking_id.clone() {
        Some(undertaking_id) => runtime_ports.with_perception_evidence(Arc::new(
            super::perception_governor::DaemonPerceptionEvidencePort::for_coder_undertaking(
                undertaking_id,
                compact_evidence_receipt_sink.clone(),
            ),
        )),
        None => runtime_ports,
    };
    let completion_gate_config = ToolLoopCompletionGateConfig {
        stream_turn_id: turn_id,
        runtime_ports,
        max_text_only_stuck_continues: turn_loop_settings.max_text_only_stuck_continues,
        parent_turn_correlation_id: parent_turn_correlation_id.clone(),
        skip_avec_ritual_check: false,
        hard_tool_round_ceiling,
        require_operator_budget_gate: require_operator_budget_gate(),
        completion_profile,
        cancel_poll_work_id: None,
        steer_poll_work_id: None,
        round_context_provider: round_context_provider.clone(),
    };
    let mut last_tool_scratch: Option<TurnScratchpad> = None;
    let loop_max_rounds = pending_active_turn_resume
        .as_ref()
        .filter(|resume| resume.restore_turn_budget)
        .map(|resume| resume.counters.max_tool_rounds)
        .filter(|rounds| *rounds > 0)
        .unwrap_or(activation.max_tool_rounds)
        .max(1);
    let inference_target_total = inference_targets.len().max(1);
    let mut first_attempt: Option<
        Result<
            stasis::application::orchestration::tool_loop_pipeline::ToolLoopExecutionResponse,
            stasis::domain::errors::StasisError,
        >,
    > = None;
    let mut inference_last_err = String::new();
    let mut visible_output_emitted = false;

    'inference_targets: for (attempt_index, target) in inference_targets.iter().enumerate() {
        if let Some(reason) =
            crate::inference_router::target_ineligibility_reason(target, capability_required)
        {
            sink.notice(crate::inference_router::telemetry_line(
                inference_profile_kind,
                attempt_index,
                inference_target_total,
                target,
                reason,
            ))
            .await;
            continue;
        }

        crate::workshop_env::apply_provider_llm_env(&target.provider);
        if let Some(checkpoint) = active_turn_checkpoint_sink.as_ref()
            && let Err(err) = checkpoint.set_model_route(&target.provider, &target.model)
        {
            tracing::warn!(error = %err, "failed to update Coder checkpoint model route");
        }
        sink.notice(crate::inference_router::telemetry_line(
            inference_profile_kind,
            attempt_index,
            inference_target_total,
            target,
            "attempt",
        ))
        .await;

        let attempt_pipeline = pipeline_for_turn_profile(
            tool_registry.clone(),
            &target.provider,
            &target.model,
            target.base_url.as_deref().or(base_url.as_deref()),
            host_bus,
            Some(session_id.as_str()),
            supports_ui_artifacts,
            supports_browser_host,
            scope_snapshot
                .as_ref()
                .and_then(|scope| scope.channel_surface.as_deref()),
            client_registry.clone(),
        );

        let mut same_target_retries = 0u8;
        loop {
            let initial_worker_scratch =
                scratch_seed_for_tool_loop(&session_scratch_seed, last_tool_scratch.as_ref());
            let mut completion_gate = completion_gate_config.bind(
                &mut orchestration_state,
                &turn_budget,
                &mut last_tool_scratch,
                loop_max_rounds,
                host_tool_round_budget_ceiling(&turn_loop_settings, loop_max_rounds),
                initial_worker_scratch,
                active_turn_checkpoint_sink.clone(),
                pending_active_turn_resume.take(),
            );

            let attempt_stream = stream_bridge.attempt();
            let attempt_result = attempt_pipeline
                .execute_with_stream_prior_messages_max_rounds(
                    request.clone(),
                    prior_messages.clone(),
                    Some(attempt_stream.sender()),
                    loop_max_rounds,
                    Some(&mut completion_gate),
                    Some(current_turn_user_message.clone()),
                )
                .await;
            let attempt_report = attempt_stream.finish().await;
            let attempt_result = fail_on_stream_overflow(attempt_result, attempt_report);
            let attempt_emitted = attempt_report.emitted;
            visible_output_emitted |= attempt_emitted;

            match attempt_result {
                Ok(response) => {
                    sink.model_receipt(turn_id, target.provider.clone(), target.model.clone())
                        .await;
                    first_attempt = Some(Ok(response));
                    break 'inference_targets;
                }
                Err(err) => {
                    let failure = crate::turn_failure::TurnFailure::from_debug(&err.to_string());
                    inference_last_err = failure.debug_message.clone();
                    if attempt_emitted {
                        sink.notice(
                            "◈ retry_policy retry_count=0 retry_reason=visible_output".to_string(),
                        )
                        .await;
                        first_attempt = Some(Err(err));
                        break 'inference_targets;
                    }
                    // Without a safe-boundary checkpoint, a fresh Coder retry
                    // could replay a tool effect whose result never reached the
                    // provider transcript. Stop and recover from Forge/activity
                    // evidence on the next turn instead.
                    if is_coder_turn && active_turn_checkpoint_sink.is_none() {
                        first_attempt = Some(Err(err));
                        break 'inference_targets;
                    }
                    if let Some(checkpoint) = active_turn_checkpoint_sink.as_ref() {
                        match checkpoint.latest_safe_resume() {
                            Ok(resume) => pending_active_turn_resume = resume,
                            Err(recovery_err) => {
                                first_attempt = Some(Err(
                                    stasis::domain::errors::StasisError::PortFailure(recovery_err),
                                ));
                                break 'inference_targets;
                            }
                        }
                    }
                    if crate::inference_router::should_retry_same_target(failure.category)
                        && same_target_retries < 1
                    {
                        same_target_retries += 1;
                        sink.notice(crate::inference_router::telemetry_line(
                            inference_profile_kind,
                            attempt_index,
                            inference_target_total,
                            target,
                            &format!("retry_{}", failure.category_label()),
                        ))
                        .await;
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        continue;
                    }
                    if crate::inference_router::should_advance_fallback(failure.category) {
                        sink.notice(crate::inference_router::telemetry_line(
                            inference_profile_kind,
                            attempt_index,
                            inference_target_total,
                            target,
                            failure.category_label(),
                        ))
                        .await;
                        break;
                    }
                    first_attempt = Some(Err(err));
                    break 'inference_targets;
                }
            }
        }
    }

    let first_attempt = first_attempt;

    match first_attempt {
        Some(Ok(response)) => {
            sink.notice("◈ fallback_mode=tool_loop retry_count=0 retry_reason=none".to_string())
                .await;
            let mut combined_invocations = response.tool_invocations.clone();
            let mut final_text = response.text;
            if response.termination_reason == "worker_spawned" {
                let tool_names = collect_tool_names(&combined_invocations);
                let work_id =
                    crate::agent_runtime::turn_worker_tools::worker_spawn_from_invocations(
                        &combined_invocations,
                    )
                    .map(|(id, _)| id);
                stream_bridge.drain().await;
                stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                sink.agent_worker_ack(turn_id, final_text, tool_names, work_id)
                    .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            if response.termination_reason == "workshop_entered" {
                let tool_names = collect_tool_names(&combined_invocations);
                let work_id = crate::turn_control_tools::workshop_entered_from_invocations(
                    &combined_invocations,
                )
                .map(|(id, _)| id);
                stream_bridge.drain().await;
                stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                sink.agent_workshop_ack(turn_id, final_text, tool_names, work_id)
                    .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            if response.termination_reason == "cognition_turn_checkpoint" {
                let tool_names = collect_tool_names(&combined_invocations);
                stream_bridge.drain().await;
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
                    &sink, turn_id, final_text, tool_names,
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            if response.termination_reason == "cognition_turn_request_input" {
                let tool_names = collect_tool_names(&combined_invocations);
                stream_bridge.drain().await;
                stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                super::turn_delivery::deliver_agent_turn_outcome(
                    &sink,
                    turn_id,
                    final_text,
                    tool_names,
                    super::turn_delivery::AgentTurnDeliveryHint {
                        activation_reason: activation.reason,
                        termination_reason: Some("cognition_turn_request_input"),
                    },
                )
                .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            if matches!(
                response.termination_reason.as_str(),
                "max_rounds_fuse"
                    | "stuck_text_only_continue"
                    | super::coder_turn_checkpoint::TOOL_ROUND_BUDGET_EXHAUSTED_REASON
                    | "workshop_cancelled"
            ) {
                stream_bridge.drain().await;
                stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                let failure = if final_text.trim().is_empty() {
                    format!("Turn stopped: {}", response.termination_reason)
                } else {
                    final_text
                };
                sink.agent_error(turn_id, failure).await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
                return;
            }
            let tool_budget_exhausted = response.termination_reason
                == super::coder_turn_checkpoint::TOOL_ROUND_BUDGET_EXHAUSTED_REASON;
            if !is_coder_turn
                && !tool_budget_exhausted
                && should_run_continuation(&combined_invocations)
                && !crate::channel_delivery::is_principal_interactive_channel(
                    origin_channel
                        .as_deref()
                        .unwrap_or(channel_delivery::CHANNEL_INTERACTIVE),
                )
                && let Some(continuation_prompt) =
                    build_continuation_prompt(&original_prompt, &final_text, &combined_invocations)
            {
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

                sink.tool_invoked("llm.chat".to_string(), "continuation synthesis".to_string())
                    .await;

                let continuation_request = ToolLoopExecutionRequest {
                    user_prompt: continuation_compiled_prompt,
                    system_prompt: Some(super::modes::system_prompt_for_mode(&agent_mode)),
                    context: prompt_ctx.clone(),
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

                    let continuation_stream = stream_bridge.attempt();
                    let continuation_result = {
                        let continuation_max_rounds = activation
                            .max_tool_rounds
                            .min(turn_loop_settings.continuation_max_tool_rounds)
                            .max(1);
                        let initial_worker_scratch = scratch_seed_for_tool_loop(
                            &session_scratch_seed,
                            last_tool_scratch.as_ref(),
                        );
                        let mut continuation_gate = completion_gate_config.bind(
                            &mut orchestration_state,
                            &turn_budget,
                            &mut last_tool_scratch,
                            continuation_max_rounds,
                            host_tool_round_budget_ceiling(
                                &turn_loop_settings,
                                continuation_max_rounds,
                            ),
                            initial_worker_scratch,
                            None,
                            None,
                        );
                        pipeline
                            .execute_with_stream_prior_messages_max_rounds(
                                continuation_request,
                                continuation_prior_messages,
                                Some(continuation_stream.sender()),
                                continuation_max_rounds,
                                Some(&mut continuation_gate),
                                None,
                            )
                            .await
                    };
                    let continuation_result = fail_on_stream_overflow(
                        continuation_result,
                        continuation_stream.finish().await,
                    );

                    match continuation_result {
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
                    sink.notice("◈ continuation synthesis skipped: turn budget limit".to_string())
                        .await;
                }
            }

            let profile = super::presentation::presentation_profile_for_channel(
                origin_channel
                    .as_deref()
                    .unwrap_or(channel_delivery::CHANNEL_INTERACTIVE),
            );
            super::presentation::maybe_append_tools_to_canonical_body(
                &mut final_text,
                &combined_invocations,
                profile,
            );
            let tool_names = collect_tool_names(&combined_invocations);
            stream_bridge.drain().await;
            sink.tool_invoked(
                "llm.chat".to_string(),
                format!("done  {} token(s)", final_text.split_whitespace().count()),
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
                    termination_reason: None,
                },
            )
            .await;
            emit_orchestration_summary(&sink, &orchestration_state).await;
        }
        Some(Err(err)) => {
            let err_text = err.to_string();
            if !visible_output_emitted
                && let Some(reason) = legacy_retryable_runtime_reason(&err_text, is_coder_turn)
            {
                // Retry uses the same tool-round budget as the primary loop unless the
                // operator explicitly set a lower retry_runtime_max_rounds cap.
                let retry_rounds = activation
                    .max_tool_rounds
                    .min(retry_max_rounds.max(activation.max_tool_rounds))
                    .max(1);
                let mut last_err = err_text;
                let mut retry_count = 0usize;
                let mut retry_stopped_after_output = false;
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
                        orchestration_state.final_mode =
                            "tool_loop_retry_budget_denied".to_string();
                        mark_active_turn_checkpoint(
                            active_turn_checkpoint_sink.as_ref(),
                            super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::BudgetExhausted,
                            super::coder_turn_checkpoint::SafeCheckpointBoundary::BudgetExhausted,
                            "turn budget exhausted before runtime retry",
                            &orchestration_state,
                        );
                        stream_bridge.drain().await;
                        sink.agent_error(turn_id, "turn budget exhausted before retry".to_string())
                            .await;
                        emit_orchestration_summary(&sink, &orchestration_state).await;
                        return;
                    }
                    orchestration_state.final_mode = "tool_loop_retry".to_string();

                    let retry_stream = stream_bridge.attempt();
                    let retry_result = {
                        let initial_worker_scratch = scratch_seed_for_tool_loop(
                            &session_scratch_seed,
                            last_tool_scratch.as_ref(),
                        );
                        let mut retry_gate = completion_gate_config.bind(
                            &mut orchestration_state,
                            &turn_budget,
                            &mut last_tool_scratch,
                            retry_rounds,
                            host_tool_round_budget_ceiling(&turn_loop_settings, retry_rounds),
                            initial_worker_scratch,
                            None,
                            None,
                        );
                        pipeline
                            .execute_with_stream_prior_messages_max_rounds(
                                request.clone(),
                                prior_messages.clone(),
                                Some(retry_stream.sender()),
                                retry_rounds,
                                Some(&mut retry_gate),
                                None,
                            )
                            .await
                    };
                    let retry_report = retry_stream.finish().await;
                    let retry_result = fail_on_stream_overflow(retry_result, retry_report);
                    let retry_emitted = retry_report.emitted;

                    match retry_result {
                        Ok(response) => {
                            let tool_names = collect_tool_names(&response.tool_invocations);
                            stream_bridge.drain().await;
                            stage_scratch_for_persist(&sink, &last_tool_scratch).await;
                            super::turn_delivery::deliver_agent_turn_outcome(
                                &sink,
                                turn_id,
                                response.text,
                                tool_names,
                                super::turn_delivery::AgentTurnDeliveryHint {
                                    activation_reason: activation.reason,
                                    termination_reason: None,
                                },
                            )
                            .await;
                            orchestration_state.final_mode = "tool_loop_retry_success".to_string();
                            emit_orchestration_summary(&sink, &orchestration_state).await;
                            return;
                        }
                        Err(retry_err) => {
                            last_err = format!("{}", retry_err);
                            if retry_emitted {
                                retry_stopped_after_output = true;
                                sink.notice(
                                    "◈ retry_policy stopped retry_reason=visible_output"
                                        .to_string(),
                                )
                                .await;
                                break;
                            }
                        }
                    }
                }
                let retry_failure = if retry_stopped_after_output {
                    format!("{reason} (retry stopped after visible output: {last_err})")
                } else {
                    format!("{reason} (retry exhausted: {last_err})")
                };
                orchestration_state.final_mode = if retry_stopped_after_output {
                    "tool_loop_retry_stopped_after_output".to_string()
                } else {
                    "tool_loop_retry_exhausted".to_string()
                };
                mark_active_turn_checkpoint(
                    active_turn_checkpoint_sink.as_ref(),
                    super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::RecoverableFailure,
                    super::coder_turn_checkpoint::SafeCheckpointBoundary::RecoverableFailure,
                    &retry_failure,
                    &orchestration_state,
                );
                stream_bridge.drain().await;
                deliver_turn_failure(&sink, turn_id, &retry_failure, &mut orchestration_state)
                    .await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            } else {
                sink.notice("◈ retry_policy retry_count=0 retry_reason=not_runtime".to_string())
                    .await;
                orchestration_state.final_mode = "tool_loop_error_non_retryable".to_string();
                mark_active_turn_checkpoint(
                    active_turn_checkpoint_sink.as_ref(),
                    super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::RecoverableFailure,
                    super::coder_turn_checkpoint::SafeCheckpointBoundary::RecoverableFailure,
                    &err_text,
                    &orchestration_state,
                );
                stream_bridge.drain().await;
                deliver_turn_failure(&sink, turn_id, &err_text, &mut orchestration_state).await;
                emit_orchestration_summary(&sink, &orchestration_state).await;
            }
        }
        None => {
            orchestration_state.final_mode = "inference_targets_exhausted".to_string();
            let err_text = if inference_last_err.trim().is_empty() {
                "all inference targets failed".to_string()
            } else {
                inference_last_err.clone()
            };
            mark_active_turn_checkpoint(
                active_turn_checkpoint_sink.as_ref(),
                super::coder_turn_checkpoint::ActiveTurnCheckpointStatus::RecoverableFailure,
                super::coder_turn_checkpoint::SafeCheckpointBoundary::RecoverableFailure,
                &err_text,
                &orchestration_state,
            );
            stream_bridge.drain().await;
            deliver_turn_failure(&sink, turn_id, &err_text, &mut orchestration_state).await;
            emit_orchestration_summary(&sink, &orchestration_state).await;
        }
    }

    stream_bridge.drain().await;
}

#[cfg(test)]
mod stream_bridge_tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use medousa_engine::receipt::ArtifactReceiptMeta;

    use super::*;
    use crate::agent_runtime::stream_sink::AgentStreamSink;

    struct OrderedSink {
        events: Mutex<Vec<String>>,
        first_gate: Option<Arc<tokio::sync::Semaphore>>,
        first_blocked: AtomicBool,
    }

    impl Default for OrderedSink {
        fn default() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                first_gate: None,
                first_blocked: AtomicBool::new(false),
            }
        }
    }

    impl OrderedSink {
        fn push(&self, event: String) {
            self.events.lock().expect("events lock").push(event);
        }

        fn blocked_on_first(gate: Arc<tokio::sync::Semaphore>) -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                first_gate: Some(gate),
                first_blocked: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl AgentStreamSink for OrderedSink {
        async fn content_chunk(&self, _turn_id: u64, delta: String) {
            if !self.first_blocked.swap(true, Ordering::Relaxed)
                && let Some(gate) = &self.first_gate
            {
                let _permit = gate.acquire().await.expect("test gate open");
            }
            tokio::task::yield_now().await;
            self.push(delta);
        }

        async fn reasoning_chunk(&self, _turn_id: u64, _delta: String) {}

        async fn agent_response(&self, _turn_id: u64, text: String, _tool_names: Vec<String>) {
            self.push(text);
        }

        async fn agent_error(&self, _turn_id: u64, _message: String) {}

        async fn notice(&self, _message: String) {}

        async fn tool_invoked(&self, _tool_name: String, _input_summary: String) {}

        async fn tool_payload(
            &self,
            _tool_name: String,
            _tool_input: Value,
            _tool_output: Value,
            _input_receipt: Option<ArtifactReceiptMeta>,
            _output_receipt: Option<ArtifactReceiptMeta>,
        ) {
        }
    }

    #[tokio::test]
    async fn drain_orders_all_streamed_deltas_before_terminal_delivery() {
        let concrete = Arc::new(OrderedSink::default());
        let sink: SharedAgentStreamSink = concrete.clone();
        let mut bridge = TurnStreamBridge::new(sink, 7);
        let attempt = bridge.attempt();

        attempt
            .sender()
            .send(StreamDelta::Content("first".to_string()))
            .await
            .expect("first delta");
        attempt
            .sender()
            .send(StreamDelta::Content("second".to_string()))
            .await
            .expect("second delta");
        let report = attempt.finish().await;
        assert_eq!(
            report,
            AttemptStreamReport {
                emitted: true,
                overflowed: false,
            }
        );
        bridge.drain().await;
        concrete
            .agent_response(7, "terminal".to_string(), Vec::new())
            .await;

        assert_eq!(
            *concrete.events.lock().expect("events lock"),
            vec!["first", "second", "terminal"]
        );
    }

    #[tokio::test]
    async fn attempt_bridge_reports_visible_output_before_retry_decision() {
        let concrete = Arc::new(OrderedSink::default());
        let sink: SharedAgentStreamSink = concrete.clone();
        let mut bridge = TurnStreamBridge::new(sink, 8);
        let attempt = bridge.attempt();

        attempt
            .sender()
            .send(StreamDelta::Content("visible".to_string()))
            .await
            .expect("attempt delta");

        assert_eq!(
            attempt.finish().await,
            AttemptStreamReport {
                emitted: true,
                overflowed: false,
            }
        );
        bridge.drain().await;
        assert_eq!(
            *concrete.events.lock().expect("events lock"),
            vec!["visible"]
        );
    }

    #[tokio::test]
    async fn attempt_bridge_allows_pre_output_failure_routing() {
        let concrete = Arc::new(OrderedSink::default());
        let sink: SharedAgentStreamSink = concrete;
        let mut bridge = TurnStreamBridge::new(sink, 9);
        let attempt = bridge.attempt();

        assert_eq!(attempt.finish().await, AttemptStreamReport::default());
        bridge.drain().await;
    }

    #[tokio::test]
    async fn oversized_provider_delta_fails_attempt_without_entering_turn_queue() {
        let concrete = Arc::new(OrderedSink::default());
        let sink: SharedAgentStreamSink = concrete.clone();
        let mut bridge = TurnStreamBridge::new(sink, 10);
        let attempt = bridge.attempt();
        attempt
            .sender()
            .send(StreamDelta::Content(
                "x".repeat(STREAM_BRIDGE_BYTE_CAPACITY + 1),
            ))
            .await
            .expect("provider delta");

        let report = attempt.finish().await;
        assert_eq!(
            report,
            AttemptStreamReport {
                emitted: true,
                overflowed: true,
            }
        );
        assert!(fail_on_stream_overflow(Ok(()), report).is_err());
        bridge.drain().await;
        assert!(concrete.events.lock().expect("events lock").is_empty());
    }

    #[tokio::test]
    async fn saturated_provider_queue_backpressures_without_growing_or_dropping() {
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let concrete = Arc::new(OrderedSink::blocked_on_first(Arc::clone(&gate)));
        let sink: SharedAgentStreamSink = concrete;
        let mut bridge = TurnStreamBridge::new(sink, 11);
        let attempt = bridge.attempt();
        let sender = attempt.sender().clone();
        let mut producer = tokio::spawn(async move {
            for _ in 0..(STREAM_BRIDGE_MESSAGE_CAPACITY + 8) {
                sender
                    .send(StreamDelta::Content("x".to_string()))
                    .await
                    .expect("provider delta");
            }
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut producer)
                .await
                .is_err(),
            "a stalled sink must propagate backpressure to the provider"
        );
        gate.add_permits(1);
        producer.await.unwrap();
        let report = attempt.finish().await;
        assert!(report.emitted);
        assert!(!report.overflowed);
        bridge.drain().await;
    }

    #[tokio::test]
    async fn dropped_stream_bridge_aborts_pump_with_live_sender_clone() {
        let concrete = Arc::new(OrderedSink::default());
        let sink: SharedAgentStreamSink = concrete;
        let bridge = TurnStreamBridge::new(sink, 10);
        let retained_sender = bridge.retained_sender();
        let pump = bridge.pump_abort_handle();

        drop(bridge);
        for _ in 0..10 {
            if pump.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }

        assert!(pump.is_finished());
        drop(retained_sender);
    }

    #[tokio::test]
    async fn dropped_attempt_bridge_aborts_pump_with_live_sender_clone() {
        let concrete = Arc::new(OrderedSink::default());
        let sink: SharedAgentStreamSink = concrete;
        let bridge = TurnStreamBridge::new(sink, 11);
        let attempt = bridge.attempt();
        let retained_sender = attempt.sender().clone();
        let pump = attempt.pump_abort_handle();

        drop(attempt);
        for _ in 0..10 {
            if pump.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }

        assert!(pump.is_finished());
        drop(retained_sender);
    }

    #[test]
    fn typed_tool_budget_exhaustion_is_never_a_runtime_retry() {
        assert_eq!(
            retryable_runtime_reason(
                super::super::coder_turn_checkpoint::TOOL_ROUND_BUDGET_EXHAUSTED_REASON,
            ),
            None
        );
        assert_eq!(
            retryable_runtime_reason("tool loop exceeded max rounds (30) without final response"),
            None
        );
        assert_eq!(
            retryable_runtime_reason("transport temporarily unavailable"),
            Some("transient_runtime")
        );
        assert_eq!(
            legacy_retryable_runtime_reason("transport temporarily unavailable", true),
            None
        );
        assert_eq!(
            legacy_retryable_runtime_reason("transport temporarily unavailable", false),
            Some("transient_runtime")
        );
    }
}
