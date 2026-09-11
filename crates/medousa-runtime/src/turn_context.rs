//! Portable model/tool transcript lanes and foreground-turn scratch adapters.

use genai::chat::ChatMessage;
use medousa_engine::TurnScratchpad;
use serde_json::Value;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

pub const ROUND_CONTEXT_PREFIX: &str = "[MEDOUSA_ROUND_CONTEXT]";

pub const SCRATCH_PREFIX: &str = "[MEDOUSA_SCRATCH]";

/// Record one Stasis invocation batch into the portable scratch DTO.
pub fn record_round_digest_from_invocations(
    scratchpad: &mut TurnScratchpad,
    invocations: &[ToolInvocation],
) {
    let entries: Vec<String> = invocations
        .iter()
        .map(|invocation| {
            let ok = tool_output_ok(&invocation.tool_output);
            let hint = compact_tool_receipt_hint(&invocation.tool_name, &invocation.tool_output);
            format_tool_digest_entry(&invocation.tool_name, ok, hint.as_deref())
        })
        .collect();
    let names: Vec<String> = invocations
        .iter()
        .map(|invocation| invocation.tool_name.clone())
        .collect();
    scratchpad.record_round_digest_entries(&names, &entries);
}

/// Mutable model-only lane. The principal-visible prefix remains immutable.
#[derive(Debug, Clone, Default)]
pub struct ToolLaneState {
    pub messages: Vec<ChatMessage>,
}

/// Optional mode-owned context: durable events after tool batches, or a
/// replaceable pointer refreshed immediately before each inference.
pub trait ToolRoundContextProvider: Send + Sync {
    fn context_for_next_round(&self) -> stasis::prelude::Result<Option<String>>;

    /// Replace a volatile tail pointer instead of retaining refresh history.
    /// Defaults to append for providers whose messages carry durable events.
    fn replaces_previous_context(&self) -> bool {
        false
    }
}

/// Fixed principal-visible prefix plus the growing tool/control lane.
#[derive(Debug, Clone)]
pub struct HostTurnContext {
    pub user_lane_prefix: Vec<ChatMessage>,
    pub tool_lane: ToolLaneState,
    pub scratchpad: TurnScratchpad,
}

impl HostTurnContext {
    pub fn new(prior_messages: Vec<ChatMessage>, user_prompt: String) -> Self {
        Self::new_with_user_message(prior_messages, ChatMessage::user(user_prompt))
    }

    pub fn new_with_user_message(
        prior_messages: Vec<ChatMessage>,
        user_message: ChatMessage,
    ) -> Self {
        let scratch_source = user_message.content.first_text().unwrap_or("").to_string();
        let scratchpad = TurnScratchpad::from_user_prompt(&scratch_source);
        let mut user_lane_prefix = prior_messages;
        user_lane_prefix.push(user_message);
        Self {
            user_lane_prefix,
            tool_lane: ToolLaneState::default(),
            scratchpad,
        }
    }

    pub fn build_model_messages(&self, system_prompt: Option<&str>) -> Vec<ChatMessage> {
        let mut messages =
            Vec::with_capacity(self.user_lane_prefix.len() + self.tool_lane.messages.len() + 1);
        if let Some(system) = system_prompt.filter(|value| !value.trim().is_empty()) {
            messages.push(ChatMessage::system(system.to_string()));
        }
        messages.extend(self.user_lane_prefix.clone());
        messages.extend(self.tool_lane.messages.clone());
        messages
    }
}

/// Keep replaceable context at the tail, leaving receipts and the fixed prefix intact.
/// The marker lives in the transcript so replacement also works after checkpoint restore.
pub fn push_round_context(messages: &mut Vec<ChatMessage>, context: String, replace: bool) {
    if replace {
        messages.retain(|message| {
            message.role != genai::chat::ChatRole::System
                || message
                    .content
                    .first_text()
                    .is_none_or(|text| !text.starts_with(ROUND_CONTEXT_PREFIX))
        });
        messages.push(ChatMessage::system(format!(
            "{ROUND_CONTEXT_PREFIX}\n{context}"
        )));
    } else {
        messages.push(ChatMessage::system(context));
    }
}

pub fn strip_prior_scratch_messages(messages: &mut Vec<ChatMessage>) {
    messages.retain(|message| {
        message
            .content
            .first_text()
            .is_none_or(|text| !text.trim_start().starts_with(SCRATCH_PREFIX))
    });
}

pub fn push_turn_scratch_message(messages: &mut Vec<ChatMessage>, scratchpad: &TurnScratchpad) {
    let body = scratchpad.format_control_body(0);
    push_scratch_body(messages, &body);
}

pub fn push_turn_scratch_message_with_budget(
    messages: &mut Vec<ChatMessage>,
    scratchpad: &TurnScratchpad,
    tool_rounds_remaining: usize,
) {
    strip_prior_scratch_messages(messages);
    let body = scratchpad.format_control_body(tool_rounds_remaining);
    push_scratch_body(messages, &body);
}

fn push_scratch_body(messages: &mut Vec<ChatMessage>, body: &str) {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    messages.push(ChatMessage::system(format!("{SCRATCH_PREFIX}\n{trimmed}")));
}

pub fn tool_output_ok(output: &Value) -> bool {
    !matches!(output.get("ok").and_then(Value::as_bool), Some(false))
}

pub fn tool_results_from_invocations(invocations: &[ToolInvocation]) -> Vec<(String, bool)> {
    invocations
        .iter()
        .map(|invocation| {
            (
                invocation.tool_name.clone(),
                tool_output_ok(&invocation.tool_output),
            )
        })
        .collect()
}

fn format_tool_digest_entry(name: &str, ok: bool, hint: Option<&str>) -> String {
    let status = if ok { "ok" } else { "fail" };
    match hint.filter(|value| !value.trim().is_empty()) {
        Some(hint) => format!("{name}:{status} ({hint})"),
        None => format!("{name}:{status}"),
    }
}

/// One-line receipt hint for compact scratch and host-to-worker handoffs.
pub fn compact_tool_receipt_hint(tool_name: &str, output: &Value) -> Option<String> {
    if matches!(output.get("ok").and_then(Value::as_bool), Some(false)) {
        return output
            .get("error")
            .or_else(|| output.get("message"))
            .and_then(Value::as_str)
            .map(|text| truncate_field(text, 96));
    }

    let normalized = tool_name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "cognition_capability_resolve"
        | "cognition.capability.resolve"
        | "cognition_capability" => {
            if let Some(hint) = output
                .get("recommended")
                .and_then(|value| value.get("reference"))
                .and_then(Value::as_str)
                .or_else(|| output.get("capability").and_then(Value::as_str))
                .map(|reference| format!("recommended={reference}"))
            {
                Some(hint)
            } else if let Some(hint) = output
                .get("matches")
                .and_then(Value::as_array)
                .and_then(|matches| matches.first())
                .and_then(|entry| entry.get("capability"))
                .and_then(Value::as_str)
                .map(|capability| format!("top={capability}"))
            {
                Some(hint)
            } else {
                output
                    .get("binding")
                    .and_then(|value| value.get("reference"))
                    .and_then(Value::as_str)
                    .or_else(|| output.get("capability").and_then(Value::as_str))
                    .map(|reference| format!("binding={reference}"))
            }
        }
        "cognition_capability_search" | "cognition.capability.search" => output
            .get("matches")
            .and_then(Value::as_array)
            .and_then(|matches| matches.first())
            .and_then(|entry| entry.get("capability"))
            .and_then(Value::as_str)
            .map(|capability| format!("top={capability}")),
        "cognition_memory_query" => output
            .get("status")
            .and_then(Value::as_str)
            .map(|status| format!("status={status}"))
            .or_else(|| {
                output
                    .get("hits")
                    .or_else(|| output.get("snippets"))
                    .and_then(Value::as_array)
                    .map(|hits| format!("hits={}", hits.len()))
            }),
        "cognition_capability_invoke" | "cognition.capability.invoke" => output
            .get("binding")
            .and_then(|value| value.get("reference"))
            .and_then(Value::as_str)
            .or_else(|| output.get("capability").and_then(Value::as_str))
            .map(|reference| format!("binding={reference}")),
        "cognition_workshop_mutate" => output
            .get("intent")
            .and_then(Value::as_str)
            .map(|intent| format!("intent={intent}")),
        _ if normalized.contains("cognition_environment") => output
            .get("revision")
            .and_then(Value::as_u64)
            .map(|revision| format!("revision={revision}"))
            .or_else(|| {
                output
                    .get("errors")
                    .and_then(Value::as_array)
                    .and_then(|errors| errors.first())
                    .and_then(Value::as_str)
                    .map(|error| format!("error={}", truncate_field(error, 80)))
            }),
        _ if normalized.contains("cognition_component") => output
            .get("component")
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .map(|id| format!("component={id}"))
            .or_else(|| {
                output
                    .get("revision")
                    .and_then(Value::as_u64)
                    .map(|revision| format!("revision={revision}"))
            }),
        _ if normalized.contains("grapheme_modules") => output
            .get("stdout")
            .and_then(Value::as_str)
            .and_then(extract_grapheme_module_ids_from_stdout)
            .map(|modules| format!("modules={modules}")),
        _ => None,
    }
}

fn extract_grapheme_module_ids_from_stdout(stdout: &str) -> Option<String> {
    let mut modules = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module_id:") {
            let id = rest.trim();
            if !id.is_empty() {
                modules.push(id.to_string());
            }
        }
    }
    if modules.is_empty() {
        return None;
    }
    modules.sort();
    modules.dedup();
    Some(truncate_field(&modules.join(","), 96))
}

pub fn scratch_digest_hash(scratch: &TurnScratchpad) -> String {
    scratch.digest_hash()
}

/// Prefer in-turn scratch progress over a session seed on retry or continuation.
pub fn scratch_seed_for_tool_loop(
    session_seed: &TurnScratchpad,
    last_in_turn: Option<&TurnScratchpad>,
) -> TurnScratchpad {
    match last_in_turn {
        Some(scratch)
            if scratch.step > 0
                || !scratch.round_digests.is_empty()
                || !scratch.working_notes.is_empty() =>
        {
            scratch.clone()
        }
        _ => session_seed.clone(),
    }
}

fn truncate_field(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_context_keeps_principal_and_tool_lanes_separate() {
        let mut context =
            HostTurnContext::new(vec![ChatMessage::user("prior")], "current ask".to_string());
        assert_eq!(context.build_model_messages(Some("sys")).len(), 3);
        context
            .tool_lane
            .messages
            .push(ChatMessage::system("tool-only"));
        assert_eq!(context.build_model_messages(Some("sys")).len(), 4);
        assert_eq!(context.user_lane_prefix.len(), 2);
        assert_eq!(context.tool_lane.messages.len(), 1);
    }

    #[test]
    fn replaceable_pointer_stays_at_tail_across_rounds_and_restore() {
        use genai::chat::{ContentPart, MessageContent, ToolCall, ToolResponse};
        let mut context = HostTurnContext::new(Vec::new(), "Current goal".into());
        let user_marker = ChatMessage::user(format!("{ROUND_CONTEXT_PREFIX} user quotation"));
        context.tool_lane.messages.push(user_marker);
        let mut receipts = Vec::new();
        for revision in 1..=20 {
            let call = ToolCall {
                call_id: format!("call-{revision}"),
                fn_name: "read".into(),
                fn_arguments: json!({}),
                thought_signatures: None,
            };
            let pair = vec![
                ChatMessage::assistant(MessageContent::from_parts(vec![ContentPart::ToolCall(
                    call.clone(),
                )])),
                ChatMessage::from(ToolResponse::from_tool_call(&call, "ok")),
            ];
            receipts.extend(pair.clone());
            context.tool_lane.messages.extend(pair);
            push_round_context(
                &mut context.tool_lane.messages,
                format!("latest={revision}"),
                true,
            );
            let model = context.build_model_messages(Some("fixed snapshot"));
            assert_eq!(model[0].content.first_text(), Some("fixed snapshot"));
            assert_eq!(model[1].content.first_text(), Some("Current goal"));
            assert_eq!(context.tool_lane.messages.len(), receipts.len() + 2);
        }
        // Checkpoint restoration needs only the existing transcript, no cursor sidecar.
        let mut restored = context.tool_lane.messages.clone();
        push_round_context(&mut restored, "latest=21".into(), true);
        assert_eq!(
            serde_json::to_value(&restored[1..restored.len() - 1]).unwrap(),
            serde_json::to_value(&receipts).unwrap()
        );
        assert!(
            restored
                .last()
                .unwrap()
                .content
                .first_text()
                .unwrap()
                .ends_with("latest=21")
        );
        push_round_context(&mut restored, "durable notice".into(), false);
        push_round_context(&mut restored, "latest=22".into(), true);
        assert!(
            restored
                .iter()
                .any(|message| message.content.first_text() == Some("durable notice"))
        );
        assert_eq!(restored.len(), receipts.len() + 3);
    }

    #[test]
    fn scratch_snapshot_is_deduplicated_and_carries_receipt_hints() {
        let mut scratch = TurnScratchpad::from_user_prompt("research");
        scratch.on_tool_round_start(1);
        record_round_digest_from_invocations(
            &mut scratch,
            &[ToolInvocation {
                tool_name: "cognition_capability".to_string(),
                tool_input: json!({}),
                tool_output: json!({
                    "ok": true,
                    "recommended": { "reference": "web.duckduckgo" }
                }),
            }],
        );
        let mut messages = Vec::new();
        push_turn_scratch_message_with_budget(&mut messages, &scratch, 3);
        push_turn_scratch_message_with_budget(&mut messages, &scratch, 2);
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]
                .content
                .first_text()
                .unwrap()
                .contains("recommended=web.duckduckgo")
        );
    }

    #[test]
    fn scratch_seed_prefers_in_turn_progress() {
        let session = TurnScratchpad::from_user_prompt("session goal");
        let mut in_turn = session.clone();
        in_turn.on_tool_round_start(2);
        in_turn.record_round_digest(&[("query".to_string(), true)]);
        assert_eq!(scratch_seed_for_tool_loop(&session, Some(&in_turn)).step, 2);
    }
}
