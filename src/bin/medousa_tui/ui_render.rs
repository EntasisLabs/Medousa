use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use medousa::tui::workspace::{ShellTabKind, SplitBranchDirection, SplitNode};

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

    render_workspace_panes(frame, state, content_area);

    let obs_count = state.observability.len();
    let jobs_count = state.job_history.len();
    let drops = state.perf.dropped_events;
    let pane_n = state.workspace.pane_count();
    let desk_n = state.workspace.desktops.len();
    let desk_idx = state
        .workspace
        .desktops
        .iter()
        .position(|d| d.id == state.workspace.active_desktop_id)
        .unwrap_or(0)
        + 1;

    let session_short = medousa::session::format_session_history_label(
        &state.session_id,
        state.session_display_name.as_deref(),
    );
    let thinking_hint = if state.is_processing {
        "  thinking... (F2 peek / Ctrl+T detail)"
    } else if !state.thinking_trace.is_empty() {
        "  [F2 thinking]"
    } else {
        ""
    };
    let prefix_hint = if state.prefix_active {
        "  PREFIX % \" hjkl z x o"
    } else {
        "  [Ctrl+; panes]"
    };
    let active_kind = state.workspace.active_tab().map(|t| t.kind()).or(
        match state.mode {
            UiMode::Notes => Some(ShellTabKind::Notes),
            UiMode::Code => Some(ShellTabKind::Code),
            UiMode::Review => Some(ShellTabKind::Review),
            UiMode::Terminal => Some(ShellTabKind::Terminal),
            _ => None,
        },
    );
    let input_title = match active_kind {
        Some(ShellTabKind::Notes) => {
            let conflict = state
                .workspace
                .active_tab()
                .and_then(|t| t.notes_path())
                .and_then(|p| state.note_buffers.get(p))
                .is_some_and(|n| n.conflict);
            if conflict {
                format!(
                    " Note CONFLICT  Ctrl+R reload · Ctrl+Y keep mine · Ctrl+S retry  panes:{pane_n}{prefix_hint}  |  obs:{obs_count} "
                )
            } else {
                format!(
                    " Note edit  Ctrl+S save · Ctrl+A ask · Ctrl+; o library  panes:{pane_n} desk:{desk_idx}/{desk_n}{prefix_hint}  |  obs:{obs_count} "
                )
            }
        }
        Some(ShellTabKind::Code) => format!(
            " Code  Tab tree/buffer · Enter open · Ctrl+S save · Ctrl+E seal · Ctrl+R review  panes:{pane_n}{prefix_hint}  |  obs:{obs_count} "
        ),
        Some(ShellTabKind::Review) => format!(
            " Review  [] files · a approve · f finish · u restore · c code  panes:{pane_n}{prefix_hint}  |  obs:{obs_count} "
        ),
        Some(ShellTabKind::Terminal) => format!(
            " Terminal  keys→PTY · Ctrl+C interrupt · Ctrl+Q quit · Ctrl+; t new  panes:{pane_n}{prefix_hint}  |  obs:{obs_count} "
        ),
        _ => format!(
            " {}  depth:{}  conn:{}  session:{session_short}  panes:{pane_n} desk:{desk_idx}/{desk_n}{}{}  |  obs:{obs_count} jobs:{jobs_count} drops:{drops}  [Ctrl+O] ",
            state.provider_model,
            state.response_depth_mode,
            state.workshop_label,
            thinking_hint,
            prefix_hint
        ),
    };
    let input_display = match active_kind {
        Some(ShellTabKind::Notes) => {
            let path = state
                .workspace
                .active_tab()
                .and_then(|t| t.notes_path())
                .unwrap_or("");
            let note = state.note_buffers.get(path);
            let dirty = note.map(|n| n.dirty).unwrap_or(false);
            let conflict = note.map(|n| n.conflict).unwrap_or(false);
            format!(
                "  {}{}{}",
                path,
                if dirty { " *" } else { "" },
                if conflict { " ⚠ conflict" } else { "" }
            )
        }
        Some(ShellTabKind::Code) => {
            let work_id = state
                .workspace
                .active_tab()
                .and_then(|t| t.code_work_id())
                .unwrap_or("");
            let ws = state.code_workspaces.get(work_id);
            let path = ws.and_then(|w| w.open_path.as_deref()).unwrap_or("(tree)");
            let dirty = ws.map(|w| w.dirty).unwrap_or(false);
            format!("  {}{}", path, if dirty { " *" } else { "" })
        }
        Some(ShellTabKind::Review) => {
            let work_id = state
                .workspace
                .active_tab()
                .and_then(|t| t.review_work_id())
                .unwrap_or("");
            let review = state.review_workspaces.get(work_id);
            let file = review
                .and_then(|r| r.files.get(r.file_selected))
                .map(|f| f.path.as_str())
                .unwrap_or("(no files)");
            format!("  {file}")
        }
        Some(ShellTabKind::Terminal) => {
            let sid = state
                .workspace
                .active_tab()
                .and_then(|t| t.terminal_session_id())
                .unwrap_or("");
            let pane = state.terminal_panes.get(sid);
            let status = pane.map(|p| p.status.as_str()).unwrap_or("missing");
            let geom = pane
                .map(|p| format!("{}x{}", p.cols, p.rows))
                .unwrap_or_else(|| "?x?".to_string());
            format!("  {sid}  [{status}]  {geom}")
        }
        _ => format!("  {}_", state.input_buffer),
    };
    let input_border = if state.prefix_active {
        Style::default().fg(Color::Yellow)
    } else if state.is_processing {
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

    if state.mode == UiMode::NotesPicker {
        render_notes_picker_overlay(frame, state);
    } else if state.mode == UiMode::ForgePicker {
        render_forge_picker_overlay(frame, state);
    } else if state.mode == UiMode::TerminalPicker {
        render_terminal_picker_overlay(frame, state);
    } else if state.mode == UiMode::ConnectionPicker {
        render_connection_picker_overlay(frame, state);
    } else if state.mode == UiMode::History {
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

fn render_workspace_panes(frame: &mut ratatui::Frame, state: &mut TuiState, area: Rect) {
    let layout = state.workspace.layout();
    let zoomed = layout.zoomed_group_id.clone();
    let active = layout.active_group_id.clone();
    let root = layout.split_root.clone();

    if let Some(zoom_id) = zoomed {
        render_group_pane(frame, state, area, &zoom_id, true);
        return;
    }

    let mut regions: Vec<(String, Rect)> = Vec::new();
    collect_pane_regions(&root, area, &mut regions);
    for (group_id, rect) in regions {
        let focused = group_id == active;
        render_group_pane(frame, state, rect, &group_id, focused);
    }
}

fn render_group_pane(
    frame: &mut ratatui::Frame,
    state: &mut TuiState,
    area: Rect,
    group_id: &str,
    focused: bool,
) {
    let kind = state
        .workspace
        .group_active_tab(group_id)
        .map(|t| t.kind())
        .unwrap_or(ShellTabKind::Chat);
    match kind {
        ShellTabKind::Notes => render_notes_pane(frame, state, area, group_id, focused),
        ShellTabKind::Code => render_code_pane(frame, state, area, group_id, focused),
        ShellTabKind::Review => render_review_pane(frame, state, area, group_id, focused),
        ShellTabKind::Terminal => render_terminal_pane(frame, state, area, group_id, focused),
        _ => render_chat_pane(frame, state, area, group_id, focused),
    }
}

fn collect_pane_regions(node: &SplitNode, area: Rect, out: &mut Vec<(String, Rect)>) {
    match node {
        SplitNode::Group { id } => out.push((id.clone(), area)),
        SplitNode::Branch {
            direction,
            ratio,
            a,
            b,
            ..
        } => {
            let pct_a = ((*ratio) * 100.0).round().clamp(20.0, 80.0) as u16;
            let pct_b = 100u16.saturating_sub(pct_a);
            let dir = match direction {
                SplitBranchDirection::Row => Direction::Vertical,
                SplitBranchDirection::Column => Direction::Horizontal,
            };
            let chunks = Layout::default()
                .direction(dir)
                .constraints([
                    Constraint::Percentage(pct_a),
                    Constraint::Percentage(pct_b),
                ])
                .split(area);
            collect_pane_regions(a, chunks[0], out);
            collect_pane_regions(b, chunks[1], out);
        }
    }
}

fn render_chat_pane(
    frame: &mut ratatui::Frame,
    state: &mut TuiState,
    area: Rect,
    group_id: &str,
    focused: bool,
) {
    let tab_title = state
        .workspace
        .group_active_tab(group_id)
        .map(|t| t.title().to_string())
        .unwrap_or_else(|| "Chat".to_string());
    let session_id = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.chat_session_id().map(str::to_string))
        .unwrap_or_else(|| state.session_id.clone());

    let processing = super::workspace_runtime::lane_is_processing(state, &session_id);
    let title = if processing {
        format!(" {tab_title}  ⟳ ")
    } else if focused {
        format!(" {tab_title}  ● ")
    } else {
        format!(" {tab_title} ")
    };

    let inner_width = area.width.saturating_sub(2);
    // Unfocused panes clone a snapshot so we can still borrow `state` mutably for scroll.
    let unfocused_turns = if focused {
        None
    } else {
        Some(
            super::workspace_runtime::lane_conversation(state, &session_id)
                .map(|slice| slice.to_vec())
                .unwrap_or_default(),
        )
    };
    let conv_text = match unfocused_turns.as_deref() {
        Some(turns) => build_conversation_text(state, turns, inner_width),
        None => build_conversation_text(state, &state.conversation, inner_width),
    };
    let visible_height = area.height.saturating_sub(2);
    let visual_lines = visual_line_count(&conv_text, inner_width);
    let max_scroll = visual_lines.saturating_sub(visible_height);

    let safe_scroll = if focused {
        state.conv_max_scroll = max_scroll;
        let scroll = if state.auto_scroll {
            max_scroll
        } else {
            state.conv_scroll.min(max_scroll)
        };
        state.conv_scroll = scroll;
        scroll
    } else {
        state
            .chat_lanes
            .get(&session_id)
            .map(|lane| {
                if lane.auto_scroll {
                    max_scroll
                } else {
                    lane.conv_scroll.min(max_scroll)
                }
            })
            .unwrap_or(max_scroll)
    };

    let conv_border = if processing {
        Style::default().fg(ui_accent_warn())
    } else if focused {
        Style::default().fg(ui_accent_primary())
    } else {
        Style::default().fg(ui_border())
    };

    let conv_widget = Paragraph::new(conv_text)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(conv_border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_panel_bg()))
        .wrap(Wrap { trim: false })
        .scroll((safe_scroll, 0));
    frame.render_widget(conv_widget, area);
}

fn render_notes_pane(
    frame: &mut ratatui::Frame,
    state: &mut TuiState,
    area: Rect,
    group_id: &str,
    focused: bool,
) {
    let path = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.notes_path().map(str::to_string))
        .unwrap_or_default();
    let (title, body, status, dirty, conflict, scroll) = state
        .note_buffers
        .get(&path)
        .map(|n| {
            (
                n.title.clone(),
                n.buffer.as_text().to_string(),
                n.status.clone(),
                n.dirty,
                n.conflict,
                n.scroll,
            )
        })
        .unwrap_or_else(|| {
            (
                path.clone(),
                String::from("(note not loaded)"),
                String::new(),
                false,
                false,
                0,
            )
        });
    let dirty_mark = if dirty { "*" } else { "" };
    let conflict_mark = if conflict { " ⚠" } else { "" };
    let focus_mark = if focused { " ●" } else { "" };
    let block_title = format!(" Note {title}{dirty_mark}{conflict_mark}{focus_mark}  {status} ");
    let border = if conflict && focused {
        Style::default().fg(ui_accent_warn())
    } else if focused {
        Style::default().fg(ui_accent_primary())
    } else {
        Style::default().fg(ui_border())
    };
    let widget = Paragraph::new(body)
        .block(
            Block::default()
                .title(block_title)
                .borders(Borders::ALL)
                .border_style(border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_panel_bg()))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(widget, area);
}

fn render_code_pane(
    frame: &mut ratatui::Frame,
    state: &mut TuiState,
    area: Rect,
    group_id: &str,
    focused: bool,
) {
    use super::forge_runtime::CodeFocus;
    let work_id = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.code_work_id().map(str::to_string))
        .unwrap_or_default();
    let Some(ws) = state.code_workspaces.get(&work_id) else {
        let widget = Paragraph::new("(code workspace not loaded — Ctrl+; f)")
            .block(
                Block::default()
                    .title(" Code ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ui_border()))
                    .style(Style::default().bg(ui_panel_bg())),
            );
        frame.render_widget(widget, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    let mut tree_lines: Vec<Line> = Vec::new();
    for (idx, path) in ws.tree.iter().enumerate() {
        let selected = idx == ws.tree_selected;
        let marker = if selected { ">" } else { " " };
        let style = if selected && ws.focus == CodeFocus::Tree && focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        tree_lines.push(Line::from(Span::styled(
            format!("{marker} {path}"),
            style,
        )));
    }
    if tree_lines.is_empty() {
        tree_lines.push(Line::from(Span::styled(
            "(empty tree)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let tree_border = if focused && ws.focus == CodeFocus::Tree {
        Style::default().fg(ui_accent_primary())
    } else {
        Style::default().fg(ui_border())
    };
    let tree = Paragraph::new(Text::from(tree_lines))
        .block(
            Block::default()
                .title(format!(" {} files ", ws.title))
                .borders(Borders::ALL)
                .border_style(tree_border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .scroll((ws.tree_scroll, 0));
    frame.render_widget(tree, chunks[0]);

    let dirty = if ws.dirty { "*" } else { "" };
    let focus_mark = if focused { " ●" } else { "" };
    let path = ws.open_path.as_deref().unwrap_or("(no file)");
    let body = if ws.open_path.is_some() {
        ws.buffer.as_text().to_string()
    } else {
        "Select a file and press Enter".to_string()
    };
    let buf_border = if focused && ws.focus == CodeFocus::Buffer {
        Style::default().fg(ui_accent_primary())
    } else {
        Style::default().fg(ui_border())
    };
    let editor = Paragraph::new(body)
        .block(
            Block::default()
                .title(format!(" {path}{dirty}{focus_mark}  {} ", ws.status))
                .borders(Borders::ALL)
                .border_style(buf_border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .wrap(Wrap { trim: false })
        .scroll((ws.scroll, 0));
    frame.render_widget(editor, chunks[1]);
}

fn render_review_pane(
    frame: &mut ratatui::Frame,
    state: &mut TuiState,
    area: Rect,
    group_id: &str,
    focused: bool,
) {
    let work_id = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.review_work_id().map(str::to_string))
        .unwrap_or_default();
    let Some(review) = state.review_workspaces.get(&work_id) else {
        let widget = Paragraph::new("(review not loaded — Ctrl+; r)")
            .block(
                Block::default()
                    .title(" Review ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ui_border()))
                    .style(Style::default().bg(ui_panel_bg())),
            );
        frame.render_widget(widget, area);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            "{} · {} · {}",
            review.title, review.human_phase, review.status
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    if !review.synthesis_summary.is_empty() {
        lines.push(Line::from(Span::styled(
            review.synthesis_summary.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    let actions = format!(
        "approve:{}  finish:{}  files:{}/{}",
        if review.can_review { "yes" } else { "no" },
        if review.can_apply { "yes" } else { "no" },
        review.file_selected.saturating_add(1).min(review.files.len().max(1)),
        review.files.len()
    );
    lines.push(Line::from(Span::styled(
        actions,
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    if let Some(file) = review.files.get(review.file_selected) {
        lines.push(Line::from(Span::styled(
            format!(
                "{}  (+{}/-{})  {}",
                file.path, file.additions, file.deletions, file.status
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        for row in &file.lines {
            let style = if row.starts_with('+') && !row.starts_with("──") {
                Style::default().fg(Color::Green)
            } else if row.starts_with('-') && !row.starts_with("──") {
                Style::default().fg(Color::Red)
            } else if row.starts_with("──") {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(row.clone(), style)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "(no changed files in this review)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let border = if focused {
        Style::default().fg(ui_accent_primary())
    } else {
        Style::default().fg(ui_border())
    };
    let widget = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(if focused {
                    " Review  ● "
                } else {
                    " Review "
                })
                .borders(Borders::ALL)
                .border_style(border)
                .style(Style::default().bg(ui_panel_bg())),
        )
        .wrap(Wrap { trim: false })
        .scroll((review.scroll, 0));
    frame.render_widget(widget, area);
}

fn render_terminal_pane(
    frame: &mut ratatui::Frame,
    state: &mut TuiState,
    area: Rect,
    group_id: &str,
    focused: bool,
) {
    let session_id = state
        .workspace
        .group_active_tab(group_id)
        .and_then(|t| t.terminal_session_id().map(str::to_string))
        .unwrap_or_default();
    let title = state
        .workspace
        .group_active_tab(group_id)
        .map(|t| t.title().to_string())
        .unwrap_or_else(|| "Terminal".to_string());

    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    if !session_id.is_empty() && inner.width > 0 && inner.height > 0 {
        super::terminal_runtime::ensure_geometry(state, &session_id, inner.width, inner.height);
    }

    let border = if focused {
        Style::default().fg(ui_accent_primary())
    } else {
        Style::default().fg(ui_border())
    };
    let status = state
        .terminal_panes
        .get(&session_id)
        .map(|p| {
            if p.connected {
                "●"
            } else {
                "○"
            }
        })
        .unwrap_or("?");
    let block = Block::default()
        .title(if focused {
            format!(" {title}  {status} ")
        } else {
            format!(" {title} ")
        })
        .borders(Borders::ALL)
        .border_style(border)
        .style(Style::default().bg(ui_panel_bg()));
    frame.render_widget(block, area);

    let Some(pane) = state.terminal_panes.get(&session_id) else {
        let msg = Paragraph::new("(terminal not attached — Ctrl+; t)")
            .style(Style::default().fg(Color::DarkGray).bg(ui_panel_bg()));
        frame.render_widget(msg, inner);
        return;
    };

    let Ok(grid) = pane.grid.lock() else {
        return;
    };
    let (cursor_col, cursor_row) = grid.cursor();
    let cols = grid.cols().min(inner.width) as usize;
    let rows = grid.rows().min(inner.height) as usize;
    let default_fg = Color::Rgb(0xe7, 0xe5, 0xe4);
    let default_bg = ui_panel_bg();
    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut spans: Vec<Span> = Vec::with_capacity(cols);
        for col in 0..cols {
            let cell = grid.cell_at(col as u16, row as u16);
            let mut fg = cell
                .fg
                .map(ansi_indexed_color)
                .unwrap_or(default_fg);
            let mut bg = cell
                .bg
                .map(ansi_indexed_color)
                .unwrap_or(default_bg);
            // Bold on dim indexed colors → bright sibling (xterm-ish).
            if cell.bold
                && let Some(idx) = cell.fg
                && idx < 8
            {
                fg = ansi_indexed_color(idx + 8);
            }
            if cell.reverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut style = Style::default().fg(fg).bg(bg);
            if cell.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            if focused && col as u16 == cursor_col && row as u16 == cursor_row {
                style = style.bg(default_fg).fg(default_bg);
            }
            spans.push(Span::styled(cell.ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    drop(grid);
    let widget = Paragraph::new(Text::from(lines)).style(Style::default().bg(default_bg));
    frame.render_widget(widget, inner);
}

/// Home TerminalPane xterm theme (16-color) — keep TUI and GUI shells aligned.
fn ansi_indexed_color(index: u8) -> Color {
    match index {
        0 => Color::Rgb(0x1c, 0x19, 0x17),  // black
        1 => Color::Rgb(0xf8, 0x71, 0x71),  // red
        2 => Color::Rgb(0x86, 0xef, 0xac),  // green
        3 => Color::Rgb(0xfd, 0xe0, 0x47),  // yellow
        4 => Color::Rgb(0x93, 0xc5, 0xfd),  // blue
        5 => Color::Rgb(0xc4, 0xb5, 0xfd),  // magenta
        6 => Color::Rgb(0x67, 0xe8, 0xf9),  // cyan
        7 => Color::Rgb(0xe7, 0xe5, 0xe4),  // white
        8 => Color::Rgb(0x78, 0x71, 0x6c),  // bright black
        9 => Color::Rgb(0xfc, 0xa5, 0xa5),  // bright red
        10 => Color::Rgb(0xbb, 0xf7, 0xd0), // bright green
        11 => Color::Rgb(0xfe, 0xf0, 0x8a), // bright yellow
        12 => Color::Rgb(0xbf, 0xdb, 0xfe), // bright blue
        13 => Color::Rgb(0xdd, 0xd6, 0xfe), // bright magenta
        14 => Color::Rgb(0xa5, 0xf3, 0xfc), // bright cyan
        _ => Color::Rgb(0xfa, 0xfa, 0xf9),  // bright white
    }
}

fn render_connection_picker_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 78, 72);
    frame.render_widget(Clear, popup);
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            " Connection  current:{}  scope:{} ",
            state.workshop_label, state.workshop_scope
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    if state.connection_picker_editing_custom {
        lines.push(Line::from(Span::styled(
            format!(" Paste URL: {}_  Enter apply · Esc cancel", state.connection_picker_custom),
            Style::default().fg(Color::Yellow),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "Filter: /{}   Enter switch · u paste URL · Esc close · Ctrl+; w",
                state.connection_picker_query
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));
    if state.connection_picker_hits.is_empty() && !state.connection_picker_editing_custom {
        lines.push(Line::from(Span::styled(
            "No workshops listed — press u to paste a daemon URL",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (idx, hit) in state.connection_picker_hits.iter().enumerate() {
            let selected = idx == state.connection_picker_selected;
            let marker = if selected { ">" } else { " " };
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let active = if medousa::tui::workshop_connection::normalize_daemon_url(&hit.url)
                == medousa::tui::workshop_connection::normalize_daemon_url(&state.daemon_url)
            {
                " ●"
            } else {
                ""
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "{marker} {}{active}  [{}]",
                    hit.label,
                    super::connection_runtime::choice_subtitle(hit)
                ),
                style,
            )));
        }
    }
    let widget = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Connection ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, popup);
}

fn render_terminal_picker_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 76, 70);
    frame.render_widget(Clear, popup);
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" Shell sessions  /{} ", state.terminal_picker_query),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Type to filter · Enter attach · Ctrl+N new · Esc close · Ctrl+; T",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    if state.terminal_picker_hits.is_empty() {
        lines.push(Line::from(Span::styled(
            "No sessions (Enter or Ctrl+N creates one — is medousa-session installed?)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (idx, hit) in state.terminal_picker_hits.iter().enumerate() {
            let selected = idx == state.terminal_picker_selected;
            let marker = if selected { ">" } else { " " };
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let work = hit
                .work_id
                .as_deref()
                .map(|w| format!(" work:{w}"))
                .unwrap_or_default();
            lines.push(Line::from(Span::styled(
                format!(
                    "{marker} {}  {} [{}]{work}",
                    &hit.session_id[..hit.session_id.len().min(12)],
                    hit.cwd,
                    hit.root_kind
                ),
                style,
            )));
        }
    }
    let widget = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Attach terminal ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, popup);
}

fn render_forge_picker_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 76, 70);
    frame.render_widget(Clear, popup);
    let target = match state.forge_picker_target {
        super::forge_runtime::ForgePickerTarget::Code => "Code",
        super::forge_runtime::ForgePickerTarget::Review => "Review",
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" Undertakings → {target}  /{} ", state.forge_picker_query),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Type to filter · Enter open · Esc close · Ctrl+; f/r",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    if state.forge_picker_hits.is_empty() {
        lines.push(Line::from(Span::styled(
            "No undertakings (is Forge provisioned on the daemon?)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (idx, hit) in state.forge_picker_hits.iter().enumerate() {
            let selected = idx == state.forge_picker_selected;
            let marker = if selected { ">" } else { " " };
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let title = if hit.title.is_empty() {
                hit.id.as_str()
            } else {
                hit.title.as_str()
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "{marker} {title}  [{} / {}]",
                    hit.human_phase, hit.state
                ),
                style,
            )));
            if selected && !hit.brief.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("    {}", hit.brief),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }
    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Forge ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, popup);
}

fn render_notes_picker_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 72, 70);
    frame.render_widget(Clear, popup);
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(" Library  /{} ", state.notes_picker_query),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "Type to search · Enter open · Esc close · Ctrl+; o",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    if state.notes_picker_hits.is_empty() {
        lines.push(Line::from(Span::styled(
            "No notes (is the daemon vault available?)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (idx, hit) in state.notes_picker_hits.iter().enumerate() {
            let selected = idx == state.notes_picker_selected;
            let marker = if selected { ">" } else { " " };
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(
                format!("{marker} {}  ({})", hit.title, hit.path),
                style,
            )));
            if selected && !hit.snippet.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("    {}", hit.snippet),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }
    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Notes ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, popup);
}

fn render_startup_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 72, 62);
    frame.render_widget(Clear, popup);

    let mut lines: Vec<Line> = Vec::new();

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
        let type_label = j.job_type.split('.').next_back().unwrap_or(&j.job_type);
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
            let label = medousa::session::format_session_history_label(
                &item.session_id,
                item.display_name.as_deref(),
            );
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
                "{marker} {label}  {ts}  turn={} ver={} trust={} last_verify={}  {}",
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

        if state.history_show_verification_detail
            && let Some(selected) = state.history_items.get(state.history_selected) {
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

    for (index, turn) in turns.iter().enumerate() {
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
                    "needs_input" => ("asking", Color::Cyan),
                    "final_pending" => ("wrapping up", Color::Magenta),
                    "tool_loop" => ("running tools", Color::Cyan),
                    "pack_hold" => ("held", Color::DarkGray),
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
            if let Some(handoff) = super::tui_presentation::render_handoff_line(turn) {
                lines.push(handoff);
            }
            for note in super::tui_presentation::progress_notes(turn) {
                lines.push(Line::from(Span::styled(
                    format!("  · {note}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            if state
                .active_agent_stream_turn
                .is_some_and(|active| active == index)
            {
                for note in state.turn_parts.live_progress_notes() {
                    let trimmed = note.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    lines.push(Line::from(Span::styled(
                        format!("  · {trimmed}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }

        let live_parts = state
            .active_agent_stream_turn
            .filter(|active| *active == index)
            .map(|_| &state.turn_parts);
        lines.extend(super::tui_presentation::render_turn_tool_lines(
            turn,
            live_parts,
        ));

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
