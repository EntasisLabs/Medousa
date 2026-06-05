use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::payload_receipt::ArtifactReceiptMeta;

#[async_trait]
pub trait AgentStreamSink: Send + Sync {
    async fn content_chunk(&self, turn_id: u64, delta: String);
    async fn reasoning_chunk(&self, turn_id: u64, delta: String);
    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>);
    /// Terminal turn where Medousa needs operator input (clarifying question / pivot).
    async fn agent_needs_input(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.agent_response(turn_id, text, tool_names).await;
    }
    /// Non-terminal signal after `cognition_turn_prepare_final` — still working on the final answer.
    async fn agent_final_pending(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        let _ = (turn_id, text, tool_names);
    }
    /// Short host acknowledgement while a background turn worker runs (non-terminal delivery).
    async fn agent_worker_ack(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.agent_response(turn_id, text, tool_names).await;
    }
    async fn agent_error(&self, turn_id: u64, message: String);
    async fn notice(&self, message: String);
    async fn tool_invoked(&self, tool_name: String, input_summary: String);
    async fn tool_payload(
        &self,
        tool_name: String,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
    );

    /// Clear in-flight assistant scratch text before the next model round (TUI replaces draft).
    async fn scratch_reset(&self, turn_id: u64) {
        let _ = turn_id;
    }
}

pub type SharedAgentStreamSink = Arc<dyn AgentStreamSink>;
