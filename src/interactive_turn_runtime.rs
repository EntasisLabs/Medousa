use anyhow::{Result, anyhow};
use chrono::Utc;
use uuid::Uuid;

use crate::daemon_api::{
    InteractiveTurnRequest, InteractiveTurnResponse, InteractiveTurnStreamEvent,
};

pub fn start_interactive_turn_skeleton(
    request: InteractiveTurnRequest,
    daemon_base_url: &str,
) -> Result<InteractiveTurnResponse> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(anyhow!("session_id is required"));
    }

    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err(anyhow!("prompt is required"));
    }

    let turn_id = format!("daemon-turn-{}", Uuid::new_v4().simple());
    let stream_url = format!(
        "{}/v1/interactive/turn/{}/stream",
        daemon_base_url.trim_end_matches('/'),
        turn_id
    );

    Ok(InteractiveTurnResponse {
        turn_id,
        accepted_at_utc: Utc::now(),
        stream_url,
        stream_ready: false,
        fallback_to_local: true,
        fallback_reason: Some("daemon_interactive_stream_not_ready".to_string()),
        daemon_notice: Some(
            "interactive turn accepted by daemon skeleton; local runtime fallback remains active"
                .to_string(),
        ),
    })
}

pub fn build_interactive_turn_stream_event(turn_id: &str) -> Result<InteractiveTurnStreamEvent> {
    status_stream_event(
        turn_id,
        "skeleton",
        "interactive stream endpoint is active; daemon token streaming will be added in phase 2",
    )
}

pub fn build_interactive_turn_response(
    request: &InteractiveTurnRequest,
    daemon_base_url: &str,
    turn_id: &str,
    stream_ready: bool,
    fallback_to_local: bool,
    fallback_reason: Option<String>,
    daemon_notice: Option<String>,
) -> Result<InteractiveTurnResponse> {
    let session_id = request.session_id.trim().to_string();
    if session_id.is_empty() {
        return Err(anyhow!("session_id is required"));
    }

    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err(anyhow!("prompt is required"));
    }

    let stream_url = format!(
        "{}/v1/interactive/turn/{}/stream",
        daemon_base_url.trim_end_matches('/'),
        turn_id
    );

    Ok(InteractiveTurnResponse {
        turn_id: turn_id.to_string(),
        accepted_at_utc: Utc::now(),
        stream_url,
        stream_ready,
        fallback_to_local,
        fallback_reason,
        daemon_notice,
    })
}

pub fn status_stream_event(
    turn_id: &str,
    phase: &str,
    message: &str,
) -> Result<InteractiveTurnStreamEvent> {
    build_event(turn_id, "status", phase, message)
}

pub fn debug_status_stream_event(
    turn_id: &str,
    phase: &str,
    debug_message: &str,
) -> Result<InteractiveTurnStreamEvent> {
    build_event_messages(
        turn_id,
        "status",
        phase,
        StreamMessages {
            operator_message: None,
            debug_message: Some(debug_message.trim().to_string()),
        },
    )
}

pub fn operator_status_stream_event(
    turn_id: &str,
    phase: &str,
    operator_message: &str,
) -> Result<InteractiveTurnStreamEvent> {
    build_event_messages(
        turn_id,
        "status",
        phase,
        StreamMessages {
            operator_message: Some(operator_message.trim().to_string()),
            debug_message: None,
        },
    )
}

/// Whether a stream status line is engine telemetry rather than operator copy.
pub fn is_stream_debug_telemetry(message: &str) -> bool {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('◈') {
        return true;
    }
    if trimmed.contains("orchestrator=") || trimmed.contains("fallback=") {
        return true;
    }
    trimmed.starts_with("tool=")
}

#[derive(Debug, Clone, Default)]
struct StreamMessages {
    operator_message: Option<String>,
    debug_message: Option<String>,
}

fn classify_stream_messages(phase: &str, message: &str) -> StreamMessages {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return StreamMessages::default();
    }
    if phase == "orchestration" || phase == "tool" || is_stream_debug_telemetry(trimmed) {
        return StreamMessages {
            operator_message: None,
            debug_message: Some(trimmed.to_string()),
        };
    }
    StreamMessages {
        operator_message: Some(trimmed.to_string()),
        debug_message: None,
    }
}

fn legacy_stream_message(messages: &StreamMessages) -> String {
    messages
        .operator_message
        .clone()
        .or_else(|| messages.debug_message.clone())
        .unwrap_or_default()
}

pub fn content_delta_stream_event(turn_id: &str, delta: &str) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(turn_id, "content_delta", "streaming", "")?;
    event.content_delta = Some(delta.to_string());
    Ok(event)
}

pub fn reasoning_delta_stream_event(
    turn_id: &str,
    delta: &str,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(turn_id, "reasoning_delta", "streaming", "")?;
    event.reasoning_delta = Some(delta.to_string());
    Ok(event)
}

pub fn turn_progress_stream_event(
    turn_id: &str,
    progress: &str,
    tool_names: Vec<String>,
) -> Result<InteractiveTurnStreamEvent> {
    let messages = classify_stream_messages("tool_loop", progress);
    let mut event = build_event_messages(turn_id, "turn_progress", "tool_loop", messages)?;
    event.tool_names = Some(tool_names);
    event.terminal = false;
    Ok(event)
}

/// Mid-task handoff: principal sees a durable update; turn ends without claiming final completion.
pub fn turn_checkpoint_stream_event(
    turn_id: &str,
    message: &str,
    tool_names: Vec<String>,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(
        turn_id,
        "turn_checkpoint",
        "handoff",
        "Medousa handed the turn back to you — reply when ready to continue",
    )?;
    event.final_text = Some(message.to_string());
    event.tool_names = Some(tool_names);
    event.terminal = true;
    Ok(event)
}

pub fn final_stream_event(turn_id: &str, final_text: &str) -> Result<InteractiveTurnStreamEvent> {
    final_stream_event_with_tools(turn_id, final_text, Vec::new())
}

pub fn final_stream_event_with_tools(
    turn_id: &str,
    final_text: &str,
    tool_names: Vec<String>,
) -> Result<InteractiveTurnStreamEvent> {
    final_stream_event_with_tools_terminal(turn_id, final_text, tool_names, true)
}

pub fn worker_ack_stream_event_with_tools(
    turn_id: &str,
    ack_text: &str,
    tool_names: Vec<String>,
    work_id: Option<&str>,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = final_stream_event_with_tools_terminal(turn_id, ack_text, tool_names, false)?;
    event.event_type = "worker_ack".to_string();
    event.phase = "worker_ack".to_string();
    event.message = "background worker started".to_string();
    event.operator_message = Some("background worker started".to_string());
    event.debug_message = None;
    event.work_id = work_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(event)
}

pub fn final_stream_event_with_tools_terminal(
    turn_id: &str,
    final_text: &str,
    tool_names: Vec<String>,
    terminal: bool,
) -> Result<InteractiveTurnStreamEvent> {
    final_stream_event_terminal_commit(turn_id, Some(final_text), tool_names, terminal)
}

/// Terminal commit. When `final_text` is `None`, clients keep streamed body (Phase 7A).
pub fn final_stream_event_terminal_commit(
    turn_id: &str,
    final_text: Option<&str>,
    tool_names: Vec<String>,
    terminal: bool,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(turn_id, "final", "complete", "interactive turn complete")?;
    event.final_text = final_text.map(str::to_string);
    event.tool_names = Some(tool_names);
    event.terminal = terminal;
    Ok(event)
}

pub fn final_pending_stream_event_with_tools(
    turn_id: &str,
    status_text: &str,
    tool_names: Vec<String>,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(
        turn_id,
        "final_pending",
        "wrapping_up",
        "Medousa is preparing your final answer",
    )?;
    event.final_text = Some(status_text.to_string());
    event.tool_names = Some(tool_names);
    event.terminal = false;
    Ok(event)
}

pub fn needs_input_stream_event_with_tools(
    turn_id: &str,
    question_text: &str,
    tool_names: Vec<String>,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(
        turn_id,
        "needs_input",
        "awaiting_operator",
        "Medousa is asking for clarification",
    )?;
    event.final_text = Some(question_text.to_string());
    event.tool_names = Some(tool_names);
    event.terminal = true;
    Ok(event)
}

pub fn error_stream_event(turn_id: &str, message: &str) -> Result<InteractiveTurnStreamEvent> {
    error_stream_event_from_failure(turn_id, &crate::turn_failure::TurnFailure::from_debug(message))
}

pub fn error_stream_event_from_failure(
    turn_id: &str,
    failure: &crate::turn_failure::TurnFailure,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event_messages(
        turn_id,
        "error",
        "failed",
        StreamMessages {
            operator_message: Some(failure.operator_message.clone()),
            debug_message: Some(failure.debug_message.clone()),
        },
    )?;
    event.terminal = true;
    Ok(event)
}

/// Clear in-flight assistant draft in the TUI before the next model round (tool-loop interim).
pub fn scratch_reset_stream_event(turn_id: &str) -> Result<InteractiveTurnStreamEvent> {
    build_event_messages(
        turn_id,
        "scratch_reset",
        "streaming",
        StreamMessages {
            operator_message: None,
            debug_message: Some("assistant scratch cleared".to_string()),
        },
    )
}

pub fn tool_started_stream_event(
    turn_id: &str,
    tool_run_id: &str,
    tool_name: &str,
    input_summary: &str,
    tool_round: usize,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event_messages(
        turn_id,
        "tool_started",
        "tool_loop",
        StreamMessages {
            operator_message: Some(format!("Running {tool_name}")),
            debug_message: None,
        },
    )?;
    event.tool_run_id = Some(tool_run_id.to_string());
    event.tool_name = Some(tool_name.to_string());
    event.tool_status = Some("running".to_string());
    event.tool_input_summary = Some(input_summary.to_string());
    event.tool_round = Some(tool_round.max(1));
    Ok(event)
}

pub fn tool_finished_stream_event(
    turn_id: &str,
    tool_run_id: &str,
    tool_name: &str,
    status: &str,
    input_summary: &str,
    output_summary: Option<&str>,
    tool_round: usize,
    artifact_refs: Vec<crate::daemon_api::StreamToolArtifactRef>,
) -> Result<InteractiveTurnStreamEvent> {
    let message = match output_summary.filter(|value| !value.trim().is_empty()) {
        Some(summary) => format!("{tool_name}: {summary}"),
        None => format!("{tool_name} {status}"),
    };
    let mut event = build_event_messages(
        turn_id,
        "tool_finished",
        "tool_loop",
        StreamMessages {
            operator_message: Some(message),
            debug_message: None,
        },
    )?;
    event.tool_run_id = Some(tool_run_id.to_string());
    event.tool_name = Some(tool_name.to_string());
    event.tool_status = Some(status.to_string());
    event.tool_input_summary = Some(input_summary.to_string());
    event.tool_output_summary = output_summary.map(str::to_string);
    event.tool_round = Some(tool_round.max(1));
    if !artifact_refs.is_empty() {
        event.tool_artifact_refs = Some(artifact_refs);
    }
    Ok(event)
}

pub fn artifact_presented_stream_event(
    turn_id: &str,
    artifact: crate::daemon_api::StreamUiArtifact,
) -> Result<InteractiveTurnStreamEvent> {
    let label = artifact.label.clone();
    let mut event = build_event_messages(
        turn_id,
        "artifact_presented",
        "tool_loop",
        StreamMessages {
            operator_message: Some(format!("Presented {label}")),
            debug_message: None,
        },
    )?;
    event.ui_artifact = Some(artifact);
    Ok(event)
}

pub fn artifact_updated_stream_event(
    turn_id: &str,
    previous_artifact_id: &str,
    artifact: crate::daemon_api::StreamUiArtifact,
    root_artifact_id: Option<&str>,
) -> Result<InteractiveTurnStreamEvent> {
    let label = artifact.label.clone();
    let mut event = build_event_messages(
        turn_id,
        "artifact_updated",
        "tool_loop",
        StreamMessages {
            operator_message: Some(format!("Updated {label}")),
            debug_message: None,
        },
    )?;
    event.ui_artifact = Some(artifact);
    event.previous_artifact_id = Some(previous_artifact_id.to_string());
    event.root_artifact_id = root_artifact_id.map(str::to_string);
    Ok(event)
}

pub fn budget_approval_stream_event(
    turn_id: &str,
    request_id: &str,
    rounds_executed: usize,
    max_tool_rounds: usize,
    requested_rounds: usize,
    reason: &str,
    progress_summary: Option<&str>,
) -> Result<InteractiveTurnStreamEvent> {
    let summary = progress_summary
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .unwrap_or("");
    let message = format!(
        "Turn paused at {rounds_executed}/{max_tool_rounds}. Requesting +{requested_rounds} rounds: {reason}"
    );
    let mut operator_line = format!("{message} (request {request_id})");
    if !summary.is_empty() {
        operator_line.push_str(&format!(". Progress: {summary}"));
    }
    let mut event = build_event_messages(
        turn_id,
        "budget_approval",
        "awaiting_operator",
        StreamMessages {
            operator_message: Some(operator_line.clone()),
            debug_message: None,
        },
    )?;
    event.budget_request_id = Some(request_id.to_string());
    event.requested_rounds = Some(requested_rounds);
    event.terminal = false;
    Ok(event)
}

fn build_event(
    turn_id: &str,
    event_type: &str,
    phase: &str,
    message: &str,
) -> Result<InteractiveTurnStreamEvent> {
    build_event_messages(turn_id, event_type, phase, classify_stream_messages(phase, message))
}

fn build_event_messages(
    turn_id: &str,
    event_type: &str,
    phase: &str,
    messages: StreamMessages,
) -> Result<InteractiveTurnStreamEvent> {
    let turn_id = turn_id.trim().to_string();
    if turn_id.is_empty() {
        return Err(anyhow!("turn_id is required"));
    }

    Ok(InteractiveTurnStreamEvent {
        turn_id,
        event_type: event_type.to_string(),
        phase: phase.to_string(),
        message: legacy_stream_message(&messages),
        operator_message: messages.operator_message,
        debug_message: messages.debug_message,
        content_delta: None,
        reasoning_delta: None,
        final_text: None,
        tool_names: None,
        terminal: false,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_output_summary: None,
        tool_round: None,
        tool_artifact_refs: None,
        ui_artifact: None,
        previous_artifact_id: None,
        root_artifact_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_presented_stream_event_roundtrips() {
        let event = artifact_presented_stream_event(
            "turn-1",
            crate::daemon_api::StreamUiArtifact {
                artifact_id: "art:demo:ui:abc".to_string(),
                mime: "text/html".to_string(),
                label: "Chart".to_string(),
                presentation: "inline".to_string(),
                byte_size: Some(42),
                height_px: Some(360),
            },
        )
        .expect("event");
        assert_eq!(event.event_type, "artifact_presented");
        let json = serde_json::to_string(&event).expect("json");
        let parsed: InteractiveTurnStreamEvent = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.ui_artifact.as_ref().map(|a| a.label.as_str()), Some("Chart"));
    }

    #[test]
    fn classifies_orchestration_as_debug_only() {
        let messages = classify_stream_messages(
            "orchestration",
            "◈ orchestration_summary calls_total=1 final_mode=tool_loop",
        );
        assert!(messages.operator_message.is_none());
        assert!(messages.debug_message.is_some());
    }

    #[test]
    fn classifies_operator_progress_as_operator_message() {
        let event = turn_progress_stream_event(
            "turn-1",
            "Wrapping up your answer…",
            vec!["cognition_web_search".to_string()],
        )
        .expect("event");
        assert_eq!(
            event.operator_message.as_deref(),
            Some("Wrapping up your answer…")
        );
        assert!(event.debug_message.is_none());
    }

    #[test]
    fn status_stream_event_keeps_legacy_message_for_tui() {
        let event =
            debug_status_stream_event("turn-1", "orchestration", "◈ activation heuristic class=tool")
                .expect("event");
        assert_eq!(
            event.message,
            "◈ activation heuristic class=tool"
        );
        assert!(event.operator_message.is_none());
        assert_eq!(
            event.debug_message.as_deref(),
            Some("◈ activation heuristic class=tool")
        );
    }

    #[test]
    fn error_event_splits_operator_and_debug() {
        let failure = crate::turn_failure::TurnFailure::from_debug("HTTP 429 rate limit");
        let event = error_stream_event_from_failure("turn-1", &failure).expect("event");
        assert_eq!(event.event_type, "error");
        assert_eq!(
            event.operator_message.as_deref(),
            Some(failure.operator_message.as_str())
        );
        assert_eq!(
            event.debug_message.as_deref(),
            Some(failure.debug_message.as_str())
        );
        assert!(event.terminal);
    }
}
