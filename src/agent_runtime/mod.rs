//! Shared agent turn orchestration extracted from the TUI local fallback.
//!
//! Phase 1: turn services + runtime type scaffold.
//! Phase 2+: daemon-hosted turn loop and channel-agnostic streaming.

/// Version label exposed in daemon `/health` and doctor diagnostics.
pub const AGENT_RUNTIME_VERSION: &str = "centralized-v1";

pub mod daemon_interactive_turn;
pub mod heartbeat_turn;
pub mod continuation;
pub mod prompt_prep;
pub mod runtime;
pub mod settings;
pub mod stream_sink;
pub mod system_prompt;
pub mod turn_budget;
pub mod turn_orchestrator;
pub mod turn_services;
pub mod types;

pub use continuation::{
    build_continuation_prior_messages, build_continuation_prompt, collect_tool_names,
    should_run_continuation,
};
pub use prompt_prep::{
    CheapRecallProbe, ContextPackQuality, IdentityContextProbe, RecallSnippet,
    append_identity_context_hint, append_memory_recall_hint, cheap_memory_recall_probe,
    compile_interactive_context_prompt, derive_recall_readiness, identity_context_probe,
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
pub use runtime::{MedousaAgentRuntime, build_agent_runtime, build_daemon_agent_runtime};
pub use settings::{default_daemon_runtime_settings, runtime_settings_for_interactive_turn};
pub use stream_sink::{AgentStreamSink, SharedAgentStreamSink};
pub use system_prompt::DEFAULT_SYSTEM_PROMPT;
pub use turn_budget::{
    TurnBudget, TurnOrchestrationState, emit_budget_deny, emit_orchestration_summary,
    try_consume_classifier_budget, try_consume_continuation_budget, try_consume_prompt_only_budget,
    try_consume_retry_budget, try_consume_tool_loop_budget, turn_budget_for_lane,
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
