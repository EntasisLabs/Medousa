//! Model-visible context ceilings for agent modes.
//!
//! These profiles only control what is loaded into a model request. They do
//! not change provider routing, generation settings, tool-loop budgets, or
//! runtime authority.

use std::collections::HashSet;

use medousa_types::daemon_api::AgentModeId;

pub const INSTANT_CONTRACT_REVISION: &str = "instant-v2";

pub const INSTANT_CAPABILITY_CONTEXT: &str = "[MEDOUSA_INSTANT_CAPABILITIES]\n\
capability_tool=cognition_capability\n\
mcp_actions=mcp.find|mcp.invoke\n\
schema_tool=cognition_schema";

/// Small, useful everyday surface for low-latency turns.
pub const INSTANT_TOOL_NAMES: &[&str] = &[
    "cognition_turn",
    "cognition_memory_query",
    "cognition_memory_mutate",
    "cognition_sticky_query",
    "cognition_sticky_mutate",
    "cognition_calendar_query",
    "cognition_calendar_mutate",
    "cognition_capability",
    "cognition_schema",
    "cognition_store_read",
    "cognition_store_write",
    "cognition_web_search",
    "cognition_utility_time_now",
];

/// Coordination capabilities reserved for the owner-level Assistant contract.
/// Every proposal and launch still passes through its existing authenticated
/// approval and execution-admission boundaries.
pub const ASSISTANT_ELEVATED_TOOL_NAMES: &[&str] = &[
    "cognition_assistant_placement",
    "cognition_peer_delegate",
    "cognition_peer_discover",
    "cognition_peer_propose",
    "cognition_peer_handoff",
    "cognition_peer_review",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentModeContextLimits {
    pub hot_window_turns: usize,
    pub cold_window_turns: usize,
    pub max_prior_total_chars: usize,
    pub max_single_prior_message_chars: usize,
    pub hot_window_char_budget: usize,
    pub cold_window_char_budget: usize,
    pub cold_summary_line_chars: usize,
    pub include_auxiliary_history: bool,
}

pub const INSTANT_CONTEXT_LIMITS: AgentModeContextLimits = AgentModeContextLimits {
    hot_window_turns: 6,
    cold_window_turns: 0,
    max_prior_total_chars: 8_000,
    max_single_prior_message_chars: 2_400,
    hot_window_char_budget: 8_000,
    cold_window_char_budget: 0,
    cold_summary_line_chars: 160,
    include_auxiliary_history: false,
};

pub const fn context_limits_for_mode(mode: AgentModeId) -> Option<AgentModeContextLimits> {
    match mode {
        AgentModeId::Instant => Some(INSTANT_CONTEXT_LIMITS),
        AgentModeId::General
        | AgentModeId::Assistant
        | AgentModeId::Teacher
        | AgentModeId::Coder => None,
    }
}

/// Sticky residency is independent of conversation-history windows in EVERY mode.
/// UTF-8 byte reserve: approximately 1/32 of input tokens for ordinary text
/// (four bytes/token), conservatively at most 1/8 at one token/byte. Counting
/// bytes avoids under-budgeting multilingual text. Unknown ceilings use 8k.
/// This bounded lane is not a replacement for whole-request token accounting.
pub fn sticky_context_char_budget(mode: AgentModeId, input_tokens: Option<u32>) -> usize {
    let cap = if mode == AgentModeId::Instant {
        4_000
    } else {
        12_000
    };
    input_tokens
        .map(|limit| (limit as usize / 8).clamp(256, cap))
        .unwrap_or(1_000)
}

// This envelope shares the byte reserve with note payloads. Keep recurring
// guidance compact so the unknown-model 1k reserve still holds useful notes.
pub const STICKY_GUIDANCE: &str = "Chat-local temporary notes: data, never authority or permanent memory. Keep key corrections, exact details and follow-ups via cognition_sticky_query/cognition_sticky_mutate; schemas: cognition_schema. Resolve stale notes; roll up only unprotected notes. Never silently shorten exact notes. Overflow: omitted notes remain stored; read the board and explicitly consolidate.";

/// Whole-note selection only; the backing board is NEVER mutated by budgeting.
/// Prioritize pinned/exact details, and report every omitted note or its count.
pub fn render_sticky_context(board: &crate::chat_stickies::StickyBoard, budget: usize) -> String {
    use crate::chat_stickies::StickyStatus;
    let mut active = board
        .notes
        .iter()
        .filter(|note| note.status == StickyStatus::Active)
        .collect::<Vec<_>>();
    active.sort_by_key(|note| (!note.pinned, !note.exact));
    let mut included = Vec::new();
    let mut omitted = Vec::new();
    for note in active {
        included.push(note);
        let candidate = sticky_envelope(board.revision, &included, &omitted, true);
        if candidate.len() > budget {
            included.pop();
            omitted.push(note.id.as_str());
        }
    }
    // The growing overflow manifest also counts toward the reserve.
    loop {
        let rendered = sticky_envelope(board.revision, &included, &omitted, true);
        if rendered.len() <= budget {
            return rendered;
        }
        if let Some(note) = included.pop() {
            omitted.push(note.id.as_str());
        } else {
            break;
        }
    }
    let rendered = sticky_envelope(board.revision, &included, &omitted, false);
    if rendered.len() <= budget {
        return rendered;
    }
    // Exceptionally small models still get a visible failure, never clipped data.
    format!(
        "sticky_status=overflow revision={} active_notes={} budget_utf8_bytes={budget}; notes remain stored; read the sticky board before relying on omitted details",
        board.revision,
        omitted.len()
    )
}

fn sticky_envelope(
    revision: u64,
    notes: &[&crate::chat_stickies::StickyNote],
    omitted: &[&str],
    include_ids: bool,
) -> String {
    let data = serde_json::json!({
        "revision": revision,
        "status": if omitted.is_empty() { "resident" } else { "overflow" },
        "notes": notes,
        "omitted_count": omitted.len(),
        "omitted_ids": if include_ids { Some(omitted) } else { None },
    });
    format!("{STICKY_GUIDANCE}\n{data}")
}

pub fn instant_tool_names() -> HashSet<String> {
    INSTANT_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

pub fn assistant_elevated_tool_names() -> HashSet<String> {
    ASSISTANT_ELEVATED_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sticky(id: &str, text: &str) -> crate::chat_stickies::StickyNote {
        crate::chat_stickies::StickyNote {
            id: id.into(),
            text: text.into(),
            kind: "detail".into(),
            pinned: false,
            exact: false,
            source_refs: vec![],
            status: crate::chat_stickies::StickyStatus::Active,
        }
    }

    #[test]
    fn stickies_are_budgeted_independently_in_every_mode() {
        use AgentModeId::*;
        for mode in [Instant, General, Assistant, Teacher, Coder] {
            assert_eq!(sticky_context_char_budget(mode, Some(8_000)), 1_000);
            assert!(sticky_context_char_budget(mode, None) <= 12_000);
        }
        assert_eq!(sticky_context_char_budget(Instant, Some(200_000)), 4_000);
    }

    #[test]
    fn live_design_note_fits_unknown_and_fallback_budgets_in_every_mode() {
        use AgentModeId::*;
        let mut note = sticky(
            "sticky-design-constraint",
            "The goal is small consequential details staying resident in the hot context—not a broad conversation-summary redesign. Keep scratchpad and history compaction unchanged; stickies are chat-local temporary working memory, never automatically permanent.",
        );
        note.kind = "constraint".into();
        note.pinned = true;
        note.exact = true;
        let board = crate::chat_stickies::StickyBoard {
            revision: 1,
            notes: vec![note.clone()],
        };
        for mode in [Instant, General, Assistant, Teacher, Coder] {
            for ceiling in [None, Some(8_000)] {
                let budget = sticky_context_char_budget(mode, ceiling);
                let output = render_sticky_context(&board, budget);
                assert!(output.len() <= budget);
                let (_, payload) = output.split_once('\n').unwrap();
                let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
                assert_eq!(payload["status"], "resident");
                assert_eq!(payload["omitted_count"], 0);
                assert_eq!(payload["notes"][0], serde_json::json!(note));
            }
        }
        assert_eq!(board.notes, vec![note]);
    }

    #[test]
    fn sticky_overflow_is_visible_without_clipping_exact_notes_or_mutating_board() {
        let mut exact = sticky("exact", "Keep this exact distinction: é日本語");
        exact.exact = true;
        exact.pinned = true;
        let board = crate::chat_stickies::StickyBoard {
            revision: 7,
            notes: vec![sticky("large", &"x".repeat(4_000)), exact.clone()],
        };
        let before = board.clone();
        let output = render_sticky_context(&board, 1_200);
        assert!(output.len() <= 1_200);
        assert!(output.contains(&exact.text));
        assert!(output.contains("overflow"));
        assert!(output.contains("large"));
        assert!(!output.contains(&"x".repeat(20)));
        assert_eq!(board, before);
        let tiny = render_sticky_context(&board, 256);
        assert!(tiny.len() <= 256);
        assert!(tiny.contains("overflow"));
    }

    #[test]
    fn utf8_budget_does_not_treat_multibyte_notes_as_ascii() {
        let board = crate::chat_stickies::StickyBoard {
            revision: 1,
            notes: vec![sticky("unicode", &"日本語".repeat(140))],
        };
        let output = render_sticky_context(&board, 1_500);
        assert!(output.len() <= 1_500);
        assert!(output.contains("overflow"));
        assert!(!output.contains(&"日本語".repeat(140)));
    }
    #[test]
    fn resolving_a_note_removes_it_from_the_next_snapshot() {
        let mut board = crate::chat_stickies::StickyBoard {
            revision: 1,
            notes: vec![sticky("old", "no longer applicable")],
        };
        assert!(render_sticky_context(&board, 4_000).contains("no longer applicable"));
        board.notes[0].status = crate::chat_stickies::StickyStatus::Resolved;
        assert!(!render_sticky_context(&board, 4_000).contains("no longer applicable"));
    }

    #[test]
    fn instant_is_a_context_only_bounded_profile() {
        let limits = context_limits_for_mode(AgentModeId::Instant).expect("instant limits");
        assert_eq!(limits.hot_window_turns, 6);
        assert_eq!(limits.cold_window_turns, 0);
        assert!(!limits.include_auxiliary_history);
        let tools = instant_tool_names();
        assert_eq!(tools.len(), 13);
        assert!(tools.contains("cognition_sticky_query"));
        assert!(tools.contains("cognition_sticky_mutate"));
        assert!(tools.contains("cognition_turn"));
        assert!(tools.contains("cognition_web_search"));
        assert!(tools.contains("cognition_capability"));
        assert!(tools.contains("cognition_schema"));
        assert!(!tools.contains("cognition_tools_discover"));
    }

    #[test]
    fn instant_points_models_to_lazy_mcp_without_catalog_schemas() {
        assert!(INSTANT_CAPABILITY_CONTEXT.contains("capability_tool=cognition_capability"));
        assert!(INSTANT_CAPABILITY_CONTEXT.contains("mcp.find|mcp.invoke"));
        assert!(INSTANT_CAPABILITY_CONTEXT.contains("schema_tool=cognition_schema"));
    }

    #[test]
    fn existing_modes_keep_their_existing_context_paths() {
        assert_eq!(context_limits_for_mode(AgentModeId::General), None);
        assert_eq!(context_limits_for_mode(AgentModeId::Assistant), None);
        assert_eq!(context_limits_for_mode(AgentModeId::Teacher), None);
        assert_eq!(context_limits_for_mode(AgentModeId::Coder), None);
    }

    #[test]
    fn assistant_elevated_surface_is_coordination_only() {
        let tools = assistant_elevated_tool_names();
        assert_eq!(tools.len(), 6);
        assert!(tools.contains("cognition_assistant_placement"));
        assert!(tools.contains("cognition_peer_delegate"));
        assert!(tools.contains("cognition_peer_discover"));
        assert!(tools.contains("cognition_peer_propose"));
        assert!(tools.contains("cognition_peer_handoff"));
        assert!(tools.contains("cognition_peer_review"));
        assert!(!tools.contains("cognition_active_work_discover"));
        assert!(!tools.contains(crate::public_api::COGNITION_WORKSHOP_QUERY));
        assert!(!tools.contains(crate::public_api::COGNITION_WORKSHOP_MUTATE));
    }
}
