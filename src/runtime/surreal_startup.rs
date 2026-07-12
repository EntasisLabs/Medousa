//! Surreal daemon startup step runner — labels, timeouts, and real errors only.

use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use tokio::time::timeout;

const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(30);

fn resolve_step_timeout() -> Duration {
    std::env::var("MEDOUSA_SURREAL_STEP_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_STEP_TIMEOUT)
}

/// Run one labeled startup step with a wall-clock timeout.
///
/// Timeout length: `MEDOUSA_SURREAL_STEP_TIMEOUT_SECS` (default 30).
pub async fn timed_step<T, F, Fut>(label: &str, step: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let limit = resolve_step_timeout();
    let started = Instant::now();
    tracing::info!(
        step = label,
        timeout_secs = limit.as_secs(),
        "startup step begin"
    );
    eprintln!(
        "medousa-daemon: step begin label={label} timeout_secs={}",
        limit.as_secs()
    );

    match timeout(limit, step()).await {
        Ok(Ok(value)) => {
            let elapsed_ms = started.elapsed().as_millis();
            tracing::info!(step = label, elapsed_ms, "startup step ok");
            eprintln!("medousa-daemon: step ok label={label} elapsed_ms={elapsed_ms}");
            Ok(value)
        }
        Ok(Err(err)) => {
            let elapsed_ms = started.elapsed().as_millis();
            tracing::error!(step = label, elapsed_ms, error = %err, "startup step failed");
            eprintln!("medousa-daemon: step failed label={label} elapsed_ms={elapsed_ms} error={err}");
            Err(anyhow::anyhow!(
                "startup step `{label}` failed after {elapsed_ms}ms: {err}"
            ))
        }
        Err(_) => {
            let elapsed_ms = started.elapsed().as_millis();
            tracing::error!(
                step = label,
                elapsed_ms,
                timeout_secs = limit.as_secs(),
                "startup step timed out"
            );
            eprintln!(
                "medousa-daemon: step timed out label={label} elapsed_ms={elapsed_ms} timeout_secs={}",
                limit.as_secs()
            );
            Err(anyhow::anyhow!(
                "startup step `{label}` timed out after {elapsed_ms}ms (limit {}s)",
                limit.as_secs()
            ))
        }
    }
}

/// Cheap connectivity probe after connect.
pub async fn verify_surreal_responsive(db: &Surreal<Any>) -> Result<()> {
    timed_step("ping INFO FOR NS", || async {
        db.query("INFO FOR NS")
            .await
            .map(|_| ())
            .map_err(|err| anyhow::anyhow!("INFO FOR NS: {err}"))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn timed_step_surfaces_inner_error_without_guessing() {
        let err = timed_step("example read", || async {
            Err::<(), _>(anyhow::anyhow!("SELECT blocked: permission denied"))
        })
        .await
        .expect_err("step should fail");

        let message = format!("{err:#}");
        assert!(message.contains("example read"));
        assert!(message.contains("permission denied"));
        assert!(!message.contains("likely"));
        assert!(!message.contains("wedged"));
    }

    #[tokio::test]
    async fn timed_step_timeout_message_is_factual() {
        unsafe {
            std::env::set_var("MEDOUSA_SURREAL_STEP_TIMEOUT_SECS", "1");
        }
        let err = timed_step("slow write", || async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            Ok::<(), _>(())
        })
        .await
        .expect_err("step should time out");

        unsafe {
            std::env::remove_var("MEDOUSA_SURREAL_STEP_TIMEOUT_SECS");
        }
        let message = format!("{err:#}");
        assert!(message.contains("slow write"));
        assert!(message.contains("timed out"));
        assert!(message.contains("limit 1s"));
        assert!(!message.contains("likely"));
    }

    #[tokio::test]
    async fn timed_step_ok_reports_elapsed() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let value = timed_step("fast noop", || async {
            CALLS.fetch_add(1, Ordering::SeqCst);
            Ok::<_, anyhow::Error>(7)
        })
        .await
        .expect("step should succeed");
        assert_eq!(value, 7);
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }
}
