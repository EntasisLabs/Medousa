use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use tokio::sync::mpsc;

use medousa::{TuiRuntime, events::TuiEvent};

use super::{EventOutcome, TuiState, UiMode};

const STARTUP_PROVIDER_OPTIONS: [&str; 5] = ["openai", "anthropic", "google", "xai", "ollama"];

pub(crate) async fn handle_key_event(
    event: Event,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    let key = match event {
        Event::Key(key) => key,
        Event::Mouse(mouse) => {
            if state.mode == UiMode::Settings {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        state.settings_scroll = state.settings_scroll.saturating_sub(3);
                        return EventOutcome::Continue;
                    }
                    MouseEventKind::ScrollDown => {
                        state.settings_scroll = state
                            .settings_scroll
                            .saturating_add(3)
                            .min(state.settings_max_scroll);
                        return EventOutcome::Continue;
                    }
                    _ => {}
                }
            }

            if state.mode == UiMode::CommandPalette {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        state.command_scroll = state.command_scroll.saturating_sub(2);
                        return EventOutcome::Continue;
                    }
                    MouseEventKind::ScrollDown => {
                        state.command_scroll = state
                            .command_scroll
                            .saturating_add(2)
                            .min(state.command_max_scroll);
                        return EventOutcome::Continue;
                    }
                    _ => {}
                }
            }

            if state.mode == UiMode::History {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        state.history_scroll = state.history_scroll.saturating_sub(2);
                        state.history_selected = state.history_selected.saturating_sub(1);
                        return EventOutcome::Continue;
                    }
                    MouseEventKind::ScrollDown => {
                        state.history_scroll = state
                            .history_scroll
                            .saturating_add(2)
                            .min(state.history_max_scroll);
                        if !state.history_items.is_empty() {
                            state.history_selected = (state.history_selected + 1)
                                .min(state.history_items.len().saturating_sub(1));
                        }
                        return EventOutcome::Continue;
                    }
                    _ => {}
                }
            }

            return EventOutcome::Continue;
        }
        _ => return EventOutcome::Continue,
    };

    if state.mode == UiMode::Startup {
        return handle_startup_key_event(key, state, tui_rt, event_tx).await;
    }

    if key.code == KeyCode::Esc {
        if state.mode == UiMode::RuntimeEnv {
            state.mode = UiMode::Settings;
            state.runtime_env_editing = false;
            return EventOutcome::Continue;
        }
        if state.mode == UiMode::ThemeMenu {
            return super::handle_theme_menu_key_event(key.code, state);
        }
        if state.mode == UiMode::GraphemeConsole {
            state.mode = UiMode::Chat;
            return EventOutcome::Continue;
        }
        if state.mode == UiMode::Settings {
            state.settings_draft = state.settings.clone();
            state.stage_routing_draft = state.stage_routing.clone();
            state.routing_editor_role_idx = 0;
        }
        state.settings_editing = false;
        state.mode = UiMode::Chat;
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::Char('t') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if state.mode == UiMode::ThinkingPanel {
            state.mode = UiMode::Chat;
        } else if !state.thinking_trace.is_empty() || state.is_processing {
            state.mode = UiMode::ThinkingPanel;
        }
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::F(2) {
        if state.mode == UiMode::ThinkingPeek {
            state.mode = UiMode::Chat;
        } else if !state.thinking_trace.is_empty() || state.is_processing {
            state.mode = UiMode::ThinkingPeek;
        }
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::F(3) {
        if state.mode == UiMode::GraphemeConsole {
            state.mode = UiMode::Chat;
        } else {
            state.mode = UiMode::GraphemeConsole;
        }
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::Char('k') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if state.mode == UiMode::CommandPalette {
            state.mode = UiMode::Chat;
            state.command_query.clear();
            state.command_tab = 0;
            state.command_selected = 0;
            state.command_scroll = 0;
            state.command_max_scroll = 0;
        } else {
            state.mode = UiMode::CommandPalette;
            state.command_query.clear();
            state.command_tab = 0;
            state.command_selected = 0;
            state.command_scroll = 0;
            state.command_max_scroll = 0;
        }
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::Char('h') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if state.mode == UiMode::History {
            state.mode = UiMode::Chat;
        } else {
            state.history_items = super::list_history_sessions(200);
            state.history_selected = 0;
            state.history_scroll = 0;
            state.history_max_scroll = 0;
            state.history_show_verification_detail = false;
            state.mode = UiMode::History;
        }
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::Char(',') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if state.mode == UiMode::Settings || state.mode == UiMode::RuntimeEnv {
            state.mode = UiMode::Chat;
            state.settings_editing = false;
            state.runtime_env_editing = false;
            state.settings_draft = state.settings.clone();
            state.stage_routing_draft = state.stage_routing.clone();
            state.routing_editor_role_idx = 0;
        } else {
            state.mode = UiMode::Settings;
            state.settings_tab = 0;
            state.settings_selected = 0;
            state.settings_editing = false;
            state.settings_scroll = 0;
            state.settings_max_scroll = 0;
            state.routing_editor_role_idx = 0;
            state.runtime_env_editing = false;
            state.settings_draft = state.settings.clone();
            state.stage_routing_draft = state.stage_routing.clone();
        }
        return EventOutcome::Continue;
    }

    if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if state.mode == UiMode::ObservabilityPanel {
            state.mode = UiMode::Chat;
        } else {
            state.mode = UiMode::ObservabilityPanel;
        }
        return EventOutcome::Continue;
    }

    if state.mode == UiMode::History {
        return super::handle_history_key_event(key.code, state);
    }

    if state.mode == UiMode::CommandPalette {
        return super::handle_command_palette_key_event(key.code, state, tui_rt, event_tx).await;
    }

    if state.mode == UiMode::Settings {
        return super::handle_settings_key_event(key.code, state, tui_rt, event_tx).await;
    }

    if state.mode == UiMode::RuntimeEnv {
        return super::handle_runtime_env_key_event(key.code, state);
    }

    if state.mode == UiMode::ThemeMenu {
        return super::handle_theme_menu_key_event(key.code, state);
    }

    if state.mode == UiMode::AllowlistPreview {
        return handle_allowlist_preview_key_event(key.code, state);
    }

    if state.mode == UiMode::Editor {
        return handle_editor_key_event(key, state);
    }

    if state.mode == UiMode::ThinkingPeek || state.mode == UiMode::ThinkingPanel {
        return handle_thinking_key_event(key.code, state);
    }

    if state.mode == UiMode::GraphemeConsole {
        return handle_grapheme_console_key_event(key.code, state);
    }

    if state.mode == UiMode::ObservabilityPanel {
        return handle_observability_key_event(key.code, state);
    }

    let side_scroll_mod =
        key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::ALT);
    if state.mode == UiMode::Chat && side_scroll_mod {
        match key.code {
            KeyCode::Up => {
                state.obs_scroll = state.obs_scroll.saturating_sub(1);
                state.job_scroll = state.job_scroll.saturating_sub(1);
                return EventOutcome::Continue;
            }
            KeyCode::Down => {
                state.obs_scroll = state.obs_scroll.saturating_add(1).min(state.obs_max_scroll);
                state.job_scroll = state.job_scroll.saturating_add(1).min(state.job_max_scroll);
                return EventOutcome::Continue;
            }
            KeyCode::PageUp => {
                state.obs_scroll = state.obs_scroll.saturating_sub(10);
                state.job_scroll = state.job_scroll.saturating_sub(10);
                return EventOutcome::Continue;
            }
            KeyCode::PageDown => {
                state.obs_scroll = state
                    .obs_scroll
                    .saturating_add(10)
                    .min(state.obs_max_scroll);
                state.job_scroll = state
                    .job_scroll
                    .saturating_add(10)
                    .min(state.job_max_scroll);
                return EventOutcome::Continue;
            }
            KeyCode::Home => {
                state.obs_scroll = 0;
                state.job_scroll = 0;
                return EventOutcome::Continue;
            }
            KeyCode::End => {
                state.obs_scroll = state.obs_max_scroll;
                state.job_scroll = state.job_max_scroll;
                return EventOutcome::Continue;
            }
            _ => {}
        }
    }

    if state.mode == UiMode::Chat && key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Char('o') => {
                state.obs_scroll = state.obs_scroll.saturating_sub(1);
                return EventOutcome::Continue;
            }
            KeyCode::Char('p') => {
                state.obs_scroll = state.obs_scroll.saturating_add(1).min(state.obs_max_scroll);
                return EventOutcome::Continue;
            }
            KeyCode::Char('j') => {
                state.job_scroll = state.job_scroll.saturating_sub(1);
                return EventOutcome::Continue;
            }
            KeyCode::Char('k') => {
                state.job_scroll = state.job_scroll.saturating_add(1).min(state.job_max_scroll);
                return EventOutcome::Continue;
            }
            _ => {}
        }
    }

    if state.mode == UiMode::Chat && key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('u') => {
                state.obs_scroll = state.obs_scroll.saturating_sub(10);
                state.job_scroll = state.job_scroll.saturating_sub(10);
                return EventOutcome::Continue;
            }
            KeyCode::Char('d') => {
                state.obs_scroll = state
                    .obs_scroll
                    .saturating_add(10)
                    .min(state.obs_max_scroll);
                state.job_scroll = state
                    .job_scroll
                    .saturating_add(10)
                    .min(state.job_max_scroll);
                return EventOutcome::Continue;
            }
            _ => {}
        }
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
            return EventOutcome::Break;
        }
        (KeyCode::Char('q'), m) if m.contains(KeyModifiers::CONTROL) => {
            return EventOutcome::Break;
        }
        (KeyCode::Char('g'), m) if m.contains(KeyModifiers::CONTROL) => {
            super::stop_active_generation(state);
            return EventOutcome::Continue;
        }

        (KeyCode::Up, _) => {
            let base = if state.auto_scroll {
                state.conv_max_scroll
            } else {
                state.conv_scroll
            };
            state.conv_scroll = base.saturating_sub(1);
            state.auto_scroll = false;
        }
        (KeyCode::Down, _) => {
            let base = if state.auto_scroll {
                state.conv_max_scroll
            } else {
                state.conv_scroll
            };
            let next = base.saturating_add(1).min(state.conv_max_scroll);
            state.conv_scroll = next;
            state.auto_scroll = next >= state.conv_max_scroll;
        }
        (KeyCode::PageUp, _) => {
            let base = if state.auto_scroll {
                state.conv_max_scroll
            } else {
                state.conv_scroll
            };
            state.conv_scroll = base.saturating_sub(10);
            state.auto_scroll = false;
        }
        (KeyCode::PageDown, _) => {
            let base = if state.auto_scroll {
                state.conv_max_scroll
            } else {
                state.conv_scroll
            };
            let next = base.saturating_add(10).min(state.conv_max_scroll);
            state.conv_scroll = next;
            state.auto_scroll = next >= state.conv_max_scroll;
        }
        (KeyCode::Home, _) => {
            state.conv_scroll = 0;
            state.auto_scroll = false;
        }
        (KeyCode::End, _) => {
            state.conv_scroll = state.conv_max_scroll;
            state.auto_scroll = true;
        }

        (KeyCode::Char(c), _) if !state.is_processing => {
            state.input_buffer.push(c);
        }
        (KeyCode::Backspace, _) if !state.is_processing => {
            state.input_buffer.pop();
        }

        (KeyCode::Enter, _) if !state.is_processing && !state.input_buffer.trim().is_empty() => {
            let prompt = state.input_buffer.trim().to_string();

            if prompt.starts_with('/') {
                state.input_buffer.clear();
                return super::handle_slash_command(&prompt, state, tui_rt, event_tx).await;
            }

            state.input_buffer.clear();
            super::start_prompt_run(state, tui_rt, event_tx, prompt, true);
        }

        _ => {}
    }

    EventOutcome::Continue
}

async fn handle_startup_key_event(
    key: KeyEvent,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
            return EventOutcome::Break;
        }
        (KeyCode::Char('q'), m) if m.contains(KeyModifiers::CONTROL) => {
            return EventOutcome::Break;
        }
        _ => {}
    }

    match key.code {
        KeyCode::Up => {
            state.startup_selected = state.startup_selected.saturating_sub(1);
        }
        KeyCode::Down => {
            state.startup_selected = (state.startup_selected + 1).min(2);
        }
        KeyCode::Left => {
            if state.startup_selected == 0 {
                cycle_startup_provider(state, false);
            }
        }
        KeyCode::Right | KeyCode::Tab => {
            if state.startup_selected == 0 {
                cycle_startup_provider(state, true);
            }
        }
        KeyCode::Backspace => {
            if state.startup_selected == 1 {
                state.settings_draft.model.pop();
            }
        }
        KeyCode::Char(c) => {
            if state.startup_selected == 1 {
                state.settings_draft.model.push(c);
            }
        }
        KeyCode::Enter => {
            if state.startup_selected == 2 {
                let provider = state.settings_draft.provider.trim().to_string();
                let model = state.settings_draft.model.trim().to_string();
                if provider.is_empty() || model.is_empty() {
                    super::push_obs(
                        state,
                        "⚠ startup requires a non-empty provider and model".to_string(),
                    );
                    return EventOutcome::Continue;
                }

                state.settings_draft.provider = provider;
                state.settings_draft.model = model;
                state.provider_model = format!(
                    "{}:{}",
                    state.settings_draft.provider, state.settings_draft.model
                );
                super::apply_settings(state, tui_rt, event_tx).await;
                state.mode = UiMode::Chat;
                return EventOutcome::Continue;
            }
        }
        _ => {}
    }

    EventOutcome::Continue
}

fn cycle_startup_provider(state: &mut TuiState, forward: bool) {
    let current = state.settings_draft.provider.trim().to_ascii_lowercase();
    let idx = STARTUP_PROVIDER_OPTIONS
        .iter()
        .position(|p| *p == current)
        .unwrap_or(0);

    let next = if forward {
        (idx + 1) % STARTUP_PROVIDER_OPTIONS.len()
    } else if idx == 0 {
        STARTUP_PROVIDER_OPTIONS.len() - 1
    } else {
        idx - 1
    };

    let provider = STARTUP_PROVIDER_OPTIONS[next];
    state.settings_draft.provider = provider.to_string();
    state.settings_draft.model = default_model_for_provider(provider).to_string();
}

fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-3-7-sonnet-latest",
        "google" => "gemini-2.5-pro",
        "xai" => "grok-3-mini",
        "ollama" => "llama3.2",
        _ => "gpt-4o-mini",
    }
}

fn handle_thinking_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    if state.mode == UiMode::ThinkingPeek {
        if matches!(code, KeyCode::Down | KeyCode::PageDown | KeyCode::Enter) {
            state.mode = UiMode::ThinkingPanel;
        }
        return EventOutcome::Continue;
    }

    match code {
        KeyCode::Up => {
            state.thinking_scroll = state.thinking_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.thinking_scroll = state
                .thinking_scroll
                .saturating_add(1)
                .min(state.thinking_max_scroll);
        }
        KeyCode::PageUp => {
            state.thinking_scroll = state.thinking_scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            state.thinking_scroll = state
                .thinking_scroll
                .saturating_add(10)
                .min(state.thinking_max_scroll);
        }
        KeyCode::Home => {
            state.thinking_scroll = 0;
        }
        KeyCode::End => {
            state.thinking_scroll = state.thinking_max_scroll;
        }
        _ => {}
    }

    EventOutcome::Continue
}

fn handle_observability_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    match code {
        KeyCode::Char('r') | KeyCode::Char('R') => {
            state.observability_filter = match state.observability_filter {
                super::ObservabilityFilter::All => super::ObservabilityFilter::ReceiptsOnly,
                super::ObservabilityFilter::ReceiptsOnly => {
                    super::ObservabilityFilter::ArtifactsOnly
                }
                super::ObservabilityFilter::ArtifactsOnly => super::ObservabilityFilter::All,
            };
            state.obs_scroll = 0;
        }
        KeyCode::Up => {
            state.obs_scroll = state.obs_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.obs_scroll = state.obs_scroll.saturating_add(1).min(state.obs_max_scroll);
        }
        KeyCode::PageUp => {
            state.obs_scroll = state.obs_scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            state.obs_scroll = state
                .obs_scroll
                .saturating_add(10)
                .min(state.obs_max_scroll);
        }
        KeyCode::Home => {
            state.obs_scroll = 0;
        }
        KeyCode::End => {
            state.obs_scroll = state.obs_max_scroll;
        }
        _ => {}
    }

    EventOutcome::Continue
}

fn handle_grapheme_console_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    match code {
        KeyCode::Up => {
            state.grapheme_console_scroll = state.grapheme_console_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.grapheme_console_scroll = state
                .grapheme_console_scroll
                .saturating_add(1)
                .min(state.grapheme_console_max_scroll);
        }
        KeyCode::PageUp => {
            state.grapheme_console_scroll = state.grapheme_console_scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            state.grapheme_console_scroll = state
                .grapheme_console_scroll
                .saturating_add(10)
                .min(state.grapheme_console_max_scroll);
        }
        KeyCode::Home => {
            state.grapheme_console_scroll = 0;
        }
        KeyCode::End => {
            state.grapheme_console_scroll = state.grapheme_console_max_scroll;
        }
        _ => {}
    }

    EventOutcome::Continue
}

fn handle_allowlist_preview_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    super::command_preview_ui::handle_allowlist_preview_key_event(code, state)
}

fn handle_editor_key_event(key: KeyEvent, state: &mut TuiState) -> EventOutcome {
    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        super::save_editor_buffer(state, None);
        return EventOutcome::Continue;
    }

    let mut edited = false;
    let mut vertical_nav = false;

    match key.code {
        KeyCode::Left => {
            state.editor_buffer.move_left();
            state.editor_preferred_col = None;
        }
        KeyCode::Right => {
            state.editor_buffer.move_right();
            state.editor_preferred_col = None;
        }
        KeyCode::Up => {
            let (_, col) = state.editor_buffer.line_col();
            let preferred_col = state.editor_preferred_col.unwrap_or(col);
            state.editor_buffer.move_up(preferred_col);
            state.editor_preferred_col = Some(preferred_col);
            vertical_nav = true;
        }
        KeyCode::Down => {
            let (_, col) = state.editor_buffer.line_col();
            let preferred_col = state.editor_preferred_col.unwrap_or(col);
            state.editor_buffer.move_down(preferred_col);
            state.editor_preferred_col = Some(preferred_col);
            vertical_nav = true;
        }
        KeyCode::Home => {
            state.editor_buffer.move_line_start();
            state.editor_preferred_col = Some(1);
        }
        KeyCode::End => {
            state.editor_buffer.move_line_end();
            let (_, col) = state.editor_buffer.line_col();
            state.editor_preferred_col = Some(col);
        }
        KeyCode::Enter => {
            state.editor_buffer.insert_newline();
            state.editor_preferred_col = None;
            edited = true;
        }
        KeyCode::Backspace => {
            state.editor_buffer.backspace();
            state.editor_preferred_col = None;
            edited = true;
        }
        KeyCode::Tab => {
            state.editor_buffer.insert_str("    ");
            state.editor_preferred_col = None;
            edited = true;
        }
        KeyCode::Char(c) => {
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                state.editor_buffer.insert_char(c);
                state.editor_preferred_col = None;
                edited = true;
            }
        }
        _ => {}
    }

    if edited {
        state.editor_dirty = true;
        state.editor_status = "Unsaved changes".to_string();
    }

    if edited || vertical_nav {
        keep_editor_cursor_visible(state, 12);
    }

    EventOutcome::Continue
}

pub(crate) fn keep_editor_cursor_visible(state: &mut TuiState, viewport_lines: usize) {
    let viewport = viewport_lines.max(3) as u16;
    let cursor_line = state.editor_buffer.line_col().0.saturating_sub(1) as u16;

    if cursor_line < state.editor_scroll {
        state.editor_scroll = cursor_line;
        return;
    }

    let max_visible = state
        .editor_scroll
        .saturating_add(viewport.saturating_sub(1));
    if cursor_line > max_visible {
        state.editor_scroll = cursor_line.saturating_sub(viewport.saturating_sub(1));
    }
}
