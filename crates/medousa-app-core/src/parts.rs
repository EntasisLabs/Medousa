//! Project daemon `TurnPart` lists into the application timeline model.

use medousa_types::TurnPart;

use crate::contract::{
    ChatMediaAttachment, ChatSegment, ToolArtifactRef, ToolRunState, UiArtifact,
    UiArtifactPresentation,
};
use crate::display::is_runtime_startup_text;

pub fn model_receipt_from_parts(parts: Option<&[TurnPart]>) -> Option<(String, String)> {
    parts?.iter().find_map(|part| match part {
        TurnPart::ModelReceipt { provider, model }
            if !provider.trim().is_empty() && !model.trim().is_empty() =>
        {
            Some((provider.trim().to_string(), model.trim().to_string()))
        }
        _ => None,
    })
}

pub fn tool_runs_from_parts(parts: Option<&[TurnPart]>) -> Option<Vec<ToolRunState>> {
    let parts = parts?;
    let runs: Vec<ToolRunState> = parts
        .iter()
        .filter_map(|part| match part {
            TurnPart::ToolRun {
                run_id,
                tool_name,
                status,
                input_summary,
                input_params,
                output_summary,
                artifact_refs,
                tool_round,
                ..
            } => Some(ToolRunState {
                run_id: run_id.clone(),
                tool_name: tool_name.clone(),
                status: normalize_tool_status(status),
                round: tool_round.unwrap_or(1) as u64,
                input_summary: Some(input_summary.clone()),
                input_params: if input_params.is_empty() {
                    None
                } else {
                    Some(input_params.clone())
                },
                output_summary: output_summary.clone(),
                artifact_refs: if artifact_refs.is_empty() {
                    None
                } else {
                    Some(
                        artifact_refs
                            .iter()
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
                },
            }),
            _ => None,
        })
        .collect();
    if runs.is_empty() { None } else { Some(runs) }
}

fn normalize_tool_status(status: &str) -> String {
    match status {
        "failed" => "failed".into(),
        "running" => "running".into(),
        _ => "succeeded".into(),
    }
}

fn normalize_presentation(value: Option<&str>) -> UiArtifactPresentation {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("panel") => UiArtifactPresentation::Panel,
        Some("fullscreen") => UiArtifactPresentation::Fullscreen,
        _ => UiArtifactPresentation::Inline,
    }
}

/// Project native chronological parts into Medousa's segment model.
///
/// Legacy turns intentionally return `None`: their grouped parts cannot prove
/// where prose occurred relative to tools.
pub fn chat_segments_from_parts(parts: Option<&[TurnPart]>) -> Option<Vec<ChatSegment>> {
    let parts = parts?;
    let text_parts: Vec<_> = parts
        .iter()
        .filter(|part| matches!(part, TurnPart::Text { .. }))
        .collect();
    if text_parts.is_empty()
        || text_parts.iter().any(|part| match part {
            TurnPart::Text { segment_id, .. } => {
                segment_id.as_ref().map(|id| id.trim().is_empty()).unwrap_or(true)
            }
            _ => true,
        })
    {
        return None;
    }

    let mut segments: Vec<ChatSegment> = Vec::new();
    for (part_index, part) in parts.iter().enumerate() {
        match part {
            TurnPart::Text {
                markdown,
                segment_id,
                model_round,
            } => {
                let Some(segment_id) = segment_id.as_ref().map(|id| id.trim()).filter(|id| !id.is_empty())
                else {
                    continue;
                };
                if is_runtime_startup_text(Some(markdown)) {
                    continue;
                }
                segments.push(ChatSegment::Text {
                    segment_id: segment_id.to_string(),
                    model_round: model_round.map(|round| round as u64),
                    markdown: markdown.clone(),
                    committed: true,
                });
            }
            TurnPart::Progress { markdown } => {
                if markdown.trim().is_empty() || is_runtime_startup_text(Some(markdown)) {
                    continue;
                }
                segments.push(ChatSegment::Progress {
                    progress_id: format!("history-progress:{part_index}"),
                    markdown: markdown.trim().to_string(),
                });
            }
            TurnPart::ToolRun {
                run_id,
                tool_name,
                status,
                input_summary,
                input_params,
                output_summary,
                artifact_refs,
                tool_round,
                ..
            } => {
                let run = ToolRunState {
                    run_id: run_id.clone(),
                    tool_name: tool_name.clone(),
                    status: normalize_tool_status(status),
                    round: tool_round.unwrap_or(1) as u64,
                    input_summary: Some(input_summary.clone()),
                    input_params: if input_params.is_empty() {
                        None
                    } else {
                        Some(input_params.clone())
                    },
                    output_summary: output_summary.clone(),
                    artifact_refs: if artifact_refs.is_empty() {
                        None
                    } else {
                        Some(
                            artifact_refs
                                .iter()
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
                    },
                };
                match segments.last_mut() {
                    Some(ChatSegment::ToolGroup { runs, .. }) => runs.push(run),
                    _ => segments.push(ChatSegment::ToolGroup {
                        group_id: format!("history-tool-group:{run_id}"),
                        tool_round: run.round,
                        runs: vec![run],
                    }),
                }
            }
            TurnPart::AttachmentRef {
                artifact_id,
                mime,
                label,
                byte_size,
                presentation,
                height_px,
            } => {
                segments.push(ChatSegment::Artifact {
                    artifact: UiArtifact {
                        artifact_id: artifact_id.clone(),
                        mime: mime.clone(),
                        label: label.clone(),
                        presentation: normalize_presentation(presentation.as_deref()),
                        byte_size: *byte_size,
                        height_px: *height_px,
                        root_artifact_id: None,
                    },
                });
            }
            TurnPart::Handoff {
                handoff_kind,
                text,
                work_id,
            } => {
                segments.push(ChatSegment::Handoff {
                    handoff_kind: handoff_kind.clone(),
                    text: text.clone(),
                    work_id: work_id.clone(),
                });
            }
            _ => {}
        }
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

pub fn reasoning_from_parts(parts: Option<&[TurnPart]>) -> Option<String> {
    let chunks: Vec<_> = parts?
        .iter()
        .filter_map(|part| match part {
            TurnPart::Reasoning { markdown } if !markdown.trim().is_empty() => {
                Some(markdown.as_str())
            }
            _ => None,
        })
        .collect();
    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join("\n"))
    }
}

pub fn progress_from_parts(parts: Option<&[TurnPart]>) -> Option<String> {
    parts?
        .iter()
        .rev()
        .find_map(|part| match part {
            TurnPart::Progress { markdown }
                if !markdown.trim().is_empty() && !is_runtime_startup_text(Some(markdown)) =>
            {
                Some(markdown.trim().to_string())
            }
            _ => None,
        })
}

pub fn media_from_parts(parts: Option<&[TurnPart]>) -> Option<Vec<ChatMediaAttachment>> {
    let attachments: Vec<_> = parts?
        .iter()
        .filter_map(|part| match part {
            TurnPart::UserDrawing {
                media_id,
                preview_media_id,
                label,
                ..
            } => Some(ChatMediaAttachment {
                media_id: preview_media_id.clone(),
                kind: "drawing".into(),
                mime: "image/png".into(),
                label: Some(
                    label
                        .as_ref()
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .unwrap_or("Drawing")
                        .to_string(),
                ),
                origin: Some("drawing".into()),
                editable_source_id: Some(media_id.clone()),
                generation_id: None,
                parent_generation_id: None,
                width_px: None,
                height_px: None,
            }),
            TurnPart::GeneratedMedia {
                media_id,
                mime,
                label,
                generation_id,
                parent_generation_id,
                width_px,
                height_px,
                ..
            } => Some(ChatMediaAttachment {
                media_id: media_id.clone(),
                kind: "image".into(),
                mime: mime.clone(),
                label: Some(label.clone()),
                origin: Some("generated".into()),
                editable_source_id: None,
                generation_id: Some(generation_id.clone()),
                parent_generation_id: parent_generation_id.clone(),
                width_px: *width_px,
                height_px: *height_px,
            }),
            TurnPart::UserMedia {
                media_id,
                mime,
                label,
                ..
            } => Some(ChatMediaAttachment {
                media_id: media_id.clone(),
                kind: if mime.starts_with("image/") {
                    "image".into()
                } else {
                    "document".into()
                },
                mime: mime.clone(),
                label: Some(
                    label
                        .as_ref()
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .unwrap_or(media_id.as_str())
                        .to_string(),
                ),
                origin: Some("user".into()),
                editable_source_id: None,
                generation_id: None,
                parent_generation_id: None,
                width_px: None,
                height_px: None,
            }),
            _ => None,
        })
        .collect();
    if attachments.is_empty() {
        None
    } else {
        Some(attachments)
    }
}

pub fn host_context_from_parts(
    parts: Option<&[TurnPart]>,
) -> Option<medousa_types::HostTurnContext> {
    parts?.iter().find_map(|part| match part {
        TurnPart::HostContext { context } => Some(context.clone()),
        _ => None,
    })
}

pub fn ui_artifacts_from_parts(parts: Option<&[TurnPart]>) -> Option<Vec<UiArtifact>> {
    let artifacts: Vec<_> = parts?
        .iter()
        .filter_map(|part| match part {
            TurnPart::AttachmentRef {
                artifact_id,
                mime,
                label,
                byte_size,
                presentation,
                height_px,
            } => Some(UiArtifact {
                artifact_id: artifact_id.clone(),
                mime: mime.clone(),
                label: label.clone(),
                presentation: normalize_presentation(presentation.as_deref()),
                byte_size: *byte_size,
                height_px: *height_px,
                root_artifact_id: None,
            }),
            _ => None,
        })
        .collect();
    if artifacts.is_empty() {
        None
    } else {
        Some(artifacts)
    }
}
