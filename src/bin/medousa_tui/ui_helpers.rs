use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::Color,
};
use std::sync::{LazyLock, RwLock};

#[derive(Clone, Copy)]
struct UiTheme {
    id: &'static str,
    bg: Color,
    panel_bg: Color,
    modal_bg: Color,
    border: Color,
    accent_primary: Color,
    accent_warn: Color,
}

const UI_THEMES: [UiTheme; 3] = [
    UiTheme {
        id: "medousa-default",
        bg: Color::Rgb(18, 22, 29),
        panel_bg: Color::Rgb(26, 32, 41),
        modal_bg: Color::Rgb(31, 38, 49),
        border: Color::Rgb(71, 89, 105),
        accent_primary: Color::Rgb(64, 186, 213),
        accent_warn: Color::Rgb(245, 189, 99),
    },
    UiTheme {
        id: "arctic-ink",
        bg: Color::Rgb(12, 18, 24),
        panel_bg: Color::Rgb(20, 30, 40),
        modal_bg: Color::Rgb(26, 37, 50),
        border: Color::Rgb(90, 124, 150),
        accent_primary: Color::Rgb(98, 210, 232),
        accent_warn: Color::Rgb(255, 200, 112),
    },
    UiTheme {
        id: "amber-noir",
        bg: Color::Rgb(22, 17, 12),
        panel_bg: Color::Rgb(33, 24, 18),
        modal_bg: Color::Rgb(42, 31, 23),
        border: Color::Rgb(120, 96, 74),
        accent_primary: Color::Rgb(236, 170, 92),
        accent_warn: Color::Rgb(255, 214, 130),
    },
];

static ACTIVE_UI_THEME: LazyLock<RwLock<UiTheme>> = LazyLock::new(|| RwLock::new(UI_THEMES[0]));

fn resolve_theme(theme_id: &str) -> UiTheme {
    UI_THEMES
        .iter()
        .copied()
        .find(|theme| theme.id.eq_ignore_ascii_case(theme_id))
        .unwrap_or(UI_THEMES[0])
}

fn active_ui_theme() -> UiTheme {
    ACTIVE_UI_THEME
        .read()
        .map(|guard| *guard)
        .unwrap_or(UI_THEMES[0])
}

pub(crate) fn set_active_ui_theme(theme_id: &str) {
    if let Ok(mut guard) = ACTIVE_UI_THEME.write() {
        *guard = resolve_theme(theme_id);
    }
}

pub(crate) fn ui_theme_ids() -> &'static [&'static str] {
    static IDS: [&str; 3] = ["medousa-default", "arctic-ink", "amber-noir"];
    &IDS
}

pub(crate) fn ui_theme_display_name(theme_id: &str) -> &'static str {
    match theme_id {
        "medousa-default" => "Medousa Default",
        "arctic-ink" => "Arctic Ink",
        "amber-noir" => "Amber Noir",
        _ => "Medousa Default",
    }
}

pub(crate) fn ui_bg() -> Color {
    active_ui_theme().bg
}

pub(crate) fn ui_panel_bg() -> Color {
    active_ui_theme().panel_bg
}

pub(crate) fn ui_modal_bg() -> Color {
    active_ui_theme().modal_bg
}

pub(crate) fn ui_border() -> Color {
    active_ui_theme().border
}

pub(crate) fn ui_accent_primary() -> Color {
    active_ui_theme().accent_primary
}

pub(crate) fn ui_accent_warn() -> Color {
    active_ui_theme().accent_warn
}

pub(crate) fn centered_rect(
    area: ratatui::layout::Rect,
    width_percent: u16,
    height_percent: u16,
) -> ratatui::layout::Rect {
    let height_margin = 100u16.saturating_sub(height_percent);
    let width_margin = 100u16.saturating_sub(width_percent);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(height_margin / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage(height_margin - (height_margin / 2)),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(width_margin / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage(width_margin - (width_margin / 2)),
        ])
        .split(vertical[1])[1]
}
