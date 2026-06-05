use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use reqwest::Client;

use crate::daemon_api::InteractiveTurnStreamEvent;

/// Accumulated outcome from consuming an ingest SSE stream.
#[derive(Debug, Clone, Default)]
pub struct IngestStreamResult {
    pub final_text: Option<String>,
    pub needs_input: bool,
    pub error: Option<String>,
}

/// Render the user-visible body from a completed ingest stream.
pub fn render_stream_body(result: &IngestStreamResult) -> String {
    if let Some(error) = &result.error {
        return error.clone();
    }

    result
        .final_text
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "(empty response)".to_string())
}

/// Build the daemon ingest stream URL for a given stream id.
pub fn build_ingest_stream_url(daemon_base_url: &str, stream_id: &str) -> String {
    format!(
        "{}/v1/ingest/{}/stream",
        daemon_base_url.trim_end_matches('/'),
        stream_id.trim()
    )
}

/// Consume an ingest SSE stream until a terminal event arrives.
pub async fn consume_ingest_stream(client: &Client, stream_url: &str) -> Result<IngestStreamResult> {
    let response = client
        .get(stream_url)
        .send()
        .await
        .context("failed to reach ingest stream endpoint")?
        .error_for_status()
        .context("ingest stream endpoint returned error")?;

    let mut bytes = response.bytes_stream();
    let mut buffer = String::new();
    let mut result = IngestStreamResult::default();

    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.context("ingest stream read failed")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(idx) = buffer.find("\n\n") {
            let frame = buffer[..idx].to_string();
            buffer = buffer[idx + 2..].to_string();

            let Some(payload) = parse_sse_payload(&frame) else {
                continue;
            };

            match payload.event_type.as_str() {
                "content_delta" => {
                    if let Some(delta) = payload.content_delta.filter(|value| !value.is_empty()) {
                        let entry = result
                            .final_text
                            .get_or_insert_with(String::new);
                        entry.push_str(&delta);
                    }
                }
                "needs_input" => {
                    result.needs_input = true;
                    result.final_text = payload.final_text.or_else(|| {
                        if payload.message.trim().is_empty() {
                            None
                        } else {
                            Some(payload.message)
                        }
                    });
                    return Ok(result);
                }
                "final" => {
                    result.final_text = payload.final_text.or_else(|| {
                        if payload.message.trim().is_empty() {
                            None
                        } else {
                            Some(payload.message)
                        }
                    });
                    return Ok(result);
                }
                "error" => {
                    result.error = Some(if payload.message.trim().is_empty() {
                        "ingest stream failed".to_string()
                    } else {
                        payload.message
                    });
                    return Ok(result);
                }
                _ => {}
            }

            if payload.terminal {
                return Ok(result);
            }
        }
    }

    if result.final_text.is_some() || result.error.is_some() {
        return Ok(result);
    }

    Err(anyhow!("ingest stream closed without terminal event"))
}

fn parse_sse_payload(frame: &str) -> Option<InteractiveTurnStreamEvent> {
    let data = frame
        .lines()
        .filter_map(|line| {
            if let Some(value) = line.strip_prefix("data: ") {
                Some(value)
            } else if let Some(value) = line.strip_prefix("data:") {
                Some(value.trim_start())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if data.trim().is_empty() {
        return None;
    }

    serde_json::from_str::<InteractiveTurnStreamEvent>(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_stream_url_trims_slashes() {
        assert_eq!(
            build_ingest_stream_url("http://127.0.0.1:7419/", "ingest-abc"),
            "http://127.0.0.1:7419/v1/ingest/ingest-abc/stream"
        );
    }

    #[test]
    fn parse_sse_payload_reads_json_data_line() {
        let frame = "event: content_delta\ndata: {\"turn_id\":\"ingest-1\",\"event_type\":\"final\",\"phase\":\"complete\",\"message\":\"done\",\"content_delta\":null,\"reasoning_delta\":null,\"final_text\":\"hello\",\"tool_names\":null,\"terminal\":true,\"emitted_at_utc\":\"2026-05-30T00:00:00Z\"}\n";
        let payload = parse_sse_payload(frame).expect("payload");
        assert_eq!(payload.event_type, "final");
        assert_eq!(payload.final_text.as_deref(), Some("hello"));
    }
}
