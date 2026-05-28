/// Events emitted by cognition tools and background agent tasks back to the TUI event loop.
#[derive(Debug, Clone)]
pub enum TuiEvent {
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
    },
    /// Partial assistant output chunk streamed from the model.
    AgentChunk { turn_id: u64, delta: String },
    /// Partial model reasoning chunk streamed from the model.
    AgentReasoningChunk { turn_id: u64, delta: String },
    /// The tool loop failed with an error.
    AgentError { turn_id: u64, message: String },
    /// General UI notification emitted by background workers.
    UiNotice(String),
}
