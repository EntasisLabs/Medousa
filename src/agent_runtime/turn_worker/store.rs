//! Durable turn work records (host/worker bus).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::agent_runtime::turn_context::WorkerHandoffCapsule;
use crate::session;
use crate::turn_continuation::StoredDeliveryTarget;

const TURN_WORKERS_FILE: &str = "workspace/turn_workers.json";
const LEGACY_TURN_WORKERS_FILE: &str = "turn_workers.json";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TurnWorkDisposition {
    #[default]
    Parallel,
    Bound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopSteerMessage {
    pub text: String,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_profile_id: Option<String>,
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
    #[serde(default)]
    pub disposition: TurnWorkDisposition,
    #[serde(default)]
    pub steer_messages: Vec<WorkshopSteerMessage>,
    /// Snapshotted from host client when work was delegated (Home canvas lane).
    #[serde(default)]
    pub supports_ui_artifacts: bool,
    #[serde(default)]
    pub supports_browser_host: bool,
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
        Self::path_in(&session::medousa_data_dir())
    }

    fn legacy_path() -> PathBuf {
        Self::legacy_path_in(&session::medousa_data_dir())
    }

    fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(TURN_WORKERS_FILE)
    }

    fn legacy_path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(LEGACY_TURN_WORKERS_FILE)
    }

    fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(session::medousa_data_dir().join("workspace"));
        self.reload_from_paths(&Self::path(), &Self::legacy_path());
    }

    fn reload_from_paths(&self, canonical_path: &Path, legacy_path: &Path) {
        let (raw, migrated_legacy) = match fs::read_to_string(canonical_path) {
            Ok(raw) => (raw, false),
            Err(_) => match fs::read_to_string(legacy_path) {
                Ok(raw) => (raw, true),
                Err(_) => return,
            },
        };
        let Ok(map) = serde_json::from_str::<HashMap<String, TurnWorkRecord>>(&raw) else {
            return;
        };
        *self.records.lock().expect("turn worker records") = map;
        if migrated_legacy {
            if let Some(parent) = canonical_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(err) = fs::write(canonical_path, raw) {
                eprintln!(
                    "turn_worker_store: legacy snapshot migration failed path={} error={err}",
                    canonical_path.display()
                );
            }
        }
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
        records.sort_by_key(|right| std::cmp::Reverse(right.updated_at));
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
                    ) || (record.status == TurnWorkStatus::Completed
                        && !record.synthesis_delivered))
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
        self.persist(&cloned.work_id, cloned.stasis_job_id.as_deref());
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

    pub fn active_bound_workshop(&self, session_id: &str) -> Option<TurnWorkRecord> {
        self.records
            .lock()
            .expect("turn worker records")
            .values()
            .filter(|record| {
                !record.archived
                    && record.session_id == session_id
                    && record.disposition == TurnWorkDisposition::Bound
                    && matches!(
                        record.status,
                        TurnWorkStatus::Pending | TurnWorkStatus::Running
                    )
            })
            .max_by_key(|record| record.updated_at)
            .cloned()
    }

    pub fn push_steer(
        &self,
        work_id: &str,
        text: String,
        speaker_profile_id: Option<String>,
    ) -> Option<TurnWorkRecord> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let speaker = speaker_profile_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.update(work_id, |record| {
            record.steer_messages.push(WorkshopSteerMessage {
                text: trimmed.to_string(),
                at: Utc::now(),
                speaker_profile_id: speaker,
            });
        })
    }

    pub fn drain_steer_messages(&self, work_id: &str) -> Vec<WorkshopSteerMessage> {
        let mut drained = Vec::new();
        self.update(work_id, |record| {
            drained = std::mem::take(&mut record.steer_messages);
        });
        drained
    }

    pub fn is_work_cancelled(&self, work_id: &str) -> bool {
        self.get(work_id)
            .is_some_and(|record| record.status == TurnWorkStatus::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_snapshot_lives_in_workspace_directory() {
        let root = Path::new("/tmp/medousa-test-data");
        assert_eq!(
            TurnWorkerStore::path_in(root),
            root.join("workspace/turn_workers.json")
        );
        assert_eq!(
            TurnWorkerStore::legacy_path_in(root),
            root.join("turn_workers.json")
        );
    }

    #[test]
    fn legacy_snapshot_is_migrated_without_deleting_source() {
        let temp = tempfile::tempdir().expect("temp data directory");
        let canonical = TurnWorkerStore::path_in(temp.path());
        let legacy = TurnWorkerStore::legacy_path_in(temp.path());
        fs::write(&legacy, "{}").expect("write legacy snapshot");
        let store = TurnWorkerStore {
            records: Mutex::new(HashMap::new()),
        };

        store.reload_from_paths(&canonical, &legacy);

        assert_eq!(fs::read_to_string(&canonical).unwrap(), "{}");
        assert!(legacy.exists());
    }
}
