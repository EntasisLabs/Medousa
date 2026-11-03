//! Fold Turn Stream V3 facts into the canonical chat item projection.

use medousa_types::{
    StreamToolArtifactRef, StreamUiArtifact, TurnCompletionOutcomeV3, TurnStreamEnvelopeV3,
    TurnStreamEventV3, WorkerAckKind,
};

use crate::contract::{
    ChatItem, ChatSegment, ToolArtifactRef, ToolRunState, UiArtifact, UiArtifactPresentation,
};
use crate::display::{is_runtime_startup_text, visible_chat_status_line};

fn replace_segment(segments: &mut [ChatSegment], index: usize, segment: ChatSegment) {
    if let Some(slot) = segments.get_mut(index) {
        *slot = segment;
    }
}

type CompatibilityFields = (
    String,
    Option<Vec<String>>,
    Option<Vec<ToolRunState>>,
    Option<Vec<UiArtifact>>,
);

fn text_index(segments: &[ChatSegment], segment_id: &str) -> Option<usize> {
    segments.iter().position(|segment| matches!(
        segment,
        ChatSegment::Text { segment_id: id, .. } if id == segment_id
    ))
}

fn apply_text_started(segments: &mut Vec<ChatSegment>, segment_id: &str, model_round: u64) {
    if let Some(index) = text_index(segments, segment_id) {
        if let ChatSegment::Text {
            model_round: round, ..
        } = &mut segments[index]
        {
            *round = Some(model_round);
        }
        return;
    }
    segments.push(ChatSegment::Text {
        segment_id: segment_id.to_string(),
        model_round: Some(model_round),
        markdown: String::new(),
        committed: false,
    });
}

fn apply_content_append(segments: &mut Vec<ChatSegment>, segment_id: &str, text: &str) {
    if let Some(index) = text_index(segments, segment_id) {
        if let ChatSegment::Text { markdown, .. } = &mut segments[index] {
            markdown.push_str(text);
        }
        return;
    }
    segments.push(ChatSegment::Text {
        segment_id: segment_id.to_string(),
        model_round: None,
        markdown: text.to_string(),
        committed: false,
    });
}

fn apply_text_committed(segments: &mut Vec<ChatSegment>, segment_id: &str) {
    if let Some(index) = text_index(segments, segment_id) {
        if let ChatSegment::Text { committed, .. } = &mut segments[index] {
            *committed = true;
        }
        return;
    }
    segments.push(ChatSegment::Text {
        segment_id: segment_id.to_string(),
        model_round: None,
        markdown: String::new(),
        committed: true,
    });
}

fn apply_progress(segments: &mut Vec<ChatSegment>, envelope: &TurnStreamEnvelopeV3, message: &str) {
    let markdown = message.trim();
    if markdown.is_empty() {
        return;
    }
    let progress_id = format!("progress:{}:{}", envelope.turn_id, envelope.seq);
    if segments.iter().any(|segment| {
        matches!(segment, ChatSegment::Progress { progress_id: id, .. } if id == &progress_id)
    }) {
        return;
    }
    segments.push(ChatSegment::Progress {
        progress_id,
        markdown: markdown.to_string(),
    });
}

fn map_artifact_refs(refs: &[StreamToolArtifactRef]) -> Option<Vec<ToolArtifactRef>> {
    if refs.is_empty() {
        None
    } else {
        Some(
            refs.iter()
                .map(|reference| ToolArtifactRef {
                    role: reference.role.clone(),
                    content_type: reference.content_type.clone(),
                    byte_size: reference.byte_size as u64,
                    hash64: reference.hash64.clone(),
                    artifact_id: reference.artifact_id.clone(),
                    label: reference.label.clone(),
                })
                .collect(),
        )
    }
}

fn run_from_tool_event(
    event: &TurnStreamEventV3,
    previous: Option<&ToolRunState>,
) -> Option<ToolRunState> {
    match event {
        TurnStreamEventV3::ToolStarted {
            tool_run_id,
            tool_name,
            input_summary,
            input_params,
            tool_round,
        } => Some(ToolRunState {
            run_id: tool_run_id.clone(),
            tool_name: tool_name.clone(),
            status: "running".into(),
            round: *tool_round as u64,
            input_summary: Some(input_summary.clone()),
            input_params: if input_params.is_empty() {
                previous.and_then(|run| run.input_params.clone())
            } else {
                Some(input_params.clone())
            },
            output_summary: previous.and_then(|run| run.output_summary.clone()),
            artifact_refs: previous.and_then(|run| run.artifact_refs.clone()),
        }),
        TurnStreamEventV3::ToolFinished {
            tool_run_id,
            tool_name,
            status,
            input_summary,
            input_params,
            output_summary,
            tool_round,
            artifact_refs,
        } => Some(ToolRunState {
            run_id: tool_run_id.clone(),
            tool_name: tool_name.clone(),
            status: if status == "failed" {
                "failed".into()
            } else {
                "succeeded".into()
            },
            round: *tool_round as u64,
            input_summary: Some(input_summary.clone()),
            input_params: if input_params.is_empty() {
                previous.and_then(|run| run.input_params.clone())
            } else {
                Some(input_params.clone())
            },
            output_summary: output_summary
                .clone()
                .or_else(|| previous.and_then(|run| run.output_summary.clone())),
            artifact_refs: map_artifact_refs(artifact_refs)
                .or_else(|| previous.and_then(|run| run.artifact_refs.clone())),
        }),
        _ => None,
    }
}

fn find_run(segments: &[ChatSegment], run_id: &str) -> Option<(usize, usize)> {
    for (segment_index, segment) in segments.iter().enumerate() {
        if let ChatSegment::ToolGroup { runs, .. } = segment
            && let Some(run_index) = runs.iter().position(|run| run.run_id == run_id)
        {
            return Some((segment_index, run_index));
        }
    }
    None
}

fn apply_tool_event(segments: &mut Vec<ChatSegment>, event: &TurnStreamEventV3) {
    let Some(run_id) = (match event {
        TurnStreamEventV3::ToolStarted { tool_run_id, .. }
        | TurnStreamEventV3::ToolFinished { tool_run_id, .. } => Some(tool_run_id.as_str()),
        _ => None,
    }) else {
        return;
    };
    if let Some((segment_index, run_index)) = find_run(segments, run_id) {
        let previous = match &segments[segment_index] {
            ChatSegment::ToolGroup { runs, .. } => runs.get(run_index).cloned(),
            _ => None,
        };
        let Some(run) = run_from_tool_event(event, previous.as_ref()) else {
            return;
        };
        if let ChatSegment::ToolGroup { runs, .. } = &mut segments[segment_index] {
            runs[run_index] = run;
        }
        return;
    }
    let Some(run) = run_from_tool_event(event, None) else {
        return;
    };
    let tool_round = run.round;
    if let Some(ChatSegment::ToolGroup { runs, .. }) = segments.last_mut() {
        runs.push(run);
        return;
    }
    segments.push(ChatSegment::ToolGroup {
        group_id: format!("tool-group:{run_id}"),
        tool_round,
        runs: vec![run],
    });
}

fn map_stream_ui_artifact(
    artifact: &StreamUiArtifact,
    root_artifact_id: Option<String>,
) -> UiArtifact {
    UiArtifact {
        artifact_id: artifact.artifact_id.clone(),
        mime: artifact.mime.clone(),
        label: artifact.label.clone(),
        presentation: match artifact.presentation.trim().to_ascii_lowercase().as_str() {
            "panel" => UiArtifactPresentation::Panel,
            "fullscreen" => UiArtifactPresentation::Fullscreen,
            _ => UiArtifactPresentation::Inline,
        },
        byte_size: artifact.byte_size,
        height_px: artifact.height_px,
        root_artifact_id,
    }
}

fn apply_artifact_event(segments: &mut Vec<ChatSegment>, event: &TurnStreamEventV3) {
    match event {
        TurnStreamEventV3::ArtifactPresented { artifact } => {
            let mapped = map_stream_ui_artifact(artifact, None);
            if segments.iter().any(|segment| {
                matches!(
                    segment,
                    ChatSegment::Artifact { artifact: existing }
                        if existing.artifact_id == mapped.artifact_id
                )
            }) {
                return;
            }
            segments.push(ChatSegment::Artifact { artifact: mapped });
        }
        TurnStreamEventV3::ArtifactUpdated {
            previous_artifact_id,
            artifact,
            root_artifact_id,
        } => {
            let mapped = map_stream_ui_artifact(artifact, root_artifact_id.clone());
            let index = segments.iter().position(|segment| match segment {
                ChatSegment::Artifact { artifact: existing } => {
                    existing.artifact_id == *previous_artifact_id
                        || root_artifact_id.as_ref().is_some_and(|root| {
                            existing.root_artifact_id.as_ref() == Some(root)
                        })
                }
                _ => false,
            });
            if let Some(index) = index {
                replace_segment(segments, index, ChatSegment::Artifact { artifact: mapped });
            } else {
                segments.push(ChatSegment::Artifact { artifact: mapped });
            }
        }
        _ => {}
    }
}

fn compatibility_fields(segments: &[ChatSegment]) -> CompatibilityFields {
    let text = segments
        .iter()
        .filter_map(|segment| match segment {
            ChatSegment::Text { markdown, .. } if !markdown.is_empty() => Some(markdown.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let tool_runs: Vec<ToolRunState> = segments
        .iter()
        .flat_map(|segment| match segment {
            ChatSegment::ToolGroup { runs, .. } => runs.clone(),
            _ => Vec::new(),
        })
        .collect();
    let mut tools = Vec::new();
    for run in &tool_runs {
        if !tools.contains(&run.tool_name) {
            tools.push(run.tool_name.clone());
        }
    }
    let ui_artifacts: Vec<UiArtifact> = segments
        .iter()
        .filter_map(|segment| match segment {
            ChatSegment::Artifact { artifact } => Some(artifact.clone()),
            _ => None,
        })
        .collect();
    (
        text,
        if tools.is_empty() { None } else { Some(tools) },
        if tool_runs.is_empty() {
            None
        } else {
            Some(tool_runs)
        },
        if ui_artifacts.is_empty() {
            None
        } else {
            Some(ui_artifacts)
        },
    )
}

#[derive(Default)]
struct PartialChrome {
    streaming: Option<bool>,
    answer_state: Option<String>,
    failed: Option<bool>,
    error_line: Option<String>,
    error_detail: Option<String>,
    reasoning: Option<String>,
    phase: Option<String>,
    status_line: Option<Option<String>>,
    response_provider: Option<String>,
    response_model: Option<String>,
    reactions: Option<Vec<medousa_types::MessageReaction>>,
}

fn outcome_string(outcome: &TurnCompletionOutcomeV3) -> String {
    match outcome {
        TurnCompletionOutcomeV3::Completed => "completed",
        TurnCompletionOutcomeV3::NeedsInput => "needs_input",
        TurnCompletionOutcomeV3::Checkpointed => "checkpointed",
        TurnCompletionOutcomeV3::Fatal => "fatal",
        TurnCompletionOutcomeV3::Failed => "failed",
        TurnCompletionOutcomeV3::Cancelled => "cancelled",
        TurnCompletionOutcomeV3::FuseExhausted => "fuse_exhausted",
    }
    .into()
}

/// Apply one V3 fact to Medousa's message projection.
pub fn apply_v3_envelope_to_item(message: ChatItem, envelope: &TurnStreamEnvelopeV3) -> ChatItem {
    let event = &envelope.event;
    let had_segments = message.segments.is_some();
    let mut segments = message.segments.clone().unwrap_or_default();
    let initial_len_marker = segments.len();
    let mut chrome = PartialChrome::default();

    match event {
        TurnStreamEventV3::AssistantTextStarted {
            segment_id,
            model_round,
        } => apply_text_started(&mut segments, segment_id, *model_round as u64),
        TurnStreamEventV3::ContentAppend { segment_id, text } => {
            apply_content_append(&mut segments, segment_id, text);
        }
        TurnStreamEventV3::AssistantTextCommitted { segment_id } => {
            apply_text_committed(&mut segments, segment_id);
        }
        TurnStreamEventV3::ToolStarted { .. } | TurnStreamEventV3::ToolFinished { .. } => {
            apply_tool_event(&mut segments, event);
        }
        TurnStreamEventV3::ArtifactPresented { .. }
        | TurnStreamEventV3::ArtifactUpdated { .. } => {
            apply_artifact_event(&mut segments, event);
        }
        TurnStreamEventV3::WorkerAck {
            ack_kind,
            text,
            work_id,
            ..
        } => {
            let handoff_kind = match ack_kind {
                WorkerAckKind::Worker => "worker",
                WorkerAckKind::Workshop => "workshop",
            };
            segments.push(ChatSegment::Handoff {
                handoff_kind: handoff_kind.into(),
                text: text.clone(),
                work_id: work_id.clone(),
            });
        }
        TurnStreamEventV3::ReasoningAppend { text } => {
            chrome.reasoning = Some(format!("{}{text}", message.reasoning.as_deref().unwrap_or("")));
        }
        TurnStreamEventV3::Status {
            phase,
            operator_message,
            ..
        } => {
            chrome.phase = Some(phase.clone());
            chrome.status_line = Some(if phase == "accepted" {
                None
            } else {
                visible_chat_status_line(
                    operator_message
                        .as_deref()
                        .or(message.status_line.as_deref()),
                )
            });
        }
        TurnStreamEventV3::Progress { message: progress, .. } => {
            if !is_runtime_startup_text(Some(progress)) {
                apply_progress(&mut segments, envelope, progress);
                chrome.status_line = Some(None);
            }
        }
        TurnStreamEventV3::ModelReceipt { provider, model } => {
            chrome.response_provider = Some(provider.clone());
            chrome.response_model = Some(model.clone());
        }
        TurnStreamEventV3::Reaction { reaction } => {
            let mut reactions = message.reactions.clone().unwrap_or_default();
            if !reactions
                .iter()
                .any(|existing| existing.effect_id == reaction.effect_id)
            {
                reactions.push(reaction.clone());
                chrome.reactions = Some(reactions);
            }
        }
        TurnStreamEventV3::TurnCompleted {
            aggregate_text,
            outcome,
            operator_message,
            debug_message,
            ..
        } => {
            if had_segments
                && !aggregate_text.trim().is_empty()
                && !segments.iter().any(|segment| {
                    matches!(segment, ChatSegment::Text { markdown, .. } if !markdown.trim().is_empty())
                })
            {
                segments.push(ChatSegment::Text {
                    segment_id: format!("terminal:{}", envelope.turn_id),
                    model_round: None,
                    markdown: aggregate_text.clone(),
                    committed: true,
                });
            }
            let failed = matches!(
                outcome,
                TurnCompletionOutcomeV3::Failed | TurnCompletionOutcomeV3::FuseExhausted
            );
            chrome.streaming = Some(false);
            chrome.answer_state = Some(outcome_string(outcome));
            chrome.failed = Some(failed);
            chrome.error_line = if failed {
                operator_message
                    .clone()
                    .or_else(|| message.error_line.clone())
            } else {
                None
            };
            chrome.error_detail = if failed {
                debug_message
                    .clone()
                    .or_else(|| message.error_detail.clone())
            } else {
                None
            };
        }
        _ => {}
    }

    let segments_changed = had_segments || segments.len() != initial_len_marker || !segments.is_empty() && !had_segments;
    // Detect mutation more carefully: any match arms that touch segments.
    let has_segment_projection = message.segments.is_some()
        || matches!(
            event,
            TurnStreamEventV3::AssistantTextStarted { .. }
                | TurnStreamEventV3::ContentAppend { .. }
                | TurnStreamEventV3::AssistantTextCommitted { .. }
                | TurnStreamEventV3::ToolStarted { .. }
                | TurnStreamEventV3::ToolFinished { .. }
                | TurnStreamEventV3::ArtifactPresented { .. }
                | TurnStreamEventV3::ArtifactUpdated { .. }
                | TurnStreamEventV3::WorkerAck { .. }
                | TurnStreamEventV3::Progress { .. }
                | TurnStreamEventV3::TurnCompleted { .. }
        );

    let mut next = message;
    if let Some(value) = chrome.streaming {
        next.streaming = Some(value);
    }
    if let Some(value) = chrome.answer_state {
        next.answer_state = Some(value);
    }
    if let Some(value) = chrome.failed {
        next.failed = Some(value);
    }
    if chrome.error_line.is_some() || chrome.streaming == Some(false) {
        next.error_line = chrome.error_line;
    }
    if chrome.error_detail.is_some() || chrome.streaming == Some(false) {
        next.error_detail = chrome.error_detail;
    }
    if let Some(value) = chrome.reasoning {
        next.reasoning = Some(value);
    }
    if let Some(value) = chrome.phase {
        next.phase = Some(value);
    }
    if let Some(value) = chrome.status_line {
        next.status_line = value;
    }
    if let Some(value) = chrome.response_provider {
        next.response_provider = Some(value);
    }
    if let Some(value) = chrome.response_model {
        next.response_model = Some(value);
    }
    if let Some(value) = chrome.reactions {
        next.reactions = Some(value);
    }

    if has_segment_projection {
        let (content, tools, tool_runs, ui_artifacts) = compatibility_fields(&segments);
        // Preserve plain content when we never had segments and only status chrome changed.
        let material_event = matches!(
            event,
            TurnStreamEventV3::AssistantTextStarted { .. }
                | TurnStreamEventV3::ContentAppend { .. }
                | TurnStreamEventV3::AssistantTextCommitted { .. }
                | TurnStreamEventV3::ToolStarted { .. }
                | TurnStreamEventV3::ToolFinished { .. }
                | TurnStreamEventV3::ArtifactPresented { .. }
                | TurnStreamEventV3::ArtifactUpdated { .. }
                | TurnStreamEventV3::WorkerAck { .. }
                | TurnStreamEventV3::Progress { .. }
                | TurnStreamEventV3::TurnCompleted { .. }
        );
        let chrome_only = matches!(
            event,
            TurnStreamEventV3::Status { .. }
                | TurnStreamEventV3::ReasoningAppend { .. }
                | TurnStreamEventV3::ModelReceipt { .. }
                | TurnStreamEventV3::Reaction { .. }
        );
        if (had_segments || material_event) && (!chrome_only || had_segments) {
            next.content = content;
            next.tools = tools;
            next.tool_runs = tool_runs;
            next.ui_artifacts = ui_artifacts;
            next.segments = Some(segments);
        }
        let _ = segments_changed;
    }

    next
}

/// Facts that need a chat message even when reconnect starts without a shell.
pub fn v3_event_promotes_chat_message(event: &TurnStreamEventV3) -> bool {
    match event {
        TurnStreamEventV3::AssistantTextStarted { .. }
        | TurnStreamEventV3::ContentAppend { .. }
        | TurnStreamEventV3::AssistantTextCommitted { .. }
        | TurnStreamEventV3::ReasoningAppend { .. }
        | TurnStreamEventV3::ToolStarted { .. }
        | TurnStreamEventV3::ToolFinished { .. }
        | TurnStreamEventV3::ArtifactPresented { .. }
        | TurnStreamEventV3::ArtifactUpdated { .. }
        | TurnStreamEventV3::UiScene { .. }
        | TurnStreamEventV3::WorkerAck { .. }
        | TurnStreamEventV3::BudgetApprovalRequired { .. }
        | TurnStreamEventV3::Reaction { .. } => true,
        TurnStreamEventV3::Progress { message, .. } => !is_runtime_startup_text(Some(message)),
        TurnStreamEventV3::TurnCompleted { aggregate_text, .. } => {
            !aggregate_text.trim().is_empty()
        }
        _ => false,
    }
}

pub fn fold_v3_envelopes(message: ChatItem, envelopes: &[TurnStreamEnvelopeV3]) -> ChatItem {
    envelopes.iter().fold(message, apply_v3_envelope_to_item)
}
