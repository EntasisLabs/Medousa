//! Portable, immutable context admitted for one daemon-owned turn.

/// Where the daemon delivers a turn or job result after execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDeliveryTarget {
    pub channel: String,
    pub user_id: String,
    pub channel_id: String,
    pub session_id: String,
    pub stream_id: Option<String>,
}

impl ChannelDeliveryTarget {
    pub fn new(
        channel: impl Into<String>,
        user_id: impl Into<String>,
        channel_id: impl Into<String>,
        session_id: impl Into<String>,
        stream_id: Option<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            user_id: user_id.into(),
            channel_id: channel_id.into(),
            session_id: session_id.into(),
            stream_id,
        }
    }

    pub fn interactive(
        channel: impl Into<String>,
        user_id: impl Into<String>,
        channel_id: impl Into<String>,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self::new(
            channel,
            user_id,
            channel_id,
            session_id,
            Some(turn_id.into()),
        )
    }
}

/// Compatibility context retained by the production loop while callers move
/// to typed execution ports. Authority is fixed at daemon admission time.
#[derive(Debug, Clone)]
pub struct TurnContinuationScope {
    pub turn_correlation_id: String,
    pub session_id: String,
    pub identity_user_id: Option<String>,
    pub original_prompt: String,
    pub delivery_target: Option<ChannelDeliveryTarget>,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub supports_ui_artifacts: bool,
    pub supports_liquid_markdown: bool,
    pub supports_browser_host: bool,
    pub channel_surface: Option<String>,
}
