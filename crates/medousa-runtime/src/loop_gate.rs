//! Portable foreground-loop configuration and host-port bindings.

use std::sync::Arc;

use medousa_engine::TurnScratchpad;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::budget::{TurnBudget, TurnOrchestrationState};
use crate::checkpoint::{ActiveTurnCheckpointSink, ActiveTurnResumeState};
use crate::completion_fsm::TurnCompletionProfile;
use crate::loop_state::resolve_max_text_only_stuck_continues;
use crate::ports::RuntimePorts;
use crate::turn_context::ToolRoundContextProvider;

pub const DEFAULT_FOREGROUND_MAX_TOOL_ROUNDS: usize = 30;

/// Per-execution state and optional host effects consumed by the foreground
/// tool loop. The gate carries no daemon, transport, or UI implementation.
pub struct ToolLoopCompletionGate<'a> {
    pub stream_turn_id: u64,
    pub runtime_ports: RuntimePorts,
    pub orchestration: Option<&'a mut TurnOrchestrationState>,
    pub budget: Option<&'a TurnBudget>,
    /// Configured model-round budget for this tool-loop execution.
    pub max_tool_rounds: usize,
    /// Consecutive text-only continues without new tools before the turn stops.
    pub max_text_only_stuck_continues: usize,
    /// Latest scratchpad snapshot from the tool loop (for failure explanation / debugging).
    pub scratch_out: Option<&'a mut Option<TurnScratchpad>>,
    pub parent_turn_correlation_id: Option<String>,
    /// Seeds worker tool-loop scratch from host handoff (Tier C).
    pub initial_worker_scratch: Option<TurnScratchpad>,
    /// Workshop lane (research/general worker): skip host memory AVEC ritual receipt checks.
    pub skip_avec_ritual_check: bool,
    /// Hard ceiling for silent tool-round extension (host bus cap).
    pub tool_round_budget_ceiling: usize,
    /// Optional mode-owned ceiling that operator approval cannot exceed.
    pub hard_tool_round_ceiling: Option<usize>,
    /// When true, `cognition_turn_request_more_rounds` pauses for operator approval.
    pub require_operator_budget_gate: bool,
    /// Text-only completion behavior, independent of the execution lane.
    pub completion_profile: TurnCompletionProfile,
    /// Poll turn-worker store each round; end loop when status is cancelled.
    pub cancel_poll_work_id: Option<String>,
    /// Drain steer inbox each round and inject `[MEDOUSA_WORKSHOP_STEER]`.
    pub steer_poll_work_id: Option<String>,
    /// Mode-owned ambient/delta compiler invoked after each tool batch.
    pub round_context_provider: Option<Arc<dyn ToolRoundContextProvider>>,
    /// Coder-only durable checkpoint writer. General/worker lanes leave this absent.
    pub active_turn_checkpoint_sink: Option<Arc<dyn ActiveTurnCheckpointSink>>,
    /// Exact safe-boundary state consumed once when this loop starts.
    pub active_turn_resume: Option<ActiveTurnResumeState>,
}

/// Owned, immutable context shared by primary, continuation, and retry loop
/// gates. Binding mutable budget and scratch references in one place prevents
/// those execution paths from silently drifting as gate fields evolve.
#[derive(Clone)]
pub struct ToolLoopCompletionGateConfig {
    pub stream_turn_id: u64,
    pub runtime_ports: RuntimePorts,
    pub max_text_only_stuck_continues: usize,
    pub parent_turn_correlation_id: Option<String>,
    pub skip_avec_ritual_check: bool,
    pub hard_tool_round_ceiling: Option<usize>,
    pub require_operator_budget_gate: bool,
    pub completion_profile: TurnCompletionProfile,
    pub cancel_poll_work_id: Option<String>,
    pub steer_poll_work_id: Option<String>,
    pub round_context_provider: Option<Arc<dyn ToolRoundContextProvider>>,
}

impl ToolLoopCompletionGateConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn bind<'a>(
        &self,
        orchestration: &'a mut TurnOrchestrationState,
        budget: &'a TurnBudget,
        scratch_out: &'a mut Option<TurnScratchpad>,
        max_tool_rounds: usize,
        tool_round_budget_ceiling: usize,
        initial_worker_scratch: TurnScratchpad,
        active_turn_checkpoint_sink: Option<Arc<dyn ActiveTurnCheckpointSink>>,
        active_turn_resume: Option<ActiveTurnResumeState>,
    ) -> ToolLoopCompletionGate<'a> {
        ToolLoopCompletionGate {
            stream_turn_id: self.stream_turn_id,
            runtime_ports: self.runtime_ports.clone(),
            orchestration: Some(orchestration),
            budget: Some(budget),
            max_tool_rounds,
            max_text_only_stuck_continues: self.max_text_only_stuck_continues,
            scratch_out: Some(scratch_out),
            parent_turn_correlation_id: self.parent_turn_correlation_id.clone(),
            initial_worker_scratch: Some(initial_worker_scratch),
            skip_avec_ritual_check: self.skip_avec_ritual_check,
            tool_round_budget_ceiling,
            hard_tool_round_ceiling: self.hard_tool_round_ceiling,
            require_operator_budget_gate: self.require_operator_budget_gate,
            completion_profile: self.completion_profile,
            cancel_poll_work_id: self.cancel_poll_work_id.clone(),
            steer_poll_work_id: self.steer_poll_work_id.clone(),
            round_context_provider: self.round_context_provider.clone(),
            active_turn_checkpoint_sink,
            active_turn_resume,
        }
    }
}

impl ToolLoopCompletionGate<'_> {
    /// Construct a standalone foreground execution gate from already-composed
    /// runtime ports. Host-specific adapters are intentionally composed by the
    /// caller.
    pub fn new_for_execution(
        stream_turn_id: u64,
        runtime_ports: RuntimePorts,
        max_tool_rounds: usize,
    ) -> Self {
        let max_tool_rounds = max_tool_rounds.max(1);
        Self {
            stream_turn_id,
            runtime_ports,
            orchestration: None,
            budget: None,
            max_tool_rounds,
            max_text_only_stuck_continues: resolve_max_text_only_stuck_continues(max_tool_rounds),
            scratch_out: None,
            parent_turn_correlation_id: None,
            initial_worker_scratch: None,
            skip_avec_ritual_check: false,
            tool_round_budget_ceiling: max_tool_rounds,
            hard_tool_round_ceiling: None,
            require_operator_budget_gate: false,
            completion_profile: TurnCompletionProfile::ForegroundPrincipal,
            cancel_poll_work_id: None,
            steer_poll_work_id: None,
            round_context_provider: None,
            active_turn_checkpoint_sink: None,
            active_turn_resume: None,
        }
    }
}

pub fn collect_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    invocations
        .iter()
        .map(|invocation| invocation.tool_name.clone())
        .collect()
}
