//! Portable extraction of principal-facing text from runtime diagnostics.

use serde_json::Value;

pub fn extract_output_text_from_diagnostics(diagnostics_raw: Option<&str>) -> Option<String> {
    let diagnostics_raw = diagnostics_raw?.trim();
    if diagnostics_raw.is_empty() {
        return None;
    }

    let parsed: Value = serde_json::from_str(diagnostics_raw).ok()?;
    find_output_text(&parsed)
}

fn find_output_text(payload: &Value) -> Option<String> {
    const ROOT_KEYS: [&str; 9] = [
        "output_text", "final_output_text", "response_text", "assistant_message", "final_text",
        "answer", "content", "text", "transcript_preview",
    ];
    for key in ROOT_KEYS {
        if let Some(text) = read_non_empty_text(payload.get(key)) {
            return Some(text);
        }
    }
    if let Some(transcript) = payload.get("transcript").and_then(Value::as_array) {
        for entry in transcript.iter().rev() {
            if let Some(text) = read_non_empty_text(Some(entry)) {
                return Some(text);
            }
        }
    }
    for key in ["result", "response", "output", "final", "completion"] {
        let Some(section) = payload.get(key) else { continue };
        for nested_key in ROOT_KEYS {
            if let Some(text) = read_non_empty_text(section.get(nested_key)) {
                return Some(text);
            }
        }
    }
    if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
        for choice in choices.iter().rev() {
            if let Some(text) = read_non_empty_text(choice.get("text")) {
                return Some(text);
            }
            if let Some(text) = read_non_empty_text(
                choice.get("message").and_then(|message| message.get("content")),
            ) {
                return Some(text);
            }
        }
    }
    if let Some(messages) = payload.get("messages").and_then(Value::as_array) {
        for message in messages.iter().rev() {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .map(|value| value.to_ascii_lowercase());
            if (role.as_deref() == Some("assistant") || role.is_none())
                && let Some(text) = read_non_empty_text(message.get("content"))
            {
                return Some(text);
            }
        }
    }
    read_non_empty_text(Some(payload))
}

fn read_non_empty_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

