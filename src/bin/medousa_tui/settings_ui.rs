use super::*;

use settings_rows::{
    is_float_row, is_numeric_row, is_routing_edit_row, is_toggle_row,
    selected_settings_field_mut as draft_field_for_row, settings_row_id,
    quick_adjust_setting as quick_adjust_row, SETTINGS_TABS,
};

pub(crate) async fn handle_settings_key_event(
    code: KeyCode,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    clamp_selected_to_active_tab(state);

    if state.settings_editing {
        match code {
            KeyCode::Enter => {
                state.settings_editing = false;
            }
            KeyCode::Backspace => {
                if let Some(target) = selected_route_field_mut(state) {
                    target.pop();
                } else if let Some(target) = draft_field_for_row(
                    state,
                    settings_row_id(state.settings_selected),
                ) {
                    target.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(target) = selected_route_field_mut(state) {
                    target.push(c);
                } else if let Some(target) = draft_field_for_row(
                    state,
                    settings_row_id(state.settings_selected),
                ) {
                    target.push(c);
                }
            }
            _ => {}
        }
        return EventOutcome::Continue;
    }

    match code {
        KeyCode::Char('t') | KeyCode::Char('T') => {
            open_theme_menu(state, UiMode::Settings);
        }
        KeyCode::Tab => {
            switch_settings_tab(state, true);
        }
        KeyCode::BackTab => {
            switch_settings_tab(state, false);
        }
        KeyCode::PageUp => {
            state.settings_scroll = state.settings_scroll.saturating_sub(6);
        }
        KeyCode::PageDown => {
            state.settings_scroll = state
                .settings_scroll
                .saturating_add(6)
                .min(state.settings_max_scroll);
        }
        KeyCode::Home => {
            state.settings_scroll = 0;
        }
        KeyCode::End => {
            state.settings_scroll = state.settings_max_scroll;
        }
        KeyCode::Char(' ') | KeyCode::Right => {
            quick_adjust_setting(state, true);
        }
        KeyCode::Left => {
            quick_adjust_setting(state, false);
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let row = settings_row_id(state.settings_selected);
            if is_numeric_row(row) || is_float_row(row) {
                quick_adjust_setting(state, true);
            }
        }
        KeyCode::Char('-') => {
            let row = settings_row_id(state.settings_selected);
            if is_numeric_row(row) || is_float_row(row) {
                quick_adjust_setting(state, false);
            }
        }
        KeyCode::Up => {
            let (start, _) = active_tab_bounds(state);
            state.settings_selected = state.settings_selected.saturating_sub(1).max(start);
        }
        KeyCode::Down => {
            let (_, end) = active_tab_bounds(state);
            state.settings_selected = state.settings_selected.saturating_add(1).min(end);
        }
        KeyCode::Enter => {
            use settings_rows::SettingsRowId;
            match settings_row_id(state.settings_selected) {
                SettingsRowId::Provider
                | SettingsRowId::Model
                | SettingsRowId::BaseUrl
                | SettingsRowId::ApiKey
                | SettingsRowId::AllowedModules
                | SettingsRowId::WebSearchPreferredProvider
                | SettingsRowId::RouteProvider
                | SettingsRowId::RouteModel => {
                    state.settings_editing = true;
                }
                row if is_toggle_row(row)
                    || is_numeric_row(row)
                    || is_float_row(row)
                    || matches!(
                        row,
                        SettingsRowId::RouteRole
                            | SettingsRowId::RouteTargetPreset
                            | SettingsRowId::RoutePolicyProfile
                            | SettingsRowId::RouteFallbackChain
                            | SettingsRowId::ResetSelectedRoute
                    ) => {
                    quick_adjust_setting(state, true);
                }
                SettingsRowId::EnvOverrides => {
                    state.mode = UiMode::RuntimeEnv;
                    state.runtime_env_editing = true;
                }
                SettingsRowId::ReviewConfiguration => {
                    emit_settings_validation_summary(state);
                }
                SettingsRowId::ClearApiKey => {
                    state.settings_draft.api_key.clear();
                    push_obs(
                        state,
                        "✓ API key will be removed when you save".to_string(),
                    );
                }
                SettingsRowId::UpdateApiKey => {
                    let key = state.settings_draft.api_key.trim().to_string();
                    if key.is_empty() {
                        push_obs(state, "⚠ enter an API key before updating".to_string());
                    } else {
                        save_tui_api_key(Some(&key));
                        state.settings.api_key = key.clone();
                        state.settings_draft.api_key = key;
                        push_obs(state, "✓ API key updated".to_string());
                    }
                }
                SettingsRowId::SetAllRouteTargets => {
                    sync_all_route_targets_to_global(state);
                }
                SettingsRowId::RevertChanges => {
                    state.settings_draft = state.settings.clone();
                    state.stage_routing_draft = state.stage_routing.clone();
                    state.routing_editor_role_idx = 0;
                    state.settings_editing = false;
                    push_obs(state, "✓ changes discarded".to_string());
                }
                SettingsRowId::ApplyChanges => {
                    super::apply_settings(state, tui_rt, event_tx).await;
                    state.mode = UiMode::Chat;
                }
                SettingsRowId::Cancel => {
                    state.settings_draft = state.settings.clone();
                    state.stage_routing_draft = state.stage_routing.clone();
                    state.routing_editor_role_idx = 0;
                    state.mode = UiMode::Chat;
                }
                SettingsRowId::ThemeMenu => {
                    open_theme_menu(state, UiMode::Settings);
                }
                _ => {}
            }
        }
        _ => {}
    }

    EventOutcome::Continue
}

pub(crate) fn render_settings_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 96, 92);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    let has_pending_changes =
        state.settings_draft != state.settings || state.stage_routing_draft != state.stage_routing;
    let validation_errors = settings_validation_errors(&state.settings_draft);
    let change_label = if has_pending_changes {
        "Unsaved"
    } else {
        "Up to date"
    };
    let validation_label = if validation_errors.is_empty() {
        "Looks good"
    } else {
        "Fix issues first"
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(" State: {change_label} "),
            Style::default().fg(if has_pending_changes {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
        Span::styled("|", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" Check: {validation_label} "),
            Style::default().fg(if validation_errors.is_empty() {
                Color::Green
            } else {
                Color::Red
            }),
        ),
        Span::styled("|", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(" Secure: {} ", api_key_storage_backend_label()),
            Style::default().fg(Color::Cyan),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        " Navigate: Up/Down  Edit: Enter  Adjust: Space/Left/Right +/-  Tabs: Tab/Shift+Tab  Theme: T  Close: Ctrl+,/Esc ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            " Theme: {} ({})  |  Open picker: T ",
            ui_theme_display_name(&state.settings.theme_id),
            state.settings.theme_id
        ),
        Style::default().fg(Color::LightCyan),
    )));
    lines.push(Line::from(""));

    let active_section = active_settings_tab_index(state);
    let section_nav = SETTINGS_TABS
        .iter()
        .enumerate()
        .map(|(idx, (name, _, _))| {
            if idx == active_section {
                format!("[{name}]")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    lines.push(Line::from(Span::styled(
        format!(" Tabs: {section_nav} "),
        Style::default().fg(Color::LightCyan),
    )));
    lines.push(Line::from(Span::styled(
        section_help_text(active_section),
        Style::default().fg(Color::DarkGray),
    )));
    let direct_chars = parse_usize_with_bounds(
        &state
            .settings_draft
            .activation_direct_answer_max_prompt_chars,
        320,
        64,
        4000,
    );
    let long_turns = parse_usize_with_bounds(
        &state.settings_draft.activation_long_session_turn_threshold,
        28,
        8,
        500,
    );
    let long_chars = parse_usize_with_bounds(
        &state
            .settings_draft
            .activation_long_session_max_prompt_chars,
        420,
        64,
        4000,
    );
    let hot_turns = parse_usize_with_bounds(&state.settings_draft.slice_hot_window_turns, 8, 2, 32);
    let cold_turns =
        parse_usize_with_bounds(&state.settings_draft.slice_cold_window_turns, 24, 4, 128)
            .max(hot_turns);
    let retry_max = parse_usize_with_bounds(
        &state.settings_draft.retry_runtime_max_retries,
        1,
        medousa::agent_runtime::RETRY_LIMIT_MIN,
        medousa::agent_runtime::RETRY_LIMIT_MAX,
    );
    let retry_rounds = parse_usize_with_bounds(
        &state.settings_draft.retry_runtime_max_rounds,
        medousa::agent_runtime::turn_orchestrator::DEFAULT_RETRY_RUNTIME_MAX_ROUNDS,
        medousa::agent_runtime::ROUND_LIMIT_MIN,
        medousa::agent_runtime::ROUND_LIMIT_MAX,
    );
    let policy_mode = policy_mode_label(
        direct_chars,
        long_turns,
        long_chars,
        hot_turns,
        cold_turns,
        retry_max,
        retry_rounds,
    );
    let pressure = context_pressure_label(hot_turns, cold_turns, retry_max, retry_rounds);
    lines.push(Line::from(""));

    let selected_role = routing_editor_role(state);
    let selected_route = state
        .stage_routing_draft
        .get(selected_role)
        .expect("selected routing role should exist");

    let rows = vec![
        format!(
            "Where your data lives: {}  [toggle]",
            state.settings_draft.backend
        ),
        format!("AI provider: {}  [edit]", state.settings_draft.provider),
        format!("AI model: {}  [edit]", state.settings_draft.model),
        format!(
            "Custom server address: {}  [edit]",
            if state.settings_draft.base_url.is_empty() {
                "(automatic)".to_string()
            } else {
                state.settings_draft.base_url.clone()
            }
        ),
        format!(
            "API key: {}  [edit, secret]",
            mask_secret_value(&state.settings_draft.api_key)
        ),
        format!(
            "Tools the AI may use: {}  [edit]",
            if state.settings_draft.allowed_modules.trim().is_empty() {
                "(all tools)".to_string()
            } else {
                state.settings_draft.allowed_modules.clone()
            }
        ),
        format!(
            "Web search provider: {}  [edit]",
            if state.settings_draft.web_search_preferred_provider.trim().is_empty() {
                "Auto (capability order)".to_string()
            } else {
                state.settings_draft.web_search_preferred_provider.clone()
            }
        ),
        format!(
            "Try other search providers: {}  [toggle]",
            state.settings_draft.web_search_try_fallbacks
        ),
        format!(
            "When the AI uses tools: {}  [toggle]",
            state.settings_draft.tool_call_mode
        ),
        format!(
            "Tool steps per reply: {}  [number]",
            state.settings_draft.max_tool_rounds
        ),
        format!(
            "Tool steps (full mode): {}  [number]",
            state.settings_draft.host_bus_max_tool_rounds
        ),
        format!(
            "How tools are routed: {}  [toggle]",
            state.settings_draft.host_turn_bus_mode
        ),
        format!(
            "Extra tool steps (big tasks): {}  [number]",
            state.settings_draft.activation_tool_intent_max_rounds
        ),
        format!(
            "Tool steps (quick questions): {}  [number]",
            state.settings_draft.activation_short_turn_max_tool_rounds
        ),
        format!(
            "Extra steps when wrapping up: {}  [number]",
            state.settings_draft.continuation_max_tool_rounds
        ),
        format!(
            "Retries when stuck (no tools): {}  [number]",
            state.settings_draft.max_text_only_stuck_continues
        ),
        format!(
            "Tool steps (simple requests): {}  [number]",
            state.settings_draft.classifier_restricted_max_tool_rounds
        ),
        format!(
            "Short question size limit: {}  [number]",
            state
                .settings_draft
                .activation_direct_answer_max_prompt_chars
        ),
        format!(
            "Long chat starts after (turns): {}  [number]",
            state.settings_draft.activation_long_session_turn_threshold
        ),
        format!(
            "Long chat size limit: {}  [number]",
            state
                .settings_draft
                .activation_long_session_max_prompt_chars
        ),
        format!(
            "Recent messages kept nearby: {}  [number]",
            state.settings_draft.slice_hot_window_turns
        ),
        format!(
            "Older messages summarized: {}  [number]",
            state.settings_draft.slice_cold_window_turns
        ),
        format!(
            "Show AI reasoning in Obs: {}  [toggle]",
            state.settings_draft.thinking_capture
        ),
        format!(
            "Send traces to monitoring: {}  [toggle]",
            state.settings_draft.stasis_otel_enabled
        ),
        format!(
            "Max reasoning lines shown: {}  [number]",
            state.settings_draft.thinking_max_lines
        ),
        format!(
            "Retry whole turn on error: {}  [number]",
            state.settings_draft.retry_runtime_max_retries
        ),
        format!(
            "Retry tool steps on error: {}  [number]",
            state.settings_draft.retry_runtime_max_rounds
        ),
        format!(
            "Sources must cover claims: {}  [number]",
            state.settings_draft.verifier_min_citation_coverage
        ),
        format!(
            "Average source strength: {}  [number]",
            state.settings_draft.verifier_min_avg_support_strength
        ),
        format!(
            "Share of claims backed up: {}  [number]",
            state.settings_draft.verifier_min_supported_claim_ratio
        ),
        format!(
            "Strength per claim: {}  [number]",
            state.settings_draft.verifier_min_claim_support_strength
        ),
        format!(
            "Custom environment (advanced): {} line(s)  [open]",
            state
                .settings_draft
                .env_overrides
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    !trimmed.is_empty() && !trimmed.starts_with('#')
                })
                .count()
        ),
        "Check for problems  [action]".to_string(),
        "Remove saved API key  [action]".to_string(),
        "Save API key now  [action]".to_string(),
        format!("Task type: {}  [cycle]", selected_role),
        format!("Specialist provider: {}  [edit]", selected_route.provider),
        format!("Specialist model: {}  [edit]", selected_route.model),
        format!(
            "Use main AI for all tasks: {}:{}  [action]",
            state.settings_draft.provider.trim(),
            state.settings_draft.model.trim()
        ),
        format!(
            "Quick specialist preset: {}:{}  [cycle presets]",
            selected_route.provider, selected_route.model
        ),
        format!(
            "Specialist style: {}  [cycle]",
            selected_route.policy_profile
        ),
        format!(
            "Backup specialists: {}  [cycle]",
            selected_route.fallback_chain.join(",")
        ),
        "Reset this task type  [action]".to_string(),
        "Discard changes  [action]".to_string(),
        "Save and reload  [action]".to_string(),
        "Close without saving  [action]".to_string(),
        format!(
            "Appearance: {}  [open]",
            ui_theme_display_name(&state.settings.theme_id)
        ),
    ];

    let (start, end) = active_tab_bounds(state);
    let (tab_title, _, _) = SETTINGS_TABS[active_section];
    let tab_subtitle = match active_section {
        0 => "Who answers, and which tools it may use",
        1 => "How hard the AI works with tools each turn",
        2 => "How much conversation it remembers",
        3 => "What you see while it thinks",
        4 => "How strictly answers are fact-checked",
        5 => "Keys, environment, and safety checks",
        6 => "Different AIs for different kinds of work",
        _ => "Save, discard, or change appearance",
    };
    lines.push(Line::from(Span::styled(
        format!(" {tab_title} "),
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {tab_subtitle} "),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        " ------------------------------------------------------------ ",
        Style::default().fg(ui_border()),
    )));

    let mut selected_line: Option<usize> = None;
    for idx in start..=end {
        if idx > start {
            lines.push(Line::from(Span::styled(
                " ............................................................ ",
                Style::default().fg(Color::DarkGray),
            )));
        }
        if idx == state.settings_selected {
            selected_line = Some(lines.len());
        }
        let row = &rows[idx];
        let marker = if idx == state.settings_selected {
            ">"
        } else {
            " "
        };
        let mut style = row_style_for_settings_index(idx, idx == state.settings_selected);
        if idx == state.settings_selected
            && state.settings_editing
            && (matches!(idx, 1..=5) || is_routing_edit_row(settings_row_id(idx)))
        {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        lines.push(Line::from(Span::styled(format!("{marker} {row}"), style)));
    }
    lines.push(Line::from(""));

    let container = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_accent_primary()))
        .style(Style::default().bg(ui_modal_bg()));
    frame.render_widget(container.clone(), popup);
    let inner = container.inner(popup);
    let columns = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(72),
            ratatui::layout::Constraint::Percentage(28),
        ])
        .split(inner);
    let left_area = columns[0];
    let right_area = columns[1];

    let text = Text::from(lines);
    let inner_width = left_area.width;
    let visible_height = left_area.height;
    let visual_lines_left = visual_line_count(&text, inner_width);

    if let Some(line_idx) = selected_line {
        let visible_rows = visible_height as usize;
        if visible_rows > 0 {
            let top = state.settings_scroll as usize;
            let bottom = top.saturating_add(visible_rows.saturating_sub(1));
            if line_idx < top {
                state.settings_scroll = line_idx as u16;
            } else if line_idx > bottom {
                state.settings_scroll =
                    line_idx.saturating_add(1).saturating_sub(visible_rows) as u16;
            }
            state.settings_scroll = state.settings_scroll.min(state.settings_max_scroll);
        }
    }

    let panel = Paragraph::new(text)
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.settings_scroll, 0));
    frame.render_widget(panel, left_area);

    let mut rail: Vec<Line> = Vec::new();
    rail.push(Line::from(Span::styled(
        " At a glance ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    rail.push(Line::from(""));
    rail.push(Line::from(vec![
        Span::styled("Working style: ", Style::default().fg(Color::DarkGray)),
        Span::styled(policy_mode.0, Style::default().fg(policy_mode.1)),
    ]));
    rail.push(Line::from(vec![
        Span::styled("Memory load: ", Style::default().fg(Color::DarkGray)),
        Span::styled(pressure.0, Style::default().fg(pressure.1)),
    ]));
    rail.push(Line::from(vec![
        Span::styled("Theme: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            ui_theme_display_name(&state.settings.theme_id),
            Style::default().fg(Color::Cyan),
        ),
    ]));
    rail.push(Line::from(""));

    match active_section {
        0 => {
            rail.push(Line::from(Span::styled(
                "Setup",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Provider: {}",
                state.settings_draft.provider
            )));
            rail.push(Line::from(format!("Model: {}", state.settings_draft.model)));
            rail.push(Line::from(format!(
                "Server: {}",
                if state.settings_draft.base_url.trim().is_empty() {
                    "Automatic".to_string()
                } else {
                    "Custom".to_string()
                }
            )));
            rail.push(Line::from(format!(
                "API key: {}",
                if state.settings_draft.api_key.trim().is_empty() {
                    "Not set"
                } else {
                    "Saved"
                }
            )));
        }
        1 => {
            rail.push(Line::from(Span::styled(
                "Tools",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Tool mode: {}",
                state.settings_draft.tool_call_mode
            )));
            rail.push(Line::from(format!(
                "Steps per reply: {}",
                state.settings_draft.max_tool_rounds
            )));
            rail.push(Line::from(format!(
                "Full-mode steps: {}",
                state.settings_draft.host_bus_max_tool_rounds
            )));
            rail.push(Line::from(format!(
                "Routing: {}",
                state.settings_draft.host_turn_bus_mode
            )));
        }
        2 => {
            rail.push(Line::from(Span::styled(
                "Memory",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Short questions up to {} chars",
                direct_chars
            )));
            rail.push(Line::from(format!(
                "Long chat after {} turns",
                long_turns
            )));
            rail.push(Line::from(format!(
                "Long chat up to {} chars",
                long_chars
            )));
            rail.push(Line::from(format!(
                "Keep {} recent / {} summarized turns",
                hot_turns, cold_turns
            )));
        }
        3 => {
            rail.push(Line::from(Span::styled(
                "Diagnostics",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Show reasoning: {}",
                state.settings_draft.thinking_capture
            )));
            rail.push(Line::from(format!(
                "Send traces: {}",
                state.settings_draft.stasis_otel_enabled
            )));
            rail.push(Line::from(format!(
                "Reasoning lines: {}",
                state.settings_draft.thinking_max_lines
            )));
        }
        4 => {
            rail.push(Line::from(Span::styled(
                "Quality",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Turn retries: {}",
                retry_max
            )));
            rail.push(Line::from(format!(
                "Tool retries: {}",
                retry_rounds
            )));
            rail.push(Line::from(format!(
                "Source coverage: {}",
                state.settings_draft.verifier_min_citation_coverage
            )));
            rail.push(Line::from(format!(
                "Claims backed: {}",
                state.settings_draft.verifier_min_supported_claim_ratio
            )));
        }
        5 => {
            rail.push(Line::from(Span::styled(
                "Secrets",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Issues to fix: {}",
                validation_errors.len()
            )));
            rail.push(Line::from(format!(
                "Custom env lines: {}",
                state
                    .settings_draft
                    .env_overrides
                    .lines()
                    .filter(|line| {
                        let trimmed = line.trim();
                        !trimmed.is_empty() && !trimmed.starts_with('#')
                    })
                    .count()
            )));
            rail.push(Line::from("Run “Check for problems” before saving."));
        }
        6 => {
            rail.push(Line::from(Span::styled(
                "Specialists",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!("Task: {}", selected_role)));
            rail.push(Line::from(format!(
                "Uses: {}:{}",
                selected_route.provider, selected_route.model
            )));
            rail.push(Line::from(format!(
                "Style: {}",
                selected_route.policy_profile
            )));
            rail.push(Line::from(format!(
                "Backups: {}",
                selected_route.fallback_chain.join(" → ")
            )));
        }
        _ => {
            rail.push(Line::from(Span::styled(
                "Save",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Unsaved changes: {}",
                if has_pending_changes { "Yes" } else { "No" }
            )));
            rail.push(Line::from("Discard — drop this draft."));
            rail.push(Line::from("Save and reload — makes it live."));
            rail.push(Line::from(format!(
                "Theme: {}",
                ui_theme_display_name(&state.settings.theme_id)
            )));
        }
    }

    rail.push(Line::from(""));
    rail.push(Line::from(Span::styled(
        "Tip",
        Style::default().fg(Color::DarkGray),
    )));
    rail.push(Line::from("Change one thing, try a turn, repeat."));

    let rail_text = Text::from(rail);
    let rail_visual_lines = visual_line_count(&rail_text, right_area.width.saturating_sub(1));
    state.settings_max_scroll = visual_lines_left
        .max(rail_visual_lines)
        .saturating_sub(visible_height);
    state.settings_scroll = state.settings_scroll.min(state.settings_max_scroll);

    let rail_panel = Paragraph::new(rail_text)
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(ui_border())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.settings_scroll, 0));
    frame.render_widget(rail_panel, right_area);
}

pub(crate) fn handle_runtime_env_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    if !state.runtime_env_editing {
        state.runtime_env_editing = true;
    }

    match code {
        KeyCode::Esc => {
            state.runtime_env_editing = false;
            state.mode = UiMode::Settings;
        }
        KeyCode::Enter => {
            state.settings_draft.env_overrides.push('\n');
        }
        KeyCode::Backspace => {
            state.settings_draft.env_overrides.pop();
        }
        KeyCode::Tab => {
            state.settings_draft.env_overrides.push('=');
        }
        KeyCode::Char(c) => {
            state.settings_draft.env_overrides.push(c);
        }
        _ => {}
    }

    EventOutcome::Continue
}

pub(crate) fn render_runtime_env_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 78, 66);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Environment Variables ",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        " Format: KEY=VALUE. One per line. Empty lines and # comments are ignored. ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        " Esc: back  Enter: new line  Tab: insert '=' ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let env_errors = env_overrides_validation_errors(&state.settings_draft.env_overrides);
    if env_errors.is_empty() {
        lines.push(Line::from(Span::styled(
            " Status: ready ",
            Style::default().fg(Color::Green),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(" Status: {} issue(s) ", env_errors.len()),
            Style::default().fg(Color::Red),
        )));
        for err in env_errors.iter().take(3) {
            lines.push(Line::from(Span::styled(
                format!(" - {err}"),
                Style::default().fg(Color::Red),
            )));
        }
    }
    lines.push(Line::from(""));

    if state.settings_draft.env_overrides.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "# Example",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "MEDOUSA_LLM_PROVIDER=openai",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "MEDOUSA_LLM_MODEL=gpt-4o-mini",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for line in state.settings_draft.env_overrides.lines() {
            lines.push(Line::from(line.to_string()));
        }
    }

    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Custom environment ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, popup);
}

pub(crate) fn emit_settings_validation_summary(state: &mut TuiState) -> bool {
    let errors = settings_validation_errors(&state.settings_draft);
    if errors.is_empty() {
        push_obs(state, "✓ ready to save — nothing blocking you".to_string());
        true
    } else {
        for error in errors {
            push_obs(state, format!("⚠ before saving: {error}"));
        }
        false
    }
}

fn quick_adjust_setting(state: &mut TuiState, forward: bool) {
    quick_adjust_row(state, settings_row_id(state.settings_selected), forward);
}


fn sync_all_route_targets_to_global(state: &mut TuiState) {
    let provider = state.settings_draft.provider.trim();
    let model = state.settings_draft.model.trim();
    if provider.is_empty() || model.is_empty() {
        push_obs(
            state,
            "⚠ set your main AI provider and model first".to_string(),
        );
        return;
    }

    for role in medousa::stage_routing::StageRoutingMatrix::roles() {
        if let Some(route) = state.stage_routing_draft.get_mut(role) {
            route.provider = provider.to_string();
            route.model = model.to_string();
        }
    }

    push_obs(
        state,
        format!(
            "✓ all task types now use {}:{}",
            provider, model
        ),
    );
}

pub(crate) fn routing_editor_role(state: &TuiState) -> &'static str {
    let roles = medousa::stage_routing::StageRoutingMatrix::roles();
    roles
        .get(state.routing_editor_role_idx % roles.len())
        .copied()
        .unwrap_or("final_response")
}

pub(crate) fn route_target_presets() -> [&'static str; 5] {
    [
        "openai:gpt-4o-mini",
        "anthropic:claude-3-7-sonnet-latest",
        "google:gemini-2.5-pro",
        "xai:grok-3-mini",
        "ollama:llama3.2",
    ]
}

fn selected_route_field_mut(state: &mut TuiState) -> Option<&mut String> {
    use settings_rows::SettingsRowId;
    let role = routing_editor_role(state).to_string();
    let route = state.stage_routing_draft.get_mut(&role)?;
    match settings_row_id(state.settings_selected) {
        SettingsRowId::RouteProvider => Some(&mut route.provider),
        SettingsRowId::RouteModel => Some(&mut route.model),
        _ => None,
    }
}

fn switch_settings_tab(state: &mut TuiState, forward: bool) {
    let current = active_settings_tab_index(state);
    let next = if forward {
        (current + 1) % SETTINGS_TABS.len()
    } else if current == 0 {
        SETTINGS_TABS.len() - 1
    } else {
        current - 1
    };

    state.settings_tab = next;
    let (_, start, _) = SETTINGS_TABS[next];
    state.settings_selected = start;
    state.settings_editing = false;
    state.settings_scroll = 0;
    state.settings_max_scroll = 0;
}

fn active_settings_tab_index(state: &TuiState) -> usize {
    state
        .settings_tab
        .min(SETTINGS_TABS.len().saturating_sub(1))
}

fn active_tab_bounds(state: &TuiState) -> (usize, usize) {
    let tab = active_settings_tab_index(state);
    let (_, start, end) = SETTINGS_TABS[tab];
    (start, end)
}

fn clamp_selected_to_active_tab(state: &mut TuiState) {
    let (start, end) = active_tab_bounds(state);
    state.settings_selected = state.settings_selected.clamp(start, end);
}

fn row_style_for_settings_index(idx: usize, selected: bool) -> Style {
    use settings_rows::SettingsRowId;
    let base = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        match settings_row_id(idx) {
            SettingsRowId::EnvOverrides => Style::default().fg(Color::Cyan),
            SettingsRowId::ApplyChanges => Style::default().fg(Color::Green),
            SettingsRowId::Cancel => Style::default().fg(Color::LightRed),
            SettingsRowId::ThemeMenu => Style::default().fg(Color::Cyan),
            SettingsRowId::ReviewConfiguration => Style::default().fg(Color::LightYellow),
            SettingsRowId::ClearApiKey | SettingsRowId::UpdateApiKey => {
                Style::default().fg(Color::LightMagenta)
            }
            SettingsRowId::RouteRole
            | SettingsRowId::RouteProvider
            | SettingsRowId::RouteModel
            | SettingsRowId::SetAllRouteTargets
            | SettingsRowId::RouteTargetPreset
            | SettingsRowId::RoutePolicyProfile
            | SettingsRowId::RouteFallbackChain
            | SettingsRowId::ResetSelectedRoute => Style::default().fg(Color::LightCyan),
            _ => Style::default().fg(Color::White),
        }
    };

    if matches!(
        settings_row_id(idx),
        SettingsRowId::RouteRole
            | SettingsRowId::RouteProvider
            | SettingsRowId::RouteModel
            | SettingsRowId::SetAllRouteTargets
            | SettingsRowId::RouteTargetPreset
            | SettingsRowId::RoutePolicyProfile
            | SettingsRowId::RouteFallbackChain
            | SettingsRowId::ResetSelectedRoute
            | SettingsRowId::RevertChanges
            | SettingsRowId::ApplyChanges
            | SettingsRowId::Cancel
            | SettingsRowId::ThemeMenu
    ) {
        base.add_modifier(Modifier::BOLD)
    } else {
        base
    }
}

fn section_help_text(active_section: usize) -> &'static str {
    match active_section {
        0 => " Pick your AI, where data is stored, and which tools are allowed.",
        1 => " Control tool use per reply — great for speed vs thoroughness.",
        2 => " Tune memory: short chats, long chats, recent vs summarized history.",
        3 => " Optional: watch reasoning in Obs, or send traces to your monitor.",
        4 => " How picky the assistant is about sources and evidence.",
        5 => " API keys, custom env vars, and a quick sanity check.",
        6 => " Route task types to specialist models with backups.",
        _ => " Save when ready, or discard and keep what you had.",
    }
}

fn policy_mode_label(
    direct_chars: usize,
    long_turns: usize,
    long_chars: usize,
    hot_turns: usize,
    cold_turns: usize,
    retry_max: usize,
    retry_rounds: usize,
) -> (&'static str, Color) {
    let mut score = 0isize;
    if direct_chars >= 700 {
        score += 1;
    }
    if long_turns >= 40 {
        score += 1;
    }
    if long_chars >= 700 {
        score += 1;
    }
    if hot_turns >= 12 {
        score += 1;
    }
    if cold_turns >= 40 {
        score += 1;
    }
    if retry_max >= 2 {
        score += 1;
    }
    if retry_rounds >= 4 {
        score += 1;
    }

    if score >= 5 {
        ("Aggressive", Color::LightYellow)
    } else if score <= 1 {
        ("Conservative", Color::LightGreen)
    } else {
        ("Balanced", Color::LightCyan)
    }
}

fn context_pressure_label(
    hot_turns: usize,
    cold_turns: usize,
    retry_max: usize,
    retry_rounds: usize,
) -> (&'static str, Color) {
    let pressure = hot_turns.saturating_add(cold_turns / 2)
        + retry_max.saturating_mul(4)
        + retry_rounds.saturating_mul(2);

    if pressure >= 38 {
        ("High", Color::LightRed)
    } else if pressure >= 24 {
        ("Medium", Color::Yellow)
    } else {
        ("Low", Color::LightGreen)
    }
}

fn visual_line_count(text: &Text, inner_width: u16) -> u16 {
    if inner_width == 0 {
        return text.lines.len() as u16;
    }

    text.lines
        .iter()
        .map(|line| {
            let w = line.width() as u16;
            if w == 0 { 1 } else { w.div_ceil(inner_width) }
        })
        .fold(0u16, |acc, rows| acc.saturating_add(rows))
}
