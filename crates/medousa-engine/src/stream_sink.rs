use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::receipt::ArtifactReceiptMeta;

#[async_trait]
pub trait AgentStreamSink: Send + Sync {
    async fn content_chunk(&self, turn_id: u64, delta: String);
    async fn reasoning_chunk(&self, turn_id: u64, delta: String);
    async fn agent_response(&self, turn_id: u64, text: String, tool_names: Vec<String>);
    async fn agent_needs_input(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        self.agent_response(turn_id, text, tool_names).await;
    }
    async fn agent_final_pending(&self, turn_id: u64, text: String, tool_names: Vec<String>) {
        let _ = (turn_id, text, tool_names);
    }
    async fn agent_turn_progress(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        let _ = (turn_id, message, tool_names);
    }
    async fn agent_turn_checkpoint(&self, turn_id: u64, message: String, tool_names: Vec<String>) {
        self.agent_response(turn_id, message, tool_names).await;
    }
    async fn agent_worker_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        let _ = (turn_id, text, tool_names, work_id);
    }
    async fn agent_workshop_ack(
        &self,
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        work_id: Option<String>,
    ) {
        self.agent_worker_ack(turn_id, text, tool_names, work_id)
            .await;
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
    async fn tool_run_started(
        &self,
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        tool_round: usize,
    ) {
        let _ = (tool_run_id, tool_round);
        self.tool_invoked(tool_name, input_summary).await;
    }
    async fn tool_run_finished(
        &self,
        tool_run_id: String,
        tool_name: String,
        _status: String,
        _input_summary: String,
        _output_summary: Option<String>,
        tool_input: Value,
        tool_output: Value,
        input_receipt: Option<ArtifactReceiptMeta>,
        output_receipt: Option<ArtifactReceiptMeta>,
        _tool_round: usize,
    ) {
        let _ = tool_run_id;
        self.tool_payload(
            tool_name,
            tool_input,
            tool_output,
            input_receipt,
            output_receipt,
        )
        .await;
    }

    async fn reset_streamed_markdown(&self) {}

    async fn scratch_reset(&self, turn_id: u64) {
        let _ = turn_id;
    }

    async fn stage_persist_scratch(&self, _scratch: Value) {}

    async fn turn_budget_approval_required(
        &self,
        turn_id: u64,
        request_id: String,
        rounds_executed: usize,
        max_tool_rounds: usize,
        requested_rounds: usize,
        reason: String,
        progress_summary: Option<String>,
    ) {
        let _ = (
            turn_id,
            request_id,
            rounds_executed,
            max_tool_rounds,
            requested_rounds,
            reason,
            progress_summary,
        );
    }

    async fn browser_challenge_required(
        &self,
        turn_correlation_id: &str,
        session_id: String,
        challenge_url: String,
        reason: String,
    ) {
        let _ = (turn_correlation_id, session_id, challenge_url, reason);
    }

    async fn browser_navigated(
        &self,
        turn_correlation_id: &str,
        url: String,
        title: Option<String>,
        opened_by_agent: bool,
    ) {
        let _ = (turn_correlation_id, url, title, opened_by_agent);
    }
}

pub type SharedAgentStreamSink = Arc<dyn AgentStreamSink>;
