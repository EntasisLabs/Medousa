use crate::engine_context::{EngineExecutionLane, lane_execution_budget};

use super::stream_sink::SharedAgentStreamSink;

#[derive(Debug, Clone, Default)]
pub struct TurnOrchestrationState {
    pub calls_total: usize,
    pub classifier_calls: usize,
    pub gatekeeper_calls: usize,
    pub tool_loop_calls: usize,
    pub prompt_only_calls: usize,
    pub continuations: usize,
    pub retries: usize,
    pub loop_guard_tripped: bool,
    pub final_mode: String,
}

#[derive(Debug, Clone)]
pub struct TurnBudget {
    pub max_llm_calls_total: usize,
    pub max_tool_loop_calls: usize,
    pub max_prompt_only_calls: usize,
    pub max_classifier_calls: usize,
    pub max_gatekeeper_calls: usize,
    pub max_retries: usize,
    pub max_continuations: usize,
}

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

pub async fn try_consume_classifier_budget(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.classifier_calls >= budget.max_classifier_calls {
        return emit_budget_deny(
            sink,
            state,
            "classifier",
            "max_classifier_calls",
            state.classifier_calls,
            budget.max_classifier_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            sink,
            state,
            "classifier",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.classifier_calls = state.classifier_calls.saturating_add(1);
    true
}

pub async fn try_consume_gatekeeper_budget(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if budget.max_gatekeeper_calls == 0 {
        return false;
    }
    if state.gatekeeper_calls >= budget.max_gatekeeper_calls {
        return emit_budget_deny(
            sink,
            state,
            "gatekeeper",
            "max_gatekeeper_calls",
            state.gatekeeper_calls,
            budget.max_gatekeeper_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            sink,
            state,
            "gatekeeper",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.gatekeeper_calls = state.gatekeeper_calls.saturating_add(1);
    true
}

pub async fn try_consume_prompt_only_budget(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.prompt_only_calls >= budget.max_prompt_only_calls {
        return emit_budget_deny(
            sink,
            state,
            "prompt_only",
            "max_prompt_only_calls",
            state.prompt_only_calls,
            budget.max_prompt_only_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            sink,
            state,
            "prompt_only",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.prompt_only_calls = state.prompt_only_calls.saturating_add(1);
    true
}

pub async fn try_consume_tool_loop_budget(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.tool_loop_calls >= budget.max_tool_loop_calls {
        return emit_budget_deny(
            sink,
            state,
            "tool_loop",
            "max_tool_loop_calls",
            state.tool_loop_calls,
            budget.max_tool_loop_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            sink,
            state,
            "tool_loop",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.tool_loop_calls = state.tool_loop_calls.saturating_add(1);
    true
}

pub async fn try_consume_continuation_budget(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.continuations >= budget.max_continuations {
        return emit_budget_deny(
            sink,
            state,
            "continuation",
            "max_continuations",
            state.continuations,
            budget.max_continuations,
        )
        .await;
    }
    if state.tool_loop_calls >= budget.max_tool_loop_calls {
        return emit_budget_deny(
            sink,
            state,
            "tool_loop",
            "max_tool_loop_calls",
            state.tool_loop_calls,
            budget.max_tool_loop_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            sink,
            state,
            "continuation",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.tool_loop_calls = state.tool_loop_calls.saturating_add(1);
    state.continuations = state.continuations.saturating_add(1);
    true
}

pub async fn try_consume_retry_budget(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    budget: &TurnBudget,
) -> bool {
    if state.retries >= budget.max_retries {
        return emit_budget_deny(
            sink,
            state,
            "retry",
            "max_retries",
            state.retries,
            budget.max_retries,
        )
        .await;
    }
    if state.tool_loop_calls >= budget.max_tool_loop_calls {
        return emit_budget_deny(
            sink,
            state,
            "retry",
            "max_tool_loop_calls",
            state.tool_loop_calls,
            budget.max_tool_loop_calls,
        )
        .await;
    }
    if state.calls_total >= budget.max_llm_calls_total {
        return emit_budget_deny(
            sink,
            state,
            "retry",
            "max_llm_calls_total",
            state.calls_total,
            budget.max_llm_calls_total,
        )
        .await;
    }
    state.calls_total = state.calls_total.saturating_add(1);
    state.tool_loop_calls = state.tool_loop_calls.saturating_add(1);
    state.retries = state.retries.saturating_add(1);
    true
}

pub async fn emit_budget_deny(
    sink: &SharedAgentStreamSink,
    state: &mut TurnOrchestrationState,
    stage: &str,
    reason: &str,
    used: usize,
    limit: usize,
) -> bool {
    state.loop_guard_tripped = true;
    sink.notice(format!(
        "◈ budget_deny stage={stage} reason={reason} used={used} limit={limit}"
    ))
    .await;
    false
}

pub async fn emit_orchestration_summary(
    sink: &SharedAgentStreamSink,
    state: &TurnOrchestrationState,
) {
    sink.notice(format!(
        "◈ orchestration_summary calls_total={} classifier_calls={} tool_loop_calls={} prompt_only_calls={} continuations={} retries={} loop_guard_tripped={} final_mode={}",
        state.calls_total,
        state.classifier_calls,
        state.tool_loop_calls,
        state.prompt_only_calls,
        state.continuations,
        state.retries,
        state.loop_guard_tripped,
        state.final_mode,
    ))
    .await;
}
