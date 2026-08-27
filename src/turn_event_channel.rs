//! Per-turn live SSE fan-out channel. Replay durability lives on the
//! per-turn [`TurnEventLog`] spine; this type broadcasts pre-sequenced events only.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::sync::{broadcast, watch};

use medousa_types::daemon_api::InteractiveTurnStreamEvent;
use medousa_types::{TurnStreamEnvelopeV2, TurnStreamEnvelopeV3};

#[derive(Debug, Clone)]
pub struct PublishedTurnEvent {
    pub(crate) seq: u64,
    pub v1: Option<InteractiveTurnStreamEvent>,
    pub v2: Option<TurnStreamEnvelopeV2>,
    pub v3: Option<TurnStreamEnvelopeV3>,
}

impl PublishedTurnEvent {
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

pub const MAX_TURN_STREAM_SUBSCRIBERS: usize = 64;

/// One admitted live subscriber. Dropping it returns its admission slot.
pub struct TurnEventSubscription {
    receiver: broadcast::Receiver<PublishedTurnEvent>,
    closed: watch::Receiver<bool>,
    active_subscribers: Arc<AtomicUsize>,
}

impl TurnEventSubscription {
    pub async fn recv(&mut self) -> Result<PublishedTurnEvent, broadcast::error::RecvError> {
        loop {
            match self.receiver.try_recv() {
                Ok(event) => return Ok(event),
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    return Err(broadcast::error::RecvError::Lagged(skipped));
                }
                Err(broadcast::error::TryRecvError::Closed) => {
                    return Err(broadcast::error::RecvError::Closed);
                }
                Err(broadcast::error::TryRecvError::Empty) => {}
            }

            if *self.closed.borrow() {
                return Err(broadcast::error::RecvError::Closed);
            }

            tokio::select! {
                event = self.receiver.recv() => return event,
                changed = self.closed.changed() => {
                    let _ = changed;
                    // Drain anything published before the close fence before
                    // exposing EOF to a live client.
                }
            }
        }
    }

    #[cfg(all(test, feature = "full-daemon"))]
    fn try_recv(&mut self) -> Result<PublishedTurnEvent, broadcast::error::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for TurnEventSubscription {
    fn drop(&mut self) {
        self.active_subscribers.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Broadcast channel for one interactive turn's live event stream.
pub struct TurnEventChannel {
    tx: broadcast::Sender<PublishedTurnEvent>,
    closed: AtomicBool,
    close_tx: watch::Sender<bool>,
    active_subscribers: Arc<AtomicUsize>,
    max_subscribers: usize,
}

impl TurnEventChannel {
    /// Create a new channel with the given live broadcast capacity.
    pub fn new(broadcast_capacity: usize) -> Arc<Self> {
        Self::with_limits(broadcast_capacity, MAX_TURN_STREAM_SUBSCRIBERS)
    }

    fn with_limits(broadcast_capacity: usize, max_subscribers: usize) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(broadcast_capacity);
        let (close_tx, _close_rx) = watch::channel(false);
        Arc::new(Self {
            tx,
            closed: AtomicBool::new(false),
            close_tx,
            active_subscribers: Arc::new(AtomicUsize::new(0)),
            max_subscribers,
        })
    }

    /// Admit a live SSE subscriber without allowing an unbounded receiver set.
    pub fn try_subscribe(&self) -> Option<TurnEventSubscription> {
        self.active_subscribers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < self.max_subscribers).then_some(active + 1)
            })
            .ok()?;
        Some(TurnEventSubscription {
            receiver: self.tx.subscribe(),
            closed: self.close_tx.subscribe(),
            active_subscribers: Arc::clone(&self.active_subscribers),
        })
    }

    /// Broadcast a pre-sequenced event to live SSE subscribers.
    pub fn publish(&self, event: InteractiveTurnStreamEvent) {
        debug_assert!(event.seq > 0, "SSE events must carry spine-assigned seq");
        let v2 = crate::sse_turn_projection::v1_to_v2(&event)
            .inspect_err(|error| tracing::error!(%error, "stream event has no v2 projection"))
            .ok();
        let _ = self.tx.send(PublishedTurnEvent {
            seq: event.seq,
            v1: Some(event),
            v2,
            v3: None,
        });
    }

    /// Broadcast the canonical v2 event and its frozen v1 compatibility view.
    pub fn publish_pair(&self, v1: InteractiveTurnStreamEvent, v2: TurnStreamEnvelopeV2) {
        debug_assert_eq!(v1.seq, v2.seq, "v1/v2 stream sequence must match");
        let _ = self.tx.send(PublishedTurnEvent {
            seq: v2.seq,
            v1: Some(v1),
            v2: Some(v2),
            v3: None,
        });
    }

    /// Broadcast one native V3 fact and any honest legacy compatibility views.
    pub fn publish_v3(&self, v3: TurnStreamEnvelopeV3, v2: Option<TurnStreamEnvelopeV2>) {
        debug_assert_eq!(
            v2.as_ref().map(|event| event.seq).unwrap_or(v3.seq),
            v3.seq,
            "v2/v3 stream sequence must match"
        );
        let v1 = v2.as_ref().map(crate::sse_turn_projection::v2_to_v1);
        let _ = self.tx.send(PublishedTurnEvent {
            seq: v3.seq,
            v1,
            v2,
            v3: Some(v3),
        });
    }

    /// Mark the turn finished while the registry retains replay state.
    pub fn mark_closed(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.close_tx.send_replace(true);
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

#[cfg(all(test, feature = "full-daemon"))]
mod tests {
    use super::*;

    fn ev(seq: u64) -> InteractiveTurnStreamEvent {
        let mut event =
            crate::interactive_turn_runtime::status_stream_event("turn", "phase", "msg").unwrap();
        event.seq = seq;
        event
    }

    #[test]
    fn publish_broadcasts_presequenced_events() {
        let ch = TurnEventChannel::new(8);
        let mut rx = ch.try_subscribe().unwrap();
        ch.publish(ev(1));
        let got = rx.try_recv().expect("live event");
        assert_eq!(got.seq(), 1);
        assert!(got.v1.is_some());
        assert!(got.v2.is_some());
        assert!(got.v3.is_none());
    }

    #[test]
    fn closed_flag_toggles() {
        let ch = TurnEventChannel::new(8);
        assert!(!ch.is_closed());
        ch.mark_closed();
        assert!(ch.is_closed());
    }

    #[tokio::test]
    async fn closing_wakes_an_attached_subscriber() {
        let ch = TurnEventChannel::new(8);
        let mut subscriber = ch.try_subscribe().unwrap();
        ch.mark_closed();
        assert!(matches!(
            subscriber.recv().await,
            Err(broadcast::error::RecvError::Closed)
        ));
    }

    #[tokio::test]
    async fn close_drains_already_published_events_before_eof() {
        let ch = TurnEventChannel::new(8);
        let mut subscriber = ch.try_subscribe().unwrap();
        ch.publish(ev(1));
        ch.mark_closed();

        assert_eq!(subscriber.recv().await.unwrap().seq(), 1);
        assert!(matches!(
            subscriber.recv().await,
            Err(broadcast::error::RecvError::Closed)
        ));
    }

    #[test]
    fn subscriber_admission_is_bounded_and_released_on_drop() {
        let ch = TurnEventChannel::with_limits(8, 2);
        let first = ch.try_subscribe().expect("first subscriber");
        let second = ch.try_subscribe().expect("second subscriber");
        assert!(ch.try_subscribe().is_none());

        drop(first);
        assert!(ch.try_subscribe().is_some());
        drop(second);
    }
}
