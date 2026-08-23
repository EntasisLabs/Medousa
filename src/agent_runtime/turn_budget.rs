//! Daemon budget composition over the portable turn-budget kernel.

use crate::engine_context::{EngineExecutionLane, lane_execution_budget};

pub use medousa_runtime::budget::*;

pub fn turn_budget_for_lane(lane: EngineExecutionLane) -> TurnBudget {
    let lane_budget = lane_execution_budget(lane);
    TurnBudget {
        max_llm_calls_total: lane_budget.max_llm_calls_total,
        max_tool_loop_calls: lane_budget.max_tool_loop_calls,
        max_prompt_only_calls: lane_budget.max_prompt_only_calls,
        max_classifier_calls: lane_budget.max_classifier_calls,
        max_gatekeeper_calls: lane_budget.max_gatekeeper_calls,
        max_retries: lane_budget.max_retries,
        max_continuations: lane_budget.max_continuations,
    }
}
