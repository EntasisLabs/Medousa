use medousa::{
    RuntimeConfigCommandRequest, RuntimeConfigCommandResponse, RuntimeConfigCommandSpec,
    build_tui_platform,
};

use super::daemon_commands::daemon_runtime_config_command;
use super::*;

pub(crate) async fn handle_new_session_command(
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) {
    stop_active_generation(state);
    state.session_id = Uuid::new_v4().simple().to_string();
    state.session_display_name = None;
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

    if let Ok(new_rt) = build_tui_platform(build_tui_platform_config(state), event_tx.clone()).await {
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
    let request = build_runtime_config_request(
        state,
        RuntimeConfigCommandSpec::Model {
            args: args.into_iter().map(ToString::to_string).collect::<Vec<_>>(),
        },
    );

    match execute_runtime_config_command_via_daemon(&state.daemon_url, request).await {
        Ok(response) => {
            apply_runtime_config_response(response, state, tui_rt, event_tx).await;
        }
        Err(err) => {
            push_obs(state, format!("⚠ model command failed: {err}"));
        }
    }

    EventOutcome::Continue
}

pub(crate) async fn handle_depth_command(
    mode: Option<&str>,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    let request = build_runtime_config_request(
        state,
        RuntimeConfigCommandSpec::Depth {
            mode: mode.map(ToString::to_string),
        },
    );

    match execute_runtime_config_command_via_daemon(&state.daemon_url, request).await {
        Ok(response) => {
            apply_runtime_config_response(response, state, tui_rt, event_tx).await;
        }
        Err(err) => {
            push_obs(state, format!("⚠ depth command failed: {err}"));
        }
    }

    EventOutcome::Continue
}

pub(crate) fn build_runtime_config_request(
    state: &TuiState,
    command: RuntimeConfigCommandSpec,
) -> RuntimeConfigCommandRequest {
    RuntimeConfigCommandRequest {
        current_provider: state.settings.provider.clone(),
        current_model: state.settings.model.clone(),
        draft_provider: state.settings_draft.provider.clone(),
        draft_model: state.settings_draft.model.clone(),
        current_response_depth_mode: state.response_depth_mode.clone(),
        command,
    }
}

pub(crate) async fn apply_runtime_config_response(
    response: RuntimeConfigCommandResponse,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) {
    state.settings_draft.provider = response.next_draft_provider;
    state.settings_draft.model = response.next_draft_model;
    state.response_depth_mode = response.next_response_depth_mode;

    if let Some(policy) = response.next_verify_policy_draft {
        state.settings_draft.verifier_min_citation_coverage = policy.min_citation_coverage;
        state.settings_draft.verifier_min_avg_support_strength = policy.min_avg_support_strength;
        state.settings_draft.verifier_min_supported_claim_ratio = policy.min_supported_claim_ratio;
        state.settings_draft.verifier_min_claim_support_strength = policy.min_claim_support_strength;
    }

    if response.should_persist_depth_defaults {
        persist_response_depth_defaults(state);
    }

    if let Some(rendered) = response.rendered_output {
        push_obs(state, rendered);
    }

    if response.should_apply_settings {
        apply_settings(state, tui_rt, event_tx).await;
    }
}

pub(crate) async fn execute_runtime_config_command_via_daemon(
    daemon_url: &str,
    request: RuntimeConfigCommandRequest,
) -> Result<RuntimeConfigCommandResponse, String> {
    daemon_runtime_config_command(daemon_url, &request)
        .await
        .map_err(|err| {
            format!("daemon error: {}", truncate_error(&err.to_string(), 200))
        })
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

fn truncate_error(value: &str, max_chars: usize) -> String {
    let out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}
