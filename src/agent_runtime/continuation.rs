use genai::chat::ChatMessage;
use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use super::prompt_prep::{MAX_REQUEST_PROMPT_CHARS, truncate_text_for_budget};

const CONTINUATION_TRIGGER_TOOL_OUTPUT_CHARS: usize = 8_000;
const CONTINUATION_TRIGGER_STDOUT_CHARS: usize = 4_000;
const CONTINUATION_MAX_DRAFT_CHARS: usize = 6_000;
const CONTINUATION_MAX_TOOL_OUTPUT_CHARS: usize = 2_000;
const CONTINUATION_MAX_TOOL_SUMMARIES: usize = 6;

pub fn should_run_continuation(invocations: &[ToolInvocation]) -> bool {
    for invocation in invocations {
        let output_chars = invocation.tool_output.to_string().chars().count();
        if output_chars >= CONTINUATION_TRIGGER_TOOL_OUTPUT_CHARS {
            return true;
        }

        let stdout_chars = invocation
            .tool_output
            .get("stdout")
            .and_then(|value| value.as_str())
            .map(|value| value.chars().count())
            .unwrap_or(0);
        if stdout_chars >= CONTINUATION_TRIGGER_STDOUT_CHARS {
            return true;
        }

        if invocation
            .tool_name
            .to_ascii_lowercase()
            .contains("grapheme")
            && output_chars >= 2000
        {
            return true;
        }
    }
    false
}

pub fn build_continuation_prompt(
    original_prompt: &str,
    draft_text: &str,
    invocations: &[ToolInvocation],
) -> Option<String> {
    if invocations.is_empty() {
        return None;
    }

    let summaries = invocations
        .iter()
        .take(CONTINUATION_MAX_TOOL_SUMMARIES)
        .map(|invocation| {
            let safe_output = crate::settings_guard::redact_json_value(&invocation.tool_output);
            let rendered_output = truncate_text_for_budget(
                &safe_output.to_string(),
                CONTINUATION_MAX_TOOL_OUTPUT_CHARS,
            );
            format!(
                "- tool={} output={} ",
                invocation.tool_name, rendered_output
            )
        })
        .collect::<Vec<_>>();

    if summaries.is_empty() {
        return None;
    }

    let draft = truncate_text_for_budget(draft_text, CONTINUATION_MAX_DRAFT_CHARS);
    let user_request = truncate_text_for_budget(original_prompt, 3000);
    let prompt = format!(
        "You have an initial draft answer plus additional tool context that may have arrived in chunks. \
         Rewrite one coherent reply that integrates the tool evidence into the same conversational thread. \
         Preserve substantiated details, remove contradictions, and mark uncertainty explicitly. \
         Voice: sharp loyal partner — warm and professional, never cold, never flirtatious.\n\n\
         [USER_REQUEST]\n{user_request}\n\n[DRAFT_ANSWER]\n{draft}\n\n[ADDITIONAL_TOOL_CONTEXT]\n{}\n\n\
         Return only the final answer body.",
        summaries.join("\n")
    );

    Some(truncate_text_for_budget(&prompt, MAX_REQUEST_PROMPT_CHARS))
}

pub fn build_continuation_prior_messages(original_prompt: &str, draft_text: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::user(truncate_text_for_budget(original_prompt, 2000)),
        ChatMessage::assistant(truncate_text_for_budget(draft_text, 4000)),
    ]
}

pub fn collect_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    let mut names = Vec::new();
    for invocation in invocations {
        if !names
            .iter()
            .any(|existing| existing == &invocation.tool_name)
        {
            names.push(invocation.tool_name.clone());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

    use super::should_run_continuation;

    #[test]
    fn continuation_trigger_detects_large_stdout_payload() {
        let invocations = vec![ToolInvocation {
            tool_name: "cognition.grapheme.run".to_string(),
            tool_input: serde_json::json!({"script": "noop"}),
            tool_output: serde_json::json!({
                "stdout": "x".repeat(4500)
            }),
        }];

        assert!(should_run_continuation(&invocations));
    }
}
