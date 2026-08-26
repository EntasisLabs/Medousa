//! Structural turn completion FSM.
//!
//! A turn starts in Direct. Prose with no action is delivered and ends that
//! turn. The first nonterminal action moves the turn into ActiveWork; from then
//! on prose is a chronological response segment, not an implicit terminal.
//! ActiveWork ends only through a typed terminal outcome or a runtime fuse.

/// Retained for checkpoint compatibility. ActiveWork now uses the normal round
/// fuse rather than a special empty-response allowance.
pub const HOST_EMPTY_AFTER_TOOLS_CONTINUE_CAP: usize = 1;

/// Delivery profiles remain part of the public runtime contract, but they no
/// longer change the Direct/ActiveWork completion physics.
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
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueReason {
    EmptyAfterTools,
    ActiveWork,
}

pub fn continue_control_message(reason: ContinueReason, _missing_tools: &[String]) -> String {
    match reason {
        ContinueReason::EmptyAfterTools => "Turn remains active. The last model round had no assistant text or action. Continue the work, or end with one typed outcome: turn.finish, turn.request_input, or turn.checkpoint.".to_string(),
        ContinueReason::ActiveWork => "Turn remains active. Assistant prose was already delivered and persisted in chronological order. Continue the work, or end with one typed outcome: turn.finish, turn.request_input, or turn.checkpoint.".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoToolDebtRoundContext {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub interim_continues_used: usize,
    pub interim_continue_cap: usize,
    pub completion_profile: TurnCompletionProfile,
}

#[derive(Debug, Clone)]
pub struct AfterToolsRoundContext {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub workshop_lane: bool,
    pub interim_continues_used: usize,
    pub interim_continue_cap: usize,
    pub completion_profile: TurnCompletionProfile,
    pub empty_after_tools_continues_used: usize,
}

/// Direct prose is the answer. Its wording never changes this decision.
pub fn decide_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    let _compat = (
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
        ctx.interim_continues_used,
        ctx.interim_continue_cap,
        ctx.completion_profile,
    );
    TurnRoundAction::EndTurn {
        termination_reason: "direct_prose",
    }
}

/// Once any nonterminal action has occurred, prose is committed but cannot
/// implicitly close the turn.
pub fn decide_after_tools_text_round(ctx: &AfterToolsRoundContext) -> TurnRoundAction {
    let _compat = (
        ctx.pending_final_answer,
        ctx.workshop_lane,
        ctx.interim_continues_used,
        ctx.interim_continue_cap,
        ctx.completion_profile,
        ctx.empty_after_tools_continues_used,
    );
    if ctx.rounds_executed >= ctx.max_tool_rounds.max(1) {
        return TurnRoundAction::EndTurn {
            termination_reason: "max_rounds_fuse",
        };
    }
    let reason = if ctx.draft_text.trim().is_empty() {
        ContinueReason::EmptyAfterTools
    } else {
        ContinueReason::ActiveWork
    };
    TurnRoundAction::ContinueLoop {
        reason,
        control_message: continue_control_message(reason, &[]),
        missing_tools: vec![],
    }
}

#[cfg(test)]
mod event_driven_tests {
    use super::*;

    fn direct(draft: &str, profile: TurnCompletionProfile) -> NoToolDebtRoundContext {
        NoToolDebtRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 1,
            max_tool_rounds: 10,
            interim_continues_used: 0,
            interim_continue_cap: 0,
            completion_profile: profile,
        }
    }

    fn active(draft: &str, round: usize, max: usize) -> AfterToolsRoundContext {
        AfterToolsRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: round,
            max_tool_rounds: max,
            workshop_lane: false,
            interim_continues_used: 0,
            interim_continue_cap: 0,
            completion_profile: TurnCompletionProfile::HostScheduler,
            empty_after_tools_continues_used: 0,
        }
    }

    #[test]
    fn direct_prose_ends_for_every_profile_and_wording() {
        for text in ["Let me inspect that.", "Here is the answer.", "Which repo?"] {
            for profile in [
                TurnCompletionProfile::HostScheduler,
                TurnCompletionProfile::ForegroundPrincipal,
                TurnCompletionProfile::WorkerSynthesis,
            ] {
                assert_eq!(
                    decide_no_tool_debt_text_round(&direct(text, profile)),
                    TurnRoundAction::EndTurn {
                        termination_reason: "direct_prose"
                    }
                );
            }
        }
    }

    #[test]
    fn active_work_prose_and_empty_rounds_continue() {
        assert!(matches!(
            decide_after_tools_text_round(&active("progress", 2, 10)),
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ActiveWork,
                ..
            }
        ));
        assert!(matches!(
            decide_after_tools_text_round(&active("", 2, 10)),
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::EmptyAfterTools,
                ..
            }
        ));
    }

    #[test]
    fn active_work_uses_the_round_fuse() {
        assert_eq!(
            decide_after_tools_text_round(&active("partial result", 10, 10)),
            TurnRoundAction::EndTurn {
                termination_reason: "max_rounds_fuse"
            }
        );
    }
}
