//! Daemon adapters for portable turn-ledger state.
//!
//! Record construction and loop discipline live in `medousa-runtime`; this
//! compatibility module owns filesystem persistence and GenAI transcript glue.

use std::sync::Arc;

use medousa_runtime::TurnLedgerSink;
pub use medousa_runtime::loop_state::*;

fn turn_ledger_files() -> crate::session_storage::SessionFileStore {
    crate::session_storage::SessionFileStore::new(
        crate::paths::medousa_data_dir().join("turn_ledger"),
        "jsonl",
    )
}

pub fn delete_turn_ledger(session_id: &crate::session_storage::SessionId) -> Result<(), String> {
    let files = turn_ledger_files();
    files
        .remove(session_id)
        .map_err(|error| format!("turn ledger delete failed: {error}"))?;
    if files
        .contains(session_id)
        .map_err(|error| format!("turn ledger verification failed: {error}"))?
    {
        return Err("turn ledger remains after deletion".to_string());
    }
    Ok(())
}

pub fn append_turn_ledger_record(
    session_id: &crate::session_storage::SessionId,
    record: &TurnLedgerRecord,
) {
    append_turn_ledger_record_to(&turn_ledger_files(), session_id, record);
}

fn append_turn_ledger_record_to(
    files: &crate::session_storage::SessionFileStore,
    session_id: &crate::session_storage::SessionId,
    record: &TurnLedgerRecord,
) {
    let Ok(_mutation) = crate::session_deletion::acquire_mutation(session_id) else {
        tracing::warn!(session_id = %session_id, "rejected ledger write for deleting session");
        return;
    };
    let mut record = record.clone();
    if record.active_profile_id.is_none() {
        record.active_profile_id = Some(crate::user_profiles::resolve_workshop_active_profile_id());
    }
    let Ok(mut line) = serde_json::to_vec(&record) else {
        return;
    };
    line.push(b'\n');
    let _ = files.append(session_id, &line);
}

pub struct SessionTurnLedgerSink {
    session_id: crate::session_storage::SessionId,
    files: crate::session_storage::SessionFileStore,
}

impl TurnLedgerSink for SessionTurnLedgerSink {
    fn persist(&self, record: &TurnLedgerRecord) {
        append_turn_ledger_record_to(&self.files, &self.session_id, record);
    }
}

pub fn session_turn_ledger_sink(session_id: Option<&str>) -> Option<Arc<dyn TurnLedgerSink>> {
    let session_id = crate::session_storage::SessionId::parse(session_id?).ok()?;
    Some(Arc::new(SessionTurnLedgerSink {
        session_id,
        files: turn_ledger_files(),
    }))
}

/// Compatibility helper for daemon code outside the foreground loop.
pub fn persist_ledger_record(session_id: Option<&str>, record: &TurnLedgerRecord) {
    if let Some(sink) = session_turn_ledger_sink(session_id) {
        sink.persist(record);
    }
}

#[cfg(test)]
mod tests {
    use medousa_engine::TurnScratchpad;

    use super::*;

    #[test]
    fn turn_ledger_adapter_stamps_active_profile_id() {
        let dir = tempfile::tempdir().unwrap();
        let _data_dir = crate::paths::scoped_test_data_dir(dir.path());
        let record = record_tool_round(
            1,
            1,
            &["cognition_memory_query".to_string()],
            &TurnScratchpad::default(),
        );
        assert!(record.active_profile_id.is_none());
        let session =
            crate::session_storage::SessionId::parse("test-ledger-profile-stamp").unwrap();
        let files = turn_ledger_files();
        append_turn_ledger_record_to(&files, &session, &record);
        let raw = files.read(&session).expect("ledger file");
        let parsed: TurnLedgerRecord =
            serde_json::from_slice(raw.split(|byte| *byte == b'\n').next().unwrap()).expect("json");
        assert!(parsed.active_profile_id.is_some());
        let _ = delete_turn_ledger(&session);
    }
}
