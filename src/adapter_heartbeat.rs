use std::time::Duration;

use crate::HeartbeatStatusResponse;
use reqwest::Client;

/// Whether proactive heartbeat nudges are enabled for an adapter env prefix.
pub fn heartbeat_nudges_enabled(env_prefix: &str) -> bool {
    let key = format!("{env_prefix}_HEARTBEAT_NUDGES_ENABLED");
    std::env::var(&key)
        .ok()
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

pub async fn fetch_heartbeat_summary(client: &Client, daemon_url: &str) -> Option<String> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/heartbeat/status"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<HeartbeatStatusResponse>()
        .await
        .ok()?;

    Some(format!(
        "heartbeat action={} significance={:.2} reason={}\nfailed={} dead_letter={} outbox_pending={}",
        response.action,
        response.significance,
        response.reason,
        response.failed_jobs,
        response.dead_letter_jobs,
        response.pending_outbox_events,
    ))
}

pub fn heartbeat_poll_interval() -> Duration {
    Duration::from_secs(30)
}
