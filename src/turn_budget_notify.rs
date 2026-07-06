//! Operator notifications when a turn pauses for tool-round budget approval.

use anyhow::Result;
use reqwest::Client;

use crate::channel_delivery::{
    self, ChannelDeliveryTarget, is_external_push_channel, is_home_channel, work_deep_link_url,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnBudgetNotifyPayload {
    pub request_id: String,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub requested_rounds: usize,
    pub reason: String,
    pub progress_summary: Option<String>,
}

pub fn compose_turn_budget_notify_text(payload: &TurnBudgetNotifyPayload) -> String {
    let progress = payload
        .progress_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("\nProgress: {value}"))
        .unwrap_or_default();
    let deep_link = work_deep_link_url(&payload.request_id);
    format!(
        "Medousa turn paused at {}/{} tool rounds.\n\
         Requesting +{} rounds: {}\n\
         Request id: `{}`\n\
         Approve or deny in Medousa (Blocked) or open: {deep_link}{progress}",
        payload.rounds_executed,
        payload.max_tool_rounds,
        payload.requested_rounds,
        payload.reason.trim(),
        payload.request_id,
    )
}

/// Push to external messaging channels (Telegram/Discord/Slack/WhatsApp).
/// Home surfaces rely on workspace blocked cards + SSE; TUI uses obs events.
pub async fn notify_turn_budget_approval_required(
    client: &Client,
    delivery_target: &ChannelDeliveryTarget,
    payload: TurnBudgetNotifyPayload,
) -> Result<()> {
    if is_home_channel(&delivery_target.channel) {
        let summary = payload
            .progress_summary
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(payload.reason.trim());
        crate::home_push::notify_budget_approval(
            &payload.request_id,
            summary,
            summary,
        )
        .await;
        return Ok(());
    }
    if !is_external_push_channel(&delivery_target.channel) {
        return Ok(());
    }

    let text = compose_turn_budget_notify_text(&payload);
    channel_delivery::dispatch_channel_message(client, delivery_target, &text).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_includes_deep_link_and_progress() {
        let text = compose_turn_budget_notify_text(&TurnBudgetNotifyPayload {
            request_id: "req-abc".to_string(),
            rounds_executed: 4,
            max_tool_rounds: 8,
            requested_rounds: 3,
            reason: "Need more MCP reads".to_string(),
            progress_summary: Some("Half done".to_string()),
        });
        assert!(text.contains("4/8"));
        assert!(text.contains("+3 rounds"));
        assert!(text.contains("medousa://work/req-abc"));
        assert!(text.contains("Half done"));
    }

    #[test]
    fn home_channel_skips_external_push() {
        assert!(is_home_channel("home-ios"));
        assert!(!is_external_push_channel("home-desktop"));
        assert!(is_external_push_channel("telegram"));
    }
}
