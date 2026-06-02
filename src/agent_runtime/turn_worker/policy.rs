//! Worker intent → tool allowlists (Phase 1).

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnWorkerIntent {
    MemoryAvecCalibrate,
    MemoryContext,
    Research,
    General,
}

impl TurnWorkerIntent {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "memory.avec_calibrate" | "avec_calibrate" | "avec.calibrate" => {
                Some(Self::MemoryAvecCalibrate)
            }
            "memory.context" | "memory_context" => Some(Self::MemoryContext),
            "research" | "delegate.research" | "web" | "websearch" => Some(Self::Research),
            "general" | "default" => Some(Self::General),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MemoryAvecCalibrate => "memory.avec_calibrate",
            Self::MemoryContext => "memory.context",
            Self::Research => "research",
            Self::General => "general",
        }
    }
}

/// Worker tool-loop round budget per intent (Phase 2).
pub fn max_worker_tool_rounds(intent: TurnWorkerIntent) -> usize {
    match intent {
        TurnWorkerIntent::MemoryAvecCalibrate => 12,
        TurnWorkerIntent::MemoryContext => 10,
        TurnWorkerIntent::Research => 14,
        TurnWorkerIntent::General => 10,
    }
}

pub fn allowed_tool_names_for_intent(intent: TurnWorkerIntent) -> HashSet<String> {
    let mut names = HashSet::new();
    let push = |names: &mut HashSet<String>, list: &[&str]| {
        for name in list {
            names.insert((*name).to_string());
        }
    };

    push(
        &mut names,
        &[
            "cognition_turn_prepare_final",
            "cognition.turn.prepare_final",
            "cognition_utility_time_now",
            "cognition_utility_day_of_week",
            "cognition_utility_uuid",
        ],
    );

    match intent {
        TurnWorkerIntent::MemoryAvecCalibrate => {
            push(
                &mut names,
                &[
                    "cognition_memory_schema",
                    "cognition_memory_moods",
                    "cognition_memory_calibrate",
                    "cognition_memory_context",
                    "cognition_memory_list",
                    "cognition_memory_recall",
                    "cognition_memory_store",
                ],
            );
        }
        TurnWorkerIntent::MemoryContext => {
            push(
                &mut names,
                &[
                    "cognition_memory_schema",
                    "cognition_memory_moods",
                    "cognition_memory_context",
                    "cognition_memory_list",
                    "cognition_memory_recall",
                    "cognition_memory_store",
                ],
            );
        }
        TurnWorkerIntent::Research => {
            push(
                &mut names,
                &[
                    "cognition_capability_invoke",
                    "cognition.capability.invoke",
                    "cognition_capability_search",
                    "cognition.capability.search",
                    "cognition_capability_resolve",
                    "cognition.capability.resolve",
                    "cognition_mcp_invoke",
                    "cognition.mcp.invoke",
                    "cognition_mcp_discover",
                    "cognition.mcp.discover",
                    "cognition_grapheme_modules_search",
                    "cognition_grapheme_run",
                    "cognition_grapheme_cli_run",
                ],
            );
        }
        TurnWorkerIntent::General => {
            push(
                &mut names,
                &[
                    "cognition_memory_schema",
                    "cognition_memory_moods",
                    "cognition_memory_calibrate",
                    "cognition_memory_context",
                    "cognition_memory_list",
                    "cognition_memory_recall",
                    "cognition_memory_store",
                    "cognition_capability_invoke",
                    "cognition.capability.invoke",
                    "cognition_mcp_invoke",
                    "cognition.mcp.invoke",
                ],
            );
        }
    }

    names
}

pub fn host_bus_tool_names() -> HashSet<String> {
    let mut names = HashSet::new();
    for name in [
        "cognition_spawn_turn_worker",
        "cognition_turn_worker_status",
        "cognition_turn_worker_cancel",
        "cognition_turn_prepare_final",
        "cognition.turn.prepare_final",
        "cognition_utility_time_now",
        "cognition_utility_day_of_week",
        "cognition_utility_uuid",
    ] {
        names.insert(name.to_string());
    }
    names
}

pub fn tool_allowed(name: &str, allowlist: &HashSet<String>) -> bool {
    let trimmed = name.trim();
    if allowlist.contains(trimmed) {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    allowlist.contains(&lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_intents() {
        assert_eq!(
            TurnWorkerIntent::parse("memory.avec_calibrate"),
            Some(TurnWorkerIntent::MemoryAvecCalibrate)
        );
    }

    #[test]
    fn avec_intent_includes_calibrate() {
        let names = allowed_tool_names_for_intent(TurnWorkerIntent::MemoryAvecCalibrate);
        assert!(names.contains("cognition_memory_calibrate"));
    }

    #[test]
    fn parses_research_intent() {
        assert_eq!(
            TurnWorkerIntent::parse("research"),
            Some(TurnWorkerIntent::Research)
        );
    }
}
