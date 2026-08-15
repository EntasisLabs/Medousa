//! Bidirectional mapping between the durable [`TurnEvent`] spine and live SSE
//! [`InteractiveTurnStreamEvent`] wire payloads.

use chrono::Utc;
use medousa_engine::{SequencedTurnEvent, TurnEvent};
use medousa_types::daemon_api::InteractiveTurnStreamEvent;
use medousa_types::turn_stream::{TurnStreamEnvelopeV2, TurnStreamEventV2, WorkerAckKind};

/// Lift a live SSE payload into the typed spine vocabulary for journaling.
pub fn stream_event_to_turn_event(event: &InteractiveTurnStreamEvent) -> TurnEvent {
    match event.event_type.as_str() {
        "content_delta" => TurnEvent::ContentDelta {
            delta: event.content_delta.clone().unwrap_or_default(),
        },
        "reasoning_delta" => TurnEvent::ReasoningDelta {
            delta: event.reasoning_delta.clone().unwrap_or_default(),
        },
        "turn_progress" => TurnEvent::Progress {
            message: event.message.clone(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "assistant_pack_hold" => TurnEvent::Progress {
            message: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "status" => TurnEvent::Status {
            phase: event.phase.clone(),
            message: event.message.clone(),
            operator_message: event.operator_message.clone(),
            debug_message: event.debug_message.clone(),
        },
        "scratch_reset" => TurnEvent::ScratchReset,
        "tool_started" => TurnEvent::ToolRunStarted {
            tool_run_id: event
                .tool_run_id
                .clone()
                .unwrap_or_else(|| "tool-run".to_string()),
            tool_name: event
                .tool_name
                .clone()
                .unwrap_or_else(|| "tool".to_string()),
            input_summary: event.tool_input_summary.clone().unwrap_or_default(),
            tool_round: event.tool_round.unwrap_or(1),
        },
        "tool_finished" => TurnEvent::ToolRunFinished {
            tool_run_id: event
                .tool_run_id
                .clone()
                .unwrap_or_else(|| "tool-run".to_string()),
            tool_name: event
                .tool_name
                .clone()
                .unwrap_or_else(|| "tool".to_string()),
            status: event
                .tool_status
                .clone()
                .unwrap_or_else(|| "finished".to_string()),
            output_summary: event.tool_output_summary.clone(),
            tool_round: event.tool_round.unwrap_or(1),
        },
        "final" => TurnEvent::FinalResponse {
            text: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            parts: vec![],
            committed_at: event.emitted_at_utc,
        },
        "needs_input" => TurnEvent::NeedsInput {
            text: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            parts: vec![],
            committed_at: event.emitted_at_utc,
        },
        "checkpoint" => TurnEvent::Checkpoint {
            text: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            parts: vec![],
            committed_at: event.emitted_at_utc,
        },
        "worker_ack" => TurnEvent::WorkerAck {
            text: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            work_id: event.work_id.clone(),
            parts: vec![],
            committed_at: event.emitted_at_utc,
        },
        "workshop_ack" => TurnEvent::WorkerAck {
            text: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            work_id: event.work_id.clone(),
            parts: vec![],
            committed_at: event.emitted_at_utc,
        },
        "worker_synthesis" => TurnEvent::FinalResponse {
            text: event
                .final_text
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            parts: vec![],
            committed_at: event.emitted_at_utc,
        },
        "budget_approval" => TurnEvent::BudgetApprovalRequired {
            request_id: event
                .budget_request_id
                .clone()
                .unwrap_or_else(|| "budget".to_string()),
            rounds_executed: 0,
            max_tool_rounds: 0,
            requested_rounds: event.requested_rounds.unwrap_or(0),
            reason: event.message.clone(),
            progress_summary: event.operator_message.clone(),
        },
        "browser_challenge" => TurnEvent::BrowserChallenge {
            session_id: event.browser_session_id.clone().unwrap_or_default(),
            challenge_url: event.browser_challenge_url.clone().unwrap_or_default(),
            reason: event.message.clone(),
        },
        "browser_navigated" => TurnEvent::BrowserNavigated {
            url: event.message.clone(),
            title: event.operator_message.clone(),
            opened_by_agent: false,
        },
        "error" => TurnEvent::Error {
            message: event.message.clone(),
        },
        _ => stream_mirror_from_event(event),
    }
}

fn stream_mirror_from_event(event: &InteractiveTurnStreamEvent) -> TurnEvent {
    let mut value = serde_json::to_value(event).unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.remove("turn_id");
        map.remove("seq");
    }
    TurnEvent::StreamMirror(value)
}

/// Project a sequenced spine event back to the SSE wire shape for replay.
pub fn sequenced_to_stream_event(sequenced: &SequencedTurnEvent) -> InteractiveTurnStreamEvent {
    if let Some(event) = &sequenced.stream_event_v2 {
        let envelope = TurnStreamEnvelopeV2::new(
            &sequenced.envelope.turn_id,
            sequenced.seq(),
            sequenced.emitted_at_utc.unwrap_or_else(Utc::now),
            event.clone(),
        )
        .expect("sequenced journal event has valid envelope");
        return v2_to_v1(&envelope);
    }
    let turn_id = sequenced.envelope.turn_id.clone();
    let seq = sequenced.seq();
    match &sequenced.event {
        TurnEvent::StreamMirror(value) => {
            let mut map = value.as_object().cloned().unwrap_or_default();
            map.insert(
                "turn_id".to_string(),
                serde_json::Value::String(turn_id.to_string()),
            );
            map.insert("seq".to_string(), serde_json::Value::Number(seq.into()));
            let mut event: InteractiveTurnStreamEvent =
                serde_json::from_value(serde_json::Value::Object(map))
                    .unwrap_or_else(|_| empty_stream_event(&turn_id));
            event.turn_id = turn_id.to_string();
            event.seq = seq;
            event
        }
        other => typed_turn_event_to_stream(&turn_id, seq, other),
    }
}

pub fn sequenced_to_v2(sequenced: &SequencedTurnEvent) -> Result<TurnStreamEnvelopeV2, String> {
    if let Some(event) = &sequenced.stream_event_v2 {
        return TurnStreamEnvelopeV2::new(
            sequenced.envelope.turn_id.clone(),
            sequenced.seq(),
            sequenced.emitted_at_utc.unwrap_or_else(Utc::now),
            event.clone(),
        )
        .map_err(|error| error.to_string());
    }
    let event = match &sequenced.event {
        TurnEvent::StreamMirror(_) => v1_to_v2(&sequenced_to_stream_event(sequenced))?.event,
        TurnEvent::ContentDelta { delta } => TurnStreamEventV2::ContentAppend {
            text: delta.clone(),
        },
        TurnEvent::ReasoningDelta { delta } => TurnStreamEventV2::ReasoningAppend {
            text: delta.clone(),
        },
        TurnEvent::Progress {
            message,
            tool_names,
        } => TurnStreamEventV2::Progress {
            message: message.clone(),
            tool_names: tool_names.clone(),
        },
        TurnEvent::ScratchReset => TurnStreamEventV2::ScratchReset,
        TurnEvent::ToolRunStarted {
            tool_run_id,
            tool_name,
            input_summary,
            tool_round,
        } => TurnStreamEventV2::ToolStarted {
            tool_run_id: tool_run_id.clone(),
            tool_name: tool_name.clone(),
            input_summary: input_summary.clone(),
            input_params: Vec::new(),
            tool_round: *tool_round,
        },
        TurnEvent::ToolRunFinished {
            tool_run_id,
            tool_name,
            status,
            output_summary,
            tool_round,
        } => TurnStreamEventV2::ToolFinished {
            tool_run_id: tool_run_id.clone(),
            tool_name: tool_name.clone(),
            status: status.clone(),
            input_summary: String::new(),
            input_params: Vec::new(),
            output_summary: output_summary.clone(),
            tool_round: *tool_round,
            artifact_refs: Vec::new(),
        },
        TurnEvent::Notice { message } => TurnStreamEventV2::Status {
            phase: "tool_loop".to_string(),
            operator_message: None,
            debug_message: Some(message.clone()),
        },
        TurnEvent::FinalResponse {
            text, tool_names, ..
        } => TurnStreamEventV2::Final {
            text: text.clone(),
            tool_names: tool_names.clone(),
        },
        TurnEvent::NeedsInput {
            text, tool_names, ..
        } => TurnStreamEventV2::NeedsInput {
            text: text.clone(),
            tool_names: tool_names.clone(),
        },
        TurnEvent::Checkpoint {
            text, tool_names, ..
        } => TurnStreamEventV2::Checkpoint {
            text: text.clone(),
            tool_names: tool_names.clone(),
        },
        TurnEvent::WorkerAck {
            text,
            tool_names,
            work_id,
            ..
        } => TurnStreamEventV2::WorkerAck {
            ack_kind: WorkerAckKind::Worker,
            text: text.clone(),
            tool_names: tool_names.clone(),
            work_id: work_id.clone(),
        },
        TurnEvent::BudgetApprovalRequired {
            request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            reason,
            progress_summary,
        } => TurnStreamEventV2::BudgetApprovalRequired {
            request_id: request_id.clone(),
            rounds_executed: *rounds_executed,
            max_tool_rounds: *max_tool_rounds,
            requested_rounds: *requested_rounds,
            reason: reason.clone(),
            progress_summary: progress_summary.clone(),
        },
        TurnEvent::Status {
            phase,
            operator_message,
            debug_message,
            ..
        } => TurnStreamEventV2::Status {
            phase: phase.clone(),
            operator_message: operator_message.clone(),
            debug_message: debug_message.clone(),
        },
        TurnEvent::BrowserChallenge {
            session_id,
            challenge_url,
            reason,
        } => TurnStreamEventV2::BrowserChallenge {
            session_id: session_id.clone(),
            challenge_url: challenge_url.clone(),
            reason: reason.clone(),
        },
        TurnEvent::BrowserNavigated {
            url,
            title,
            opened_by_agent,
        } => TurnStreamEventV2::BrowserNavigated {
            url: url.clone(),
            title: title.clone(),
            opened_by_agent: *opened_by_agent,
        },
        TurnEvent::Error { message } => TurnStreamEventV2::Error {
            operator_message: message.clone(),
            debug_message: None,
        },
    };
    TurnStreamEnvelopeV2::new(
        sequenced.envelope.turn_id.clone(),
        sequenced.seq(),
        sequenced.emitted_at_utc.unwrap_or_else(Utc::now),
        event,
    )
    .map_err(|error| error.to_string())
}

fn typed_turn_event_to_stream(
    turn_id: &str,
    seq: u64,
    event: &TurnEvent,
) -> InteractiveTurnStreamEvent {
    let mut base = empty_stream_event(turn_id);
    base.seq = seq;
    match event {
        TurnEvent::ContentDelta { delta } => {
            base.event_type = "content_delta".to_string();
            base.phase = "streaming".to_string();
            base.content_delta = Some(delta.clone());
        }
        TurnEvent::ReasoningDelta { delta } => {
            base.event_type = "reasoning_delta".to_string();
            base.phase = "streaming".to_string();
            base.reasoning_delta = Some(delta.clone());
        }
        TurnEvent::Progress {
            message,
            tool_names,
        } => {
            base.event_type = "turn_progress".to_string();
            base.phase = "tool_loop".to_string();
            base.message = message.clone();
            base.tool_names = Some(tool_names.clone());
        }
        TurnEvent::Status {
            phase,
            message,
            operator_message,
            debug_message,
        } => {
            base.event_type = "status".to_string();
            base.phase = phase.clone();
            base.message = message.clone();
            base.operator_message = operator_message.clone();
            base.debug_message = debug_message.clone();
        }
        TurnEvent::ScratchReset => {
            base.event_type = "scratch_reset".to_string();
            base.phase = "streaming".to_string();
        }
        TurnEvent::ToolRunStarted {
            tool_run_id,
            tool_name,
            input_summary,
            tool_round,
        } => {
            base.event_type = "tool_started".to_string();
            base.phase = "tool_loop".to_string();
            base.message = format!("Running {tool_name}");
            base.operator_message = Some(format!("Running {tool_name}"));
            base.tool_run_id = Some(tool_run_id.clone());
            base.tool_name = Some(tool_name.clone());
            base.tool_status = Some("running".to_string());
            base.tool_input_summary = Some(input_summary.clone());
            base.tool_round = Some(*tool_round);
        }
        TurnEvent::ToolRunFinished {
            tool_run_id,
            tool_name,
            status,
            output_summary,
            tool_round,
        } => {
            base.event_type = "tool_finished".to_string();
            base.phase = "tool_loop".to_string();
            base.message = output_summary
                .as_deref()
                .map(|summary| format!("{tool_name}: {summary}"))
                .unwrap_or_else(|| format!("{tool_name} {status}"));
            base.tool_run_id = Some(tool_run_id.clone());
            base.tool_name = Some(tool_name.clone());
            base.tool_status = Some(status.clone());
            base.tool_output_summary = output_summary.clone();
            base.tool_round = Some(*tool_round);
        }
        TurnEvent::FinalResponse {
            text, tool_names, ..
        } => {
            base.event_type = "final".to_string();
            base.phase = "completed".to_string();
            base.message = text.clone();
            base.final_text = Some(text.clone());
            base.tool_names = Some(tool_names.clone());
            base.terminal = true;
        }
        TurnEvent::NeedsInput {
            text, tool_names, ..
        } => {
            base.event_type = "needs_input".to_string();
            base.phase = "awaiting_operator".to_string();
            base.message = text.clone();
            base.final_text = Some(text.clone());
            base.tool_names = Some(tool_names.clone());
            base.terminal = true;
        }
        TurnEvent::Checkpoint {
            text, tool_names, ..
        } => {
            base.event_type = "checkpoint".to_string();
            base.phase = "awaiting_operator".to_string();
            base.message = text.clone();
            base.final_text = Some(text.clone());
            base.tool_names = Some(tool_names.clone());
            base.terminal = true;
        }
        TurnEvent::WorkerAck {
            text,
            tool_names,
            work_id,
            ..
        } => {
            base.event_type = "worker_ack".to_string();
            base.phase = "handoff".to_string();
            base.message = text.clone();
            base.final_text = Some(text.clone());
            base.tool_names = Some(tool_names.clone());
            base.work_id = work_id.clone();
        }
        TurnEvent::BudgetApprovalRequired {
            request_id,
            requested_rounds,
            reason,
            progress_summary,
            ..
        } => {
            base.event_type = "budget_approval".to_string();
            base.phase = "awaiting_operator".to_string();
            base.message = reason.clone();
            base.budget_request_id = Some(request_id.clone());
            base.requested_rounds = Some(*requested_rounds);
            base.operator_message = progress_summary.clone();
        }
        TurnEvent::BrowserChallenge {
            session_id,
            challenge_url,
            reason,
        } => {
            base.event_type = "browser_challenge".to_string();
            base.phase = "awaiting_operator".to_string();
            base.message = reason.clone();
            base.browser_session_id = Some(session_id.clone());
            base.browser_challenge_url = Some(challenge_url.clone());
        }
        TurnEvent::BrowserNavigated { url, title, .. } => {
            base.event_type = "browser_navigated".to_string();
            base.phase = "tool".to_string();
            base.message = url.clone();
            base.operator_message = title.clone();
        }
        TurnEvent::Notice { message } => {
            base.event_type = "status".to_string();
            base.phase = "tool_loop".to_string();
            base.message = message.clone();
            base.debug_message = Some(message.clone());
        }
        TurnEvent::Error { message } => {
            base.event_type = "error".to_string();
            base.phase = "failed".to_string();
            base.message = message.clone();
            base.terminal = true;
        }
        TurnEvent::StreamMirror(_) => {}
    }
    base
}

fn empty_stream_event(turn_id: &str) -> InteractiveTurnStreamEvent {
    InteractiveTurnStreamEvent {
        turn_id: turn_id.to_string(),
        seq: 0,
        event_type: String::new(),
        phase: String::new(),
        message: String::new(),
        content_delta: None,
        reasoning_delta: None,
        final_text: None,
        tool_names: None,
        response_provider: None,
        response_model: None,
        terminal: false,
        emitted_at_utc: Utc::now(),
        budget_request_id: None,
        requested_rounds: None,
        work_id: None,
        tool_run_id: None,
        tool_name: None,
        tool_status: None,
        tool_input_summary: None,
        tool_input_params: None,
        tool_output_summary: None,
        tool_round: None,
        tool_artifact_refs: None,
        ui_artifact: None,
        previous_artifact_id: None,
        root_artifact_id: None,
        ui_scene: None,
        operator_message: None,
        debug_message: None,
        browser_session_id: None,
        browser_challenge_url: None,
        context_usage: None,
        permission_request_id: None,
        agent_session_id: None,
        agent_runtime: None,
    }
}

/// The only v2-to-v1 compatibility projection. New producers emit v2; legacy
/// clients receive this deliberately lossy nullable DTO until v1 removal.
pub fn v2_to_v1(envelope: &TurnStreamEnvelopeV2) -> InteractiveTurnStreamEvent {
    let mut wire = empty_stream_event(&envelope.turn_id);
    wire.seq = envelope.seq;
    wire.emitted_at_utc = envelope.emitted_at_utc;
    match &envelope.event {
        TurnStreamEventV2::ContentAppend { text } => {
            wire.event_type = "content_delta".to_string();
            wire.phase = "streaming".to_string();
            wire.content_delta = Some(text.clone());
        }
        TurnStreamEventV2::ReasoningAppend { text } => {
            wire.event_type = "reasoning_delta".to_string();
            wire.phase = "streaming".to_string();
            wire.reasoning_delta = Some(text.clone());
        }
        TurnStreamEventV2::Status {
            phase,
            operator_message,
            debug_message,
        } => {
            wire.event_type = "status".to_string();
            wire.phase = phase.clone();
            wire.message = operator_message
                .clone()
                .or_else(|| debug_message.clone())
                .unwrap_or_default();
            wire.operator_message = operator_message.clone();
            wire.debug_message = debug_message.clone();
        }
        TurnStreamEventV2::Progress {
            message,
            tool_names,
        } => {
            wire.event_type = "turn_progress".to_string();
            wire.phase = "tool_loop".to_string();
            wire.message = message.clone();
            wire.operator_message = Some(message.clone());
            wire.tool_names = Some(tool_names.clone());
        }
        TurnStreamEventV2::PackHold { text, tool_names } => {
            wire.event_type = "assistant_pack_hold".to_string();
            wire.phase = "pack_hold".to_string();
            wire.message = text.clone();
            wire.operator_message = Some(text.clone());
            wire.final_text = Some(text.clone());
            wire.tool_names = Some(tool_names.clone());
        }
        TurnStreamEventV2::ModelReceipt { provider, model } => {
            wire.event_type = "model_receipt".to_string();
            wire.phase = "inference".to_string();
            wire.message = "Inference route selected".to_string();
            wire.response_provider = Some(provider.clone());
            wire.response_model = Some(model.clone());
        }
        TurnStreamEventV2::Final { text, tool_names } => {
            terminal_body(&mut wire, "final", "complete", text, tool_names);
        }
        TurnStreamEventV2::NeedsInput { text, tool_names } => {
            terminal_body(
                &mut wire,
                "needs_input",
                "awaiting_operator",
                text,
                tool_names,
            );
        }
        TurnStreamEventV2::Checkpoint { text, tool_names } => {
            terminal_body(&mut wire, "turn_checkpoint", "handoff", text, tool_names);
        }
        TurnStreamEventV2::WorkerAck {
            ack_kind,
            text,
            tool_names,
            work_id,
        } => {
            wire.event_type = match ack_kind {
                WorkerAckKind::Worker => "worker_ack",
                WorkerAckKind::Workshop => "workshop_ack",
            }
            .to_string();
            wire.phase = wire.event_type.clone();
            wire.message = match ack_kind {
                WorkerAckKind::Worker => "background worker started",
                WorkerAckKind::Workshop => "bound workshop started",
            }
            .to_string();
            wire.operator_message = Some(wire.message.clone());
            wire.final_text = Some(text.clone());
            wire.tool_names = Some(tool_names.clone());
            wire.work_id = work_id.clone();
        }
        TurnStreamEventV2::WorkerSynthesis {
            text,
            tool_names,
            work_id,
        } => {
            terminal_body(
                &mut wire,
                "worker_synthesis",
                "worker_synthesis",
                text,
                tool_names,
            );
            wire.work_id = work_id.clone();
        }
        TurnStreamEventV2::FinalPending { text, tool_names } => {
            wire.event_type = "final_pending".to_string();
            wire.phase = "wrapping_up".to_string();
            wire.message = "Medousa is preparing your final answer".to_string();
            wire.operator_message = Some(wire.message.clone());
            wire.final_text = Some(text.clone());
            wire.tool_names = Some(tool_names.clone());
        }
        TurnStreamEventV2::Error {
            operator_message,
            debug_message,
        } => {
            wire.event_type = "error".to_string();
            wire.phase = "failed".to_string();
            wire.message = operator_message.clone();
            wire.operator_message = Some(operator_message.clone());
            wire.debug_message = debug_message.clone();
            wire.terminal = true;
        }
        TurnStreamEventV2::ScratchReset => {
            wire.event_type = "scratch_reset".to_string();
            wire.phase = "streaming".to_string();
            wire.debug_message = Some("assistant scratch cleared".to_string());
        }
        TurnStreamEventV2::ToolStarted {
            tool_run_id,
            tool_name,
            input_summary,
            input_params,
            tool_round,
        } => {
            wire.event_type = "tool_started".to_string();
            wire.phase = "tool_loop".to_string();
            wire.message = format!("Running {tool_name}");
            wire.operator_message = Some(wire.message.clone());
            wire.tool_run_id = Some(tool_run_id.clone());
            wire.tool_name = Some(tool_name.clone());
            wire.tool_status = Some("running".to_string());
            wire.tool_input_summary = Some(input_summary.clone());
            wire.tool_input_params = (!input_params.is_empty()).then(|| input_params.clone());
            wire.tool_round = Some(*tool_round);
        }
        TurnStreamEventV2::ToolFinished {
            tool_run_id,
            tool_name,
            status,
            input_summary,
            input_params,
            output_summary,
            tool_round,
            artifact_refs,
        } => {
            wire.event_type = "tool_finished".to_string();
            wire.phase = "tool_loop".to_string();
            wire.message = output_summary
                .as_ref()
                .map(|summary| format!("{tool_name}: {summary}"))
                .unwrap_or_else(|| format!("{tool_name} {status}"));
            wire.operator_message = Some(wire.message.clone());
            wire.tool_run_id = Some(tool_run_id.clone());
            wire.tool_name = Some(tool_name.clone());
            wire.tool_status = Some(status.clone());
            wire.tool_input_summary = Some(input_summary.clone());
            wire.tool_input_params = (!input_params.is_empty()).then(|| input_params.clone());
            wire.tool_output_summary = output_summary.clone();
            wire.tool_round = Some(*tool_round);
            wire.tool_artifact_refs = (!artifact_refs.is_empty()).then(|| artifact_refs.clone());
        }
        TurnStreamEventV2::ArtifactPresented { artifact } => {
            wire.event_type = "artifact_presented".to_string();
            wire.phase = "tool_loop".to_string();
            wire.message = format!("Presented {}", artifact.label);
            wire.operator_message = Some(wire.message.clone());
            wire.ui_artifact = Some(artifact.clone());
        }
        TurnStreamEventV2::ArtifactUpdated {
            previous_artifact_id,
            artifact,
            root_artifact_id,
        } => {
            wire.event_type = "artifact_updated".to_string();
            wire.phase = "tool_loop".to_string();
            wire.message = format!("Updated {}", artifact.label);
            wire.operator_message = Some(wire.message.clone());
            wire.ui_artifact = Some(artifact.clone());
            wire.previous_artifact_id = Some(previous_artifact_id.clone());
            wire.root_artifact_id = root_artifact_id.clone();
        }
        TurnStreamEventV2::UiScene { scene } => {
            wire.event_type = "ui_scene".to_string();
            wire.phase = "tool_loop".to_string();
            wire.message = "Updated the view".to_string();
            wire.operator_message = Some(wire.message.clone());
            wire.ui_scene = Some(scene.clone());
        }
        TurnStreamEventV2::BudgetApprovalRequired {
            request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            reason,
            progress_summary,
        } => {
            wire.event_type = "budget_approval".to_string();
            wire.phase = "awaiting_operator".to_string();
            wire.message = format!(
                "Turn paused at {rounds_executed}/{max_tool_rounds}. Requesting +{requested_rounds} rounds: {reason}"
            );
            wire.operator_message = progress_summary
                .clone()
                .or_else(|| Some(wire.message.clone()));
            wire.budget_request_id = Some(request_id.clone());
            wire.requested_rounds = Some(*requested_rounds);
        }
        TurnStreamEventV2::BrowserChallenge {
            session_id,
            challenge_url,
            reason,
        } => {
            wire.event_type = "browser_challenge".to_string();
            wire.phase = "awaiting_operator".to_string();
            wire.message = reason.clone();
            wire.operator_message = Some(reason.clone());
            wire.browser_session_id = Some(session_id.clone());
            wire.browser_challenge_url = Some(challenge_url.clone());
        }
        TurnStreamEventV2::BrowserNavigated { url, title, .. } => {
            wire.event_type = "browser_navigated".to_string();
            wire.phase = "tool".to_string();
            wire.message = url.clone();
            wire.operator_message = title.clone();
        }
        TurnStreamEventV2::ContextUsage {
            report,
            operator_summary,
        } => {
            wire.event_type = "context_usage".to_string();
            wire.phase = "orchestration".to_string();
            wire.message = operator_summary.clone().unwrap_or_default();
            wire.operator_message = operator_summary.clone();
            wire.context_usage = Some(report.clone());
        }
        TurnStreamEventV2::PermissionRequest {
            request_id,
            message,
            agent_session_id,
            agent_runtime,
        } => {
            wire.event_type = "permission_request".to_string();
            wire.phase = "awaiting_permission".to_string();
            wire.message = message.clone();
            wire.operator_message = Some(message.clone());
            wire.permission_request_id = Some(request_id.clone());
            wire.agent_session_id = agent_session_id.clone();
            wire.agent_runtime = agent_runtime.clone();
        }
    }
    wire
}

/// Lift a legacy wire event into the v2 envelope. New pipeline events bypass
/// this compatibility path and retain their original typed representation.
pub fn v1_to_v2(event: &InteractiveTurnStreamEvent) -> Result<TurnStreamEnvelopeV2, String> {
    let text = || {
        event
            .final_text
            .clone()
            .unwrap_or_else(|| event.message.clone())
    };
    let typed = match event.event_type.as_str() {
        "content_delta" => TurnStreamEventV2::ContentAppend {
            text: event.content_delta.clone().unwrap_or_default(),
        },
        "reasoning_delta" => TurnStreamEventV2::ReasoningAppend {
            text: event.reasoning_delta.clone().unwrap_or_default(),
        },
        "status" => TurnStreamEventV2::Status {
            phase: event.phase.clone(),
            operator_message: event.operator_message.clone(),
            debug_message: event.debug_message.clone(),
        },
        "turn_progress" => TurnStreamEventV2::Progress {
            message: event.message.clone(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "assistant_pack_hold" => TurnStreamEventV2::PackHold {
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "model_receipt" => TurnStreamEventV2::ModelReceipt {
            provider: event.response_provider.clone().unwrap_or_default(),
            model: event.response_model.clone().unwrap_or_default(),
        },
        "final" => TurnStreamEventV2::Final {
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "needs_input" => TurnStreamEventV2::NeedsInput {
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "checkpoint" | "turn_checkpoint" => TurnStreamEventV2::Checkpoint {
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "worker_synthesis" => TurnStreamEventV2::WorkerSynthesis {
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            work_id: event.work_id.clone(),
        },
        "worker_ack" | "workshop_ack" => TurnStreamEventV2::WorkerAck {
            ack_kind: if event.event_type == "workshop_ack" {
                WorkerAckKind::Workshop
            } else {
                WorkerAckKind::Worker
            },
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
            work_id: event.work_id.clone(),
        },
        "final_pending" => TurnStreamEventV2::FinalPending {
            text: text(),
            tool_names: event.tool_names.clone().unwrap_or_default(),
        },
        "error" => TurnStreamEventV2::Error {
            operator_message: event
                .operator_message
                .clone()
                .unwrap_or_else(|| event.message.clone()),
            debug_message: event.debug_message.clone(),
        },
        "scratch_reset" => TurnStreamEventV2::ScratchReset,
        "tool_started" => TurnStreamEventV2::ToolStarted {
            tool_run_id: required_legacy(&event.tool_run_id, "tool_run_id")?,
            tool_name: required_legacy(&event.tool_name, "tool_name")?,
            input_summary: required_legacy(&event.tool_input_summary, "tool_input_summary")?,
            input_params: event.tool_input_params.clone().unwrap_or_default(),
            tool_round: event.tool_round.unwrap_or(1),
        },
        "tool_finished" => TurnStreamEventV2::ToolFinished {
            tool_run_id: required_legacy(&event.tool_run_id, "tool_run_id")?,
            tool_name: required_legacy(&event.tool_name, "tool_name")?,
            status: required_legacy(&event.tool_status, "tool_status")?,
            input_summary: event.tool_input_summary.clone().unwrap_or_default(),
            input_params: event.tool_input_params.clone().unwrap_or_default(),
            output_summary: event.tool_output_summary.clone(),
            tool_round: event.tool_round.unwrap_or(1),
            artifact_refs: event.tool_artifact_refs.clone().unwrap_or_default(),
        },
        "artifact_presented" => TurnStreamEventV2::ArtifactPresented {
            artifact: required_legacy(&event.ui_artifact, "ui_artifact")?,
        },
        "artifact_updated" => TurnStreamEventV2::ArtifactUpdated {
            previous_artifact_id: required_legacy(
                &event.previous_artifact_id,
                "previous_artifact_id",
            )?,
            artifact: required_legacy(&event.ui_artifact, "ui_artifact")?,
            root_artifact_id: event.root_artifact_id.clone(),
        },
        "ui_scene" => TurnStreamEventV2::UiScene {
            scene: required_legacy(&event.ui_scene, "ui_scene")?,
        },
        "budget_approval" => TurnStreamEventV2::BudgetApprovalRequired {
            request_id: required_legacy(&event.budget_request_id, "budget_request_id")?,
            rounds_executed: 0,
            max_tool_rounds: 0,
            requested_rounds: event.requested_rounds.unwrap_or(0),
            reason: event.message.clone(),
            progress_summary: event.operator_message.clone(),
        },
        "browser_challenge" => TurnStreamEventV2::BrowserChallenge {
            session_id: required_legacy(&event.browser_session_id, "browser_session_id")?,
            challenge_url: required_legacy(&event.browser_challenge_url, "browser_challenge_url")?,
            reason: event.message.clone(),
        },
        "browser_navigated" => TurnStreamEventV2::BrowserNavigated {
            url: event.message.clone(),
            title: event.operator_message.clone(),
            opened_by_agent: false,
        },
        "context_usage" => TurnStreamEventV2::ContextUsage {
            report: required_legacy(&event.context_usage, "context_usage")?,
            operator_summary: event.operator_message.clone(),
        },
        "permission_request" => TurnStreamEventV2::PermissionRequest {
            request_id: required_legacy(&event.permission_request_id, "permission_request_id")?,
            message: event.message.clone(),
            agent_session_id: event.agent_session_id.clone(),
            agent_runtime: event.agent_runtime.clone(),
        },
        other => {
            return Err(format!(
                "legacy stream event '{other}' has no v2 projection"
            ));
        }
    };
    TurnStreamEnvelopeV2::new(
        event.turn_id.clone(),
        event.seq,
        event.emitted_at_utc,
        typed,
    )
    .map_err(|error| error.to_string())
}

fn required_legacy<T: Clone>(value: &Option<T>, field: &str) -> Result<T, String> {
    value
        .clone()
        .ok_or_else(|| format!("legacy stream event is missing {field}"))
}

fn terminal_body(
    wire: &mut InteractiveTurnStreamEvent,
    event_type: &str,
    phase: &str,
    text: &str,
    tool_names: &[String],
) {
    wire.event_type = event_type.to_string();
    wire.phase = phase.to_string();
    wire.message = text.to_string();
    wire.final_text = Some(text.to_string());
    wire.tool_names = Some(tool_names.to_vec());
    wire.terminal = true;
}

/// Prefer a lossless mirror for wire events carrying rich UI / artifact fields.
pub fn journal_turn_event_for_stream(
    event: &InteractiveTurnStreamEvent,
    journal_override: Option<TurnEvent>,
) -> TurnEvent {
    if let Some(typed) = journal_override {
        return typed;
    }
    if event.ui_artifact.is_some()
        || event.previous_artifact_id.is_some()
        || event.tool_artifact_refs.is_some()
        || event.context_usage.is_some()
        || event.ui_scene.is_some()
        || matches!(
            event.event_type.as_str(),
            "artifact_presented" | "artifact_updated" | "ui_scene"
        )
    {
        return stream_mirror_from_event(event);
    }
    stream_event_to_turn_event(event)
}

/// Preserve the canonical v2 payload in the journal while retaining the typed
/// spine variants used by history folding and terminal commit semantics.
pub fn journal_turn_event_for_v2(envelope: &TurnStreamEnvelopeV2) -> TurnEvent {
    match &envelope.event {
        TurnStreamEventV2::ContentAppend { text } => TurnEvent::ContentDelta {
            delta: text.clone(),
        },
        TurnStreamEventV2::ReasoningAppend { text } => TurnEvent::ReasoningDelta {
            delta: text.clone(),
        },
        TurnStreamEventV2::Progress {
            message,
            tool_names,
        } => TurnEvent::Progress {
            message: message.clone(),
            tool_names: tool_names.clone(),
        },
        TurnStreamEventV2::ScratchReset => TurnEvent::ScratchReset,
        TurnStreamEventV2::ToolStarted {
            tool_run_id,
            tool_name,
            input_summary,
            input_params,
            tool_round,
        } if input_params.is_empty() => TurnEvent::ToolRunStarted {
            tool_run_id: tool_run_id.clone(),
            tool_name: tool_name.clone(),
            input_summary: input_summary.clone(),
            tool_round: *tool_round,
        },
        TurnStreamEventV2::ToolFinished {
            tool_run_id,
            tool_name,
            status,
            input_summary,
            input_params,
            output_summary,
            tool_round,
            artifact_refs,
        } if input_summary.is_empty() && input_params.is_empty() && artifact_refs.is_empty() => {
            TurnEvent::ToolRunFinished {
                tool_run_id: tool_run_id.clone(),
                tool_name: tool_name.clone(),
                status: status.clone(),
                output_summary: output_summary.clone(),
                tool_round: *tool_round,
            }
        }
        TurnStreamEventV2::Final { text, tool_names } => TurnEvent::FinalResponse {
            text: text.clone(),
            tool_names: tool_names.clone(),
            parts: Vec::new(),
            committed_at: envelope.emitted_at_utc,
        },
        TurnStreamEventV2::NeedsInput { text, tool_names } => TurnEvent::NeedsInput {
            text: text.clone(),
            tool_names: tool_names.clone(),
            parts: Vec::new(),
            committed_at: envelope.emitted_at_utc,
        },
        TurnStreamEventV2::Checkpoint { text, tool_names } => TurnEvent::Checkpoint {
            text: text.clone(),
            tool_names: tool_names.clone(),
            parts: Vec::new(),
            committed_at: envelope.emitted_at_utc,
        },
        TurnStreamEventV2::WorkerAck {
            ack_kind: WorkerAckKind::Worker,
            text,
            tool_names,
            work_id,
        } => TurnEvent::WorkerAck {
            text: text.clone(),
            tool_names: tool_names.clone(),
            work_id: work_id.clone(),
            parts: Vec::new(),
            committed_at: envelope.emitted_at_utc,
        },
        TurnStreamEventV2::BudgetApprovalRequired {
            request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            reason,
            progress_summary,
        } => TurnEvent::BudgetApprovalRequired {
            request_id: request_id.clone(),
            rounds_executed: *rounds_executed,
            max_tool_rounds: *max_tool_rounds,
            requested_rounds: *requested_rounds,
            reason: reason.clone(),
            progress_summary: progress_summary.clone(),
        },
        TurnStreamEventV2::Status {
            phase,
            operator_message,
            debug_message,
        } => TurnEvent::Status {
            phase: phase.clone(),
            message: operator_message
                .clone()
                .or_else(|| debug_message.clone())
                .unwrap_or_default(),
            operator_message: operator_message.clone(),
            debug_message: debug_message.clone(),
        },
        TurnStreamEventV2::BrowserChallenge {
            session_id,
            challenge_url,
            reason,
        } => TurnEvent::BrowserChallenge {
            session_id: session_id.clone(),
            challenge_url: challenge_url.clone(),
            reason: reason.clone(),
        },
        TurnStreamEventV2::BrowserNavigated {
            url,
            title,
            opened_by_agent,
        } => TurnEvent::BrowserNavigated {
            url: url.clone(),
            title: title.clone(),
            opened_by_agent: *opened_by_agent,
        },
        TurnStreamEventV2::Error {
            operator_message, ..
        } => TurnEvent::Error {
            message: operator_message.clone(),
        },
        _ => journal_turn_event_for_stream(&v2_to_v1(envelope), None),
    }
}

/// Freeze only variants whose v2 fields cannot be recovered from the typed
/// domain event. Common token/status events avoid duplicating their payload in
/// every journal record.
pub fn frozen_v2_replay_event(event: &TurnStreamEventV2) -> Option<TurnStreamEventV2> {
    match event {
        TurnStreamEventV2::PackHold { .. }
        | TurnStreamEventV2::ModelReceipt { .. }
        | TurnStreamEventV2::WorkerAck {
            ack_kind: WorkerAckKind::Workshop,
            ..
        }
        | TurnStreamEventV2::WorkerSynthesis { .. }
        | TurnStreamEventV2::FinalPending { .. }
        | TurnStreamEventV2::ArtifactPresented { .. }
        | TurnStreamEventV2::ArtifactUpdated { .. }
        | TurnStreamEventV2::UiScene { .. }
        | TurnStreamEventV2::ContextUsage { .. }
        | TurnStreamEventV2::PermissionRequest { .. } => Some(event.clone()),
        TurnStreamEventV2::ToolStarted { input_params, .. } if !input_params.is_empty() => {
            Some(event.clone())
        }
        TurnStreamEventV2::ToolFinished {
            input_summary,
            input_params,
            artifact_refs,
            ..
        } if !input_summary.is_empty() || !input_params.is_empty() || !artifact_refs.is_empty() => {
            Some(event.clone())
        }
        TurnStreamEventV2::Error {
            debug_message: Some(_),
            ..
        } => Some(event.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::StreamUiArtifact;
    use medousa_engine::{Principal, TurnEnvelope};

    #[test]
    fn artifact_presented_roundtrips_through_spine_mirror() {
        let turn_id = "turn-artifact";
        let artifact = StreamUiArtifact {
            artifact_id: "art-1".to_string(),
            mime: "text/html".to_string(),
            label: "Chart".to_string(),
            presentation: "inline".to_string(),
            byte_size: Some(42),
            height_px: Some(240),
        };
        let wire = crate::interactive_turn_runtime::artifact_presented_stream_event(
            turn_id,
            artifact.clone(),
        )
        .expect("wire event");
        let journal = journal_turn_event_for_stream(&wire, None);
        let envelope = TurnEnvelope::new(turn_id, Principal::operator());
        let sequenced = SequencedTurnEvent {
            envelope: envelope.at_seq(3),
            event: journal,
            emitted_at_utc: None,
            stream_event_v2: None,
        };
        let replay = sequenced_to_stream_event(&sequenced);
        assert_eq!(replay.event_type, "artifact_presented");
        assert_eq!(
            replay.ui_artifact.as_ref().map(|a| a.label.as_str()),
            Some("Chart")
        );
    }

    #[test]
    fn content_delta_maps_typed_without_mirror() {
        let wire =
            crate::interactive_turn_runtime::content_delta_stream_event("turn-1", "hello").unwrap();
        match stream_event_to_turn_event(&wire) {
            TurnEvent::ContentDelta { delta } => assert_eq!(delta, "hello"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn v2_projection_preserves_envelope_and_variant_semantics() {
        let emitted_at = Utc::now();
        let envelope = TurnStreamEnvelopeV2::new(
            "turn-v2",
            42,
            emitted_at,
            TurnStreamEventV2::Checkpoint {
                text: "continue when ready".to_string(),
                tool_names: vec!["search".to_string()],
            },
        )
        .unwrap();
        let legacy = v2_to_v1(&envelope);
        assert_eq!(legacy.turn_id, "turn-v2");
        assert_eq!(legacy.seq, 42);
        assert_eq!(legacy.emitted_at_utc, emitted_at);
        assert_eq!(legacy.event_type, "turn_checkpoint");
        assert_eq!(legacy.final_text.as_deref(), Some("continue when ready"));
        assert!(legacy.terminal);
    }

    #[test]
    fn v2_budget_fields_survive_durable_replay() {
        let envelope = TurnStreamEnvelopeV2::new(
            "turn-budget",
            7,
            Utc::now(),
            TurnStreamEventV2::BudgetApprovalRequired {
                request_id: "budget-1".to_string(),
                rounds_executed: 4,
                max_tool_rounds: 5,
                requested_rounds: 3,
                reason: "more work".to_string(),
                progress_summary: Some("almost there".to_string()),
            },
        )
        .unwrap();
        let sequenced = SequencedTurnEvent {
            envelope: TurnEnvelope::new("turn-budget", Principal::operator()).at_seq(7),
            event: journal_turn_event_for_v2(&envelope),
            emitted_at_utc: Some(envelope.emitted_at_utc),
            stream_event_v2: frozen_v2_replay_event(&envelope.event),
        };

        let replay = sequenced_to_v2(&sequenced).unwrap();
        assert_eq!(replay.emitted_at_utc, envelope.emitted_at_utc);
        assert!(sequenced.stream_event_v2.is_none());
        match replay.event {
            TurnStreamEventV2::BudgetApprovalRequired {
                rounds_executed,
                max_tool_rounds,
                requested_rounds,
                ..
            } => {
                assert_eq!(rounds_executed, 4);
                assert_eq!(max_tool_rounds, 5);
                assert_eq!(requested_rounds, 3);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn rich_v2_events_replay_without_legacy_field_loss() {
        let artifact = StreamUiArtifact {
            artifact_id: "art-2".to_string(),
            mime: "text/html".to_string(),
            label: "Dashboard".to_string(),
            presentation: "inline".to_string(),
            byte_size: Some(128),
            height_px: Some(480),
        };
        let envelope = TurnStreamEnvelopeV2::new(
            "turn-rich",
            9,
            Utc::now(),
            TurnStreamEventV2::ArtifactPresented {
                artifact: artifact.clone(),
            },
        )
        .unwrap();
        let sequenced = SequencedTurnEvent {
            envelope: TurnEnvelope::new("turn-rich", Principal::operator()).at_seq(9),
            event: journal_turn_event_for_v2(&envelope),
            emitted_at_utc: Some(envelope.emitted_at_utc),
            stream_event_v2: frozen_v2_replay_event(&envelope.event),
        };

        let replay = sequenced_to_v2(&sequenced).unwrap();
        match replay.event {
            TurnStreamEventV2::ArtifactPresented { artifact: replayed } => {
                assert_eq!(replayed.artifact_id, artifact.artifact_id);
                assert_eq!(replayed.height_px, artifact.height_px);
            }
            other => panic!("unexpected {other:?}"),
        }
        let legacy = sequenced_to_stream_event(&sequenced);
        assert_eq!(legacy.event_type, "artifact_presented");
        assert_eq!(legacy.ui_artifact.unwrap().label, "Dashboard");
    }

    #[test]
    fn workshop_ack_freezes_only_the_ambiguous_v2_payload() {
        let envelope = TurnStreamEnvelopeV2::new(
            "turn-workshop",
            11,
            Utc::now(),
            TurnStreamEventV2::WorkerAck {
                ack_kind: WorkerAckKind::Workshop,
                text: "workshop started".to_string(),
                tool_names: vec!["forge".to_string()],
                work_id: Some("work-1".to_string()),
            },
        )
        .unwrap();
        let sequenced = SequencedTurnEvent {
            envelope: TurnEnvelope::new("turn-workshop", Principal::operator()).at_seq(11),
            event: journal_turn_event_for_v2(&envelope),
            emitted_at_utc: Some(envelope.emitted_at_utc),
            stream_event_v2: frozen_v2_replay_event(&envelope.event),
        };

        assert!(sequenced.stream_event_v2.is_some());
        match sequenced_to_v2(&sequenced).unwrap().event {
            TurnStreamEventV2::WorkerAck { ack_kind, .. } => {
                assert!(matches!(ack_kind, WorkerAckKind::Workshop));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            sequenced_to_stream_event(&sequenced).event_type,
            "workshop_ack"
        );
    }
}
