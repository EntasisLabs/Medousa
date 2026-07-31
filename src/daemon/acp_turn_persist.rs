//! Persist ACP (Cursor/Codex) prompt turns into Medousa session history.
//!
//! Live SSE is unchanged; this closes the reopen/restart gap so Home can reload
//! via `GET /v1/sessions/{id}/history`. Forge resume tokens remain wire-reattach
//! only — not a transcript substitute.

use medousa_acp_client::AcpEvent;
use medousa_types::session::ConversationTurn;
use serde_json::Value;

use crate::session_writer;
use crate::turn_parts::{
    conversation_turn_from_parts, user_conversation_turn, TurnPart, TurnPartsAccumulator,
};

/// Accumulates one ACP prompt into a durable assistant turn.
#[derive(Debug, Default)]
pub struct AcpPromptPersistState {
    streamed_markdown: String,
    message_done_fallback: String,
    tool_names: Vec<String>,
    parts: TurnPartsAccumulator,
    tool_round: usize,
    finalized: bool,
}

impl AcpPromptPersistState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, event: &AcpEvent) {
        match event {
            AcpEvent::MessageDelta { text } => {
                self.streamed_markdown.push_str(text);
            }
            AcpEvent::MessageDone { text } => {
                self.message_done_fallback = text.clone();
            }
            AcpEvent::ReasoningDelta { text } => {
                self.parts.push_reasoning_delta(text);
            }
            AcpEvent::ToolCall { id, name, input } => {
                if !self.tool_names.iter().any(|n| n == name) {
                    self.tool_names.push(name.clone());
                }
                let summary = input_summary(input);
                self.tool_round = self.tool_round.saturating_add(1);
                self.parts
                    .tool_started(id, name, &summary, self.tool_round);
            }
            AcpEvent::PermissionRequest { .. }
            | AcpEvent::Error { .. }
            | AcpEvent::Done => {}
        }
    }

    fn canonical_body(&self) -> String {
        if self.streamed_markdown.trim().is_empty() {
            self.message_done_fallback.clone()
        } else {
            self.streamed_markdown.clone()
        }
    }

    /// Build the assistant turn once. Subsequent calls return `None`.
    pub fn take_assistant_turn(&mut self, answer_state: Option<&str>) -> Option<ConversationTurn> {
        if self.finalized {
            return None;
        }
        self.finalized = true;
        let body = self.canonical_body();
        let tool_names = std::mem::take(&mut self.tool_names);
        let turn = self.parts.finalize_assistant_turn(
            body,
            tool_names,
            answer_state.map(str::to_owned),
        );
        Some(turn)
    }

    pub fn is_finalized(&self) -> bool {
        self.finalized
    }
}

fn input_summary(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::String(s) => {
            let t = s.trim();
            if t.len() > 240 {
                format!("{}…", &t[..240])
            } else {
                t.to_owned()
            }
        }
        other => {
            let s = other.to_string();
            if s.len() > 240 {
                format!("{}…", &s[..240])
            } else {
                s
            }
        }
    }
}

/// Persist the user prompt at the start of an ACP pump.
pub fn persist_user_prompt(session_id: &str, prompt: &str) {
    session_writer::persist_turn(session_id, user_conversation_turn(prompt), None);
}

/// Persist the assistant turn once (Done / idle / Error).
pub fn persist_assistant_if_needed(
    session_id: &str,
    state: &mut AcpPromptPersistState,
    answer_state: Option<&str>,
) {
    if let Some(turn) = state.take_assistant_turn(answer_state) {
        session_writer::persist_turn(session_id, turn, None);
    }
}

/// Fold a sequence of ACP events into user + assistant turns (for tests).
pub fn fold_prompt_to_turns(prompt: &str, events: &[AcpEvent]) -> (ConversationTurn, ConversationTurn) {
    let user = user_conversation_turn(prompt);
    let mut state = AcpPromptPersistState::new();
    let mut answer_state: Option<&str> = None;
    for event in events {
        state.observe(event);
        if let AcpEvent::Error { .. } = event {
            answer_state = Some("error");
        }
    }
    let assistant = state
        .take_assistant_turn(answer_state)
        .unwrap_or_else(|| {
            conversation_turn_from_parts(
                "assistant",
                String::new(),
                Vec::new(),
                answer_state.map(str::to_owned),
                vec![TurnPart::Text {
                    markdown: String::new(),
                }],
            )
        });
    (user, assistant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn folds_deltas_and_message_done_preferring_stream() {
        let events = vec![
            AcpEvent::MessageDelta {
                text: "Hello ".into(),
            },
            AcpEvent::MessageDelta {
                text: "world".into(),
            },
            AcpEvent::MessageDone {
                text: "ignored fallback".into(),
            },
            AcpEvent::Done,
        ];
        let (user, assistant) = fold_prompt_to_turns("hi", &events);
        assert_eq!(user.role, "user");
        assert_eq!(user.content, "hi");
        assert_eq!(assistant.role, "assistant");
        assert_eq!(assistant.content, "Hello world");
        assert!(assistant.answer_state.is_none());
    }

    #[test]
    fn falls_back_to_message_done_when_no_deltas() {
        let events = vec![
            AcpEvent::MessageDone {
                text: "final only".into(),
            },
            AcpEvent::Done,
        ];
        let (_, assistant) = fold_prompt_to_turns("q", &events);
        assert_eq!(assistant.content, "final only");
    }

    #[test]
    fn captures_reasoning_and_tool_names() {
        let events = vec![
            AcpEvent::ReasoningDelta {
                text: "think ".into(),
            },
            AcpEvent::ReasoningDelta {
                text: "hard".into(),
            },
            AcpEvent::ToolCall {
                id: "t1".into(),
                name: "Read".into(),
                input: json!({"path": "a.rs"}),
            },
            AcpEvent::MessageDelta {
                text: "done".into(),
            },
            AcpEvent::Done,
        ];
        let (_, assistant) = fold_prompt_to_turns("q", &events);
        assert_eq!(assistant.content, "done");
        assert_eq!(assistant.tool_names, vec!["Read".to_string()]);
        let parts = assistant.parts.expect("parts");
        assert!(
            parts.iter().any(|p| matches!(
                p,
                TurnPart::Reasoning { markdown } if markdown == "think hard"
            )),
            "expected reasoning part: {parts:?}"
        );
        assert!(
            parts.iter().any(|p| matches!(
                p,
                TurnPart::ToolRun { tool_name, .. } if tool_name == "Read"
            )),
            "expected tool run: {parts:?}"
        );
    }

    #[test]
    fn error_sets_answer_state() {
        let events = vec![
            AcpEvent::MessageDelta {
                text: "partial".into(),
            },
            AcpEvent::Error {
                message: "boom".into(),
            },
        ];
        let (_, assistant) = fold_prompt_to_turns("q", &events);
        assert_eq!(assistant.content, "partial");
        assert_eq!(assistant.answer_state.as_deref(), Some("error"));
    }

    #[test]
    fn take_assistant_turn_is_idempotent() {
        let mut state = AcpPromptPersistState::new();
        state.observe(&AcpEvent::MessageDelta {
            text: "x".into(),
        });
        assert!(state.take_assistant_turn(None).is_some());
        assert!(state.take_assistant_turn(None).is_none());
        assert!(state.is_finalized());
    }
}
