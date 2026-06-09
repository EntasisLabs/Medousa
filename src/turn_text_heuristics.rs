//! Shared heuristics for assistant text during tool-loop turns.

const WORK_IN_PROGRESS_ANYWHERE: &[&str] = &[
    "let me ",
    "i'll ",
    "i will ",
    "i'm going to ",
    "going to ",
    "one moment",
    "one sec",
    "hang on",
    "just a sec",
    "checking ",
    "looking ",
    "working on ",
    "pulling ",
    "fetching ",
    "searching ",
    "reading ",
    "lock it in",
    "pull up ",
    "calibrate to",
    "calibrating",
];

pub fn looks_like_interim_status(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();

    if WORK_IN_PROGRESS_ANYWHERE
        .iter()
        .any(|phrase| lower.contains(phrase))
    {
        return true;
    }

    const SHORT_ACKS: &[&str] = &[
        "stored.",
        "stored!",
        "done.",
        "done!",
        "ok.",
        "ok!",
        "okay.",
        "okay!",
        "got it.",
        "got it!",
        "sure.",
        "sure!",
        "saved.",
        "saved!",
    ];
    if SHORT_ACKS.iter().any(|ack| lower == *ack) {
        return true;
    }

    let word_count = trimmed.split_whitespace().count();
    if word_count <= 6 && !trimmed.contains('?') {
        return true;
    }

    false
}

pub fn looks_like_substantive_final_answer(text: &str) -> bool {
    if looks_like_interim_status(text) {
        return false;
    }

    let trimmed = text.trim();
    let word_count = trimmed.split_whitespace().count();
    if word_count < 12 {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    const OUTCOME_HINTS: &[&str] = &[
        "stability",
        "friction",
        "autonomy",
        "logic",
        "drift",
        "calibrat",
        "avec",
        "session",
        "memory",
        "node",
        "stored",
        "saved",
        "here's",
        "here is",
        "result",
        "summary",
    ];
    if word_count >= 20 {
        return true;
    }

    OUTCOME_HINTS.iter().any(|hint| lower.contains(hint))
}

/// Legacy loop finalize helper. Turn completion FSM owns runtime policy; kept for tests.
pub fn should_finalize_on_text_only_response(
    has_selected_tool: bool,
    invocations_len: usize,
    text: &str,
    pending_final_answer: bool,
    rounds_executed: usize,
    max_tool_rounds: usize,
) -> bool {
    if has_selected_tool {
        return false;
    }
    if pending_final_answer {
        return !text.trim().is_empty();
    }
    if rounds_executed >= max_tool_rounds {
        return true;
    }
    if invocations_len == 0 {
        return false;
    }
    looks_like_substantive_final_answer(text)
}

pub fn termination_reason_for_text_only_finalize(
    pending_final_answer: bool,
    rounds_executed: usize,
    max_tool_rounds: usize,
) -> &'static str {
    if pending_final_answer {
        "prepare_final_then_text"
    } else if rounds_executed >= max_tool_rounds {
        "max_rounds_fuse"
    } else {
        "heuristic_substantive"
    }
}

/// True when assistant text is a direct clarifying question for the operator.
pub fn looks_like_clarifying_question(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || !trimmed.ends_with('?') {
        return false;
    }
    if looks_like_interim_status(text) {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    if word_count > 120 {
        return false;
    }
    let question_marks = trimmed.chars().filter(|ch| *ch == '?').count();
    if question_marks > 3 {
        return false;
    }
    true
}

const HOST_DELEGATION_USER_PHRASES: &[&str] = &[
    "spin up",
    "spin them",
    "send it",
    "go ahead",
    "do it now",
    "do it!",
    "start the worker",
    "start workers",
    "spawn worker",
    "spawn workers",
    "research worker",
    "delegate",
    "run grapheme",
    "execute",
    "send them",
    "lets go",
    "let's go",
    "perfect!!",
    "multi-topic",
    "research these",
    "spin workers",
    "launch worker",
    "background worker",
];

const PENDING_SPAWN_DRAFT_PHRASES: &[&str] = &[
    "spin up",
    "spawn worker",
    "spawn workers",
    "spawn them",
    "i'll spawn",
    "i will spawn",
    "going to spawn",
    "let me spawn",
    "delegate to",
    "background worker",
    "workshop",
    "cognition_spawn_turn_worker",
    "hand off",
    "handoff",
    "workers next",
    "worker next",
];

/// User message implies host should delegate heavy execution (spawn worker), not stop at plan prose.
pub fn user_prompt_implies_host_delegation(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    HOST_DELEGATION_USER_PHRASES
        .iter()
        .any(|phrase| lower.contains(phrase))
}

/// Assistant draft promises spawn/delegation that has not happened yet.
pub fn draft_implies_pending_spawn(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    PENDING_SPAWN_DRAFT_PHRASES
        .iter()
        .any(|phrase| lower.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interim_status_before_first_tool_does_not_finalize_legacy_helper() {
        assert!(looks_like_interim_status("Let me check that for you."));
        assert!(!should_finalize_on_text_only_response(
            false,
            0,
            "Let me check that for you.",
            false,
            1,
            10
        ));
    }

    #[test]
    fn substantive_answer_after_tools_finalizes_legacy_helper() {
        let answer = "Your memory profile shows stability at 0.95 and three recent nodes about \
                      the ingester roadmap. I stored the update in Locus.";
        assert!(looks_like_substantive_final_answer(answer));
        assert!(should_finalize_on_text_only_response(
            false, 2, answer, false, 3, 10
        ));
    }

    #[test]
    fn termination_reason_reflects_finalize_path() {
        assert_eq!(
            termination_reason_for_text_only_finalize(true, 2, 10),
            "prepare_final_then_text"
        );
        assert_eq!(
            termination_reason_for_text_only_finalize(false, 10, 10),
            "max_rounds_fuse"
        );
        assert_eq!(
            termination_reason_for_text_only_finalize(false, 3, 10),
            "heuristic_substantive"
        );
    }

    #[test]
    fn delegation_prompt_and_plan_draft_detected() {
        assert!(user_prompt_implies_host_delegation(
            "perfect!! spin them up and lets see what we can get!!"
        ));
        assert!(draft_implies_pending_spawn(
            "I'll spin up five research workers next."
        ));
    }
}
