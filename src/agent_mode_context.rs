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
    "cognition_calendar_query",
    "cognition_calendar_mutate",
    "cognition_capability",
    "cognition_schema",
    "cognition_store_read",
    "cognition_store_write",
    "cognition_web_search",
    "cognition_utility_time_now",
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
        AgentModeId::General | AgentModeId::Teacher | AgentModeId::Coder => None,
    }
}

pub fn instant_tool_names() -> HashSet<String> {
    INSTANT_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_is_a_context_only_bounded_profile() {
        let limits = context_limits_for_mode(AgentModeId::Instant).expect("instant limits");
        assert_eq!(limits.hot_window_turns, 6);
        assert_eq!(limits.cold_window_turns, 0);
        assert!(!limits.include_auxiliary_history);
        let tools = instant_tool_names();
        assert_eq!(tools.len(), 11);
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
        assert_eq!(context_limits_for_mode(AgentModeId::Teacher), None);
        assert_eq!(context_limits_for_mode(AgentModeId::Coder), None);
    }
}
