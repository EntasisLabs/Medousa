//! Persistent daemon ask jobs — workspace cards + durable results.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

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
    admit_mutations: MutationAdmitter,
}

type MutationAdmitter = std::sync::Arc<
    dyn Fn(
            Vec<crate::workspace::persist::WorkspaceMutation>,
        ) -> Result<(), crate::persistence::PersistenceError>
        + Send
        + Sync,
>;

impl AskJobStore {
    fn new() -> Self {
        let store = Self::with_admitter(std::sync::Arc::new(
            crate::workspace::persist::queue_mutations,
        ));
        store.reload_from_disk();
        store
    }

    fn with_admitter(admit_mutations: MutationAdmitter) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            admit_mutations,
        }
    }

    fn reload_from_disk(&self) {
        if let Some(projection) = crate::workspace::persist::startup_projection() {
            *self.records.lock().expect("ask job records") = projection.ask_jobs;
        } else {
            tracing::error!(
                "ask_job_store: workspace persistence projection unavailable; refusing ambient snapshot fallback"
            );
        }
    }

    fn transition<R>(
        &self,
        job_id: &str,
        update: impl FnOnce(&mut HashMap<String, AskJobRecord>) -> Option<R>,
        publish_after_admission_error: bool,
    ) -> Result<Option<R>, crate::persistence::PersistenceError> {
        let mut guard = self.records.lock().expect("ask job records");
        // Observed output/terminal state remains authoritative in memory even
        // when storage is unavailable. Move that map under its lock instead of
        // copying every unrelated result on each streamed update.
        let mut candidate = if publish_after_admission_error {
            std::mem::take(&mut *guard)
        } else {
            guard.clone()
        };
        let Some(result) = update(&mut candidate) else {
            if publish_after_admission_error {
                *guard = candidate;
            }
            return Ok(None);
        };
        let mut changed = Self::prune_map(&mut candidate);
        if let Some(record) = candidate.get(job_id).cloned() {
            changed.push(record);
        }
        let retained = candidate.keys().cloned().collect::<Vec<_>>();
        changed.sort_by(|left, right| left.job_id.cmp(&right.job_id));
        changed.dedup_by(|left, right| left.job_id == right.job_id);
        let mut mutations = changed
            .into_iter()
            .map(
                |record| crate::workspace::persist::WorkspaceMutation::UpsertAskJob {
                    record: Box::new(record),
                },
            )
            .collect::<Vec<_>>();
        mutations.push(
            crate::workspace::persist::WorkspaceMutation::RetainAskJobs { job_ids: retained },
        );
        let admission = (self.admit_mutations)(mutations);
        if let Err(error) = admission {
            if publish_after_admission_error {
                *guard = candidate;
                drop(guard);
                Self::notify_ask_job_changed(job_id);
            }
            return Err(error);
        }
        *guard = candidate;
        drop(guard);
        Self::notify_ask_job_changed(job_id);
        Ok(Some(result))
    }

    fn report_persistence_error(
        action: &str,
        job_id: &str,
        error: &crate::persistence::PersistenceError,
    ) {
        tracing::error!(job_id, action, error = %error, "ask_job_store persistence admission failed");
    }

    fn notify_ask_job_changed(job_id: &str) {
        crate::workspace::domain_event::notify_workspace_event(
            crate::workspace::domain_event::WorkspaceDomainEvent::AskJobChanged {
                job_id: job_id.to_string(),
            },
        );
    }

    fn prune_map(map: &mut HashMap<String, AskJobRecord>) -> Vec<AskJobRecord> {
        let mut changed = Vec::new();
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
                    changed.push(entry.clone());
                }
            }
        }
        changed
    }

    pub fn try_register_pending(
        &self,
        record: AskJobRecord,
    ) -> Result<(), crate::persistence::PersistenceError> {
        let job_id = record.job_id.clone();
        let key = job_id.clone();
        self.transition(
            &job_id,
            move |records| {
                records.insert(key, record);
                Some(())
            },
            false,
        )?;
        Ok(())
    }

    pub fn register_pending(&self, record: AskJobRecord) {
        if let Err(error) = self.try_register_pending(record) {
            Self::report_persistence_error("register_pending", "unknown", &error);
        }
    }

    pub fn try_set_interim_text(
        &self,
        job_id: &str,
        interim_text: String,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                if matches!(
                    record.status,
                    AskJobStatus::Succeeded | AskJobStatus::Failed | AskJobStatus::Canceled
                ) {
                    return None;
                }
                record.interim_text = Some(interim_text);
                if record.status == AskJobStatus::Pending {
                    record.status = AskJobStatus::Running;
                }
                record.updated_at_utc = now;
                Some(())
            },
            true,
        )?;
        Ok(updated.is_some())
    }

    pub fn set_interim_text(&self, job_id: &str, interim_text: String) {
        if let Err(error) = self.try_set_interim_text(job_id, interim_text) {
            Self::report_persistence_error("set_interim_text", job_id, &error);
        }
    }

    pub fn try_mark_running(
        &self,
        job_id: &str,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                record.status = AskJobStatus::Running;
                record.updated_at_utc = now;
                Some(())
            },
            false,
        )?;
        Ok(updated.is_some())
    }

    pub fn mark_running(&self, job_id: &str) {
        if let Err(error) = self.try_mark_running(job_id) {
            Self::report_persistence_error("mark_running", job_id, &error);
        }
    }

    pub fn try_mark_succeeded(
        &self,
        job_id: &str,
        output_text: String,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                record.status = AskJobStatus::Succeeded;
                record.output_text = Some(output_text);
                record.error = None;
                record.updated_at_utc = now;
                record.finished_at_utc = Some(now);
                Some(())
            },
            true,
        )?;
        Ok(updated.is_some())
    }

    pub fn mark_succeeded(&self, job_id: &str, output_text: String) {
        if let Err(error) = self.try_mark_succeeded(job_id, output_text) {
            Self::report_persistence_error("mark_succeeded", job_id, &error);
        }
    }

    pub fn try_mark_failed(
        &self,
        job_id: &str,
        failure: String,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                record.status = AskJobStatus::Failed;
                record.error = Some(failure);
                record.updated_at_utc = now;
                record.finished_at_utc = Some(now);
                Some(())
            },
            true,
        )?;
        Ok(updated.is_some())
    }

    pub fn mark_failed(&self, job_id: &str, failure: String) {
        if let Err(error) = self.try_mark_failed(job_id, failure) {
            Self::report_persistence_error("mark_failed", job_id, &error);
        }
    }

    pub fn try_reset_for_retry(
        &self,
        job_id: &str,
    ) -> Result<Option<AskJobRecord>, crate::persistence::PersistenceError> {
        let now = Utc::now();
        self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                if !matches!(record.status, AskJobStatus::Failed | AskJobStatus::Canceled) {
                    return None;
                }
                record.status = AskJobStatus::Pending;
                record.error = None;
                record.output_text = None;
                record.interim_text = None;
                record.finished_at_utc = None;
                record.updated_at_utc = now;
                Some(record.clone())
            },
            false,
        )
    }

    pub fn reset_for_retry(&self, job_id: &str) -> Option<AskJobRecord> {
        match self.try_reset_for_retry(job_id) {
            Ok(record) => record,
            Err(error) => {
                Self::report_persistence_error("reset_for_retry", job_id, &error);
                None
            }
        }
    }

    pub fn try_mark_canceled(
        &self,
        job_id: &str,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                if matches!(
                    record.status,
                    AskJobStatus::Succeeded | AskJobStatus::Failed
                ) {
                    return None;
                }
                record.status = AskJobStatus::Canceled;
                record.updated_at_utc = now;
                record.finished_at_utc = Some(now);
                Some(())
            },
            true,
        )?;
        Ok(updated.is_some())
    }

    pub fn mark_canceled(&self, job_id: &str) {
        if let Err(error) = self.try_mark_canceled(job_id) {
            Self::report_persistence_error("mark_canceled", job_id, &error);
        }
    }

    pub fn try_archive(
        &self,
        job_id: &str,
        purge_body: bool,
    ) -> Result<Option<AskJobRecord>, crate::persistence::PersistenceError> {
        let now = Utc::now();
        self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                record.archived = true;
                record.updated_at_utc = now;
                if purge_body {
                    record.output_text = None;
                    record.interim_text = None;
                }
                Some(record.clone())
            },
            true,
        )
    }

    pub fn archive(&self, job_id: &str, purge_body: bool) -> Option<AskJobRecord> {
        match self.try_archive(job_id, purge_body) {
            Ok(record) => record,
            Err(error) => {
                Self::report_persistence_error("archive", job_id, &error);
                None
            }
        }
    }

    pub fn try_set_journal_path(
        &self,
        job_id: &str,
        path: String,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                record.journal_path = Some(path);
                record.updated_at_utc = now;
                Some(())
            },
            true,
        )?;
        Ok(updated.is_some())
    }

    pub fn set_journal_path(&self, job_id: &str, path: String) {
        if let Err(error) = self.try_set_journal_path(job_id, path) {
            Self::report_persistence_error("set_journal_path", job_id, &error);
        }
    }

    pub fn try_set_notified_channel(
        &self,
        job_id: &str,
        channel: String,
    ) -> Result<bool, crate::persistence::PersistenceError> {
        let now = Utc::now();
        let updated = self.transition(
            job_id,
            move |records| {
                let record = records.get_mut(job_id)?;
                record.notified_channel = Some(channel);
                record.updated_at_utc = now;
                Some(())
            },
            true,
        )?;
        Ok(updated.is_some())
    }

    pub fn set_notified_channel(&self, job_id: &str, channel: String) {
        if let Err(error) = self.try_set_notified_channel(job_id, channel) {
            Self::report_persistence_error("set_notified_channel", job_id, &error);
        }
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
    use std::sync::atomic::{AtomicBool, Ordering};

    fn test_record(job_id: &str, status: AskJobStatus) -> AskJobRecord {
        AskJobRecord {
            job_id: job_id.to_string(),
            prompt: "research openclaw".to_string(),
            status,
            output_text: None,
            interim_text: None,
            error: None,
            session_id: "daemon-api:test".to_string(),
            manuscript_id: None,
            additional_manuscript_ids: None,
            suggested_capability_ids: Some(vec!["websearch.search".to_string()]),
            model_hint: Some("ollama:qwen".to_string()),
            created_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            finished_at_utc: None,
            archived: false,
            journal_path: None,
            notified_channel: None,
        }
    }

    fn memory_store() -> AskJobStore {
        AskJobStore::with_admitter(std::sync::Arc::new(|_| Ok(())))
    }

    #[test]
    fn ask_job_session_id_is_isolated_per_job() {
        assert_eq!(
            ask_job_session_id("medousa-daemon-ask-123"),
            "medousa-ask:medousa-daemon-ask-123"
        );
    }

    #[test]
    fn reset_for_retry_only_failed_or_canceled() {
        let store = memory_store();
        let job_id = "medousa-daemon-ask-test-1".to_string();
        let mut failed = test_record(&job_id, AskJobStatus::Failed);
        failed.error = Some("tool denied".to_string());
        failed.finished_at_utc = Some(Utc::now());
        store.register_pending(failed);

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

    #[test]
    fn registration_admits_record_and_retention_as_one_batch_before_publication() {
        let batches = std::sync::Arc::new(Mutex::new(Vec::new()));
        let captured = batches.clone();
        let store = AskJobStore::with_admitter(std::sync::Arc::new(move |batch| {
            captured.lock().unwrap().push(batch);
            Ok(())
        }));
        let job_id = "medousa-daemon-ask-atomic";

        store
            .try_register_pending(test_record(job_id, AskJobStatus::Pending))
            .unwrap();

        let batches = batches.lock().unwrap();
        assert_eq!(batches.len(), 1);
        assert!(batches[0].iter().any(|mutation| matches!(
            mutation,
            crate::workspace::persist::WorkspaceMutation::UpsertAskJob { record }
                if record.job_id == job_id
        )));
        assert!(batches[0].iter().any(|mutation| matches!(
            mutation,
            crate::workspace::persist::WorkspaceMutation::RetainAskJobs { job_ids }
                if job_ids.iter().any(|candidate| candidate == job_id)
        )));
        assert_eq!(store.get(job_id).unwrap().status, AskJobStatus::Pending);
    }

    #[test]
    fn failed_new_admission_is_not_published_but_observed_terminal_result_is_kept() {
        let reject = std::sync::Arc::new(AtomicBool::new(false));
        let reject_admission = reject.clone();
        let store = AskJobStore::with_admitter(std::sync::Arc::new(move |_| {
            if reject_admission.load(Ordering::SeqCst) {
                Err(crate::persistence::PersistenceError::new(
                    crate::persistence::PersistenceErrorKind::Overloaded,
                    "test queue full",
                ))
            } else {
                Ok(())
            }
        }));

        assert!(
            store
                .try_register_pending(test_record(
                    "medousa-daemon-ask-rejected",
                    AskJobStatus::Pending,
                ))
                .is_ok()
        );
        reject.store(true, Ordering::SeqCst);
        assert!(
            store
                .try_register_pending(test_record(
                    "medousa-daemon-ask-not-admitted",
                    AskJobStatus::Pending,
                ))
                .is_err()
        );
        assert!(store.get("medousa-daemon-ask-not-admitted").is_none());

        let error = store
            .try_mark_succeeded(
                "medousa-daemon-ask-rejected",
                "completed answer".to_string(),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "test queue full");
        let observed = store.get("medousa-daemon-ask-rejected").unwrap();
        assert_eq!(observed.status, AskJobStatus::Succeeded);
        assert_eq!(observed.output_text.as_deref(), Some("completed answer"));
    }
}
