//! Environment / canvas theme validation.

pub const ALLOWED_COLOR_THEME_IDS: &[&str] = &[
    "medousa",
    "black-lily",
    "cupertino",
    "graphite",
    "midnight",
    "one-dark",
    "catppuccin",
    "tokyo-night",
    "github",
    "dracula",
    "nord",
    "solarized",
];

pub fn is_valid_color_theme_id(id: &str) -> bool {
    let trimmed = id.trim();
    !trimmed.is_empty() && ALLOWED_COLOR_THEME_IDS.contains(&trimmed)
}

pub fn is_valid_brand_color(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    (hex.len() == 3 || hex.len() == 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}
