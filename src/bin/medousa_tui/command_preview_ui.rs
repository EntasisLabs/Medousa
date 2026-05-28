use super::*;

#[derive(Clone, Copy)]
struct PaletteAction {
    category: PaletteCategory,
    title: &'static str,
    subtitle: &'static str,
    command: &'static str,
    risk: ActionRisk,
    key_hint: &'static str,
    aliases: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteCategory {
    QuickActions,
    Session,
    ModelRuntime,
    ToolsScripts,
    SafetyKeys,
}

#[derive(Clone, Copy)]
enum ActionRisk {
    Safe,
    Caution,
}

const CATEGORY_ORDER: [PaletteCategory; 5] = [
    PaletteCategory::QuickActions,
    PaletteCategory::Session,
    PaletteCategory::ModelRuntime,
    PaletteCategory::ToolsScripts,
    PaletteCategory::SafetyKeys,
];

const PALETTE_ACTIONS: [PaletteAction; 17] = [
    PaletteAction {
        category: PaletteCategory::QuickActions,
        title: "Start New Chat",
        subtitle: "Create a fresh session and clear the current thread",
        command: "/new",
        risk: ActionRisk::Caution,
        key_hint: "/new",
        aliases: &["new", "reset", "fresh"],
    },
    PaletteAction {
        category: PaletteCategory::QuickActions,
        title: "Open Past Sessions",
        subtitle: "Browse and switch to a previous conversation",
        command: "/history",
        risk: ActionRisk::Safe,
        key_hint: "Ctrl+H",
        aliases: &["history", "sessions", "recent"],
    },
    PaletteAction {
        category: PaletteCategory::QuickActions,
        title: "Open Settings",
        subtitle: "Adjust model, runtime, and safety preferences",
        command: "/settings",
        risk: ActionRisk::Safe,
        key_hint: "Ctrl+,",
        aliases: &["preferences", "config", "options"],
    },
    PaletteAction {
        category: PaletteCategory::QuickActions,
        title: "Open Theme Menu",
        subtitle: "Preview and apply UI themes",
        command: "/themes",
        risk: ActionRisk::Safe,
        key_hint: "/themes",
        aliases: &["theme", "appearance", "colors"],
    },
    PaletteAction {
        category: PaletteCategory::QuickActions,
        title: "Open Script Editor",
        subtitle: "Edit Grapheme source before running",
        command: "/edit",
        risk: ActionRisk::Safe,
        key_hint: "/edit",
        aliases: &["editor", "script", "open file"],
    },
    PaletteAction {
        category: PaletteCategory::QuickActions,
        title: "Run Editor Script",
        subtitle: "Execute source from the editor buffer",
        command: "/run",
        risk: ActionRisk::Caution,
        key_hint: "/run",
        aliases: &["run", "execute", "launch"],
    },
    PaletteAction {
        category: PaletteCategory::ToolsScripts,
        title: "Run Current File",
        subtitle: "Execute the active script file path",
        command: "/run-current",
        risk: ActionRisk::Caution,
        key_hint: "/run-current",
        aliases: &["current", "run file", "execute file"],
    },
    PaletteAction {
        category: PaletteCategory::ToolsScripts,
        title: "Allowlist Preview",
        subtitle: "Check referenced operations against policy",
        command: "/allowlist-preview",
        risk: ActionRisk::Safe,
        key_hint: "/allowlist-preview",
        aliases: &["allowlist", "policy", "permissions"],
    },
    PaletteAction {
        category: PaletteCategory::Session,
        title: "Stop Generation",
        subtitle: "Interrupt the active response stream",
        command: "/stop",
        risk: ActionRisk::Safe,
        key_hint: "Ctrl+G",
        aliases: &["stop", "cancel", "interrupt"],
    },
    PaletteAction {
        category: PaletteCategory::Session,
        title: "Regenerate Last Response",
        subtitle: "Re-run the last assistant turn",
        command: "/regen",
        risk: ActionRisk::Safe,
        key_hint: "/regen",
        aliases: &["regen", "retry", "again"],
    },
    PaletteAction {
        category: PaletteCategory::ModelRuntime,
        title: "Show Active Model",
        subtitle: "Display current provider and model",
        command: "/model",
        risk: ActionRisk::Safe,
        key_hint: "/model",
        aliases: &["model", "provider", "status"],
    },
    PaletteAction {
        category: PaletteCategory::ModelRuntime,
        title: "Check Daemon Status",
        subtitle: "Read daemon health and connectivity",
        command: "/daemon health",
        risk: ActionRisk::Safe,
        key_hint: "/daemon health",
        aliases: &["daemon", "health", "status"],
    },
    PaletteAction {
        category: PaletteCategory::ModelRuntime,
        title: "Daemon Command Help",
        subtitle: "See daemon subcommands and examples",
        command: "/daemon",
        risk: ActionRisk::Safe,
        key_hint: "/daemon",
        aliases: &["daemon help", "watch", "jobs"],
    },
    PaletteAction {
        category: PaletteCategory::Session,
        title: "Export Session (Markdown)",
        subtitle: "Save the current conversation as markdown",
        command: "/export md",
        risk: ActionRisk::Safe,
        key_hint: "/export md",
        aliases: &["export", "markdown", "save"],
    },
    PaletteAction {
        category: PaletteCategory::Session,
        title: "Export Session (JSONL)",
        subtitle: "Save the current conversation as jsonl",
        command: "/export jsonl",
        risk: ActionRisk::Safe,
        key_hint: "/export jsonl",
        aliases: &["export", "jsonl", "archive"],
    },
    PaletteAction {
        category: PaletteCategory::SafetyKeys,
        title: "Clear API Key",
        subtitle: "Remove the stored key from secure backend",
        command: "/clear-key",
        risk: ActionRisk::Caution,
        key_hint: "/clear-key",
        aliases: &["api", "key", "clear", "revoke"],
    },
    PaletteAction {
        category: PaletteCategory::SafetyKeys,
        title: "Rotate API Key",
        subtitle: "Replace key and update runtime auth",
        command: "/rotate-key",
        risk: ActionRisk::Caution,
        key_hint: "/rotate-key",
        aliases: &["api", "key", "rotate", "update"],
    },
];

fn ranked_palette_actions(
    query: &str,
    usage_counts: &HashMap<String, u64>,
    category: PaletteCategory,
) -> Vec<PaletteAction> {
    let q = query.trim().to_ascii_lowercase();
    let mut scored = PALETTE_ACTIONS
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, action)| action.category == category)
        .filter_map(|(index, action)| {
            score_palette_action(&q, &action, usage_counts).map(|score| (score, index, action))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, action)| action).collect()
}

fn score_palette_action(
    query: &str,
    action: &PaletteAction,
    usage_counts: &HashMap<String, u64>,
) -> Option<i32> {
    let usage_bonus = usage_counts
        .get(action.command)
        .copied()
        .unwrap_or(0)
        .min(25) as i32
        * 8;

    if query.is_empty() {
        let quick_action_bonus = if action.category == PaletteCategory::QuickActions {
            120
        } else {
            0
        };
        return Some(usage_bonus + quick_action_bonus);
    }

    let title = action.title.to_ascii_lowercase();
    let subtitle = action.subtitle.to_ascii_lowercase();
    let command = action.command.to_ascii_lowercase();
    let alias_match = action
        .aliases
        .iter()
        .any(|alias| alias.to_ascii_lowercase().contains(query));

    let mut score = 0i32;
    if title.starts_with(query) {
        score += 180;
    }
    if title.contains(query) {
        score += 120;
    }
    if subtitle.contains(query) {
        score += 70;
    }
    if command.starts_with(query) {
        score += 90;
    }
    if command.contains(query) {
        score += 60;
    }
    if alias_match {
        score += 80;
    }

    if score == 0 {
        None
    } else {
        Some(score + usage_bonus)
    }
}

fn active_palette_category(state: &TuiState) -> PaletteCategory {
    CATEGORY_ORDER
        .get(state.command_tab)
        .copied()
        .unwrap_or(PaletteCategory::QuickActions)
}

fn switch_palette_tab(state: &mut TuiState, forward: bool) {
    let current = state
        .command_tab
        .min(CATEGORY_ORDER.len().saturating_sub(1));
    state.command_tab = if forward {
        (current + 1) % CATEGORY_ORDER.len()
    } else if current == 0 {
        CATEGORY_ORDER.len() - 1
    } else {
        current - 1
    };

    state.command_selected = 0;
    state.command_scroll = 0;
    state.command_max_scroll = 0;
}

pub(crate) async fn handle_command_palette_key_event(
    code: KeyCode,
    state: &mut TuiState,
    tui_rt: &mut TuiRuntime,
    event_tx: &mpsc::Sender<TuiEvent>,
) -> EventOutcome {
    match code {
        KeyCode::Tab => {
            switch_palette_tab(state, true);
        }
        KeyCode::BackTab => {
            switch_palette_tab(state, false);
        }
        KeyCode::Backspace => {
            state.command_query.pop();
            state.command_selected = 0;
            state.command_scroll = 0;
        }
        KeyCode::Char(c) => {
            state.command_query.push(c);
            state.command_selected = 0;
            state.command_scroll = 0;
        }
        KeyCode::Up => {
            state.command_selected = state.command_selected.saturating_sub(1);
        }
        KeyCode::Down => {
            let max = ranked_palette_actions(
                &state.command_query,
                &state.command_usage_counts,
                active_palette_category(state),
            )
            .len()
            .saturating_sub(1);
            state.command_selected = (state.command_selected + 1).min(max);
        }
        KeyCode::PageUp => {
            state.command_scroll = state.command_scroll.saturating_sub(8);
        }
        KeyCode::PageDown => {
            state.command_scroll = state
                .command_scroll
                .saturating_add(8)
                .min(state.command_max_scroll);
        }
        KeyCode::Home => {
            state.command_scroll = 0;
        }
        KeyCode::End => {
            state.command_scroll = state.command_max_scroll;
        }
        KeyCode::Enter => {
            let actions = ranked_palette_actions(
                &state.command_query,
                &state.command_usage_counts,
                active_palette_category(state),
            );
            if let Some(action) = actions.get(state.command_selected).copied() {
                record_palette_usage(state, action.command);
                state.mode = UiMode::Chat;
                state.command_query.clear();
                state.command_tab = 0;
                state.command_selected = 0;
                state.command_scroll = 0;
                state.command_max_scroll = 0;
                return super::handle_slash_command(action.command, state, tui_rt, event_tx).await;
            }
        }
        _ => {}
    }

    EventOutcome::Continue
}

fn record_palette_usage(state: &mut TuiState, command: &str) {
    let entry = state
        .command_usage_counts
        .entry(command.to_string())
        .or_insert(0);
    *entry = entry.saturating_add(1);

    save_tui_defaults(&TuiDefaults {
        backend: Some(state.settings.backend.clone()),
        theme_id: Some(state.settings.theme_id.clone()),
        provider: Some(state.settings.provider.clone()),
        model: Some(state.settings.model.clone()),
        base_url: if state.settings.base_url.trim().is_empty() {
            None
        } else {
            Some(state.settings.base_url.clone())
        },
        env_overrides: if state.settings.env_overrides.trim().is_empty() {
            None
        } else {
            Some(state.settings.env_overrides.clone())
        },
        allowed_modules: {
            let parsed = parse_allowed_modules(&state.settings.allowed_modules);
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        },
        tool_call_mode: Some(state.settings.tool_call_mode.clone()),
        max_tool_rounds: Some(parse_usize_with_bounds(
            &state.settings.max_tool_rounds,
            10,
            1,
            50,
        )),
        thinking_capture: Some(parse_bool_with_default(
            &state.settings.thinking_capture,
            true,
        )),
        thinking_max_lines: Some(parse_usize_with_bounds(
            &state.settings.thinking_max_lines,
            300,
            50,
            5000,
        )),
        activation_direct_answer_max_prompt_chars: Some(parse_usize_with_bounds(
            &state.settings.activation_direct_answer_max_prompt_chars,
            320,
            64,
            4000,
        )),
        activation_long_session_turn_threshold: Some(parse_usize_with_bounds(
            &state.settings.activation_long_session_turn_threshold,
            28,
            8,
            500,
        )),
        activation_long_session_max_prompt_chars: Some(parse_usize_with_bounds(
            &state.settings.activation_long_session_max_prompt_chars,
            420,
            64,
            4000,
        )),
        slice_hot_window_turns: Some(parse_usize_with_bounds(
            &state.settings.slice_hot_window_turns,
            8,
            2,
            32,
        )),
        slice_cold_window_turns: Some(parse_usize_with_bounds(
            &state.settings.slice_cold_window_turns,
            24,
            4,
            128,
        )),
        retry_runtime_max_retries: Some(parse_usize_with_bounds(
            &state.settings.retry_runtime_max_retries,
            1,
            0,
            5,
        )),
        retry_runtime_max_rounds: Some(parse_usize_with_bounds(
            &state.settings.retry_runtime_max_rounds,
            3,
            1,
            10,
        )),
        verifier_min_citation_coverage: Some(parse_f32_with_bounds(
            &state.settings.verifier_min_citation_coverage,
            0.60,
            0.0,
            1.0,
        )),
        verifier_min_avg_support_strength: Some(parse_f32_with_bounds(
            &state.settings.verifier_min_avg_support_strength,
            0.70,
            0.0,
            1.0,
        )),
        verifier_min_supported_claim_ratio: Some(parse_f32_with_bounds(
            &state.settings.verifier_min_supported_claim_ratio,
            0.60,
            0.0,
            1.0,
        )),
        verifier_min_claim_support_strength: Some(parse_f32_with_bounds(
            &state.settings.verifier_min_claim_support_strength,
            0.65,
            0.0,
            1.0,
        )),
        response_depth_mode: Some(state.response_depth_mode.clone()),
        stage_routing: Some(state.stage_routing.clone()),
        command_usage_counts: if state.command_usage_counts.is_empty() {
            None
        } else {
            Some(state.command_usage_counts.clone())
        },
    });
}

pub(crate) fn handle_allowlist_preview_key_event(
    code: KeyCode,
    state: &mut TuiState,
) -> EventOutcome {
    let analysis = analyze_allowlist_preview(
        &state.allowlist_preview_source,
        &state.settings_draft.allowed_modules,
    );

    match code {
        KeyCode::Backspace => {
            state.allowlist_preview_source.pop();
        }
        KeyCode::Char(c) => {
            state.allowlist_preview_source.push(c);
        }
        KeyCode::Enter => {
            state.allowlist_preview_source.push('\n');
        }
        KeyCode::Tab => {
            if !analysis.invalid_allowlist.is_empty() {
                push_obs(
                    state,
                    format!(
                        "⚠ invalid allowlist ids: {}",
                        analysis.invalid_allowlist.join(", ")
                    ),
                );
                return EventOutcome::Continue;
            }
            if analysis.referenced_ops.is_empty() {
                push_obs(state, "no module calls found in source".to_string());
                return EventOutcome::Continue;
            }
            if analysis.blocked_ops.is_empty() {
                push_obs(
                    state,
                    format!(
                        "✓ all referenced operations are allowed ({})",
                        analysis.referenced_ops.join(", ")
                    ),
                );
            } else {
                push_obs(
                    state,
                    format!("⚠ blocked operations: {}", analysis.blocked_ops.join(", ")),
                );
            }
        }
        KeyCode::F(5) => {
            if analysis.referenced_ops.is_empty() {
                push_obs(
                    state,
                    "⚠ replace skipped: no operations detected".to_string(),
                );
            } else {
                state.settings_draft.allowed_modules = analysis.referenced_ops.join(",");
                push_obs(
                    state,
                    format!(
                        "✓ allowlist replaced with {} operation(s)",
                        analysis.referenced_ops.len()
                    ),
                );
            }
        }
        KeyCode::F(6) => {
            if analysis.referenced_ops.is_empty() {
                push_obs(
                    state,
                    "⚠ append skipped: no operations detected".to_string(),
                );
            } else {
                let mut merged = parse_allowed_modules(&state.settings_draft.allowed_modules);
                let mut seen = merged.iter().cloned().collect::<HashSet<String>>();

                let mut appended = 0usize;
                for op in &analysis.referenced_ops {
                    if seen.insert(op.clone()) {
                        merged.push(op.clone());
                        appended += 1;
                    }
                }

                state.settings_draft.allowed_modules = merged.join(",");
                push_obs(
                    state,
                    format!("✓ allowlist appended with {appended} operation(s)"),
                );
            }
        }
        _ => {}
    }
    EventOutcome::Continue
}

pub(crate) fn render_command_palette_overlay(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 78, 64);
    frame.render_widget(Clear, popup);

    let active_category = active_palette_category(state);
    let actions = ranked_palette_actions(
        &state.command_query,
        &state.command_usage_counts,
        active_category,
    );
    let selected = state.command_selected.min(actions.len().saturating_sub(1));
    state.command_selected = selected;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            " Find Action: {}",
            if state.command_query.is_empty() {
                "(type to filter)"
            } else {
                state.command_query.as_str()
            }
        ),
        Style::default().fg(Color::Cyan),
    )));

    let tab_nav = CATEGORY_ORDER
        .iter()
        .enumerate()
        .map(|(idx, category)| {
            let label = category_label(*category);
            if idx == state.command_tab {
                format!("[{label}]")
            } else {
                label.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("  ");
    lines.push(Line::from(Span::styled(
        format!(" Tabs: {tab_nav} "),
        Style::default().fg(Color::LightCyan),
    )));
    lines.push(Line::from(Span::styled(
        " Up/Down: move  Tab/Shift+Tab: tab  PgUp/PgDn/Wheel: scroll  Enter: run  Esc/Ctrl+K: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {} ", tab_help_text(active_category)),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let mut selected_line: Option<usize> = None;

    if actions.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "No matching commands in {}.",
                category_label(active_category)
            ),
            Style::default().fg(Color::Gray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(" {} ", category_label(active_category)),
            Style::default()
                .fg(ui_accent_primary())
                .add_modifier(Modifier::BOLD),
        )));

        for (idx, action) in actions.iter().enumerate() {
            if idx == selected {
                selected_line = Some(lines.len());
            }

            let marker = if idx == selected { ">" } else { " " };
            let row_style = if idx == selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(Span::styled(
                format!(
                    "{marker} {}  [{}]  {}  ({})",
                    action.title,
                    risk_label(action.risk),
                    action.command,
                    action.key_hint
                ),
                row_style,
            )));

            let subtitle_style = if idx == selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            };
            lines.push(Line::from(Span::styled(
                format!("    {}", action.subtitle),
                subtitle_style,
            )));
        }

        lines.push(Line::from(""));

        if let Some(action) = actions.get(selected) {
            lines.push(Line::from(Span::styled(
                " Selected Action ",
                Style::default()
                    .fg(ui_accent_primary())
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!(" Will run: {}", action.command),
                Style::default().fg(Color::Cyan),
            )));
            lines.push(Line::from(Span::styled(
                format!(" Effect: {}", command_effect(action.command)),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let text = Text::from(lines);
    let inner_width = popup.width.saturating_sub(2);
    let visible_height = popup.height.saturating_sub(2);
    let visual_lines = visual_line_count(&text, inner_width);
    state.command_max_scroll = visual_lines.saturating_sub(visible_height);
    state.command_scroll = state.command_scroll.min(state.command_max_scroll);

    if let Some(line_idx) = selected_line {
        let visible_rows = visible_height as usize;
        if visible_rows > 0 {
            let top = state.command_scroll as usize;
            let bottom = top.saturating_add(visible_rows.saturating_sub(1));
            if line_idx < top {
                state.command_scroll = line_idx as u16;
            } else if line_idx > bottom {
                state.command_scroll =
                    line_idx.saturating_add(1).saturating_sub(visible_rows) as u16;
            }
            state.command_scroll = state.command_scroll.min(state.command_max_scroll);
        }
    }

    let panel = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Command Center ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false })
        .scroll((state.command_scroll, 0));
    frame.render_widget(panel, popup);
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

fn category_label(category: PaletteCategory) -> &'static str {
    match category {
        PaletteCategory::QuickActions => "Quick Actions",
        PaletteCategory::Session => "Session",
        PaletteCategory::ModelRuntime => "Model & Runtime",
        PaletteCategory::ToolsScripts => "Tools & Scripts",
        PaletteCategory::SafetyKeys => "Safety & Keys",
    }
}

fn tab_help_text(category: PaletteCategory) -> &'static str {
    match category {
        PaletteCategory::QuickActions => " High-frequency moves to keep momentum.",
        PaletteCategory::Session => " History, exports, and response iteration controls.",
        PaletteCategory::ModelRuntime => " Model visibility and daemon runtime tooling.",
        PaletteCategory::ToolsScripts => " Script execution and allowlist diagnostics.",
        PaletteCategory::SafetyKeys => " API key lifecycle and sensitive access actions.",
    }
}

fn risk_label(risk: ActionRisk) -> &'static str {
    match risk {
        ActionRisk::Safe => "safe",
        ActionRisk::Caution => "caution",
    }
}

fn command_effect(command: &str) -> &'static str {
    match command {
        "/new" => "Resets context to a fresh conversation.",
        "/history" => "Opens session history for quick switching.",
        "/settings" => "Opens settings tabs for model/runtime changes.",
        "/edit" => "Opens script editor.",
        "/run" | "/run-current" => "Executes script against runtime and tools.",
        "/allowlist-preview" => "Analyzes policy coverage for referenced operations.",
        "/stop" => "Stops the active generation stream.",
        "/regen" => "Repeats the previous assistant response attempt.",
        "/model" => "Shows the currently active model routing.",
        "/daemon health" => "Reads daemon health state.",
        "/daemon" => "Shows daemon command help.",
        "/export md" | "/export jsonl" => "Writes session transcript to disk.",
        "/clear-key" => "Removes stored API key from secure storage.",
        "/rotate-key" => "Rotates API key and updates runtime access.",
        _ => "Runs the selected slash command.",
    }
}

pub(crate) fn render_allowlist_preview_overlay(frame: &mut ratatui::Frame, state: &TuiState) {
    let area = frame.area();
    let popup = centered_rect(area, 86, 70);
    frame.render_widget(Clear, popup);

    let analysis = analyze_allowlist_preview(
        &state.allowlist_preview_source,
        &state.settings_draft.allowed_modules,
    );

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Type source  Enter: new line  Tab: evaluate  F5: replace allowlist  F6: append allowlist  Esc: close ",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        " Uses the same parser and policy checks as runtime enforcement ",
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "Allowlist: {}",
            if state.settings_draft.allowed_modules.trim().is_empty() {
                "(all operations allowed)".to_string()
            } else {
                state.settings_draft.allowed_modules.clone()
            }
        ),
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(Span::styled(
        "Source (editable):",
        Style::default().fg(Color::Yellow),
    )));
    if state.allowlist_preview_source.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty)",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for (idx, source_line) in state.allowlist_preview_source.lines().enumerate() {
            lines.push(Line::from(Span::styled(
                format!("  {:>2}: {}", idx + 1, source_line),
                Style::default().fg(Color::Yellow),
            )));
        }
    }
    lines.push(Line::from(""));

    if !analysis.invalid_allowlist.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "Invalid allowlist IDs: {}",
                analysis.invalid_allowlist.join(", ")
            ),
            Style::default().fg(Color::Red),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            if analysis.blocked_ops.is_empty() {
                "Result: Allowed".to_string()
            } else {
                "Result: Blocked".to_string()
            },
            Style::default().fg(if analysis.blocked_ops.is_empty() {
                Color::Green
            } else {
                Color::Red
            }),
        )));
    }
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        "Referenced Ops:",
        Style::default().fg(Color::Cyan),
    )));
    if analysis.referenced_ops.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none detected)",
            Style::default().fg(Color::Gray),
        )));
    } else {
        for op in &analysis.referenced_ops {
            lines.push(Line::from(Span::styled(
                format!("  - {op}"),
                Style::default().fg(Color::White),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Blocked Ops:",
        Style::default().fg(Color::Cyan),
    )));
    if analysis.blocked_ops.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::Green),
        )));
    } else {
        for op in &analysis.blocked_ops {
            lines.push(Line::from(Span::styled(
                format!("  - {op}"),
                Style::default().fg(Color::Red),
            )));
        }
    }

    let panel = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(" Allowlist Preview ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ui_accent_primary()))
                .style(Style::default().bg(ui_modal_bg())),
        )
        .style(Style::default().fg(Color::White).bg(ui_modal_bg()))
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, popup);
}
