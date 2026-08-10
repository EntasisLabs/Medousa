//! Agent Browser Host tool gating via `TurnSurfaceContext.supports_browser_host`.

use medousa_types::daemon_api::TurnSurfaceContext;

use crate::semantic_values::TrimmedText;

pub const COGNITION_BROWSER_FETCH: &str = "cognition_browser_fetch";
pub const COGNITION_BROWSER_SNAPSHOT: &str = "cognition_browser_snapshot";
pub const COGNITION_BROWSER_ACT: &str = "cognition_browser_act";

pub const BROWSER_COGNITION_TOOLS: &[&str] = &[
    COGNITION_BROWSER_FETCH,
    COGNITION_BROWSER_SNAPSHOT,
    COGNITION_BROWSER_ACT,
];

#[derive(Debug)]
pub(crate) struct BrowserUrlCommand {
    pub(crate) url: TrimmedText,
    pub(crate) max_chars: usize,
}

impl BrowserUrlCommand {
    pub(crate) fn new(
        url: Option<String>,
        max_chars: usize,
        tool_id: &str,
    ) -> stasis::prelude::Result<Self> {
        let url = url.ok_or_else(|| {
            stasis::domain::errors::StasisError::PortFailure(format!("{tool_id}: url is required"))
        })?;
        let url = TrimmedText::new(url).map_err(|_| {
            stasis::domain::errors::StasisError::PortFailure(format!("{tool_id}: url is required"))
        })?;
        Ok(Self { url, max_chars })
    }
}

pub fn surface_supports_browser_host(surface: Option<&TurnSurfaceContext>) -> bool {
    surface.is_some_and(|ctx| ctx.supports_browser_host)
}

pub fn channel_surface_label(surface: Option<&TurnSurfaceContext>) -> Option<String> {
    surface
        .and_then(|ctx| ctx.channel_surface.clone())
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
}

pub fn is_client_executed_browser(surface: Option<&TurnSurfaceContext>) -> bool {
    channel_surface_label(surface)
        .is_some_and(|label| label.starts_with("home-ios") || label.starts_with("home-android"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_host_requires_client_flag() {
        assert!(!surface_supports_browser_host(None));
        assert!(!surface_supports_browser_host(Some(&TurnSurfaceContext::tui())));
        assert!(surface_supports_browser_host(Some(
            &TurnSurfaceContext::default().with_browser_host(true)
        )));
    }

    #[test]
    fn browser_url_command_normalizes_required_url() {
        let command = BrowserUrlCommand::new(
            Some("  https://example.test  ".to_string()),
            4000,
            "cognition_browser_fetch",
        )
        .expect("url");
        assert_eq!(command.url.as_str(), "https://example.test");
        assert_eq!(command.max_chars, 4000);
    }

    #[test]
    fn browser_url_command_rejects_blank_url() {
        let error = BrowserUrlCommand::new(
            Some(" \n\t".to_string()),
            4000,
            "cognition_browser_snapshot",
        )
        .expect_err("blank url should fail");
        assert!(error.to_string().contains("url is required"));
    }
}
