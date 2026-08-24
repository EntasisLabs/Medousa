//! Shared turn-journal recovery over the canonical daemon session store.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use medousa_engine::{RecoveredTurn, StoreError, TurnEventLog, TurnStorePort, UpsertOutcome};
use medousa_types::session::ConversationTurn;

use crate::session_storage::SessionId;
use crate::session_store::{SessionStore, TranscriptAppend};

/// Canonical session-store adapter used by every persistent daemon deployment.
pub struct SessionStoreTurnStore {
    store: Arc<dyn SessionStore>,
}

impl SessionStoreTurnStore {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl TurnStorePort for SessionStoreTurnStore {
    async fn upsert_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        turn: ConversationTurn,
    ) -> Result<UpsertOutcome, StoreError> {
        if self.turn_exists(session_id, turn_id).await? {
            return Ok(UpsertOutcome::AlreadyPresent);
        }
        let session_id =
            SessionId::parse(session_id).map_err(|error| StoreError(error.to_string()))?;
        let caused_by = crate::workshop_authority::execution_ref(session_id.as_str(), turn_id)
            .map_err(StoreError)?;
        self.store
            .append_transcript_batch(
                &session_id,
                &[TranscriptAppend::native(turn, Some(caused_by))],
            )
            .await
            .map_err(|error| StoreError(error.to_string()))?;
        Ok(UpsertOutcome::Inserted)
    }

    async fn turn_exists(&self, session_id: &str, turn_id: &str) -> Result<bool, StoreError> {
        let session_id =
            SessionId::parse(session_id).map_err(|error| StoreError(error.to_string()))?;
        Ok(self
            .store
            .load_transcript_entries(&session_id)
            .iter()
            .any(|entry| {
                entry.turn.role != "user"
                    && entry.caused_by.as_ref().is_some_and(|execution| {
                        execution.session_id == session_id
                            && execution.execution_id.as_str() == turn_id
                    })
            }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecoveryReport {
    pub session_id: String,
    pub turn_id: String,
    pub inserted: usize,
    pub already_present: usize,
}

/// Persist one interrupted terminal journal and only then write its commit
/// fence. Session identity must come from the journal; recovery never guesses a
/// fallback workshop or conversation.
pub async fn recover_journal_item(
    root: &Path,
    item: RecoveredTurn,
    store: &dyn TurnStorePort,
) -> anyhow::Result<TurnRecoveryReport> {
    let session_id = item.session_id.clone().ok_or_else(|| {
        anyhow::anyhow!("turn journal '{}' has no session identity", item.turn_id)
    })?;
    let mut inserted = 0;
    let mut already_present = 0;
    for turn in item.history {
        match store
            .upsert_turn(&session_id, &item.turn_id, turn)
            .await
            .map_err(anyhow::Error::new)?
        {
            UpsertOutcome::Inserted => inserted += 1,
            UpsertOutcome::AlreadyPresent => already_present += 1,
        }
    }

    TurnEventLog::open_in(root, item.envelope)
        .and_then(|log| log.mark_committed().map(|_| ()))
        .map_err(anyhow::Error::new)?;

    Ok(TurnRecoveryReport {
        session_id,
        turn_id: item.turn_id,
        inserted,
        already_present,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_engine::{Principal, TurnEnvelope};

    struct UnexpectedStore;

    #[async_trait]
    impl TurnStorePort for UnexpectedStore {
        async fn upsert_turn(
            &self,
            _session_id: &str,
            _turn_id: &str,
            _turn: ConversationTurn,
        ) -> Result<UpsertOutcome, StoreError> {
            panic!("sessionless recovery must not write a transcript")
        }

        async fn turn_exists(&self, _session_id: &str, _turn_id: &str) -> Result<bool, StoreError> {
            panic!("sessionless recovery must not inspect a fallback transcript")
        }
    }

    #[tokio::test]
    async fn recovery_refuses_to_guess_a_session() {
        let root = tempfile::tempdir().expect("turn recovery root");
        let item = RecoveredTurn {
            turn_id: "turn-without-session".to_string(),
            session_id: None,
            envelope: TurnEnvelope::new("turn-without-session", Principal::operator()),
            history: Vec::new(),
        };

        let error = recover_journal_item(root.path(), item, &UnexpectedStore)
            .await
            .expect_err("sessionless journal recovery");
        assert!(error.to_string().contains("has no session identity"));
    }
}
