//! Typed context supplied by editor, notes, and browser hosts.

use crate::session::ConversationTurn;
use crate::turn_parts::TurnPart;
use medousa_types::HostTurnContext;

const MAX_LABEL_CHARS: usize = 1_000;
const MAX_SELECTION_CHARS: usize = 12_000;
const MAX_EXCERPT_CHARS: usize = 24_000;
const MAX_DIAGNOSTICS: usize = 100;
const MAX_RELATED_RESOURCES: usize = 100;

fn bounded(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

fn bounded_optional(value: Option<&str>, max_chars: usize) -> Option<String> {
    value
        .map(|value| bounded(value, max_chars))
        .filter(|value| !value.is_empty())
}

pub fn bound_host_context(context: &HostTurnContext) -> HostTurnContext {
    let mut bounded_context = context.clone();
    bounded_context.source = bounded(&context.source, 64);
    bounded_context.workspace = bounded_optional(context.workspace.as_deref(), MAX_LABEL_CHARS);
    bounded_context.resource_kind = bounded_optional(context.resource_kind.as_deref(), 32);
    bounded_context.resource_path =
        bounded_optional(context.resource_path.as_deref(), MAX_LABEL_CHARS);
    bounded_context.resource_title =
        bounded_optional(context.resource_title.as_deref(), MAX_LABEL_CHARS);
    bounded_context.resource_url =
        bounded_optional(context.resource_url.as_deref(), MAX_LABEL_CHARS * 2);
    bounded_context.language = bounded_optional(context.language.as_deref(), 128);
    bounded_context.selection = context.selection.as_ref().map(|selection| {
        let mut selection = selection.clone();
        selection.text = selection.text.chars().take(MAX_SELECTION_CHARS).collect();
        selection
    });
    bounded_context.document_excerpt = context
        .document_excerpt
        .as_deref()
        .map(|value| value.chars().take(MAX_EXCERPT_CHARS).collect())
        .filter(|value: &String| !value.trim().is_empty());
    bounded_context.diagnostics = context
        .diagnostics
        .iter()
        .take(MAX_DIAGNOSTICS)
        .map(|diagnostic| {
            let mut diagnostic = diagnostic.clone();
            diagnostic.message = bounded(&diagnostic.message, MAX_LABEL_CHARS);
            diagnostic.severity = bounded_optional(diagnostic.severity.as_deref(), 32);
            diagnostic.source = bounded_optional(diagnostic.source.as_deref(), 128);
            diagnostic
        })
        .filter(|diagnostic| !diagnostic.message.is_empty())
        .collect();
    bounded_context.related_resources = context
        .related_resources
        .iter()
        .take(MAX_RELATED_RESOURCES)
        .map(|value| bounded(value, MAX_LABEL_CHARS))
        .filter(|value| !value.is_empty())
        .collect();
    bounded_context
}

pub fn format_host_context_block(context: &HostTurnContext) -> String {
    let payload = serde_json::to_string(context).unwrap_or_else(|_| "{}".to_string());
    format!("[MEDOUSA_HOST_CONTEXT]\ntrust=advisory\nauthority=none\npayload={payload}")
}

pub fn append_host_context(prompt: &str, context: Option<&HostTurnContext>) -> String {
    let Some(context) = context else {
        return prompt.to_string();
    };
    format!(
        "{}\n\n{}",
        prompt.trim(),
        format_host_context_block(context)
    )
}

pub fn host_context_from_turn(turn: &ConversationTurn) -> Option<&HostTurnContext> {
    turn.parts.as_deref()?.iter().find_map(|part| match part {
        TurnPart::HostContext { context } => Some(context),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::{HostContextSelection, HostTurnContext};

    #[test]
    fn bounds_large_host_payloads() {
        let context = HostTurnContext {
            source: "vscode".to_string(),
            workspace: None,
            resource_kind: Some("file".to_string()),
            resource_path: Some("src/main.rs".to_string()),
            resource_title: None,
            resource_url: None,
            language: Some("rust".to_string()),
            cursor: None,
            selection: Some(HostContextSelection {
                text: "x".repeat(MAX_SELECTION_CHARS + 20),
                start: None,
                end: None,
            }),
            document_excerpt: None,
            diagnostics: Vec::new(),
            related_resources: Vec::new(),
        };

        let result = bound_host_context(&context);
        assert_eq!(
            result.selection.unwrap().text.chars().count(),
            MAX_SELECTION_CHARS
        );
    }

    #[test]
    fn projection_is_separate_from_human_prompt() {
        let context = HostTurnContext {
            source: "browser".to_string(),
            workspace: None,
            resource_kind: Some("page".to_string()),
            resource_path: None,
            resource_title: Some("Example".to_string()),
            resource_url: Some("https://example.com".to_string()),
            language: None,
            cursor: None,
            selection: None,
            document_excerpt: None,
            diagnostics: Vec::new(),
            related_resources: Vec::new(),
        };
        let result = append_host_context("Summarize this", Some(&context));
        assert!(result.starts_with("Summarize this\n\n[MEDOUSA_HOST_CONTEXT]"));
        assert!(result.contains("https://example.com"));
    }
}
