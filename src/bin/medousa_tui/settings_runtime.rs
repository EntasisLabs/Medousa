use std::time::Duration;

use tokio::sync::mpsc;

use medousa::{TuiRuntime, build_tui_runtime, events::TuiEvent, parse_backend};

use super::{EventOutcome, PendingSettingsApply, SettingsApplySnapshot, TuiState};

pub(crate) async fn handle_settings_key_event(
    code: crossterm::event::KeyCode,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    super::settings_ui::handle_settings_key_event(code, state, tui_rt, event_tx).await
}

pub(crate) fn emit_settings_validation_summary(state: &mut TuiState) -> bool {
    super::settings_ui::emit_settings_validation_summary(state)
}

pub(crate) fn handle_runtime_env_key_event(
    code: crossterm::event::KeyCode,
    state: &mut TuiState,
) -> EventOutcome {
    super::settings_ui::handle_runtime_env_key_event(code, state)
}

pub(crate) async fn apply_settings(
    state: &mut TuiState,
    _tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) {
    if !emit_settings_validation_summary(state) {
        return;
    }

    let allowed_modules = super::parse_allowed_modules(&state.settings_draft.allowed_modules);
    let invalid_modules = super::invalid_module_ids(&allowed_modules);
    if !invalid_modules.is_empty() {
        let invalid_list = invalid_modules.join(", ");
        super::push_obs(
            state,
            format!(
                "⚠ settings rejected: invalid allowed module ids ({invalid_list}). use dotted ids like websearch.search"
            ),
        );
        return;
    }

    let backend = super::resolve_backend_name(Some(state.settings_draft.backend.trim()));
    let theme_id = super::resolve_theme_id_name(Some(state.settings_draft.theme_id.trim()));
    let tool_call_mode =
        super::resolve_tool_call_mode_name(Some(state.settings_draft.tool_call_mode.trim()));
    let max_tool_rounds =
        super::parse_usize_with_bounds(&state.settings_draft.max_tool_rounds, 10, 1, 50);
    let thinking_capture =
        super::parse_bool_with_default(&state.settings_draft.thinking_capture, true);
    let thinking_max_lines =
        super::parse_usize_with_bounds(&state.settings_draft.thinking_max_lines, 300, 50, 5000);
    let activation_direct_answer_max_prompt_chars = super::parse_usize_with_bounds(
        &state
            .settings_draft
            .activation_direct_answer_max_prompt_chars,
        320,
        64,
        4000,
    );
    let activation_long_session_turn_threshold = super::parse_usize_with_bounds(
        &state.settings_draft.activation_long_session_turn_threshold,
        28,
        8,
        500,
    );
    let activation_long_session_max_prompt_chars = super::parse_usize_with_bounds(
        &state
            .settings_draft
            .activation_long_session_max_prompt_chars,
        420,
        64,
        4000,
    );
    let slice_hot_window_turns =
        super::parse_usize_with_bounds(&state.settings_draft.slice_hot_window_turns, 8, 2, 32);
    let slice_cold_window_turns =
        super::parse_usize_with_bounds(&state.settings_draft.slice_cold_window_turns, 24, 4, 128)
            .max(slice_hot_window_turns);
    let retry_runtime_max_retries =
        super::parse_usize_with_bounds(&state.settings_draft.retry_runtime_max_retries, 1, 0, 5);
    let retry_runtime_max_rounds =
        super::parse_usize_with_bounds(&state.settings_draft.retry_runtime_max_rounds, 3, 1, 10);
    let verifier_min_citation_coverage = super::parse_f32_with_bounds(
        &state.settings_draft.verifier_min_citation_coverage,
        0.60,
        0.0,
        1.0,
    );
    let verifier_min_avg_support_strength = super::parse_f32_with_bounds(
        &state.settings_draft.verifier_min_avg_support_strength,
        0.70,
        0.0,
        1.0,
    );
    let verifier_min_supported_claim_ratio = super::parse_f32_with_bounds(
        &state.settings_draft.verifier_min_supported_claim_ratio,
        0.60,
        0.0,
        1.0,
    );
    let verifier_min_claim_support_strength = super::parse_f32_with_bounds(
        &state.settings_draft.verifier_min_claim_support_strength,
        0.65,
        0.0,
        1.0,
    );
    let provider = if state.settings_draft.provider.trim().is_empty() {
        super::resolve_llm_provider(None)
    } else {
        super::resolve_llm_provider(Some(state.settings_draft.provider.trim()))
    };
    let model = if state.settings_draft.model.trim().is_empty() {
        super::resolve_llm_model(None)
    } else {
        super::resolve_llm_model(Some(state.settings_draft.model.trim()))
    };
    let base_url = if state.settings_draft.base_url.trim().is_empty() {
        None
    } else {
        Some(state.settings_draft.base_url.trim().to_string())
    };
    let env_overrides_raw = state.settings_draft.env_overrides.clone();
    let changed = apply_env_overrides(&env_overrides_raw);

    let api_key = state.settings_draft.api_key.trim().to_string();
    let snapshot = SettingsApplySnapshot {
        backend: backend.clone(),
        theme_id,
        provider: provider.clone(),
        model: model.clone(),
        base_url: base_url.clone(),
        env_overrides_raw,
        allowed_modules: allowed_modules.clone(),
        tool_call_mode,
        max_tool_rounds,
        thinking_capture,
        thinking_max_lines,
        activation_direct_answer_max_prompt_chars,
        activation_long_session_turn_threshold,
        activation_long_session_max_prompt_chars,
        slice_hot_window_turns,
        slice_cold_window_turns,
        retry_runtime_max_retries,
        retry_runtime_max_rounds,
        verifier_min_citation_coverage,
        verifier_min_avg_support_strength,
        verifier_min_supported_claim_ratio,
        verifier_min_claim_support_strength,
        stage_routing: state.stage_routing_draft.clone(),
        api_key,
    };

    let request_id = state.next_settings_apply_request_id.saturating_add(1);
    state.next_settings_apply_request_id = request_id;
    state.active_settings_apply_request_id = Some(request_id);

    if let Some(previous) = state.pending_settings_apply.take() {
        previous.handle.abort();
        super::push_obs(
            state,
            format!(
                "↻ settings apply request #{request_id} superseded request #{}",
                previous.request_id
            ),
        );
    }

    let session_id = state.session_id.clone();
    let event_tx = event_tx.clone();
    let backend_for_build = snapshot.backend.clone();
    let provider_for_build = snapshot.provider.clone();
    let model_for_build = snapshot.model.clone();
    let base_url_for_build = snapshot.base_url.clone();
    let allowed_modules_for_build = snapshot.allowed_modules.clone();
    let handle = tokio::spawn(async move {
        build_tui_runtime(
            parse_backend(Some(&backend_for_build)),
            Some(&provider_for_build),
            Some(&model_for_build),
            base_url_for_build.as_deref(),
            allowed_modules_for_build,
            &session_id,
            event_tx,
        )
        .await
        .map_err(|err| err.to_string())
    });

    state.pending_settings_apply = Some(PendingSettingsApply {
        request_id,
        changed_env_count: changed,
        snapshot,
        handle,
    });
    super::push_obs(
        state,
        format!("↻ settings apply queued (request #{request_id})"),
    );
}

pub(crate) fn next_ui_wake_delay(state: &TuiState) -> Duration {
    if state.pending_settings_apply.is_some() {
        Duration::from_millis(50)
    } else if state.is_processing || state.active_agent_stream_turn.is_some() {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(1000)
    }
}

pub(crate) async fn finalize_settings_apply_if_ready(
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
) -> bool {
    let is_ready = state
        .pending_settings_apply
        .as_ref()
        .map(|pending| pending.handle.is_finished())
        .unwrap_or(false);
    if !is_ready {
        return false;
    }

    let Some(pending) = state.pending_settings_apply.take() else {
        return false;
    };

    if state.active_settings_apply_request_id != Some(pending.request_id) {
        return false;
    }

    let request_id = pending.request_id;
    match pending.handle.await {
        Ok(Ok(new_rt)) => {
            *tui_rt = new_rt;
            let snapshot = pending.snapshot;
            state.settings.backend = snapshot.backend.clone();
            state.settings.theme_id = snapshot.theme_id.clone();
            state.settings.provider = snapshot.provider.clone();
            state.settings.model = snapshot.model.clone();
            state.settings.base_url = snapshot.base_url.clone().unwrap_or_default();
            state.settings.env_overrides = snapshot.env_overrides_raw.clone();
            state.settings.allowed_modules = snapshot.allowed_modules.join(",");
            state.settings.tool_call_mode = snapshot.tool_call_mode.clone();
            state.settings.max_tool_rounds = snapshot.max_tool_rounds.to_string();
            state.settings.thinking_capture = snapshot.thinking_capture.to_string();
            state.settings.thinking_max_lines = snapshot.thinking_max_lines.to_string();
            state.settings.activation_direct_answer_max_prompt_chars = snapshot
                .activation_direct_answer_max_prompt_chars
                .to_string();
            state.settings.activation_long_session_turn_threshold =
                snapshot.activation_long_session_turn_threshold.to_string();
            state.settings.activation_long_session_max_prompt_chars = snapshot
                .activation_long_session_max_prompt_chars
                .to_string();
            state.settings.slice_hot_window_turns = snapshot.slice_hot_window_turns.to_string();
            state.settings.slice_cold_window_turns = snapshot.slice_cold_window_turns.to_string();
            state.settings.retry_runtime_max_retries =
                snapshot.retry_runtime_max_retries.to_string();
            state.settings.retry_runtime_max_rounds = snapshot.retry_runtime_max_rounds.to_string();
            state.settings.verifier_min_citation_coverage =
                format!("{:.2}", snapshot.verifier_min_citation_coverage);
            state.settings.verifier_min_avg_support_strength =
                format!("{:.2}", snapshot.verifier_min_avg_support_strength);
            state.settings.verifier_min_supported_claim_ratio =
                format!("{:.2}", snapshot.verifier_min_supported_claim_ratio);
            state.settings.verifier_min_claim_support_strength =
                format!("{:.2}", snapshot.verifier_min_claim_support_strength);
            state.stage_routing = snapshot.stage_routing.clone();
            state.settings.api_key = snapshot.api_key.clone();
            state.provider_model = format!("{}:{}", snapshot.provider, snapshot.model);

            if snapshot.api_key.is_empty() {
                super::save_tui_api_key(None);
            } else {
                super::save_tui_api_key(Some(&snapshot.api_key));
            }

            state.settings_draft = state.settings.clone();
            state.stage_routing_draft = state.stage_routing.clone();

            super::save_tui_defaults(&super::TuiDefaults {
                backend: Some(snapshot.backend),
                theme_id: Some(snapshot.theme_id),
                provider: Some(snapshot.provider),
                model: Some(snapshot.model),
                base_url: snapshot.base_url,
                env_overrides: if snapshot.env_overrides_raw.trim().is_empty() {
                    None
                } else {
                    Some(snapshot.env_overrides_raw)
                },
                allowed_modules: if snapshot.allowed_modules.is_empty() {
                    None
                } else {
                    Some(snapshot.allowed_modules)
                },
                tool_call_mode: Some(snapshot.tool_call_mode),
                max_tool_rounds: Some(snapshot.max_tool_rounds),
                thinking_capture: Some(snapshot.thinking_capture),
                thinking_max_lines: Some(snapshot.thinking_max_lines),
                activation_direct_answer_max_prompt_chars: Some(
                    snapshot.activation_direct_answer_max_prompt_chars,
                ),
                activation_long_session_turn_threshold: Some(
                    snapshot.activation_long_session_turn_threshold,
                ),
                activation_long_session_max_prompt_chars: Some(
                    snapshot.activation_long_session_max_prompt_chars,
                ),
                slice_hot_window_turns: Some(snapshot.slice_hot_window_turns),
                slice_cold_window_turns: Some(snapshot.slice_cold_window_turns),
                retry_runtime_max_retries: Some(snapshot.retry_runtime_max_retries),
                retry_runtime_max_rounds: Some(snapshot.retry_runtime_max_rounds),
                verifier_min_citation_coverage: Some(snapshot.verifier_min_citation_coverage),
                verifier_min_avg_support_strength: Some(snapshot.verifier_min_avg_support_strength),
                verifier_min_supported_claim_ratio: Some(
                    snapshot.verifier_min_supported_claim_ratio,
                ),
                verifier_min_claim_support_strength: Some(
                    snapshot.verifier_min_claim_support_strength,
                ),
                response_depth_mode: Some(state.response_depth_mode.clone()),
                stage_routing: Some(state.stage_routing.clone()),
                command_usage_counts: if state.command_usage_counts.is_empty() {
                    None
                } else {
                    Some(state.command_usage_counts.clone())
                },
            });

            super::push_obs(
                state,
                format!(
                    "✓ settings applied (request #{request_id}, sensitive values redacted, {} env override(s) active)",
                    pending.changed_env_count
                ),
            );
        }
        Ok(Err(err)) => {
            super::push_obs(
                state,
                format!("⚠ settings apply failed (request #{request_id}): {err}"),
            );
        }
        Err(err) => {
            super::push_obs(
                state,
                format!("⚠ settings apply task failed (request #{request_id}): {err}"),
            );
        }
    }

    if state.active_settings_apply_request_id == Some(request_id) {
        state.active_settings_apply_request_id = None;
    }

    true
}

pub(crate) fn apply_env_overrides(raw: &str) -> usize {
    let mut changed = 0usize;
    for (key, value) in super::parse_env_overrides(raw) {
        if value.is_empty() {
            // Runtime env mutation is process-global; keep it explicit.
            unsafe {
                std::env::remove_var(&key);
            }
        } else {
            // Runtime env mutation is process-global; keep it explicit.
            unsafe {
                std::env::set_var(&key, &value);
            }
        }
        changed = changed.saturating_add(1);
    }
    changed
}
