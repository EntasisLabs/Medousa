//! Surreal daemon startup diagnostics — pin down wedged queries without guessing.

use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use tokio::time::timeout;

const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// Run one Surreal startup step with a wall-clock label and per-step timeout.
pub async fn timed_step<T, F, Fut>(label: &str, step: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let started = Instant::now();
    eprintln!("medousa-daemon: surreal step start pid={} label={label}", std::process::id());
    let result = timeout(DEFAULT_STEP_TIMEOUT, step())
        .await
        .with_context(|| {
            format!(
                "surreal step timed out after {}s at `{label}` — likely DB lock, wedged WS response, or restart storm (check `pgrep -x medousa_daemon`)",
                DEFAULT_STEP_TIMEOUT.as_secs()
            )
        })??;
    eprintln!(
        "medousa-daemon: surreal step ok pid={} label={label} elapsed_ms={}",
        std::process::id(),
        started.elapsed().as_millis()
    );
    Ok(result)
}

/// Cheap connectivity probe after connect — fails fast if the router is wedged.
pub async fn verify_surreal_responsive(db: &Surreal<Any>) -> Result<()> {
    timed_step("ping INFO FOR NS", || async {
        db.query("INFO FOR NS")
            .await
            .map(|_| ())
            .map_err(|err| anyhow::anyhow!("INFO FOR NS failed: {err}"))
    })
    .await
}
