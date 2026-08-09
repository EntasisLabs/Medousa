//! Mechanical dispatch shared by the InMemory and Surreal runtime
//! compositions. Domain policy stays at the call site.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use stasis::domain::runtime::job::{Job, JobState, NewJob};
use stasis::domain::runtime::job_attempt::JobAttempt;
use stasis::domain::runtime::outbox::OutboxEvent;
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
use stasis::ports::outbound::runtime::job_store::JobStore;
use stasis::ports::outbound::runtime::outbox_store::OutboxStore;
use stasis::ports::outbound::runtime::recurring_store::RecurringStore;
use stasis::prelude::{Result, RuntimeComposition};

#[async_trait]
pub trait RuntimeCompositionExt {
    async fn get_job(&self, job_id: &str) -> Result<Option<Job>>;
    async fn save_job(&self, job: Job) -> Result<()>;
    async fn list_jobs_by_state(&self, state: JobState) -> Result<Vec<Job>>;
    async fn list_job_attempts(&self, job_id: &str) -> Result<Vec<JobAttempt>>;
    async fn enqueue_job(&self, job: NewJob) -> Result<()>;
    async fn list_recurring(&self) -> Result<Vec<RecurringDefinition>>;
    async fn save_recurring(&self, definition: RecurringDefinition) -> Result<()>;
    async fn register_recurring(&self, definition: RecurringDefinition) -> Result<()>;
    async fn list_pending_outbox_events(&self, limit: usize) -> Result<Vec<OutboxEvent>>;
    async fn lease_due_recurring(
        &self,
        now: DateTime<Utc>,
        scheduler_id: &str,
        lease_seconds: i64,
    ) -> Result<Vec<RecurringDefinition>>;
}

#[async_trait]
impl RuntimeCompositionExt for RuntimeComposition {
    async fn get_job(&self, job_id: &str) -> Result<Option<Job>> {
        match self {
            Self::InMemory(runtime) => runtime.job_store.get(job_id).await,
            Self::Surreal(runtime) => runtime.job_store.get(job_id).await,
        }
    }

    async fn save_job(&self, job: Job) -> Result<()> {
        match self {
            Self::InMemory(runtime) => runtime.job_store.save(job).await,
            Self::Surreal(runtime) => runtime.job_store.save(job).await,
        }
    }

    async fn list_jobs_by_state(&self, state: JobState) -> Result<Vec<Job>> {
        match self {
            Self::InMemory(runtime) => runtime.job_store.list_by_state(state).await,
            Self::Surreal(runtime) => runtime.job_store.list_by_state(state).await,
        }
    }

    async fn list_job_attempts(&self, job_id: &str) -> Result<Vec<JobAttempt>> {
        match self {
            Self::InMemory(runtime) => runtime.job_attempt_store.list_by_job_id(job_id).await,
            Self::Surreal(runtime) => runtime.job_attempt_store.list_by_job_id(job_id).await,
        }
    }

    async fn enqueue_job(&self, job: NewJob) -> Result<()> {
        match self {
            Self::InMemory(runtime) => runtime.enqueue(job).await,
            Self::Surreal(runtime) => runtime.enqueue(job).await,
        }
    }

    async fn list_recurring(&self) -> Result<Vec<RecurringDefinition>> {
        match self {
            Self::InMemory(runtime) => runtime.recurring_store.list().await,
            Self::Surreal(runtime) => runtime.recurring_store.list().await,
        }
    }

    async fn save_recurring(&self, definition: RecurringDefinition) -> Result<()> {
        match self {
            Self::InMemory(runtime) => runtime.recurring_store.save(definition).await,
            Self::Surreal(runtime) => runtime.recurring_store.save(definition).await,
        }
    }

    async fn register_recurring(&self, definition: RecurringDefinition) -> Result<()> {
        match self {
            Self::InMemory(runtime) => runtime.register_recurring(definition).await,
            Self::Surreal(runtime) => runtime.register_recurring(definition).await,
        }
    }

    async fn list_pending_outbox_events(&self, limit: usize) -> Result<Vec<OutboxEvent>> {
        match self {
            Self::InMemory(runtime) => runtime.outbox_store.list_pending(limit).await,
            Self::Surreal(runtime) => runtime.outbox_store.list_pending(limit).await,
        }
    }

    async fn lease_due_recurring(
        &self,
        now: DateTime<Utc>,
        scheduler_id: &str,
        lease_seconds: i64,
    ) -> Result<Vec<RecurringDefinition>> {
        match self {
            Self::InMemory(runtime) => {
                runtime
                    .recurring_store
                    .lease_due(now, scheduler_id, lease_seconds)
                    .await
            }
            Self::Surreal(runtime) => {
                runtime
                    .recurring_store
                    .lease_due(now, scheduler_id, lease_seconds)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use stasis::application::runtime::in_memory_runtime::InMemoryRuntime;
    use stasis::domain::runtime::job::{BackoffPolicy, NewJob};
    use stasis::prelude::RuntimeComposition;

    use super::RuntimeCompositionExt;

    #[tokio::test]
    async fn common_job_operations_dispatch_through_in_memory_runtime() {
        let runtime = RuntimeComposition::InMemory(InMemoryRuntime::new());
        let job = NewJob {
            id: "job-ext-test".to_string(),
            queue: "default".to_string(),
            job_type: "test.job".to_string(),
            payload_ref: "payload".to_string(),
            priority: 1,
            max_attempts: 1,
            idempotency_key: "idem-job-ext-test".to_string(),
            correlation_id: "corr-job-ext-test".to_string(),
            causation_id: "test".to_string(),
            trace_id: "trace-job-ext-test".to_string(),
            sttp_input_node_id: "sttp:in:test".to_string(),
            scheduled_at: Utc::now(),
            backoff_policy: BackoffPolicy::default(),
        };

        runtime.enqueue_job(job).await.expect("enqueue");
        let stored = runtime
            .get_job("job-ext-test")
            .await
            .expect("get")
            .expect("stored job");
        assert_eq!(stored.job_type, "test.job");
    }
}
