//! Surface presentation profiles — canonical turn body vs channel-specific formatting.
//!
//! P0 rule: **session + SSE store prose-only answers**; `tool_names` carry structured
//! metadata. External push channels (Telegram, WhatsApp, …) append a plain tool footer
//! at **dispatch** time via [`format_channel_delivery_text`].

use stasis::application::orchestration::tool_loop_pipeline::ToolInvocation;

use crate::channel_delivery::{self, normalize_channel_surface};
use crate::daemon_api::TurnSurfaceContext;

/// How a surface wants tool visibility in user-facing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelToolsFooter {
    /// Rich surfaces — tools rendered by the client (chips, timeline).
    None,
    /// Plain comma-separated list appended at channel dispatch.
    PlainList,
}

/// Presentation policy for a turn surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationProfile {
    /// When true, append a markdown tool footer into canonical session/SSE body.
    /// P0+: always false — keep canonical prose clean for Home/TUI/journal.
    pub append_tools_to_canonical_body: bool,
    /// Tool footer style when pushing to external messaging channels.
    pub channel_tools_footer: ChannelToolsFooter,
}

impl PresentationProfile {
    pub const RICH_SURFACE: Self = Self {
        append_tools_to_canonical_body: false,
        channel_tools_footer: ChannelToolsFooter::None,
    };

    pub const EXTERNAL_PUSH: Self = Self {
        append_tools_to_canonical_body: false,
        channel_tools_footer: ChannelToolsFooter::PlainList,
    };
}

pub fn presentation_profile_for_channel(channel: &str) -> PresentationProfile {
    let normalized = normalize_channel_surface(channel);
    if channel_delivery::is_external_push_channel(&normalized) {
        PresentationProfile::EXTERNAL_PUSH
    } else {
        PresentationProfile::RICH_SURFACE
    }
}

pub fn presentation_profile_for_surface(surface: Option<&TurnSurfaceContext>) -> PresentationProfile {
    let channel = surface
        .and_then(|ctx| ctx.channel_surface.as_deref())
        .unwrap_or(channel_delivery::CHANNEL_INTERACTIVE);
    presentation_profile_for_channel(channel)
}

pub fn unique_tool_names(invocations: &[ToolInvocation]) -> Vec<String> {
    let mut names = invocations
        .iter()
        .map(|inv| inv.tool_name.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort();
    names
}

pub fn unique_tool_names_from_slices(tool_names: &[String]) -> Vec<String> {
    let mut names = tool_names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Legacy markdown footer — retained for opt-in canonical append (not used in P0).
pub fn format_tools_footer_markdown_from_invocations(
    invocations: &[ToolInvocation],
) -> Option<String> {
    format_tools_footer_markdown(&unique_tool_names(invocations))
}

pub fn format_tools_footer_markdown(tool_names: &[String]) -> Option<String> {
    let names = unique_tool_names_from_slices(tool_names);
    if names.is_empty() {
        return None;
    }
    Some(format!(
        "\n\n---\n_Tools this turn:_ {}",
        names.join(", ")
    ))
}

pub fn format_tools_footer_plain(tool_names: &[String]) -> Option<String> {
    let names = unique_tool_names_from_slices(tool_names);
    if names.is_empty() {
        return None;
    }
    Some(format!("\n\nTools: {}", names.join(", ")))
}

pub fn maybe_append_tools_to_canonical_body(
    body: &mut String,
    invocations: &[ToolInvocation],
    profile: PresentationProfile,
) {
    if !profile.append_tools_to_canonical_body {
        return;
    }
    if let Some(footer) = format_tools_footer_markdown_from_invocations(invocations) {
        body.push_str(&footer);
    }
}

/// Format assistant answer text for an external channel push.
pub fn format_channel_delivery_text(
    answer: &str,
    tool_names: &[String],
    channel: &str,
) -> String {
    match presentation_profile_for_channel(channel).channel_tools_footer {
        ChannelToolsFooter::None => answer.to_string(),
        ChannelToolsFooter::PlainList => {
            let mut out = answer.to_string();
            if let Some(footer) = format_tools_footer_plain(tool_names) {
                out.push_str(&footer);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_api::TurnSurfaceContext;

    #[test]
    fn rich_surfaces_keep_canonical_body_clean() {
        let profile = presentation_profile_for_surface(Some(&TurnSurfaceContext::tui()));
        assert!(!profile.append_tools_to_canonical_body);
        assert_eq!(profile.channel_tools_footer, ChannelToolsFooter::None);

        let home = presentation_profile_for_channel("home-desktop");
        assert_eq!(home, PresentationProfile::RICH_SURFACE);
    }

    #[test]
    fn external_push_appends_plain_footer_at_dispatch_only() {
        let profile = presentation_profile_for_channel("telegram");
        assert!(!profile.append_tools_to_canonical_body);
        assert_eq!(profile.channel_tools_footer, ChannelToolsFooter::PlainList);

        let body = format_channel_delivery_text(
            "Here is the answer.",
            &["cognition_mcp_invoke".to_string(), "cognition_mcp_invoke".to_string()],
            "telegram",
        );
        assert!(body.starts_with("Here is the answer."));
        assert!(body.contains("\n\nTools: cognition_mcp_invoke"));
        assert!(!body.contains("_Tools this turn:_"));
    }

    #[test]
    fn home_dispatch_does_not_append_tools() {
        let body = format_channel_delivery_text(
            "Here is the answer.",
            &["cognition_mcp_invoke".to_string()],
            "home-desktop",
        );
        assert_eq!(body, "Here is the answer.");
    }

    #[test]
    fn canonical_append_respects_profile_gate() {
        let mut body = "Answer".to_string();
        maybe_append_tools_to_canonical_body(
            &mut body,
            &[],
            PresentationProfile::RICH_SURFACE,
        );
        assert_eq!(body, "Answer");

        let invocations = vec![ToolInvocation {
            tool_name: "cognition_mcp_invoke".to_string(),
            tool_input: serde_json::Value::Null,
            tool_output: serde_json::Value::Null,
        }];
        let mut legacy = "Answer".to_string();
        maybe_append_tools_to_canonical_body(
            &mut legacy,
            &invocations,
            PresentationProfile {
                append_tools_to_canonical_body: true,
                channel_tools_footer: ChannelToolsFooter::PlainList,
            },
        );
        assert!(legacy.contains("_Tools this turn:_"));
    }
}
