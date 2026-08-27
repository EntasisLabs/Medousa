//! Ordered turn timeline parts (P3 presentation envelope).

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use medousa_types::session::ConversationTurn;

pub use medousa_types::turn::{TurnArtifactRef, TurnPart};

use crate::daemon_api::StreamToolArtifactRef;

/// Accumulates structured timeline parts for one persisted assistant turn.
#[derive(Debug, Default)]
pub struct TurnPartsAccumulator {
    parts: Vec<TurnPart>,
    tool_run_indexes: HashMap<String, usize>,
    text_segment_indexes: HashMap<String, usize>,
    open_text_segments: HashSet<String>,
    reasoning_index: Option<usize>,
    model_receipt_index: Option<usize>,
    progress_notes: Vec<String>,
    open_legacy_draft: Option<String>,
}

impl TurnPartsAccumulator {
    /// Snapshot the live ordered timeline without settling the turn.
    pub fn preview_parts(&self) -> Vec<TurnPart> {
        self.parts.clone()
    }

    /// Open a V3 text segment at the point where it was observed.
    pub fn start_text_segment(&mut self, segment_id: &str, model_round: Option<usize>) {
        if self.text_segment_indexes.contains_key(segment_id) {
            return;
        }
        let index = self.parts.len();
        self.parts.push(TurnPart::Text {
            markdown: String::new(),
            segment_id: Some(segment_id.to_string()),
            model_round,
        });
        self.text_segment_indexes
            .insert(segment_id.to_string(), index);
        self.open_text_segments.insert(segment_id.to_string());
    }

    /// Append only to the addressed V3 segment. A suffix-only replay may omit
    /// its start fact, so preserve the append at its observation position with
    /// unknown round metadata instead of discarding visible prose.
    pub fn append_text_segment(&mut self, segment_id: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        if !self.text_segment_indexes.contains_key(segment_id) {
            self.start_text_segment(segment_id, None);
        }
        if !self.open_text_segments.contains(segment_id) {
            return;
        }
        let Some(index) = self.text_segment_indexes.get(segment_id).copied() else {
            return;
        };
        if let Some(TurnPart::Text { markdown, .. }) = self.parts.get_mut(index) {
            markdown.push_str(text);
        }
    }

    /// Segment commits are fences, not new content. Keeping the identity index
    /// makes a duplicate/replayed lifecycle fact idempotent.
    pub fn finish_text_segment(&mut self, segment_id: &str) {
        self.open_text_segments.remove(segment_id);
    }

    pub fn chronological_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                TurnPart::Text { markdown, .. } if !markdown.trim().is_empty() => {
                    Some(markdown.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn live_progress_notes(&self) -> &[String] {
        &self.progress_notes
    }

    pub fn push_reasoning_delta(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(index) = self.reasoning_index
            && let Some(TurnPart::Reasoning { markdown }) = self.parts.get_mut(index)
        {
            markdown.push_str(delta);
            return;
        }
        self.reasoning_index = Some(self.parts.len());
        self.parts.push(TurnPart::Reasoning {
            markdown: delta.to_string(),
        });
    }

    pub fn set_model_receipt(&mut self, provider: &str, model: &str) {
        let receipt = TurnPart::ModelReceipt {
            provider: provider.to_string(),
            model: model.to_string(),
        };
        if let Some(index) = self.model_receipt_index
            && let Some(part) = self.parts.get_mut(index)
        {
            *part = receipt;
            return;
        }
        self.model_receipt_index = Some(self.parts.len());
        self.parts.push(receipt);
    }

    /// Preserve an explicit ephemeral-style progress update without treating it
    /// as model-authored visible prose.
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
        self.parts.push(TurnPart::Progress {
            markdown: trimmed.to_string(),
        });
    }

    pub fn push_handoff(&mut self, handoff_kind: &str, text: &str, work_id: Option<String>) {
        self.parts.push(TurnPart::Handoff {
            handoff_kind: handoff_kind.to_string(),
            text: text.to_string(),
            work_id,
        });
    }

    /// Commit model-authored visible prose at its current timeline position.
    /// V2 callers may omit identity metadata; V3 callers provide both fields.
    pub fn commit_text_segment(
        &mut self,
        markdown: &str,
        segment_id: Option<&str>,
        model_round: Option<usize>,
    ) {
        if markdown.trim().is_empty() {
            return;
        }
        if segment_id.is_some_and(|id| self.text_segment_indexes.contains_key(id)) {
            return;
        }
        self.parts.push(TurnPart::Text {
            markdown: markdown.to_string(),
            segment_id: segment_id.map(str::to_string),
            model_round,
        });
        if let Some(segment_id) = segment_id {
            self.text_segment_indexes
                .insert(segment_id.to_string(), self.parts.len() - 1);
        }
    }

    /// Commit the current V2 draft once before its first chronological fence.
    /// Parallel tool starts and the later scratch reset may observe the same
    /// buffer, so this compatibility path must be idempotent within a draft.
    pub fn commit_legacy_text_draft(&mut self, markdown: &str) {
        if markdown.trim().is_empty()
            || self
                .open_legacy_draft
                .as_deref()
                .is_some_and(|open| open == markdown)
        {
            return;
        }
        self.commit_text_segment(markdown, None, None);
        self.open_legacy_draft = Some(markdown.to_string());
    }

    /// Close a V2 draft at reset, committing it only if no earlier tool fence
    /// already did so.
    pub fn close_legacy_text_draft(&mut self, markdown: &str) {
        self.commit_legacy_text_draft(markdown);
        self.open_legacy_draft = None;
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
        self.parts.push(TurnPart::AttachmentRef {
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
        for part in &mut self.parts {
            if let TurnPart::AttachmentRef {
                artifact_id: existing,
                ..
            } = part
                && existing == previous_artifact_id
            {
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
        self.push_attachment_ref(artifact_id, mime, label, byte_size, presentation, height_px);
    }

    pub fn tool_started(
        &mut self,
        run_id: &str,
        tool_name: &str,
        input_summary: &str,
        tool_round: usize,
    ) {
        if self.tool_run_indexes.contains_key(run_id) {
            return;
        }
        let index = self.parts.len();
        self.parts.push(TurnPart::ToolRun {
            run_id: run_id.to_string(),
            tool_name: tool_name.to_string(),
            status: "running".to_string(),
            input_summary: input_summary.to_string(),
            output_summary: None,
            artifact_refs: Vec::new(),
            tool_round: Some(tool_round),
            started_at: Utc::now(),
            finished_at: None,
        });
        self.tool_run_indexes.insert(run_id.to_string(), index);
    }

    pub fn tool_finished(
        &mut self,
        run_id: &str,
        status: &str,
        output_summary: Option<String>,
        artifact_refs: Vec<TurnArtifactRef>,
    ) {
        let Some(index) = self.tool_run_indexes.get(run_id).copied() else {
            return;
        };
        let Some(TurnPart::ToolRun {
            status: run_status,
            output_summary: run_output_summary,
            artifact_refs: run_artifact_refs,
            finished_at,
            ..
        }) = self.parts.get_mut(index)
        else {
            return;
        };
        *run_status = status.to_string();
        *run_output_summary = output_summary;
        *run_artifact_refs = artifact_refs;
        *finished_at = Some(Utc::now());
    }

    /// Apply a V3 finish fact even when a reconnect suffix did not include the
    /// corresponding start. The receipt stays exactly where it was observed.
    #[allow(clippy::too_many_arguments)]
    pub fn tool_finished_observed(
        &mut self,
        run_id: &str,
        tool_name: &str,
        input_summary: &str,
        tool_round: usize,
        status: &str,
        output_summary: Option<String>,
        artifact_refs: Vec<TurnArtifactRef>,
    ) {
        if !self.tool_run_indexes.contains_key(run_id) {
            self.tool_started(run_id, tool_name, input_summary, tool_round);
        }
        self.tool_finished(run_id, status, output_summary, artifact_refs);
    }

    pub fn preview_tool_runs(&self) -> Vec<TurnPart> {
        self.parts
            .iter()
            .filter(|part| matches!(part, TurnPart::ToolRun { .. }))
            .cloned()
            .collect()
    }

    pub fn has_pending_tool_runs(&self) -> bool {
        !self.tool_run_indexes.is_empty()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn finalize_parts(
        &mut self,
        text: &str,
        handoff: Option<(String, Option<String>)>,
    ) -> Vec<TurnPart> {
        self.parts.retain(
            |part| !matches!(part, TurnPart::Reasoning { markdown } if markdown.is_empty()),
        );
        if let Some((kind, work_id)) = handoff {
            self.parts.push(TurnPart::Handoff {
                handoff_kind: kind,
                text: text.to_string(),
                work_id,
            });
        }
        self.parts.push(TurnPart::Text {
            markdown: text.to_string(),
            segment_id: None,
            model_round: None,
        });
        std::mem::take(&mut self.parts)
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

    /// Finalize a V3 turn whose visible text segments were committed at their
    /// observation positions. Terminal settlement must not append the aggregate
    /// body as a second authoritative text part.
    pub fn finalize_chronological_turn(
        &mut self,
        content: String,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) -> ConversationTurn {
        self.parts.retain(|part| {
            !matches!(part, TurnPart::Reasoning { markdown } if markdown.is_empty())
                && !matches!(part, TurnPart::Text { markdown, .. } if markdown.is_empty())
        });
        let has_visible_text = self.parts.iter().any(
            |part| matches!(part, TurnPart::Text { markdown, .. } if !markdown.trim().is_empty()),
        );
        if !has_visible_text && !content.trim().is_empty() {
            self.commit_text_segment(&content, None, None);
        }
        let parts = std::mem::take(&mut self.parts);
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
            segment_id: None,
            model_round: None,
        });
    } else if parts.is_empty() {
        parts.push(TurnPart::Text {
            markdown: String::new(),
            segment_id: None,
            model_round: None,
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
        parts: if parts.is_empty() { None } else { Some(parts) },
        slice_summary: None,
        speaker_profile_id: None,
    }
}

pub fn user_conversation_turn_with_media_and_speaker(
    content: impl Into<String>,
    media_refs: &[crate::daemon_api::MediaRef],
    speaker_profile_id: Option<&str>,
) -> ConversationTurn {
    let mut turn = user_conversation_turn_with_media(content, media_refs);
    if let Some(profile_id) = speaker_profile_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        turn.speaker_profile_id = Some(profile_id.to_string());
    }
    turn
}

pub fn user_conversation_turn_with_context_media_and_speaker(
    content: impl Into<String>,
    host_context: Option<medousa_types::HostTurnContext>,
    media_refs: &[crate::daemon_api::MediaRef],
    speaker_profile_id: Option<&str>,
) -> ConversationTurn {
    let mut turn =
        user_conversation_turn_with_media_and_speaker(content, media_refs, speaker_profile_id);
    if let Some(context) = host_context {
        turn.parts
            .get_or_insert_with(Vec::new)
            .push(TurnPart::HostContext { context });
    }
    turn
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
            TurnPart::ModelReceipt { .. } => {}
            TurnPart::Text { markdown, .. } => {
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
                handoff_kind, text, ..
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
            TurnPart::HostContext { .. } => {}
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
        assert!(
            matches!(&parts[0], TurnPart::Progress { markdown } if markdown == "Pulling context…")
        );
        assert!(matches!(&parts[1], TurnPart::ToolRun { .. }));
        assert!(
            matches!(&parts[2], TurnPart::Text { markdown, .. } if markdown == "Final answer.")
        );
    }

    #[test]
    fn text_and_parallel_tools_remain_in_declared_chronological_order() {
        let mut acc = TurnPartsAccumulator::default();
        acc.commit_text_segment("Let me check.", Some("segment-1"), Some(1));
        acc.tool_started("run-1", "search", "first query", 1);
        acc.tool_started("run-2", "fetch", "second query", 1);

        // Completion timing must not move the declared runs.
        acc.tool_finished("run-2", "succeeded", Some("second result".into()), vec![]);
        acc.tool_finished("run-1", "failed", Some("first failed".into()), vec![]);
        acc.commit_text_segment("I recovered.", Some("segment-2"), Some(2));
        acc.tool_started("run-3", "search", "fallback query", 2);
        acc.tool_finished("run-3", "succeeded", Some("found".into()), vec![]);

        let turn = acc.finalize_assistant_turn("Here is the answer.".into(), vec![], None);
        let parts = turn.parts.expect("parts");
        assert_eq!(parts.len(), 6);
        assert!(matches!(
            &parts[0],
            TurnPart::Text { markdown, segment_id: Some(id), model_round: Some(1) }
                if markdown == "Let me check." && id == "segment-1"
        ));
        assert!(matches!(
            &parts[1],
            TurnPart::ToolRun { run_id, status, .. } if run_id == "run-1" && status == "failed"
        ));
        assert!(matches!(
            &parts[2],
            TurnPart::ToolRun { run_id, status, .. }
                if run_id == "run-2" && status == "succeeded"
        ));
        assert!(matches!(
            &parts[3],
            TurnPart::Text { markdown, segment_id: Some(id), model_round: Some(2) }
                if markdown == "I recovered." && id == "segment-2"
        ));
        assert!(matches!(&parts[4], TurnPart::ToolRun { run_id, .. } if run_id == "run-3"));
        assert!(matches!(
            &parts[5],
            TurnPart::Text { markdown, segment_id: None, model_round: None }
                if markdown == "Here is the answer."
        ));
    }

    #[test]
    fn chronological_finalize_keeps_segments_without_duplicate_aggregate_text() {
        let mut acc = TurnPartsAccumulator::default();
        acc.commit_text_segment("Let me check.", Some("segment-1"), Some(1));
        acc.tool_started("run-1", "search", "query", 1);
        acc.tool_finished("run-1", "succeeded", Some("found".into()), vec![]);
        acc.commit_text_segment("Found it.", Some("segment-2"), Some(2));

        let turn = acc.finalize_chronological_turn(
            "Let me check.\n\nFound it.".into(),
            vec!["search".into()],
            None,
        );
        let parts = turn.parts.expect("parts");

        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0],
            TurnPart::Text { markdown, segment_id: Some(id), model_round: Some(1) }
                if markdown == "Let me check." && id == "segment-1"
        ));
        assert!(matches!(&parts[1], TurnPart::ToolRun { run_id, .. } if run_id == "run-1"));
        assert!(matches!(
            &parts[2],
            TurnPart::Text { markdown, segment_id: Some(id), model_round: Some(2) }
                if markdown == "Found it." && id == "segment-2"
        ));
    }

    #[test]
    fn live_v3_segments_and_failed_receipts_keep_observation_order() {
        let mut acc = TurnPartsAccumulator::default();
        acc.start_text_segment("segment-1", Some(1));
        acc.append_text_segment("segment-1", "Let me check.");
        acc.finish_text_segment("segment-1");
        acc.tool_started("run-1", "search", "first", 1);
        acc.tool_started("run-2", "fetch", "second", 1);
        acc.tool_finished("run-2", "succeeded", Some("ok".into()), vec![]);
        acc.tool_finished("run-1", "failed", Some("timeout".into()), vec![]);
        acc.start_text_segment("segment-2", Some(2));
        acc.append_text_segment("segment-2", "I can recover.");
        acc.finish_text_segment("segment-2");
        acc.tool_started("run-3", "search", "fallback", 2);
        acc.tool_finished("run-3", "succeeded", Some("found".into()), vec![]);
        acc.start_text_segment("segment-3", Some(3));
        acc.append_text_segment("segment-3", "Done.");

        let parts = acc
            .finalize_chronological_turn(
                "Let me check.\n\nI can recover.\n\nDone.".into(),
                vec!["search".into(), "fetch".into()],
                None,
            )
            .parts
            .expect("parts");
        let identities = parts
            .iter()
            .map(|part| match part {
                TurnPart::Text {
                    segment_id: Some(id),
                    ..
                } => id.as_str(),
                TurnPart::ToolRun { run_id, .. } => run_id.as_str(),
                _ => "other",
            })
            .collect::<Vec<_>>();
        assert_eq!(
            identities,
            [
                "segment-1",
                "run-1",
                "run-2",
                "segment-2",
                "run-3",
                "segment-3"
            ]
        );
        assert!(matches!(
            &parts[1],
            TurnPart::ToolRun { status, .. } if status == "failed"
        ));
    }

    #[test]
    fn reconnect_suffix_preserves_unknown_segment_and_finish_facts() {
        let mut acc = TurnPartsAccumulator::default();
        acc.append_text_segment("segment-gap", "suffix prose");
        acc.tool_finished_observed(
            "run-gap",
            "search",
            "replayed suffix",
            4,
            "failed",
            Some("offline".into()),
            vec![],
        );

        let parts = acc.preview_parts();
        assert!(matches!(
            &parts[0],
            TurnPart::Text { markdown, segment_id: Some(id), model_round: None }
                if markdown == "suffix prose" && id == "segment-gap"
        ));
        assert!(matches!(
            &parts[1],
            TurnPart::ToolRun { run_id, status, .. }
                if run_id == "run-gap" && status == "failed"
        ));
    }

    #[test]
    fn chronological_finalize_adds_terminal_text_only_when_no_segment_was_observed() {
        let mut acc = TurnPartsAccumulator::default();

        let turn = acc.finalize_chronological_turn("Fallback answer.".into(), vec![], None);
        let parts = turn.parts.expect("parts");

        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0],
            TurnPart::Text { markdown, segment_id: None, model_round: None }
                if markdown == "Fallback answer."
        ));
    }

    #[test]
    fn legacy_draft_commits_before_parallel_tools_without_reset_duplication() {
        let mut acc = TurnPartsAccumulator::default();
        acc.commit_legacy_text_draft("I will inspect this.");
        acc.tool_started("run-1", "search", "first", 1);
        acc.commit_legacy_text_draft("I will inspect this.");
        acc.tool_started("run-2", "fetch", "second", 1);
        acc.close_legacy_text_draft("I will inspect this.");
        acc.tool_finished("run-2", "succeeded", None, vec![]);
        acc.tool_finished("run-1", "succeeded", None, vec![]);

        let parts = acc
            .finalize_assistant_turn("Both checks passed.".into(), vec![], None)
            .parts
            .expect("parts");
        assert_eq!(parts.len(), 4);
        assert!(matches!(
            &parts[0],
            TurnPart::Text { markdown, .. } if markdown == "I will inspect this."
        ));
        assert!(matches!(&parts[1], TurnPart::ToolRun { run_id, .. } if run_id == "run-1"));
        assert!(matches!(&parts[2], TurnPart::ToolRun { run_id, .. } if run_id == "run-2"));
        assert!(matches!(
            &parts[3],
            TurnPart::Text { markdown, .. } if markdown == "Both checks passed."
        ));
    }

    #[test]
    fn closing_a_legacy_draft_allows_identical_prose_in_a_later_round() {
        let mut acc = TurnPartsAccumulator::default();
        acc.commit_legacy_text_draft("Still checking.");
        acc.close_legacy_text_draft("Still checking.");
        acc.commit_legacy_text_draft("Still checking.");

        let text_count = acc
            .finalize_assistant_turn("Done.".into(), vec![], None)
            .parts
            .expect("parts")
            .into_iter()
            .filter(|part| matches!(part, TurnPart::Text { markdown, .. } if markdown == "Still checking."))
            .count();
        assert_eq!(text_count, 2);
    }

    #[test]
    fn attachment_update_preserves_its_original_timeline_position() {
        let mut acc = TurnPartsAccumulator::default();
        acc.commit_text_segment("Chart incoming.", Some("segment-1"), Some(1));
        acc.push_attachment_ref("artifact-old", "text/html", "Old chart", None, None, None);
        acc.tool_started("run-1", "chart", "refresh", 1);
        acc.replace_attachment_ref(
            "artifact-old",
            "artifact-new",
            "text/html",
            "New chart",
            Some(42),
            Some("inline".into()),
            Some(240),
        );

        let parts = acc
            .finalize_assistant_turn("Updated.".into(), vec![], None)
            .parts
            .expect("parts");
        assert!(matches!(&parts[0], TurnPart::Text { .. }));
        assert!(matches!(
            &parts[1],
            TurnPart::AttachmentRef { artifact_id, label, .. }
                if artifact_id == "artifact-new" && label == "New chart"
        ));
        assert!(matches!(&parts[2], TurnPart::ToolRun { .. }));
        assert!(matches!(&parts[3], TurnPart::Text { .. }));
    }

    #[test]
    fn reasoning_does_not_reorder_later_parts_or_tool_indexes() {
        let mut acc = TurnPartsAccumulator::default();
        acc.push_reasoning_delta("keep me");
        acc.tool_started("run-1", "search", "query", 1);
        acc.tool_finished("run-1", "succeeded", None, vec![]);

        let parts = acc
            .finalize_assistant_turn("Done.".into(), vec![], None)
            .parts
            .expect("parts");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            &parts[0],
            TurnPart::ToolRun { run_id, status, .. }
                if run_id == "run-1" && status == "succeeded"
        ));
        assert!(matches!(
            &parts[1],
            TurnPart::Reasoning { markdown } if markdown == "keep me"
        ));
        assert!(matches!(&parts[2], TurnPart::Text { .. }));
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
        assert!(matches!(&parts[2], TurnPart::Text { markdown, .. } if markdown == "Hello"));
    }

    #[test]
    fn assistant_turn_persists_model_receipt() {
        let mut acc = TurnPartsAccumulator::default();
        acc.set_model_receipt("openai-codex", "gpt-5.6-sol");
        let turn = acc.finalize_assistant_turn("Hello".into(), vec![], None);
        assert!(turn.parts.as_deref().is_some_and(|parts| matches!(
            parts.first(),
            Some(TurnPart::ModelReceipt { provider, model })
                if provider == "openai-codex" && model == "gpt-5.6-sol"
        )));
    }

    #[test]
    fn compose_parts_markdown_includes_tool_callout() {
        let markdown = compose_parts_markdown(&[
            TurnPart::Text {
                markdown: "Answer".into(),
                segment_id: None,
                model_round: None,
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
                segment_id: None,
                model_round: None,
            },
        ];
        let raw = serde_json::to_string(&parts).expect("serialize");
        assert!(raw.contains("\"kind\":\"progress\""));
        let decoded: Vec<TurnPart> = serde_json::from_str(&raw).expect("deserialize");
        assert!(matches!(&decoded[0], TurnPart::Progress { .. }));
    }

    #[test]
    fn host_context_is_structured_without_changing_user_content() {
        let context = medousa_types::HostTurnContext {
            source: "vscode".to_string(),
            workspace: Some("Medousa".to_string()),
            resource_kind: Some("file".to_string()),
            resource_path: Some("src/main.rs".to_string()),
            resource_title: None,
            resource_url: None,
            language: Some("rust".to_string()),
            cursor: None,
            selection: None,
            document_excerpt: None,
            diagnostics: Vec::new(),
            related_resources: Vec::new(),
        };
        let turn = user_conversation_turn_with_context_media_and_speaker(
            "Explain this",
            Some(context),
            &[],
            None,
        );
        assert_eq!(turn.content, "Explain this");
        assert!(turn.parts.as_deref().is_some_and(|parts| {
            parts
                .iter()
                .any(|part| matches!(part, TurnPart::HostContext { .. }))
        }));
    }
}
