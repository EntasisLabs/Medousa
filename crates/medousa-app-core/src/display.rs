//! Shared display filters used while projecting daemon facts into chat items.

use regex::Regex;
use std::sync::OnceLock;

fn startup_text_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:(?:(?:interactive|background) turn|ingest) accepted;\s*)?(?:daemon )?agent runtime (?:started|accepted|running|ready|streaming active|completed|finished)[.!]?$|^(?:(?:interactive|background) turn accepted|background turn started)(?:;\s*streaming to attached clients)?[.!]?$",
        )
        .expect("startup text regex")
    })
}

/// Routine lifecycle acknowledgements add no useful progress, including in saved turns.
pub fn is_runtime_startup_text(message: Option<&str>) -> bool {
    let text = message.map(str::trim).unwrap_or("");
    if text.is_empty() {
        return false;
    }
    startup_text_re().is_match(text)
}

pub fn visible_chat_status_line(message: Option<&str>) -> Option<String> {
    let text = message.map(str::trim).filter(|value| !value.is_empty())?;
    if is_runtime_startup_text(Some(text)) {
        None
    } else {
        Some(text.to_string())
    }
}
