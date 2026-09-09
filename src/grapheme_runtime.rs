//! Shared Stasis-backed execution path for portable Grapheme scripts.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use chrono::Utc;
use serde_json::{Value, json};
use stasis::domain::errors::StasisError;
use stasis::domain::runtime::job::JobState;
use stasis::domain::runtime::job_attempt::{JobAttempt, JobAttemptOutcome};
use stasis::prelude::RuntimeComposition;
use uuid::Uuid;

use crate::runtime_composition_ext::{RuntimeCompositionExt, process_once};
use crate::runtime_job_spec::ToolJobSpec;

pub async fn run_grapheme_via_runtime(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
    causation: &str,
) -> stasis::prelude::Result<Value> {
    if crate::grapheme_grants::source_contains_secret_grant(source) {
        return Err(StasisError::PortFailure(
            "ephemeral Grapheme grants require grapheme.invoke with matching secret_grant_ids"
                .to_string(),
        ));
    }
    let job_id = format!("cognition-gph-runtime-{}", Uuid::new_v4().simple());
    let job = ToolJobSpec::new(
        job_id.clone(),
        "default",
        "workflow.grapheme.run",
        format!("grapheme:inline:{source}"),
        causation,
        Utc::now(),
    )
    .build();

    runtime.enqueue_job(job).await?;
    wait_for_grapheme_job(runtime, &job_id, causation, None).await
}

const JOB_WAIT_BUDGET: Duration = Duration::from_secs(60);
const JOB_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Drive the shared queue, but observe only this submission's durable result.
/// A queue pass can select unrelated work, or another worker can own our job.
pub(crate) async fn wait_for_grapheme_job(
    runtime: &Arc<RuntimeComposition>,
    job_id: &str,
    worker_id: &str,
    keep_alive: Option<Arc<crate::grapheme_secret_bridge::GraphemeRunTokenGuard>>,
) -> stasis::prelude::Result<Value> {
    wait_for_job(runtime, job_id, worker_id, JOB_WAIT_BUDGET, keep_alive).await
}

async fn wait_for_job(
    runtime: &Arc<RuntimeComposition>,
    job_id: &str,
    worker_id: &str,
    budget: Duration,
    keep_alive: Option<Arc<crate::grapheme_secret_bridge::GraphemeRunTokenGuard>>,
) -> stasis::prelude::Result<Value> {
    let deadline = Instant::now() + budget;
    let mut pass: Option<tokio::task::JoinHandle<anyhow::Result<Option<String>>>> = None;
    let mut driver_error = None;
    loop {
        let observed = async {
            let job = runtime.get_job(job_id).await?;
            let attempts = runtime.list_job_attempts(job_id).await?;
            Ok::<_, StasisError>((job, attempts))
        };
        let (job, attempts) = match tokio::time::timeout_at(deadline, observed).await {
            Ok(Ok(observed)) => observed,
            Ok(Err(error)) => {
                return Err(StasisError::PortFailure(format!(
                    "cannot observe Grapheme job {job_id}: {error}; execution state is unknown, do not resubmit blindly"
                )));
            }
            Err(_) => return Ok(pending_result(job_id, "unknown", driver_error.as_deref())),
        };
        let Some(job) = job else {
            return Err(StasisError::PortFailure(format!(
                "Grapheme job {job_id} is missing"
            )));
        };
        let terminal = matches!(
            job.state,
            JobState::Succeeded | JobState::Failed | JobState::DeadLetter
        );
        if terminal {
            let expected_attempt = job.attempts + u32::from(job.state == JobState::Succeeded);
            // Stasis writes the terminal job state before its attempt receipt.
            // A Deferred/retry receipt is not evidence for the terminal state.
            if let Some(attempt) = attempts.iter().rev().find(|attempt| {
                attempt.attempt_number == expected_attempt
                    && matches!(
                        (&job.state, &attempt.outcome),
                        (JobState::Succeeded, JobAttemptOutcome::Succeeded)
                            | (
                                JobState::Failed | JobState::DeadLetter,
                                JobAttemptOutcome::FatalFailure
                                    | JobAttemptOutcome::RetryableFailure
                            )
                    )
            }) {
                return Ok(attempt_result(job_id, attempt));
            }
        } else if job.state == JobState::Canceled {
            return Ok(json!({
                "mode": "runtime", "job_id": job_id, "status": "canceled",
                "completed": true, "succeeded": false, "attempt_outcome": Value::Null,
                "execution_id": Value::Null, "diagnostics": {"raw": "Grapheme job was canceled"}
            }));
        }
        if Instant::now() >= deadline {
            let status = match job.state {
                JobState::Enqueued => "queued",
                JobState::Leased | JobState::Running => "running",
                _ => "awaiting_receipt",
            };
            return Ok(pending_result(job_id, status, driver_error.as_deref()));
        }
        if pass.as_ref().is_some_and(|pass| pass.is_finished()) {
            match pass.take().unwrap().await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => driver_error = Some(error.to_string()),
                Err(error) => driver_error = Some(error.to_string()),
            }
        }
        if pass.is_none() && driver_error.is_none() && job.state == JobState::Enqueued {
            let runtime = Arc::clone(runtime);
            let worker_id = worker_id.to_owned();
            let keep_alive = keep_alive.clone();
            // Dropping the waiter/JoinHandle must not abort an in-flight queue
            // pass: that can strand a lease or interrupt unrelated execution.
            pass = Some(tokio::spawn(async move {
                let _keep_alive = keep_alive;
                process_once(&runtime, &worker_id).await
            }));
        }
        tokio::time::sleep_until(deadline.min(Instant::now() + JOB_POLL_INTERVAL)).await;
    }
}

fn pending_result(job_id: &str, status: &str, driver_error: Option<&str>) -> Value {
    json!({
        "mode": "runtime", "job_id": job_id, "status": status,
        "completed": false, "succeeded": Value::Null, "attempt_outcome": Value::Null,
        "execution_id": Value::Null,
        "diagnostics": {
            "raw": "The wait budget elapsed without a terminal receipt. This is not an execution failure. The job may still execute; inspect its result before resubmitting.",
            "result_path": format!("/v1/jobs/{job_id}/result"),
            "driver_error": driver_error,
        }
    })
}

fn attempt_result(job_id: &str, last: &JobAttempt) -> Value {
    let succeeded = last.outcome == JobAttemptOutcome::Succeeded;
    let diagnostics = last
        .diagnostics
        .as_deref()
        .and_then(|diagnostics| serde_json::from_str::<Value>(diagnostics).ok())
        .unwrap_or_else(|| json!({ "raw": last.diagnostics.clone().unwrap_or_default() }));
    json!({
        "mode": "runtime", "job_id": job_id,
        "status": if succeeded { "succeeded" } else { "failed" },
        "completed": true, "succeeded": succeeded,
        "attempt_outcome": format!("{:?}", last.outcome),
        "execution_id": last.execution_id, "diagnostics": diagnostics
    })
}

pub(crate) fn preflight_rejection_reason(validation: &Value) -> &'static str {
    if validation.get("completed").and_then(Value::as_bool) == Some(false) {
        "grapheme_preflight_pending"
    } else {
        "invalid_grapheme_source"
    }
}

pub async fn validate_grapheme_source_for_schedule(
    runtime: &Arc<RuntimeComposition>,
    source: &str,
) -> stasis::prelude::Result<Value> {
    let result = run_grapheme_via_runtime(runtime, source, "cognition_tui_preflight").await?;
    let succeeded = result
        .get("succeeded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let diagnostics_value = result
        .get("diagnostics")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let diagnostics_preview = truncate_for_error(
        &serde_json::to_string_pretty(&diagnostics_value).unwrap_or_else(|_| "{}".to_string()),
        1_600,
    );

    Ok(json!({
        "validated": if result.get("completed").and_then(Value::as_bool) == Some(false) { Value::Null } else { Value::Bool(succeeded) },
        "status": result.get("status").cloned().unwrap_or(Value::Null),
        "completed": result.get("completed").cloned().unwrap_or(Value::Null),
        "mode": "runtime_preflight",
        "job_id": result.get("job_id").cloned().unwrap_or(Value::Null),
        "execution_id": result.get("execution_id").cloned().unwrap_or(Value::Null),
        "attempt_outcome": result.get("attempt_outcome").cloned().unwrap_or(Value::Null),
        "diagnostics": diagnostics_value,
        "diagnostics_preview": diagnostics_preview
    }))
}

fn truncate_for_error(text: &str, max_chars: usize) -> String {
    let out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{out}...")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use stasis::application::runtime::in_memory_runtime::{
        InMemoryRuntime, JobExecutionOutcome, JobHandler,
    };
    use stasis::domain::runtime::job::Job;
    use stasis::ports::outbound::runtime::job_attempt_store::JobAttemptStore;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    struct Handler {
        calls: Arc<AtomicUsize>,
        entered: Arc<Notify>,
        release: Option<Arc<Notify>>,
        fail: bool,
        defer_first: bool,
    }
    #[async_trait]
    impl JobHandler for Handler {
        fn job_type(&self) -> &'static str {
            "workflow.grapheme.run"
        }
        async fn execute(&self, _job: &Job) -> stasis::prelude::Result<JobExecutionOutcome> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            if let Some(release) = &self.release {
                release.notified().await;
            }
            if self.defer_first && call == 0 {
                return Ok(JobExecutionOutcome::Deferred {
                    scheduled_at: Utc::now(),
                    message: "resume later".into(),
                    execution_id: None,
                    diagnostics: None,
                });
            }
            if self.fail {
                Ok(JobExecutionOutcome::FatalFailure {
                    message: "sandbox denied".into(),
                    execution_id: Some("exec-denied".into()),
                    diagnostics: Some(json!({"guardrail_code":"POLICY_VIOLATION"}).to_string()),
                })
            } else {
                Ok(JobExecutionOutcome::Success {
                    output_provenance: None,
                    execution_id: Some("exec-matched".into()),
                    diagnostics: Some(
                        json!({"final_state":{"stdout":"probe result","exit_code":0}}).to_string(),
                    ),
                })
            }
        }
    }
    fn setup(
        block: bool,
        fail: bool,
        defer_first: bool,
    ) -> (
        Arc<RuntimeComposition>,
        Arc<AtomicUsize>,
        Arc<Notify>,
        Arc<Notify>,
    ) {
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let runtime = InMemoryRuntime::new();
        runtime
            .register_handler(Handler {
                calls: calls.clone(),
                entered: entered.clone(),
                release: block.then(|| release.clone()),
                fail,
                defer_first,
            })
            .unwrap();
        (
            Arc::new(RuntimeComposition::InMemory(runtime)),
            calls,
            entered,
            release,
        )
    }
    async fn enqueue(runtime: &RuntimeComposition, id: &str) {
        runtime
            .enqueue_job(
                ToolJobSpec::new(
                    id,
                    "default",
                    "workflow.grapheme.run",
                    "unused",
                    "test",
                    Utc::now(),
                )
                .build(),
            )
            .await
            .unwrap();
    }
    async fn wait(runtime: &Arc<RuntimeComposition>, id: &str, budget: Duration) -> Value {
        wait_for_job(runtime, id, "test-waiter", budget, None)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn shared_queue_work_does_not_become_a_false_preflight_failure() {
        let (runtime, calls, _, _) = setup(false, false, false);
        runtime
            .enqueue_job(
                ToolJobSpec::new(
                    "unrelated",
                    "default",
                    "test.unregistered",
                    "unused",
                    "test",
                    Utc::now(),
                )
                .priority(200)
                .build(),
            )
            .await
            .unwrap();
        let result = run_grapheme_via_runtime(&runtime, "query Probe { shell.status }", "test")
            .await
            .unwrap();
        assert_eq!(result["succeeded"], true);
        assert_eq!(result["execution_id"], "exec-matched");
        assert_eq!(
            result["diagnostics"]["final_state"]["stdout"],
            "probe result"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            runtime.list_job_attempts("unrelated").await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn another_worker_can_claim_the_submission_without_duplicate_execution() {
        let (runtime, calls, entered, release) = setup(true, false, false);
        enqueue(&runtime, "claimed").await;
        let other = runtime.clone();
        let worker =
            tokio::spawn(async move { process_once(&other, "other-worker").await.unwrap() });
        entered.notified().await;
        let waiting = runtime.clone();
        let waiter =
            tokio::spawn(async move { wait(&waiting, "claimed", Duration::from_secs(1)).await });
        release.notify_one();
        assert_eq!(waiter.await.unwrap()["succeeded"], true);
        worker.await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn timeout_preserves_job_identity_and_does_not_abort_inflight_execution() {
        let (runtime, calls, _, release) = setup(true, false, false);
        enqueue(&runtime, "slow").await;
        let pending = wait(&runtime, "slow", Duration::from_millis(30)).await;
        assert_eq!(pending["job_id"], "slow");
        assert_eq!(pending["completed"], false);
        assert!(pending["succeeded"].is_null());
        assert!(
            pending["diagnostics"]["raw"]
                .as_str()
                .unwrap()
                .contains("before resubmitting")
        );
        release.notify_one();
        assert_eq!(
            wait(&runtime, "slow", Duration::from_secs(1)).await["succeeded"],
            true
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn dropping_the_waiter_does_not_abort_its_queue_pass() {
        let (runtime, calls, entered, release) = setup(true, false, false);
        enqueue(&runtime, "abandoned-wait").await;
        let waiting = runtime.clone();
        let waiter =
            tokio::spawn(
                async move { wait(&waiting, "abandoned-wait", Duration::from_secs(1)).await },
            );
        entered.notified().await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        release.notify_one();
        assert_eq!(
            wait(&runtime, "abandoned-wait", Duration::from_secs(1)).await["succeeded"],
            true
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn actual_failure_keeps_its_receipt_and_deferred_work_is_not_terminal() {
        let (runtime, _, _, _) = setup(false, true, false);
        enqueue(&runtime, "denied").await;
        let failure = wait(&runtime, "denied", Duration::from_secs(1)).await;
        assert_eq!(failure["completed"], true);
        assert_eq!(failure["succeeded"], false);
        assert_eq!(failure["diagnostics"]["guardrail_code"], "POLICY_VIOLATION");
        let (runtime, calls, _, _) = setup(false, false, true);
        enqueue(&runtime, "deferred").await;
        assert_eq!(
            wait(&runtime, "deferred", Duration::from_secs(1)).await["succeeded"],
            true
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn canceled_jobs_without_attempts_are_terminal() {
        let (runtime, calls, _, _) = setup(false, false, false);
        enqueue(&runtime, "canceled").await;
        let mut job = runtime.get_job("canceled").await.unwrap().unwrap();
        job.state = JobState::Canceled;
        runtime.save_job(job).await.unwrap();
        let result = wait(&runtime, "canceled", Duration::from_secs(1)).await;
        assert_eq!(result["status"], "canceled");
        assert_eq!(result["completed"], true);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    fn success_receipt(id: &str) -> JobAttempt {
        JobAttempt {
            attempt_id: format!("attempt-{id}"),
            job_id: id.into(),
            attempt_number: 1,
            worker_id: "other".into(),
            started_at: Utc::now(),
            finished_at: Utc::now(),
            outcome: JobAttemptOutcome::Succeeded,
            error_message: None,
            output_provenance: None,
            execution_id: Some("late-receipt".into()),
            guardrail_code: None,
            policy_reason: None,
            duration_ms: None,
            diagnostics: None,
        }
    }
    #[tokio::test]
    async fn terminal_state_waits_for_the_attempt_persistence_barrier() {
        let (runtime, calls, _, _) = setup(false, false, false);
        enqueue(&runtime, "late").await;
        let mut job = runtime.get_job("late").await.unwrap().unwrap();
        job.state = JobState::Succeeded;
        runtime.save_job(job).await.unwrap();
        let writing = runtime.clone();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(60)).await;
            let RuntimeComposition::InMemory(runtime) = writing.as_ref() else {
                unreachable!()
            };
            runtime
                .job_attempt_store
                .insert(success_receipt("late"))
                .await
                .unwrap();
        });
        let result = wait(&runtime, "late", Duration::from_secs(1)).await;
        assert_eq!(result["execution_id"], "late-receipt");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn an_earlier_retry_receipt_cannot_satisfy_a_later_terminal_failure() {
        let (runtime, calls, _, _) = setup(false, false, false);
        enqueue(&runtime, "retried").await;
        let mut job = runtime.get_job("retried").await.unwrap().unwrap();
        job.state = JobState::DeadLetter;
        job.attempts = 2;
        runtime.save_job(job).await.unwrap();
        let RuntimeComposition::InMemory(memory) = runtime.as_ref() else {
            unreachable!()
        };
        let mut earlier = success_receipt("retried");
        earlier.outcome = JobAttemptOutcome::RetryableFailure;
        memory.job_attempt_store.insert(earlier).await.unwrap();
        let pending = wait(&runtime, "retried", Duration::from_millis(30)).await;
        assert_eq!(pending["completed"], false);

        let mut latest = success_receipt("retried");
        latest.attempt_id = "attempt-retried-final".into();
        latest.attempt_number = 2;
        latest.outcome = JobAttemptOutcome::FatalFailure;
        memory.job_attempt_store.insert(latest).await.unwrap();
        let result = wait(&runtime, "retried", Duration::from_secs(1)).await;
        assert_eq!(result["completed"], true);
        assert_eq!(result["attempt_outcome"], "FatalFailure");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn timeout_does_not_abort_unrelated_work_selected_by_the_queue_pass() {
        let (runtime, calls, _, release) = setup(true, false, false);
        enqueue(&runtime, "unrelated-slow").await;
        let mut unrelated = runtime.get_job("unrelated-slow").await.unwrap().unwrap();
        unrelated.priority = 200;
        runtime.save_job(unrelated).await.unwrap();
        enqueue(&runtime, "waiting").await;
        let pending = wait(&runtime, "waiting", Duration::from_millis(30)).await;
        assert_eq!(pending["completed"], false);
        assert_eq!(pending["job_id"], "waiting");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release.notify_one();
        assert_eq!(
            wait(&runtime, "unrelated-slow", Duration::from_secs(1)).await["succeeded"],
            true
        );
        release.notify_one();
        assert_eq!(
            wait(&runtime, "waiting", Duration::from_secs(1)).await["succeeded"],
            true
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
