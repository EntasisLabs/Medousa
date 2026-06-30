//! Reconnecting interactive turn SSE stream with spine-backed `?since=` replay.

#[cfg(all(feature = "async", feature = "sse"))]
use std::pin::Pin;
#[cfg(all(feature = "async", feature = "sse"))]
use std::task::{Context, Poll};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::{Stream, StreamExt};

#[cfg(all(feature = "async", feature = "sse"))]
use medousa_types::InteractiveTurnStreamEvent;

#[cfg(all(feature = "async", feature = "sse"))]
use tokio::time::{sleep, Sleep};

#[cfg(all(feature = "async", feature = "sse"))]
use crate::client::MedousaClient;
#[cfg(all(feature = "async", feature = "sse"))]
use crate::reconnect::{
    stream_path_with_since, CircuitBreaker, CircuitState, OverlapGuard, ReconnectPolicy,
};
#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{decode_sse_json, SseLineStream};
#[cfg(all(feature = "async", feature = "sse"))]
use crate::SdkError;

#[cfg(all(feature = "async", feature = "sse"))]
type EventStream<'a> = Pin<
    Box<dyn Stream<Item = Result<InteractiveTurnStreamEvent, SdkError>> + Send + 'a>,
>;

#[cfg(all(feature = "async", feature = "sse"))]
enum Phase<'a> {
    OpenStream,
    Streaming(EventStream<'a>),
    Backoff(Pin<Box<Sleep>>),
    Done,
}

/// Interactive SSE stream that reattaches with `?since=<last_seq>` after drops.
#[cfg(all(feature = "async", feature = "sse"))]
pub struct ReconnectingInteractiveStream<'a> {
    client: &'a MedousaClient,
    base_path: String,
    policy: ReconnectPolicy,
    overlap: OverlapGuard,
    breaker: CircuitBreaker,
    last_seq: u64,
    reconnect_attempt: u32,
    terminal_seen: bool,
    phase: Phase<'a>,
    _reconnect_permit: Option<crate::reconnect::OverlapPermit>,
}

#[cfg(all(feature = "async", feature = "sse"))]
impl<'a> ReconnectingInteractiveStream<'a> {
    pub fn new(client: &'a MedousaClient, stream_path: impl Into<String>) -> Self {
        Self::with_policy(client, stream_path, ReconnectPolicy::default())
    }

    pub fn with_policy(
        client: &'a MedousaClient,
        stream_path: impl Into<String>,
        policy: ReconnectPolicy,
    ) -> Self {
        Self {
            client,
            base_path: stream_path.into(),
            breaker: CircuitBreaker::new(policy.breaker.clone()),
            policy,
            overlap: OverlapGuard::new(),
            last_seq: 0,
            reconnect_attempt: 0,
            terminal_seen: false,
            phase: Phase::OpenStream,
            _reconnect_permit: None,
        }
    }

    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    fn open_stream(&self) -> EventStream<'a> {
        let path = stream_path_with_since(&self.base_path, self.last_seq);
        let byte_stream = self
            .client
            .transport()
            .stream_sse(self.client.base_url(), path);
        Box::pin(
            SseLineStream::new(byte_stream).map(|line| line.and_then(|data| decode_sse_json(&data))),
        )
    }

    fn begin_backoff(&mut self) -> Result<(), SdkError> {
        if self.terminal_seen {
            self.phase = Phase::Done;
            return Ok(());
        }
        if !self.policy.backoff.may_retry(self.reconnect_attempt) {
            return Err(SdkError::Transport(
                "interactive stream reconnect attempts exhausted".to_string(),
            ));
        }
        if self.breaker.state() == CircuitState::Open {
            return Err(SdkError::Transport(
                "interactive stream reconnect circuit open".to_string(),
            ));
        }
        self._reconnect_permit = self.overlap.try_enter();
        if self._reconnect_permit.is_none() {
            return Err(SdkError::Transport(
                "interactive stream reconnect already running".to_string(),
            ));
        }
        let delay = self.policy.backoff.delay(self.reconnect_attempt);
        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
        self.phase = Phase::Backoff(Box::pin(sleep(delay)));
        Ok(())
    }

    fn ingest(&mut self, event: InteractiveTurnStreamEvent) -> Option<InteractiveTurnStreamEvent> {
        if !apply_stream_seq(&mut self.last_seq, &event) {
            return None;
        }
        if event.terminal {
            self.terminal_seen = true;
        }
        Some(event)
    }
}

#[cfg(all(feature = "async", feature = "sse"))]
impl Stream for ReconnectingInteractiveStream<'_> {
    type Item = Result<InteractiveTurnStreamEvent, SdkError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match std::mem::replace(&mut self.phase, Phase::Done) {
                Phase::Done => return Poll::Ready(None),
                Phase::OpenStream => {
                    if !self.breaker.allow() {
                        return Poll::Ready(Some(Err(SdkError::Transport(
                            "interactive stream reconnect circuit open".to_string(),
                        ))));
                    }
                    let inner = self.open_stream();
                    self.phase = Phase::Streaming(inner);
                }
                Phase::Backoff(mut sleep) => match Pin::new(&mut sleep).poll(cx) {
                    Poll::Pending => {
                        self.phase = Phase::Backoff(sleep);
                        return Poll::Pending;
                    }
                    Poll::Ready(()) => {
                        self._reconnect_permit = None;
                        self.phase = Phase::OpenStream;
                    }
                },
                Phase::Streaming(mut inner) => match Pin::new(&mut inner).poll_next(cx) {
                    Poll::Ready(Some(Ok(event))) => {
                        self.phase = Phase::Streaming(inner);
                        self.breaker.on_success();
                        self.reconnect_attempt = 0;
                        if self.terminal_seen {
                            self.phase = Phase::Done;
                        }
                        if let Some(out) = self.ingest(event) {
                            if self.terminal_seen {
                                self.phase = Phase::Done;
                            }
                            return Poll::Ready(Some(Ok(out)));
                        }
                    }
                    Poll::Ready(Some(Err(_err))) => {
                        self.breaker.on_failure();
                        match self.begin_backoff() {
                            Ok(()) => {
                                if matches!(self.phase, Phase::Done) {
                                    return Poll::Ready(None);
                                }
                            }
                            Err(err) => return Poll::Ready(Some(Err(err))),
                        }
                    }
                    Poll::Ready(None) => {
                        if self.terminal_seen {
                            self.phase = Phase::Done;
                            return Poll::Ready(None);
                        }
                        self.breaker.on_failure();
                        match self.begin_backoff() {
                            Ok(()) => {
                                if matches!(self.phase, Phase::Done) {
                                    return Poll::Ready(None);
                                }
                            }
                            Err(err) => return Poll::Ready(Some(Err(err))),
                        }
                    }
                    Poll::Pending => {
                        self.phase = Phase::Streaming(inner);
                        return Poll::Pending;
                    }
                },
            }
        }
    }
}

#[cfg(all(feature = "async", feature = "sse"))]
pub fn apply_stream_seq(last_seq: &mut u64, event: &InteractiveTurnStreamEvent) -> bool {
    if event.seq != 0 && event.seq <= *last_seq {
        return false;
    }
    if event.seq != 0 {
        *last_seq = (*last_seq).max(event.seq);
    }
    true
}
