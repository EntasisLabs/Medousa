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

pub fn turn_progress_stream_event(
    turn_id: &str,
    progress: &str,
    tool_names: Vec<String>,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(turn_id, "turn_progress", "tool_loop", progress)?;
    event.tool_names = Some(tool_names);
    event.terminal = false;
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
    event.phase = "worker_ack".to_string();
    event.message = "background worker started".to_string();
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
    let mut event = build_event(turn_id, "error", "failed", message)?;
    event.terminal = true;
    Ok(event)
}

/// Clear in-flight assistant draft in the TUI before the next model round (tool-loop interim).
pub fn scratch_reset_stream_event(turn_id: &str) -> Result<InteractiveTurnStreamEvent> {
    build_event(turn_id, "scratch_reset", "streaming", "assistant scratch cleared")
}

pub fn tool_started_stream_event(
    turn_id: &str,
    tool_run_id: &str,
    tool_name: &str,
    input_summary: &str,
    tool_round: usize,
) -> Result<InteractiveTurnStreamEvent> {
    let mut event = build_event(
        turn_id,
        "tool_started",
        "tool_loop",
        &format!("Running {tool_name}"),
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
    let mut event = build_event(turn_id, "tool_finished", "tool_loop", &message)?;
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
    let mut event = build_event(turn_id, "budget_approval", "awaiting_operator", &message)?;
    event.message = format!("{message} (request {request_id})");
    event.budget_request_id = Some(request_id.to_string());
    event.requested_rounds = Some(requested_rounds);
    if !summary.is_empty() {
        event.message.push_str(&format!(". Progress: {summary}"));
    }
    event.terminal = false;
    Ok(event)
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
    })
}
