use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::{
    ConversationTurn, ObservabilityFilter, TuiState, UiMode, api_key_storage_backend_label,
    centered_rect, command_preview_ui, set_active_ui_theme, settings_ui, ui_accent_primary,
    ui_accent_warn, ui_bg, ui_border, ui_modal_bg, ui_panel_bg,
};
use crate::markdown_cache::render_markdown_lines_cached;

pub(crate) fn render(frame: &mut ratatui::Frame, state: &mut TuiState) {
    set_active_ui_theme(&state.settings.theme_id);
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(ui_bg()).fg(Color::White)),
        area,
    );

    if state.mode == UiMode::Startup {
        render_startup_overlay(frame, state);
        return;
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let content_area = outer[0];
    let input_area = outer[1];

    let left = content_area;

    let conv_title = if state.is_processing {
        " Conversation  ⟳ "
    } else {
        " Conversation "
    };

    let inner_width = left.width.saturating_sub(2);
    let conv_text = build_conversation_text(state, &state.conversation, inner_width);
    let visible_height = left.height.saturating_sub(2);
    let visual_lines = visual_line_count(&conv_text, inner_width);
    let max_scroll = visual_lines.saturating_sub(visible_height);
    state.conv_max_scroll = max_scroll;
    let safe_scroll = if state.auto_scroll {
        max_scroll
    } else {
        state.conv_scroll.min(max_scroll)
    };
    state.conv_scroll = safe_scroll;

    let conv_border = if state.is_processing {
        Style::default().fg(ui_accent_warn())
    } else {
        Style::default().fg(ui_border())
    };

    let conv_widget = Paragraph::new(conv_text)
        .block(
            Block::default()
                .title(conv_title)
                .borders(Borders::ALL)
                .border_style(conv_border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_panel_bg()))
        .wrap(Wrap { trim: false })
        .scroll((safe_scroll, 0));
    frame.render_widget(conv_widget, left);

    let obs_count = state.observability.len();
    let jobs_count = state.job_history.len();
    let drops = state.perf.dropped_events;

    let session_short: String = state.session_id.chars().take(8).collect();
    let thinking_hint = if state.is_processing {
        "  thinking... (F2 peek / Ctrl+T detail)"
    } else if !state.thinking_trace.is_empty() {
        "  [F2 thinking]"
    } else {
        ""
    };
    let input_title = format!(
        " {}  depth:{}  session:{session_short}{}  |  obs:{obs_count} jobs:{jobs_count} drops:{drops}  [Ctrl+O for details] ",
        state.provider_model, state.response_depth_mode, thinking_hint
    );
    let input_display = format!("  {}_", state.input_buffer);
    let input_border = if state.is_processing {
        Style::default().fg(ui_accent_warn())
    } else {
        Style::default().fg(ui_accent_primary())
    };

    let input_widget = Paragraph::new(input_display)
        .block(
            Block::default()
                .title(input_title)
                .borders(Borders::ALL)
                .border_style(input_border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_panel_bg()));
    frame.render_widget(input_widget, input_area);

    if state.mode == UiMode::History {
        render_history_overlay(frame, state);
    } else if state.mode == UiMode::CommandPalette {
        render_command_palette_overlay(frame, state);
    } else if state.mode == UiMode::ThemeMenu {
        super::theme_ui::render_theme_menu_overlay(frame, state);
    } else if state.mode == UiMode::AllowlistPreview {
        render_allowlist_preview_overlay(frame, state);
    } else if state.mode == UiMode::Editor {
        render_editor_overlay(frame, state);
    } else if state.mode == UiMode::RuntimeEnv {
        settings_ui::render_runtime_env_overlay(frame, state);
    } else if state.mode == UiMode::Settings {
        render_settings_overlay(frame, state);
    } else if state.mode == UiMode::ObservabilityPanel {
        render_observability_panel_overlay(frame, state);
    } else if state.mode == UiMode::ThinkingPeek {
        render_thinking_peek_overlay(frame, state);
    } else if state.mode == UiMode::ThinkingPanel {
        render_thinking_panel_overlay(frame, state);
    } else if state.mode == UiMode::GraphemeConsole {
        render_grapheme_console_overlay(frame, state);
    }
}

fn render_startup_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 72, 62);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Welcome to Medousa ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Choose your model, then start.",
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(Span::styled(
        "Up/Down: select  Left/Right: cycle provider  Type/Backspace: edit model  Enter: continue",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let rows = [
        format!("Provider: {}", state.settings_draft.provider),
        format!("Model: {}", state.settings_draft.model),
        "Start".to_string(),
    ];

    for (idx, row) in rows.iter().enumerate() {
        let selected = idx == state.startup_selected;
        let marker = if selected { ">" } else { " " };
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if idx == 2 {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(format!("{marker} {row}"), style)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tip: changing provider sets a sensible model default.",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Need detail? F2 for thinking, Ctrl+O for diagnostics.",
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(Span::styled(
        format!("Secret backend: {}", api_key_storage_backend_label()),
        Style::default().fg(Color::DarkGray),
    )));

    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Get Started ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false });

    frame.render_widget(panel, popup);
}

fn render_grapheme_console_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 90, 82);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Up/Down/Page: scroll  Home/End: jump  Esc/F3: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    if state.grapheme_console.is_empty() {
        lines.push(Line::from(Span::styled(
            "No console output yet. Run /run or /run-current to capture results.",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for (idx, entry) in state.grapheme_console.iter().enumerate() {
            if idx > 0 {
                lines.push(Line::from(Span::styled(
                    "",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            for line in render_markdown_lines_cached(state, entry, popup.width.saturating_sub(2)) {
                lines.push(line);
            }
        }
    }

    let text = Text::from(lines);
    let inner_width = popup.width.saturating_sub(2);
    let visible_height = popup.height.saturating_sub(2);
    let visual_lines = visual_line_count(&text, inner_width);
    let max_scroll = visual_lines.saturating_sub(visible_height);
    state.grapheme_console_max_scroll = max_scroll;
    state.grapheme_console_scroll = state.grapheme_console_scroll.min(max_scroll);

    let panel = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Grapheme Console ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.grapheme_console_scroll, 0));
    frame.render_widget(panel, popup);
}

fn build_observability_text(state: &TuiState, expanded: bool, width: u16) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        format!(
            " Diagnostics are redacted | Secure storage: {} ",
            api_key_storage_backend_label()
        ),
        Style::default().fg(Color::Cyan),
    )));
    let settings_queue_depth = usize::from(state.pending_settings_apply.is_some());
    lines.push(Line::from(Span::styled(
        format!(
            " Perf: input->paint={}ms | frame={}ms | settings_q={} | worker_q={}/{} | coalesced(chunk/key)={}/{} | dropped={} ",
            state.perf.last_input_to_paint_ms,
            state.perf.last_frame_render_ms,
            settings_queue_depth,
            state.perf.worker_queue_depth,
            state.perf.worker_queue_peak,
            state.perf.coalesced_agent_chunks,
            state.perf.coalesced_key_events,
            state.perf.dropped_events
        ),
        Style::default().fg(Color::LightCyan),
    )));
    lines.push(Line::from(""));

    if expanded {
        lines.push(Line::from(Span::styled(
            " Up/Down/Page: scroll  Home/End: jump  Esc/Ctrl+O: close ",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    let filter_label = match state.observability_filter {
        ObservabilityFilter::All => "all",
        ObservabilityFilter::ReceiptsOnly => "receipts",
        ObservabilityFilter::ArtifactsOnly => "artifacts",
    };
    let artifact_stats = medousa::artifact_store::artifact_index_stats(&state.session_id);
    lines.push(Line::from(Span::styled(
        format!(
            " Filter: {filter_label} | artifacts(records={}, unique={}, bytes={}) ",
            artifact_stats.records, artifact_stats.unique_hashes, artifact_stats.total_bytes
        ),
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(""));

    let filtered_events: Vec<_> = state
        .observability
        .iter()
        .filter(|ev| match state.observability_filter {
            ObservabilityFilter::All => true,
            ObservabilityFilter::ReceiptsOnly => ev.text.contains("◈ receipt "),
            ObservabilityFilter::ArtifactsOnly => {
                ev.text.contains("◈ artifact ")
                    || ev.text.contains("◈ chunk refs ")
                    || ev.text.contains("◈ verification ")
                    || ev.text.contains("◈ context pack verification")
            }
        })
        .collect();

    if filtered_events.is_empty() {
        lines.push(Line::from(Span::styled(
            match state.observability_filter {
                ObservabilityFilter::All => "No diagnostics yet.",
                ObservabilityFilter::ReceiptsOnly => "No receipt diagnostics yet.",
                ObservabilityFilter::ArtifactsOnly => "No artifact diagnostics yet.",
            },
            Style::default().fg(Color::Gray),
        )));
        return Text::from(lines);
    }

    for (idx, ev) in filtered_events.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::from(Span::styled(
                "",
                Style::default().fg(Color::DarkGray),
            )));
        }
        for line in render_markdown_lines_cached(state, &ev.text, width) {
            lines.push(line);
        }
    }

    Text::from(lines)
}

fn build_job_history_text(state: &TuiState, width: u16) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if state.job_history.is_empty() {
        lines.push(Line::from(Span::styled(
            "No jobs yet.",
            Style::default().fg(Color::Gray),
        )));
        return Text::from(lines);
    }

    for (idx, j) in state.job_history.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::from(""));
        }

        let symbol = match j.status.as_str() {
            "succeeded" => "✓",
            "failed" => "✗",
            _ => "·",
        };
        let type_label = j.job_type.split('.').last().unwrap_or(&j.job_type);
        let id_short: String = j.job_id.chars().take(12).collect();
        let summary = format!("{symbol} {type_label}  {id_short}  [{}]", j.status);
        lines.extend(render_markdown_lines_cached(state, &summary, width));
    }

    Text::from(lines)
}

fn render_observability_panel_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 90, 82);
    frame.render_widget(Clear, popup);

    let inner_width = popup.width.saturating_sub(2);
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Up/Down/Page: scroll  Home/End: jump  R: receipt filter  Esc/Ctrl+O: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        " Diagnostics ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    for line in build_observability_text(state, false, inner_width).lines {
        lines.push(line);
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Recent Jobs ",
        Style::default()
            .fg(ui_accent_primary())
            .add_modifier(Modifier::BOLD),
    )));
    for line in build_job_history_text(state, inner_width).lines {
        lines.push(line);
    }

    let text = Text::from(lines);
    let visible_height = popup.height.saturating_sub(2);
    let visual_lines = visual_line_count(&text, inner_width);
    let max_scroll = visual_lines.saturating_sub(visible_height);
    state.obs_max_scroll = max_scroll;
    state.obs_scroll = state.obs_scroll.min(max_scroll);

    let panel = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Awareness Detail ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.obs_scroll, 0));
    frame.render_widget(panel, popup);
}

fn render_thinking_peek_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 62, 38);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Esc/F2: close  Enter/Down: open detail ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    if state.thinking_trace.is_empty() {
        lines.push(Line::from(Span::styled(
            if state.is_processing {
                "Thinking is active. Waiting for updates..."
            } else {
                "No thinking updates in this run."
            },
            Style::default().fg(Color::Gray),
        )));
    } else {
        for item in state.thinking_trace.iter().take(8).rev() {
            lines.push(Line::from(Span::styled(
                item.clone(),
                Style::default().fg(Color::Cyan),
            )));
        }
    }

    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Thinking Peek ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, popup);
}

fn render_thinking_panel_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 86, 78);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Up/Down/Page: scroll  Home/End: jump  Esc/Ctrl+T: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    if state.thinking_trace.is_empty() {
        lines.push(Line::from(Span::styled(
            "No thinking details yet.",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for item in state.thinking_trace.iter().rev() {
            lines.push(Line::from(Span::styled(
                item.clone(),
                Style::default().fg(Color::Cyan),
            )));
        }
    }

    let text = Text::from(lines);
    let inner_width = popup.width.saturating_sub(2);
    let visible_height = popup.height.saturating_sub(2);
    let visual_lines = visual_line_count(&text, inner_width);
    let max_scroll = visual_lines.saturating_sub(visible_height);
    state.thinking_max_scroll = max_scroll;
    state.thinking_scroll = state.thinking_scroll.min(max_scroll);

    let panel = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Thinking ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.thinking_scroll, 0));
    frame.render_widget(panel, popup);
}

fn render_history_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 80, 70);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Up/Down: move  PgUp/PgDn/Wheel: scroll  Home/End: jump  V: trust detail  Enter: open session  Esc: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let mut selected_line: Option<usize> = None;

    if state.history_items.is_empty() {
        lines.push(Line::from(Span::styled(
            "No saved sessions yet.",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for (idx, item) in state.history_items.iter().enumerate() {
            if idx == state.history_selected {
                selected_line = Some(lines.len());
            }

            let marker = if idx == state.history_selected {
                ">"
            } else {
                " "
            };
            let ts = item
                .last_timestamp
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "-".to_string());
            let verification_ts = item
                .last_verification_timestamp
                .map(|t| t.format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "-".to_string());
            let id_short: String = item.session_id.chars().take(8).collect();
            let trust = item
                .last_verification_confidence
                .map(|confidence| {
                    let level = if confidence >= 0.80 {
                        "H"
                    } else if confidence >= 0.60 {
                        "M"
                    } else {
                        "L"
                    };
                    format!("{level}:{confidence:.2}")
                })
                .unwrap_or_else(|| "-".to_string());
            let line = format!(
                "{marker} {id_short}  {ts}  turn={} ver={} trust={} last_verify={}  {}",
                item.turns, item.verification_runs, trust, verification_ts, item.preview
            );

            let style = if idx == state.history_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(line, style)));
        }

        if state.history_show_verification_detail {
            if let Some(selected) = state.history_items.get(state.history_selected) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    " Verification Signals ",
                    Style::default()
                        .fg(ui_accent_primary())
                        .add_modifier(Modifier::BOLD),
                )));

                match (
                    selected.last_verification_confidence,
                    selected.last_verification_coverage,
                    selected.last_verification_verified,
                ) {
                    (Some(confidence), Some(coverage), Some(verified)) => {
                        let trust_label = if confidence >= 0.80 {
                            "high"
                        } else if confidence >= 0.60 {
                            "medium"
                        } else {
                            "low"
                        };
                        let status = if verified { "verified" } else { "failed" };
                        let status_style = if verified {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Red)
                        };
                        lines.push(Line::from(vec![
                            Span::styled(" status=", Style::default().fg(Color::Gray)),
                            Span::styled(status, status_style),
                            Span::styled(
                                format!(
                                    "  confidence={confidence:.2} ({trust_label})  citation_coverage={coverage:.2}"
                                ),
                                Style::default().fg(Color::White),
                            ),
                        ]));
                    }
                    _ => {
                        lines.push(Line::from(Span::styled(
                            " no verification metrics available for selected session",
                            Style::default().fg(Color::Gray),
                        )));
                    }
                }
            }
        }
    }

    let text = Text::from(lines);
    let inner_width = popup.width.saturating_sub(2);
    let visible_height = popup.height.saturating_sub(2);
    let visual_lines = visual_line_count(&text, inner_width);
    state.history_max_scroll = visual_lines.saturating_sub(visible_height);
    state.history_scroll = state.history_scroll.min(state.history_max_scroll);

    if let Some(line_idx) = selected_line {
        let visible_rows = visible_height as usize;
        if visible_rows > 0 {
            let top = state.history_scroll as usize;
            let bottom = top.saturating_add(visible_rows.saturating_sub(1));
            if line_idx < top {
                state.history_scroll = line_idx as u16;
            } else if line_idx > bottom {
                state.history_scroll =
                    line_idx.saturating_add(1).saturating_sub(visible_rows) as u16;
            }
            state.history_scroll = state.history_scroll.min(state.history_max_scroll);
        }
    }

    let panel = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Sessions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.history_scroll, 0));
    frame.render_widget(panel, popup);
}

fn render_command_palette_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    command_preview_ui::render_command_palette_overlay(frame, state)
}

fn render_allowlist_preview_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    command_preview_ui::render_allowlist_preview_overlay(frame, state)
}

fn render_settings_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    settings_ui::render_settings_overlay(frame, state)
}

fn render_editor_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 90, 80);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();
    let (line, col) = state.editor_buffer.line_col();
    let dirty_marker = if state.editor_dirty { "*" } else { "" };
    lines.push(Line::from(Span::styled(
        " Type to edit  Enter: new line  Up/Down: keep column  Ctrl+S: save  /save [path]: save  /run [path]: run  Esc: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            " File{dirty_marker}: {} | Cursor: {line}:{col} | {} ",
            state
                .editor_file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(unspecified)".to_string()),
            state.editor_status
        ),
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(""));

    let content_height = popup.height.saturating_sub(5) as usize;
    let total_lines = state.editor_buffer.line_count();
    let start = state.editor_scroll as usize;
    let end = start.saturating_add(content_height).min(total_lines.max(1));

    for idx in start..end {
        let src_line = state.editor_buffer.line_at(idx).unwrap_or("");
        if idx + 1 == line {
            let cursor_index = col.saturating_sub(1);
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::styled(
                format!("{:>4}  ", idx + 1),
                Style::default().fg(Color::DarkGray),
            ));

            let mut chars = src_line.chars().collect::<Vec<_>>();
            if chars.is_empty() {
                spans.push(Span::styled(
                    " ",
                    Style::default().bg(Color::White).fg(Color::Black),
                ));
            } else if cursor_index >= chars.len() {
                let body = chars.drain(..).collect::<String>();
                spans.push(Span::styled(body, Style::default().fg(Color::White)));
                spans.push(Span::styled(
                    " ",
                    Style::default().bg(Color::White).fg(Color::Black),
                ));
            } else {
                let before = chars.iter().take(cursor_index).collect::<String>();
                let current = chars[cursor_index].to_string();
                let after = chars.iter().skip(cursor_index + 1).collect::<String>();
                spans.push(Span::styled(before, Style::default().fg(Color::White)));
                spans.push(Span::styled(
                    current,
                    Style::default().bg(Color::White).fg(Color::Black),
                ));
                spans.push(Span::styled(after, Style::default().fg(Color::White)));
            }

            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(Span::styled(
                format!("{:>4}  {}", idx + 1, src_line),
                Style::default().fg(Color::White),
            )));
        }
    }

    if lines.len() <= 3 {
        lines.push(Line::from(Span::styled(
            "(empty buffer)",
            Style::default().fg(Color::Gray),
        )));
    }

    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Script Editor ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, popup);
}

fn build_conversation_text(
    state: &TuiState,
    turns: &[ConversationTurn],
    width: u16,
) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    for turn in turns {
        match turn.role.as_str() {
            "user" => {
                lines.push(Line::from(Span::styled(
                    "  you".to_string(),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            _ => {
                lines.push(Line::from(Span::styled(
                    "  ◈".to_string(),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                )));
            }
        }

        if turn.role == "user" {
            for content_line in turn.content.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {content_line}"),
                    Style::default().fg(Color::White),
                )));
            }
        } else {
            let (legacy_answer_state, content_body) = split_answer_state_prefix(&turn.content);
            let answer_state = turn.answer_state.as_deref().or(legacy_answer_state);
            if let Some(answer_state) = answer_state {
                let (label, color) = match answer_state {
                    "verified" => ("verified", Color::Green),
                    "provisional" => ("provisional", Color::Yellow),
                    _ => (answer_state, Color::Gray),
                };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("[{label}]"),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
            lines.extend(render_markdown_lines_cached(state, content_body, width));
        }

        if !turn.tool_names.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  [{}]", turn.tool_names.join(", ")),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.push(Line::from(""));
    }

    Text::from(lines)
}

fn split_answer_state_prefix(content: &str) -> (Option<&str>, &str) {
    let Some(rest) = content.strip_prefix("◈ answer_state=") else {
        return (None, content);
    };

    let Some((state, remainder)) = rest.split_once('\n') else {
        return (Some(rest.trim()), "");
    };

    (Some(state.trim()), remainder)
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
