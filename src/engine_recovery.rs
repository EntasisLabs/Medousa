//! Startup replay for uncommitted turn journals (kill -9 mid-turn recovery).

use std::path::PathBuf;

use medousa_engine::{
    TurnEventLog, TurnStorePort, UpsertOutcome, configure_log_root, default_log_root,
    recover_uncommitted, TURN_LOG_DIR,
};

use crate::engine_adapters::SessionTurnStore;
use crate::paths;
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};

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
        tracing::warn!(session_id, "rejected recovery ledger write for deleting session");
        return;
    };
    let mut map = load_recovery_ledger();
    let entry = map
        .entry(session_id.to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let Some(arr) = entry.as_array_mut()
        && !arr.iter().any(|id| id.as_str() == Some(turn_id)) {
            arr.push(serde_json::Value::String(turn_id.to_string()));
        }
    save_recovery_ledger(&map);
}

pub fn delete_session_recovery(session_id: &str) -> Result<(), String> {
    let root_path = paths::medousa_data_dir().join(TURN_LOG_DIR);
    let root = StoreRoot::open_or_create(&root_path).map_err(|error| error.to_string())?;
    let ledger_path = StorePath::parse("recovery_ledger.json").expect("static recovery ledger path");
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
        root.remove_file(&entry.path).map_err(|error| error.to_string())?;
        root.remove_file(&marker).map_err(|error| error.to_string())?;
    }

    let verify_ledger = root.read(&ledger_path).map_err(|error| error.to_string())?;
    let verify_ledger = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(
        &verify_ledger,
    )
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
    if recovered.is_empty() {
        return;
    }

    tracing::info!(count = recovered.len(), "recovering uncommitted turn journals");

    let store = SessionTurnStore;
    for item in recovered {
        let session_id = item
            .session_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let Ok((_typed_session_id, _mutation)) =
            crate::session_deletion::acquire_mutation_for_str(&session_id)
        else {
            let _ = delete_session_recovery(&session_id);
            continue;
        };
        let turn_id = item.turn_id.clone();
        let mut committed_any = false;

        for turn in item.history {
            let outcome = store
                .upsert_turn(&session_id, &turn_id, turn)
                .await;
            match outcome {
                Ok(UpsertOutcome::Inserted) => {
                    committed_any = true;
                    tracing::info!(
                        session_id = %session_id,
                        turn_id = %turn_id,
                        "recovered uncommitted turn body"
                    );
                }
                Ok(UpsertOutcome::AlreadyPresent) => {
                    tracing::debug!(
                        session_id = %session_id,
                        turn_id = %turn_id,
                        "recovery skipped duplicate turn body"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        session_id = %session_id,
                        turn_id = %turn_id,
                        error = %err,
                        "turn recovery persist failed"
                    );
                }
            }
        }

        if (committed_any || recovery_ledger_contains(&session_id, &turn_id))
            && let Ok(log) = TurnEventLog::open_in(&root, item.envelope) {
                log.mark_committed();
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_engine::{Principal, TurnEnvelope, TurnEvent, TurnStorePort};
    #[tokio::test]
    async fn recovery_replays_uncommitted_journal_and_marks_committed() {
        let temp = tempfile::tempdir().expect("temporary recovery root");
        let root = temp.path().join("turn-logs");
        configure_log_root(root.clone());
        let envelope = TurnEnvelope::new("turn-recover-1", Principal::operator())
            .with_surface(Some(medousa_engine::TurnSurface {
                channel_id: Some("session-recover".into()),
                ..Default::default()
            }));
        {
            let log = TurnEventLog::open_in(&root, envelope.clone()).unwrap();
            log.append(TurnEvent::FinalResponse {
                text: "recovered answer".into(),
                tool_names: vec![],
                parts: vec![],
                committed_at: Utc::now(),
            });
        }

        let pending = recover_uncommitted(&root);
        assert_eq!(pending.len(), 1);

        let store = SessionTurnStore;
        let session_id = "session-recover".to_string();
        for turn in pending[0].history.clone() {
            store
                .upsert_turn(&session_id, "turn-recover-1", turn)
                .await
                .expect("upsert");
        }
        if let Ok(log) = TurnEventLog::open_in(&root, envelope) {
            log.mark_committed();
        }

        assert!(recover_uncommitted(&root).is_empty());
        assert!(recovery_ledger_contains("session-recover", "turn-recover-1"));
    }
}
