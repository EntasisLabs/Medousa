//! Daemon stream adapter for portable foreground-loop presentation events.

use medousa_runtime::{RuntimePortFuture, TurnPresentationPort};

use super::stream_sink::SharedAgentStreamSink;

#[derive(Clone)]
pub struct DaemonTurnPresentationPort {
    sink: SharedAgentStreamSink,
}

impl DaemonTurnPresentationPort {
    pub fn new(sink: SharedAgentStreamSink) -> Self {
        Self { sink }
    }
}

impl TurnPresentationPort for DaemonTurnPresentationPort {
    fn notice(&self, message: String) -> RuntimePortFuture<()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.notice(message).await;
        })
    }

    fn scratch_reset(&self, stream_turn_id: u64) -> RuntimePortFuture<()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.scratch_reset(stream_turn_id).await;
        })
    }

    fn turn_progress(
        &self,
        stream_turn_id: u64,
        message: String,
        tool_names: Vec<String>,
    ) -> RuntimePortFuture<()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.agent_turn_progress(stream_turn_id, message, tool_names)
                .await;
        })
    }

    fn pack_hold(
        &self,
        stream_turn_id: u64,
        fragments: Vec<String>,
        tool_names: Vec<String>,
    ) -> RuntimePortFuture<()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.agent_pack_hold(stream_turn_id, fragments, tool_names)
                .await;
        })
    }
}
