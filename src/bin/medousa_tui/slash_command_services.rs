use super::*;

pub(crate) async fn handle_new_session_command(
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) {
    stop_active_generation(state);
    state.session_id = Uuid::new_v4().simple().to_string();
    state.selected_context_pack_query = None;
    state.conversation.clear();
    invalidate_markdown_cache(state);
    state.active_agent_stream_turn = None;
    state.thinking_trace.clear();
    state.thinking_scroll = 0;
    state.thinking_max_scroll = 0;
    state.in_thinking_tag = false;
    state.stream_tag_tail.clear();
    state.is_processing = false;
    state.open_stream_turn_id = None;
    state.pending_agent_chunk_delta.clear();
    state.pending_agent_chunk_count = 0;
    state.auto_scroll = true;
    state.conv_scroll = 0;
    save_last_session_id(&state.session_id);
    push_obs(state, format!("✓ new session {}", &state.session_id[..8]));

    if let Ok(new_rt) = build_tui_runtime(
        parse_backend(Some(&state.settings.backend)),
        Some(&state.settings.provider),
        Some(&state.settings.model),
        if state.settings.base_url.trim().is_empty() {
            None
        } else {
            Some(state.settings.base_url.as_str())
        },
        parse_allowed_modules(&state.settings.allowed_modules),
        &state.session_id,
        event_tx.clone(),
    )
    .await
    {
        *tui_rt = new_rt;
    } else {
        push_obs(state, "⚠ new session runtime rebind failed".to_string());
    }
}

pub(crate) async fn handle_model_command(
    args: Vec<&str>,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    if args.is_empty() {
        push_obs(
            state,
            format!("model {}:{}", state.settings.provider, state.settings.model),
        );
        return EventOutcome::Continue;
    }

    let mut draft = state.settings_draft.clone();
    if args.len() == 1 {
        if let Some((provider, model)) = args[0].split_once(':') {
            draft.provider = provider.trim().to_string();
            draft.model = model.trim().to_string();
        } else {
            draft.model = args[0].trim().to_string();
        }
    } else {
        draft.provider = args[0].trim().to_string();
        draft.model = args[1].trim().to_string();
    }

    state.settings_draft = draft;
    apply_settings(state, tui_rt, event_tx).await;
    EventOutcome::Continue
}

pub(crate) fn handle_depth_command(mode: Option<&str>, state: &mut TuiState) -> EventOutcome {
    if mode.is_none() {
        let hint = depth_mode_hint(&state.response_depth_mode);
        push_obs(
            state,
            format!(
                "◈ response depth mode={} ({hint}) options: concise | standard | deep",
                state.response_depth_mode,
            ),
        );
        return EventOutcome::Continue;
    }

    let normalized = super::normalize_response_depth_mode(mode.unwrap_or("standard"));
    state.response_depth_mode = normalized.clone();
    persist_response_depth_defaults(state);
    let hint = depth_mode_hint(&normalized);
    push_obs(
        state,
        format!("✓ response depth mode set to {} ({hint})", normalized),
    );
    EventOutcome::Continue
}

pub(crate) fn handle_perf_command(
    sub: &str,
    trailing_args: &[&str],
    state: &mut TuiState,
) -> EventOutcome {
    match sub {
        "baseline" => {
            let label = trailing_args.join(" ");
            let label = if label.trim().is_empty() {
                "baseline".to_string()
            } else {
                label.trim().to_string()
            };
            let snapshot = capture_perf_snapshot(state, label.clone());
            state.perf_baseline = Some(snapshot.clone());
            push_obs(
                state,
                format!("✓ perf baseline set: {}", format_perf_snapshot(&snapshot)),
            );
        }
        "reset" => {
            state.perf = UiPerfStats::default();
            state.perf_baseline = None;
            push_obs(state, "✓ perf counters and baseline reset".to_string());
        }
        _ => {
            let label = if sub == "report" {
                "report".to_string()
            } else {
                sub.to_string()
            };
            let current = capture_perf_snapshot(state, label);
            let mut line = format!("perf {}", format_perf_snapshot(&current));
            if let Some(baseline) = &state.perf_baseline {
                line.push_str(" | ");
                line.push_str(&format_perf_delta(&current, baseline));
            }
            push_obs(state, line);
        }
    }

    EventOutcome::Continue
}

fn persist_response_depth_defaults(state: &TuiState) {
    let mut defaults = load_tui_defaults();
    defaults.response_depth_mode = Some(state.response_depth_mode.clone());
    save_tui_defaults(&defaults);
}

fn depth_mode_hint(mode: &str) -> &'static str {
    match mode {
        "concise" => "short direct answers",
        "deep" => "detailed evidence-forward answers",
        _ => "balanced answer depth",
    }
}
