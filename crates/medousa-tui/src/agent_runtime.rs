use super::TuiState;
use super::daemon_commands::daemon_start_interactive_turn;
use futures_util::StreamExt;
use medousa_tui::{InteractiveTurnRequest, events::TuiEvent};
use medousa_types::TurnStreamEnvelopeV3;
#[cfg(test)]
use medousa_types::TurnStreamEventV3;
use tokio::sync::mpsc;

pub(crate) async fn start_prompt_run(
    state: &mut TuiState,
    event_tx: &mpsc::Sender<TuiEvent>,
    prompt: String,
    persist_user_turn: bool,
) {
    if state.is_processing {
        super::push_obs(
            state,
            "⚠ this pane is already running a turn (Ctrl+G to stop)".to_string(),
        );
        return;
    }
    if super::workspace_runtime::live_stream_count(state)
        >= super::workspace_runtime::MAX_LIVE_STREAMS
    {
        super::push_obs(
            state,
            "⚠ live stream cap reached (max 4) — wait or stop a turn".to_string(),
        );
        return;
    }
    match attempt_daemon_interactive_turn(state, &prompt, persist_user_turn).await {
        Ok(response) if response.stream_ready && !response.fallback_to_local => {
            if let Some(notice) = response.daemon_notice {
                super::push_obs(state, notice);
            }
            start_daemon_stream_prompt_run(
                state,
                event_tx,
                &prompt,
                persist_user_turn,
                &response.turn_id,
            )
            .await;
        }
        Ok(response) => super::push_obs_alert(
            state,
            format!(
                "Daemon could not stream turn {}: {}",
                response.turn_id,
                response
                    .fallback_reason
                    .or(response.daemon_notice)
                    .unwrap_or_else(|| "stream unavailable".into())
            ),
        ),
        Err(error) => super::push_obs_alert(state, format!("Daemon turn request failed: {error}")),
    }
}

async fn attempt_daemon_interactive_turn(
    state: &TuiState,
    prompt: &str,
    persist_user_turn: bool,
) -> std::result::Result<medousa_tui::InteractiveTurnResponse, String> {
    let request = InteractiveTurnRequest {
        session_id: state.session_id.clone(),
        prompt: prompt.to_string(),
        agent_mode: None,
        code_context: None,
        code_project_setup_authorized: false,
        worker_execution_target: None,
        persist_user_turn,
        response_depth_mode: state.response_depth_mode.clone(),
        reasoning_effort: state.reasoning_effort.clone(),
        provider: state.settings.provider.clone(),
        model: state.settings.model.clone(),
        stage_routing: state.stage_routing.clone(),
        surface: Some(medousa_tui::TurnSurfaceContext::tui()),
        host_context: None,
        // The daemon reads the persisted General-mode setting. Leaving this
        // unset also lets Coder select its independent 100-round default.
        max_tool_rounds: None,
        retry_runtime_max_rounds: Some(medousa_tui::tui::settings::parse_usize_with_bounds(
            &state.settings.retry_runtime_max_rounds,
            medousa_tui::preferences::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
            medousa_tui::preferences::ROUND_LIMIT_MIN,
            medousa_tui::preferences::ROUND_LIMIT_MAX,
        )),
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
        scheduled_tool_allowlist: None,
        voice_preset_id: None,
        voice_appendix: None,
        media_refs: Vec::new(),
        liquid_interactions: Vec::new(),
        identity_user_id: None,
        wake: None,
    };

    daemon_start_interactive_turn(&state.daemon_url, &request)
        .await
        .map_err(|err| err.to_string())
}

async fn start_daemon_stream_prompt_run(
    state: &mut TuiState,
    event_tx: &mpsc::Sender<TuiEvent>,
    prompt: &str,
    persist_user_turn: bool,
    daemon_turn_id: &str,
) {
    state.active_agent_turn_id = state.active_agent_turn_id.saturating_add(1);
    let turn_id = state.active_agent_turn_id;
    state.open_stream_turn_id = Some(turn_id);
    state.is_processing = true;
    state.auto_scroll = true;
    state.conv_scroll = state.conv_max_scroll;
    state.active_agent_stream_turn = None;
    state.turn_parts.reset();
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    state.in_thinking_tag = false;
    state.stream_tag_tail.clear();
    state.received_native_reasoning = false;
    state.pending_response_verified = None;
    super::workspace_runtime::register_stream_turn(state, turn_id);

    if persist_user_turn {
        let user_turn = medousa_tui::turn_parts::user_conversation_turn(prompt.to_string());
        state.conversation.push(user_turn);
    }

    let tx = event_tx.clone();
    let daemon_url = state.daemon_url.clone();
    let stream_path = format!(
        "/v1/interactive/turn/{}/stream",
        urlencoding::encode(daemon_turn_id)
    );
    let daemon_turn_id = daemon_turn_id.to_string();
    let handle = tokio::spawn(async move {
        if let Err(err) =
            consume_daemon_interactive_stream(&daemon_url, &stream_path, turn_id, &tx).await
        {
            let _ = tx
                .send(TuiEvent::AgentError {
                    turn_id,
                    message: format!(
                        "daemon interactive stream {} failed: {}",
                        daemon_turn_id, err
                    ),
                })
                .await;
        }
    });

    state.active_request_task = Some(handle);
}

async fn consume_daemon_interactive_stream(
    daemon_url: &str,
    stream_path: &str,
    turn_id: u64,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> std::result::Result<(), String> {
    let sdk = super::daemon_commands::daemon_client(daemon_url).map_err(|e| e.to_string())?;
    let interactive = sdk.interactive();
    let mut events = interactive.stream_reconnecting_v3(stream_path);
    let mut saw_terminal = false;

    while let Some(payload) = events.next().await {
        let payload = payload.map_err(|err| err.to_string())?;
        if dispatch_daemon_stream_event(payload, turn_id, event_tx).await? {
            saw_terminal = true;
        }
    }

    if !saw_terminal {
        return Err("stream closed without terminal event".to_string());
    }

    Ok(())
}

async fn dispatch_daemon_stream_event(
    payload: TurnStreamEnvelopeV3,
    turn_id: u64,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> std::result::Result<bool, String> {
    let terminal = payload.event.is_terminal();
    event_tx
        .send(TuiEvent::TurnStreamV3 {
            turn_id,
            envelope: payload,
        })
        .await
        .map_err(|err| err.to_string())?;

    Ok(terminal)
}

pub(crate) async fn cancel_turn(state: &mut TuiState) {
    if !state.is_processing {
        return;
    }
    let result = async {
        let client = super::daemon_commands::daemon_client(&state.daemon_url)?;
        client.interactive().cancel(&state.session_id).await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(err) = result {
        super::push_obs_alert(state, format!("Could not cancel the daemon turn: {err}"));
    }
    // The daemon publishes the terminal fact on the existing stream.
}

pub(crate) fn detach_stream(state: &mut TuiState) {
    let turn_id = state.open_stream_turn_id;
    if let Some(task) = state.active_request_task.take() {
        task.abort();
        state.is_processing = false;
        state.open_stream_turn_id = None;
        state.active_agent_stream_turn = None;
        state.pending_response_verified = None;
        state.pending_agent_chunk_delta.clear();
        state.pending_agent_chunk_count = 0;
        if let Some(tid) = turn_id {
            super::workspace_runtime::clear_stream_turn(state, tid);
        }
        state.session_tasks.remove(&state.session_id);
        super::flush_thinking_buffer(state);
    }
}

#[cfg(test)]
mod stream_v3_tests {
    use super::*;

    #[tokio::test]
    async fn typed_completion_dispatches_one_native_terminal_fact() {
        let envelope = TurnStreamEnvelopeV3::new(
            "daemon-turn-1",
            1,
            chrono::Utc::now(),
            TurnStreamEventV3::TurnCompleted {
                outcome: medousa_types::TurnCompletionOutcomeV3::Completed,
                aggregate_text: "done".to_string(),
                tool_names: vec!["search".to_string()],
                operator_message: None,
                debug_message: None,
            },
        )
        .expect("v3 envelope");
        let (event_tx, mut event_rx) = mpsc::channel(1);

        let terminal = dispatch_daemon_stream_event(envelope, 7, &event_tx)
            .await
            .expect("dispatch");

        assert!(terminal);
        assert!(matches!(
            event_rx.recv().await,
            Some(TuiEvent::TurnStreamV3 {
                turn_id: 7,
                envelope: TurnStreamEnvelopeV3 {
                    event: TurnStreamEventV3::TurnCompleted { aggregate_text, .. },
                    ..
                },
            }) if aggregate_text == "done"
        ));
    }
}
