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
