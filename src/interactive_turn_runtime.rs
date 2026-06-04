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
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = final_stream_event_with_tools_terminal(turn_id, ack_text, tool_names, false)?;
    event.phase = "worker_ack".to_string();
    event.message = "background worker started".to_string();
    Ok(event)
}

pub fn final_stream_event_with_tools_terminal(
    turn_id: &str,
    final_text: &str,
    tool_names: Vec<String>,
    terminal: bool,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(turn_id, "final", "complete", "interactive turn complete")?;
    event.final_text = Some(final_text.to_string());
    event.tool_names = Some(tool_names);
    event.terminal = terminal;
    Ok(event)
}

pub fn error_stream_event(turn_id: &str, message: &str) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(turn_id, "error", "failed", message)?;
    event.terminal = true;
    Ok(event)
}

/// Clear in-flight assistant draft in the TUI before the next model round (tool-loop interim).
pub fn scratch_reset_stream_event(turn_id: &str) -> Result<InteractiveTurnStreamEvent> {
    build_event(turn_id, "scratch_reset", "streaming", "assistant scratch cleared")
}

fn build_event(
    turn_id: &str,
    event_type: &str,
    phase: &str,
    message: &str,
) -> Result<InteractiveTurnStreamEvent> {
    let turn_id = turn_id.trim().to_string();
    if turn_id.is_empty() {
        return Err(anyhow!("turn_id is required"));
    }

    Ok(InteractiveTurnStreamEvent {
        turn_id,
        event_type: event_type.to_string(),
        phase: phase.to_string(),
        message: message.to_string(),
        content_delta: None,
        reasoning_delta: None,
        final_text: None,
        tool_names: None,
        terminal: false,
        emitted_at_utc: Utc::now(),
    })
}
