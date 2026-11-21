//! Host and worker prompt composition.

use super::policy::TurnWorkerIntent;

pub fn host_route_appendix(intent: Option<&str>) -> String {
    let intent = intent.unwrap_or("general");
    format!("[MEDOUSA_HUD]\nrecommended_worker_intent={intent}")
}

pub fn worker_system_prompt(
    session_id: &str,
    intent: TurnWorkerIntent,
    manuscript: Option<&crate::identity_manuscript::WorkerManuscriptHandoff>,
    supports_ui_artifacts: bool,
    supports_liquid_markdown: bool,
) -> String {
    worker_system_prompt_for_parent_mode(
        session_id,
        intent,
        manuscript,
        supports_ui_artifacts,
        supports_liquid_markdown,
        None,
    )
}

pub fn worker_system_prompt_for_parent_mode(
    session_id: &str,
    intent: TurnWorkerIntent,
    manuscript: Option<&crate::identity_manuscript::WorkerManuscriptHandoff>,
    supports_ui_artifacts: bool,
    supports_liquid_markdown: bool,
    parent_agent_mode: Option<&str>,
) -> String {
    let policy_mode = if intent == TurnWorkerIntent::Coder {
        crate::agent_runtime::prompt_policy::SttpPolicyMode::CoderWork
    } else if parent_agent_mode == Some("teacher") {
        crate::agent_runtime::prompt_policy::SttpPolicyMode::Teacher
    } else {
        // The parent's mode does not confer its Forge/tool authority on a
        // research or general peer. Match the worker's actual environment.
        crate::agent_runtime::prompt_policy::SttpPolicyMode::General
    };
    let policy = crate::agent_runtime::prompt_policy::compile_sttp_policy(
        crate::agent_runtime::prompt_policy::SttpPolicySelection::new(
            policy_mode,
            crate::agent_runtime::prompt_policy::SttpPolicyActor::Worker,
        ),
    )
    .expect("built-in STTP worker policy must compile")
    .rendered;
    let manuscript_block = manuscript
        .map(crate::identity_manuscript::format_worker_manuscript_block)
        .map(|block| format!("\n{block}\n"))
        .unwrap_or_default();
    format!(
        "{policy}{manuscript_block}\n\n[MEDOUSA_HUD]\n\
         session_id={session_id}\n\
         worker_intent={}\n\
         ui_artifacts={}\n\
         liquid_markdown={}\n\
         parent_agent_mode={}\n\
         handback_contract=End with turn.finish needs_synthesis=false when your prose is a complete principal-facing answer. Use needs_synthesis=true only when the host must combine your partial evidence with other work before replying.",
        intent.as_str(),
        supports_ui_artifacts,
        supports_liquid_markdown,
        parent_agent_mode.unwrap_or("general"),
    )
}

pub fn host_system_prompt_for_parent_mode(parent_agent_mode: Option<&str>) -> String {
    let policy_mode = match parent_agent_mode {
        Some("coder") => crate::agent_runtime::prompt_policy::SttpPolicyMode::CoderWork,
        Some("assistant") => crate::agent_runtime::prompt_policy::SttpPolicyMode::Assistant,
        Some("teacher") => crate::agent_runtime::prompt_policy::SttpPolicyMode::Teacher,
        _ => crate::agent_runtime::prompt_policy::SttpPolicyMode::General,
    };
    crate::agent_runtime::prompt_policy::compile_sttp_policy(
        crate::agent_runtime::prompt_policy::SttpPolicySelection::new(
            policy_mode,
            crate::agent_runtime::prompt_policy::SttpPolicyActor::Host,
        ),
    )
    .expect("built-in STTP host policy must compile")
    .rendered
}

pub fn system_prompt_for_host_profile(
    base: &str,
    host_bus_active: bool,
    supports_ui_artifacts: bool,
    supports_liquid_markdown: bool,
    worker_intent: Option<&str>,
) -> String {
    if !host_bus_active {
        return base.to_string();
    }
    let mut out = format!(
        "{base}\n\n[MEDOUSA_HUD]\nhost_bus=active\nui_artifacts={supports_ui_artifacts}\nliquid_markdown={supports_liquid_markdown}"
    );
    if let Some(intent) = worker_intent {
        out.push('\n');
        out.push_str(&host_route_appendix(Some(intent)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_prompt_uses_exact_worker_policy_and_plain_hud() {
        let prompt = worker_system_prompt("sess-1", TurnWorkerIntent::Research, None, false, false);
        assert!(prompt.contains("p1_core(.99)"));
        assert!(prompt.contains("p2_mode_general(.99)"));
        assert!(prompt.contains("p3_actor_worker(.99)"));
        assert!(!prompt.contains("p3_actor_host(.99)"));
        assert!(prompt.contains("[MEDOUSA_HUD]"));
        assert!(prompt.contains("worker_intent=research"));
    }

    #[test]
    fn teacher_policy_survives_worker_and_host_handoffs() {
        let worker = worker_system_prompt_for_parent_mode(
            "sess-teacher",
            TurnWorkerIntent::Research,
            None,
            false,
            false,
            Some("teacher"),
        );
        assert!(worker.contains("p2_mode_teacher(.99)"));
        assert!(worker.contains("p3_actor_worker(.99)"));
        assert!(!worker.contains("p2_mode_general(.99)"));

        let host = host_system_prompt_for_parent_mode(Some("teacher"));
        assert!(host.contains("p2_mode_teacher(.99)"));
        assert!(host.contains("p3_actor_host(.99)"));
        assert!(!host.contains("p2_mode_general(.99)"));
    }

    #[test]
    fn worker_intent_does_not_swap_the_policy_slice() {
        let prompt = worker_system_prompt(
            "sess-1",
            TurnWorkerIntent::MemoryAvecCalibrate,
            None,
            false,
            false,
        );
        assert!(prompt.contains("worker_intent=memory.avec_calibrate"));
        assert!(prompt.contains("p2_mode_general(.99)"));
        assert!(!prompt.contains("MEDOUSA_WORKER_GRAPHEME"));
    }

    #[test]
    fn coder_worker_selects_coder_work_with_the_exact_actor() {
        let worker = worker_system_prompt_for_parent_mode(
            "sess-1",
            TurnWorkerIntent::Coder,
            None,
            false,
            false,
            Some("coder"),
        );
        assert!(worker.contains("p2_mode_coder_work(.99)"));
        assert!(worker.contains("p3_actor_worker(.99)"));
        let host = host_system_prompt_for_parent_mode(Some("coder"));
        assert!(host.contains("p2_mode_coder_work(.99)"));
        assert!(host.contains("p3_actor_host(.99)"));
    }

    #[test]
    fn research_peer_does_not_claim_parent_coder_authority() {
        let worker = worker_system_prompt_for_parent_mode(
            "session",
            TurnWorkerIntent::Research,
            None,
            false,
            false,
            Some("coder"),
        );
        assert!(worker.contains("p2_mode_general(.99)"));
        assert!(!worker.contains("p2_mode_coder_work(.99)"));
    }

    #[test]
    fn capabilities_are_hud_facts() {
        let host = system_prompt_for_host_profile("base-sttp", true, true, false, None);
        assert!(host.contains("ui_artifacts=true"));
        assert!(host.contains("liquid_markdown=false"));
        assert!(!host.contains("[MEDOUSA_UI_ARTIFACTS]"));
        let worker = worker_system_prompt("sess-1", TurnWorkerIntent::General, None, false, true);
        assert!(worker.contains("liquid_markdown=true"));
    }
}
