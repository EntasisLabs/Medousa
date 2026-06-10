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

/// Minimum worker tool-loop rounds per intent when host config is lower (fallback floor).
pub fn max_worker_tool_rounds(intent: TurnWorkerIntent) -> usize {
    match intent {
        TurnWorkerIntent::MemoryAvecCalibrate => 12,
        TurnWorkerIntent::MemoryContext => 10,
        TurnWorkerIntent::Research => 10,
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
            "cognition_turn_begin_work",
            "cognition.turn.begin_work",
            "cognition_turn_finish",
            "cognition.turn.finish",
            "cognition_turn_request_more_rounds",
            "cognition.turn.request_more_rounds",
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
                    "cognition_identity_recall",
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
                    "cognition_identity_recall",
                ],
            );
        }
        TurnWorkerIntent::Research => {
            push(
                &mut names,
                &[
                    "cognition_memory_context",
                    "cognition_memory_recall",
                    "cognition_identity_recall",
                    "cognition_capability_invoke",
                    "cognition_capability_search",
                    "cognition_capability_resolve",
                    "cognition_mcp_invoke",
                    "cognition_mcp_discover",
                    "cognition_mcp_servers",
                    "cognition_grapheme_template_run",
                    "cognition_grapheme_modules",
                    "cognition_grapheme_modules_info",
                    "cognition_grapheme_modules_ops",
                    "cognition_grapheme_examples",
                    "cognition_grapheme_run",
                    "cognition_grapheme_cli_run",
                    "cognition_openshell_status",
                    "cognition_openshell_sandbox_run",
                    "cognition_skill_discover",
                    "cognition_skill_propose",
                    "cognition_skill_probe",
                    "cognition_vault_list",
                    "cognition_vault_read",
                    "cognition_vault_search",
                    "cognition_vault_write",
                    "cognition_grapheme_script_save",
                    "cognition_grapheme_script_list",
                    "cognition_grapheme_script_search",
                    "cognition_grapheme_script_load",
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
                    "cognition_identity_recall",
                    "cognition_capability_invoke",
                    "cognition_capability_search",
                    "cognition_capability_resolve",
                    "cognition_mcp_invoke",
                    "cognition_mcp_discover",
                    "cognition_grapheme_template_run",
                    "cognition_grapheme_modules",
                    "cognition_grapheme_examples",
                    "cognition_grapheme_run",
                    "cognition_grapheme_script_save",
                    "cognition_grapheme_script_list",
                    "cognition_grapheme_script_search",
                    "cognition_grapheme_script_load",
                    "cognition_vault_list",
                    "cognition_vault_read",
                    "cognition_vault_search",
                    "cognition_vault_write",
                ],
            );
        }
    }

    names
}

/// Tools exposed to the host (main) agent — runtime orchestrator, not Grapheme/MCP executor.
pub fn host_bus_tool_names() -> HashSet<String> {
    let mut names = HashSet::new();
    let push = |names: &mut HashSet<String>, list: &[&str]| {
        for name in list {
            names.insert((*name).to_string());
        }
    };

    push(
        &mut names,
        &[
            "cognition_turn_begin_work",
            "cognition.turn.begin_work",
            "cognition_turn_finish",
            "cognition.turn.finish",
            "cognition_turn_request_more_rounds",
            "cognition.turn.request_more_rounds",
            "cognition_utility_time_now",
            "cognition_utility_day_of_week",
            "cognition_utility_uuid",
        ],
    );

    push(
        &mut names,
        &[
            "cognition_spawn_turn_worker",
            "cognition_turn_worker_status",
            "cognition_turn_worker_cancel",
        ],
    );

    push(
        &mut names,
        &[
            "cognition_identity_context",
            "cognition_identity_propose",
            "cognition_identity_commit",
            "cognition_identity_recall",
            "cognition_identity_remember",
            "cognition_manuscript_list",
            "cognition_manuscript_resolve",
            "cognition_skill_discover",
            "cognition_skill_propose",
            "cognition_openshell_status",
            "cognition_vault_list",
            "cognition_vault_read",
            "cognition_vault_search",
            "cognition_vault_write",
            "cognition_tool_history_summary",
            "cognition_tool_history_detail",
            "cognition_grapheme_script_list",
            "cognition_grapheme_script_search",
            "cognition_manuscript_overlay_propose",
            "cognition_manuscript_overlay_list",
        ],
    );

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

    push(
        &mut names,
        &[
            "cognition_capability_list",
            "cognition.capability.list",
            "cognition_capability_search",
            "cognition.capability.search",
            "cognition_capability_resolve",
            "cognition.capability.resolve",
        ],
    );

    push(
        &mut names,
        &[
            "cognition_job_enqueue",
            "cognition.job.enqueue",
            "cognition_runtime_jobs_list",
            "cognition_runtime_jobs_status",
            "cognition_runtime_jobs_cancel",
            "cognition_runtime_delivery_status",
            "cognition_runtime_recurring_list",
            "cognition_runtime_recurring_register",
            "cognition_runtime_recurring_pause",
            "cognition_runtime_recurring_cancel",
            "cognition_runtime_recurring_doctor",
            "cognition_runtime_recurring_preview",
            "cognition_runtime_workflow_run",
            "cognition_runtime_workflow_schedule",
            "cognition_runtime_workflow_status",
            "cognition_runtime_workflow_cancel",
            "cognition_runtime_workflow_plan",
        ],
    );

    names
}

pub fn tool_allowed(name: &str, allowlist: &HashSet<String>) -> bool {
    crate::tool_aliases::tool_allowed_matches_with_legacy(name, allowlist)
}

pub fn worker_allowlist_for_intent_and_tools(
    intent: TurnWorkerIntent,
    manuscript_tools: &[String],
) -> HashSet<String> {
    let intent_allow = allowed_tool_names_for_intent(intent);
    if manuscript_tools.is_empty() {
        return intent_allow;
    }
    manuscript_tools
        .iter()
        .filter(|tool| tool_allowed(tool, &intent_allow))
        .map(|tool| tool.to_string())
        .collect()
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

    #[test]
    fn research_intent_includes_grapheme_discovery_tools() {
        let names = allowed_tool_names_for_intent(TurnWorkerIntent::Research);
        assert!(names.contains("cognition_memory_context"));
        assert!(names.contains("cognition_grapheme_modules"));
        assert!(names.contains("cognition_grapheme_run"));
        assert!(names.contains("cognition_grapheme_template_run"));
        assert!(names.contains("cognition_capability_invoke"));
        assert!(!names.contains("cognition_memory_calibrate"));
        assert!(names.contains("cognition_identity_recall"));
        assert!(!names.contains("cognition_identity_remember"));
        assert!(names.contains("cognition_openshell_status"));
        assert!(names.contains("cognition_openshell_sandbox_run"));
        assert!(names.contains("cognition_skill_discover"));
        assert!(names.contains("cognition_skill_probe"));
        assert!(names.contains("cognition_vault_write"));
    }

    #[test]
    fn manuscript_allowlist_intersects_intent_tools() {
        let tools = vec![
            "cognition_identity_recall".to_string(),
            "cognition_memory_context".to_string(),
            "cognition_capability_invoke".to_string(),
            "cognition_spawn_turn_worker".to_string(),
        ];
        let allow =
            worker_allowlist_for_intent_and_tools(TurnWorkerIntent::Research, &tools);
        assert!(allow.contains("cognition_identity_recall"));
        assert!(allow.contains("cognition_memory_context"));
        assert!(allow.contains("cognition_capability_invoke"));
        assert!(!allow.contains("cognition_spawn_turn_worker"));
        assert!(!allow.contains("cognition_grapheme_run"));
    }

    #[test]
    fn host_orchestrator_has_memory_runtime_and_catalog_not_grapheme() {
        let names = host_bus_tool_names();
        assert!(names.contains("cognition_memory_calibrate"));
        assert!(names.contains("cognition_identity_propose"));
        assert!(names.contains("cognition_identity_recall"));
        assert!(names.contains("cognition_identity_remember"));
        assert!(names.contains("cognition_job_enqueue"));
        assert!(names.contains("cognition_spawn_turn_worker"));
        assert!(names.contains("cognition_capability_search"));
        assert!(names.contains("cognition_runtime_workflow_run"));
        assert!(!names.contains("cognition_grapheme_run"));
        assert!(!names.contains("cognition_capability_invoke"));
        assert!(!names.contains("cognition_mcp_invoke"));
        assert!(!names.contains("cognition_turn_prepare_final"));
        assert!(names.contains("cognition_turn_begin_work"));
        assert!(names.contains("cognition_turn_finish"));
    }
}
