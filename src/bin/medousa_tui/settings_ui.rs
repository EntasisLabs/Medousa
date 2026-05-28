use super::*;

const SETTINGS_SECTIONS: [(&str, usize, usize); 6] = [
    ("Model", 0, 5),
    ("Runtime", 6, 16),
    ("Verifier", 17, 20),
    ("Safety", 21, 24),
    ("Routing", 25, 32),
    ("Session", 33, 36),
];

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
                } else {
                    let target = selected_settings_field_mut(state);
                    target.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(target) = selected_route_field_mut(state) {
                    target.push(c);
                } else {
                    let target = selected_settings_field_mut(state);
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
            if matches!(
                state.settings_selected,
                7 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20
            ) {
                quick_adjust_setting(state, true);
            }
        }
        KeyCode::Char('-') => {
            if matches!(
                state.settings_selected,
                7 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20
            ) {
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
        KeyCode::Enter => match state.settings_selected {
            1..=5 | 26 | 27 => {
                state.settings_editing = true;
            }
            0 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 25 | 29
            | 30 | 31 | 32 => {
                quick_adjust_setting(state, true);
            }
            21 => {
                state.mode = UiMode::RuntimeEnv;
                state.runtime_env_editing = true;
            }
            22 => {
                emit_settings_validation_summary(state);
            }
            23 => {
                state.settings_draft.api_key.clear();
                push_obs(
                    state,
                    "✓ API key will be cleared when changes are applied".to_string(),
                );
            }
            24 => {
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
            28 => {
                sync_all_route_targets_to_global(state);
            }
            33 => {
                state.settings_draft = state.settings.clone();
                state.stage_routing_draft = state.stage_routing.clone();
                state.routing_editor_role_idx = 0;
                state.settings_editing = false;
                push_obs(state, "✓ changes reverted".to_string());
            }
            34 => {
                super::apply_settings(state, tui_rt, event_tx).await;
                state.mode = UiMode::Chat;
            }
            35 => {
                state.settings_draft = state.settings.clone();
                state.stage_routing_draft = state.stage_routing.clone();
                state.routing_editor_role_idx = 0;
                state.mode = UiMode::Chat;
            }
            36 => {
                open_theme_menu(state, UiMode::Settings);
            }
            _ => {}
        },
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
        "Pending"
    } else {
        "Applied"
    };
    let validation_label = if validation_errors.is_empty() {
        "Ready"
    } else {
        "Needs review"
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
            format!(" Validation: {validation_label} "),
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
    let section_nav = SETTINGS_SECTIONS
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
    let retry_max =
        parse_usize_with_bounds(&state.settings_draft.retry_runtime_max_retries, 1, 0, 5);
    let retry_rounds =
        parse_usize_with_bounds(&state.settings_draft.retry_runtime_max_rounds, 3, 1, 10);
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
            "Runtime backend: {}  [toggle]",
            state.settings_draft.backend
        ),
        format!("Provider: {}  [edit]", state.settings_draft.provider),
        format!("Model: {}  [edit]", state.settings_draft.model),
        format!(
            "Base URL: {}  [edit]",
            if state.settings_draft.base_url.is_empty() {
                "(auto)".to_string()
            } else {
                state.settings_draft.base_url.clone()
            }
        ),
        format!(
            "API Key: {}  [edit, secret]",
            mask_secret_value(&state.settings_draft.api_key)
        ),
        format!(
            "Allowed modules: {}  [edit]",
            if state.settings_draft.allowed_modules.trim().is_empty() {
                "(all)".to_string()
            } else {
                state.settings_draft.allowed_modules.clone()
            }
        ),
        format!(
            "Tool calls: {}  [toggle]",
            state.settings_draft.tool_call_mode
        ),
        format!(
            "Max Tool Rounds: {}  [number]",
            state.settings_draft.max_tool_rounds
        ),
        format!(
            "Thinking Capture: {}  [toggle]",
            state.settings_draft.thinking_capture
        ),
        format!(
            "Thinking Max Lines: {}  [number]",
            state.settings_draft.thinking_max_lines
        ),
        format!(
            "Activation Direct Prompt Max Chars: {}  [number]",
            state
                .settings_draft
                .activation_direct_answer_max_prompt_chars
        ),
        format!(
            "Activation Long Session Turn Threshold: {}  [number]",
            state.settings_draft.activation_long_session_turn_threshold
        ),
        format!(
            "Activation Long Session Prompt Max Chars: {}  [number]",
            state
                .settings_draft
                .activation_long_session_max_prompt_chars
        ),
        format!(
            "Slice Hot Window Turns: {}  [number]",
            state.settings_draft.slice_hot_window_turns
        ),
        format!(
            "Slice Cold Window Turns: {}  [number]",
            state.settings_draft.slice_cold_window_turns
        ),
        format!(
            "Retry Runtime Max Retries: {}  [number]",
            state.settings_draft.retry_runtime_max_retries
        ),
        format!(
            "Retry Runtime Max Rounds: {}  [number]",
            state.settings_draft.retry_runtime_max_rounds
        ),
        format!(
            "Verifier Min Citation Coverage: {}  [number]",
            state.settings_draft.verifier_min_citation_coverage
        ),
        format!(
            "Verifier Min Avg Support Strength: {}  [number]",
            state.settings_draft.verifier_min_avg_support_strength
        ),
        format!(
            "Verifier Min Supported Claim Ratio: {}  [number]",
            state.settings_draft.verifier_min_supported_claim_ratio
        ),
        format!(
            "Verifier Min Claim Support Strength: {}  [number]",
            state.settings_draft.verifier_min_claim_support_strength
        ),
        format!(
            "Environment variables: {} line(s)  [open]",
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
        "Review configuration  [action]".to_string(),
        "Clear API key  [action]".to_string(),
        "Update API key  [action]".to_string(),
        format!("Route Role: {}  [cycle]", selected_role),
        format!("Route Provider: {}  [edit]", selected_route.provider),
        format!("Route Model: {}  [edit]", selected_route.model),
        format!(
            "Set All Route Targets: {}:{}  [action]",
            state.settings_draft.provider.trim(),
            state.settings_draft.model.trim()
        ),
        format!(
            "Route Target Preset: {}:{}  [cycle presets]",
            selected_route.provider, selected_route.model
        ),
        format!(
            "Route Policy Profile: {}  [cycle]",
            selected_route.policy_profile
        ),
        format!(
            "Route Fallback Chain: {}  [cycle]",
            selected_route.fallback_chain.join(",")
        ),
        "Reset Selected Route to Defaults  [action]".to_string(),
        "Revert changes  [action]".to_string(),
        "Apply changes  [action]".to_string(),
        "Cancel  [action]".to_string(),
        format!(
            "Theme menu: {}  [open]",
            ui_theme_display_name(&state.settings.theme_id)
        ),
    ];

    let (start, end) = active_tab_bounds(state);
    let (tab_title, _, _) = SETTINGS_SECTIONS[active_section];
    let tab_subtitle = match active_section {
        0 => "Provider, model, and access",
        1 => "Tool behavior and turn policy",
        2 => "Evidence confidence thresholds",
        3 => "Secrets and safety checks",
        4 => "Role targets and fallback",
        _ => "Commit or discard your draft",
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
            && matches!(idx, 1..=5 | 26 | 27)
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
        " Quick View ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    rail.push(Line::from(""));
    rail.push(Line::from(vec![
        Span::styled("Assistant Style: ", Style::default().fg(Color::DarkGray)),
        Span::styled(policy_mode.0, Style::default().fg(policy_mode.1)),
    ]));
    rail.push(Line::from(vec![
        Span::styled("Context Load: ", Style::default().fg(Color::DarkGray)),
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
                "Model",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Provider: {}",
                state.settings_draft.provider
            )));
            rail.push(Line::from(format!("Model: {}", state.settings_draft.model)));
            rail.push(Line::from(format!(
                "Base URL: {}",
                if state.settings_draft.base_url.trim().is_empty() {
                    "Auto".to_string()
                } else {
                    "Custom".to_string()
                }
            )));
            rail.push(Line::from(format!(
                "API key: {}",
                if state.settings_draft.api_key.trim().is_empty() {
                    "Not set"
                } else {
                    "Configured"
                }
            )));
        }
        1 => {
            rail.push(Line::from(Span::styled(
                "Activation",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Direct answer under {} chars",
                direct_chars
            )));
            rail.push(Line::from(format!(
                "Long-session trigger after {} turns",
                long_turns
            )));
            rail.push(Line::from(format!(
                "Long-session prompt under {} chars",
                long_chars
            )));
            rail.push(Line::from(""));
            rail.push(Line::from(Span::styled(
                "Window + Retry",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "History window: {} hot / {} cold",
                hot_turns, cold_turns
            )));
            rail.push(Line::from(format!("Runtime retries: {} max", retry_max)));
            rail.push(Line::from(format!("Retry rounds: {}", retry_rounds)));
        }
        2 => {
            rail.push(Line::from(Span::styled(
                "Verifier",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Citation coverage: {}",
                state.settings_draft.verifier_min_citation_coverage
            )));
            rail.push(Line::from(format!(
                "Avg support: {}",
                state.settings_draft.verifier_min_avg_support_strength
            )));
            rail.push(Line::from(format!(
                "Supported claims: {}",
                state.settings_draft.verifier_min_supported_claim_ratio
            )));
            rail.push(Line::from(format!(
                "Claim support floor: {}",
                state.settings_draft.verifier_min_claim_support_strength
            )));
        }
        3 => {
            rail.push(Line::from(Span::styled(
                "Safety",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Validation issues: {}",
                validation_errors.len()
            )));
            rail.push(Line::from(format!(
                "Env lines: {}",
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
            rail.push(Line::from("Use Review before Apply."));
        }
        4 => {
            rail.push(Line::from(Span::styled(
                "Routing",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!("Role: {}", selected_role)));
            rail.push(Line::from(format!(
                "Target: {}:{}",
                selected_route.provider, selected_route.model
            )));
            rail.push(Line::from(format!(
                "Policy profile: {}",
                selected_route.policy_profile
            )));
            rail.push(Line::from(format!(
                "Fallback: {}",
                selected_route.fallback_chain.join(" -> ")
            )));
        }
        _ => {
            rail.push(Line::from(Span::styled(
                "Session",
                Style::default().fg(Color::Cyan),
            )));
            rail.push(Line::from(format!(
                "Draft changes: {}",
                if has_pending_changes { "Yes" } else { "No" }
            )));
            rail.push(Line::from("Revert resets this draft."));
            rail.push(Line::from("Apply writes runtime + defaults."));
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
    rail.push(Line::from("Tune one setting, then run."));

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
                .title(" Environment ")
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
        push_obs(state, "✓ configuration ready".to_string());
        true
    } else {
        for error in errors {
            push_obs(state, format!("⚠ configuration: {error}"));
        }
        false
    }
}

fn quick_adjust_setting(state: &mut TuiState, forward: bool) {
    match state.settings_selected {
        0 => {
            state.settings_draft.backend = cycle_backend(&state.settings_draft.backend, forward);
        }
        6 => {
            state.settings_draft.tool_call_mode =
                cycle_tool_call_mode(&state.settings_draft.tool_call_mode, forward);
        }
        7 => {
            let current = parse_usize_with_bounds(&state.settings_draft.max_tool_rounds, 10, 1, 50);
            let step = if current < 20 { 1 } else { 5 };
            let next = if forward {
                current.saturating_add(step)
            } else {
                current.saturating_sub(step)
            }
            .clamp(1, 50);
            state.settings_draft.max_tool_rounds = next.to_string();
        }
        8 => {
            let value = parse_bool_with_default(&state.settings_draft.thinking_capture, true);
            state.settings_draft.thinking_capture = (!value).to_string();
        }
        9 => {
            let current =
                parse_usize_with_bounds(&state.settings_draft.thinking_max_lines, 300, 50, 5000);
            let step = if current < 500 { 50 } else { 100 };
            let next = if forward {
                current.saturating_add(step)
            } else {
                current.saturating_sub(step)
            }
            .clamp(50, 5000);
            state.settings_draft.thinking_max_lines = next.to_string();
        }
        10 => {
            let current = parse_usize_with_bounds(
                &state
                    .settings_draft
                    .activation_direct_answer_max_prompt_chars,
                320,
                64,
                4000,
            );
            let step = if current < 1000 { 32 } else { 128 };
            let next = if forward {
                current.saturating_add(step)
            } else {
                current.saturating_sub(step)
            }
            .clamp(64, 4000);
            state
                .settings_draft
                .activation_direct_answer_max_prompt_chars = next.to_string();
        }
        11 => {
            let current = parse_usize_with_bounds(
                &state.settings_draft.activation_long_session_turn_threshold,
                28,
                8,
                500,
            );
            let step = if current < 100 { 1 } else { 10 };
            let next = if forward {
                current.saturating_add(step)
            } else {
                current.saturating_sub(step)
            }
            .clamp(8, 500);
            state.settings_draft.activation_long_session_turn_threshold = next.to_string();
        }
        12 => {
            let current = parse_usize_with_bounds(
                &state
                    .settings_draft
                    .activation_long_session_max_prompt_chars,
                420,
                64,
                4000,
            );
            let step = if current < 1000 { 32 } else { 128 };
            let next = if forward {
                current.saturating_add(step)
            } else {
                current.saturating_sub(step)
            }
            .clamp(64, 4000);
            state
                .settings_draft
                .activation_long_session_max_prompt_chars = next.to_string();
        }
        13 => {
            let current =
                parse_usize_with_bounds(&state.settings_draft.slice_hot_window_turns, 8, 2, 32);
            let next = if forward {
                current.saturating_add(1)
            } else {
                current.saturating_sub(1)
            }
            .clamp(2, 32);
            state.settings_draft.slice_hot_window_turns = next.to_string();
            let cold =
                parse_usize_with_bounds(&state.settings_draft.slice_cold_window_turns, 24, 4, 128);
            if cold < next {
                state.settings_draft.slice_cold_window_turns = next.to_string();
            }
        }
        14 => {
            let hot =
                parse_usize_with_bounds(&state.settings_draft.slice_hot_window_turns, 8, 2, 32);
            let current =
                parse_usize_with_bounds(&state.settings_draft.slice_cold_window_turns, 24, 4, 128);
            let next = if forward {
                current.saturating_add(1)
            } else {
                current.saturating_sub(1)
            }
            .clamp(4, 128)
            .max(hot);
            state.settings_draft.slice_cold_window_turns = next.to_string();
        }
        15 => {
            let current =
                parse_usize_with_bounds(&state.settings_draft.retry_runtime_max_retries, 1, 0, 5);
            let next = if forward {
                current.saturating_add(1)
            } else {
                current.saturating_sub(1)
            }
            .clamp(0, 5);
            state.settings_draft.retry_runtime_max_retries = next.to_string();
        }
        16 => {
            let current =
                parse_usize_with_bounds(&state.settings_draft.retry_runtime_max_rounds, 3, 1, 10);
            let next = if forward {
                current.saturating_add(1)
            } else {
                current.saturating_sub(1)
            }
            .clamp(1, 10);
            state.settings_draft.retry_runtime_max_rounds = next.to_string();
        }
        17 => {
            let current = parse_f32_with_bounds(
                &state.settings_draft.verifier_min_citation_coverage,
                0.60,
                0.0,
                1.0,
            );
            let step = 0.05;
            let next = if forward {
                current + step
            } else {
                current - step
            }
            .clamp(0.0, 1.0);
            state.settings_draft.verifier_min_citation_coverage = format!("{next:.2}");
        }
        18 => {
            let current = parse_f32_with_bounds(
                &state.settings_draft.verifier_min_avg_support_strength,
                0.70,
                0.0,
                1.0,
            );
            let step = 0.05;
            let next = if forward {
                current + step
            } else {
                current - step
            }
            .clamp(0.0, 1.0);
            state.settings_draft.verifier_min_avg_support_strength = format!("{next:.2}");
        }
        19 => {
            let current = parse_f32_with_bounds(
                &state.settings_draft.verifier_min_supported_claim_ratio,
                0.60,
                0.0,
                1.0,
            );
            let step = 0.05;
            let next = if forward {
                current + step
            } else {
                current - step
            }
            .clamp(0.0, 1.0);
            state.settings_draft.verifier_min_supported_claim_ratio = format!("{next:.2}");
        }
        20 => {
            let current = parse_f32_with_bounds(
                &state.settings_draft.verifier_min_claim_support_strength,
                0.65,
                0.0,
                1.0,
            );
            let step = 0.05;
            let next = if forward {
                current + step
            } else {
                current - step
            }
            .clamp(0.0, 1.0);
            state.settings_draft.verifier_min_claim_support_strength = format!("{next:.2}");
        }
        25 => {
            let roles = medousa::stage_routing::StageRoutingMatrix::roles();
            if roles.is_empty() {
                return;
            }
            state.routing_editor_role_idx = if forward {
                (state.routing_editor_role_idx + 1) % roles.len()
            } else if state.routing_editor_role_idx == 0 {
                roles.len() - 1
            } else {
                state.routing_editor_role_idx - 1
            };
        }
        29 => {
            let role = routing_editor_role(state).to_string();
            if let Some(route) = state.stage_routing_draft.get_mut(&role) {
                let presets = route_target_presets();
                let current = format!("{}:{}", route.provider, route.model);
                let idx = presets.iter().position(|v| *v == current).unwrap_or(0);
                let next = if forward {
                    (idx + 1) % presets.len()
                } else if idx == 0 {
                    presets.len() - 1
                } else {
                    idx - 1
                };
                if let Some((provider, model)) = presets[next].split_once(':') {
                    route.provider = provider.to_string();
                    route.model = model.to_string();
                }
            }
        }
        30 => {
            let role = routing_editor_role(state).to_string();
            if let Some(route) = state.stage_routing_draft.get_mut(&role) {
                let options = ["balanced", "strict", "analytical", "fast"];
                let idx = options
                    .iter()
                    .position(|v| v.eq_ignore_ascii_case(route.policy_profile.as_str()))
                    .unwrap_or(0);
                let next = if forward {
                    (idx + 1) % options.len()
                } else if idx == 0 {
                    options.len() - 1
                } else {
                    idx - 1
                };
                route.policy_profile = options[next].to_string();
            }
        }
        31 => {
            let role = routing_editor_role(state).to_string();
            if let Some(route) = state.stage_routing_draft.get_mut(&role) {
                let options = vec![
                    vec![role.clone(), "safe-default".to_string()],
                    vec!["safe-default".to_string()],
                    vec![role, "balanced".to_string(), "safe-default".to_string()],
                ];
                let idx = options
                    .iter()
                    .position(|v| *v == route.fallback_chain)
                    .unwrap_or(0);
                let next = if forward {
                    (idx + 1) % options.len()
                } else if idx == 0 {
                    options.len() - 1
                } else {
                    idx - 1
                };
                route.fallback_chain = options[next].clone();
            }
        }
        32 => {
            let role = routing_editor_role(state).to_string();
            let defaults = medousa::stage_routing::StageRoutingMatrix::default_for(
                &state.settings_draft.provider,
                &state.settings_draft.model,
            );
            if let (Some(current), Some(default_route)) = (
                state.stage_routing_draft.get_mut(&role),
                defaults.get(&role),
            ) {
                *current = default_route.clone();
            }
        }
        _ => {}
    }
}

fn sync_all_route_targets_to_global(state: &mut TuiState) {
    let provider = state.settings_draft.provider.trim();
    let model = state.settings_draft.model.trim();
    if provider.is_empty() || model.is_empty() {
        push_obs(
            state,
            "⚠ cannot sync route targets: Provider and Model must be set".to_string(),
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
            "✓ route targets synced to {}:{} for all stages",
            provider, model
        ),
    );
}

fn routing_editor_role(state: &TuiState) -> &'static str {
    let roles = medousa::stage_routing::StageRoutingMatrix::roles();
    roles
        .get(state.routing_editor_role_idx % roles.len())
        .copied()
        .unwrap_or("final_response")
}

fn route_target_presets() -> [&'static str; 5] {
    [
        "openai:gpt-4o-mini",
        "anthropic:claude-3-7-sonnet-latest",
        "google:gemini-2.5-pro",
        "xai:grok-3-mini",
        "ollama:llama3.2",
    ]
}

fn selected_settings_field_mut(state: &mut TuiState) -> &mut String {
    match state.settings_selected {
        0 => &mut state.settings_draft.backend,
        1 => &mut state.settings_draft.provider,
        2 => &mut state.settings_draft.model,
        3 => &mut state.settings_draft.base_url,
        4 => &mut state.settings_draft.api_key,
        5 => &mut state.settings_draft.allowed_modules,
        6 => &mut state.settings_draft.tool_call_mode,
        7 => &mut state.settings_draft.max_tool_rounds,
        8 => &mut state.settings_draft.thinking_capture,
        9 => &mut state.settings_draft.thinking_max_lines,
        10 => {
            &mut state
                .settings_draft
                .activation_direct_answer_max_prompt_chars
        }
        11 => &mut state.settings_draft.activation_long_session_turn_threshold,
        12 => {
            &mut state
                .settings_draft
                .activation_long_session_max_prompt_chars
        }
        13 => &mut state.settings_draft.slice_hot_window_turns,
        14 => &mut state.settings_draft.slice_cold_window_turns,
        15 => &mut state.settings_draft.retry_runtime_max_retries,
        16 => &mut state.settings_draft.retry_runtime_max_rounds,
        17 => &mut state.settings_draft.verifier_min_citation_coverage,
        18 => &mut state.settings_draft.verifier_min_avg_support_strength,
        19 => &mut state.settings_draft.verifier_min_supported_claim_ratio,
        20 => &mut state.settings_draft.verifier_min_claim_support_strength,
        _ => &mut state.settings_draft.base_url,
    }
}

fn selected_route_field_mut(state: &mut TuiState) -> Option<&mut String> {
    let role = routing_editor_role(state).to_string();
    let route = state.stage_routing_draft.get_mut(&role)?;
    match state.settings_selected {
        26 => Some(&mut route.provider),
        27 => Some(&mut route.model),
        _ => None,
    }
}

fn switch_settings_tab(state: &mut TuiState, forward: bool) {
    let current = active_settings_tab_index(state);
    let next = if forward {
        (current + 1) % SETTINGS_SECTIONS.len()
    } else if current == 0 {
        SETTINGS_SECTIONS.len() - 1
    } else {
        current - 1
    };

    state.settings_tab = next;
    let (_, start, _) = SETTINGS_SECTIONS[next];
    state.settings_selected = start;
    state.settings_editing = false;
    state.settings_scroll = 0;
    state.settings_max_scroll = 0;
}

fn active_settings_tab_index(state: &TuiState) -> usize {
    state
        .settings_tab
        .min(SETTINGS_SECTIONS.len().saturating_sub(1))
}

fn active_tab_bounds(state: &TuiState) -> (usize, usize) {
    let tab = active_settings_tab_index(state);
    let (_, start, end) = SETTINGS_SECTIONS[tab];
    (start, end)
}

fn clamp_selected_to_active_tab(state: &mut TuiState) {
    let (start, end) = active_tab_bounds(state);
    state.settings_selected = state.settings_selected.clamp(start, end);
}

fn row_style_for_settings_index(idx: usize, selected: bool) -> Style {
    let base = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        match idx {
            22 => Style::default().fg(Color::Cyan),
            34 => Style::default().fg(Color::Green),
            35 => Style::default().fg(Color::LightRed),
            36 => Style::default().fg(Color::Cyan),
            23 => Style::default().fg(Color::LightYellow),
            24 => Style::default().fg(Color::LightMagenta),
            32 => Style::default().fg(Color::LightCyan),
            _ => Style::default().fg(Color::White),
        }
    };

    if idx >= 26 {
        base.add_modifier(Modifier::BOLD)
    } else {
        base
    }
}

fn section_help_text(active_section: usize) -> &'static str {
    match active_section {
        0 => " Provider, model, connection, API key, and module access.",
        1 => " Runtime behavior, tool limits, and thinking capture.",
        2 => " Verification thresholds for confidence and evidence gating.",
        3 => " Review configuration and key-related safety actions.",
        4 => " Stage routing role, manual provider/model, presets, policy, fallback, and reset.",
        _ => " Revert, apply, close, or open the theme picker.",
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
