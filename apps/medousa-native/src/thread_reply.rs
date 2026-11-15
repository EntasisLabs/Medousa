//! Per-work-item composer state. No UI dependency or automatic mutation replay.
use crate::backend::Command;

#[derive(Default)]
pub(crate) struct State {
    pub draft: String,
    pub notice: Option<String>,
    pending: Option<Pending>,
    unconfirmed: bool,
}
struct Pending {
    session: String,
    work_id: String,
    nonce: u64,
    message: String,
}
impl State {
    pub fn sending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn can_send(&self) -> bool {
        !self.sending() && !self.unconfirmed && !self.draft.trim().is_empty()
    }
    /// Only call for a selected, nonterminal bound turn-worker. Other execution
    /// sessions have no governed task-message endpoint and remain read-only.
    pub fn begin(&mut self, session: &str, work_id: &str, nonce: u64) -> Option<Command> {
        if !self.can_send() || session.trim().is_empty() || work_id.trim().is_empty() {
            return None;
        }
        let message = self.draft.trim().to_string();
        self.pending = Some(Pending {
            session: session.into(),
            work_id: work_id.into(),
            nonce,
            message: self.draft.clone(),
        });
        self.notice = None;
        Some(Command::ThreadReply {
            session: session.into(),
            work_id: work_id.into(),
            message,
            nonce,
        })
    }
    /// Returns false for stale/wrong-route receipts. Draft edits made in flight
    /// survive success. Failure preserves text and gates another send pending
    /// explicit reconciliation; a transport failure can follow a durable queue.
    pub fn complete(
        &mut self,
        session: &str,
        work_id: &str,
        nonce: u64,
        result: Result<(), String>,
    ) -> bool {
        let Some(pending) = &self.pending else {
            return false;
        };
        if pending.session != session || pending.work_id != work_id || pending.nonce != nonce {
            return false;
        }
        let pending = self.pending.take().unwrap();
        match result {
            Ok(()) => {
                if self.draft == pending.message {
                    self.draft.clear();
                }
                self.notice = Some("Reply queued for the worker.".into());
            }
            Err(message) => {
                self.unconfirmed = true;
                self.notice = Some(message);
            }
        }
        true
    }
    /// Explicit user reconciliation only, never a timer or blind polling hook.
    /// The caller must first review canonical thread history for delivery.
    pub fn acknowledge_reconciled(&mut self) {
        if !self.sending() {
            self.unconfirmed = false;
            self.notice = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reply_state_routes_receipts_preserves_edits_and_never_replays() {
        let mut state = State {
            draft: "first".into(),
            ..Default::default()
        };
        assert!(state.begin("parent", "worker", 1).is_some());
        assert!(state.sending());
        assert!(state.begin("parent", "worker", 2).is_none());
        assert!(!state.complete("other", "worker", 1, Ok(())));
        assert!(!state.complete("parent", "worker", 2, Ok(())));
        state.draft = "new text".into();
        assert!(state.complete("parent", "worker", 1, Ok(())));
        assert_eq!(state.draft, "new text");
        assert!(state.begin("parent", "worker", 2).is_some());
        assert!(state.complete("parent", "worker", 2, Err("Unknown".into())));
        assert_eq!(state.draft, "new text");
        assert!(!state.can_send());
        state.acknowledge_reconciled();
        assert!(state.can_send());
        assert!(state.begin("parent", "worker", 3).is_some());
        assert!(state.complete("parent", "worker", 3, Ok(())));
        assert_eq!(state.draft, "");
    }
}
