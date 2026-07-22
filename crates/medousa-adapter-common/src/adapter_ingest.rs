use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use medousa_types::daemon_api::{DeliverPollResponse, IngestResponse, JobResultResponse};
use reqwest::Client;
use tokio::time::sleep;

use crate::ingest_stream::consume_ingest_stream;

/// Outcome of waiting for outbox-driven delivery after an ask ingest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterDeliveryOutcome {
    /// Push delivery completed; adapter should not send final text.
    PushDelivered,
    /// Stream reported an error before terminal delivery.
    StreamError { message: String },
    /// Push did not confirm in time; caller should render fallback text.
    Fallback { text: String },
}

const DEFAULT_DELIVERY_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(700);

/// Wait for outbox push delivery. Returns fallback output when push does not confirm in time.
pub async fn wait_for_ask_delivery(
    client: &Client,
    daemon_url: &str,
    response: &IngestResponse,
    timeout: Duration,
) -> Result<AdapterDeliveryOutcome> {
    let job_id = response
        .job_id
        .as_deref()
        .ok_or_else(|| anyhow!("missing job_id on stream_ready ingest response"))?;

    if let Some(stream_url) = response.stream_url.as_deref()
        && let Some(error) = consume_ingest_stream_errors_only(client, stream_url).await? {
            return Ok(AdapterDeliveryOutcome::StreamError { message: error });
        }

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let poll = poll_delivery_status(client, daemon_url, job_id).await?;
        match poll.status.as_str() {
            "delivered" => {
                if poll
                    .error
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    break;
                }
                return Ok(AdapterDeliveryOutcome::PushDelivered);
            }
            "failed" => {
                let message = poll
                    .error
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "request failed".to_string());
                return Ok(AdapterDeliveryOutcome::StreamError { message });
            }
            _ if tokio::time::Instant::now() >= deadline => break,
            _ => sleep(DEFAULT_POLL_INTERVAL).await,
        }
    }

    let result = fetch_job_result(client, daemon_url, job_id).await?;
    if result.status == "succeeded" {
        let text = result
            .output_text
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "(empty response)".to_string());
        return Ok(AdapterDeliveryOutcome::Fallback { text });
    }

    Ok(AdapterDeliveryOutcome::StreamError {
        message: result
            .latest_outcome
            .unwrap_or_else(|| format!("job ended with status={}", result.status)),
    })
}

/// Consume ingest SSE until terminal, returning only stream errors (not final text).
pub async fn consume_ingest_stream_errors_only(
    client: &Client,
    stream_url: &str,
) -> Result<Option<String>> {
    let result = consume_ingest_stream(client, stream_url).await?;
    Ok(result.error)
}

pub async fn poll_delivery_status(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
) -> Result<DeliverPollResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/deliver/poll/{job_id}"))
        .send()
        .await
        .context("failed to reach deliver poll endpoint")?
        .error_for_status()
        .context("deliver poll endpoint returned error")?;

    response
        .json::<DeliverPollResponse>()
        .await
        .context("failed to decode deliver poll response")
}

pub async fn fetch_job_result(
    client: &Client,
    daemon_url: &str,
    job_id: &str,
) -> Result<JobResultResponse> {
    let daemon_url = daemon_url.trim_end_matches('/');
    let response = client
        .get(format!("{daemon_url}/v1/jobs/{job_id}/result"))
        .send()
        .await
        .context("failed to reach job result endpoint")?
        .error_for_status()
        .context("job result endpoint returned error")?;

    response
        .json::<JobResultResponse>()
        .await
        .context("failed to decode job result response")
}

pub fn default_delivery_timeout() -> Duration {
    DEFAULT_DELIVERY_TIMEOUT
}

pub fn should_send_immediate_ingest_reply(response: &IngestResponse) -> bool {
    !response.stream_ready && !response.reply.trim().is_empty()
}

pub fn format_ingest_ack(response: &IngestResponse) -> String {
    if response.is_new_session {
        format!("🆕 {}\n\n{}", response.reply, ADAPTER_COMMAND_HINT)
    } else {
        response.reply.clone()
    }
}

pub const ADAPTER_COMMAND_HINT: &str =
    "Commands: /new /help /history /model /depth /stop /regen /health /heartbeat — or send a message to chat.";

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::daemon_api::IngestResponse;

    #[test]
    fn skip_immediate_reply_when_stream_ready() {
        let response = IngestResponse {
            session_id: "s1".to_string(),
            job_id: Some("job-1".to_string()),
            reply: "processing your request…".to_string(),
            is_new_session: false,
            stream_id: Some("stream-1".to_string()),
            stream_url: Some("http://localhost/stream".to_string()),
            stream_ready: true,
        };
        assert!(!should_send_immediate_ingest_reply(&response));
    }

    #[test]
    fn send_immediate_reply_for_command_results() {
        let response = IngestResponse {
            session_id: "s1".to_string(),
            job_id: None,
            reply: "Medousa ingester online.".to_string(),
            is_new_session: false,
            stream_id: None,
            stream_url: None,
            stream_ready: false,
        };
        assert!(should_send_immediate_ingest_reply(&response));
    }
}
