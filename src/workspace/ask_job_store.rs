//! Persistent daemon ask jobs — workspace cards + durable results.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::session;

const ASK_JOBS_FILE: &str = "workspace/ask_jobs.json";
const MAX_ACTIVE_ASK_JOBS: usize = 200;
use crate::workspace::retention::WorkspaceRetentionConfig;

static STORE: Lazy<AskJobStore> = Lazy::new(AskJobStore::new);

pub fn ask_job_store() -> &'static AskJobStore {
    &STORE
}

/// Isolated session ledger for one ask job — concurrent asks do not share transcript.
pub fn ask_job_session_id(job_id: &str) -> String {
    format!("medousa-ask:{}", job_id.trim())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskJobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskJobRecord {
    pub job_id: String,
    pub prompt: String,
    pub status: AskJobStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    /// Short host follow-up while a background worker runs (non-terminal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interim_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_manuscript_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_capability_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_hint: Option<String>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notified_channel: Option<String>,
}

pub struct AskJobStore {
    records: Mutex<HashMap<String, AskJobRecord>>,
}

impl AskJobStore {
    fn new() -> Self {
        let store = Self {
            records: Mutex::new(HashMap::new()),
        };
        store.reload_from_disk();
        store
    }

    fn path() -> PathBuf {
        session::medousa_data_dir().join(ASK_JOBS_FILE.strip_prefix("workspace/").unwrap_or(ASK_JOBS_FILE))
    }

    fn reload_from_disk(&self) {
        let _ = fs::create_dir_all(session::medousa_data_dir().join("workspace"));
        let Ok(raw) = fs::read_to_string(Self::path()) else {
            return;
        };
        let Ok(mut map) = serde_json::from_str::<HashMap<String, AskJobRecord>>(&raw) else {
            return;
        };
        let mut changed = false;
        for record in map.values_mut() {
            if record.status == AskJobStatus::Running {
                record.status = AskJobStatus::Failed;
                record.error = Some("interrupted by daemon restart".to_string());
                record.updated_at_utc = Utc::now();
                record.finished_at_utc = Some(Utc::now());
                changed = true;
            }
        }
        if changed {
            let _ = Self::write_map(&map);
        }
        *self.records.lock().expect("ask job records") = map;
    }

    fn write_map(map: &HashMap<String, AskJobRecord>) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(map)?;
        fs::write(path, body)
    }

    fn persist(&self) {
        let mut guard = self.records.lock().expect("ask job records");
        Self::prune_map(&mut guard);
        let snapshot = guard.clone();
        drop(guard);
        if let Err(err) = Self::write_map(&snapshot) {
            eprintln!("ask_job_store: persist failed: {err}");
        }
    }

    fn prune_map(map: &mut HashMap<String, AskJobRecord>) {
        let retention = WorkspaceRetentionConfig::load();
        let cutoff = retention.wipe_cutoff(Utc::now());
        map.retain(|_, record| {
            if record.archived {
                return record.updated_at_utc >= cutoff;
            }
            true
        });

        let active: Vec<_> = map
            .values()
            .filter(|record| !record.archived)
            .map(|record| record.job_id.clone())
            .collect();
        if active.len() > MAX_ACTIVE_ASK_JOBS {
            let overflow = active.len().saturating_sub(MAX_ACTIVE_ASK_JOBS);
            let mut stale_ids: Vec<_> = map
                .values()
                .filter(|record| !record.archived && record.status == AskJobStatus::Succeeded)
                .map(|record| (record.updated_at_utc, record.job_id.clone()))
                .collect();
            stale_ids.sort_by_key(|(updated, _)| *updated);
            for (_, job_id) in stale_ids.into_iter().take(overflow) {
                if let Some(entry) = map.get_mut(&job_id) {
                    entry.archived = true;
                    entry.output_text = None;
                    entry.interim_text = None;
                    entry.updated_at_utc = Utc::now();
                }
            }
        }
    }

    pub fn register_pending(&self, record: AskJobRecord) {
        let mut guard = self.records.lock().expect("ask job records");
        guard.insert(record.job_id.clone(), record);
        drop(guard);
        self.persist();
    }

    pub fn set_interim_text(&self, job_id: &str, interim_text: String) {
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        if matches!(
            record.status,
            AskJobStatus::Succeeded | AskJobStatus::Failed | AskJobStatus::Canceled
        ) {
            return;
        }
        record.interim_text = Some(interim_text);
        if record.status == AskJobStatus::Pending {
            record.status = AskJobStatus::Running;
        }
        record.updated_at_utc = Utc::now();
        drop(guard);
        self.persist();
    }

    pub fn mark_running(&self, job_id: &str) {
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        record.status = AskJobStatus::Running;
        record.updated_at_utc = Utc::now();
        drop(guard);
        self.persist();
    }

    pub fn mark_succeeded(&self, job_id: &str, output_text: String) {
        let now = Utc::now();
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        record.status = AskJobStatus::Succeeded;
        record.output_text = Some(output_text);
        record.error = None;
        record.updated_at_utc = now;
        record.finished_at_utc = Some(now);
        drop(guard);
        self.persist();
    }

    pub fn mark_failed(&self, job_id: &str, error: String) {
        let now = Utc::now();
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        record.status = AskJobStatus::Failed;
        record.error = Some(error);
        record.updated_at_utc = now;
        record.finished_at_utc = Some(now);
        drop(guard);
        self.persist();
    }

    pub fn reset_for_retry(&self, job_id: &str) -> Option<AskJobRecord> {
        let now = Utc::now();
        let mut guard = self.records.lock().expect("ask job records");
        let record = guard.get_mut(job_id)?;
        if !matches!(
            record.status,
            AskJobStatus::Failed | AskJobStatus::Canceled
        ) {
            return None;
        }
        record.status = AskJobStatus::Pending;
        record.error = None;
        record.output_text = None;
        record.interim_text = None;
        record.finished_at_utc = None;
        record.updated_at_utc = now;
        let snapshot = record.clone();
        drop(guard);
        self.persist();
        Some(snapshot)
    }

    pub fn mark_canceled(&self, job_id: &str) {
        let now = Utc::now();
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        if record.status == AskJobStatus::Succeeded || record.status == AskJobStatus::Failed {
            return;
        }
        record.status = AskJobStatus::Canceled;
        record.updated_at_utc = now;
        record.finished_at_utc = Some(now);
        drop(guard);
        self.persist();
    }

    pub fn archive(&self, job_id: &str, purge_body: bool) -> Option<AskJobRecord> {
        let now = Utc::now();
        let mut guard = self.records.lock().expect("ask job records");
        let record = guard.get_mut(job_id)?;
        record.archived = true;
        record.updated_at_utc = now;
        if purge_body {
            record.output_text = None;
            record.interim_text = None;
        }
        let snapshot = record.clone();
        drop(guard);
        self.persist();
        Some(snapshot)
    }

    pub fn set_journal_path(&self, job_id: &str, path: String) {
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        record.journal_path = Some(path);
        record.updated_at_utc = Utc::now();
        drop(guard);
        self.persist();
    }

    pub fn set_notified_channel(&self, job_id: &str, channel: String) {
        let mut guard = self.records.lock().expect("ask job records");
        let Some(record) = guard.get_mut(job_id) else {
            return;
        };
        record.notified_channel = Some(channel);
        record.updated_at_utc = Utc::now();
        drop(guard);
        self.persist();
    }

    pub fn get(&self, job_id: &str) -> Option<AskJobRecord> {
        self.records
            .lock()
            .expect("ask job records")
            .get(job_id)
            .cloned()
    }

    pub fn list_for_workspace(&self, include_archived: bool) -> Vec<AskJobRecord> {
        self.records
            .lock()
            .expect("ask job records")
            .values()
            .filter(|record| include_archived || !record.archived)
            .cloned()
            .collect()
    }

    pub fn is_ask_job_id(job_id: &str) -> bool {
        job_id.starts_with("medousa-daemon-ask-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_job_session_id_is_isolated_per_job() {
        assert_eq!(
            ask_job_session_id("medousa-daemon-ask-123"),
            "medousa-ask:medousa-daemon-ask-123"
        );
    }

    #[test]
    fn reset_for_retry_only_failed_or_canceled() {
        let store = AskJobStore {
            records: Mutex::new(HashMap::new()),
        };
        let job_id = "medousa-daemon-ask-test-1".to_string();
        store.register_pending(AskJobRecord {
            job_id: job_id.clone(),
            prompt: "research openclaw".to_string(),
            status: AskJobStatus::Failed,
            output_text: None,
            interim_text: None,
            error: Some("tool denied".to_string()),
            session_id: "daemon-api:test".to_string(),
            manuscript_id: None,
            additional_manuscript_ids: None,
            suggested_capability_ids: Some(vec!["websearch.search".to_string()]),
            model_hint: Some("ollama:qwen".to_string()),
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            finished_at_utc: Some(Utc::now()),
            archived: false,
            journal_path: None,
            notified_channel: None,
        });

        let retried = store.reset_for_retry(&job_id).expect("reset");
        assert_eq!(retried.status, AskJobStatus::Pending);
        assert!(retried.error.is_none());
        assert!(retried.finished_at_utc.is_none());
        assert_eq!(
            retried.suggested_capability_ids,
            Some(vec!["websearch.search".to_string()])
        );

        store.mark_running(&job_id);
        assert!(store.reset_for_retry(&job_id).is_none());
    }
}
