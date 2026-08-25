//! Surreal daemon startup step runner — labels, timeouts, and real errors only.

use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use stasis::prelude::RuntimeBackend;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use tokio::time::timeout;

const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// Bytes before `?` so filesystem operations do not treat SurrealKV engine
/// settings as part of the database path.
pub fn surrealkv_filesystem_path(path: &str) -> &str {
    path.split_once('?')
        .map(|(prefix, _)| prefix)
        .unwrap_or(path)
}

/// Prepare a persistent runtime backend before Stasis opens it.
pub fn ensure_runtime_backend_prerequisites(backend: &RuntimeBackend) -> Result<()> {
    if let RuntimeBackend::SurrealKv { path, .. } = backend {
        let path_buf = PathBuf::from(surrealkv_filesystem_path(path));
        if let Some(parent) = path_buf.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create SurrealKV runtime directory {}",
                    parent.display()
                )
            })?;
        }

        clear_stale_surrealkv_lock(backend)?;
    }

    Ok(())
}

/// Remove a leftover SurrealKV `LOCK` file before opening the database.
pub fn clear_stale_surrealkv_lock(backend: &RuntimeBackend) -> Result<()> {
    let Some(lock_path) = surrealkv_lock_path(backend) else {
        return Ok(());
    };
    if !lock_path.exists() {
        return Ok(());
    }

    std::fs::remove_file(&lock_path).with_context(|| {
        format!(
            "failed to remove stale SurrealKV lock at {} — another medousa_daemon may be running. \
             Stop it before retrying, or remove the lock manually if no daemon is running.",
            lock_path.display()
        )
    })
}

/// Path to the SurrealKV lock file for diagnostics (`None` for non-KV backends).
pub fn surrealkv_lock_path(backend: &RuntimeBackend) -> Option<PathBuf> {
    match backend {
        RuntimeBackend::SurrealKv { path, .. } => {
            Some(PathBuf::from(surrealkv_filesystem_path(path)).join("LOCK"))
        }
        _ => None,
    }
}

/// Remove the SurrealKV lock file during graceful shutdown.
pub fn remove_surrealkv_lock(backend: &RuntimeBackend) {
    let Some(lock_path) = surrealkv_lock_path(backend) else {
        return;
    };
    if lock_path.exists()
        && let Err(err) = std::fs::remove_file(&lock_path)
    {
        tracing::warn!(
            path = %lock_path.display(),
            error = %err,
            "failed to remove SurrealKV lock file during shutdown"
        );
    }
}

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
            eprintln!(
                "medousa-daemon: step failed label={label} elapsed_ms={elapsed_ms} error={err}"
            );
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

#[cfg(all(test, feature = "full-daemon"))]
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
        let _env = crate::test_env::set_var("MEDOUSA_SURREAL_STEP_TIMEOUT_SECS", "1");
        let err = timed_step("slow write", || async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            Ok::<(), _>(())
        })
        .await
        .expect_err("step should time out");
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

#[cfg(test)]
mod backend_tests {
    use super::*;

    #[test]
    fn persistent_prerequisites_strip_query_and_clear_stale_lock() {
        let sandbox = tempfile::tempdir().expect("sandbox");
        let database = sandbox.path().join("runtime.surrealkv");
        std::fs::create_dir_all(&database).expect("database directory");
        std::fs::write(database.join("LOCK"), b"stale").expect("stale lock");
        let backend = RuntimeBackend::surreal_kv(
            format!("{}?surrealkv_max_memtable_size=1048576", database.display()),
            "medousa",
            "runtime",
        );

        ensure_runtime_backend_prerequisites(&backend).expect("runtime prerequisites");

        assert_eq!(surrealkv_lock_path(&backend), Some(database.join("LOCK")));
        assert!(!database.join("LOCK").exists());
    }
}
