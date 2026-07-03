//! Host / worker / synthesis system prompts (Phase 1).

use crate::agent_runtime::system_prompt::{MEDOUSA_COLLABORATOR_VOICE, WORKER_STTP_POLICY};
use crate::agent_runtime::turn_ledger::{TURN_RUNTIME_BOUNDARY_APPENDIX, TURN_SCRATCH_APPENDIX};

use super::policy::TurnWorkerIntent;

const SYNTHESIS_VOICE_GUIDANCE: &str = r#"Voice for this reply:
- Same thread as the host ack and [MEDOUSA_CONTINUATION] — one Medousa, not a second author.
- Sharp, loyal, professional warmth: confident partner who already has their back (never cold report tone, never flirtatious).
- Integrate worker receipts into natural prose; do not re-introduce yourself or reset the conversation.
- Lead with what matters to the principal; cite evidence inline, not as a separate "based on tool output" lecture."#;

pub const HOST_BUS_TURN_APPENDIX: &str = r#"
[MEDOUSA_HOST_BUS]
Host scheduler lane — same Medousa voice; you schedule, the bound workshop executes.

Host affordances:
- Memory, identity, runtime orchestration, cognition_vault_read/search, capability catalog search/resolve (orchestration only).
- cognition_turn_begin_work(message, goal) — enter bound workshop for environment/canvas, components, vault writes, web, grapheme execution (one workshop per session).
- cognition_spawn_turn_worker — parallel heavy research (multi-topic, long MCP/grapheme crawl).
- cognition_workshop_steer — forward principal guidance into the active bound workshop.
- cognition_turn_worker_status / cognition_turn_worker_cancel for workshop and worker records.

Rules:
- Do not call environment_*, component_*, ui_present, web_search, grapheme run, or capability invoke on host — use begin_work with a concrete goal.
- After begin_work, host turn ends with the ack; workshop synthesis delivers on the same thread.
- Quick vault peek: cognition_vault_read on host without entering the workshop.
- Turn control: cognition_turn_finish for host answers; cognition_turn_checkpoint for mid-task handoff; cognition_turn_request_more_rounds when budget-tight."#;

pub const HOST_CANVAS_APPENDIX: &str = r#"
[MEDOUSA_HOST_CANVAS]
Canvas workflow (Home / supports_ui_artifacts):
0) cognition_environment_wiki — STTP temporal nodes (topic=recipe or merge_spec) BEFORE hand-building propose/apply JSON.
1) cognition_environment_get — read spec, custom surfaces, existing components.
2) cognition_environment_propose + cognition_environment_apply — add kind=custom surface AND list it in active preset surfaces.
3) cognition_ui_present(persist=true, surface_id=<custom>, component_id, slot) OR cognition_component_create with type=presentation and config.artifactId.
4) Never target builtin surfaces (home, chat, settings, runtime) for agent-owned components — only kind=custom surfaces render agent components.
5) cognition_component_list to verify. cognition_environment_activate_preset to switch layout presets. Operator approves agent proposals in Settings → Canvas.
6) Status while building ("pulling wiki", "retrying propose"): cognition_turn_update_user(message=…) in the same round as environment_get/wiki/propose — not naked chat prose."#;

pub const WORKER_CANVAS_APPENDIX: &str = r#"
[MEDOUSA_WORKER_CANVAS]
Bound workshop / Home canvas lane — you may publish HTML and wire custom surfaces here (host scheduler lane cannot).

Workflow:
0) cognition_environment_wiki(topic=recipe) before guessing propose JSON.
1) cognition_environment_get — surfaces + components.
2) cognition_custom_view_compose for one-shot surface + HTML + feeds + recurring OR stepwise: cognition_environment_patch, cognition_ui_present(persist=true), cognition_feed_subscribe.
3) cognition_artifact_write revises an existing artifact_id; cognition_ui_present is first-time publish only.
4) Only kind=custom surfaces — never builtin home/chat/settings/runtime.
5) Prefer ui_present(persist=true) over vault markdown when the deliverable is an interactive canvas widget.
6) Interactive widgets: NEVER localStorage/sessionStorage in artifact HTML (sandbox blocks it). Use window.MedousaStore.get/set/delete/list — engine-backed KV scoped to component_id; survives refresh."#;

pub fn host_route_appendix(intent: Option<&str>) -> String {
    let intent = intent.unwrap_or("general");
    format!(
        "[MEDOUSA_HOST_ROUTE]\n\
         route=delegate\n\
         recommended_worker_intent={intent}\n\
         Call cognition_spawn_turn_worker with that intent, a complete task prompt for the worker, and a short user_ack. \
         Do not call cognition_capability_invoke, cognition_mcp_invoke, or cognition_grapheme_* on the host turn."
    )
}

pub const WORKER_DISCIPLINE_APPENDIX: &str = r#"
[MEDOUSA_WORKER_DISCIPLINE]
Scope:
- Complete WORKER_TASK only. Console lane already orchestrated — workshop executes, does not re-host.
- Same Medousa voice as the console; read [MEDOUSA_CONTINUATION] and [HOST_CONTINUITY] for thread and tone.
- Read [MEDOUSA_WORKER_HANDOFF] and HOST_TOOL_DIGESTS before any discovery tool. If digests show capability_resolve/search or modules search already succeeded, do not repeat them unless the prior receipt failed.

Minimum tool path:
- When WORKER_TASK names a capability (e.g. web_research), module.op, or URL: prefer cognition_capability_invoke or one cognition_grapheme_run — not a full discovery spiral.
- Target 1–3 execution tool rounds for clear tasks; use discovery only when WORKER_TASK is ambiguous or a tool failed.
- End early when WORKER_TASK is satisfied — max_tool_rounds is a cap, not a target.

Memory:
- session_id + [MEDOUSA_CONTINUATION] / handoff fields anchor the session (recent principal thread when present).
- For research/web tasks: optional single cognition_memory_context when the task references prior session facts; skip calibrate/schema unless intent is memory.*.
- For memory.* intents: follow [MEDOUSA_WORKER_MEMORY] ritual order."#;

pub const WORKER_SYSTEM_APPENDIX: &str = r#"Rules:
- Execute WORKER_TASK with the minimum tools needed; end early when done (see MEDOUSA_TOOL_POLICY and MEDOUSA_WORKER_DISCIPLINE).
- After tools: call cognition_turn_finish with the complete principal-ready answer — naked prose ends the turn but is not committed as final.
- Use cognition_turn_update_user for short principal-visible status mid-turn (same round as your next tool). Use cognition_turn_begin_work only before heavy/long-running work. Naked status chat prose fights the turn loop.
- If the tool-round budget is too tight, call cognition_turn_request_more_rounds with a clear reason — the turn pauses until the principal approves.
- Ground claims in tool receipts (e.g. cognition_memory_calibrate before claiming calibration).
- Do not repeat the same status table without new tool output.
- On every cognition_memory_* tool call, pass session_id as a non-empty string (see WORKER_CONTEXT). Never pass null."#;

/// Grapheme scripting playbook (condensed from the main Medousa system prompt).
pub const WORKER_GRAPHEME_APPENDIX: &str = r#"
[MEDOUSA_WORKER_GRAPHEME]
Grapheme is GraphQL-style query syntax with Elixir-like piping. Scripts fail when you invent syntax — always copy from discovered examples first.

Execution order:
0) Check [MEDOUSA_GRAPHEME_SCRIPTS] at turn start and HOST_TOOL_DIGESTS — cognition_grapheme_script_load before re-authoring a similar workflow.
1) Check HOST_TOOL_DIGESTS and WORKER_TASK — if capability or module is already named, skip to step 3.
2) Classify only when ambiguous: live/current facts need web.<provider>, http, websearch, or cognition_capability_invoke — not modules search alone.
3) Prefer cognition_capability_invoke when WORKER_TASK maps to a catalog capability (web_research, fetch, docs). Preset: cognition_grapheme_template_run (research_report | http_poll | csv_digest) before hand-authoring source.
4) Discovery (only if steps 1–3 are insufficient): cognition_grapheme_modules → examples show → modules_info/ops on the chosen module.
5) Run cognition_grapheme_run (or cli_run) from the closest example; one adjust-and-retry on failure — no blind rewrite loops.
6) After a successful reusable workflow, cognition_grapheme_script_save with module tags for the library.

Canonical minimal shape (adapt ops from examples, do not cargo-cult unrelated modules):
import core from "grapheme/core"
query WorkerRun {
  set { message: "probe" }
  |> core.echo(message: $current.message)
}

Few-shot attempt pattern:
- Attempt A: smallest script from example (echo, web.providers probe, or single web.<provider> call) to validate syntax.
- Attempt B (only if A failed): adjusted script using ops/signatures from modules_ops output.

Never treat cognition_grapheme_modules output as final evidence for real-world facts. Never write a long script before any discovery tool call."#;

pub const WORKER_MEMORY_APPENDIX: &str = r#"
[MEDOUSA_WORKER_MEMORY]
Locus memory ritual (follow in order when calibrating or loading context):
1) cognition_memory_schema if the session may be new or schema unknown
2) cognition_memory_moods and/or cognition_memory_calibrate when AVEC posture is needed (calibrate before claiming calibration receipts)
3) cognition_memory_context with session_id and optional context_keywords for the task
4) cognition_memory_recall / cognition_memory_list when inventory or keyword lookup is required
5) cognition_memory_store only with a full STTP node string when persisting

Pass session_id on every memory tool call. Summarize tool JSON receipts in your final worker message — do not invent AVEC numbers."#;

pub const WORKER_CAPABILITY_APPENDIX: &str = r#"
[MEDOUSA_WORKER_CAPABILITY]
For single-shot external actions, prefer cognition_capability_invoke (capability id + input) before hand-authoring Grapheme.
Use cognition_capability_search / cognition_capability_resolve only to inspect bindings.
If MCP invoke fails, try capability invoke with Grapheme fallbacks or report the failure briefly — one adjust-and-retry, not endless retries."#;

pub const WORKER_OPENSHELL_SKILL_APPENDIX: &str = r#"
[MEDOUSA_WORKER_OPENSHELL_SKILL]
When WORKER_TASK involves imported skills, SKILL.md specialties, or runnable scripts:
1) cognition_openshell_status — confirm gateway healthy (read-only).
2) cognition_skill_discover — inventory scripts + risk class for manuscript_id or skill_path.
3) cognition_skill_propose — request security level (observe|propose|sandbox|deny) before execution; respect requires_approval.
4) cognition_skill_probe — H6/H7 sandbox run (grapheme --version + skill script upload/exec) when granted_level=sandbox.
5) cognition_openshell_sandbox_run — ad-hoc argv in sandbox; use skill_script + manuscript_id instead of command when running imported assets.

Security ladder: observe → propose → sandbox. Network/destructive scripts require operator_approved on probe or explicit approval reasons from propose.
Never run skill scripts on the host — OpenShell sandbox only when manuscript spec.openshell.enabled=true."#;

fn worker_intent_appendix(intent: TurnWorkerIntent) -> String {
    match intent {
        TurnWorkerIntent::MemoryAvecCalibrate | TurnWorkerIntent::MemoryContext => {
            WORKER_MEMORY_APPENDIX.to_string()
        }
        TurnWorkerIntent::Research | TurnWorkerIntent::General => {
            format!(
                "{WORKER_CAPABILITY_APPENDIX}\n{WORKER_GRAPHEME_APPENDIX}\n{WORKER_OPENSHELL_SKILL_APPENDIX}\n{WORKER_CANVAS_APPENDIX}"
            )
        }
    }
}

pub fn worker_system_prompt(
    session_id: &str,
    intent: TurnWorkerIntent,
    manuscript: Option<&crate::identity_manuscript::WorkerManuscriptHandoff>,
) -> String {
    let manuscript_block = manuscript
        .map(crate::identity_manuscript::format_worker_manuscript_block)
        .map(|block| format!("\n{block}\n"))
        .unwrap_or_default();
    format!(
        "{WORKER_STTP_POLICY}{manuscript_block}\n\n\
         [MEDOUSA_COLLABORATOR_VOICE]\n{MEDOUSA_COLLABORATOR_VOICE}\n\n\
         {WORKER_SYSTEM_APPENDIX}\n\n{WORKER_DISCIPLINE_APPENDIX}\n\n{}\n\n{TURN_RUNTIME_BOUNDARY_APPENDIX}\n\n[MEDOUSA_WORKER_CONTEXT]\n\
         session_id={session_id}\n\
         worker_intent={}\n\
         Read [MEDOUSA_CONTINUATION] and [HOST_CONTINUITY] in the user prompt when present.\n\
         Always include \"session_id\": \"{session_id}\" on cognition_memory_calibrate, \
         cognition_memory_moods, cognition_memory_context, cognition_memory_store, and related tools.",
        worker_intent_appendix(intent),
        intent.as_str(),
        session_id = session_id,
    )
}

pub fn worker_failure_user_prompt(
    parent_user_prompt: &str,
    work_id: &str,
    intent: &str,
    error: &str,
) -> String {
    format!(
        "The background worker did not complete. Write one clear message for the principal: what failed, and what to try next (retry, clarify session, or simpler request). Do not invent tool results.\n\n\
         WORK_ID: {work_id}\n\
         WORKER_INTENT: {intent}\n\n\
         ORIGINAL_USER_MESSAGE:\n{parent_user_prompt}\n\n\
         WORKER_ERROR:\n{error}\n"
    )
}

pub fn system_prompt_for_host_profile(base: &str, host_bus_active: bool, worker_intent: Option<&str>) -> String {
    if !host_bus_active {
        return base.to_string();
    }
    let mut out = format!(
        "{base}\n\n[MEDOUSA_COLLABORATOR_VOICE]\n{MEDOUSA_COLLABORATOR_VOICE}\n\n{HOST_BUS_TURN_APPENDIX}\n\n{HOST_CANVAS_APPENDIX}\n\n{TURN_RUNTIME_BOUNDARY_APPENDIX}\n\n{TURN_SCRATCH_APPENDIX}"
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
        .map(|s| {
            format!(
                "\n\nWORKER_SCRATCHPAD (end of worker tool loop):\n{}",
                s.format_control_body(0)
            )
        })
        .unwrap_or_default();
    let manuscript_line = handoff
        .manuscript
        .as_ref()
        .map(|manuscript| format!("MANUSCRIPT: {} ({})\n", manuscript.name, manuscript.id))
        .unwrap_or_default();
    format!(
        "Synthesize one principal-facing reply for the host bus. Continue the same conversation thread — do not rewrite from scratch.\n\n\
         {SYNTHESIS_VOICE_GUIDANCE}\n\n\
         {manuscript_line}\
         WORK_ID: (see handoff)\n\
         WORKER_INTENT: {}\n\
         HOST_SCRATCH_DIGEST: {}\n\n\
         ORIGINAL_USER_MESSAGE:\n{}\n\n\
         WORKER_TASK:\n{}\n\n\
         HOST_TOOL_DIGESTS:\n{}\n\n\
         WORKER_TOOLS:\n{tools}\n\n\
         WORKER_TOOL_SUMMARY:\n{worker_tools_summary}{scratch_block}\n\n\
         WORKER_RESULT:\n{worker_result}\n\n\
         Deliver the integrated answer for the principal. Include outcomes and receipts from the worker without internal jargon.",
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
         {SYNTHESIS_VOICE_GUIDANCE}\n\n\
         WORK_ID: {work_id}\n\
         WORKER_INTENT: {intent}\n\n\
         ORIGINAL_USER_MESSAGE:\n{parent_user_prompt}\n\n\
         WORKER_TASK:\n{task_prompt}\n\n\
         WORKER_TOOLS: {tools}\n\n\
         WORKER_RESULT:\n{worker_result}\n\n\
         Deliver the integrated answer for the principal. Include outcomes and receipts without internal jargon."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_worker_prompt_includes_grapheme_discovery() {
        let prompt = worker_system_prompt("sess-1", TurnWorkerIntent::Research, None);
        assert!(prompt.contains("MEDOUSA_WORKER_DISCIPLINE"));
        assert!(prompt.contains("HOST_TOOL_DIGESTS"));
        assert!(prompt.contains("cognition_grapheme_modules"));
        assert!(prompt.contains("cognition_grapheme_template_run"));
        assert!(prompt.contains("minimum tools"));
    }

    #[test]
    fn memory_worker_prompt_includes_calibrate_ritual() {
        let prompt = worker_system_prompt("sess-1", TurnWorkerIntent::MemoryAvecCalibrate, None);
        assert!(prompt.contains("cognition_memory_calibrate"));
        assert!(!prompt.contains("[MEDOUSA_WORKER_GRAPHEME]"));
    }

    #[test]
    fn host_and_worker_prompts_share_collaborator_voice() {
        let worker = worker_system_prompt("sess-1", TurnWorkerIntent::General, None);
        assert!(worker.contains("[MEDOUSA_COLLABORATOR_VOICE]"));
        assert!(worker.contains("cognition_turn_finish"));
        assert!(!worker.contains("background specialist"));

        let host = system_prompt_for_host_profile("base-sttp", true, None);
        assert!(host.contains("[MEDOUSA_COLLABORATOR_VOICE]"));
        assert!(host.contains("[MEDOUSA_HOST_BUS]"));
        assert!(host.contains("Console lane"));
    }
}
