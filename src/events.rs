use tokio::sync::mpsc;

/// Mechanical best-effort event publication for tool/application helpers.
pub trait ToolEventSenderExt {
    fn tool_invoked(&self, tool_name: impl Into<String>, input_summary: impl Into<String>) -> bool;
    fn job_enqueued(&self, job_id: impl Into<String>, job_type: impl Into<String>) -> bool;
}

impl ToolEventSenderExt for mpsc::Sender<TuiEvent> {
    fn tool_invoked(&self, tool_name: impl Into<String>, input_summary: impl Into<String>) -> bool {
        self.try_send(TuiEvent::ToolInvoked {
            tool_name: tool_name.into(),
            input_summary: input_summary.into(),
        })
        .is_ok()
    }

    fn job_enqueued(&self, job_id: impl Into<String>, job_type: impl Into<String>) -> bool {
        self.try_send(TuiEvent::JobEnqueued {
            job_id: job_id.into(),
            job_type: job_type.into(),
        })
        .is_ok()
    }
}

/// Events emitted by cognition tools and background agent tasks back to the TUI event loop.
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// One native chronological daemon fact. `turn_id` is the TUI-local routing
    /// id; the envelope retains the daemon turn id and replay cursor.
    TurnStreamV3 {
        turn_id: u64,
        envelope: medousa_types::TurnStreamEnvelopeV3,
    },
    /// Structured tool run started (P1/P4 presentation).
    ToolRunStarted {
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        tool_round: usize,
    },
    /// Structured tool run finished (P1/P4 presentation).
    ToolRunFinished {
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        output_summary: Option<String>,
        tool_round: usize,
    },
    /// A cognition tool was invoked during the tool loop.
    ToolInvoked {
        tool_name: String,
        input_summary: String,
    },
    /// Full tool payload emitted after an invocation completes.
    ToolPayload {
        tool_name: String,
        tool_input: serde_json::Value,
        tool_output: serde_json::Value,
        input_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
        output_receipt: Option<crate::payload_receipt::ArtifactReceiptMeta>,
    },
    /// A job was enqueued into the Stasis runtime.
    JobEnqueued { job_id: String, job_type: String },
    /// A job was processed (synchronously executed inside a tool invocation).
    JobProcessed {
        job_id: String,
        succeeded: bool,
        execution_id: Option<String>,
    },
    /// The tool loop returned a final agent response.
    AgentResponse {
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
        /// When false, the turn stays open (host worker ack); a later terminal response completes it.
        terminal: bool,
        /// Workspace card id when this is a worker handoff ack.
        work_id: Option<String>,
    },
    /// Medousa is asking the operator a clarifying question (terminal, distinct from a full answer).
    AgentNeedsInput {
        turn_id: u64,
        text: String,
        tool_names: Vec<String>,
    },
    /// Non-terminal: `begin_work` progress line — status only.
    AgentTurnProgress {
        turn_id: u64,
        message: String,
        tool_names: Vec<String>,
    },
    /// Partial assistant output chunk streamed from the model.
    AgentChunk { turn_id: u64, delta: String },
    /// Partial model reasoning chunk streamed from the model.
    AgentReasoningChunk { turn_id: u64, delta: String },
    /// The tool loop failed with an error.
    AgentError { turn_id: u64, message: String },
    /// General UI notification emitted by background workers.
    UiNotice(String),
    /// Turn-start context budget breakdown (Cursor-style telemetry).
    ContextUsage {
        report: crate::daemon_api::ContextUsageReport,
    },
    /// MCP invoke blocked pending operator approval for a side-effect action.
    ApprovalRequired {
        server_id: String,
        tool_name: String,
        reason: String,
    },
    /// Tool-round budget extension waiting on operator approve/deny.
    TurnBudgetApprovalRequired {
        turn_id: u64,
        request_id: String,
        rounds_executed: usize,
        max_tool_rounds: usize,
        requested_rounds: usize,
        reason: String,
        progress_summary: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::{ToolEventSenderExt, TuiEvent};

    #[tokio::test]
    async fn tool_events_are_best_effort_and_structured() {
        let (sender, mut receiver) = mpsc::channel(2);
        assert!(sender.tool_invoked("tool.test", "summary"));
        assert!(sender.job_enqueued("job-1", "test.job"));

        assert!(matches!(
            receiver.recv().await,
            Some(TuiEvent::ToolInvoked { tool_name, .. }) if tool_name == "tool.test"
        ));
        assert!(matches!(
            receiver.recv().await,
            Some(TuiEvent::JobEnqueued { job_id, .. }) if job_id == "job-1"
        ));
    }

    #[test]
    fn closed_event_receivers_are_ignored() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        assert!(!sender.tool_invoked("tool.test", "summary"));
        assert!(!sender.job_enqueued("job-1", "test.job"));
    }
}
