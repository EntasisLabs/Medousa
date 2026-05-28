use super::*;

pub(crate) fn open_theme_menu(state: &mut TuiState, return_mode: UiMode) {
    state.theme_menu_return_mode = return_mode;
    state.theme_menu_original_theme_id = state.settings.theme_id.clone();
    state.theme_menu_original_draft_theme_id = state.settings_draft.theme_id.clone();

    let ids = ui_theme_ids();
    let preferred = if return_mode == UiMode::Settings {
        state.settings_draft.theme_id.as_str()
    } else {
        state.settings.theme_id.as_str()
    };

    state.theme_menu_selected = ids
        .iter()
        .position(|id| id.eq_ignore_ascii_case(preferred))
        .unwrap_or(0);
    state.theme_menu_scroll = 0;
    state.theme_menu_max_scroll = 0;

    if let Some(selected_id) = ids.get(state.theme_menu_selected) {
        state.settings.theme_id = (*selected_id).to_string();
    }

    state.mode = UiMode::ThemeMenu;
}

pub(crate) fn handle_theme_menu_key_event(code: KeyCode, state: &mut TuiState) -> EventOutcome {
    let ids = ui_theme_ids();
    if ids.is_empty() {
        state.mode = state.theme_menu_return_mode;
        return EventOutcome::Continue;
    }

    match code {
        KeyCode::Up => {
            state.theme_menu_selected = state.theme_menu_selected.saturating_sub(1);
            preview_selected_theme(state);
        }
        KeyCode::Down => {
            state.theme_menu_selected = (state.theme_menu_selected + 1).min(ids.len() - 1);
            preview_selected_theme(state);
        }
        KeyCode::Home => {
            state.theme_menu_selected = 0;
            preview_selected_theme(state);
        }
        KeyCode::End => {
            state.theme_menu_selected = ids.len() - 1;
            preview_selected_theme(state);
        }
        KeyCode::PageUp => {
            state.theme_menu_selected = state.theme_menu_selected.saturating_sub(3);
            preview_selected_theme(state);
        }
        KeyCode::PageDown => {
            state.theme_menu_selected = (state.theme_menu_selected + 3).min(ids.len() - 1);
            preview_selected_theme(state);
        }
        KeyCode::Enter | KeyCode::Char('a') | KeyCode::Char('A') => {
            apply_selected_theme(state);
            state.mode = state.theme_menu_return_mode;
        }
        KeyCode::Esc => {
            state.settings.theme_id = state.theme_menu_original_theme_id.clone();
            state.settings_draft.theme_id = state.theme_menu_original_draft_theme_id.clone();
            state.mode = state.theme_menu_return_mode;
        }
        _ => {}
    }

    EventOutcome::Continue
}

pub(crate) fn render_theme_menu_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 82, 74);
    frame.render_widget(Clear, popup);

    let panel = Block::default()
        .title(" Theme Menu ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ui_accent_primary()))
        .style(Style::default().bg(ui_modal_bg()));
    frame.render_widget(panel.clone(), popup);
    let inner = panel.inner(popup);

    let columns = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(58),
            ratatui::layout::Constraint::Percentage(42),
        ])
        .split(inner);

    let left = columns[0];
    let right = columns[1];

    let ids = ui_theme_ids();
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Browse themes and preview instantly ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        " Up/Down: select  Enter/A: apply  Esc: cancel ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let mut selected_line: Option<usize> = None;
    for (idx, id) in ids.iter().enumerate() {
        if idx == state.theme_menu_selected {
            selected_line = Some(lines.len());
        }
        let marker = if idx == state.theme_menu_selected {
            ">"
        } else {
            " "
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {}  ({id})", ui_theme_display_name(id)),
            if idx == state.theme_menu_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        )));
    }

    let text = Text::from(lines);
    let inner_width = left.width.saturating_sub(1);
    let visible_height = left.height;
    let visual_lines = visual_line_count(&text, inner_width);
    state.theme_menu_max_scroll = visual_lines.saturating_sub(visible_height);

    if let Some(line_idx) = selected_line {
        let visible_rows = visible_height as usize;
        if visible_rows > 0 {
            let top = state.theme_menu_scroll as usize;
            let bottom = top.saturating_add(visible_rows.saturating_sub(1));
            if line_idx < top {
                state.theme_menu_scroll = line_idx as u16;
            } else if line_idx > bottom {
                state.theme_menu_scroll =
                    line_idx.saturating_add(1).saturating_sub(visible_rows) as u16;
            }
            state.theme_menu_scroll = state.theme_menu_scroll.min(state.theme_menu_max_scroll);
        }
    }

    let left_panel = Paragraph::new(text)
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.theme_menu_scroll, 0));
    frame.render_widget(left_panel, left);

    let selected_id = ids
        .get(state.theme_menu_selected)
        .copied()
        .unwrap_or("medousa-default");
    let mut rail: Vec<Line> = Vec::new();
    rail.push(Line::from(Span::styled(
        " Preview ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    rail.push(Line::from(""));
    rail.push(Line::from(format!(
        "Theme: {}",
        ui_theme_display_name(selected_id)
    )));
    rail.push(Line::from(format!("ID: {selected_id}")));
    rail.push(Line::from(""));
    rail.push(Line::from(Span::styled(
        "Primary Accent",
        Style::default().fg(ui_accent_primary()),
    )));
    rail.push(Line::from(Span::styled(
        "Warning Accent",
        Style::default().fg(ui_accent_warn()),
    )));
    rail.push(Line::from(Span::styled(
        "Panel Border",
        Style::default().fg(ui_border()),
    )));
    rail.push(Line::from(""));
    rail.push(Line::from(Span::styled(
        "Apply saves to defaults immediately.",
        Style::default().fg(Color::DarkGray),
    )));

    let right_panel = Paragraph::new(Text::from(rail))
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(ui_border())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false });
    frame.render_widget(right_panel, right);
}

fn preview_selected_theme(state: &mut TuiState) {
    if let Some(selected_id) = ui_theme_ids().get(state.theme_menu_selected) {
        state.settings.theme_id = (*selected_id).to_string();
    }
}

fn apply_selected_theme(state: &mut TuiState) {
    if let Some(selected_id) = ui_theme_ids().get(state.theme_menu_selected) {
        let selected = (*selected_id).to_string();
        state.settings.theme_id = selected.clone();
        state.settings_draft.theme_id = selected.clone();

        let mut defaults = load_tui_defaults();
        defaults.theme_id = Some(selected.clone());
        save_tui_defaults(&defaults);

        push_obs(
            state,
            format!(
                "✓ theme applied: {} ({selected})",
                ui_theme_display_name(&selected)
            ),
        );
    }
}

fn visual_line_count(text: &Text, inner_width: u16) -> u16 {
    let width = inner_width.max(1) as usize;
    let mut total = 0usize;

    for line in &text.lines {
        let chars = line.width();
        if chars == 0 {
            total += 1;
        } else {
            total += (chars.saturating_sub(1) / width) + 1;
        }
    }

    total.min(u16::MAX as usize) as u16
}
