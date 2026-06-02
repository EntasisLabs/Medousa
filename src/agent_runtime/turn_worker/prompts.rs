//! Host / worker / synthesis system prompts (Phase 1).

pub const HOST_BUS_TURN_APPENDIX: &str = r#"
[MEDOUSA_HOST_BUS]
You are the conversational host on the Medousa turn bus. You do not run heavy tool work inline — delegate with cognition_spawn_turn_worker (intent + task + user_ack).
After spawning, give a short user-visible ack only; a background worker runs tools and synthesis delivers the final answer.
Use cognition_turn_worker_status for pending work. Do not claim tool receipts the worker has not produced."#;

pub fn host_route_appendix(intent: Option<&str>) -> String {
    let intent = intent.unwrap_or("general");
    format!(
        "[MEDOUSA_HOST_ROUTE]\n\
         route=delegate\n\
         recommended_worker_intent={intent}\n\
         Call cognition_spawn_turn_worker with that intent, a complete task prompt for the worker, and a short user_ack. \
         Do not call memory, MCP, or grapheme tools on the host turn."
    )
}

pub const WORKER_SYSTEM_PROMPT: &str = r#"You are a Medousa turn worker (background specialist). The user is not in this thread — only the task prompt.

Rules:
- Use tools until the ritual or task is complete.
- Do not emit user-facing prose until work is done.
- When finished, call cognition_turn_prepare_final once, then send one complete result message on the next turn without further tools.
- Ground claims in tool receipts (e.g. cognition_memory_calibrate before claiming calibration).
- Do not repeat the same status table without new tool output.
- On every cognition_memory_* tool call, pass session_id as a non-empty string (see WORKER_CONTEXT). Never pass null."#;

pub fn worker_system_prompt(session_id: &str) -> String {
    format!(
        "{WORKER_SYSTEM_PROMPT}\n\n[MEDOUSA_WORKER_CONTEXT]\nsession_id={session_id}\n\
         Always include \"session_id\": \"{session_id}\" on cognition_memory_calibrate, \
         cognition_memory_moods, cognition_memory_context, cognition_memory_store, and related tools."
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
