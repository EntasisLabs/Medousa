//! Explicit turn completion FSM — text-only model rounds.
//!
//! Completion is structural, never inferred from the wording of model prose:
//! - principal-facing turns preserve and commit two consecutive text-only
//!   responses, whether or not tools have run;
//! - any intervening tool call resets the held response;
//! - cognition_turn_finish ends immediately and appends onto one held response;
//! - synthesis-bound workers retain their explicit-finish boundary after tools.

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

/// Host scheduler: empty model round after tools may continue at most once.
pub const HOST_EMPTY_AFTER_TOOLS_CONTINUE_CAP: usize = 1;

/// Principal-delivery contract for text-only model rounds.
///
/// This must not be inferred from where execution happens: Coder executes in the
/// foreground but still owns a principal-facing reply, while workshop workers
/// produce synthesis-bound results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnCompletionProfile {
    HostScheduler,
    ForegroundPrincipal,
    WorkerSynthesis,
}

impl TurnCompletionProfile {
    pub fn uses_host_scheduler_rules(self) -> bool {
        matches!(self, Self::HostScheduler)
    }

    /// This profile can hold one prose round so a second consecutive text-only
    /// round commits the combined response. Any intervening tool call resets it.
    pub fn holds_first_prose(self) -> bool {
        matches!(self, Self::HostScheduler | Self::ForegroundPrincipal)
    }
}

/// What the tool loop should do after a text-only model response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnRoundAction {
    EndTurn {
        termination_reason: &'static str,
    },
    ContinueLoop {
        reason: ContinueReason,
        control_message: String,
        missing_tools: Vec<String>,
    },
}

impl TurnRoundAction {
    /// Whether this round is eligible to resolve a previously held answer.
    /// An empty recovery round remains inside the loop. Any prose response or
    /// terminal action resolves a held response.
    pub fn resolves_existing_pack_hold(&self) -> bool {
        !matches!(
            self,
            Self::ContinueLoop {
                reason: ContinueReason::EmptyAfterTools,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueReason {
    /// Model returned no text and no tool calls after tools already ran — nudge one more round.
    EmptyAfterTools,
    /// Principal-facing content held for one bounded resolution round.
    PackHold,
}

/// Developer-facing turn-control body for `[MEDOUSA_TURN_CONTROL]`.
pub fn continue_control_message(reason: ContinueReason, _missing_tools: &[String]) -> String {
    match reason {
        ContinueReason::EmptyAfterTools => {
            "Turn continues: last model round had no tool calls and no assistant text. \
             Call the tools you still need in this round, then deliver the complete answer; \
             cognition_turn action=turn.finish is the explicit hard stop and cognition_turn action=turn.checkpoint \
             is for a mid-task handoff. Synthesis-bound workers must use cognition_turn action=turn.finish \
             for direct pass-through."
                .to_string()
        }
        ContinueReason::PackHold => {
            crate::agent_runtime::turn_ledger::pack_hold_resolution_control_message()
        }
    }
}

fn host_empty_after_tools_control_message() -> String {
    "Turn continues: last round had no assistant text after tools. Call cognition_turn action=turn.begin_work for \
     execution work or any host tools you still need, then deliver your answer in prose."
        .to_string()
}

fn continue_loop(reason: ContinueReason, missing_tools: Vec<String>) -> TurnRoundAction {
    TurnRoundAction::ContinueLoop {
        reason,
        control_message: continue_control_message(reason, &missing_tools),
        missing_tools,
    }
}

fn common_terminal_guard(
    draft: &str,
    pending_final_answer: bool,
    rounds_executed: usize,
    max_tool_rounds: usize,
) -> Option<TurnRoundAction> {
    if pending_final_answer && !draft.is_empty() {
        return Some(TurnRoundAction::EndTurn {
            termination_reason: "prepare_final_then_text",
        });
    }

    if rounds_executed >= max_tool_rounds.max(1) {
        return Some(TurnRoundAction::EndTurn {
            termination_reason: "max_rounds_fuse",
        });
    }

    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoToolDebtRoundContext {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    /// Legacy checkpoint field; semantic prose auto-continues are disabled.
    pub interim_continues_used: usize,
    /// Legacy checkpoint field; always zero for new turns.
    pub interim_continue_cap: usize,
    pub completion_profile: TurnCompletionProfile,
}

#[derive(Debug, Clone)]
pub struct AfterToolsRoundContext<'a> {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub invocations: &'a [ToolInvocation],
    pub workshop_lane: bool,
    /// Legacy checkpoint field; semantic prose auto-continues are disabled.
    pub interim_continues_used: usize,
    /// Legacy checkpoint field; always zero for new turns.
    pub interim_continue_cap: usize,
    pub completion_profile: TurnCompletionProfile,
    /// Host scheduler: empty-after-tools continues already spent this turn.
    pub empty_after_tools_continues_used: usize,
}

fn decide_host_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    decide_principal_no_tool_debt_text_round(ctx)
}

fn decide_host_after_tools_text_round(ctx: &AfterToolsRoundContext<'_>) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    if draft.is_empty() {
        if ctx.empty_after_tools_continues_used < HOST_EMPTY_AFTER_TOOLS_CONTINUE_CAP {
            return TurnRoundAction::ContinueLoop {
                reason: ContinueReason::EmptyAfterTools,
                control_message: host_empty_after_tools_control_message(),
                missing_tools: vec![],
            };
        }
        return TurnRoundAction::EndTurn {
            termination_reason: "max_rounds_fuse",
        };
    }

    continue_loop(ContinueReason::PackHold, vec![])
}

/// Decide a text-only round before any tools have run.
pub fn decide_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    match ctx.completion_profile {
        TurnCompletionProfile::HostScheduler => {
            return decide_host_no_tool_debt_text_round(ctx);
        }
        TurnCompletionProfile::ForegroundPrincipal => {
            return decide_foreground_no_tool_debt_text_round(ctx);
        }
        TurnCompletionProfile::WorkerSynthesis => {}
    }

    let draft = ctx.draft_text.trim();

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    TurnRoundAction::EndTurn {
        termination_reason: "no_tools_prose",
    }
}

fn decide_foreground_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    decide_principal_no_tool_debt_text_round(ctx)
}

fn decide_principal_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    continue_loop(ContinueReason::PackHold, vec![])
}

/// Decide a text-only round after tools have run.
pub fn decide_after_tools_text_round(ctx: &AfterToolsRoundContext<'_>) -> TurnRoundAction {
    match ctx.completion_profile {
        TurnCompletionProfile::HostScheduler => {
            return decide_host_after_tools_text_round(ctx);
        }
        TurnCompletionProfile::ForegroundPrincipal => {
            return decide_foreground_after_tools_text_round(ctx);
        }
        TurnCompletionProfile::WorkerSynthesis => {}
    }

    let draft = ctx.draft_text.trim();

    if ctx.workshop_lane && ctx.pending_final_answer && !draft.is_empty() {
        return TurnRoundAction::EndTurn {
            termination_reason: "workshop_lane_prepare_final",
        };
    }

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    if draft.is_empty() {
        return continue_loop(ContinueReason::EmptyAfterTools, vec![]);
    }

    TurnRoundAction::EndTurn {
        termination_reason: "prose_requires_finish",
    }
}

fn decide_foreground_after_tools_text_round(ctx: &AfterToolsRoundContext<'_>) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    if draft.is_empty() {
        return continue_loop(ContinueReason::EmptyAfterTools, vec![]);
    }

    continue_loop(ContinueReason::PackHold, vec![])
}

#[cfg(test)]
mod event_driven_tests {
    use super::*;
    use serde_json::Value;

    fn no_tools(draft: &str, profile: TurnCompletionProfile) -> NoToolDebtRoundContext {
        NoToolDebtRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 1,
            max_tool_rounds: 10,
            interim_continues_used: 0,
            interim_continue_cap: 8,
            completion_profile: profile,
        }
    }

    fn after_tools(draft: &str, profile: TurnCompletionProfile) -> AfterToolsRoundContext<'static> {
        let invocations = Box::leak(Box::new([ToolInvocation {
            tool_name: "cognition_coder_shell_run".to_string(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        }]));
        AfterToolsRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 2,
            max_tool_rounds: 10,
            invocations,
            workshop_lane: false,
            interim_continues_used: 0,
            interim_continue_cap: 8,
            completion_profile: profile,
            empty_after_tools_continues_used: 0,
        }
    }

    #[test]
    fn principal_prose_before_tools_enters_the_same_hold() {
        for draft in [
            "Let me inspect that next.",
            "Here is the complete answer.",
            "Which repository?",
        ] {
            for profile in [
                TurnCompletionProfile::HostScheduler,
                TurnCompletionProfile::ForegroundPrincipal,
            ] {
                assert!(matches!(
                    decide_no_tool_debt_text_round(&no_tools(draft, profile)),
                    TurnRoundAction::ContinueLoop {
                        reason: ContinueReason::PackHold,
                        ..
                    }
                ));
            }
        }
    }

    #[test]
    fn worker_prose_before_tools_still_ends_without_pack_hold() {
        assert!(matches!(
            decide_no_tool_debt_text_round(&no_tools(
                "Here is the worker result.",
                TurnCompletionProfile::WorkerSynthesis,
            )),
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
    }

    #[test]
    fn principal_prose_after_tools_always_enters_the_same_hold() {
        for draft in [
            "Let me inspect one more thing.",
            "Here is the complete answer.",
            "Which repository?",
        ] {
            for profile in [
                TurnCompletionProfile::HostScheduler,
                TurnCompletionProfile::ForegroundPrincipal,
            ] {
                assert!(matches!(
                    decide_after_tools_text_round(&after_tools(draft, profile)),
                    TurnRoundAction::ContinueLoop {
                        reason: ContinueReason::PackHold,
                        ..
                    }
                ));
            }
        }
    }

    #[test]
    fn worker_prose_after_tools_ends_without_semantic_classification() {
        assert!(matches!(
            decide_after_tools_text_round(&after_tools(
                "I'll summarize next.",
                TurnCompletionProfile::WorkerSynthesis,
            )),
            TurnRoundAction::EndTurn {
                termination_reason: "prose_requires_finish"
            }
        ));
    }
}
