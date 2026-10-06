//! Bounded, content-free transport evidence for manual network qualification.
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use iroh::endpoint::Connection;
use n0_future::time::Instant;
use serde::Serialize;

const CAPACITY: usize = 128;

#[derive(Debug, Clone, Serialize)]
pub struct TransportDiagnostic {
    pub sequence: u64,
    pub phase: &'static str,
    pub outcome: &'static str,
    pub elapsed_ms: u64,
    pub peer: Option<String>,
    pub path: Option<&'static str>,
    pub rtt_ms: Option<u64>,
}

#[derive(Default)]
struct History {
    sequence: u64,
    events: VecDeque<TransportDiagnostic>,
}

impl History {
    fn push(&mut self, mut event: TransportDiagnostic) {
        self.sequence += 1;
        event.sequence = self.sequence;
        if self.events.len() == CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

fn history() -> &'static Mutex<History> {
    static HISTORY: OnceLock<Mutex<History>> = OnceLock::new();
    HISTORY.get_or_init(|| Mutex::new(History::default()))
}

/// Recent events from this client process. No requests, credentials, tickets,
/// URLs, bodies, or peer-supplied close reasons are retained.
pub fn transport_diagnostics() -> Vec<TransportDiagnostic> {
    history()
        .lock()
        .map(|history| history.events.iter().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn record(
    phase: &'static str,
    outcome: &'static str,
    started: Instant,
    connection: Option<&Connection>,
) {
    let paths = connection.map(Connection::paths);
    let selected = paths
        .as_ref()
        .and_then(|paths| paths.iter().find(|path| path.is_selected()));
    let event = TransportDiagnostic {
        sequence: 0,
        phase,
        outcome,
        elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        peer: connection.map(|connection| {
            connection
                .remote_id()
                .to_string()
                .chars()
                .take(12)
                .collect()
        }),
        path: selected.as_ref().map(|path| {
            if path.is_ip() {
                "direct"
            } else if path.is_relay() {
                "relay"
            } else {
                "custom"
            }
        }),
        rtt_ms: selected.map(|path| path.rtt().as_millis().min(u64::MAX as u128) as u64),
    };
    if let Ok(mut history) = history().lock() {
        history.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_bounded_and_only_records_transport_metadata() {
        let mut history = History::default();
        for _ in 0..CAPACITY + 5 {
            history.push(TransportDiagnostic {
                sequence: 0,
                phase: "dial",
                outcome: "failed",
                elapsed_ms: 10,
                peer: None,
                path: None,
                rtt_ms: None,
            });
        }
        assert_eq!(history.events.len(), CAPACITY);
        assert_eq!(history.events.front().unwrap().sequence, 6);
        let json = serde_json::to_value(history.events.back().unwrap()).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 7);
        for key in ["ticket", "url", "headers", "body", "error", "close_reason"] {
            assert!(json.get(key).is_none());
        }
    }
}
