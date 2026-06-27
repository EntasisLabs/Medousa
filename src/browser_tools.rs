//! Agent Browser Host tool gating via `TurnSurfaceContext.supports_browser_host`.

use medousa_types::daemon_api::TurnSurfaceContext;

pub const COGNITION_BROWSER_FETCH: &str = "cognition_browser_fetch";
pub const COGNITION_BROWSER_SNAPSHOT: &str = "cognition_browser_snapshot";

pub const BROWSER_COGNITION_TOOLS: &[&str] =
    &[COGNITION_BROWSER_FETCH, COGNITION_BROWSER_SNAPSHOT];

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
}
