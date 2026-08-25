//! Portable foreground-turn runtime shared by Medousa daemon deployments.
//!
//! This crate owns Medousa's completion semantics and, as extraction proceeds,
//! the production tool loop. Host capabilities enter through explicit ports;
//! transport, process, vault, and delivery infrastructure stay outside. Stasis
//! remains authoritative for deployed-node capabilities and work admission.

pub mod budget;
pub mod checkpoint;
pub mod completion_fsm;
pub mod credentialed_ai;
pub mod execution_boundary;
pub mod execution_policy;
pub mod loop_gate;
pub mod loop_state;
pub mod perception;
pub mod ports;
pub mod tool_loop;
pub mod turn_context;
pub mod turn_control;
pub mod turn_policy;
pub mod turn_presentation;

/// Maximum user prompt admitted to one foreground production turn.
pub const MAX_REQUEST_PROMPT_CHARS: usize = 48_000;

#[cfg(test)]
mod golden_turn;
#[cfg(test)]
mod target_chronological_contract;

pub use budget::{TurnBudget, TurnOrchestrationState};
pub use checkpoint::{
    ActiveTurnCheckpointSink, ActiveTurnCheckpointStatus, ActiveTurnCounters,
    ActiveTurnResumeState, ActiveTurnTranscript, CheckpointToolInvocation, OutstandingTurnBoundary,
    SafeCheckpointBoundary, TOOL_ROUND_BUDGET_EXHAUSTED_REASON, ToolLoopCheckpointState,
};
pub use completion_fsm::{
    AfterToolsRoundContext, ContinueReason, HOST_EMPTY_AFTER_TOOLS_CONTINUE_CAP,
    NoToolDebtRoundContext, TurnCompletionProfile, TurnRoundAction, continue_control_message,
    decide_after_tools_text_round, decide_no_tool_debt_text_round,
};
pub use credentialed_ai::{
    CredentialProvider, CredentialedAiChatBuildError, CredentialedAiChatClient,
    CredentialedAiChatConfig, CredentialedAiChatConfigError, ProviderCredential,
    ProviderCredentialError, genai_model_target, resolve_genai_adapter_kind,
};
pub use execution_boundary::{
    TurnExecutionBoundary, TurnExecutionBoundaryError, active_turn_execution_boundary,
    await_turn_boundary, missing_turn_execution_boundary_invocations, with_turn_execution_boundary,
};
pub use loop_gate::{
    DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS, ToolLoopCompletionGate, ToolLoopCompletionGateConfig,
    collect_tool_names,
};
pub use ports::{
    DelegationControlPort, HostHandoffPort, PendingTurnBudgetApproval, PerceptionEvidencePort,
    PerceptionEvidenceRequest, PersistedPerceptionEvidence, RuntimePortFuture, RuntimePorts,
    ToolRunEventPort, ToolRunFinish, ToolRunStart, TurnBudgetApprovalPort,
    TurnBudgetApprovalRequest, TurnBudgetApprovalResolution, TurnLedgerSink, TurnPresentationPort,
    TurnSteerMessage,
};
pub use tool_loop::MedousaToolLoopPipeline;
pub use turn_context::{HostTurnContext, ToolLaneState, ToolRoundContextProvider};
pub use turn_presentation::append_voice_preset_hint;
