//! In-process turn work records (Phase 1 bus).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::turn_continuation::StoredDeliveryTarget;

static STORE: Lazy<RwLock<Arc<TurnWorkerStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(TurnWorkerStore::default())));

pub fn turn_worker_store() -> Arc<TurnWorkerStore> {
    STORE.read().expect("turn worker store lock").clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnWorkStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnWorkRecord {
    pub work_id: String,
    pub session_id: String,
    pub parent_turn_correlation_id: Option<String>,
    pub intent: String,
    pub task_prompt: String,
    pub status: TurnWorkStatus,
    pub result_text: Option<String>,
    pub tool_names: Vec<String>,
    pub termination_reason: Option<String>,
    pub error: Option<String>,
    pub user_ack: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub delivery_target: Option<StoredDeliveryTarget>,
    pub parent_user_prompt: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct TurnWorkerStore {
    records: RwLock<HashMap<String, TurnWorkRecord>>,
}

impl TurnWorkerStore {
    pub fn insert(&self, record: TurnWorkRecord) {
        let mut guard = self.records.write().expect("turn worker records");
        guard.insert(record.work_id.clone(), record);
    }

    pub fn get(&self, work_id: &str) -> Option<TurnWorkRecord> {
        self.records
            .read()
            .expect("turn worker records")
            .get(work_id)
            .cloned()
    }

    pub fn list_for_session(&self, session_id: &str) -> Vec<TurnWorkRecord> {
        self.records
            .read()
            .expect("turn worker records")
            .values()
            .filter(|record| record.session_id == session_id)
            .cloned()
            .collect()
    }

    pub fn update<F>(&self, work_id: &str, update: F) -> Option<TurnWorkRecord>
    where
        F: FnOnce(&mut TurnWorkRecord),
    {
        let mut guard = self.records.write().expect("turn worker records");
        let record = guard.get_mut(work_id)?;
        update(record);
        record.updated_at = Utc::now();
        Some(record.clone())
    }
}

