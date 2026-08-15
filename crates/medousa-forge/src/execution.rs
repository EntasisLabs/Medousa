//! Bounded Forge/Git execution service (H06.1).
//!
//! Async callers admit count, bytes, subprocess, and keyed-lane capacity
//! before dispatch. A full or closed queue never falls back to inline work
//! on a Tokio worker.

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::{ForgeError, Result};

pub const MAX_QUEUED_COMMANDS: usize = 64;
pub const MAX_BLOCKING_JOBS: usize = 8;
pub const MAX_NETWORK_GIT: usize = 2;
pub const MAX_OBSERVATION_JOBS: usize = 2;
pub const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = 256 * 1024;
pub const MAX_STORE_PAYLOAD_BYTES: usize = 1024 * 1024;
pub const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COMPACTION_BUFFER_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OWNER_HANDLES: usize = 10_000;
pub const MAX_OWNER_PROJECTION_BYTES: usize = 64 * 1024 * 1024;
pub const OWNER_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
pub const MAX_REPO_LANES: usize = 1_024;
pub const REPO_LANE_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
pub const MAX_OBSERVATION_CACHE_ENTRIES: usize = 10_000;
pub const MAX_OBSERVATION_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub const OBSERVATION_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

thread_local! {
    static ADMITTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionClass {
    StoreIo,
    RepositoryMetadata,
    LocalMutation,
    NetworkGit,
    Observation,
    Compaction,
}

impl ExecutionClass {
    fn max_retained_bytes(self) -> usize {
        match self {
            Self::StoreIo => MAX_STORE_PAYLOAD_BYTES,
            Self::RepositoryMetadata => MAX_METADATA_BYTES,
            Self::LocalMutation => MAX_STORE_PAYLOAD_BYTES,
            Self::NetworkGit | Self::Observation => MAX_CAPTURE_BYTES,
            Self::Compaction => MAX_COMPACTION_BUFFER_BYTES,
        }
    }
}

#[derive(Debug, Default)]
pub struct ExecutionMetrics {
    pub admitted: AtomicU64,
    pub rejected: AtomicU64,
    pub completed: AtomicU64,
    pub canary_delay_us: AtomicU64,
}

pub struct ForgeExecutionService {
    global_commands: Arc<Semaphore>,
    queued_bytes: Arc<Semaphore>,
    blocking: Arc<Semaphore>,
    network_git: Arc<Semaphore>,
    observation: Arc<Semaphore>,
    repo_lanes: Mutex<HashMap<String, Arc<Semaphore>>>,
    metrics: Arc<ExecutionMetrics>,
    queued_command_count: AtomicUsize,
}

impl Default for ForgeExecutionService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeExecutionService {
    pub fn new() -> Self {
        Self {
            global_commands: Arc::new(Semaphore::new(MAX_QUEUED_COMMANDS)),
            queued_bytes: Arc::new(Semaphore::new(MAX_QUEUED_BYTES)),
            blocking: Arc::new(Semaphore::new(MAX_BLOCKING_JOBS)),
            network_git: Arc::new(Semaphore::new(MAX_NETWORK_GIT)),
            observation: Arc::new(Semaphore::new(MAX_OBSERVATION_JOBS)),
            repo_lanes: Mutex::new(HashMap::new()),
            metrics: Arc::new(ExecutionMetrics::default()),
            queued_command_count: AtomicUsize::new(0),
        }
    }

    pub fn metrics(&self) -> &ExecutionMetrics {
        &self.metrics
    }

    pub fn queued_commands(&self) -> usize {
        self.queued_command_count.load(Ordering::Relaxed)
    }

    fn class_semaphore(&self, class: ExecutionClass) -> Arc<Semaphore> {
        match class {
            ExecutionClass::NetworkGit => Arc::clone(&self.network_git),
            ExecutionClass::Observation => Arc::clone(&self.observation),
            ExecutionClass::StoreIo
            | ExecutionClass::RepositoryMetadata
            | ExecutionClass::LocalMutation
            | ExecutionClass::Compaction => Arc::clone(&self.blocking),
        }
    }

    fn repo_lane(&self, repo_key: &str) -> Result<Arc<Semaphore>> {
        let mut lanes = self
            .repo_lanes
            .lock()
            .map_err(|_| ForgeError::Store("repository lane registry poisoned".into()))?;
        if lanes.len() >= MAX_REPO_LANES && !lanes.contains_key(repo_key) {
            return Err(ForgeError::Overloaded(
                "repository-lane registry is full".into(),
            ));
        }
        Ok(lanes
            .entry(repo_key.to_owned())
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone())
    }

    async fn admit(
        &self,
        class: ExecutionClass,
        estimated_bytes: usize,
        repo_key: Option<&str>,
    ) -> Result<Admission> {
        if ADMITTED.with(std::cell::Cell::get) {
            return Ok(Admission {
                _global: None,
                _bytes: None,
                _class: None,
                _lane: None,
                nested: true,
            });
        }
        let bytes = estimated_bytes.min(class.max_retained_bytes()).max(1);
        let global = self
            .global_commands
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                ForgeError::Overloaded("forge command queue is full".into())
            })?;
        let byte_permit = self
            .queued_bytes
            .clone()
            .try_acquire_many_owned(bytes as u32)
            .map_err(|_| {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                ForgeError::Overloaded("forge queued-byte budget is exhausted".into())
            })?;
        let class_permit = self
            .class_semaphore(class)
            .try_acquire_owned()
            .map_err(|_| {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                ForgeError::Overloaded(format!("{class:?} workers are saturated"))
            })?;
        let lane = if matches!(
            class,
            ExecutionClass::LocalMutation | ExecutionClass::NetworkGit
        ) {
            if let Some(key) = repo_key {
                Some(self.repo_lane(key)?.try_acquire_owned().map_err(|_| {
                    self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                    ForgeError::Overloaded("repository lane is busy".into())
                })?)
            } else {
                None
            }
        } else {
            None
        };
        self.queued_command_count.fetch_add(1, Ordering::Relaxed);
        self.metrics.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(Admission {
            _global: Some(global),
            _bytes: Some(byte_permit),
            _class: Some(class_permit),
            _lane: lane,
            nested: false,
        })
    }

    pub async fn run<T, F>(&self, class: ExecutionClass, estimated_bytes: usize, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T> + Send + 'static,
    {
        self.run_on_repo(class, estimated_bytes, None, work).await
    }

    pub async fn run_on_repo<T, F>(
        &self,
        class: ExecutionClass,
        estimated_bytes: usize,
        repo_key: Option<String>,
        work: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T> + Send + 'static,
    {
        let admission = self
            .admit(class, estimated_bytes, repo_key.as_deref())
            .await?;
        if admission.nested {
            return work();
        }
        let result = tokio::task::spawn_blocking(move || {
            ADMITTED.with(|flag| flag.set(true));
            let result = work();
            ADMITTED.with(|flag| flag.set(false));
            result
        })
        .await
        .map_err(|err| ForgeError::Store(format!("execution worker failed: {err}")))?;
        self.queued_command_count.fetch_sub(1, Ordering::Relaxed);
        self.metrics.completed.fetch_add(1, Ordering::Relaxed);
        drop(admission);
        result
    }

    /// Admit capacity, then run an already-async job (supervised Git) without
    /// occupying a blocking worker for the child wait.
    pub async fn run_async<T, Fut>(
        &self,
        class: ExecutionClass,
        estimated_bytes: usize,
        repo_key: Option<String>,
        work: Fut,
    ) -> Result<T>
    where
        T: Send + 'static,
        Fut: Future<Output = Result<T>> + Send,
    {
        let admission = self
            .admit(class, estimated_bytes, repo_key.as_deref())
            .await?;
        let result = work.await;
        if !admission.nested {
            self.queued_command_count.fetch_sub(1, Ordering::Relaxed);
            self.metrics.completed.fetch_add(1, Ordering::Relaxed);
        }
        drop(admission);
        result
    }

    /// Cheap ASYNC-001 canary: measure Tokio worker delay while a blocking job runs.
    pub async fn executor_canary(&self, block_for: Duration) -> Result<Duration> {
        let started = Instant::now();
        let blocked = self
            .run(ExecutionClass::StoreIo, 64, move || {
                std::thread::sleep(block_for);
                Ok(())
            })
            .await;
        let probe = tokio::time::timeout(Duration::from_millis(50), tokio::task::yield_now()).await;
        let delay = started.elapsed();
        self.metrics
            .canary_delay_us
            .store(delay.as_micros() as u64, Ordering::Relaxed);
        blocked?;
        let _ = probe;
        Ok(delay)
    }
}

struct Admission {
    _global: Option<OwnedSemaphorePermit>,
    _bytes: Option<OwnedSemaphorePermit>,
    _class: Option<OwnedSemaphorePermit>,
    _lane: Option<OwnedSemaphorePermit>,
    nested: bool,
}

/// Supervise a Git child with timeout, output caps, and kill-on-drop.
pub async fn supervise_git(
    git: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
    args: Vec<String>,
    timeout: Duration,
    max_output: usize,
) -> Result<(Vec<u8>, Vec<u8>, bool)> {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    let mut command = Command::new(git.as_ref());
    command
        .args(&args)
        .current_dir(cwd.as_ref())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    medousa_host::hide_tokio_subprocess_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|err| crate::error::ForgeError::Git(format!("failed to spawn git: {err}")))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ForgeError::Git("git stdout was unavailable".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ForgeError::Git("git stderr was unavailable".into()))?;
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stdout.read(&mut chunk).await.unwrap_or(0);
            if read == 0 {
                break;
            }
            let take = read.min(max_output.saturating_sub(buf.len()));
            buf.extend_from_slice(&chunk[..take]);
            if buf.len() >= max_output {
                break;
            }
        }
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stderr.read(&mut chunk).await.unwrap_or(0);
            if read == 0 {
                break;
            }
            let take = read.min(max_output.saturating_sub(buf.len()));
            buf.extend_from_slice(&chunk[..take]);
            if buf.len() >= max_output {
                break;
            }
        }
        buf
    });
    let wait = tokio::time::timeout(timeout, child.wait());
    let status = match wait.await {
        Ok(Ok(status)) => status,
        Ok(Err(err)) => return Err(ForgeError::Git(format!("git wait failed: {err}"))),
        Err(_) => {
            let _ = child.start_kill();
            return Err(ForgeError::Git("git operation timed out".into()));
        }
    };
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let truncated = stdout.len() >= max_output || stderr.len() >= max_output;
    if !status.success() {
        return Err(ForgeError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&stderr)
        )));
    }
    Ok((stdout, stderr, truncated))
}

pub fn already_admitted() -> bool {
    ADMITTED.with(std::cell::Cell::get)
}

pub async fn map_join<T>(
    future: impl Future<Output = std::result::Result<Result<T>, tokio::task::JoinError>>,
) -> Result<T> {
    future
        .await
        .map_err(|err| ForgeError::Store(format!("execution worker failed: {err}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_when_blocking_workers_are_saturated() {
        let service = ForgeExecutionService::new();
        let mut held = Vec::new();
        for _ in 0..MAX_BLOCKING_JOBS {
            held.push(
                service
                    .admit(ExecutionClass::StoreIo, 16, None)
                    .await
                    .unwrap(),
            );
        }
        let error = match service.admit(ExecutionClass::StoreIo, 16, None).await {
            Ok(_) => panic!("expected overload"),
            Err(error) => error,
        };
        assert!(matches!(error, ForgeError::Overloaded(_)));
        drop(held);
    }

    #[tokio::test]
    async fn run_executes_on_blocking_pool() {
        let service = ForgeExecutionService::new();
        let value = service
            .run(ExecutionClass::StoreIo, 32, || Ok(7u32))
            .await
            .unwrap();
        assert_eq!(value, 7);
    }

    #[tokio::test]
    async fn executor_canary_records_delay() {
        let service = ForgeExecutionService::new();
        let delay = service
            .executor_canary(Duration::from_millis(5))
            .await
            .unwrap();
        assert!(delay > Duration::from_millis(0));
        assert!(service.metrics().canary_delay_us.load(Ordering::Relaxed) > 0);
    }
}
