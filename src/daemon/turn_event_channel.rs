//! Per-turn sequenced event channel: a broadcast sender backed by a monotonic
//! sequence counter and a bounded replay ring buffer.
//!
//! This is the backbone for exactly-once SSE replay. Every interactive-turn
//! event is stamped with a monotonic `seq` and retained in a ring buffer, so a
//! client that drops mid-turn (app backgrounded, network blip) can reattach
//! with `?since=<lastSeq>` and catch up on exactly the events it missed —
//! never replaying ones it already rendered (which is what produced the
//! duplicate bubbles), and never silently losing the terminal event.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::daemon_api::InteractiveTurnStreamEvent;

/// How many recent events to retain for replay. A single turn rarely emits more
/// than a few hundred events; 1024 leaves comfortable headroom for a client that
/// reconnects after a long background stint while bounding memory.
const DEFAULT_RING_CAP: usize = 1024;

struct Buffer {
    next_seq: u64,
    events: VecDeque<InteractiveTurnStreamEvent>,
    closed: bool,
}

/// Broadcast channel + replay buffer for one interactive turn's event stream.
pub struct TurnEventChannel {
    tx: broadcast::Sender<InteractiveTurnStreamEvent>,
    buffer: Mutex<Buffer>,
    ring_cap: usize,
}

impl TurnEventChannel {
    /// Create a new channel with the given live broadcast capacity. Returns an
    /// `Arc` because the channel is shared between the registry, the turn sink,
    /// and every attached SSE stream.
    pub fn new(broadcast_capacity: usize) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(broadcast_capacity);
        Arc::new(Self {
            tx,
            buffer: Mutex::new(Buffer {
                // seq starts at 1 so a client default of lastSeq=0 replays everything.
                next_seq: 1,
                events: VecDeque::new(),
                closed: false,
            }),
            ring_cap: DEFAULT_RING_CAP,
        })
    }

    /// Subscribe to live events. Used by each attached SSE stream.
    pub fn subscribe(&self) -> broadcast::Receiver<InteractiveTurnStreamEvent> {
        self.tx.subscribe()
    }

    /// Stamp the event with the next monotonic seq, retain it in the ring
    /// buffer, then broadcast to live subscribers. Buffer push happens-before
    /// the broadcast send so a subscriber can never observe a live event that
    /// is missing from the replay buffer.
    pub fn publish(&self, mut event: InteractiveTurnStreamEvent) {
        let mut buf = match self.buffer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        event.seq = buf.next_seq;
        buf.next_seq = buf.next_seq.saturating_add(1);
        buf.events.push_back(event.clone());
        while buf.events.len() > self.ring_cap {
            buf.events.pop_front();
        }
        drop(buf);
        let _ = self.tx.send(event);
    }

    /// All buffered events with `seq > since`, in order. Used both for the
    /// initial replay on attach and to recover from broadcast lag / closure.
    pub fn snapshot_since(&self, since: u64) -> Vec<InteractiveTurnStreamEvent> {
        let buf = match self.buffer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        buf.events
            .iter()
            .filter(|event| event.seq > since)
            .cloned()
            .collect()
    }

    /// Mark the turn finished. The buffer is retained (for the registry's
    /// post-run grace window) so a client reconnecting right at the end still
    /// replays the terminal event.
    pub fn mark_closed(&self) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.closed = true;
        }
    }

    pub fn is_closed(&self) -> bool {
        self.buffer.lock().map(|buf| buf.closed).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev() -> InteractiveTurnStreamEvent {
        crate::interactive_turn_runtime::status_stream_event("turn", "phase", "msg").unwrap()
    }

    #[test]
    fn publish_stamps_monotonic_seq_from_one() {
        let ch = TurnEventChannel::new(8);
        ch.publish(ev());
        ch.publish(ev());
        ch.publish(ev());
        let seqs: Vec<u64> = ch.snapshot_since(0).iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3]);
    }

    #[test]
    fn snapshot_since_returns_only_newer() {
        let ch = TurnEventChannel::new(8);
        for _ in 0..5 {
            ch.publish(ev());
        }
        let seqs: Vec<u64> = ch.snapshot_since(3).iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![4, 5]);
        assert!(ch.snapshot_since(5).is_empty());
    }

    #[test]
    fn ring_buffer_evicts_oldest_past_cap() {
        let ch = TurnEventChannel::new(8);
        let total = DEFAULT_RING_CAP + 76;
        for _ in 0..total {
            ch.publish(ev());
        }
        let retained = ch.snapshot_since(0);
        assert_eq!(retained.len(), DEFAULT_RING_CAP);
        // Oldest retained is the first that wasn't evicted.
        assert_eq!(retained.first().unwrap().seq, (total - DEFAULT_RING_CAP + 1) as u64);
        assert_eq!(retained.last().unwrap().seq, total as u64);
    }

    #[test]
    fn closed_flag_toggles() {
        let ch = TurnEventChannel::new(8);
        assert!(!ch.is_closed());
        ch.mark_closed();
        assert!(ch.is_closed());
    }
}
