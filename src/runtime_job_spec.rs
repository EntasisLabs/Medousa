//! Pure construction for Medousa-created Stasis jobs.

use chrono::{DateTime, Utc};
use stasis::domain::runtime::job::{BackoffPolicy, NewJob};

/// Construction spec for a first-party job.
///
/// The defaults mirror the existing tool-created jobs. Continuation wiring and
/// persistence remain explicit operations performed by the caller.
#[derive(Debug, Clone)]
pub struct ToolJobSpec {
    id: String,
    queue: String,
    job_type: String,
    payload_ref: String,
    priority: i32,
    max_attempts: u32,
    idempotency_key: String,
    correlation_id: String,
    causation_id: String,
    trace_id: String,
    sttp_input_node_id: String,
    scheduled_at: DateTime<Utc>,
    backoff_policy: BackoffPolicy,
}

impl ToolJobSpec {
    pub fn new(
        id: impl Into<String>,
        queue: impl Into<String>,
        job_type: impl Into<String>,
        payload_ref: impl Into<String>,
        causation_id: impl Into<String>,
        sttp_input_node_id: impl Into<String>,
        scheduled_at: DateTime<Utc>,
    ) -> Self {
        let id = id.into();
        Self {
            idempotency_key: format!("idem-{id}"),
            correlation_id: id.clone(),
            trace_id: id.clone(),
            id,
            queue: queue.into(),
            job_type: job_type.into(),
            payload_ref: payload_ref.into(),
            priority: 100,
            max_attempts: 1,
            causation_id: causation_id.into(),
            sttp_input_node_id: sttp_input_node_id.into(),
            scheduled_at,
            backoff_policy: BackoffPolicy::default(),
        }
    }

    pub fn priority(mut self, value: i32) -> Self {
        self.priority = value;
        self
    }

    pub fn max_attempts(mut self, value: u32) -> Self {
        self.max_attempts = value;
        self
    }

    pub fn correlation_id(mut self, value: impl Into<String>) -> Self {
        self.correlation_id = value.into();
        self
    }

    pub fn trace_id(mut self, value: impl Into<String>) -> Self {
        self.trace_id = value.into();
        self
    }

    pub fn backoff_policy(mut self, value: BackoffPolicy) -> Self {
        self.backoff_policy = value;
        self
    }

    pub fn build(self) -> NewJob {
        NewJob {
            id: self.id,
            queue: self.queue,
            job_type: self.job_type,
            payload_ref: self.payload_ref,
            priority: self.priority,
            max_attempts: self.max_attempts,
            idempotency_key: self.idempotency_key,
            correlation_id: self.correlation_id,
            causation_id: self.causation_id,
            trace_id: self.trace_id,
            sttp_input_node_id: self.sttp_input_node_id,
            scheduled_at: self.scheduled_at,
            backoff_policy: self.backoff_policy,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::ToolJobSpec;

    #[test]
    fn defaults_correlate_job_identity_and_keep_injected_time() {
        let now = Utc
            .with_ymd_and_hms(2026, 2, 3, 4, 5, 6)
            .single()
            .expect("valid timestamp");
        let job = ToolJobSpec::new(
            "job-1",
            "default",
            "workflow.test",
            "payload",
            "tool.test",
            "sttp:in:test",
            now,
        )
        .build();

        assert_eq!(job.id, "job-1");
        assert_eq!(job.idempotency_key, "idem-job-1");
        assert_eq!(job.correlation_id, "job-1");
        assert_eq!(job.trace_id, "job-1");
        assert_eq!(job.priority, 100);
        assert_eq!(job.max_attempts, 1);
        assert_eq!(job.scheduled_at, now);
    }

    #[test]
    fn overrides_are_explicit() {
        let job = ToolJobSpec::new(
            "job-2",
            "queue",
            "workflow.test",
            "payload",
            "tool.test",
            "sttp:in:test",
            Utc::now(),
        )
        .priority(7)
        .max_attempts(3)
        .correlation_id("workflow-2")
        .trace_id("trace-2")
        .build();

        assert_eq!(job.priority, 7);
        assert_eq!(job.max_attempts, 3);
        assert_eq!(job.correlation_id, "workflow-2");
        assert_eq!(job.trace_id, "trace-2");
    }
}
