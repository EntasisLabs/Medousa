//! Reconnecting interactive turn SSE stream with spine-backed `?since=` replay.

#[cfg(all(feature = "async", feature = "sse"))]
use std::pin::Pin;
#[cfg(all(feature = "async", feature = "sse"))]
use std::task::{Context, Poll};

#[cfg(all(feature = "async", feature = "sse"))]
use futures_util::{Stream, StreamExt};

#[cfg(all(feature = "async", feature = "sse"))]
use medousa_types::{InteractiveTurnStreamEvent, TurnStreamEnvelopeV2};
#[cfg(all(feature = "async", feature = "sse"))]
use serde::de::DeserializeOwned;

#[cfg(all(feature = "async", feature = "sse"))]
use tokio::time::{Sleep, sleep};

#[cfg(all(feature = "async", feature = "sse"))]
use crate::SdkError;
#[cfg(all(feature = "async", feature = "sse"))]
use crate::client::MedousaClient;
#[cfg(all(feature = "async", feature = "sse"))]
use crate::reconnect::{
    CircuitBreaker, CircuitState, OverlapGuard, ReconnectPolicy, stream_path_with_since,
};
#[cfg(all(feature = "async", feature = "sse"))]
use crate::streaming::{SseLineStream, decode_sse_json};

#[cfg(all(feature = "async", feature = "sse"))]
type EventStream<'a, E> = Pin<Box<dyn Stream<Item = Result<E, SdkError>> + Send + 'a>>;

#[cfg(all(feature = "async", feature = "sse"))]
enum Phase<'a, E> {
    OpenStream,
    Streaming(EventStream<'a, E>),
    Backoff(Pin<Box<Sleep>>),
    Done,
}

/// Interactive SSE stream that reattaches with `?since=<last_seq>` after drops.
#[cfg(all(feature = "async", feature = "sse"))]
pub struct ReconnectingTurnStream<'a, E> {
    client: &'a MedousaClient,
    base_path: String,
    accept: &'static str,
    sequence: fn(&E) -> u64,
    terminal: fn(&E) -> bool,
    policy: ReconnectPolicy,
    overlap: OverlapGuard,
    breaker: CircuitBreaker,
    last_seq: u64,
    reconnect_attempt: u32,
    terminal_seen: bool,
    phase: Phase<'a, E>,
    _reconnect_permit: Option<crate::reconnect::OverlapPermit>,
}

#[cfg(all(feature = "async", feature = "sse"))]
pub type ReconnectingInteractiveStream<'a> = ReconnectingTurnStream<'a, InteractiveTurnStreamEvent>;

#[cfg(all(feature = "async", feature = "sse"))]
pub type ReconnectingInteractiveStreamV2<'a> = ReconnectingTurnStream<'a, TurnStreamEnvelopeV2>;

#[cfg(all(feature = "async", feature = "sse"))]
impl<'a> ReconnectingTurnStream<'a, InteractiveTurnStreamEvent> {
    pub fn new(client: &'a MedousaClient, stream_path: impl Into<String>) -> Self {
        Self::with_policy(client, stream_path, ReconnectPolicy::default())
    }

    pub fn with_policy(
        client: &'a MedousaClient,
        stream_path: impl Into<String>,
        policy: ReconnectPolicy,
    ) -> Self {
        Self::with_wire(
            client,
            stream_path,
            policy,
            "text/event-stream",
            |event| event.seq,
            |event| event.terminal,
        )
    }
}

#[cfg(all(feature = "async", feature = "sse"))]
impl<'a> ReconnectingTurnStream<'a, TurnStreamEnvelopeV2> {
    pub fn new_v2(client: &'a MedousaClient, stream_path: impl Into<String>) -> Self {
        Self::with_policy_v2(client, stream_path, ReconnectPolicy::default())
    }

    pub fn with_policy_v2(
        client: &'a MedousaClient,
        stream_path: impl Into<String>,
        policy: ReconnectPolicy,
    ) -> Self {
        Self::with_wire(
            client,
            stream_path,
            policy,
            medousa_types::turn_stream::TURN_STREAM_V2_MEDIA_TYPE,
            |envelope| envelope.seq,
            |envelope| envelope.event.is_terminal(),
        )
    }
}

#[cfg(all(feature = "async", feature = "sse"))]
impl<'a, E> ReconnectingTurnStream<'a, E>
where
    E: DeserializeOwned + Send + 'a,
{
    fn with_wire(
        client: &'a MedousaClient,
        stream_path: impl Into<String>,
        policy: ReconnectPolicy,
        accept: &'static str,
        sequence: fn(&E) -> u64,
        terminal: fn(&E) -> bool,
    ) -> Self {
        Self {
            client,
            base_path: stream_path.into(),
            accept,
            sequence,
            terminal,
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

    fn open_stream(&self) -> EventStream<'a, E> {
        let path = stream_path_with_since(&self.base_path, self.last_seq);
        let byte_stream = self.client.transport().stream_sse_with_accept(
            self.client.base_url(),
            path,
            self.accept,
        );
        Box::pin(
            SseLineStream::new(byte_stream)
                .map(|line| line.and_then(|data| decode_sse_json(&data))),
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

    fn ingest(&mut self, event: E) -> Option<E> {
        let sequence = (self.sequence)(&event);
        if !apply_sequence(&mut self.last_seq, sequence) {
            return None;
        }
        if (self.terminal)(&event) {
            self.terminal_seen = true;
        }
        Some(event)
    }
}

#[cfg(all(feature = "async", feature = "sse"))]
impl<'a, E> Stream for ReconnectingTurnStream<'a, E>
where
    E: DeserializeOwned + Send + 'a,
{
    type Item = Result<E, SdkError>;

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
                        if self.terminal_seen {
                            self.phase = Phase::Done;
                        }
                        if let Some(out) = self.ingest(event) {
                            self.breaker.on_success();
                            self.reconnect_attempt = 0;
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
    apply_sequence(last_seq, event.seq)
}

#[cfg(all(feature = "async", feature = "sse"))]
pub fn apply_stream_seq_v2(last_seq: &mut u64, event: &TurnStreamEnvelopeV2) -> bool {
    apply_sequence(last_seq, event.seq)
}

#[cfg(all(feature = "async", feature = "sse"))]
fn apply_sequence(last_seq: &mut u64, sequence: u64) -> bool {
    if sequence != 0 && sequence <= *last_seq {
        return false;
    }
    if sequence != 0 {
        *last_seq = (*last_seq).max(sequence);
    }
    true
}
