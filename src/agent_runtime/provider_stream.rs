use std::sync::{Arc, OnceLock};

use stasis::ports::outbound::ai_chat_client::StreamDelta;
use tokio::sync::{Semaphore, mpsc};

use super::stream_sink::SharedAgentStreamSink;

pub(crate) const PROVIDER_STREAM_MESSAGE_CAPACITY: usize = 256;
pub(crate) const PROVIDER_STREAM_BYTE_CAPACITY: usize = 1024 * 1024;
const GLOBAL_PROVIDER_STREAM_BYTE_CAPACITY: usize = 64 * 1024 * 1024;

struct QueuedStreamDelta {
    delta: StreamDelta,
    _turn_bytes: tokio::sync::OwnedSemaphorePermit,
    _global_bytes: tokio::sync::OwnedSemaphorePermit,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderStreamReport {
    pub(crate) emitted: bool,
    pub(crate) overflowed: bool,
}

/// Owns the bounded side of a provider stream and serializes accepted deltas
/// into the turn sink. Draining this bridge is the terminal publication fence.
pub(crate) struct ProviderStreamBridge {
    sender: Option<mpsc::Sender<QueuedStreamDelta>>,
    turn_bytes: Arc<Semaphore>,
    global_bytes: Arc<Semaphore>,
    pump: Option<tokio::task::JoinHandle<()>>,
}

impl ProviderStreamBridge {
    pub(crate) fn new(sink: SharedAgentStreamSink, turn_id: u64) -> Self {
        static GLOBAL_BYTES: OnceLock<Arc<Semaphore>> = OnceLock::new();
        let (sender, mut receiver) =
            mpsc::channel::<QueuedStreamDelta>(PROVIDER_STREAM_MESSAGE_CAPACITY);
        let pump = tokio::spawn(async move {
            while let Some(queued) = receiver.recv().await {
                match queued.delta {
                    StreamDelta::Content(delta) => sink.content_chunk(turn_id, delta).await,
                    StreamDelta::Reasoning(delta) | StreamDelta::ThoughtSignature(delta) => {
                        sink.reasoning_chunk(turn_id, delta).await
                    }
                }
            }
        });
        Self {
            sender: Some(sender),
            turn_bytes: Arc::new(Semaphore::new(PROVIDER_STREAM_BYTE_CAPACITY)),
            global_bytes: Arc::clone(
                GLOBAL_BYTES
                    .get_or_init(|| Arc::new(Semaphore::new(GLOBAL_PROVIDER_STREAM_BYTE_CAPACITY))),
            ),
            pump: Some(pump),
        }
    }

    pub(crate) fn attempt(&self) -> ProviderStreamAttempt {
        ProviderStreamAttempt::new(
            self.sender
                .as_ref()
                .expect("provider stream attempt requested after drain")
                .clone(),
            Arc::clone(&self.turn_bytes),
            Arc::clone(&self.global_bytes),
        )
    }

    pub(crate) async fn drain(&mut self) {
        self.sender.take();
        if let Some(pump) = self.pump.take() {
            let _ = pump.await;
        }
    }

    #[cfg(test)]
    pub(crate) fn pump_abort_handle(&self) -> tokio::task::AbortHandle {
        self.pump.as_ref().unwrap().abort_handle()
    }

    #[cfg(test)]
    pub(crate) fn retained_sender(&self) -> impl Clone + Send + 'static {
        self.sender.as_ref().unwrap().clone()
    }
}

impl Drop for ProviderStreamBridge {
    fn drop(&mut self) {
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
    }
}

/// The provider API is fixed to an unbounded Tokio sender. This compatibility
/// pump never blocks on downstream work: it admits into bounded memory or marks
/// the attempt overflowed so the turn fails visibly after the provider returns.
pub(crate) struct ProviderStreamAttempt {
    sender: Option<mpsc::UnboundedSender<StreamDelta>>,
    pump: Option<tokio::task::JoinHandle<ProviderStreamReport>>,
}

impl ProviderStreamAttempt {
    fn new(
        target: mpsc::Sender<QueuedStreamDelta>,
        turn_bytes: Arc<Semaphore>,
        global_bytes: Arc<Semaphore>,
    ) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<StreamDelta>();
        let pump = tokio::spawn(async move {
            let mut report = ProviderStreamReport::default();
            while let Some(delta) = receiver.recv().await {
                report.emitted |= !stream_delta_text(&delta).is_empty();
                if report.overflowed {
                    continue;
                }
                let bytes = stream_delta_text(&delta).len().max(1);
                if bytes > PROVIDER_STREAM_BYTE_CAPACITY {
                    report.overflowed = true;
                    continue;
                }
                let Ok(turn_permit) = Arc::clone(&turn_bytes).try_acquire_many_owned(bytes as u32)
                else {
                    report.overflowed = true;
                    continue;
                };
                let Ok(global_permit) =
                    Arc::clone(&global_bytes).try_acquire_many_owned(bytes as u32)
                else {
                    report.overflowed = true;
                    continue;
                };
                if target
                    .try_send(QueuedStreamDelta {
                        delta,
                        _turn_bytes: turn_permit,
                        _global_bytes: global_permit,
                    })
                    .is_err()
                {
                    report.overflowed = true;
                }
            }
            report
        });
        Self {
            sender: Some(sender),
            pump: Some(pump),
        }
    }

    pub(crate) fn sender(&self) -> &mpsc::UnboundedSender<StreamDelta> {
        self.sender
            .as_ref()
            .expect("provider sender already closed")
    }

    pub(crate) async fn finish(mut self) -> ProviderStreamReport {
        self.sender.take();
        match self.pump.take() {
            Some(pump) => pump.await.unwrap_or(ProviderStreamReport {
                emitted: true,
                overflowed: true,
            }),
            None => ProviderStreamReport {
                emitted: true,
                overflowed: true,
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn pump_abort_handle(&self) -> tokio::task::AbortHandle {
        self.pump.as_ref().unwrap().abort_handle()
    }
}

impl Drop for ProviderStreamAttempt {
    fn drop(&mut self) {
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
    }
}

fn stream_delta_text(delta: &StreamDelta) -> &str {
    match delta {
        StreamDelta::Content(text)
        | StreamDelta::Reasoning(text)
        | StreamDelta::ThoughtSignature(text) => text,
    }
}

pub(crate) fn fail_on_stream_overflow<T>(
    result: stasis::domain::errors::Result<T>,
    report: ProviderStreamReport,
) -> stasis::domain::errors::Result<T> {
    if report.overflowed {
        Err(stasis::domain::errors::StasisError::PortFailure(
            "provider stream exceeded the bounded turn ingress".to_string(),
        ))
    } else {
        result
    }
}
