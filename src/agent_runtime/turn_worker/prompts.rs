//! Host, worker, and synthesis prompt composition.

use super::policy::TurnWorkerIntent;

const SYNTHESIS_VOICE_GUIDANCE: &str = r#"Voice for this reply:
- Same thread as the host ack and [MEDOUSA_CONTINUATION] — one Medousa, not a second author.
- Sharp, loyal, professional warmth: confident partner who already has their back.
- Integrate worker receipts into natural prose; do not re-introduce yourself or reset the conversation.
- Lead with what matters to the principal; cite evidence inline."#;

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
    let policy_mode = if parent_agent_mode == Some("coder") {
        crate::agent_runtime::prompt_policy::SttpPolicyMode::CoderWork
    } else {
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
         parent_agent_mode={}",
        intent.as_str(),
        supports_ui_artifacts,
        supports_liquid_markdown,
        parent_agent_mode.unwrap_or("general"),
    )
}

pub fn host_system_prompt_for_parent_mode(parent_agent_mode: Option<&str>) -> String {
    let policy_mode = if parent_agent_mode == Some("coder") {
        crate::agent_runtime::prompt_policy::SttpPolicyMode::CoderWork
    } else {
        crate::agent_runtime::prompt_policy::SttpPolicyMode::General
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

pub fn worker_failure_user_prompt(
    parent_user_prompt: &str,
    work_id: &str,
    intent: &str,
    error: &str,
) -> String {
    format!(
        "The background worker did not complete. Write one clear message for the principal: what failed, and what to try next. Do not invent tool results.\n\n\
         WORK_ID: {work_id}\nWORKER_INTENT: {intent}\n\n\
         ORIGINAL_USER_MESSAGE:\n{parent_user_prompt}\n\nWORKER_ERROR:\n{error}\n"
    )
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

pub fn synthesis_user_prompt_with_handoff(
    handoff: &crate::agent_runtime::turn_context::WorkerHandoffCapsule,
    worker_scratch: Option<&crate::agent_runtime::turn_context::TurnScratchpad>,
    worker_result: &str,
    tool_names: &[String],
    worker_tools_summary: &str,
) -> String {
    let tools = if tool_names.is_empty() {
        "(none)".to_string()
    } else {
        tool_names.join(", ")
    };
    let scratch_block = worker_scratch
        .map(|scratch| {
            format!(
                "\n\nWORKER_SCRATCHPAD (end of worker tool loop):\n{}",
                scratch.format_control_body(0)
            )
        })
        .unwrap_or_default();
    let manuscript_line = handoff
        .manuscript
        .as_ref()
        .map(|manuscript| format!("MANUSCRIPT: {} ({})\n", manuscript.name, manuscript.id))
        .unwrap_or_default();
    format!(
        "Synthesize one principal-facing reply for the host bus. Continue the same conversation thread.\n\n\
         {SYNTHESIS_VOICE_GUIDANCE}\n\n{manuscript_line}\
         WORKER_INTENT: {}\nHOST_SCRATCH_DIGEST: {}\n\n\
         ORIGINAL_USER_MESSAGE:\n{}\n\nWORKER_TASK:\n{}\n\n\
         HOST_TOOL_DIGESTS:\n{}\n\nWORKER_TOOLS:\n{tools}\n\n\
         WORKER_TOOL_SUMMARY:\n{worker_tools_summary}{scratch_block}\n\n\
         WORKER_RESULT:\n{worker_result}\n\n\
         Deliver the integrated answer for the principal. Include outcomes and receipts without internal jargon.",
        handoff.intent,
        handoff.scratch_digest_hash,
        handoff.parent_user_prompt,
        handoff.task_prompt,
        handoff.host_tool_digests.join("\n"),
    )
}

pub fn synthesis_user_prompt(
    parent_user_prompt: &str,
    task_prompt: &str,
    work_id: &str,
    intent: &str,
    worker_result: &str,
    tool_names: &[String],
) -> String {
    let tools = if tool_names.is_empty() {
        "(none)".to_string()
    } else {
        tool_names.join(", ")
    };
    format!(
        "Synthesize one principal-facing reply for the host bus. Continue the same conversation thread.\n\n\
         {SYNTHESIS_VOICE_GUIDANCE}\n\nWORK_ID: {work_id}\nWORKER_INTENT: {intent}\n\n\
         ORIGINAL_USER_MESSAGE:\n{parent_user_prompt}\n\nWORKER_TASK:\n{task_prompt}\n\n\
         WORKER_TOOLS: {tools}\n\nWORKER_RESULT:\n{worker_result}\n\n\
         Deliver the integrated answer for the principal. Include outcomes and receipts without internal jargon."
    )
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
    fn coder_parent_selects_coder_work_with_the_exact_actor() {
        let worker = worker_system_prompt_for_parent_mode(
            "sess-1",
            TurnWorkerIntent::General,
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
    fn capabilities_are_hud_facts() {
        let host = system_prompt_for_host_profile("base-sttp", true, true, false, None);
        assert!(host.contains("ui_artifacts=true"));
        assert!(host.contains("liquid_markdown=false"));
        assert!(!host.contains("[MEDOUSA_UI_ARTIFACTS]"));
        let worker = worker_system_prompt("sess-1", TurnWorkerIntent::General, None, false, true);
        assert!(worker.contains("liquid_markdown=true"));
    }
}
