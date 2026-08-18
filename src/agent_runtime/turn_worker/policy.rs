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
            "cognition_tools_discover",
            "cognition_turn_prepare_final",
            "cognition.turn.prepare_final",
            "cognition_turn_checkpoint",
            "cognition.turn.checkpoint",
            "cognition_turn_update_user",
            "cognition.turn.update_user",
            "cognition_turn_finish",
            "cognition.turn.finish",
            "cognition_turn_request_more_rounds",
            "cognition.turn.request_more_rounds",
            "cognition_turn_propose_mode",
            "cognition.turn.propose_mode",
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
                    "cognition_memory_store",
                    "cognition_identity_recall",
                    "cognition_capability",
                    "cognition_web_search",
                    "cognition_openshell_status",
                    "cognition_openshell_sandbox_run",
                    "cognition_shell_status",
                    "cognition_shell_run",
                    "cognition_code_hover",
                    "cognition_code_definition",
                    "cognition_code_diagnostics",
                    "cognition_code_symbols",
                    "cognition_skill_discover",
                    "cognition_skill_propose",
                    "cognition_skill_probe",
                    "cognition_calendar_list",
                    "cognition_calendar_create",
                    "cognition_calendar_update",
                    "cognition_calendar_delete",
                    "cognition_calendar_import",
                    "cognition_calendar_export",
                ],
            );
            push(
                &mut names,
                &[
                    "cognition_ui_build",
                    "cognition_ui_scene",
                    "cognition_ui_present",
                ],
            );
            push(&mut names, crate::tool_bootstrap::ENVIRONMENT_DOMAIN_TOOLS);
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
                    "cognition_web_search",
                    "cognition_capability",
                    "cognition_shell_status",
                    "cognition_shell_run",
                    "cognition_code_hover",
                    "cognition_code_definition",
                    "cognition_code_diagnostics",
                    "cognition_code_symbols",
                    "cognition_calendar_list",
                    "cognition_calendar_create",
                    "cognition_calendar_update",
                    "cognition_calendar_delete",
                    "cognition_calendar_import",
                    "cognition_calendar_export",
                    "cognition_ui_build",
                    "cognition_ui_scene",
                    "cognition_ui_present",
                ],
            );
            push(&mut names, crate::tool_bootstrap::ENVIRONMENT_DOMAIN_TOOLS);
        }
    }

    crate::public_api::ensure_public_api(&mut names);
    names
}

/// Tools exposed to the host (main) agent — scheduler, not execution engine.
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
            "cognition_tools_discover",
            "cognition_turn_begin_work",
            "cognition.turn.begin_work",
            "cognition_turn_checkpoint",
            "cognition.turn.checkpoint",
            "cognition_turn_update_user",
            "cognition.turn.update_user",
            "cognition_turn_finish",
            "cognition.turn.finish",
            "cognition_turn_request_more_rounds",
            "cognition.turn.request_more_rounds",
            "cognition_turn_propose_mode",
            "cognition.turn.propose_mode",
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
            "cognition_workshop_steer",
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
            "cognition_calendar_list",
            "cognition_calendar_create",
            "cognition_calendar_update",
            "cognition_calendar_delete",
            "cognition_calendar_import",
            "cognition_calendar_export",
            "cognition_tool_history_summary",
            "cognition_tool_history_detail",
            "cognition_chat_history_search",
            "cognition_chat_history_read",
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

    push(
        &mut names,
        &[
            "cognition_web_search",
            "cognition_browser_fetch",
            "cognition_browser_snapshot",
            "cognition_browser_act",
        ],
    );

    push(
        &mut names,
        &[
            "cognition_skill_discover",
            "cognition_skill_propose",
            "cognition_openshell_status",
        ],
    );

    crate::public_api::ensure_public_api(&mut names);
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
    let mut names: HashSet<String> = manuscript_tools
        .iter()
        .filter(|tool| tool_allowed(tool, &intent_allow))
        .map(|tool| tool.to_string())
        .collect();
    crate::public_api::ensure_public_api(&mut names);
    names
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
        assert!(names.contains("cognition_capability"));
        assert!(names.contains("cognition_ui_build"));
        assert!(names.contains("cognition_ui_scene"));
        assert!(names.contains("cognition_ui_present"));
        assert!(names.contains("cognition_store_write"));
        assert!(names.contains("cognition_environment_get"));
        assert!(!names.contains("cognition_memory_calibrate"));
        assert!(names.contains("cognition_identity_recall"));
        assert!(!names.contains("cognition_identity_remember"));
        assert!(names.contains("cognition_openshell_status"));
        assert!(names.contains("cognition_openshell_sandbox_run"));
        assert!(names.contains("cognition_skill_discover"));
        assert!(names.contains("cognition_skill_probe"));
        assert!(names.contains("cognition_store_write"));
        assert!(names.contains("cognition_calendar_list"));
        assert!(names.contains("cognition_calendar_create"));
        assert!(names.contains("cognition_memory_store"));
    }

    #[test]
    fn manuscript_allowlist_intersects_intent_tools() {
        let tools = vec![
            "cognition_identity_recall".to_string(),
            "cognition_memory_context".to_string(),
            "cognition_capability".to_string(),
            "cognition_spawn_turn_worker".to_string(),
        ];
        let allow = worker_allowlist_for_intent_and_tools(TurnWorkerIntent::Research, &tools);
        assert!(allow.contains("cognition_identity_recall"));
        assert!(allow.contains("cognition_memory_context"));
        assert!(allow.contains("cognition_capability"));
        assert!(!allow.contains("cognition_spawn_turn_worker"));
        assert!(!allow.contains("cognition_grapheme_run"));
    }

    #[test]
    fn host_scheduler_has_memory_runtime_and_catalog_not_execution() {
        let names = host_bus_tool_names();
        assert!(names.contains("cognition_memory_calibrate"));
        assert!(names.contains("cognition_identity_propose"));
        assert!(names.contains("cognition_identity_recall"));
        assert!(names.contains("cognition_identity_remember"));
        assert!(names.contains("cognition_job_enqueue"));
        assert!(names.contains("cognition_spawn_turn_worker"));
        assert!(names.contains("cognition_capability"));
        assert!(names.contains("cognition_runtime_workflow_run"));
        assert!(names.contains("cognition_store_read"));
        assert!(names.contains("cognition_calendar_list"));
        assert!(names.contains("cognition_calendar_create"));
        assert!(names.contains("cognition_workshop_steer"));
        assert!(!names.contains("cognition_grapheme_run"));
        assert!(!names.contains("cognition_capability_invoke"));
        assert!(!names.contains("cognition_mcp_invoke"));
        assert!(!names.contains("cognition_turn_prepare_final"));
        assert!(names.contains("cognition_turn_begin_work"));
        assert!(names.contains("cognition_turn_update_user"));
        assert!(names.contains("cognition_turn_finish"));
        assert!(names.contains("cognition_tools_discover"));
        assert!(names.contains("cognition_web_search"));
        assert!(names.contains("cognition_browser_fetch"));
        assert!(!names.contains("cognition_environment_get"));
        assert!(!names.contains("cognition_ui_present"));
    }

    #[test]
    fn general_worker_includes_environment_tools() {
        let names = allowed_tool_names_for_intent(TurnWorkerIntent::General);
        assert!(names.contains("cognition_environment_get"));
        assert!(names.contains("cognition_component_create"));
        assert!(names.contains("cognition_turn_update_user"));
        assert!(names.contains("cognition_tools_discover"));
    }
}
