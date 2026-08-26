//! Daemon stream adapter for portable foreground-loop presentation events.

use medousa_runtime::{
    ModelResponseCompleted, ModelResponseEventPort, RuntimePortFuture, TurnPresentationPort,
};

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

#[derive(Clone)]
pub struct DaemonModelResponseEventPort {
    sink: SharedAgentStreamSink,
    stream_turn_id: u64,
}

impl DaemonModelResponseEventPort {
    pub fn new(sink: SharedAgentStreamSink, stream_turn_id: u64) -> Self {
        Self {
            sink,
            stream_turn_id,
        }
    }
}

impl ModelResponseEventPort for DaemonModelResponseEventPort {
    fn completed(&self, event: ModelResponseCompleted) -> RuntimePortFuture<()> {
        let sink = self.sink.clone();
        let stream_turn_id = self.stream_turn_id;
        Box::pin(async move {
            sink.model_response_completed(stream_turn_id, event.model_round)
                .await;
        })
    }
}

impl TurnPresentationPort for DaemonTurnPresentationPort {
    fn notice(&self, message: String) -> RuntimePortFuture<()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.notice(message).await;
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
}
