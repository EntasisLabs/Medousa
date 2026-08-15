use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use medousa_sdk::{HttpTransport, MedousaClient};
use medousa_types::TurnStreamEventV2;
use reqwest::Client;

/// Accumulated outcome from consuming an ingest SSE stream.
#[derive(Debug, Clone, Default)]
pub struct IngestStreamResult {
    pub final_text: Option<String>,
    pub needs_input: bool,
    pub final_pending: bool,
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
    let sdk = MedousaClient::with_transport(
        Arc::new(HttpTransport::with_client(client.clone())),
        stream_url,
    );
    let interactive = sdk.interactive();
    let mut events = interactive.stream_reconnecting_v2(stream_url);
    let mut result = IngestStreamResult::default();

    while let Some(payload) = events.next().await {
        let payload = payload.context("ingest stream read failed")?;

        match payload.event {
            TurnStreamEventV2::ContentAppend { text } => {
                if !text.is_empty() {
                    result.final_text.get_or_insert_with(String::new).push_str(&text);
                }
            }
            TurnStreamEventV2::NeedsInput { text, .. } => {
                result.needs_input = true;
                result.final_text = non_empty(text);
                return Ok(result);
            }
            TurnStreamEventV2::FinalPending { text, .. } => {
                result.final_pending = true;
                if let Some(text) = non_empty(text) {
                    result.final_text = Some(text);
                }
            }
            TurnStreamEventV2::Final { text, .. }
            | TurnStreamEventV2::Checkpoint { text, .. }
            | TurnStreamEventV2::WorkerSynthesis { text, .. } => {
                if let Some(text) = non_empty(text) {
                    result.final_text = Some(text);
                }
                return Ok(result);
            }
            TurnStreamEventV2::Error {
                operator_message, ..
            } => {
                result.error = Some(non_empty(operator_message).unwrap_or_else(|| {
                    "ingest stream failed".to_string()
                }));
                return Ok(result);
            }
            TurnStreamEventV2::ReasoningAppend { .. }
            | TurnStreamEventV2::Status { .. }
            | TurnStreamEventV2::Progress { .. }
            | TurnStreamEventV2::PackHold { .. }
            | TurnStreamEventV2::ModelReceipt { .. }
            | TurnStreamEventV2::WorkerAck { .. }
            | TurnStreamEventV2::ScratchReset
            | TurnStreamEventV2::ToolStarted { .. }
            | TurnStreamEventV2::ToolFinished { .. }
            | TurnStreamEventV2::ArtifactPresented { .. }
            | TurnStreamEventV2::ArtifactUpdated { .. }
            | TurnStreamEventV2::UiScene { .. }
            | TurnStreamEventV2::BudgetApprovalRequired { .. }
            | TurnStreamEventV2::BrowserChallenge { .. }
            | TurnStreamEventV2::BrowserNavigated { .. }
            | TurnStreamEventV2::ContextUsage { .. }
            | TurnStreamEventV2::PermissionRequest { .. } => {}
        }
    }

    if result.final_text.is_some() || result.error.is_some() {
        return Ok(result);
    }

    Err(anyhow!("ingest stream closed without terminal event"))
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
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
    fn v2_final_payload_decodes() {
        let payload: medousa_types::TurnStreamEnvelopeV2 =
            medousa_sdk::streaming::decode_sse_json(
                r#"{"schema_version":2,"turn_id":"ingest-1","seq":1,"emitted_at_utc":"2026-05-30T00:00:00Z","event":{"type":"final","text":"hello"}}"#,
            )
            .expect("payload");
        assert!(matches!(
            payload.event,
            TurnStreamEventV2::Final { text, .. } if text == "hello"
        ));
    }
}
