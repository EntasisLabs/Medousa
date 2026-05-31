use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::prelude::RuntimeComposition;
use surrealdb::engine::any::Any;
use surrealdb::Surreal;
use surrealdb_types::SurrealValue;
use tokio::sync::RwLock as AsyncRwLock;

use crate::channel_delivery::{self, ChannelDeliveryTarget};

const TABLE: &str = "turn_continuation_record";

const SCHEMA_STATEMENTS: &[&str] = &[
    "DEFINE TABLE turn_continuation_record SCHEMAFULL",
    "DEFINE FIELD child_job_id ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD turn_correlation_id ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD session_id ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD original_prompt ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD tool_name ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD job_type ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD await_mode ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD status ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD turn_finished ON TABLE turn_continuation_record TYPE bool",
    "DEFINE FIELD turn_outcome ON TABLE turn_continuation_record TYPE option<string>",
    "DEFINE FIELD child_was_dead_letter ON TABLE turn_continuation_record TYPE bool",
    "DEFINE FIELD delivery_target ON TABLE turn_continuation_record TYPE option<object>",
    "DEFINE FIELD provider ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD model ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD response_depth_mode ON TABLE turn_continuation_record TYPE string",
    "DEFINE FIELD created_at ON TABLE turn_continuation_record TYPE datetime",
    "DEFINE FIELD updated_at ON TABLE turn_continuation_record TYPE datetime",
    "DEFINE INDEX idx_turn_continuation_child ON TABLE turn_continuation_record COLUMNS child_job_id UNIQUE",
    "DEFINE INDEX idx_turn_continuation_turn ON TABLE turn_continuation_record COLUMNS turn_correlation_id",
];

static TURN_CONTINUATION_STORE: Lazy<RwLock<Arc<dyn TurnContinuationStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(InMemoryTurnContinuationStore::default())));

pub fn turn_continuation_store() -> Arc<dyn TurnContinuationStore> {
    TURN_CONTINUATION_STORE.read().unwrap().clone()
}

pub fn set_turn_continuation_store(store: Arc<dyn TurnContinuationStore>) {
    let mut guard = TURN_CONTINUATION_STORE.write().unwrap();
    *guard = store;
}

pub async fn init_turn_continuation_store_with_runtime(runtime: &RuntimeComposition) {
    match runtime {
        RuntimeComposition::Surreal(rt) => {
            let store = SurrealTurnContinuationStore::new(rt.job_store.db());
            if let Err(err) = store.ensure_schema().await {
                eprintln!(
                    "Surreal turn continuation store schema init error: {err}; keeping in-memory store"
                );
                return;
            }
            set_turn_continuation_store(Arc::new(store));
            eprintln!(
                "Surreal runtime detected; turn continuation store switched to SurrealDB backend"
            );
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationAwaitMode {
    Sync,
    Async,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnContinuationStatus {
    Pending,
    Consumed,
    Resumed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredDeliveryTarget {
    pub channel: String,
    pub user_id: String,
    pub channel_id: String,
    pub session_id: String,
    pub stream_id: Option<String>,
}

impl From<&ChannelDeliveryTarget> for StoredDeliveryTarget {
    fn from(value: &ChannelDeliveryTarget) -> Self {
        Self {
            channel: value.channel.clone(),
            user_id: value.user_id.clone(),
            channel_id: value.channel_id.clone(),
            session_id: value.session_id.clone(),
            stream_id: value.stream_id.clone(),
        }
    }
}

impl From<&StoredDeliveryTarget> for ChannelDeliveryTarget {
    fn from(value: &StoredDeliveryTarget) -> Self {
        Self {
            channel: value.channel.clone(),
            user_id: value.user_id.clone(),
            channel_id: value.channel_id.clone(),
            session_id: value.session_id.clone(),
            stream_id: value.stream_id.clone(),
        }
    }
}

/// Active turn context set for the duration of `run_agent_turn`.
#[derive(Debug, Clone)]
pub struct TurnContinuationScope {
    pub turn_correlation_id: String,
    pub session_id: String,
    pub original_prompt: String,
    pub delivery_target: Option<ChannelDeliveryTarget>,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnContinuationRecord {
    pub child_job_id: String,
    pub turn_correlation_id: String,
    pub session_id: String,
    pub original_prompt: String,
    pub tool_name: String,
    pub job_type: String,
    pub await_mode: ContinuationAwaitMode,
    pub status: TurnContinuationStatus,
    pub turn_finished: bool,
    pub turn_outcome: Option<TurnOutcome>,
    pub child_was_dead_letter: bool,
    pub delivery_target: Option<StoredDeliveryTarget>,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TurnContinuationRecord {
    pub fn should_resume(&self) -> bool {
        if self.status != TurnContinuationStatus::Pending {
            return false;
        }
        if self.child_was_dead_letter {
            return true;
        }
        if self.turn_finished && self.turn_outcome == Some(TurnOutcome::Error) {
            return true;
        }
        false
    }
}

pub fn apply_turn_correlation_to_job(
    job: &mut stasis::prelude::NewJob,
    scope: &TurnContinuationScope,
    tool_name: &str,
) {
    job.correlation_id = scope.turn_correlation_id.clone();
    job.causation_id = format!("{}:{}", scope.session_id, tool_name);
    job.trace_id = scope.turn_correlation_id.clone();
}

pub fn build_turn_resume_prompt(
    original_prompt: &str,
    tool_name: &str,
    job_type: &str,
    child_job_id: &str,
    job_output: &str,
) -> String {
    format!(
        "A background job from an earlier turn has now completed successfully after a prior failure or dead-letter replay.\n\n\
         [ORIGINAL_USER_REQUEST]\n{original_prompt}\n\n\
         [COMPLETED_JOB]\njob_id={child_job_id} job_type={job_type} tool={tool_name}\n\n\
         [JOB_OUTPUT]\n{job_output}\n\n\
         Continue helping the user: incorporate this job output into a complete, user-facing reply. \
         Do not mention internal job IDs unless the user asked for operational detail."
    )
}

/// Apply parent-turn correlation to a job and persist a continuation record.
pub async fn wire_turn_child_job(
    job: &mut stasis::prelude::NewJob,
    scope: &TurnContinuationScope,
    tool_name: &str,
    job_type: &str,
    await_mode: ContinuationAwaitMode,
) {
    apply_turn_correlation_to_job(job, scope, tool_name);
    register_turn_child_job(scope, &job.id, tool_name, job_type, await_mode).await;
}

/// JSON fragment for tool responses when a child job is linked to the active turn.
pub fn continuation_tool_metadata(
    scope: &TurnContinuationScope,
    child_job_id: &str,
    await_mode: ContinuationAwaitMode,
) -> serde_json::Value {
    serde_json::json!({
        "continuation_registered": true,
        "child_job_id": child_job_id,
        "turn_correlation_id": scope.turn_correlation_id,
        "await_mode": match await_mode {
            ContinuationAwaitMode::Sync => "sync",
            ContinuationAwaitMode::Async => "async",
        },
        "resume_on_replay": true,
    })
}

pub async fn register_turn_child_job(
    scope: &TurnContinuationScope,
    child_job_id: &str,
    tool_name: &str,
    job_type: &str,
    await_mode: ContinuationAwaitMode,
) {
    let now = Utc::now();
    let record = TurnContinuationRecord {
        child_job_id: child_job_id.to_string(),
        turn_correlation_id: scope.turn_correlation_id.clone(),
        session_id: scope.session_id.clone(),
        original_prompt: scope.original_prompt.clone(),
        tool_name: tool_name.to_string(),
        job_type: job_type.to_string(),
        await_mode,
        status: TurnContinuationStatus::Pending,
        turn_finished: false,
        turn_outcome: None,
        child_was_dead_letter: false,
        delivery_target: scope.delivery_target.as_ref().map(StoredDeliveryTarget::from),
        provider: scope.provider.clone(),
        model: scope.model.clone(),
        response_depth_mode: scope.response_depth_mode.clone(),
        created_at: now,
        updated_at: now,
    };
    if let Err(err) = turn_continuation_store().upsert(record).await {
        eprintln!(
            "turn continuation register failed child_job_id={child_job_id}: {err:#}"
        );
    }
}

pub async fn resolve_succeeded_job_output_text(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> Option<String> {
    let attempts = match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_attempt_store.list_by_job_id(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_attempt_store.list_by_job_id(job_id).await,
    };
    let Ok(attempts) = attempts else {
        return None;
    };
    attempts.iter().rev().find_map(|attempt| {
        channel_delivery::extract_output_text_from_diagnostics(attempt.diagnostics.as_deref())
    })
}

#[async_trait]
pub trait TurnContinuationStore: Send + Sync {
    async fn upsert(&self, record: TurnContinuationRecord) -> anyhow::Result<()>;
    async fn get(&self, child_job_id: &str) -> Option<TurnContinuationRecord>;
    async fn mark_consumed(&self, child_job_id: &str) -> anyhow::Result<()>;
    async fn mark_resumed(&self, child_job_id: &str) -> anyhow::Result<bool>;
    async fn mark_child_dead_letter(&self, child_job_id: &str) -> anyhow::Result<()>;
    async fn mark_turn_finished(
        &self,
        turn_correlation_id: &str,
        outcome: TurnOutcome,
    ) -> anyhow::Result<()>;
}

#[derive(Default)]
struct InMemoryTurnContinuationStore {
    records: AsyncRwLock<HashMap<String, TurnContinuationRecord>>,
}

#[async_trait]
impl TurnContinuationStore for InMemoryTurnContinuationStore {
    async fn upsert(&self, record: TurnContinuationRecord) -> anyhow::Result<()> {
        self.records
            .write()
            .await
            .insert(record.child_job_id.clone(), record);
        Ok(())
    }

    async fn get(&self, child_job_id: &str) -> Option<TurnContinuationRecord> {
        self.records.read().await.get(child_job_id).cloned()
    }

    async fn mark_consumed(&self, child_job_id: &str) -> anyhow::Result<()> {
        let mut guard = self.records.write().await;
        if let Some(record) = guard.get_mut(child_job_id) {
            record.status = TurnContinuationStatus::Consumed;
            record.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn mark_resumed(&self, child_job_id: &str) -> anyhow::Result<bool> {
        let mut guard = self.records.write().await;
        let Some(record) = guard.get_mut(child_job_id) else {
            return Ok(false);
        };
        if record.status != TurnContinuationStatus::Pending {
            return Ok(false);
        }
        record.status = TurnContinuationStatus::Resumed;
        record.updated_at = Utc::now();
        Ok(true)
    }

    async fn mark_child_dead_letter(&self, child_job_id: &str) -> anyhow::Result<()> {
        let mut guard = self.records.write().await;
        if let Some(record) = guard.get_mut(child_job_id) {
            record.child_was_dead_letter = true;
            record.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn mark_turn_finished(
        &self,
        turn_correlation_id: &str,
        outcome: TurnOutcome,
    ) -> anyhow::Result<()> {
        let mut guard = self.records.write().await;
        let now = Utc::now();
        for record in guard.values_mut() {
            if record.turn_correlation_id == turn_correlation_id {
                record.turn_finished = true;
                record.turn_outcome = Some(outcome);
                record.updated_at = now;
            }
        }
        Ok(())
    }
}

struct SurrealTurnContinuationStore {
    db: Surreal<Any>,
}

impl SurrealTurnContinuationStore {
    fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        for statement in SCHEMA_STATEMENTS {
            if let Err(err) = self.db.query(*statement).await {
                let text = err.to_string();
                if !(text.contains("already exists")
                    || text.contains("already defined")
                    || text.contains("Overwrite index"))
                {
                    return Err(anyhow::anyhow!(
                        "turn continuation schema bootstrap ({statement}): {text}"
                    ));
                }
            }
        }
        Ok(())
    }

    fn record_id(child_job_id: &str) -> String {
        format!("{TABLE}:{}", child_job_id.replace(':', "_"))
    }
}

#[async_trait]
impl TurnContinuationStore for SurrealTurnContinuationStore {
    async fn upsert(&self, record: TurnContinuationRecord) -> anyhow::Result<()> {
        let id = Self::record_id(&record.child_job_id);
        let _: Option<TurnContinuationRecord> = self
            .db
            .upsert((TABLE, id))
            .content(record)
            .await?;
        Ok(())
    }

    async fn get(&self, child_job_id: &str) -> Option<TurnContinuationRecord> {
        let id = Self::record_id(child_job_id);
        self.db
            .select((TABLE, id))
            .await
            .ok()
            .flatten()
    }

    async fn mark_consumed(&self, child_job_id: &str) -> anyhow::Result<()> {
        let Some(mut record) = self.get(child_job_id).await else {
            return Ok(());
        };
        record.status = TurnContinuationStatus::Consumed;
        record.updated_at = Utc::now();
        self.upsert(record).await
    }

    async fn mark_resumed(&self, child_job_id: &str) -> anyhow::Result<bool> {
        let Some(mut record) = self.get(child_job_id).await else {
            return Ok(false);
        };
        if record.status != TurnContinuationStatus::Pending {
            return Ok(false);
        }
        record.status = TurnContinuationStatus::Resumed;
        record.updated_at = Utc::now();
        self.upsert(record).await?;
        Ok(true)
    }

    async fn mark_child_dead_letter(&self, child_job_id: &str) -> anyhow::Result<()> {
        let Some(mut record) = self.get(child_job_id).await else {
            return Ok(());
        };
        record.child_was_dead_letter = true;
        record.updated_at = Utc::now();
        self.upsert(record).await
    }

    async fn mark_turn_finished(
        &self,
        turn_correlation_id: &str,
        outcome: TurnOutcome,
    ) -> anyhow::Result<()> {
        let mut response = self
            .db
            .query(format!(
                "UPDATE {TABLE} SET turn_finished = true, turn_outcome = $outcome, updated_at = time::now() \
                 WHERE turn_correlation_id = $turn_id"
            ))
            .bind((
                "outcome",
                match outcome {
                    TurnOutcome::Success => "success".to_string(),
                    TurnOutcome::Error => "error".to_string(),
                },
            ))
            .bind(("turn_id", turn_correlation_id.to_string()))
            .await?;
        let _: Vec<TurnContinuationRecord> = response.take(0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> TurnContinuationRecord {
        let now = Utc::now();
        TurnContinuationRecord {
            child_job_id: "cognition-gph-abc".to_string(),
            turn_correlation_id: "medousa-daemon-ingest-1".to_string(),
            session_id: "sess-1".to_string(),
            original_prompt: "run this script".to_string(),
            tool_name: "cognition_grapheme_run".to_string(),
            job_type: "workflow.grapheme.run".to_string(),
            await_mode: ContinuationAwaitMode::Sync,
            status: TurnContinuationStatus::Pending,
            turn_finished: true,
            turn_outcome: Some(TurnOutcome::Error),
            child_was_dead_letter: false,
            delivery_target: None,
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            response_depth_mode: "standard".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn should_resume_on_turn_error() {
        let record = sample_record();
        assert!(record.should_resume());
    }

    #[test]
    fn should_resume_on_dead_letter_replay() {
        let mut record = sample_record();
        record.turn_outcome = Some(TurnOutcome::Success);
        record.child_was_dead_letter = true;
        assert!(record.should_resume());
    }

    #[test]
    fn should_not_resume_when_consumed() {
        let mut record = sample_record();
        record.status = TurnContinuationStatus::Consumed;
        assert!(!record.should_resume());
    }

    #[test]
    fn should_not_resume_async_success_without_dead_letter() {
        let mut record = sample_record();
        record.await_mode = ContinuationAwaitMode::Async;
        record.turn_outcome = Some(TurnOutcome::Success);
        record.child_was_dead_letter = false;
        assert!(!record.should_resume());
    }

    #[tokio::test]
    async fn in_memory_mark_resumed_is_idempotent() {
        let store = InMemoryTurnContinuationStore::default();
        store.upsert(sample_record()).await.unwrap();
        assert!(store.mark_resumed("cognition-gph-abc").await.unwrap());
        assert!(!store.mark_resumed("cognition-gph-abc").await.unwrap());
    }
}
