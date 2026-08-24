//! Startup replay for uncommitted turn journals (kill -9 mid-turn recovery).

use std::path::PathBuf;

use medousa_engine::{
    TURN_LOG_DIR, TurnEventLog, configure_log_root, default_log_root, prune_committed,
    recover_uncommitted,
};

use crate::paths;
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};
use crate::turn_recovery::SessionStoreTurnStore;

const MAX_RECOVERY_LOG_BYTES: u64 = 128 * 1024 * 1024;

pub fn recovery_ledger_path() -> PathBuf {
    default_log_root().join("recovery_ledger.json")
}

pub fn load_recovery_ledger() -> serde_json::Map<String, serde_json::Value> {
    let path = recovery_ledger_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return serde_json::Map::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_recovery_ledger(map: &serde_json::Map<String, serde_json::Value>) {
    let path = recovery_ledger_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(path, json);
    }
}

pub fn recovery_ledger_contains(session_id: &str, turn_id: &str) -> bool {
    let map = load_recovery_ledger();
    map.get(session_id)
        .and_then(|v| v.as_array())
        .is_some_and(|arr| arr.iter().any(|id| id.as_str() == Some(turn_id)))
}

pub fn mark_recovery_ledger(session_id: &str, turn_id: &str) {
    let Ok((_session, _mutation)) = crate::session_deletion::acquire_mutation_for_str(session_id)
    else {
        tracing::warn!(
            session_id,
            "rejected recovery ledger write for deleting session"
        );
        return;
    };
    let mut map = load_recovery_ledger();
    let entry = map
        .entry(session_id.to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let Some(arr) = entry.as_array_mut()
        && !arr.iter().any(|id| id.as_str() == Some(turn_id))
    {
        arr.push(serde_json::Value::String(turn_id.to_string()));
    }
    save_recovery_ledger(&map);
}

pub fn delete_session_recovery(session_id: &str) -> Result<(), String> {
    let root_path = paths::medousa_data_dir().join(TURN_LOG_DIR);
    let root = StoreRoot::open_or_create(&root_path).map_err(|error| error.to_string())?;
    let ledger_path =
        StorePath::parse("recovery_ledger.json").expect("static recovery ledger path");
    let mut ledger = match root.read(&ledger_path) {
        Ok(bytes) => serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes)
            .map_err(|_| "recovery ledger is corrupt".to_string())?,
        Err(error) if error.is_not_found() => serde_json::Map::new(),
        Err(error) => return Err(error.to_string()),
    };
    ledger.remove(session_id);
    let ledger_bytes = serde_json::to_vec_pretty(&ledger).map_err(|error| error.to_string())?;
    root.atomic_write(&ledger_path, &ledger_bytes)
        .map_err(|error| error.to_string())?;

    for entry in root.list_root().map_err(|error| error.to_string())? {
        if entry.kind != StoreEntryKind::File || !entry.path.file_name().ends_with(".jsonl") {
            continue;
        }
        let bytes = root
            .read_limited(&entry.path, MAX_RECOVERY_LOG_BYTES)
            .map_err(|error| error.to_string())?;
        let belongs_to_session = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
            .filter_map(|line| {
                serde_json::from_slice::<medousa_engine::SequencedTurnEvent>(line).ok()
            })
            .any(|event| {
                event
                    .envelope
                    .surface
                    .as_ref()
                    .and_then(|surface| surface.channel_id.as_deref())
                    == Some(session_id)
            });
        if !belongs_to_session {
            continue;
        }
        let marker_name = format!(
            "{}.committed",
            entry.path.file_name().trim_end_matches(".jsonl")
        );
        let marker = StorePath::parse(&marker_name).map_err(|error| error.to_string())?;
        root.remove_file(&entry.path)
            .map_err(|error| error.to_string())?;
        root.remove_file(&marker)
            .map_err(|error| error.to_string())?;
    }

    let verify_ledger = root.read(&ledger_path).map_err(|error| error.to_string())?;
    let verify_ledger =
        serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&verify_ledger)
            .map_err(|_| "recovery ledger is corrupt".to_string())?;
    if verify_ledger.contains_key(session_id) {
        return Err("session recovery ledger entry remains after deletion".to_string());
    }
    Ok(())
}

/// Configure the engine journal root from the daemon data dir and replay any
/// uncommitted terminal turns into session history (idempotent by turn id).
pub async fn run_startup_turn_recovery() {
    let root = paths::medousa_data_dir().join(TURN_LOG_DIR);
    configure_log_root(root.clone());

    let recovered = recover_uncommitted(&root);
    if !recovered.is_empty() {
        tracing::info!(
            count = recovered.len(),
            "recovering uncommitted turn journals"
        );
    }

    let store = SessionStoreTurnStore::new(crate::session_store::get_session_store());
    for item in recovered {
        let Some(session_id) = item.session_id.clone() else {
            tracing::warn!(
                turn_id = %item.turn_id,
                "turn recovery refused a journal without session identity"
            );
            continue;
        };
        let Ok((_typed_session_id, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&session_id)
        else {
            let _ = delete_session_recovery(&session_id);
            continue;
        };
        let turn_id = item.turn_id.clone();
        if recovery_ledger_contains(&session_id, &turn_id) {
            if let Ok(log) = TurnEventLog::open_in(&root, item.envelope)
                && let Err(error) = log.mark_committed()
            {
                tracing::warn!(%turn_id, %error, "recovery commit marker write failed");
            }
            continue;
        }

        match crate::turn_recovery::recover_journal_item(&root, item, &store).await {
            Ok(report) => {
                mark_recovery_ledger(&session_id, &turn_id);
                tracing::info!(
                    session_id = %session_id,
                    turn_id = %turn_id,
                    inserted = report.inserted,
                    already_present = report.already_present,
                    "reconciled interrupted turn journal"
                );
            }
            Err(error) => tracing::warn!(
                session_id = %session_id,
                turn_id = %turn_id,
                %error,
                "turn recovery persist failed"
            ),
        }
    }

    match prune_committed(&root) {
        Ok(0) => {}
        Ok(count) => tracing::info!(count, "pruned committed turn journals"),
        Err(error) => tracing::warn!(%error, "failed to prune committed turn journals"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_engine::{
        Principal, StoreError, TurnEnvelope, TurnEvent, TurnStorePort, UpsertOutcome,
    };
    use medousa_types::session::ConversationTurn;

    #[derive(Default)]
    struct RecordingTurnStore(std::sync::Mutex<Vec<(String, String)>>);

    #[async_trait::async_trait]
    impl TurnStorePort for RecordingTurnStore {
        async fn upsert_turn(
            &self,
            session_id: &str,
            turn_id: &str,
            _turn: ConversationTurn,
        ) -> Result<UpsertOutcome, StoreError> {
            let mut turns = self.0.lock().expect("recording turn store");
            if turns
                .iter()
                .any(|(session, turn)| session == session_id && turn == turn_id)
            {
                return Ok(UpsertOutcome::AlreadyPresent);
            }
            turns.push((session_id.to_string(), turn_id.to_string()));
            Ok(UpsertOutcome::Inserted)
        }

        async fn turn_exists(&self, session_id: &str, turn_id: &str) -> Result<bool, StoreError> {
            Ok(self
                .0
                .lock()
                .expect("recording turn store")
                .iter()
                .any(|(session, turn)| session == session_id && turn == turn_id))
        }
    }

    #[tokio::test]
    async fn recovery_replays_uncommitted_journal_and_marks_committed() {
        let temp = tempfile::tempdir().expect("temporary recovery root");
        let root = temp.path().join("turn-logs");
        configure_log_root(root.clone());
        let envelope = TurnEnvelope::new("turn-recover-1", Principal::operator()).with_surface(
            Some(medousa_engine::TurnSurface {
                channel_id: Some("session-recover".into()),
                ..Default::default()
            }),
        );
        {
            let log = TurnEventLog::open_in(&root, envelope.clone()).unwrap();
            log.append(TurnEvent::FinalResponse {
                text: "recovered answer".into(),
                tool_names: vec![],
                parts: vec![],
                committed_at: Utc::now(),
            })
            .unwrap();
        }

        let mut pending = recover_uncommitted(&root);
        assert_eq!(pending.len(), 1);

        let report = crate::turn_recovery::recover_journal_item(
            &root,
            pending.remove(0),
            &RecordingTurnStore::default(),
        )
        .await
        .expect("recover journal");

        assert_eq!(report.session_id, "session-recover");
        assert_eq!(report.inserted, 1);
        assert!(recover_uncommitted(&root).is_empty());
    }
}
