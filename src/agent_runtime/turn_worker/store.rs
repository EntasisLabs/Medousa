//! Durable turn work records (host/worker bus).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::agent_runtime::turn_context::WorkerHandoffCapsule;
use crate::session;
use crate::turn_continuation::StoredDeliveryTarget;

const TURN_WORKERS_FILE: &str = "workspace/turn_workers.json";
const MAX_ACTIVE_TURN_WORKERS: usize = 500;
use crate::workspace::retention::WorkspaceRetentionConfig;

static STORE: Lazy<Arc<TurnWorkerStore>> = Lazy::new(|| Arc::new(TurnWorkerStore::new()));

pub fn turn_worker_store() -> Arc<TurnWorkerStore> {
    STORE.clone()
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

fn default_worker_max_tool_rounds() -> usize {
    10
}

fn default_parent_stream_turn_id() -> u64 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnWorkRecord {
    pub work_id: String,
    pub session_id: String,
    pub parent_turn_correlation_id: Option<String>,
    #[serde(default = "default_parent_stream_turn_id")]
    pub parent_stream_turn_id: u64,
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
    /// Tool-round budget snapshotted from the host turn's operator settings at spawn.
    #[serde(default = "default_worker_max_tool_rounds")]
    pub max_tool_rounds: usize,
    pub delivery_target: Option<StoredDeliveryTarget>,
    pub parent_user_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_capsule: Option<WorkerHandoffCapsule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_scratch: Option<crate::agent_runtime::turn_context::TurnScratchpad>,
    /// Host synthesis delivered the worker result to the parent turn.
    #[serde(default)]
    pub synthesis_delivered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stasis_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_group_id: Option<String>,
    #[serde(default)]
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct TurnWorkerStore {
    records: Mutex<HashMap<String, TurnWorkRecord>>,
}

impl Default for TurnWorkerStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnWorkerStore {
    pub fn new() -> Self {
        let store = Self {
            records: Mutex::new(HashMap::new()),
        };
        store.reload_from_disk();
        store
    }

    fn path() -> PathBuf {
        session::medousa_data_dir().join(
            TURN_WORKERS_FILE
                .strip_prefix("workspace/")
                .unwrap_or(TURN_WORKERS_FILE),
        )
    }

    fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(session::medousa_data_dir().join("workspace"));
        let Ok(raw) = fs::read_to_string(Self::path()) else {
            return;
        };
        let Ok(map) = serde_json::from_str::<HashMap<String, TurnWorkRecord>>(&raw) else {
            return;
        };
        *self.records.lock().expect("turn worker records") = map;
    }

    fn write_map(map: &HashMap<String, TurnWorkRecord>) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(map)?;
        fs::write(path, body)
    }

    fn persist(&self, work_id: &str, stasis_job_id: Option<&str>) {
        let mut guard = self.records.lock().expect("turn worker records");
        Self::prune_map(&mut guard);
        let body = match serde_json::to_string_pretty(&*guard) {
            Ok(body) => body,
            Err(err) => {
                eprintln!("turn_worker_store: serialize failed: {err}");
                return;
            }
        };
        drop(guard);
        crate::workspace::persist::queue_snapshot_turn_workers(body);
        Self::notify_turn_worker_changed(work_id, stasis_job_id);
    }

    fn notify_turn_worker_changed(work_id: &str, stasis_job_id: Option<&str>) {
        crate::workspace::domain_event::notify_workspace_event(
            crate::workspace::domain_event::WorkspaceDomainEvent::TurnWorkerChanged {
                work_id: work_id.to_string(),
            },
        );
        if let Some(job_id) = stasis_job_id.filter(|value| !value.is_empty()) {
            crate::workspace::domain_event::notify_workspace_event(
                crate::workspace::domain_event::WorkspaceDomainEvent::StasisJobChanged {
                    job_id: job_id.to_string(),
                },
            );
        }
    }

    fn prune_map(map: &mut HashMap<String, TurnWorkRecord>) {
        let retention = WorkspaceRetentionConfig::load();
        let cutoff = retention.wipe_cutoff(Utc::now());
        map.retain(|_, record| {
            if record.archived {
                return record.updated_at >= cutoff;
            }
            true
        });

        let active: Vec<_> = map
            .values()
            .filter(|record| !record.archived)
            .map(|record| record.work_id.clone())
            .collect();
        if active.len() > MAX_ACTIVE_TURN_WORKERS {
            let overflow = active.len().saturating_sub(MAX_ACTIVE_TURN_WORKERS);
            let mut stale_ids: Vec<_> = map
                .values()
                .filter(|record| {
                    !record.archived
                        && matches!(
                            record.status,
                            TurnWorkStatus::Completed
                                | TurnWorkStatus::Failed
                                | TurnWorkStatus::Cancelled
                        )
                })
                .map(|record| (record.updated_at, record.work_id.clone()))
                .collect();
            stale_ids.sort_by_key(|(updated, _)| *updated);
            for (_, work_id) in stale_ids.into_iter().take(overflow) {
                if let Some(entry) = map.get_mut(&work_id) {
                    entry.archived = true;
                    entry.result_text = None;
                    entry.worker_scratch = None;
                    entry.updated_at = Utc::now();
                }
            }
        }
    }

    pub fn insert(&self, record: TurnWorkRecord) {
        let work_id = record.work_id.clone();
        let stasis_job_id = record.stasis_job_id.clone();
        let mut guard = self.records.lock().expect("turn worker records");
        guard.insert(work_id.clone(), record);
        drop(guard);
        self.persist(&work_id, stasis_job_id.as_deref());
    }

    pub fn get(&self, work_id: &str) -> Option<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .get(work_id)
            .cloned()
    }

    pub fn list_for_session(&self, session_id: &str) -> Vec<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| record.session_id == session_id && !record.archived)
            .cloned()
            .collect()
    }

    pub fn list_all(&self, limit: usize) -> Vec<TurnWorkRecord> {
        let mut records = self
            .records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| !record.archived)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        records.truncate(limit);
        records
    }

    pub fn list_all_unbounded(&self) -> Vec<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| !record.archived)
            .cloned()
            .collect()
    }

    pub fn list_incomplete(&self) -> Vec<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| {
                !record.archived
                    && (matches!(
                        record.status,
                        TurnWorkStatus::Pending | TurnWorkStatus::Running
                    ) || (record.status == TurnWorkStatus::Completed && !record.synthesis_delivered))
            })
            .cloned()
            .collect()
    }

    pub fn update<F>(&self, work_id: &str, update: F) -> Option<TurnWorkRecord>
    where
        F: FnOnce(&mut TurnWorkRecord),
    {
        let mut guard = self.records.lock().expect("turn worker records");
        let record = guard.get_mut(work_id)?;
        update(record);
        record.updated_at = Utc::now();
        let cloned = record.clone();
        drop(guard);
        self.persist(
            &cloned.work_id,
            cloned.stasis_job_id.as_deref(),
        );
        Some(cloned)
    }

    pub fn archive(&self, work_id: &str, purge_body: bool) -> Option<TurnWorkRecord> {
        let now = Utc::now();
        let mut guard = self.records.lock().expect("turn worker records");
        let record = guard.get_mut(work_id)?;
        record.archived = true;
        record.updated_at = now;
        if purge_body {
            record.result_text = None;
            record.worker_scratch = None;
        }
        let snapshot = record.clone();
        drop(guard);
        self.persist(&snapshot.work_id, snapshot.stasis_job_id.as_deref());
        Some(snapshot)
    }
}
