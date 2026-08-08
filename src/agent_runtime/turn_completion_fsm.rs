//! Explicit turn completion FSM — text-only model rounds.
//!
//! Completion is independent of the execution lane:
//! - host scheduling uses a one-round content-pack hold for ambiguous prose;
//! - principal-facing foreground work commits two consecutive prose rounds as one answer;
//! - synthesis-bound workers require an explicit finish after tools.

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::turn_text_heuristics::{
    EXTENDED_PROSE_CHAR_THRESHOLD, is_extended_prose, looks_like_clarifying_question,
    looks_like_interim_status, looks_like_planning_prose, looks_like_substantive_final_answer,
};

/// A non-tool draft at or below this many characters is treated as a brief
/// interim note (alongside `looks_like_interim_status`), eligible for a bounded
/// auto-continue instead of ending the turn.
pub const INTERIM_MAX_CHARS: usize = EXTENDED_PROSE_CHAR_THRESHOLD;

/// Per-turn budget for bounded interim auto-continues (short non-tool notes).
/// Scales with operator `max_tool_rounds` so canvas/workshop flows can emit several
/// `cognition_turn_update_user`-style status beats without prematurely ending the turn.
pub fn resolve_interim_continue_cap(max_tool_rounds: usize) -> usize {
    let rounds = max_tool_rounds.max(1);
    ((rounds * 4) / 5).clamp(4, 10)
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinueReason {
    /// Model returned no text and no tool calls after tools already ran — nudge one more round.
    EmptyAfterTools,
    /// Model returned a short interim acknowledgment/status (no tool call) — continue
    /// (bounded) so a brief "let me check…" doesn't prematurely end the turn.
    InterimProse,
    /// Long planning/status prose — reloop with full text kept in transcript.
    ExtendedProse,
    /// Principal-facing content held for one bounded resolution round.
    PackHold,
}

/// Developer-facing turn-control body for `[MEDOUSA_TURN_CONTROL]`.
pub fn continue_control_message(reason: ContinueReason, _missing_tools: &[String]) -> String {
    match reason {
        ContinueReason::EmptyAfterTools => {
            "Turn continues: last model round had no tool calls and no assistant text. \
             Call the tools you still need in this round, then deliver the complete answer; \
             cognition_turn_finish is the explicit hard stop and cognition_turn_checkpoint \
             is for a mid-task handoff. Synthesis-bound workers must use cognition_turn_finish \
             for direct pass-through."
                .to_string()
        }
        ContinueReason::InterimProse => {
            "Turn continues: call tools for the work you described, or cognition_turn_begin_work(goal, message) \
             to enter the bound workshop for multi-tool execution. Host scheduling prose may continue briefly; \
             use cognition_turn_finish once the principal-facing answer is ready."
                .to_string()
        }
        ContinueReason::ExtendedProse => {
            "Runtime reloop: your last message was kept in history. Next round: call cognition_turn_begin_work \
             for execution work, or the tools you still need on host (memory, vault read, runtime). \
             Check [MEDOUSA_SCRATCH] digests_recent before re-calling tools you already ran."
                .to_string()
        }
        ContinueReason::PackHold => crate::agent_runtime::turn_ledger::pack_hold_resolution_control_message(),
    }
}

fn host_empty_after_tools_control_message() -> String {
    "Turn continues: last round had no assistant text after tools. Call cognition_turn_begin_work for \
     execution work or any host tools you still need, then deliver your answer in prose."
        .to_string()
}

fn is_short_interim_prose(draft: &str) -> bool {
    let trimmed = draft.trim();
    if trimmed.is_empty() || is_extended_prose(trimmed) {
        return false;
    }
    if looks_like_substantive_final_answer(draft) {
        return false;
    }
    looks_like_interim_status(draft) || trimmed.chars().count() <= INTERIM_MAX_CHARS
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

fn maybe_continue_prose(
    draft: &str,
    interim_continues_used: usize,
    interim_continue_cap: usize,
    after_tools: bool,
) -> Option<TurnRoundAction> {
    if interim_continues_used >= interim_continue_cap {
        return None;
    }
    if looks_like_substantive_final_answer(draft) {
        return None;
    }
    if is_extended_prose(draft) {
        return Some(continue_loop(ContinueReason::ExtendedProse, vec![]));
    }
    if is_short_interim_prose(draft) {
        return Some(continue_loop(ContinueReason::InterimProse, vec![]));
    }
    if !after_tools && looks_like_planning_prose(draft) {
        return Some(continue_loop(ContinueReason::ExtendedProse, vec![]));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoToolDebtRoundContext {
    pub draft_text: String,
    pub pending_final_answer: bool,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    /// Interim auto-continues already spent this turn.
    pub interim_continues_used: usize,
    /// Per-turn budget for interim auto-continues (bounded so the loop can't spin).
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
    /// Interim auto-continues already spent this turn.
    pub interim_continues_used: usize,
    /// Per-turn budget for interim auto-continues (bounded so the loop can't spin).
    pub interim_continue_cap: usize,
    pub completion_profile: TurnCompletionProfile,
    /// Host scheduler: empty-after-tools continues already spent this turn.
    pub empty_after_tools_continues_used: usize,
}

fn decide_host_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    if looks_like_substantive_final_answer(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "no_tools_prose",
        };
    }

    if !draft.is_empty() {
        return continue_loop(ContinueReason::PackHold, vec![]);
    }

    TurnRoundAction::EndTurn {
        termination_reason: "no_tools_prose",
    }
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

    if looks_like_substantive_final_answer(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "host_scheduler_prose",
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

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    if let Some(action) = maybe_continue_prose(
        &ctx.draft_text,
        ctx.interim_continues_used,
        ctx.interim_continue_cap,
        false,
    ) {
        return action;
    }

    TurnRoundAction::EndTurn {
        termination_reason: "no_tools_prose",
    }
}

fn decide_foreground_no_tool_debt_text_round(ctx: &NoToolDebtRoundContext) -> TurnRoundAction {
    let draft = ctx.draft_text.trim();

    if let Some(action) = common_terminal_guard(
        draft,
        ctx.pending_final_answer,
        ctx.rounds_executed,
        ctx.max_tool_rounds,
    ) {
        return action;
    }

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    if draft.is_empty() {
        return TurnRoundAction::EndTurn {
            termination_reason: "no_tools_prose",
        };
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

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    if let Some(action) = maybe_continue_prose(
        &ctx.draft_text,
        ctx.interim_continues_used,
        ctx.interim_continue_cap,
        true,
    ) {
        return action;
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

    if looks_like_clarifying_question(&ctx.draft_text) {
        return TurnRoundAction::EndTurn {
            termination_reason: "clarifying_question",
        };
    }

    continue_loop(ContinueReason::PackHold, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn ctx(draft: &str) -> NoToolDebtRoundContext {
        NoToolDebtRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 1,
            max_tool_rounds: 10,
            interim_continues_used: 0,
            interim_continue_cap: 2,
            completion_profile: TurnCompletionProfile::WorkerSynthesis,
        }
    }

    fn foreground_ctx(draft: &str) -> NoToolDebtRoundContext {
        let mut round = ctx(draft);
        round.completion_profile = TurnCompletionProfile::ForegroundPrincipal;
        round
    }

    fn after_tools<'a>(
        draft: &str,
        invocations: &'a [ToolInvocation],
    ) -> AfterToolsRoundContext<'a> {
        AfterToolsRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 3,
            max_tool_rounds: 10,
            invocations,
            workshop_lane: false,
            interim_continues_used: 0,
            interim_continue_cap: 2,
            completion_profile: TurnCompletionProfile::WorkerSynthesis,
            empty_after_tools_continues_used: 0,
        }
    }

    fn foreground_after_tools<'a>(
        draft: &str,
        invocations: &'a [ToolInvocation],
    ) -> AfterToolsRoundContext<'a> {
        let mut round = after_tools(draft, invocations);
        round.completion_profile = TurnCompletionProfile::ForegroundPrincipal;
        round
    }

    fn host_ctx(draft: &str) -> NoToolDebtRoundContext {
        NoToolDebtRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 1,
            max_tool_rounds: 10,
            interim_continues_used: 0,
            interim_continue_cap: 1,
            completion_profile: TurnCompletionProfile::HostScheduler,
        }
    }

    fn host_after_tools<'a>(
        draft: &str,
        invocations: &'a [ToolInvocation],
    ) -> AfterToolsRoundContext<'a> {
        AfterToolsRoundContext {
            draft_text: draft.to_string(),
            pending_final_answer: false,
            rounds_executed: 3,
            max_tool_rounds: 10,
            invocations,
            workshop_lane: false,
            interim_continues_used: 0,
            interim_continue_cap: 1,
            completion_profile: TurnCompletionProfile::HostScheduler,
            empty_after_tools_continues_used: 0,
        }
    }

    fn tool(name: &str) -> ToolInvocation {
        ToolInvocation {
            tool_name: name.to_string(),
            tool_input: Value::Null,
            tool_output: Value::Null,
        }
    }

    #[test]
    fn interim_before_tools_continues_bounded() {
        let action = decide_no_tool_debt_text_round(&ctx("Let me check that for you."));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::InterimProse,
                ..
            }
        ));
    }

    #[test]
    fn planning_prose_before_tools_continues_extended() {
        let planning = "The environment is confirmed — 11 surfaces, blank canvas, and the full \
                        component toolkit is live. Let's make the first mark. I'm going to build \
                        you a Home dashboard — a persistent component on the home surface. \
                        I'll start with environment_get, then propose a custom surface in the active \
                        preset, then component_create with presentation type and artifactId config.";
        assert!(is_extended_prose(planning));
        let action = decide_no_tool_debt_text_round(&ctx(planning));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ExtendedProse,
                ..
            }
        ));
    }

    #[test]
    fn interim_before_tools_ends_when_cap_exhausted() {
        let cap = resolve_interim_continue_cap(10);
        let mut round = ctx("Let me check that for you.");
        round.interim_continues_used = cap;
        round.interim_continue_cap = cap;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
    }

    #[test]
    fn self_correction_after_tools_continues_as_interim() {
        let invocations = vec![tool("cognition_environment_get")];
        let draft = "Now I see what went wrong before — I was targeting home (builtin), which \
                     silently rejects components. Let me grab the schemas.";
        assert!(crate::turn_text_heuristics::looks_like_interim_status(
            draft
        ));
        let action = decide_after_tools_text_round(&after_tools(draft, &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::InterimProse,
                ..
            }
        ));
    }

    #[test]
    fn foreground_announcement_stays_a_preamble() {
        // Both principal-facing profiles keep the announcement in a content pack;
        // a following tool call resets it instead of ending the turn.
        let announce = "I'll correct the hardcoded port in the three tool modules, then rerun the shell \
             smoke test to confirm the session proxy answers.";
        let mut foreground = foreground_ctx(announce);
        foreground.interim_continue_cap = resolve_interim_continue_cap(12);
        assert!(matches!(
            decide_no_tool_debt_text_round(&foreground),
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
        assert!(matches!(
            decide_no_tool_debt_text_round(&host_ctx(announce)),
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn interim_continue_cap_scales_with_round_budget() {
        assert_eq!(resolve_interim_continue_cap(10), 8);
        assert_eq!(resolve_interim_continue_cap(4), 4);
        assert_eq!(resolve_interim_continue_cap(20), 10);
    }

    #[test]
    fn substantive_no_tool_answer_ends() {
        let answer = "Here is a complete explanation of how the ingester maps channel \
                      sessions to Medousa history without any further steps needed.";
        let action = decide_no_tool_debt_text_round(&ctx(answer));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
    }

    #[test]
    fn prepare_final_with_text_ends() {
        let mut round = ctx("Here is your answer.");
        round.pending_final_answer = true;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prepare_final_then_text"
            }
        ));
    }

    #[test]
    fn interim_prose_after_tools_continues_bounded() {
        let invocations = vec![tool("cognition_memory_context")];
        let action =
            decide_after_tools_text_round(&after_tools("I'll spin up workers next.", &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::InterimProse,
                ..
            }
        ));
    }

    #[test]
    fn celebratory_preamble_after_tools_continues_extended() {
        let preamble = "Yesss! Let's do this — I'll pull up the current context, check what's \
                          resonating in memory, and calibrate to a focused AVEC posture. Boom — \
                          focused preset pulled. Let me lock it in and then call cognition_turn_finish \
                          once the full calibration summary is ready for you to read.";
        assert!(is_extended_prose(preamble));
        let invocations = vec![
            tool("cognition_memory_moods"),
            tool("cognition_memory_calibrate"),
        ];
        let action = decide_after_tools_text_round(&after_tools(preamble, &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::ExtendedProse,
                ..
            }
        ));
    }

    #[test]
    fn worker_interim_prose_after_tools_requires_finish_when_cap_exhausted() {
        let invocations = vec![tool("cognition_memory_context")];
        let cap = resolve_interim_continue_cap(10);
        let mut round = after_tools("I'll spin up workers next.", &invocations);
        round.interim_continues_used = cap;
        round.interim_continue_cap = cap;
        let action = decide_after_tools_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prose_requires_finish"
            }
        ));
    }

    #[test]
    fn foreground_substantive_prose_after_tools_enters_one_round_hold() {
        let invocations = vec![tool("cognition_memory_moods")];
        let action = decide_after_tools_text_round(&foreground_after_tools(
            "Focused preset pulled and applied: stability is now 0.95, friction dropped to 0.12, \
             and autonomy holds at 0.80. I stored the calibration summary in Locus for this session.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn long_foreground_final_is_not_mistaken_for_planning() {
        let invocations = vec![tool("cognition_coder_shell_run")];
        let answer = "The runtime now keeps announcements alive until work begins, while the final \
                      response remains principal-facing after tool execution. I verified the focused \
                      completion tests and the streamed terminal body is committed from the model \
                      response rather than whichever chunks happened to arrive first. No additional \
                      repository changes are pending, and the implementation preserves the worker \
                      synthesis boundary for delegated tasks.";
        assert!(is_extended_prose(answer));
        assert!(looks_like_substantive_final_answer(answer));
        assert!(matches!(
            decide_after_tools_text_round(&foreground_after_tools(answer, &invocations)),
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn synthesis_worker_substantive_prose_after_tools_requires_finish() {
        let invocations = vec![tool("cognition_memory_moods")];
        let action = decide_after_tools_text_round(&after_tools(
            "Focused preset pulled and applied: stability is now 0.95, friction dropped to 0.12, \
             and autonomy holds at 0.80. I stored the calibration summary in Locus for this session.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "prose_requires_finish"
            }
        ));
    }

    #[test]
    fn clarifying_question_after_tools_commits_prose() {
        let invocations = vec![tool("cognition_memory_context")];
        let action = decide_after_tools_text_round(&foreground_after_tools(
            "Which repository should I search — medousa or stasis?",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "clarifying_question"
            }
        ));
    }

    #[test]
    fn empty_after_tools_continues_without_draft() {
        let invocations = vec![tool("cognition_tool_history_summary")];
        let action = decide_after_tools_text_round(&foreground_after_tools("", &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::EmptyAfterTools,
                ..
            }
        ));
    }

    #[test]
    fn workshop_lane_prepare_final_ends() {
        let invocations = vec![tool("cognition_turn_prepare_final")];
        let mut round = after_tools(
            "searching tavily — here are raw results:\n- title one",
            &invocations,
        );
        round.pending_final_answer = true;
        round.workshop_lane = true;
        round.completion_profile = TurnCompletionProfile::WorkerSynthesis;
        let action = decide_after_tools_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "workshop_lane_prepare_final"
            }
        ));
    }

    #[test]
    fn interim_prose_control_message_recommends_begin_work() {
        let msg = continue_control_message(ContinueReason::InterimProse, &[]);
        assert!(msg.contains("cognition_turn_begin_work"));
        assert!(msg.contains("cognition_turn_finish"));
    }

    #[test]
    fn extended_prose_control_message_mentions_reloop() {
        let msg = continue_control_message(ContinueReason::ExtendedProse, &[]);
        assert!(msg.contains("Runtime reloop"));
        assert!(msg.contains("cognition_turn_begin_work"));
    }

    #[test]
    fn empty_after_tools_control_message_mentions_prose_rule() {
        let msg = continue_control_message(ContinueReason::EmptyAfterTools, &[]);
        assert!(msg.contains("cognition_turn_finish"));
    }

    #[test]
    fn host_preamble_before_tools_enters_pack_hold() {
        let action = decide_no_tool_debt_text_round(&host_ctx("Let me check that for you."));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn host_planning_before_tools_enters_pack_hold() {
        let planning = "The environment is confirmed — 11 surfaces, blank canvas, and the full \
                        component toolkit is live. Let's make the first mark. I'm going to build \
                        you a Home dashboard — a persistent component on the home surface.";
        let action = decide_no_tool_debt_text_round(&host_ctx(planning));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn host_substantive_no_tool_answer_ends() {
        let answer = "Here is a complete explanation of how the ingester maps channel \
                      sessions to Medousa history without any further steps needed.";
        let action = decide_no_tool_debt_text_round(&host_ctx(answer));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "no_tools_prose"
            }
        ));
    }

    #[test]
    fn host_ambiguous_no_tool_prose_enters_pack_hold() {
        let mut round = host_ctx("Let me check that for you.");
        round.interim_continues_used = 99;
        let action = decide_no_tool_debt_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn host_prose_after_tools_commits_directly() {
        let invocations = vec![tool("cognition_memory_moods")];
        let action = decide_after_tools_text_round(&host_after_tools(
            "Focused preset pulled and applied: stability is now 0.95, friction dropped to 0.12, \
             and autonomy holds at 0.80. I stored the calibration summary in Locus for this session.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "host_scheduler_prose"
            }
        ));
    }

    #[test]
    fn host_interim_after_tools_enters_pack_hold() {
        let invocations = vec![tool("cognition_memory_context")];
        let action = decide_after_tools_text_round(&host_after_tools(
            "I'll spin up workers next.",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn host_clarifying_question_after_tools_enters_pack_hold() {
        let invocations = vec![tool("cognition_memory_context")];
        let action = decide_after_tools_text_round(&host_after_tools(
            "Which repository should I search — medousa or stasis?",
            &invocations,
        ));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::PackHold,
                ..
            }
        ));
    }

    #[test]
    fn host_empty_after_tools_continues_once() {
        let invocations = vec![tool("cognition_tool_history_summary")];
        let action = decide_after_tools_text_round(&host_after_tools("", &invocations));
        assert!(matches!(
            action,
            TurnRoundAction::ContinueLoop {
                reason: ContinueReason::EmptyAfterTools,
                ..
            }
        ));
    }

    #[test]
    fn host_empty_after_tools_fuses_after_one_continue() {
        let invocations = vec![tool("cognition_tool_history_summary")];
        let mut round = host_after_tools("", &invocations);
        round.empty_after_tools_continues_used = HOST_EMPTY_AFTER_TOOLS_CONTINUE_CAP;
        let action = decide_after_tools_text_round(&round);
        assert!(matches!(
            action,
            TurnRoundAction::EndTurn {
                termination_reason: "max_rounds_fuse"
            }
        ));
    }
}
