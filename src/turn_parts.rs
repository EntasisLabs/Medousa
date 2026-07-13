//! Ordered turn timeline parts (P3 presentation envelope).

use chrono::{DateTime, Utc};

pub use medousa_types::turn::{TurnArtifactRef, TurnPart};

use crate::daemon_api::StreamToolArtifactRef;
use crate::session::ConversationTurn;

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
    progress_notes: Vec<String>,
    attachment_parts: Vec<TurnPart>,
}

impl TurnPartsAccumulator {
    pub fn live_progress_notes(&self) -> &[String] {
        &self.progress_notes
    }

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

    /// Preserve streamed prose before the next tool round clears the live draft buffer.
    pub fn archive_progress_note(&mut self, markdown: &str) {
        let trimmed = markdown.trim();
        if trimmed.is_empty() {
            return;
        }
        if self
            .progress_notes
            .last()
            .is_some_and(|last| last == trimmed)
        {
            return;
        }
        self.progress_notes.push(trimmed.to_string());
    }

    pub fn push_attachment_ref(
        &mut self,
        artifact_id: &str,
        mime: &str,
        label: &str,
        byte_size: Option<u64>,
        presentation: Option<String>,
        height_px: Option<u32>,
    ) {
        self.attachment_parts.push(TurnPart::AttachmentRef {
            artifact_id: artifact_id.to_string(),
            mime: mime.to_string(),
            label: label.to_string(),
            byte_size,
            presentation,
            height_px,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn replace_attachment_ref(
        &mut self,
        previous_artifact_id: &str,
        artifact_id: &str,
        mime: &str,
        label: &str,
        byte_size: Option<u64>,
        presentation: Option<String>,
        height_px: Option<u32>,
    ) {
        for part in &mut self.attachment_parts {
            if let TurnPart::AttachmentRef { artifact_id: existing, .. } = part
                && existing == previous_artifact_id {
                    *part = TurnPart::AttachmentRef {
                        artifact_id: artifact_id.to_string(),
                        mime: mime.to_string(),
                        label: label.to_string(),
                        byte_size,
                        presentation,
                        height_px,
                    };
                    return;
                }
        }
        self.push_attachment_ref(
            artifact_id,
            mime,
            label,
            byte_size,
            presentation,
            height_px,
        );
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

    pub fn preview_tool_runs(&self) -> Vec<TurnPart> {
        self.tool_run_parts()
    }

    pub fn has_pending_tool_runs(&self) -> bool {
        !self.tool_runs.is_empty()
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
        parts.extend(std::mem::take(&mut self.attachment_parts));
        for note in std::mem::take(&mut self.progress_notes) {
            parts.push(TurnPart::Progress { markdown: note });
        }
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
    user_conversation_turn_with_media(content, &[])
}

pub fn user_conversation_turn_with_media(
    content: impl Into<String>,
    media_refs: &[crate::daemon_api::MediaRef],
) -> ConversationTurn {
    let content = content.into();
    let mut parts = Vec::new();
    for media_ref in media_refs {
        parts.push(TurnPart::UserMedia {
            media_id: media_ref.media_id.clone(),
            mime: media_ref.mime.clone(),
            label: media_ref.label.clone(),
            byte_size: None,
        });
    }
    if !content.trim().is_empty() {
        parts.push(TurnPart::Text {
            markdown: content.clone(),
        });
    } else if parts.is_empty() {
        parts.push(TurnPart::Text {
            markdown: String::new(),
        });
    }
    conversation_turn_from_parts("user", content, Vec::new(), None, parts)
}

pub fn conversation_turn_from_parts(
    role: &str,
    content: String,
    tool_names: Vec<String>,
    answer_state: Option<String>,
    parts: Vec<TurnPart>,
) -> ConversationTurn {
    conversation_turn_from_parts_at(role, content, tool_names, answer_state, parts, Utc::now())
}

/// Same as [`conversation_turn_from_parts`] but with an explicit commit
/// timestamp. The durable event-log spine fold uses this so a persisted body
/// reconstructed from a journaled terminal event is byte-identical to the live
/// `append_turn` body (which captured its timestamp at finalize time).
pub fn conversation_turn_from_parts_at(
    role: &str,
    content: String,
    tool_names: Vec<String>,
    answer_state: Option<String>,
    parts: Vec<TurnPart>,
    timestamp: DateTime<Utc>,
) -> ConversationTurn {
    ConversationTurn {
        role: role.to_string(),
        content,
        timestamp,
        tool_names,
        answer_state,
        parts: if parts.is_empty() {
            None
        } else {
            Some(parts)
        },
        slice_summary: None,
    }
}

pub fn artifact_refs_from_stream(refs: &[StreamToolArtifactRef]) -> Vec<TurnArtifactRef> {
    refs.iter()
        .map(|item| TurnArtifactRef {
            role: item.role.clone(),
            content_type: item.content_type.clone(),
            byte_size: item.byte_size,
            hash64: item.hash64.clone(),
            artifact_id: item.artifact_id.clone(),
            label: item.label.clone(),
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
            TurnPart::Progress { markdown } => {
                if !markdown.trim().is_empty() {
                    out.push_str("\n\n> [!note] Progress\n> ");
                    out.push_str(&markdown.replace('\n', "\n> "));
                }
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
            TurnPart::UserMedia {
                media_id,
                mime,
                label,
                ..
            } => {
                let name = label.as_deref().unwrap_or("attachment");
                out.push_str(&format!(
                    "\n\n> [!note] Attachment: {name} ({mime})\n> `media:{media_id}`"
                ));
            }
            TurnPart::AttachmentRef {
                artifact_id,
                mime,
                label,
                ..
            } => {
                out.push_str(&format!(
                    "\n\n> [!note] Attachment: {label} ({mime})\n> `artifact:{artifact_id}`"
                ));
            }
            TurnPart::Unknown => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_attachment_ref_finalize_includes_attachment_before_text() {
        let mut acc = TurnPartsAccumulator::default();
        acc.push_attachment_ref(
            "art:demo:ui:abc",
            "text/html",
            "Chart",
            Some(1200),
            Some("inline".to_string()),
            Some(360),
        );
        let turn = acc.finalize_assistant_turn("See above.".into(), vec![], None);
        let parts = turn.parts.expect("parts");
        assert!(matches!(&parts[0], TurnPart::AttachmentRef { label, .. } if label == "Chart"));
        assert!(matches!(&parts[1], TurnPart::Text { .. }));
    }

    #[test]
    fn archive_progress_note_dedupes_and_finalize_includes_progress() {
        let mut acc = TurnPartsAccumulator::default();
        acc.archive_progress_note("Pulling context…");
        acc.archive_progress_note("Pulling context…");
        acc.tool_started("tr-1", "search", "query=rust", 1);
        acc.tool_finished("tr-1", "succeeded", Some("3 hits".into()), vec![]);

        let turn = acc.finalize_assistant_turn("Final answer.".into(), vec!["search".into()], None);
        let parts = turn.parts.expect("parts");
        assert!(matches!(&parts[0], TurnPart::ToolRun { .. }));
        assert!(matches!(&parts[1], TurnPart::Progress { markdown } if markdown == "Pulling context…"));
        assert!(matches!(&parts[2], TurnPart::Text { markdown } if markdown == "Final answer."));
    }

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

    #[test]
    fn progress_part_roundtrips_json() {
        let parts = vec![
            TurnPart::Progress {
                markdown: "Pulling context…".to_string(),
            },
            TurnPart::Text {
                markdown: "Final.".to_string(),
            },
        ];
        let raw = serde_json::to_string(&parts).expect("serialize");
        assert!(raw.contains("\"kind\":\"progress\""));
        let decoded: Vec<TurnPart> = serde_json::from_str(&raw).expect("deserialize");
        assert!(matches!(&decoded[0], TurnPart::Progress { .. }));
    }
}
