//! Permissive history wire shape for application projection.
//!
//! Daemon `TranscriptEntry` validates prefixed ids. The application core also
//! accepts UI/test fixtures that carry the same fields as plain strings.

use chrono::{DateTime, Utc};
use medousa_types::{HostTurnContext, MessageReaction, TurnPart};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WireTranscriptEntry {
    #[serde(default)]
    pub entry_id: String,
    #[serde(default)]
    pub entry_seq: u64,
    #[serde(default)]
    pub caused_by: Option<WireExecutionRef>,
    #[serde(default)]
    pub source: Option<WireTranscriptEntryRef>,
    #[serde(default)]
    pub content_digest: String,
    pub role: String,
    pub content: String,
    #[serde(default, alias = "created_at")]
    pub timestamp: Option<DateTime<Utc>>,
    /// Legacy / fixture alias for assistant turn ticket identity.
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub answer_state: Option<String>,
    #[serde(default)]
    pub parts: Option<Vec<TurnPart>>,
    /// Permissive summary: UI fixtures often send only `failures`.
    #[serde(default)]
    pub slice_summary: Option<WireSliceSummary>,
    #[serde(default)]
    pub speaker_profile_id: Option<String>,
    #[serde(default)]
    pub reactions: Vec<MessageReaction>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WireSliceSummary {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub tool_rounds: usize,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub outcomes: Vec<String>,
    #[serde(default)]
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireExecutionRef {
    #[serde(default)]
    pub authority_id: String,
    #[serde(default)]
    pub session_id: String,
    pub execution_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireTranscriptEntryRef {
    pub session: WireSessionRef,
    pub entry_id: String,
    pub entry_seq: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireSessionRef {
    pub authority_id: String,
    pub session_id: String,
}

use crate::contract::{
    ChatItem, ChatItemRole, ChatLane, ChatSegment, TranscriptCoordinate, TranscriptSourceRef,
};
use crate::display::is_runtime_startup_text;
use crate::parts::{
    chat_segments_from_parts, host_context_from_parts, media_from_parts, model_receipt_from_parts,
    progress_from_parts, reasoning_from_parts, tool_runs_from_parts, ui_artifacts_from_parts,
};
use crate::history::attach_live_transcripts;

fn project_wire_entry(
    turn: &WireTranscriptEntry,
    index: usize,
    session_id: &str,
    authority_id: &str,
    lane: ChatLane,
    ask_job_id: Option<&str>,
) -> ChatItem {
    let parts = turn.parts.as_deref();
    let model_receipt = model_receipt_from_parts(parts);
    let entry_id = turn.entry_id.trim();
    let live_handoff = parts.and_then(|parts| {
        parts.iter().find_map(|part| match part {
            TurnPart::Handoff {
                handoff_kind,
                text,
                work_id,
            } if handoff_kind == "live_transcript" => Some(ChatSegment::Handoff {
                handoff_kind: handoff_kind.clone(),
                text: text.clone(),
                work_id: work_id.clone(),
            }),
            _ => None,
        })
    });
    let segments = if live_handoff.is_some() {
        live_handoff.map(|segment| vec![segment])
    } else if turn.role == "assistant" {
        chat_segments_from_parts(parts)
    } else {
        None
    };

    let content =
        if turn.role == "assistant" && is_runtime_startup_text(Some(&turn.content)) {
            String::new()
        } else {
            turn.content.clone()
        };

    let failed = turn.answer_state.as_deref() == Some("failed");
    let error_line = if failed {
        turn.slice_summary
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
                turn.timestamp
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_else(|| index.to_string()),
                turn.role,
            )
        },
        role: ChatItemRole::parse(&turn.role),
        created_at: turn.timestamp.map(|value| value.to_rfc3339()),
        turn_id: if turn.role == "assistant" {
            turn.caused_by
                .as_ref()
                .map(|execution| execution.execution_id.clone())
                .or_else(|| {
                    turn.turn_id
                        .as_ref()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                })
        } else {
            None
        },
        content,
        reactions: if turn.reactions.is_empty() {
            None
        } else {
            Some(turn.reactions.clone())
        },
        lane: Some(lane),
        ask_job_id: ask_job_id.map(str::to_string),
        turn_index: Some(if turn.entry_seq > 0 {
            turn.entry_seq
        } else {
            (index as u64) + 1
        }),
        answer_state: turn.answer_state.clone(),
        failed: if failed { Some(true) } else { None },
        error_line,
        tools: if turn.tool_names.is_empty() {
            None
        } else {
            Some(turn.tool_names.clone())
        },
        tool_runs: tool_runs_from_parts(parts),
        status_line: if turn.role == "assistant" && segments.is_none() {
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
                    authority_id: source.session.authority_id.clone(),
                    session_id: source.session.session_id.clone(),
                    entry_id: source.entry_id.clone(),
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

pub fn project_wire_history(
    session_id: &str,
    authority_id: &str,
    turns: &[WireTranscriptEntry],
    lane: Option<ChatLane>,
    ask_job_id: Option<String>,
) -> Vec<ChatItem> {
    let lane = lane.unwrap_or(ChatLane::Chat);
    let session_id = if session_id.trim().is_empty() {
        "session"
    } else {
        session_id.trim()
    };
    let projected: Vec<ChatItem> = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            project_wire_entry(
                turn,
                index,
                session_id,
                authority_id.trim(),
                lane,
                ask_job_id.as_deref(),
            )
        })
        .collect();
    attach_live_transcripts(projected)
}

pub fn decode_wire_turns(values: &[serde_json::Value]) -> Vec<WireTranscriptEntry> {
    values
        .iter()
        .filter_map(|value| serde_json::from_value::<WireTranscriptEntry>(value.clone()).ok())
        .collect()
}

// Silence unused import warning for HostTurnContext re-export path via parts.
#[allow(dead_code)]
fn _host_context_marker(_: HostTurnContext) {}
