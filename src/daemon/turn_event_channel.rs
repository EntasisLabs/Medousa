//! Per-turn live SSE fan-out channel. Replay durability lives on the
//! per-turn [`TurnEventLog`] spine; this type broadcasts pre-sequenced events only.

use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::daemon_api::InteractiveTurnStreamEvent;

struct ChannelState {
    closed: bool,
}

/// Broadcast channel for one interactive turn's live event stream.
pub struct TurnEventChannel {
    tx: broadcast::Sender<InteractiveTurnStreamEvent>,
    state: Mutex<ChannelState>,
}

impl TurnEventChannel {
    /// Create a new channel with the given live broadcast capacity.
    pub fn new(broadcast_capacity: usize) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(broadcast_capacity);
        Arc::new(Self {
            tx,
            state: Mutex::new(ChannelState { closed: false }),
        })
    }

    /// Subscribe to live events. Used by each attached SSE stream.
    pub fn subscribe(&self) -> broadcast::Receiver<InteractiveTurnStreamEvent> {
        self.tx.subscribe()
    }

    /// Broadcast a pre-sequenced event to live SSE subscribers.
    pub fn publish(&self, event: InteractiveTurnStreamEvent) {
        debug_assert!(event.seq > 0, "SSE events must carry spine-assigned seq");
        let _ = self.tx.send(event);
    }

    /// Mark the turn finished while the registry retains replay state.
    pub fn mark_closed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
    }

    pub fn is_closed(&self) -> bool {
        self.state.lock().map(|state| state.closed).unwrap_or(true)
    }
}

#[cfg(test)]
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
        let mut rx = ch.subscribe();
        ch.publish(ev(1));
        let got = rx.try_recv().expect("live event");
        assert_eq!(got.seq, 1);
    }

    #[test]
    fn closed_flag_toggles() {
        let ch = TurnEventChannel::new(8);
        assert!(!ch.is_closed());
        ch.mark_closed();
        assert!(ch.is_closed());
    }
}
