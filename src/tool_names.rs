//! Canonical cognition tool names (`StasisTool::name()` values). Keep in sync with registrations.

/// All cognition tools registered in `assemble_tui_runtime` (snake_case canonical).
pub const REGISTERED_COGNITION_TOOLS: &[&str] = &[
    "cognition_job_enqueue",
    "cognition_grapheme_run",
    "cognition_grapheme_modules",
    "cognition_grapheme_modules_info",
    "cognition_grapheme_modules_ops",
    "cognition_grapheme_examples",
    "cognition_grapheme_cli_run",
    "cognition_grapheme_promote_to_job",
    "cognition_grapheme_promote_to_recurring",
    "cognition_grapheme_promote_last_run_to_recurring",
    "cognition_identity_context",
    "cognition_identity_propose",
    "cognition_identity_commit",
    "cognition_identity_recall",
    "cognition_identity_remember",
    "cognition_manuscript_list",
    "cognition_manuscript_resolve",
    "cognition_memory_schema",
    "cognition_memory_moods",
    "cognition_memory_calibrate",
    "cognition_memory_context",
    "cognition_memory_list",
    "cognition_memory_recall",
    "cognition_memory_store",
    "cognition_utility_time_now",
    "cognition_utility_day_of_week",
    "cognition_utility_uuid",
    "cognition_spawn_turn_worker",
    "cognition_turn_worker_status",
    "cognition_turn_worker_cancel",
    "cognition_turn_prepare_final",
    "cognition_runtime_recurring_preview",
    "cognition_runtime_jobs_status",
    "cognition_runtime_jobs_list",
    "cognition_runtime_jobs_cancel",
    "cognition_runtime_recurring_list",
    "cognition_runtime_recurring_doctor",
    "cognition_runtime_recurring_register",
    "cognition_runtime_recurring_pause",
    "cognition_runtime_recurring_cancel",
    "cognition_runtime_delivery_status",
    "cognition_runtime_workflow_run",
    "cognition_runtime_workflow_schedule",
    "cognition_runtime_workflow_status",
    "cognition_runtime_workflow_cancel",
    "cognition_runtime_workflow_plan",
    "cognition_capability_resolve",
    "cognition_capability_list",
    "cognition_capability_search",
    "cognition_mcp_discover",
    "cognition_mcp_invoke",
    "cognition_mcp_servers",
    "cognition_capability_invoke",
    "cognition_mcp_promote_to_job",
    "cognition_grapheme_template_run",
    "cognition_openshell_status",
    "cognition_openshell_sandbox_run",
    "cognition_skill_discover",
    "cognition_skill_propose",
    "cognition_skill_probe",
    "cognition_vault_list",
    "cognition_vault_read",
    "cognition_vault_search",
    "cognition_vault_write",
];

/// Grapheme first-class tools (templates + discovery + run) — worker execution surface.
pub const WORKER_GRAPHEME_EXECUTION_TOOLS: &[&str] = &[
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
        for name in REGISTERED_COGNITION_TOOLS {
            assert!(!name.contains('.'), "use snake_case canonical name: {name}");
            assert_eq!(
                sanitize_tool_advertised_name(name),
                *name,
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
        ] {
            assert!(tool_allowed(tool, &host), "host bus missing {tool}");
        }
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
        let set: HashSet<_> = REGISTERED_COGNITION_TOOLS.iter().copied().collect();
        assert_eq!(set.len(), REGISTERED_COGNITION_TOOLS.len());
    }
}
