//! Ordered turn timeline parts (P3 presentation envelope).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::daemon_api::StreamToolArtifactRef;
use crate::session::ConversationTurn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnArtifactRef {
    pub role: String,
    pub content_type: String,
    pub byte_size: usize,
    pub hash64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnPart {
    Text {
        markdown: String,
    },
    Reasoning {
        markdown: String,
    },
    ToolRun {
        run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        artifact_refs: Vec<TurnArtifactRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_round: Option<usize>,
        started_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at: Option<DateTime<Utc>>,
    },
    Handoff {
        handoff_kind: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
}

#[derive(Debug, Default)]
struct PendingToolRun {
    run_id: String,
    tool_name: String,
    input_summary: String,
    tool_round: usize,
    started_at: DateTime<Utc>,
    status: Option<String>,
    output_summary: Option<String>,
    artifact_refs: Vec<TurnArtifactRef>,
    finished_at: Option<DateTime<Utc>>,
}

/// Accumulates structured timeline parts for one persisted assistant turn.
#[derive(Debug, Default)]
pub struct TurnPartsAccumulator {
    reasoning: String,
    tool_runs: Vec<PendingToolRun>,
}

impl TurnPartsAccumulator {
    pub fn push_content_delta(&mut self, _delta: &str) {
        // Final answer text is taken from the terminal sink payload; deltas are
        // mirrored in SSE only.
    }

    pub fn push_reasoning_delta(&mut self, delta: &str) {
        self.reasoning.push_str(delta);
    }

    pub fn scratch_reset(&mut self) {
        self.reasoning.clear();
    }

    pub fn tool_started(
        &mut self,
        run_id: &str,
        tool_name: &str,
        input_summary: &str,
        tool_round: usize,
    ) {
        self.tool_runs.push(PendingToolRun {
            run_id: run_id.to_string(),
            tool_name: tool_name.to_string(),
            input_summary: input_summary.to_string(),
            tool_round,
            started_at: Utc::now(),
            ..PendingToolRun::default()
        });
    }

    pub fn tool_finished(
        &mut self,
        run_id: &str,
        status: &str,
        output_summary: Option<String>,
        artifact_refs: Vec<TurnArtifactRef>,
    ) {
        let Some(run) = self.tool_runs.iter_mut().find(|run| run.run_id == run_id) else {
            return;
        };
        run.status = Some(status.to_string());
        run.output_summary = output_summary;
        run.artifact_refs = artifact_refs;
        run.finished_at = Some(Utc::now());
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn tool_run_parts(&self) -> Vec<TurnPart> {
        self.tool_runs
            .iter()
            .map(|run| TurnPart::ToolRun {
                run_id: run.run_id.clone(),
                tool_name: run.tool_name.clone(),
                status: run
                    .status
                    .clone()
                    .unwrap_or_else(|| "succeeded".to_string()),
                input_summary: run.input_summary.clone(),
                output_summary: run.output_summary.clone(),
                artifact_refs: run.artifact_refs.clone(),
                tool_round: Some(run.tool_round),
                started_at: run.started_at,
                finished_at: run.finished_at,
            })
            .collect()
    }

    fn finalize_parts(
        &mut self,
        text: &str,
        handoff: Option<(String, Option<String>)>,
    ) -> Vec<TurnPart> {
        let mut parts = self.tool_run_parts();
        if let Some((kind, work_id)) = handoff {
            parts.push(TurnPart::Handoff {
                handoff_kind: kind,
                text: text.to_string(),
                work_id,
            });
        }
        if !self.reasoning.is_empty() {
            parts.push(TurnPart::Reasoning {
                markdown: std::mem::take(&mut self.reasoning),
            });
        }
        parts.push(TurnPart::Text {
            markdown: text.to_string(),
        });
        parts
    }

    pub fn finalize_assistant_turn(
        &mut self,
        content: String,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) -> ConversationTurn {
        let parts = self.finalize_parts(&content, None);
        self.reset();
        conversation_turn_from_parts("assistant", content, tool_names, answer_state, parts)
    }

    pub fn finalize_worker_ack_turn(
        &mut self,
        content: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) -> ConversationTurn {
        let parts = self.finalize_parts(&content, Some(("worker_ack".to_string(), work_id)));
        self.reset();
        conversation_turn_from_parts("assistant", content, tool_names, None, parts)
    }
}

pub fn user_conversation_turn(content: impl Into<String>) -> ConversationTurn {
    let content = content.into();
    conversation_turn_from_parts(
        "user",
        content.clone(),
        Vec::new(),
        None,
        vec![TurnPart::Text {
            markdown: content,
        }],
    )
}

pub fn conversation_turn_from_parts(
    role: &str,
    content: String,
    tool_names: Vec<String>,
    answer_state: Option<String>,
    parts: Vec<TurnPart>,
) -> ConversationTurn {
    ConversationTurn {
        role: role.to_string(),
        content,
        timestamp: Utc::now(),
        tool_names,
        answer_state,
        parts: if parts.is_empty() {
            None
        } else {
            Some(parts)
        },
    }
}

pub fn artifact_refs_from_stream(refs: &[StreamToolArtifactRef]) -> Vec<TurnArtifactRef> {
    refs.iter()
        .map(|item| TurnArtifactRef {
            role: item.role.clone(),
            content_type: item.content_type.clone(),
            byte_size: item.byte_size,
            hash64: item.hash64.clone(),
        })
        .collect()
}

/// Compose journal-friendly markdown from structured parts (falls back to content).
pub fn compose_turn_markdown(turn: &ConversationTurn) -> String {
    match turn.parts.as_deref() {
        Some(parts) if !parts.is_empty() => compose_parts_markdown(parts),
        _ => turn.content.clone(),
    }
}

pub fn compose_parts_markdown(parts: &[TurnPart]) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TurnPart::Text { markdown } => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(markdown);
            }
            TurnPart::Reasoning { markdown } => {
                if !markdown.trim().is_empty() {
                    out.push_str("\n\n> [!abstract] Reasoning\n> ");
                    out.push_str(&markdown.replace('\n', "\n> "));
                }
            }
            TurnPart::ToolRun {
                tool_name,
                status,
                input_summary,
                output_summary,
                ..
            } => {
                out.push_str(&format!(
                    "\n\n> [!info] Tool: {tool_name} ({status})\n> {input_summary}"
                ));
                if let Some(summary) = output_summary.as_deref().filter(|s| !s.is_empty()) {
                    out.push_str("\n> \n> ");
                    out.push_str(summary);
                }
            }
            TurnPart::Handoff {
                handoff_kind,
                text,
                ..
            } => {
                out.push_str(&format!("\n\n> [!note] Handoff ({handoff_kind})\n> "));
                out.push_str(&text.replace('\n', "\n> "));
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulator_builds_tool_and_text_parts() {
        let mut acc = TurnPartsAccumulator::default();
        acc.tool_started("tr-1", "search", "query=rust", 1);
        acc.tool_finished("tr-1", "succeeded", Some("3 hits".into()), vec![]);
        acc.push_reasoning_delta("thinking…");

        let turn = acc.finalize_assistant_turn("Hello".into(), vec!["search".into()], None);
        let parts = turn.parts.expect("parts");
        assert_eq!(parts.len(), 3);
        assert!(matches!(&parts[0], TurnPart::ToolRun { tool_name, .. } if tool_name == "search"));
        assert!(matches!(&parts[1], TurnPart::Reasoning { .. }));
        assert!(matches!(&parts[2], TurnPart::Text { markdown } if markdown == "Hello"));
    }

    #[test]
    fn compose_parts_markdown_includes_tool_callout() {
        let markdown = compose_parts_markdown(&[
            TurnPart::Text {
                markdown: "Answer".into(),
            },
            TurnPart::ToolRun {
                run_id: "tr-1".into(),
                tool_name: "search".into(),
                status: "succeeded".into(),
                input_summary: "query=test".into(),
                output_summary: Some("ok".into()),
                artifact_refs: vec![],
                tool_round: Some(1),
                started_at: Utc::now(),
                finished_at: Some(Utc::now()),
            },
        ]);
        assert!(markdown.contains("Answer"));
        assert!(markdown.contains("Tool: search"));
    }
}
