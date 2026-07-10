//! Bidirectional mapping between the durable [`TurnEvent`] spine and live SSE
//! [`InteractiveTurnStreamEvent`] wire payloads.

use chrono::Utc;
use medousa_engine::{SequencedTurnEvent, TurnEvent};
use medousa_types::daemon_api::{
    InteractiveTurnStreamEvent,
};

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
            session_id: event
                .browser_session_id
                .clone()
                .unwrap_or_default(),
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
    let turn_id = sequenced.envelope.turn_id.clone();
    let seq = sequenced.seq();
    match &sequenced.event {
        TurnEvent::StreamMirror(value) => {
            let mut map = value
                .as_object()
                .cloned()
                .unwrap_or_default();
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
        TurnEvent::Progress { message, tool_names } => {
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
            text,
            tool_names,
            ..
        } => {
            base.event_type = "final".to_string();
            base.phase = "completed".to_string();
            base.message = text.clone();
            base.final_text = Some(text.clone());
            base.tool_names = Some(tool_names.clone());
            base.terminal = true;
        }
        TurnEvent::NeedsInput {
            text,
            tool_names,
            ..
        } => {
            base.event_type = "needs_input".to_string();
            base.phase = "awaiting_operator".to_string();
            base.message = text.clone();
            base.final_text = Some(text.clone());
            base.tool_names = Some(tool_names.clone());
            base.terminal = true;
        }
        TurnEvent::Checkpoint {
            text,
            tool_names,
            ..
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
        ui_scene: None,
        operator_message: None,
        debug_message: None,
        browser_session_id: None,
        browser_challenge_url: None,
        context_usage: None,
    }
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
}
