//! Output-side composition for the structural tool-loop completion gate.

use medousa_runtime::{RuntimePorts, TurnPresentationPort};
use std::sync::Arc;

use super::stream_sink::SharedAgentStreamSink;

pub use medousa_runtime::{
    ToolLoopCompletionGate, ToolLoopCompletionGateConfig, collect_tool_names,
};

/// Daemon convenience composition used by integration and golden-loop tests.
pub fn tool_loop_completion_gate_for_execution(
    stream_turn_id: u64,
    session_id: Option<String>,
    sink: Option<SharedAgentStreamSink>,
    max_tool_rounds: usize,
) -> ToolLoopCompletionGate<'static> {
    let tool_run_events = sink.clone().map(|sink| {
        Arc::new(super::tool_stream::DaemonToolRunEventPort::new(sink))
            as Arc<dyn medousa_runtime::ToolRunEventPort>
    });
    let turn_presentation = sink.clone().map(|sink| {
        Arc::new(super::turn_presentation::DaemonTurnPresentationPort::new(
            sink,
        )) as Arc<dyn TurnPresentationPort>
    });
    let model_response_events = sink.clone().map(|sink| {
        Arc::new(super::turn_presentation::DaemonModelResponseEventPort::new(
            sink,
            stream_turn_id,
        )) as Arc<dyn medousa_runtime::ModelResponseEventPort>
    });
    let budget_approval = Arc::new(
        crate::turn_budget_request::DaemonTurnBudgetApprovalPort::new(
            None,
            stream_turn_id,
            session_id.clone(),
            None,
            None,
            sink,
        ),
    );
    let runtime_ports = RuntimePorts::new()
        .with_optional_ledger_sink(super::turn_ledger::session_turn_ledger_sink(
            session_id.as_deref(),
        ))
        .with_optional_tool_run_events(tool_run_events)
        .with_optional_model_response_events(model_response_events)
        .with_optional_turn_presentation(turn_presentation)
        .with_budget_approval(budget_approval);
    ToolLoopCompletionGate::new_for_execution(stream_turn_id, runtime_ports, max_tool_rounds)
}
