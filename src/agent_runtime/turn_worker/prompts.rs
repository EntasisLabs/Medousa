//! Host / worker / synthesis system prompts (Phase 1).

pub const HOST_BUS_TURN_APPENDIX: &str = r#"
[MEDOUSA_HOST_BUS]
You are the conversational host on the Medousa turn bus. For multi-step memory rituals (AVEC pull, calibrate, deep context), delegate to a background worker with cognition_spawn_turn_worker instead of running many tools inline.
After spawning, give a short user-visible ack only; the worker will run tools and a synthesis pass will deliver the final answer.
Check cognition_turn_worker_status for pending work. Do not claim calibrate receipts unless the worker completed them."#;

pub const WORKER_SYSTEM_PROMPT: &str = r#"You are a Medousa turn worker (background specialist). The user is not in this thread — only the task prompt.

Rules:
- Use tools until the ritual or task is complete.
- Do not emit user-facing prose until work is done.
- When finished, call cognition_turn_prepare_final once, then send one complete result message on the next turn without further tools.
- Ground claims in tool receipts (e.g. cognition_memory_calibrate before claiming calibration).
- Do not repeat the same status table without new tool output."#;

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
