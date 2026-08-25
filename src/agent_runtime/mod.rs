//! Shared agent turn orchestration extracted from the TUI local fallback.
//!
//! Phase 1: turn services + runtime type scaffold.
//! Phase 2+: daemon-hosted turn loop and channel-agnostic streaming.

/// Version label exposed in daemon `/health` and doctor diagnostics.
pub use crate::daemon_runtime::AGENT_RUNTIME_VERSION;

pub mod active_stream_sink;
pub mod ambient_context;
pub mod daemon_interactive_turn;
pub mod execution_context;
pub(crate) mod provider_stream;
#[cfg(test)]
mod spine_byte_parity;
/// Re-export shim so in-tree `super::stream_sink` paths keep working.
pub mod stream_sink {
    pub use medousa_engine::stream_sink::*;
}
/// Re-export shim so in-tree `super::turn_event` paths keep working.
pub mod turn_event {
    pub use medousa_engine::turn_event::*;
}
/// Re-export shim so in-tree `super::turn_event_log` paths keep working.
pub mod turn_event_log {
    pub use medousa_engine::turn_event_log::*;
}
/// Re-export shim so in-tree `super::engine` paths keep working.
pub mod engine {
    pub use medousa_engine::engine::*;
}
pub mod coder_activity;
pub mod coder_causal;
pub mod coder_claims;
pub mod coder_evidence;
pub mod coder_experiments;
pub mod coder_memory;
pub mod coder_mode;
pub mod coder_pointers;
pub mod coder_semantic_actions;
pub mod coder_setup_tools;
pub mod coder_tools;
pub mod coder_turn_checkpoint;
pub mod context_usage;
pub mod continuation;
pub mod heartbeat_turn;
pub mod host_context;
pub mod modes;
pub mod perception_governor;
pub mod presentation;
pub mod prompt_prep;
#[cfg(test)]
mod prompt_footprint_baseline;
pub mod runtime;
pub mod settings;
pub mod sttp;
pub mod system_prompt;
pub mod tool_stream;
pub mod turn_budget;
pub mod turn_completion;
pub mod turn_completion_fsm;
pub mod turn_context;
pub mod turn_delivery;
pub mod turn_ledger;
pub mod turn_loop_settings;
pub mod turn_orchestrator;
pub mod turn_presentation;
pub mod turn_services;
pub mod turn_worker;
pub mod turn_worker_job;
pub mod turn_worker_tools;
pub mod types;
pub mod vibe_signature;
pub mod worker_continuity;

pub use ambient_context::{
    AmbientContextBlock, AmbientContextInput, ChannelAmbientPolicy, append_ambient_context,
    build_ambient_context, operator_zoned_now, resolve_operator_timezone,
    resolve_operator_timezone_label,
};
pub use coder_activity::{
    CoderActivityStore, CoderAgentIdentity, CoderEngineeringDelta, CoderSharedSpaceSnapshot,
    coder_activity_store, engineering_delta_prompt_appendix, shared_space_prompt_appendix,
};
pub use coder_claims::{CoderClaimMode, CoderClaimScope};
pub use coder_mode::{CoderEntryContext, CoderEntryError, compile_coder_entry};
pub use coder_pointers::{CoderEngineeringPointer, CoderPointerDetail, CoderPointerKind};
pub use coder_tools::{CoderBoundToolRegistry, CoderTurnLease};
pub use coder_turn_checkpoint::{
    ActiveTurnCheckpoint, ActiveTurnCheckpointSink, ActiveTurnCheckpointStatus,
    ActiveTurnResumeState, CoderRecoveryPlan, CoderTurnCheckpointController,
    SafeCheckpointBoundary, TOOL_ROUND_BUDGET_EXHAUSTED_REASON, coder_turn_checkpoint_store,
    plan_coder_recovery,
};
pub use continuation::{
    build_continuation_prior_messages, build_continuation_prompt, collect_tool_names,
    should_run_continuation,
};
pub use daemon_interactive_turn::{
    InteractiveTurnDeliveryContext, InteractiveTurnSessionHooks, run_agent_turn,
    run_daemon_interactive_turn,
};
pub use heartbeat_turn::{
    HeartbeatRuntimeSnapshot, build_heartbeat_turn_prompt, heartbeat_agent_turn_enabled,
    heartbeat_policy_doc_path, load_heartbeat_policy_doc, run_heartbeat_agent_turn,
};
pub use medousa_engine::{
    EngineTurnHandle, Principal, PrincipalKind, RecoveredTurn, SequencedTurnEvent, TurnEnvelope,
    TurnEvent, TurnEventLog, TurnRunOutcome, TurnSurface, project_turn_to_history,
    recover_uncommitted,
};
pub use modes::{
    AgentModeUnavailable, CoderRuntimePhase, ModeExecutionLane, ResolvedAgentMode,
    list_agent_modes, resolve_agent_mode, system_prompt_for_mode,
};
pub use presentation::{
    ChannelToolsFooter, PresentationProfile, format_channel_delivery_text,
    format_tools_footer_markdown, format_tools_footer_markdown_from_invocations,
    format_tools_footer_plain, maybe_append_tools_to_canonical_body,
    presentation_profile_for_channel, presentation_profile_for_surface, unique_tool_names,
};
pub use prompt_prep::{
    CheapRecallProbe, ContextPackQuality, IdentityContextProbe, MAX_REQUEST_PROMPT_CHARS,
    RecallSnippet, append_identity_context_hint, append_manuscript_hint, append_memory_recall_hint,
    append_suggested_capabilities_hint, channel_policy_probe, cheap_memory_recall_probe,
    compile_interactive_context_prompt, derive_recall_readiness, identity_context_probe,
    resolve_prompt_with_context_pack, truncate_text_for_budget,
    verifier_policy_from_settings_and_route,
};
pub use runtime::{
    MedousaAgentRuntime, build_agent_runtime, build_daemon_agent_runtime,
    build_daemon_agent_runtime_from_composition,
};
pub use settings::{default_daemon_runtime_settings, runtime_settings_for_interactive_turn};
pub use stream_sink::{AgentStreamSink, SharedAgentStreamSink};
pub use sttp::{SttpValidationError, validate_canonical_sttp_node};
pub use system_prompt::{DEFAULT_SYSTEM_PROMPT, LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT};
pub use turn_budget::{
    TurnBudget, TurnOrchestrationState, emit_budget_deny, emit_orchestration_summary,
    try_consume_classifier_budget, try_consume_continuation_budget, try_consume_gatekeeper_budget,
    try_consume_prompt_only_budget, try_consume_retry_budget, try_consume_tool_loop_budget,
    turn_budget_for_lane,
};
pub use turn_completion::{
    ToolLoopCompletionGate, TurnCompletionDecision, build_turn_completion_docket,
    resolve_turn_completion,
};
pub use turn_context::{
    HostTurnContext, SCRATCH_PREFIX, ToolLaneState, ToolRoundContextProvider, TurnScratchPhase,
    TurnScratchpad, WORKER_HANDOFF_PREFIX, WorkerHandoffCapsule, publish_host_handoff_snapshot,
    push_turn_scratch_message, push_turn_scratch_message_with_budget, scratch_digest_hash,
    tool_output_ok, tool_results_from_invocations,
};
pub use turn_delivery::{
    AgentTurnDeliveryHint, AgentTurnDeliveryKind, classify_agent_turn_delivery,
    deliver_agent_turn_checkpoint, deliver_agent_turn_outcome,
};
pub use turn_ledger::{
    MAX_TEXT_ONLY_STUCK_CONTINUES, TurnLedgerEventKind, TurnLedgerRecord, TurnLoopAwareness,
    TurnLoopDiscipline, USER_RESPONSE_PREVIEW_MAX_CHARS, append_tool_loop_policy,
    append_turn_ledger_record, persist_ledger_record, push_turn_control_message,
    resolve_max_text_only_stuck_continues, stuck_turn_user_message,
};
pub use turn_loop_settings::{
    DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS, DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS,
    DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS, DEFAULT_CODER_MAX_TOOL_ROUNDS,
    DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS, DEFAULT_GENERAL_MAX_TOOL_ROUNDS,
    DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS, DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES, RETRY_LIMIT_MAX,
    RETRY_LIMIT_MIN, ROUND_LIMIT_MAX, ROUND_LIMIT_MIN, TurnLoopSettings,
    apply_turn_loop_field_defaults, default_host_turn_bus_mode_label, parse_host_turn_bus_mode,
};
pub use turn_orchestrator::{
    AssembleLocalTurnParams, AssembledLocalTurn, COLD_SUMMARY_LINE_CHARS, COLD_WINDOW_CHAR_BUDGET,
    DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS, DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS,
    DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD, DEFAULT_COLD_WINDOW_TURNS,
    DEFAULT_HOT_WINDOW_TURNS, DEFAULT_RETRY_RUNTIME_MAX_RETRIES, DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
    HOT_WINDOW_CHAR_BUDGET, IntentClassification, LocalTurnExecutionParams, MAX_COLD_WINDOW_TURNS,
    MAX_HOT_WINDOW_TURNS, MAX_PRIOR_TOTAL_CHARS, MAX_SINGLE_PRIOR_MESSAGE_CHARS,
    MIN_COLD_WINDOW_TURNS, MIN_HOT_WINDOW_TURNS, PrepareTurnPromptParams, PreparedTurnPrompt,
    apply_intent_classifier_override, assemble_local_turn, classify_turn_intent_with_model,
    execute_local_turn, prepare_turn_prompt, retryable_runtime_reason,
    should_invoke_intent_classifier,
};
pub use turn_worker::{
    HostTurnProfile, HostTurnRoute, classify_host_turn_route_heuristic, host_bus_env_mode,
    resolve_host_turn_profile,
};
pub use types::{AgentStreamEvent, AgentTurnRequest};
pub use vibe_signature::{default_handoff_model_avec, derive_vibe_signature};
