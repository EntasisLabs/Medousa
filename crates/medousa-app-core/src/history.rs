//! Project durable session history into canonical chat items.

use medousa_types::{SessionHistoryResponse, TranscriptEntry};

use crate::contract::{
    ChatItem, ChatItemRole, ChatLane, LiveTranscriptAttachment, LiveTranscriptRow,
    TranscriptCoordinate, TranscriptSourceRef,
};
use crate::display::is_runtime_startup_text;
use crate::parts::{
    chat_segments_from_parts, host_context_from_parts, media_from_parts, model_receipt_from_parts,
    progress_from_parts, reasoning_from_parts, tool_runs_from_parts, ui_artifacts_from_parts,
};

#[derive(Debug, Clone, Default)]
pub struct ProjectHistoryOptions {
    pub lane: Option<ChatLane>,
    pub ask_job_id: Option<String>,
}

fn matches_live_target(message: &ChatItem, target: &str) -> bool {
    if message.role != ChatItemRole::Assistant {
        return false;
    }
    if let Some(rest) = target.strip_prefix("execution:") {
        return message.turn_id.as_deref() == Some(rest);
    }
    message.id.ends_with(&format!(":{target}"))
}

/// Fold durable attachment entries into their exact committed assistant entry.
pub fn attach_live_transcripts(messages: Vec<ChatItem>) -> Vec<ChatItem> {
    let filtered: Vec<ChatItem> = messages
        .iter()
        .filter(|message| {
            !message.segments.as_ref().is_some_and(|segments| {
                segments.iter().any(|segment| match segment {
                    crate::contract::ChatSegment::Handoff {
                        handoff_kind,
                        work_id,
                        ..
                    } if handoff_kind == "live_transcript" => work_id.as_ref().is_some_and(|id| {
                        messages
                            .iter()
                            .any(|target| matches_live_target(target, id))
                    }),
                    _ => false,
                })
            })
        })
        .cloned()
        .collect();

    let mut result = filtered;
    for message in &messages {
        let Some(segments) = message.segments.as_ref() else {
            continue;
        };
        for segment in segments {
            let crate::contract::ChatSegment::Handoff {
                handoff_kind,
                text,
                work_id,
            } = segment
            else {
                continue;
            };
            if handoff_kind != "live_transcript" {
                continue;
            }
            let target_index = work_id
                .as_ref()
                .and_then(|id| {
                    result
                        .iter()
                        .position(|item| matches_live_target(item, id))
                })
                .or_else(|| result.iter().position(|item| item.id == message.id));
            let Some(target_index) = target_index else {
                continue;
            };
            let Ok(rows_value) = serde_json::from_str::<serde_json::Value>(text) else {
                continue;
            };
            let Some(rows_array) = rows_value.as_array() else {
                continue;
            };
            let mut rows = Vec::new();
            let mut valid = true;
            for row in rows_array {
                let role = row.get("role").and_then(|value| value.as_str());
                let text = row.get("text").and_then(|value| value.as_str());
                match (role, text) {
                    (Some("user"), Some(text)) => rows.push(LiveTranscriptRow {
                        role: ChatItemRole::User,
                        text: text.to_string(),
                    }),
                    (Some("assistant"), Some(text)) => rows.push(LiveTranscriptRow {
                        role: ChatItemRole::Assistant,
                        text: text.to_string(),
                    }),
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                continue;
            }
            let target = &mut result[target_index];
            let mut live = target.live_transcripts.clone().unwrap_or_default();
            live.push(LiveTranscriptAttachment {
                id: message.id.clone(),
                rows,
            });
            target.live_transcripts = Some(live);
            if target.id == message.id {
                target.segments = None;
                if target.content.trim().is_empty() {
                    target.content = "Voice transcript".into();
                }
            }
        }
    }
    result
}

fn project_entry(
    turn: &TranscriptEntry,
    index: usize,
    session_id: &str,
    authority_id: &str,
    lane: ChatLane,
    ask_job_id: Option<&str>,
) -> ChatItem {
    let parts = turn.turn.parts.as_deref();
    let model_receipt = model_receipt_from_parts(parts);
    let entry_id = turn.entry_id.as_str().trim();
    let live_handoff = parts.and_then(|parts| {
        parts.iter().find_map(|part| match part {
            medousa_types::TurnPart::Handoff {
                handoff_kind,
                text,
                work_id,
            } if handoff_kind == "live_transcript" => Some(crate::contract::ChatSegment::Handoff {
                handoff_kind: handoff_kind.clone(),
                text: text.clone(),
                work_id: work_id.clone(),
            }),
            _ => None,
        })
    });
    let segments = if live_handoff.is_some() {
        live_handoff.map(|segment| vec![segment])
    } else if turn.turn.role == "assistant" {
        chat_segments_from_parts(parts)
    } else {
        None
    };

    let content = if turn.turn.role == "assistant" && is_runtime_startup_text(Some(&turn.turn.content))
    {
        String::new()
    } else {
        turn.turn.content.clone()
    };

    let failed = turn.turn.answer_state.as_deref() == Some("failed");
    let error_line = if failed {
        turn.turn
            .slice_summary
            .as_ref()
            .and_then(|summary| summary.failures.first().cloned())
            .or_else(|| Some("This turn stopped before it could finish.".into()))
    } else {
        None
    };

    ChatItem {
        id: if !entry_id.is_empty() {
            format!("{session_id}:{entry_id}")
        } else {
            format!(
                "{session_id}:{}:{}:{index}",
                turn.turn.timestamp.to_rfc3339(),
                turn.turn.role,
            )
        },
        role: ChatItemRole::parse(&turn.turn.role),
        created_at: Some(turn.turn.timestamp.to_rfc3339()),
        turn_id: if turn.turn.role == "assistant" {
            turn.caused_by
                .as_ref()
                .map(|execution| execution.execution_id.as_str().to_string())
        } else {
            None
        },
        content,
        reactions: if turn.turn.reactions.is_empty() {
            None
        } else {
            Some(turn.turn.reactions.clone())
        },
        lane: Some(lane),
        ask_job_id: ask_job_id.map(str::to_string),
        turn_index: Some(if turn.entry_seq > 0 {
            turn.entry_seq
        } else {
            (index as u64) + 1
        }),
        answer_state: turn.turn.answer_state.clone(),
        failed: if failed { Some(true) } else { None },
        error_line,
        tools: if turn.turn.tool_names.is_empty() {
            None
        } else {
            Some(turn.turn.tool_names.clone())
        },
        tool_runs: tool_runs_from_parts(parts),
        status_line: if turn.turn.role == "assistant" && segments.is_none() {
            progress_from_parts(parts)
        } else {
            None
        },
        segments,
        ui_artifacts: ui_artifacts_from_parts(parts),
        reasoning: reasoning_from_parts(parts),
        media_attachments: media_from_parts(parts),
        host_context: host_context_from_parts(parts),
        speaker_profile_id: turn
            .turn
            .speaker_profile_id
            .as_ref()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty()),
        response_provider: model_receipt.as_ref().map(|(provider, _)| provider.clone()),
        response_model: model_receipt.map(|(_, model)| model),
        transcript: if !authority_id.is_empty() && !entry_id.is_empty() && turn.entry_seq > 0 {
            Some(TranscriptCoordinate {
                authority_id: authority_id.to_string(),
                session_id: session_id.to_string(),
                entry_id: entry_id.to_string(),
                entry_seq: turn.entry_seq,
                source: turn.source.as_ref().map(|source| TranscriptSourceRef {
                    authority_id: source.session.authority_id.as_str().to_string(),
                    session_id: source.session.session_id.as_str().to_string(),
                    entry_id: source.entry_id.as_str().to_string(),
                    entry_seq: source.entry_seq,
                }),
            })
        } else {
            None
        },
        streaming: None,
        work_id: None,
        phase: None,
        stage_whisper: None,
        budget_request_id: None,
        requested_rounds: None,
        error_detail: None,
        live_transcripts: None,
    }
}

/// Map daemon history turns into canonical chat items.
pub fn project_history(
    history: &SessionHistoryResponse,
    options: ProjectHistoryOptions,
) -> Vec<ChatItem> {
    let lane = options.lane.unwrap_or(ChatLane::Chat);
    let session_id = if history.session_id.trim().is_empty() {
        "session"
    } else {
        history.session_id.trim()
    };
    let authority_id = history.authority_id.as_str();
    let ask_job_id = options.ask_job_id.as_deref();
    let projected: Vec<ChatItem> = history
        .turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            project_entry(turn, index, session_id, authority_id, lane, ask_job_id)
        })
        .collect();
    attach_live_transcripts(projected)
}
