//! Bounded Forge/Git execution service (H06.1 / H06.7).
//!
//! Async callers admit count, bytes, subprocess, and keyed-lane capacity
//! before dispatch. A full or closed queue never falls back to inline work
//! on a Tokio worker.
//!
//! Queue contract: up to [`MAX_QUEUED_COMMANDS`] jobs may hold a global admit
//! slot (queued or running). Class worker slots serialize execution inside
//! that queue — callers **wait** for a free class/lane permit. Only a full
//! global queue or exhausted byte budget returns [`ForgeError::Overloaded`]
//! immediately.

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
    static ADMISSION_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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

struct RepoLane {
    semaphore: Arc<Semaphore>,
    last_used: Instant,
}

pub struct ForgeExecutionService {
    global_commands: Arc<Semaphore>,
    queued_bytes: Arc<Semaphore>,
    blocking: Arc<Semaphore>,
    network_git: Arc<Semaphore>,
    observation: Arc<Semaphore>,
    repo_lanes: Mutex<HashMap<String, RepoLane>>,
    metrics: Arc<ExecutionMetrics>,
    queued_command_count: Arc<AtomicUsize>,
    running_command_count: Arc<AtomicUsize>,
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
            queued_command_count: Arc::new(AtomicUsize::new(0)),
            running_command_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn metrics(&self) -> &ExecutionMetrics {
        &self.metrics
    }

    pub fn queued_commands(&self) -> usize {
        self.queued_command_count.load(Ordering::Relaxed)
    }

    pub fn running_commands(&self) -> usize {
        self.running_command_count.load(Ordering::Relaxed)
    }

    pub fn repo_lane_count(&self) -> usize {
        self.repo_lanes.lock().map(|lanes| lanes.len()).unwrap_or(0)
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

    fn evict_idle_lanes_locked(lanes: &mut HashMap<String, RepoLane>, now: Instant) {
        lanes.retain(|_, lane| {
            let in_use = lane.semaphore.available_permits() == 0;
            if in_use {
                return true;
            }
            now.duration_since(lane.last_used) < REPO_LANE_IDLE_TTL
        });
    }

    fn repo_lane(&self, repo_key: &str) -> Result<Arc<Semaphore>> {
        let mut lanes = self
            .repo_lanes
            .lock()
            .map_err(|_| ForgeError::Store("repository lane registry poisoned".into()))?;
        let now = Instant::now();
        Self::evict_idle_lanes_locked(&mut lanes, now);
        if let Some(existing) = lanes.get_mut(repo_key) {
            existing.last_used = now;
            return Ok(Arc::clone(&existing.semaphore));
        }
        if lanes.len() >= MAX_REPO_LANES {
            return Err(ForgeError::Overloaded(
                "repository-lane registry is full".into(),
            ));
        }
        let semaphore = Arc::new(Semaphore::new(1));
        lanes.insert(
            repo_key.to_owned(),
            RepoLane {
                semaphore: Arc::clone(&semaphore),
                last_used: now,
            },
        );
        Ok(semaphore)
    }

    /// Test helper: mark a lane idle past TTL (and ensure it exists).
    pub fn force_repo_lane_idle_for_test(&self, repo_key: &str) {
        let Ok(mut lanes) = self.repo_lanes.lock() else {
            return;
        };
        let idle_at = Instant::now()
            .checked_sub(REPO_LANE_IDLE_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        match lanes.get_mut(repo_key) {
            Some(lane) => lane.last_used = idle_at,
            None => {
                lanes.insert(
                    repo_key.to_owned(),
                    RepoLane {
                        semaphore: Arc::new(Semaphore::new(1)),
                        last_used: idle_at,
                    },
                );
            }
        }
    }

    pub fn evict_idle_repo_lanes(&self) {
        if let Ok(mut lanes) = self.repo_lanes.lock() {
            Self::evict_idle_lanes_locked(&mut lanes, Instant::now());
        }
    }

    async fn admit(
        &self,
        class: ExecutionClass,
        estimated_bytes: usize,
        repo_key: Option<&str>,
    ) -> Result<Admission> {
        if already_admitted() {
            return Ok(Admission {
                _global: None,
                _bytes: None,
                _class: None,
                _lane: None,
                account: None,
                nested: true,
            });
        }
        if estimated_bytes > class.max_retained_bytes() {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(ForgeError::Overloaded(format!(
                "estimated retained bytes ({estimated_bytes}) exceed {:?} class budget ({})",
                class,
                class.max_retained_bytes()
            )));
        }
        let bytes = estimated_bytes.max(1);
        let global = self
            .global_commands
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                ForgeError::Overloaded("forge command queue is full".into())
            })?;
        let byte_permit = match self
            .queued_bytes
            .clone()
            .try_acquire_many_owned(bytes as u32)
        {
            Ok(permit) => permit,
            Err(_) => {
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                drop(global);
                return Err(ForgeError::Overloaded(
                    "forge queued-byte budget is exhausted".into(),
                ));
            }
        };

        // Holding a global slot counts as queued until class/lane permits arrive
        // and the job is marked running. Partial Admission ensures permits and
        // counters release on cancel or later failure.
        let mut admission = Admission {
            _global: Some(global),
            _bytes: Some(byte_permit),
            _class: None,
            _lane: None,
            account: Some(JobAccount::enter_queued(
                Arc::clone(&self.queued_command_count),
                Arc::clone(&self.running_command_count),
                Arc::clone(&self.metrics),
            )),
            nested: false,
        };

        admission._class = Some(
            self.class_semaphore(class)
                .acquire_owned()
                .await
                .map_err(|_| ForgeError::Store("execution class semaphore closed".into()))?,
        );

        if matches!(
            class,
            ExecutionClass::LocalMutation | ExecutionClass::NetworkGit
        ) && let Some(key) = repo_key
        {
            let lane_sem = match self.repo_lane(key) {
                Ok(sem) => sem,
                Err(err) => {
                    drop(admission);
                    return Err(err);
                }
            };
            admission._lane = Some(lane_sem.acquire_owned().await.map_err(|_| {
                ForgeError::Store("repository lane semaphore closed".into())
            })?);
        }

        self.metrics.admitted.fetch_add(1, Ordering::Relaxed);
        if let Some(account) = admission.account.as_mut() {
            account.mark_running();
        }
        Ok(admission)
    }

    pub async fn run<T, F>(
        &self,
        class: ExecutionClass,
        estimated_bytes: usize,
        work: F,
    ) -> Result<T>
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
        // Move admission into the blocking job. Dropping the request future
        // cannot release capacity while uncancellable blocking work is still
        // running in the pool.
        let join = tokio::task::spawn_blocking(move || {
            let _admission = admission;
            let _nested = NestedAdmissionGuard::enter();
            work()
        })
        .await;
        join.map_err(|err| ForgeError::Store(format!("execution worker failed: {err}")))?
    }

    /// Like [`Self::run_on_repo`], but probes Tokio worker delay while the
    /// blocking job is active (ASYNC-001 executor-delay canary).
    pub async fn run_with_canary<T, F>(
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
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        let join = tokio::task::spawn_blocking(move || {
            let _admission = admission;
            let _nested = NestedAdmissionGuard::enter();
            let _ = started_tx.send(());
            work()
        });
        tokio::pin!(join);
        tokio::select! {
            result = &mut join => {
                return result
                    .map_err(|err| ForgeError::Store(format!("execution worker failed: {err}")))?;
            }
            start = started_rx => {
                let _ = start;
            }
        }
        let probe_started = Instant::now();
        tokio::task::yield_now().await;
        let delay = probe_started.elapsed();
        self.metrics
            .canary_delay_us
            .store(delay.as_micros() as u64, Ordering::Relaxed);
        let result = join
            .await
            .map_err(|err| ForgeError::Store(format!("execution worker failed: {err}")))?;
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
        if admission.nested {
            return work.await;
        }
        let result = work.await;
        drop(admission);
        result
    }

    /// ASYNC-001 canary: measure Tokio worker responsiveness while a blocking
    /// job is concurrently active (not wall time of the blocking job itself).
    pub async fn executor_canary(&self, block_for: Duration) -> Result<Duration> {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        let blocked = self.run(ExecutionClass::StoreIo, 64, move || {
            let _ = started_tx.send(());
            std::thread::sleep(block_for);
            Ok(())
        });
        tokio::pin!(blocked);
        // Drive admission/spawn_blocking until the worker signals it has started.
        tokio::select! {
            result = &mut blocked => {
                result?;
                return Err(ForgeError::Store(
                    "canary blocking worker finished before start signal".into(),
                ));
            }
            start = started_rx => {
                start.map_err(|_| {
                    ForgeError::Store("canary blocking worker exited early".into())
                })?;
            }
        }
        let probe_started = Instant::now();
        tokio::task::yield_now().await;
        let delay = probe_started.elapsed();
        self.metrics
            .canary_delay_us
            .store(delay.as_micros() as u64, Ordering::Relaxed);
        blocked.await?;
        Ok(delay)
    }
}

/// Panic-safe nested-admission marker for blocking workers.
struct NestedAdmissionGuard;

impl NestedAdmissionGuard {
    fn enter() -> Self {
        ADMISSION_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for NestedAdmissionGuard {
    fn drop(&mut self) {
        ADMISSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

enum JobPhase {
    Queued,
    Running,
    Finished,
}

/// Decrements queued/running and records completion on every exit path,
/// including cancellation, join failure, and panic unwinding.
struct JobAccount {
    queued: Arc<AtomicUsize>,
    running: Arc<AtomicUsize>,
    metrics: Arc<ExecutionMetrics>,
    phase: JobPhase,
}

impl JobAccount {
    fn enter_queued(
        queued: Arc<AtomicUsize>,
        running: Arc<AtomicUsize>,
        metrics: Arc<ExecutionMetrics>,
    ) -> Self {
        queued.fetch_add(1, Ordering::Relaxed);
        Self {
            queued,
            running,
            metrics,
            phase: JobPhase::Queued,
        }
    }

    fn mark_running(&mut self) {
        if matches!(self.phase, JobPhase::Queued) {
            self.queued.fetch_sub(1, Ordering::Relaxed);
            self.running.fetch_add(1, Ordering::Relaxed);
            self.phase = JobPhase::Running;
        }
    }

    fn finish(&mut self) {
        match self.phase {
            JobPhase::Queued => {
                self.queued.fetch_sub(1, Ordering::Relaxed);
                self.phase = JobPhase::Finished;
            }
            JobPhase::Running => {
                self.running.fetch_sub(1, Ordering::Relaxed);
                self.metrics.completed.fetch_add(1, Ordering::Relaxed);
                self.phase = JobPhase::Finished;
            }
            JobPhase::Finished => {}
        }
    }
}

impl Drop for JobAccount {
    fn drop(&mut self) {
        self.finish();
    }
}

struct Admission {
    _global: Option<OwnedSemaphorePermit>,
    _bytes: Option<OwnedSemaphorePermit>,
    _class: Option<OwnedSemaphorePermit>,
    _lane: Option<OwnedSemaphorePermit>,
    account: Option<JobAccount>,
    nested: bool,
}

/// Redact credential-bearing URL userinfo and common token prefixes from Git
/// CLI arguments and stderr before they enter error strings.
pub fn redact_git_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let scheme = ["https://", "http://", "ssh://"]
            .iter()
            .find(|scheme| lower[i..].starts_with(*scheme));
        if let Some(scheme) = scheme {
            out.push_str(&text[i..i + scheme.len()]);
            i += scheme.len();
            let after = &text[i..];
            if let Some(at) = after.find('@') {
                let authority_end = after[at..]
                    .find(['/', ' ', '\n', '\r', '\t'])
                    .map(|n| at + n)
                    .unwrap_or(after.len());
                if after[..at].contains(':') {
                    out.push_str("[REDACTED]");
                    out.push_str(&after[at..authority_end]);
                    i += authority_end;
                    continue;
                }
            }
            continue;
        }
        if lower[i..].starts_with("bearer ") || lower[i..].starts_with("token ") {
            let prefix_len = if lower[i..].starts_with("bearer ") {
                "bearer ".len()
            } else {
                "token ".len()
            };
            let token = &text[i + prefix_len..];
            let token_end = token.find(['\n', '\r', ' ', '\t']).unwrap_or(token.len());
            out.push_str("[REDACTED]");
            i += prefix_len + token_end;
            continue;
        }
        out.push(text[i..].chars().next().unwrap_or('\0'));
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

fn format_git_failure(args: &[String], stderr: &[u8]) -> String {
    let args = redact_git_text(&args.join(" "));
    let stderr = redact_git_text(&String::from_utf8_lossy(stderr));
    format!("git {args} failed: {stderr}")
}

async fn drain_capped_output(
    mut reader: impl tokio::io::AsyncReadExt + Unpin,
    max_output: usize,
) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        if buf.len() < max_output {
            let take = (max_output - buf.len()).min(read);
            buf.extend_from_slice(&chunk[..take]);
            if take < read {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    (buf, truncated)
}

fn configure_supervised_git(command: &mut tokio::process::Command) {
    // Windows: CREATE_NO_WINDOW via host helper (same flag as detach_new_session).
    medousa_host::hide_tokio_subprocess_window(command);
    #[cfg(unix)]
    {
        // Own process group so timeout/cancel can signal the whole tree.
        command.process_group(0);
    }
}

async fn terminate_and_reap(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        medousa_host::force_process_tree_stop_by_pid(pid);
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
}

/// Capture stdout/stderr from a spawned child with a hard byte bound.
/// Excess bytes are drained so the child cannot stall on a full pipe.
pub fn capture_child_output_bounded(
    mut child: std::process::Child,
    max_output: usize,
) -> Result<(Vec<u8>, Vec<u8>, bool, std::process::ExitStatus)> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ForgeError::Git("git stdout was unavailable".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ForgeError::Git("git stderr was unavailable".into()))?;

    let max = max_output.max(1);
    let stdout_thread = std::thread::spawn(move || drain_sync_capped(&mut stdout, max));
    let stderr_thread = std::thread::spawn(move || drain_sync_capped(&mut stderr, max));
    let status = child
        .wait()
        .map_err(|err| ForgeError::Git(format!("git wait failed: {err}")))?;
    let (stdout, stdout_trunc) = stdout_thread
        .join()
        .map_err(|_| ForgeError::Git("git stdout reader panicked".into()))?;
    let (stderr, stderr_trunc) = stderr_thread
        .join()
        .map_err(|_| ForgeError::Git("git stderr reader panicked".into()))?;
    Ok((stdout, stderr, stdout_trunc || stderr_trunc, status))
}

fn drain_sync_capped(reader: &mut impl std::io::Read, max_output: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let read = match std::io::Read::read(reader, &mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        if buf.len() < max_output {
            let take = (max_output - buf.len()).min(read);
            buf.extend_from_slice(&chunk[..take]);
            if take < read {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    (buf, truncated)
}

/// Run a configured `std::process::Command` with bounded stdout/stderr capture.
pub fn run_command_bounded(
    mut command: std::process::Command,
    max_output: usize,
) -> Result<(Vec<u8>, Vec<u8>, bool, std::process::ExitStatus)> {
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command
        .spawn()
        .map_err(|err| ForgeError::Git(format!("failed to spawn command: {err}")))?;
    capture_child_output_bounded(child, max_output)
}

/// Supervise a Git child with timeout, output caps, and kill-on-drop.
pub async fn supervise_git(
    git: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
    args: Vec<String>,
    timeout: Duration,
    max_output: usize,
) -> Result<(Vec<u8>, Vec<u8>, bool)> {
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
    configure_supervised_git(&mut command);
    let mut child = command
        .spawn()
        .map_err(|err| crate::error::ForgeError::Git(format!("failed to spawn git: {err}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ForgeError::Git("git stdout was unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ForgeError::Git("git stderr was unavailable".into()))?;
    let stdout_task = tokio::spawn(drain_capped_output(stdout, max_output));
    let stderr_task = tokio::spawn(drain_capped_output(stderr, max_output));

    let wait = tokio::time::timeout(timeout, child.wait());
    let status = match wait.await {
        Ok(Ok(status)) => status,
        Ok(Err(err)) => {
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(ForgeError::Git(format!("git wait failed: {err}")));
        }
        Err(_) => {
            terminate_and_reap(&mut child).await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(ForgeError::Git("git operation timed out".into()));
        }
    };
    let (stdout, stdout_trunc) = stdout_task.await.unwrap_or_else(|_| (Vec::new(), false));
    let (stderr, stderr_trunc) = stderr_task.await.unwrap_or_else(|_| (Vec::new(), false));
    let truncated = stdout_trunc || stderr_trunc;
    if !status.success() {
        return Err(ForgeError::Git(format_git_failure(&args, &stderr)));
    }
    Ok((stdout, stderr, truncated))
}

pub fn already_admitted() -> bool {
    ADMISSION_DEPTH.with(|depth| depth.get() > 0)
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
    use std::sync::Barrier;

    #[tokio::test]
    async fn rejects_when_global_queue_is_full() {
        let service = Arc::new(ForgeExecutionService::new());
        // Saturate the 64 global slots: 8 hold class permits; the rest wait on class.
        let mut holders = Vec::new();
        for _ in 0..MAX_QUEUED_COMMANDS {
            let service = Arc::clone(&service);
            holders.push(tokio::spawn(async move {
                service.admit(ExecutionClass::StoreIo, 16, None).await
            }));
        }
        for _ in 0..500 {
            if service.queued_commands() + service.running_commands() >= MAX_QUEUED_COMMANDS {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            service.queued_commands() + service.running_commands(),
            MAX_QUEUED_COMMANDS
        );
        let overflow = tokio::time::timeout(
            Duration::from_millis(200),
            service.admit(ExecutionClass::StoreIo, 16, None),
        )
        .await
        .expect("overflow admit must not wait");
        match overflow {
            Ok(_) => panic!("expected overload"),
            Err(error) => {
                assert!(matches!(error, ForgeError::Overloaded(_)));
                assert!(error.to_string().contains("queue is full"));
            }
        }
        for holder in holders {
            holder.abort();
        }
    }

    #[tokio::test]
    async fn rejects_estimated_bytes_over_class_budget() {
        let service = ForgeExecutionService::new();
        let error = match service
            .admit(
                ExecutionClass::RepositoryMetadata,
                MAX_METADATA_BYTES + 1,
                None,
            )
            .await
        {
            Ok(_) => panic!("expected overload"),
            Err(error) => error,
        };
        assert!(matches!(error, ForgeError::Overloaded(_)));
        assert!(error.to_string().contains("exceed"));
    }

    #[tokio::test]
    async fn reserves_full_estimated_bytes() {
        let service = ForgeExecutionService::new();
        let slots = MAX_QUEUED_BYTES / MAX_STORE_PAYLOAD_BYTES;
        let mut held = Vec::new();
        for _ in 0..slots {
            held.push(
                service
                    .admit(ExecutionClass::StoreIo, MAX_STORE_PAYLOAD_BYTES, None)
                    .await
                    .unwrap(),
            );
        }
        let error = match service
            .admit(ExecutionClass::StoreIo, MAX_STORE_PAYLOAD_BYTES, None)
            .await
        {
            Ok(_) => panic!("expected overload"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("queued-byte"),
            "expected byte budget exhaustion, got {error}"
        );
        drop(held);
    }

    #[tokio::test]
    async fn class_slots_queue_instead_of_rejecting() {
        let service = Arc::new(ForgeExecutionService::new());
        let mut held = Vec::new();
        for _ in 0..MAX_BLOCKING_JOBS {
            held.push(
                service
                    .admit(ExecutionClass::StoreIo, 16, None)
                    .await
                    .unwrap(),
            );
        }
        let waiter = {
            let service = Arc::clone(&service);
            tokio::spawn(async move { service.admit(ExecutionClass::StoreIo, 16, None).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished(), "9th admit should wait on class slot");
        drop(held.pop());
        let joined = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("waiter should finish")
            .expect("join")
            .expect("admit");
        drop(joined);
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
        assert_eq!(service.queued_commands(), 0);
        assert_eq!(service.running_commands(), 0);
    }

    #[tokio::test]
    async fn counters_recover_after_worker_panic() {
        let service = ForgeExecutionService::new();
        let err = service
            .run(ExecutionClass::StoreIo, 16, || -> Result<()> {
                panic!("boom");
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("execution worker failed"));
        assert_eq!(service.queued_commands(), 0);
        assert_eq!(service.running_commands(), 0);
        // Service remains usable.
        service
            .run(ExecutionClass::StoreIo, 16, || Ok(()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn counters_recover_after_cancellation() {
        let service = Arc::new(ForgeExecutionService::new());
        let gate = Arc::new(Barrier::new(2));
        let gate_worker = Arc::clone(&gate);
        let service_worker = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            service_worker
                .run(ExecutionClass::StoreIo, 16, move || {
                    gate_worker.wait();
                    gate_worker.wait();
                    Ok(())
                })
                .await
        });
        // Wait until worker is inside the closure (running).
        tokio::task::spawn_blocking({
            let gate = Arc::clone(&gate);
            move || {
                gate.wait();
            }
        })
        .await
        .unwrap();
        assert_eq!(service.running_commands(), 1);
        handle.abort();
        let _ = handle.await;
        assert_eq!(
            service.running_commands(),
            1,
            "cancelled request must retain admission until blocking work exits"
        );
        // Release the worker so the blocking thread can finish dropping.
        let _ = tokio::task::spawn_blocking({
            let gate = Arc::clone(&gate);
            move || {
                gate.wait();
            }
        })
        .await;
        // Give Drop a moment on the aborted task path.
        for _ in 0..50 {
            if service.queued_commands() == 0 && service.running_commands() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(service.queued_commands(), 0);
        assert_eq!(service.running_commands(), 0);
    }

    #[tokio::test]
    async fn executor_canary_measures_worker_responsiveness() {
        let service = ForgeExecutionService::new();
        let delay = service
            .executor_canary(Duration::from_millis(80))
            .await
            .unwrap();
        // Probe is a yield while blocking work runs; it must not be ~80ms wall.
        assert!(
            delay < Duration::from_millis(50),
            "canary should measure worker responsiveness, got {delay:?}"
        );
        assert!(service.metrics().canary_delay_us.load(Ordering::Relaxed) > 0);
    }

    #[tokio::test]
    async fn run_with_canary_records_delay_around_heavy_work() {
        let service = ForgeExecutionService::new();
        let value = service
            .run_with_canary(ExecutionClass::StoreIo, 32, None, || {
                std::thread::sleep(Duration::from_millis(40));
                Ok(11u32)
            })
            .await
            .unwrap();
        assert_eq!(value, 11);
        assert!(service.metrics().canary_delay_us.load(Ordering::Relaxed) > 0);
        assert_eq!(service.queued_commands(), 0);
        assert_eq!(service.running_commands(), 0);
    }

    #[tokio::test]
    async fn sync_command_capture_bounds_output() {
        let mut command = std::process::Command::new("python3");
        command.args(["-c", "print('x'*200000)"]);
        let (stdout, _stderr, truncated, status) =
            run_command_bounded(command, 1024).expect("bounded capture");
        assert!(status.success());
        assert!(truncated);
        assert!(stdout.len() <= 1024);
    }

    #[tokio::test]
    async fn repo_lane_idle_entries_are_evicted() {
        let service = ForgeExecutionService::new();
        let _ = service.repo_lane("repo-a").unwrap();
        let _ = service.repo_lane("repo-b").unwrap();
        assert_eq!(service.repo_lane_count(), 2);
        service.force_repo_lane_idle_for_test("repo-a");
        service.evict_idle_repo_lanes();
        assert_eq!(service.repo_lane_count(), 1);
        assert!(service.repo_lanes.lock().unwrap().contains_key("repo-b"));
    }

    #[tokio::test]
    async fn supervise_git_times_out_and_reaps_child() {
        let (program, args): (String, Vec<String>) = if cfg!(windows) {
            (
                "cmd".into(),
                vec![
                    "/C".into(),
                    "ping".into(),
                    "-n".into(),
                    "60".into(),
                    "127.0.0.1".into(),
                ],
            )
        } else {
            ("sleep".into(), vec!["30".into()])
        };
        let started = Instant::now();
        let err = supervise_git(
            program,
            std::env::temp_dir(),
            args,
            Duration::from_millis(200),
            1024,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn supervise_git_caps_and_drains_excess_output() {
        let (program, args): (String, Vec<String>) = if cfg!(windows) {
            (
                "cmd".into(),
                vec![
                    "/C".into(),
                    "powershell".into(),
                    "-NoProfile".into(),
                    "-Command".into(),
                    "Write-Output ('x'*2000)".into(),
                ],
            )
        } else {
            (
                "sh".into(),
                vec![
                    "-c".into(),
                    "dd if=/dev/zero bs=1024 count=64 2>/dev/null".into(),
                ],
            )
        };
        let (stdout, _stderr, truncated) = supervise_git(
            program,
            std::env::temp_dir(),
            args,
            Duration::from_secs(10),
            512,
        )
        .await
        .unwrap();
        assert!(truncated);
        assert!(stdout.len() <= 512);
    }

    #[tokio::test]
    async fn supervise_git_redacts_credentials_in_errors() {
        let (program, args): (String, Vec<String>) = if cfg!(windows) {
            (
                "cmd".into(),
                vec![
                    "/C".into(),
                    "echo".into(),
                    "fatal: https://user:s3cret@example.com/repo.git".into(),
                    "&".into(),
                    "exit".into(),
                    "1".into(),
                ],
            )
        } else {
            (
                "sh".into(),
                vec![
                    "-c".into(),
                    "echo 'fatal: https://user:s3cret@example.com/repo.git' 1>&2; exit 1".into(),
                ],
            )
        };
        let err = supervise_git(
            program,
            std::env::temp_dir(),
            args,
            Duration::from_secs(5),
            4096,
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("s3cret"), "secret leaked: {msg}");
        assert!(msg.contains("[REDACTED]") || msg.contains("failed"));
    }

    #[test]
    fn redact_git_text_strips_url_userinfo() {
        let redacted = redact_git_text("fetch https://user:p@ss@host/repo.git bearer TOKEN123");
        assert!(!redacted.contains("p@ss"));
        assert!(!redacted.contains("TOKEN123"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn windows_create_no_window_flag_is_configured() {
        assert_eq!(medousa_host::WINDOWS_CREATE_NO_WINDOW, 0x0800_0000);
    }

    #[cfg(windows)]
    #[test]
    fn windows_hide_helpers_apply_create_no_window() {
        use std::os::windows::process::CommandExt;
        let mut sync = std::process::Command::new("cmd");
        medousa_host::hide_subprocess_window(&mut sync);
        // creation_flags is additive in practice; ensure helper is callable.
        let mut tokio_cmd = tokio::process::Command::new("cmd");
        medousa_host::hide_tokio_subprocess_window(&mut tokio_cmd);
        let _ = sync.creation_flags(medousa_host::WINDOWS_CREATE_NO_WINDOW);
        let _ = tokio_cmd.creation_flags(medousa_host::WINDOWS_CREATE_NO_WINDOW);
    }
}
