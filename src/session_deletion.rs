//! Durable session-deletion tombstones and in-process mutation exclusion.
//!
//! A delete persists its tombstone while holding the coordinator lock, then
//! waits for every mutation lease acquired before the tombstone to drain.
//! Mutations after the tombstone fail closed, including after daemon restart.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};

use chrono::Utc;
use once_cell::sync::Lazy;

use crate::session_storage::{SessionFileStore, SessionId};

pub use medousa_types::daemon_api::{SessionDeletionStatus, SessionDeletionSurfaceResult};

const DELETION_LAYOUT_VERSION: u8 = 1;
const MAX_DELETION_RECORD_BYTES: u64 = 1024 * 1024;
const MAX_DELETION_RECORDS: usize = 100_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionDeletionRecord {
    pub layout_version: u8,
    pub deletion_id: String,
    pub session_id: String,
    pub purge_locus: bool,
    pub status: SessionDeletionStatus,
    pub attempt_count: u32,
    pub created_at_utc: chrono::DateTime<Utc>,
    pub updated_at_utc: chrono::DateTime<Utc>,
    #[serde(default)]
    pub surfaces: Vec<SessionDeletionSurfaceResult>,
    #[serde(default)]
    pub cancelled_active_turn: bool,
    #[serde(default)]
    pub locus_purged: bool,
    #[serde(default)]
    pub locus_nodes_deleted: usize,
}

impl SessionDeletionRecord {
    fn new(session_id: &SessionId, purge_locus: bool) -> Self {
        let now = Utc::now();
        Self {
            layout_version: DELETION_LAYOUT_VERSION,
            deletion_id: format!("del_{}", uuid::Uuid::new_v4().simple()),
            session_id: session_id.to_string(),
            purge_locus,
            status: SessionDeletionStatus::Deleting,
            attempt_count: 1,
            created_at_utc: now,
            updated_at_utc: now,
            surfaces: Vec::new(),
            cancelled_active_turn: false,
            locus_purged: false,
            locus_nodes_deleted: 0,
        }
    }
}

#[derive(Debug, Default)]
struct SessionState {
    loaded: bool,
    tombstoned: bool,
    deletion_active: bool,
    active_mutations: usize,
}

#[derive(Debug, Default)]
struct CoordinatorState {
    sessions: HashMap<String, SessionState>,
}

pub struct SessionDeletionCoordinator {
    records: SessionFileStore,
    state: Mutex<CoordinatorState>,
    changed: Condvar,
}

impl SessionDeletionCoordinator {
    pub fn new(data_root: PathBuf) -> Self {
        Self {
            records: SessionFileStore::new(data_root.join("session_deletions"), "json"),
            state: Mutex::new(CoordinatorState::default()),
            changed: Condvar::new(),
        }
    }

    pub fn acquire_mutation(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionMutationLease<'_>, String> {
        let mut state = self.lock_state()?;
        self.load_tombstone_locked(&mut state, session_id)?;
        let session = state
            .sessions
            .get_mut(session_id.as_str())
            .expect("session state was initialized");
        if session.tombstoned {
            return Err("session is deleting or deleted".to_string());
        }
        session.active_mutations = session.active_mutations.saturating_add(1);
        Ok(SessionMutationLease {
            coordinator: self,
            session_id: session_id.to_string(),
        })
    }

    pub fn begin_deletion(
        &self,
        session_id: &SessionId,
        purge_locus: bool,
    ) -> Result<BeginDeletion<'_>, String> {
        let mut state = self.lock_state()?;
        self.load_tombstone_locked(&mut state, session_id)?;

        let existing = self.read_record(session_id)?;
        let session = state
            .sessions
            .get_mut(session_id.as_str())
            .expect("session state was initialized");
        if session.deletion_active {
            return existing
                .map(BeginDeletion::AlreadyActive)
                .ok_or_else(|| "deletion is active without a durable record".to_string());
        }
        if let Some(record) = existing.as_ref()
            && record.status == SessionDeletionStatus::Complete
        {
            return Ok(BeginDeletion::Complete(record.clone()));
        }

        session.tombstoned = true;
        session.deletion_active = true;
        let is_retry = existing.is_some();
        let mut record =
            existing.unwrap_or_else(|| SessionDeletionRecord::new(session_id, purge_locus));
        if is_retry {
            record.attempt_count = record.attempt_count.saturating_add(1);
        }
        record.purge_locus |= purge_locus;
        record.status = SessionDeletionStatus::Deleting;
        record.updated_at_utc = Utc::now();
        if let Err(error) = self.write_record(session_id, &record) {
            session.deletion_active = false;
            if record.attempt_count == 1 && record.surfaces.is_empty() {
                session.tombstoned = false;
            }
            return Err(error);
        }

        while state
            .sessions
            .get(session_id.as_str())
            .is_some_and(|session| session.active_mutations > 0)
        {
            state = self
                .changed
                .wait(state)
                .map_err(|_| "session deletion coordinator lock poisoned".to_string())?;
        }

        Ok(BeginDeletion::Owner(SessionDeletionLease {
            coordinator: self,
            session_id: session_id.clone(),
            record,
            finished: false,
        }))
    }

    pub fn record(&self, session_id: &SessionId) -> Result<Option<SessionDeletionRecord>, String> {
        self.read_record(session_id)
    }

    pub fn find_record(&self, deletion_id: &str) -> Result<Option<SessionDeletionRecord>, String> {
        if deletion_id.len() != 36
            || !deletion_id.starts_with("del_")
            || !deletion_id.as_bytes()[4..]
                .iter()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("invalid deletion id".to_string());
        }
        let entries = self.records.list().map_err(|error| error.to_string())?;
        if entries.len() > MAX_DELETION_RECORDS {
            return Err("session deletion record limit exceeded".to_string());
        }
        for entry in entries {
            let bytes = self
                .records
                .read_entry_limited(&entry, MAX_DELETION_RECORD_BYTES)
                .map_err(|error| error.to_string())?;
            let record = serde_json::from_slice::<SessionDeletionRecord>(&bytes)
                .map_err(|_| "session deletion record is corrupt".to_string())?;
            if record.deletion_id == deletion_id {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }

    fn load_tombstone_locked(
        &self,
        state: &mut CoordinatorState,
        session_id: &SessionId,
    ) -> Result<(), String> {
        if state
            .sessions
            .get(session_id.as_str())
            .is_some_and(|session| session.loaded)
        {
            return Ok(());
        }
        let tombstoned = self.read_record(session_id)?.is_some();
        let session = state.sessions.entry(session_id.to_string()).or_default();
        session.loaded = true;
        session.tombstoned = tombstoned;
        Ok(())
    }

    fn read_record(&self, session_id: &SessionId) -> Result<Option<SessionDeletionRecord>, String> {
        match self
            .records
            .read_limited(session_id, MAX_DELETION_RECORD_BYTES)
        {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "session deletion record is corrupt".to_string()),
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(format!("read session deletion record: {error}")),
        }
    }

    fn write_record(
        &self,
        session_id: &SessionId,
        record: &SessionDeletionRecord,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(record)
            .map_err(|_| "encode session deletion record".to_string())?;
        self.records
            .atomic_write(session_id, &bytes)
            .map_err(|error| format!("write session deletion record: {error}"))
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, CoordinatorState>, String> {
        self.state
            .lock()
            .map_err(|_| "session deletion coordinator lock poisoned".to_string())
    }

    fn release_mutation(&self, session_id: &str) {
        if let Ok(mut state) = self.state.lock()
            && let Some(session) = state.sessions.get_mut(session_id)
        {
            session.active_mutations = session.active_mutations.saturating_sub(1);
            if session.active_mutations == 0 {
                self.changed.notify_all();
            }
        }
    }

    fn release_deletion(&self, session_id: &SessionId) {
        if let Ok(mut state) = self.state.lock()
            && let Some(session) = state.sessions.get_mut(session_id.as_str())
        {
            session.deletion_active = false;
            self.changed.notify_all();
        }
    }
}

pub enum BeginDeletion<'a> {
    Owner(SessionDeletionLease<'a>),
    AlreadyActive(SessionDeletionRecord),
    Complete(SessionDeletionRecord),
}

pub struct SessionMutationLease<'a> {
    coordinator: &'a SessionDeletionCoordinator,
    session_id: String,
}

impl Drop for SessionMutationLease<'_> {
    fn drop(&mut self) {
        self.coordinator.release_mutation(&self.session_id);
    }
}

pub struct SessionDeletionLease<'a> {
    coordinator: &'a SessionDeletionCoordinator,
    session_id: SessionId,
    record: SessionDeletionRecord,
    finished: bool,
}

impl SessionDeletionLease<'_> {
    pub fn record(&self) -> &SessionDeletionRecord {
        &self.record
    }

    pub fn record_surface(&mut self, result: SessionDeletionSurfaceResult) -> Result<(), String> {
        self.record
            .surfaces
            .retain(|entry| entry.surface != result.surface);
        self.record.surfaces.push(result);
        self.record.updated_at_utc = Utc::now();
        self.coordinator
            .write_record(&self.session_id, &self.record)
    }

    pub fn record_runtime_outcome(
        &mut self,
        cancelled_active_turn: Option<bool>,
        locus: Option<(bool, usize)>,
    ) -> Result<(), String> {
        if let Some(cancelled) = cancelled_active_turn {
            self.record.cancelled_active_turn |= cancelled;
        }
        if let Some((purged, deleted)) = locus {
            self.record.locus_purged |= purged;
            self.record.locus_nodes_deleted = self.record.locus_nodes_deleted.max(deleted);
        }
        self.record.updated_at_utc = Utc::now();
        self.coordinator
            .write_record(&self.session_id, &self.record)
    }

    pub fn finish(
        mut self,
        status: SessionDeletionStatus,
    ) -> Result<SessionDeletionRecord, String> {
        self.record.status = status;
        self.record.updated_at_utc = Utc::now();
        self.coordinator
            .write_record(&self.session_id, &self.record)?;
        self.finished = true;
        self.coordinator.release_deletion(&self.session_id);
        Ok(self.record.clone())
    }
}

impl Drop for SessionDeletionLease<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.coordinator.release_deletion(&self.session_id);
        }
    }
}

static SESSION_DELETIONS: Lazy<SessionDeletionCoordinator> =
    Lazy::new(|| SessionDeletionCoordinator::new(crate::paths::medousa_data_dir()));

pub fn coordinator() -> &'static SessionDeletionCoordinator {
    &SESSION_DELETIONS
}

pub fn acquire_mutation(session_id: &SessionId) -> Result<SessionMutationLease<'static>, String> {
    coordinator().acquire_mutation(session_id)
}

pub fn acquire_mutation_for_str(
    session_id: &str,
) -> Result<(SessionId, SessionMutationLease<'static>), String> {
    let session_id = SessionId::parse(session_id).map_err(|error| error.to_string())?;
    let lease = acquire_mutation(&session_id)?;
    Ok((session_id, lease))
}

pub fn records_root(data_root: &Path) -> PathBuf {
    data_root.join("session_deletions")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    fn id() -> SessionId {
        SessionId::parse("session-delete-coordinator-test").unwrap()
    }

    #[test]
    fn tombstone_survives_fresh_coordinator_and_rejects_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = SessionDeletionCoordinator::new(temp.path().to_path_buf());
        let lease = match coordinator.begin_deletion(&id(), true).unwrap() {
            BeginDeletion::Owner(lease) => lease,
            _ => panic!("first delete must own the operation"),
        };
        lease
            .finish(SessionDeletionStatus::RetryablePartial)
            .unwrap();

        let fresh = SessionDeletionCoordinator::new(temp.path().to_path_buf());
        assert!(fresh.acquire_mutation(&id()).is_err());
        let record = fresh.record(&id()).unwrap().unwrap();
        assert_eq!(record.attempt_count, 1);
    }

    #[test]
    fn delete_waits_for_preexisting_mutation_and_denies_new_ones() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = Arc::new(SessionDeletionCoordinator::new(temp.path().to_path_buf()));
        let mutation = coordinator.acquire_mutation(&id()).unwrap();
        let delete_coordinator = Arc::clone(&coordinator);
        let handle = thread::spawn(move || {
            let result = delete_coordinator.begin_deletion(&id(), false).unwrap();
            matches!(result, BeginDeletion::Owner(_))
        });
        for _ in 0..100 {
            if coordinator.record(&id()).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(coordinator.acquire_mutation(&id()).is_err());
        assert!(!handle.is_finished());
        drop(mutation);
        assert!(handle.join().unwrap());
    }

    #[test]
    fn duplicate_delete_reuses_durable_deletion_id() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = SessionDeletionCoordinator::new(temp.path().to_path_buf());
        let lease = match coordinator.begin_deletion(&id(), false).unwrap() {
            BeginDeletion::Owner(lease) => lease,
            _ => panic!("first delete must own the operation"),
        };
        let deletion_id = lease.record().deletion_id.clone();
        let duplicate = coordinator.begin_deletion(&id(), false).unwrap();
        let BeginDeletion::AlreadyActive(record) = duplicate else {
            panic!("duplicate delete must join active record");
        };
        assert_eq!(record.deletion_id, deletion_id);
    }

    #[test]
    fn retry_reuses_id_replaces_surface_result_and_can_complete() {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = SessionDeletionCoordinator::new(temp.path().to_path_buf());
        let mut first = match coordinator.begin_deletion(&id(), false).unwrap() {
            BeginDeletion::Owner(lease) => lease,
            _ => panic!("first delete must own the operation"),
        };
        let deletion_id = first.record().deletion_id.clone();
        first
            .record_surface(SessionDeletionSurfaceResult {
                surface: "media".to_string(),
                deleted: false,
                reason_class: Some("io".to_string()),
            })
            .unwrap();
        first
            .finish(SessionDeletionStatus::RetryablePartial)
            .unwrap();

        let fresh = SessionDeletionCoordinator::new(temp.path().to_path_buf());
        let mut retry = match fresh.begin_deletion(&id(), false).unwrap() {
            BeginDeletion::Owner(lease) => lease,
            _ => panic!("partial deletion must be retryable"),
        };
        assert_eq!(retry.record().deletion_id, deletion_id);
        assert_eq!(retry.record().attempt_count, 2);
        retry
            .record_surface(SessionDeletionSurfaceResult {
                surface: "media".to_string(),
                deleted: true,
                reason_class: None,
            })
            .unwrap();
        let complete = retry.finish(SessionDeletionStatus::Complete).unwrap();
        assert_eq!(complete.status, SessionDeletionStatus::Complete);
        assert_eq!(complete.surfaces.len(), 1);
        assert!(complete.surfaces[0].deleted);
    }
}
