use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use stasis::domain::runtime::job::JobState;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::ports::outbound::runtime::job_store::JobStore;
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
    if let RuntimeComposition::Surreal(rt) = runtime {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationAwaitMode {
    Sync,
    Async,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
pub enum TurnContinuationStatus {
    Pending,
    Consumed,
    Resumed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, SurrealValue)]
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
    /// Copied from `TurnSurfaceContext.supports_ui_artifacts` for the active turn.
    pub supports_ui_artifacts: bool,
    /// Copied from `TurnSurfaceContext.supports_browser_host` for the active turn.
    pub supports_browser_host: bool,
    /// Channel surface label (home-desktop, home-ios, telegram, …).
    pub channel_surface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, SurrealValue)]
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

pub fn apply_turn_correlation_to_existing_job(
    job: &mut stasis::domain::runtime::job::Job,
    scope: &TurnContinuationScope,
    tool_name: &str,
) {
    job.correlation_id = scope.turn_correlation_id.clone();
    job.causation_id = format!("{}:{}", scope.session_id, tool_name);
    job.trace_id = scope.turn_correlation_id.clone();
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TurnContinuationSnapshot {
    pub pending_count: usize,
    pub consumed_count: usize,
    pub resumed_count: usize,
    pub dead_letter_pending_count: usize,
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnContinuationResumeEvent {
    pub child_job_id: String,
    pub turn_correlation_id: String,
    pub session_id: String,
    pub resumed_at: DateTime<Utc>,
}

static LAST_CONTINUATION_RESUME: Lazy<RwLock<Option<TurnContinuationResumeEvent>>> =
    Lazy::new(|| RwLock::new(None));

pub fn record_continuation_resume(event: TurnContinuationResumeEvent) {
    if let Ok(mut guard) = LAST_CONTINUATION_RESUME.write() {
        *guard = Some(event);
    }
}

pub fn last_continuation_resume() -> Option<TurnContinuationResumeEvent> {
    LAST_CONTINUATION_RESUME.read().ok()?.clone()
}

pub async fn replay_dead_letter_job(
    runtime: &RuntimeComposition,
    job_id: &str,
) -> anyhow::Result<bool> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt
            .replay_dead_letter_now(job_id)
            .await
            .map_err(|err| anyhow::anyhow!("replay dead letter failed: {err}")),
        RuntimeComposition::Surreal(rt) => rt
            .replay_dead_letter_now(job_id)
            .await
            .map_err(|err| anyhow::anyhow!("replay dead letter failed: {err}")),
    }
}

pub async fn materialize_recurring_now(
    runtime: &RuntimeComposition,
    scheduler_id: &str,
) -> anyhow::Result<usize> {
    match runtime {
        RuntimeComposition::InMemory(rt) => rt
            .materialize_recurring_now(scheduler_id)
            .await
            .map_err(|err| anyhow::anyhow!("materialize recurring failed: {err}")),
        RuntimeComposition::Surreal(rt) => rt
            .materialize_recurring_now(scheduler_id)
            .await
            .map_err(|err| anyhow::anyhow!("materialize recurring failed: {err}")),
    }
}

pub async fn find_active_job_by_correlation_id(
    runtime: &RuntimeComposition,
    correlation_id: &str,
) -> Option<String> {
    for state in [JobState::Enqueued, JobState::Leased, JobState::Running] {
        let jobs = match runtime {
            RuntimeComposition::InMemory(rt) => rt.job_store.list_by_state(state).await,
            RuntimeComposition::Surreal(rt) => rt.job_store.list_by_state(state).await,
        };
        let Ok(jobs) = jobs else {
            continue;
        };
        if let Some(job) = jobs
            .into_iter()
            .filter(|job| job.correlation_id == correlation_id)
            .max_by_key(|job| job.scheduled_at)
        {
            return Some(job.id);
        }
    }
    None
}

pub async fn patch_existing_job_correlation(
    runtime: &RuntimeComposition,
    job_id: &str,
    scope: &TurnContinuationScope,
    tool_name: &str,
) -> anyhow::Result<()> {
    let job = match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.get(job_id).await,
        RuntimeComposition::Surreal(rt) => rt.job_store.get(job_id).await,
    }
    .map_err(|err| anyhow::anyhow!("load job for correlation patch failed: {err}"))?;

    let Some(mut job) = job else {
        return Ok(());
    };
    apply_turn_correlation_to_existing_job(&mut job, scope, tool_name);
    match runtime {
        RuntimeComposition::InMemory(rt) => rt.job_store.save(job).await?,
        RuntimeComposition::Surreal(rt) => rt.job_store.save(job).await?,
    }
    Ok(())
}

pub async fn continuation_snapshot() -> TurnContinuationSnapshot {
    turn_continuation_store().snapshot().await
}

pub async fn continuation_lineage_for_turn(
    turn_correlation_id: &str,
    limit: usize,
) -> Vec<TurnContinuationRecord> {
    turn_continuation_store()
        .list_by_turn_correlation(turn_correlation_id, limit)
        .await
}

pub fn lineage_entry_from_record(record: &TurnContinuationRecord) -> crate::daemon_api::TurnContinuationLineageEntry {
    crate::daemon_api::TurnContinuationLineageEntry {
        child_job_id: record.child_job_id.clone(),
        turn_correlation_id: record.turn_correlation_id.clone(),
        session_id: record.session_id.clone(),
        tool_name: record.tool_name.clone(),
        job_type: record.job_type.clone(),
        await_mode: match record.await_mode {
            ContinuationAwaitMode::Sync => "sync".to_string(),
            ContinuationAwaitMode::Async => "async".to_string(),
        },
        status: match record.status {
            TurnContinuationStatus::Pending => "pending",
            TurnContinuationStatus::Consumed => "consumed",
            TurnContinuationStatus::Resumed => "resumed",
            TurnContinuationStatus::Abandoned => "abandoned",
        }
        .to_string(),
        turn_finished: record.turn_finished,
        turn_outcome: record.turn_outcome.map(|outcome| match outcome {
            TurnOutcome::Success => "success".to_string(),
            TurnOutcome::Error => "error".to_string(),
        }),
        child_was_dead_letter: record.child_was_dead_letter,
        created_at_utc: record.created_at,
        updated_at_utc: record.updated_at,
    }
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
    async fn snapshot(&self) -> TurnContinuationSnapshot;
    async fn list_by_turn_correlation(
        &self,
        turn_correlation_id: &str,
        limit: usize,
    ) -> Vec<TurnContinuationRecord>;
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

    async fn snapshot(&self) -> TurnContinuationSnapshot {
        let guard = self.records.read().await;
        let mut snapshot = TurnContinuationSnapshot {
            total_count: guard.len(),
            ..Default::default()
        };
        for record in guard.values() {
            match record.status {
                TurnContinuationStatus::Pending => {
                    snapshot.pending_count += 1;
                    if record.child_was_dead_letter {
                        snapshot.dead_letter_pending_count += 1;
                    }
                }
                TurnContinuationStatus::Consumed => snapshot.consumed_count += 1,
                TurnContinuationStatus::Resumed => snapshot.resumed_count += 1,
                TurnContinuationStatus::Abandoned => {}
            }
        }
        snapshot
    }

    async fn list_by_turn_correlation(
        &self,
        turn_correlation_id: &str,
        limit: usize,
    ) -> Vec<TurnContinuationRecord> {
        let mut records: Vec<_> = self
            .records
            .read()
            .await
            .values()
            .filter(|record| record.turn_correlation_id == turn_correlation_id)
            .cloned()
            .collect();
        records.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        records.truncate(limit.max(1));
        records
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
        let sql = "UPSERT type::record($table, $id) CONTENT $data";
        self.db
            .query(sql)
            .bind(("table", TABLE))
            .bind(("id", id))
            .bind(("data", record))
            .await?;
        Ok(())
    }

    async fn get(&self, child_job_id: &str) -> Option<TurnContinuationRecord> {
        let id = Self::record_id(child_job_id);
        let sql = "SELECT * FROM type::record($table, $id)";
        let mut response = self
            .db
            .query(sql)
            .bind(("table", TABLE))
            .bind(("id", id))
            .await
            .ok()?;
        response.take::<Option<TurnContinuationRecord>>(0).ok().flatten()
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
        let outcome = match outcome {
            TurnOutcome::Success => "success",
            TurnOutcome::Error => "error",
        };
        let mut response = self
            .db
            .query(format!(
                "UPDATE {TABLE} SET turn_finished = true, turn_outcome = $outcome, updated_at = time::now() \
                 WHERE turn_correlation_id = $turn_id"
            ))
            .bind(("outcome", outcome.to_string()))
            .bind(("turn_id", turn_correlation_id.to_string()))
            .await?;
        let _: Vec<TurnContinuationRecord> = response.take(0)?;
        Ok(())
    }

    async fn snapshot(&self) -> TurnContinuationSnapshot {
        let sql = format!("SELECT status, child_was_dead_letter FROM {TABLE}");
        let Ok(mut response) = self.db.query(sql).await else {
            return TurnContinuationSnapshot::default();
        };
        #[derive(Debug, Deserialize, SurrealValue)]
        struct Row {
            status: TurnContinuationStatus,
            child_was_dead_letter: bool,
        }
        let rows = response.take::<Vec<Row>>(0).unwrap_or_default();
        let mut snapshot = TurnContinuationSnapshot {
            total_count: rows.len(),
            ..TurnContinuationSnapshot::default()
        };
        for row in rows {
            match row.status {
                TurnContinuationStatus::Pending => {
                    snapshot.pending_count += 1;
                    if row.child_was_dead_letter {
                        snapshot.dead_letter_pending_count += 1;
                    }
                }
                TurnContinuationStatus::Consumed => snapshot.consumed_count += 1,
                TurnContinuationStatus::Resumed => snapshot.resumed_count += 1,
                TurnContinuationStatus::Abandoned => {}
            }
        }
        snapshot
    }

    async fn list_by_turn_correlation(
        &self,
        turn_correlation_id: &str,
        limit: usize,
    ) -> Vec<TurnContinuationRecord> {
        let sql = format!(
            "SELECT * FROM {TABLE} WHERE turn_correlation_id = $turn_id ORDER BY created_at DESC LIMIT $limit"
        );
        let Ok(mut response) = self
            .db
            .query(sql)
            .bind(("turn_id", turn_correlation_id.to_string()))
            .bind(("limit", limit.max(1) as i64))
            .await
        else {
            return Vec::new();
        };
        response.take::<Vec<TurnContinuationRecord>>(0).unwrap_or_default()
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
