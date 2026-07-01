//! Startup replay for uncommitted turn journals (kill -9 mid-turn recovery).

use std::path::PathBuf;

use medousa_engine::{
    TurnEventLog, TurnStorePort, UpsertOutcome, configure_log_root, default_log_root,
    recover_uncommitted, TURN_LOG_DIR,
};

use crate::engine_adapters::SessionTurnStore;
use crate::paths;

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
    let mut map = load_recovery_ledger();
    let entry = map
        .entry(session_id.to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let Some(arr) = entry.as_array_mut() {
        if !arr.iter().any(|id| id.as_str() == Some(turn_id)) {
            arr.push(serde_json::Value::String(turn_id.to_string()));
        }
    }
    save_recovery_ledger(&map);
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

        if committed_any || recovery_ledger_contains(&session_id, &turn_id) {
            if let Ok(log) = TurnEventLog::open_in(&root, item.envelope) {
                log.mark_committed();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use medousa_engine::{Principal, TurnEnvelope, TurnEvent, TurnStorePort};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-recovery-test-{}",
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[tokio::test]
    async fn recovery_replays_uncommitted_journal_and_marks_committed() {
        let root = tmp_root();
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

        std::fs::remove_dir_all(&root).ok();
    }
}
