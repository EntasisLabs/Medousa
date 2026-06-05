//! Shared agent turn orchestration extracted from the TUI local fallback.
//!
//! Phase 1: turn services + runtime type scaffold.
//! Phase 2+: daemon-hosted turn loop and channel-agnostic streaming.

/// Version label exposed in daemon `/health` and doctor diagnostics.
pub const AGENT_RUNTIME_VERSION: &str = "centralized-v1";

pub mod ambient_context;
pub mod daemon_interactive_turn;
pub mod turn_delivery;
pub mod heartbeat_turn;
pub mod continuation;
pub mod prompt_prep;
pub mod runtime;
pub mod settings;
pub mod stream_sink;
pub mod system_prompt;
pub mod turn_budget;
pub mod turn_completion;
pub mod turn_context;
pub mod turn_ledger;
pub mod turn_loop_settings;
pub mod turn_orchestrator;
pub mod turn_worker;
pub mod turn_worker_tools;
pub mod turn_services;
pub mod types;
pub mod vibe_signature;

pub use ambient_context::{
    append_ambient_context, build_ambient_context, operator_zoned_now, resolve_operator_timezone,
    resolve_operator_timezone_label, AmbientContextBlock, AmbientContextInput,
    ChannelAmbientPolicy,
};
pub use turn_delivery::{
    classify_agent_turn_delivery, deliver_agent_turn_outcome, AgentTurnDeliveryHint,
    AgentTurnDeliveryKind,
};
pub use continuation::{
    build_continuation_prior_messages, build_continuation_prompt, collect_tool_names,
    should_run_continuation,
};
pub use prompt_prep::{
    CheapRecallProbe, ContextPackQuality, IdentityContextProbe, RecallSnippet,
    append_identity_context_hint, append_memory_recall_hint, channel_policy_probe,
    cheap_memory_recall_probe, compile_interactive_context_prompt, derive_recall_readiness,
    identity_context_probe,
    resolve_prompt_with_context_pack, truncate_text_for_budget, verifier_policy_from_settings_and_route,
    MAX_REQUEST_PROMPT_CHARS,
};
pub use heartbeat_turn::{
    HeartbeatRuntimeSnapshot, build_heartbeat_turn_prompt, heartbeat_agent_turn_enabled,
    heartbeat_policy_doc_path, load_heartbeat_policy_doc, run_heartbeat_agent_turn,
};
pub use daemon_interactive_turn::{
    InteractiveTurnDeliveryContext, run_agent_turn, run_daemon_interactive_turn,
};
pub use runtime::{
    MedousaAgentRuntime, build_agent_runtime, build_daemon_agent_runtime,
    build_daemon_agent_runtime_from_composition,
};
pub use settings::{default_daemon_runtime_settings, runtime_settings_for_interactive_turn};
pub use stream_sink::{AgentStreamSink, SharedAgentStreamSink};
pub use system_prompt::DEFAULT_SYSTEM_PROMPT;
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
pub use turn_loop_settings::{
    TurnLoopSettings, apply_turn_loop_field_defaults, default_host_turn_bus_mode_label,
    parse_host_turn_bus_mode,
    DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS,
    DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS, DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS,
    DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS, DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS,
    DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES, RETRY_LIMIT_MAX, RETRY_LIMIT_MIN, ROUND_LIMIT_MAX,
    ROUND_LIMIT_MIN,
};
pub use turn_context::{
    HostTurnContext, ToolLaneState, TurnScratchPhase, TurnScratchpad, WorkerHandoffCapsule,
    publish_host_handoff_snapshot, push_turn_scratch_message,
    push_turn_scratch_message_with_budget,
    scratch_digest_hash, tool_output_ok, tool_results_from_invocations, SCRATCH_PREFIX,
    WORKER_HANDOFF_PREFIX,
};
pub use vibe_signature::{default_handoff_model_avec, derive_vibe_signature};
pub use turn_ledger::{
    TurnLedgerEventKind, TurnLedgerRecord, TurnLoopAwareness, TurnLoopDiscipline,
    MAX_TEXT_ONLY_STUCK_CONTINUES, USER_RESPONSE_PREVIEW_MAX_CHARS, append_tool_loop_policy,
    append_turn_ledger_record, developer_message_for_gatekeeper_continue,
    developer_message_for_heuristic_interim_continue, persist_ledger_record,
    push_turn_control_message, resolve_max_text_only_stuck_continues, stuck_turn_user_message,
};
pub use turn_worker::{
    HostTurnProfile, HostTurnRoute, classify_host_turn_route_heuristic, host_bus_env_mode,
    resolve_host_turn_profile,
};
pub use turn_orchestrator::{
    IntentClassification, LocalTurnExecutionParams, PreparedTurnPrompt, PrepareTurnPromptParams,
    AssembleLocalTurnParams, AssembledLocalTurn,
    apply_intent_classifier_override, assemble_local_turn, classify_turn_intent_with_model,
    execute_local_turn, prepare_turn_prompt, retryable_runtime_reason,
    should_invoke_intent_classifier,
    DEFAULT_ACTIVATION_DIRECT_PROMPT_CHARS, DEFAULT_ACTIVATION_LONG_SESSION_PROMPT_CHARS,
    DEFAULT_ACTIVATION_LONG_SESSION_TURN_THRESHOLD, DEFAULT_COLD_WINDOW_TURNS,
    DEFAULT_HOT_WINDOW_TURNS, DEFAULT_RETRY_RUNTIME_MAX_RETRIES, DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
    MAX_COLD_WINDOW_TURNS, MAX_HOT_WINDOW_TURNS, MAX_PRIOR_TOTAL_CHARS,
    MAX_SINGLE_PRIOR_MESSAGE_CHARS, MIN_COLD_WINDOW_TURNS, MIN_HOT_WINDOW_TURNS,
    COLD_SUMMARY_LINE_CHARS, COLD_WINDOW_CHAR_BUDGET, HOT_WINDOW_CHAR_BUDGET,
};
pub use types::{AgentStreamEvent, AgentTurnRequest};
