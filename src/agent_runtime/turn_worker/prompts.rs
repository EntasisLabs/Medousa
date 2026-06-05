//! Host / worker / synthesis system prompts (Phase 1).

use crate::agent_runtime::system_prompt::WORKER_STTP_POLICY;

use super::policy::TurnWorkerIntent;

pub const HOST_BUS_TURN_APPENDIX: &str = r#"
[MEDOUSA_HOST_BUS]
You are the runtime orchestrator on the Medousa turn bus — not the Grapheme/MCP executor.

Your tools:
- Session memory: cognition_memory_* (schema, calibrate, moods, context, list, recall, store) for posture and light reads.
- Capability catalog: cognition_capability_list / search / resolve to learn capability ids and bindings (inspect only — do not invoke here).
- Turn workers: cognition_spawn_turn_worker for heavy rituals (web, Grapheme scripts, deep memory work); cognition_turn_worker_status / cancel.
- Runtime control: cognition_runtime_workflow_* , cognition_runtime_jobs_* , cognition_runtime_recurring_* , cognition_job_enqueue , cognition_runtime_delivery_status.

Rules:
- Delegate execution (Grapheme template_run / run, MCP invoke, capability invoke, multi-tool research) via cognition_spawn_turn_worker — use intent research for web/Grapheme rituals, general for lighter capability+template work.
- After spawning, give only a short user_ack; synthesis delivers the final answer.
- Use workflows/jobs when work must be durable across turns.
- Do not claim tool receipts the worker has not produced.
- Tool errors arrive as JSON receipts (ok=false). Read them, adjust or delegate via cognition_spawn_turn_worker, retry once per policy — a single failed host tool does not end the turn.
- On spawn, the worker receives a [MEDOUSA_WORKER_HANDOFF] capsule (host goal, tool digests with receipt hints, open gaps) — not parent chat. Put resolved capability/module/op into the task prompt so the worker executes instead of rediscovering."#;

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
- Complete WORKER_TASK only. The host already orchestrated the turn — you are the executor, not a second host.
- Read [MEDOUSA_WORKER_HANDOFF] and HOST_TOOL_DIGESTS before any discovery tool. If digests show capability_resolve/search or modules search already succeeded, do not repeat them unless the prior receipt failed.

Minimum tool path:
- When WORKER_TASK names a capability (e.g. web_research), module.op, or URL: prefer cognition_capability_invoke or one cognition_grapheme_run — not a full discovery spiral.
- Target 1–3 execution tool rounds for clear tasks; use discovery only when WORKER_TASK is ambiguous or a tool failed.
- End early when WORKER_TASK is satisfied — you do not need to exhaust max_tool_rounds.

Memory:
- session_id + [MEDOUSA_CONTINUATION] / handoff fields are your session anchor (recent operator thread when present).
- For research/web tasks: optional single cognition_memory_context when the task references prior session facts; skip calibrate/schema unless intent is memory.*.
- For memory.* intents: follow [MEDOUSA_WORKER_MEMORY] ritual order."#;

pub const WORKER_SYSTEM_APPENDIX: &str = r#"Rules:
- Execute WORKER_TASK with the minimum tools needed; end early when done (see MEDOUSA_TOOL_POLICY and MEDOUSA_WORKER_DISCIPLINE).
- Do not emit operator-facing prose until work is done — the host synthesizes your receipts.
- When finished, call cognition_turn_prepare_final once, then send one complete result message on the next turn without further tools.
- Ground claims in tool receipts (e.g. cognition_memory_calibrate before claiming calibration).
- Do not repeat the same status table without new tool output.
- On every cognition_memory_* tool call, pass session_id as a non-empty string (see WORKER_CONTEXT). Never pass null."#;

/// Grapheme scripting playbook (condensed from the main Medousa system prompt).
pub const WORKER_GRAPHEME_APPENDIX: &str = r#"
[MEDOUSA_WORKER_GRAPHEME]
Grapheme is GraphQL-style query syntax with Elixir-like piping. Scripts fail when you invent syntax — always copy from discovered examples first.

Execution order:
1) Check HOST_TOOL_DIGESTS and WORKER_TASK — if capability or module is already named, skip to step 3.
2) Classify only when ambiguous: live/current facts need web.<provider>, http, websearch, or cognition_capability_invoke — not modules search alone.
3) Prefer cognition_capability_invoke when WORKER_TASK maps to a catalog capability (web_research, fetch, docs). Preset: cognition_grapheme_template_run (research_report | http_poll | csv_digest) before hand-authoring source.
4) Discovery (only if steps 1–3 are insufficient): cognition_grapheme_modules → examples show → modules_info/ops on the chosen module.
5) Run cognition_grapheme_run (or cli_run) from the closest example; one adjust-and-retry on failure — no blind rewrite loops.

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

fn worker_intent_appendix(intent: TurnWorkerIntent) -> String {
    match intent {
        TurnWorkerIntent::MemoryAvecCalibrate | TurnWorkerIntent::MemoryContext => {
            WORKER_MEMORY_APPENDIX.to_string()
        }
        TurnWorkerIntent::Research | TurnWorkerIntent::General => {
            format!("{WORKER_CAPABILITY_APPENDIX}\n{WORKER_GRAPHEME_APPENDIX}")
        }
    }
}

pub fn worker_system_prompt(session_id: &str, intent: TurnWorkerIntent) -> String {
    format!(
        "{WORKER_STTP_POLICY}\n\n{WORKER_SYSTEM_APPENDIX}\n\n{WORKER_DISCIPLINE_APPENDIX}\n\n{}\n\n[MEDOUSA_WORKER_CONTEXT]\n\
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
        "The background worker failed. Write one clear user-facing message explaining what went wrong \
         and what they can try next (retry, clarify session, or simpler request). Do not invent tool results.\n\n\
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
    let mut out = format!("{base}\n\n{HOST_BUS_TURN_APPENDIX}");
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
    format!(
        "Synthesize a single user-facing reply for the host bus. Use the worker handoff and \
         worker tool summary — not the full parent chat transcript.\n\n\
         WORK_ID: (see handoff)\n\
         WORKER_INTENT: {}\n\
         HOST_SCRATCH_DIGEST: {}\n\n\
         ORIGINAL_USER_MESSAGE:\n{}\n\n\
         WORKER_TASK:\n{}\n\n\
         HOST_TOOL_DIGESTS:\n{}\n\n\
         WORKER_TOOLS:\n{tools}\n\n\
         WORKER_TOOL_SUMMARY:\n{worker_tools_summary}{scratch_block}\n\n\
         WORKER_RESULT:\n{worker_result}\n\n\
         Produce the final answer for the user. Include outcomes and receipts from the worker. \
         Do not mention internal worker IDs unless helpful for debugging.",
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
        "Synthesize a single user-facing reply for the host bus.\n\n\
         WORK_ID: {work_id}\n\
         WORKER_INTENT: {intent}\n\n\
         ORIGINAL_USER_MESSAGE:\n{parent_user_prompt}\n\n\
         WORKER_TASK:\n{task_prompt}\n\n\
         WORKER_TOOLS: {tools}\n\n\
         WORKER_RESULT:\n{worker_result}\n\n\
         Produce the final answer for the user. Include outcomes and receipts from the worker. \
         Do not mention internal worker IDs unless helpful for debugging."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_worker_prompt_includes_grapheme_discovery() {
        let prompt = worker_system_prompt("sess-1", TurnWorkerIntent::Research);
        assert!(prompt.contains("MEDOUSA_WORKER_DISCIPLINE"));
        assert!(prompt.contains("HOST_TOOL_DIGESTS"));
        assert!(prompt.contains("cognition_grapheme_modules"));
        assert!(prompt.contains("cognition_grapheme_template_run"));
        assert!(prompt.contains("minimum tools"));
    }

    #[test]
    fn memory_worker_prompt_includes_calibrate_ritual() {
        let prompt = worker_system_prompt("sess-1", TurnWorkerIntent::MemoryAvecCalibrate);
        assert!(prompt.contains("cognition_memory_calibrate"));
        assert!(!prompt.contains("cognition_grapheme_run"));
    }
}
