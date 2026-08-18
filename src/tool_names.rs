//! Canonical first-party tool inventory used by the contract migration audit.

/// Manual `StasisTool` contracts that must shrink as typed tools replace them.
pub const LEGACY_MANUAL_TOOL_CONTRACTS: &[&str] = &[];

/// Typed contracts already excluded from the manual migration allowlist.
pub const TYPED_TOOL_CONTRACTS: &[&str] = &[
    "cognition_browser_act",
    "cognition_browser_fetch",
    "cognition_browser_snapshot",
    "cognition_calendar_create",
    "cognition_calendar_delete",
    "cognition_calendar_export",
    "cognition_calendar_import",
    "cognition_calendar_list",
    "cognition_calendar_update",
    "cognition_chat_history_read",
    "cognition_chat_history_search",
    "cognition_capability_list",
    "cognition_capability_invoke",
    "cognition_capability_resolve",
    "cognition_capability_search",
    "cognition_code_definition",
    "cognition_code_diagnostics",
    "cognition_code_hover",
    "cognition_code_symbols",
    "cognition_coder_shell_run",
    "cognition_coder_shell_status",
    "cognition_component_create",
    "cognition_component_delete",
    "cognition_component_get",
    "cognition_component_list",
    "cognition_component_update",
    "cognition_context_follow_pointer",
    "cognition_context_list_pointers",
    "cognition_custom_view_doctor",
    "cognition_custom_view_compose",
    "cognition_detamu_code_avec",
    "cognition_detamu_files",
    "cognition_detamu_find",
    "cognition_detamu_impact",
    "cognition_detamu_status",
    "cognition_environment_activate_preset",
    "cognition_environment_apply",
    "cognition_environment_get",
    "cognition_environment_patch",
    "cognition_environment_propose",
    "cognition_environment_wiki",
    "cognition_feed_publish",
    "cognition_feed_subscribe",
    "cognition_grapheme_cli_run",
    "cognition_grapheme_examples",
    "cognition_grapheme_modules",
    "cognition_grapheme_modules_info",
    "cognition_grapheme_modules_ops",
    "cognition_grapheme_promote_last_run_to_recurring",
    "cognition_grapheme_promote_to_job",
    "cognition_grapheme_promote_to_recurring",
    "cognition_grapheme_run",
    "cognition_grapheme_template_run",
    "cognition_identity_commit",
    "cognition_identity_context",
    "cognition_identity_propose",
    "cognition_identity_recall",
    "cognition_identity_remember",
    "cognition_job_enqueue",
    "cognition_intent_resolve",
    "cognition_layout_apply",
    "cognition_layout_get",
    "cognition_layout_reset",
    "cognition_manuscript_list",
    "cognition_manuscript_overlay_list",
    "cognition_manuscript_overlay_propose",
    "cognition_manuscript_resolve",
    "cognition_mcp_discover",
    "cognition_mcp_invoke",
    "cognition_mcp_promote_to_job",
    "cognition_mcp_servers",
    "cognition_memory_calibrate",
    "cognition_memory_context",
    "cognition_memory_evict",
    "cognition_memory_list",
    "cognition_memory_moods",
    "cognition_memory_recall",
    "cognition_memory_schema",
    "cognition_memory_store",
    "cognition_memory_tags",
    "cognition_openshell_sandbox_run",
    "cognition_project_bind",
    "cognition_project_create",
    "cognition_project_list",
    "cognition_runtime_delivery_status",
    "cognition_runtime_jobs_cancel",
    "cognition_runtime_jobs_status",
    "cognition_runtime_jobs_list",
    "cognition_runtime_recurring_cancel",
    "cognition_runtime_recurring_doctor",
    "cognition_runtime_recurring_list",
    "cognition_runtime_recurring_pause",
    "cognition_runtime_recurring_preview",
    "cognition_runtime_recurring_register",
    "cognition_runtime_workflow_cancel",
    "cognition_runtime_workflow_plan",
    "cognition_runtime_workflow_run",
    "cognition_runtime_workflow_schedule",
    "cognition_runtime_workflow_status",
    "cognition_openshell_status",
    "cognition_shell_session_interrupt",
    "cognition_shell_session_run",
    "cognition_shell_session_status",
    "cognition_shell_status",
    "cognition_shell_run",
    "cognition_skill_discover",
    "cognition_skill_probe",
    "cognition_skill_propose",
    "cognition_spawn_turn_worker",
    "cognition_store_read",
    "cognition_store_write",
    "cognition_tool_history_detail",
    "cognition_tool_history_summary",
    "cognition_tools_discover",
    "cognition_turn_checkpoint",
    "cognition_turn_begin_work",
    "cognition_turn_finish",
    "cognition_turn_prepare_final",
    "cognition_turn_propose_mode",
    "cognition_turn_request_more_rounds",
    "cognition_turn_update_user",
    "cognition_turn_worker_cancel",
    "cognition_turn_worker_status",
    "cognition_ui_build",
    "cognition_ui_present",
    "cognition_ui_scene",
    "cognition_utility_day_of_week",
    "cognition_utility_time_now",
    "cognition_utility_uuid",
    "cognition_web_search",
    "cognition_workshop_steer",
];

pub fn registered_cognition_tools() -> impl Iterator<Item = &'static str> {
    LEGACY_MANUAL_TOOL_CONTRACTS
        .iter()
        .chain(TYPED_TOOL_CONTRACTS.iter())
        .copied()
}

/// Grapheme first-class tools (templates + discovery + run) — worker execution surface.
pub const WORKER_GRAPHEME_EXECUTION_TOOLS: &[&str] = &[
    "cognition_web_search",
    "cognition_grapheme_template_run",
    "cognition_grapheme_modules",
    "cognition_grapheme_modules_info",
    "cognition_grapheme_modules_ops",
    "cognition_grapheme_examples",
    "cognition_grapheme_run",
    "cognition_grapheme_cli_run",
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::agent_runtime::turn_worker::{
        TurnWorkerIntent, allowed_tool_names_for_intent, host_bus_tool_names, tool_allowed,
    };
    use crate::tool_aliases::sanitize_tool_advertised_name;

    #[test]
    fn registered_names_are_stable_snake_case() {
        for name in registered_cognition_tools() {
            assert!(!name.contains('.'), "use snake_case canonical name: {name}");
            assert_eq!(
                sanitize_tool_advertised_name(name),
                name,
                "sanitized alias should match canonical: {name}"
            );
        }
    }

    #[test]
    fn research_worker_sees_grapheme_and_capability_execution() {
        let allow = allowed_tool_names_for_intent(TurnWorkerIntent::Research);
        for tool in WORKER_GRAPHEME_EXECUTION_TOOLS {
            assert!(
                tool_allowed(tool, &allow),
                "research allowlist missing {tool}"
            );
        }
        assert!(tool_allowed("cognition_capability_invoke", &allow));
        assert!(tool_allowed("cognition_mcp_discover", &allow));
    }

    #[test]
    fn general_worker_sees_capability_and_template_shortcuts() {
        let allow = allowed_tool_names_for_intent(TurnWorkerIntent::General);
        assert!(tool_allowed("cognition_capability_invoke", &allow));
        assert!(tool_allowed("cognition_grapheme_template_run", &allow));
        assert!(tool_allowed("cognition_grapheme_modules", &allow));
        assert!(tool_allowed("cognition_grapheme_examples", &allow));
    }

    #[test]
    fn host_bus_includes_skill_discover_and_openshell_status() {
        let host = host_bus_tool_names();
        assert!(tool_allowed("cognition_skill_discover", &host));
        assert!(tool_allowed("cognition_skill_propose", &host));
        assert!(tool_allowed("cognition_openshell_status", &host));
        assert!(!tool_allowed("cognition_skill_probe", &host));
        assert!(!tool_allowed("cognition_openshell_sandbox_run", &host));
    }

    #[test]
    fn host_bus_allowlist_matches_registered_runtime_and_memory_tools() {
        let host = host_bus_tool_names();
        for tool in [
            "cognition_memory_store",
            "cognition_job_enqueue",
            "cognition_runtime_workflow_run",
            "cognition_capability_search",
            "cognition_spawn_turn_worker",
            "cognition_turn_begin_work",
            "cognition_turn_finish",
        ] {
            assert!(tool_allowed(tool, &host), "host bus missing {tool}");
        }
        assert!(!tool_allowed("cognition_turn_prepare_final", &host));
        assert!(!tool_allowed("cognition_grapheme_run", &host));
        assert!(!tool_allowed("cognition_capability_invoke", &host));
        assert!(!tool_allowed("cognition_grapheme_template_run", &host));
        assert!(tool_allowed("cognition_identity_recall", &host));
        assert!(tool_allowed("cognition_identity_remember", &host));
        let research = allowed_tool_names_for_intent(TurnWorkerIntent::Research);
        assert!(tool_allowed("cognition_identity_recall", &research));
        assert!(!tool_allowed("cognition_identity_remember", &research));
    }

    #[test]
    fn no_duplicate_registered_entries() {
        let names = registered_cognition_tools().collect::<Vec<_>>();
        let set: HashSet<_> = names.iter().copied().collect();
        assert_eq!(set.len(), names.len());
    }
}
