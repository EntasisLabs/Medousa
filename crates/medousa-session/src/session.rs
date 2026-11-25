//! Session model + PTY lifecycle (portable-pty).

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, broadcast};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRootKind {
    Scripts,
    Forge,
    Workspace,
    Ssh,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMeta {
    pub session_id: SessionId,
    pub cwd: PathBuf,
    pub root_kind: SessionRootKind,
    #[serde(default)]
    pub work_id: Option<String>,
    /// Empty for an interactive login shell; otherwise the directly hosted command.
    pub argv: Vec<String>,
    #[serde(skip)]
    pub created_at: Instant,
    #[serde(skip)]
    pub last_activity: Arc<RwLock<Instant>>,
}

struct PtyHandles {
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    kill: Mutex<Box<dyn portable_pty::ChildKiller + Send>>,
    exited: Arc<AtomicBool>,
    exit_code: Arc<Mutex<Option<i32>>>,
    output_finished: Arc<AtomicBool>,
    process_id: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct OutputChunk {
    pub sequence: u64,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
struct OutputHistory {
    chunks: VecDeque<OutputChunk>,
    bytes: usize,
}

pub const MAX_SESSIONS: usize = 64;
const CONFIRMED_DESTROY_RECEIPTS: usize = 256;
const OUTPUT_HISTORY_CHUNKS: usize = 4096;
const OUTPUT_HISTORY_BYTES: usize = 2 * 1024 * 1024;

pub struct Session {
    _capacity: Option<Arc<OwnedSemaphorePermit>>,
    pub meta: SessionMeta,
    /// Fan-out of raw PTY output bytes to all attaches + agent readers.
    pub output: broadcast::Sender<OutputChunk>,
    output_history: Arc<Mutex<OutputHistory>>,
    pty: PtyHandles,
}

impl Session {
    pub fn spawn(
        session_id: SessionId,
        cwd: PathBuf,
        root_kind: SessionRootKind,
        work_id: Option<String>,
    ) -> anyhow::Result<Arc<Self>> {
        Self::spawn_with_size(session_id, cwd, root_kind, work_id, 80, 24)
    }

    pub fn spawn_with_size(
        session_id: SessionId,
        cwd: PathBuf,
        root_kind: SessionRootKind,
        work_id: Option<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Arc<Self>> {
        Self::spawn_command_with_size(session_id, cwd, root_kind, work_id, Vec::new(), cols, rows)
    }

    pub fn spawn_command_with_size(
        session_id: SessionId,
        cwd: PathBuf,
        root_kind: SessionRootKind,
        work_id: Option<String>,
        argv: Vec<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Arc<Self>> {
        Self::spawn_command_admitted(session_id, cwd, root_kind, work_id, argv, cols, rows, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_command_admitted(
        session_id: SessionId,
        cwd: PathBuf,
        root_kind: SessionRootKind,
        work_id: Option<String>,
        argv: Vec<String>,
        cols: u16,
        rows: u16,
        capacity: Option<OwnedSemaphorePermit>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut cmd = if let Some(program) = argv.first() {
            let mut command = CommandBuilder::new(program);
            for arg in &argv[1..] {
                command.arg(arg);
            }
            command
        } else {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| {
                if cfg!(windows) {
                    "cmd.exe".into()
                } else {
                    "/bin/sh".into()
                }
            });
            let mut command = CommandBuilder::new(shell.clone());
            if !cfg!(windows) && (shell.ends_with("zsh") || shell.ends_with("bash")) {
                command.arg("-l");
            }
            command
        };
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");

        Self::spawn_prepared_admitted(
            session_id, cwd, root_kind, work_id, argv, cmd, cols, rows, capacity,
        )
    }

    /// Host-only entry point for a daemon-prepared sandbox command. This is not
    /// an HTTP request field; the host owns argv, environment, and jail policy.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_prepared_command(
        session_id: SessionId,
        cwd: PathBuf,
        root_kind: SessionRootKind,
        work_id: Option<String>,
        argv: Vec<String>,
        cmd: CommandBuilder,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Arc<Self>> {
        Self::spawn_prepared_admitted(
            session_id, cwd, root_kind, work_id, argv, cmd, cols, rows, None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_prepared_admitted(
        session_id: SessionId,
        cwd: PathBuf,
        root_kind: SessionRootKind,
        work_id: Option<String>,
        argv: Vec<String>,
        mut cmd: CommandBuilder,
        cols: u16,
        rows: u16,
        capacity: Option<OwnedSemaphorePermit>,
    ) -> anyhow::Result<Arc<Self>> {
        let capacity = capacity.map(Arc::new);
        // The directory must already exist. Creating it here would let a
        // caller-supplied cwd materialize arbitrary filesystem locations.
        let cwd = cwd.canonicalize().map_err(|error| {
            anyhow::anyhow!(
                "session cwd must be an existing directory ({}): {error}",
                cwd.display()
            )
        })?;
        if !cwd.is_dir() {
            anyhow::bail!(
                "session cwd must be an existing directory ({})",
                cwd.display()
            );
        }
        let size = normalized_pty_size(cols, rows);

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(size)?;

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        cmd.cwd(&cwd);
        let mut child = pair.slave.spawn_command(cmd)?;
        let killer = child.clone_killer();
        let process_id = child.process_id();

        let exited = Arc::new(AtomicBool::new(false));
        let exited_reader = Arc::clone(&exited);
        let exit_code = Arc::new(Mutex::new(None));
        let exit_code_reader = Arc::clone(&exit_code);
        let waiter_capacity = capacity.clone();
        std::thread::spawn(move || {
            let _admission = waiter_capacity;
            match child.wait() {
                Ok(status) => {
                    if let Ok(mut code) = exit_code_reader.lock() {
                        *code = Some(status.exit_code() as i32);
                    }
                    exited_reader.store(true, Ordering::SeqCst);
                }
                Err(error) => {
                    tracing::warn!(%error, "PTY child wait failed; termination unconfirmed")
                }
            }
        });

        let master = pair.master;

        let (output, _) = broadcast::channel(256);
        let output_reader = output.clone();
        let output_history = Arc::new(Mutex::new(OutputHistory::default()));
        let history_reader = Arc::clone(&output_history);
        let next_sequence = Arc::new(AtomicU64::new(1));
        let sequence_reader = Arc::clone(&next_sequence);
        let output_finished = Arc::new(AtomicBool::new(false));
        let reader_finished = output_finished.clone();
        let reader_capacity = capacity.clone();
        std::thread::spawn(move || {
            let _admission = reader_capacity;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = OutputChunk {
                            sequence: sequence_reader.fetch_add(1, Ordering::Relaxed),
                            bytes: buf[..n].to_vec(),
                        };
                        if let Ok(mut history) = history_reader.lock() {
                            history.bytes += chunk.bytes.len();
                            history.chunks.push_back(chunk.clone());
                            while history.bytes > OUTPUT_HISTORY_BYTES
                                || history.chunks.len() > OUTPUT_HISTORY_CHUNKS
                            {
                                let Some(removed) = history.chunks.pop_front() else {
                                    break;
                                };
                                history.bytes = history.bytes.saturating_sub(removed.bytes.len());
                            }
                        }
                        let _ = output_reader.send(chunk);
                    }
                    Err(_) => break,
                }
            }

            reader_finished.store(true, Ordering::SeqCst);
        });

        Ok(Arc::new(Self {
            _capacity: capacity,
            meta: SessionMeta {
                session_id,
                cwd,
                root_kind,
                work_id,
                argv,
                created_at: Instant::now(),
                last_activity: Arc::new(RwLock::new(Instant::now())),
            },
            output,
            output_history,
            pty: PtyHandles {
                master: Mutex::new(Some(master)),
                writer: Mutex::new(Some(writer)),
                kill: Mutex::new(killer),
                exited,
                exit_code,
                output_finished,
                process_id,
            },
        }))
    }

    pub fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let mut writer = self
            .pty
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("pty writer poisoned"))?;
        let writer = writer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("session destroyed"))?;
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<PtySize> {
        let size = normalized_pty_size(cols, rows);
        let master = self
            .pty
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("pty master poisoned"))?;
        let master = master
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session destroyed"))?;
        master.resize(size)?;
        master.get_size()
    }

    pub fn size(&self) -> anyhow::Result<PtySize> {
        self.pty
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("pty master poisoned"))?
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("session destroyed"))?
            .get_size()
    }

    pub fn output_snapshot(&self) -> Vec<OutputChunk> {
        self.output_history
            .lock()
            .map(|history| history.chunks.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn signal_interrupt(&self) {
        let _ = self.write(b"\x03");
    }

    pub fn exited(&self) -> bool {
        self.pty.exited.load(Ordering::SeqCst)
    }

    pub fn process_id(&self) -> Option<u32> {
        self.pty.process_id
    }

    pub fn output_finished(&self) -> bool {
        self.pty.output_finished.load(Ordering::SeqCst)
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.pty.exit_code.lock().ok().and_then(|code| *code)
    }

    fn request_hangup(&self) {
        #[cfg(unix)]
        if !self.exited()
            && let Some(pid) = self.process_id().map(|pid| pid as libc::pid_t)
        {
            // Let interactive shells forward HUP to their job-control jobs
            // before escalation. No process-table enumeration is involved.
            unsafe {
                if pid > 1 && libc::getsid(pid) == pid && libc::getpgid(pid) == pid {
                    libc::kill(-pid, libc::SIGHUP);
                }
            }
        }
    }

    pub fn kill(&self) {
        if self.exited() {
            return;
        }
        // Only signal groups proven to belong to this still-live PTY session.
        // Never revisit a numeric PID once our child waiter confirmed exit.
        #[cfg(unix)]
        if !self.exited()
            && let Some(pid) = self.process_id().map(|pid| pid as libc::pid_t)
            && let Ok(master) = self.pty.master.lock()
            && let Some(fd) = master.as_ref().and_then(|master| master.as_raw_fd())
        {
            unsafe {
                let foreground = libc::tcgetpgrp(fd);
                if foreground > 1 && libc::getsid(foreground) == pid {
                    libc::kill(-foreground, libc::SIGKILL);
                }
                if pid > 1 && libc::getsid(pid) == pid && libc::getpgid(pid) == pid {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
        }
        if !self.exited()
            && let Ok(mut k) = self.pty.kill.lock()
        {
            let _ = k.kill();
        }
    }

    pub async fn touch(&self) {
        *self.meta.last_activity.write().await = Instant::now();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.exited() {
            self.kill();
        }
    }
}

fn normalized_pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols: cols.clamp(2, 1_000),
        rows: rows.clamp(1, 500),
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SessionResources {
    pub registered_sessions: usize,
    pub reserved_sessions: usize,
    pub waiter_threads: usize,
    pub reader_threads: usize,
    pub history_bytes: usize,
}

pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<Session>>>,
    destroyed: RwLock<VecDeque<SessionId>>,
    capacity: Arc<Semaphore>,
    #[cfg(test)]
    failed_destroys: std::sync::atomic::AtomicUsize,
    default_workspace: PathBuf,
    execution: medousa_forge::execution::ForgeExecutionService,
}

impl SessionManager {
    pub fn new(default_workspace: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            destroyed: RwLock::new(VecDeque::new()),
            capacity: Arc::new(Semaphore::new(MAX_SESSIONS)),
            #[cfg(test)]
            failed_destroys: std::sync::atomic::AtomicUsize::new(0),
            default_workspace,
            execution: medousa_forge::execution::ForgeExecutionService::new(),
        })
    }

    pub async fn create(
        &self,
        root_kind: SessionRootKind,
        cwd: Option<PathBuf>,
        work_id: Option<String>,
    ) -> anyhow::Result<Arc<Session>> {
        self.create_with_size(root_kind, cwd, work_id, 80, 24).await
    }

    pub async fn create_with_size(
        &self,
        root_kind: SessionRootKind,
        cwd: Option<PathBuf>,
        work_id: Option<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Arc<Session>> {
        self.create_command_with_size(root_kind, cwd, work_id, Vec::new(), cols, rows)
            .await
    }

    pub async fn create_command_with_size(
        &self,
        root_kind: SessionRootKind,
        cwd: Option<PathBuf>,
        work_id: Option<String>,
        argv: Vec<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Arc<Session>> {
        let capacity = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| anyhow::anyhow!("session host capacity reached ({MAX_SESSIONS})"))?;
        let cwd = self.session_cwd(cwd)?;
        let id = SessionId::new();
        let spawn_id = id.clone();
        let session = self
            .execution
            .run(
                medousa_forge::execution::ExecutionClass::WorkEnvironment,
                256 * 1024,
                move || {
                    Ok(Session::spawn_command_admitted(
                        spawn_id,
                        cwd,
                        root_kind,
                        work_id,
                        argv,
                        cols,
                        rows,
                        Some(capacity),
                    ))
                },
            )
            .await??;
        self.sessions.write().await.insert(id, Arc::clone(&session));
        Ok(session)
    }

    fn session_cwd(&self, cwd: Option<PathBuf>) -> anyhow::Result<PathBuf> {
        match cwd {
            Some(cwd) => Ok(cwd),
            None => confined_workspace(&self.default_workspace),
        }
    }

    pub async fn get(&self, id: &SessionId) -> Option<Arc<Session>> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<SessionMeta> {
        self.sessions
            .read()
            .await
            .values()
            .map(|s| s.meta.clone())
            .collect()
    }

    /// Only retained confirmation receipts make an absent ID idempotent.
    /// Unknown IDs (including after host replacement) are not proof of exit.
    /// Failed confirmation leaves the record and capacity reserved for retry.
    pub async fn resources(&self) -> SessionResources {
        let sessions = self.sessions.read().await;
        SessionResources {
            registered_sessions: sessions.len(),
            reserved_sessions: MAX_SESSIONS - self.capacity.available_permits(),
            waiter_threads: sessions.values().filter(|s| !s.exited()).count(),
            reader_threads: sessions.values().filter(|s| !s.output_finished()).count(),
            history_bytes: sessions
                .values()
                .map(|s| s.output_history.lock().unwrap().bytes)
                .sum(),
        }
    }

    pub async fn destroy(&self, id: &SessionId) -> bool {
        #[cfg(test)]
        if self
            .failed_destroys
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return false;
        }
        let Some(session) = self.get(id).await else {
            return self.destroyed.read().await.contains(id);
        };
        session.request_hangup();
        let grace = tokio::time::Instant::now() + Duration::from_millis(100);
        while !session.exited() && tokio::time::Instant::now() < grace {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        session.kill();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !session.exited() || !session.output_finished() {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // Release descriptors and replay storage even if an attachment still
        // holds an Arc. Admission capacity remains reserved until that Arc drops.
        session.pty.writer.lock().unwrap().take();
        session.pty.master.lock().unwrap().take();
        *session.output_history.lock().unwrap() = OutputHistory::default();
        {
            let mut destroyed = self.destroyed.write().await;
            if !destroyed.contains(id) {
                destroyed.push_back(id.clone());
            }
            while destroyed.len() > CONFIRMED_DESTROY_RECEIPTS {
                destroyed.pop_front();
            }
        }
        self.sessions.write().await.remove(id);
        true
    }

    /// Creation is supervised independently of the request future. Cancellation
    /// cannot lose a spawned PTY between host creation and returning its ID.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_turn_command(
        self: &Arc<Self>,
        owner_path: PathBuf,
        root_kind: SessionRootKind,
        cwd: PathBuf,
        work_id: Option<String>,
        argv: Vec<String>,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<Arc<Session>> {
        let owner = crate::owner::OwnerWatch::open(owner_path)?;
        let manager = self.clone();
        tokio::spawn(async move {
            let session = manager
                .create_command_with_size(root_kind, Some(cwd), work_id, argv, cols, rows)
                .await?;
            let id = session.meta.session_id.clone();
            let supervisor = manager.clone();
            let supervisor_capacity = session._capacity.clone();
            tokio::spawn(async move {
                let _admission = supervisor_capacity;
                loop {
                    if supervisor.get(&id).await.is_none() {
                        // Only confirmed destroy removes a record in this manager.
                        // Don't wait forever if its bounded receipt was evicted.
                        if matches!(owner.gone(), Ok(true)) {
                            owner.remove();
                        }
                        return;
                    }
                    match owner.gone() {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(error) => tracing::warn!(%error, %id, "cannot confirm PTY owner loss"),
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                while supervisor.get(&id).await.is_some() && !supervisor.destroy(&id).await {
                    tracing::warn!(%id, "PTY termination unconfirmed; retrying");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                // Only remove the lease inode after actual owner loss, not an
                // explicit destroy of one shell during an otherwise live turn.
                if matches!(owner.gone(), Ok(true)) {
                    owner.remove();
                }
            });
            Ok::<_, anyhow::Error>(session)
        })
        .await?
    }

    /// Reap idle sessions (no attach / activity) older than `ttl`. Caller decides
    /// whether to skip sessions with active agent leases.
    pub async fn reap_idle(&self, ttl: Duration) -> usize {
        let now = Instant::now();
        let mut removed = Vec::new();
        {
            let guard = self.sessions.read().await;
            for (id, session) in guard.iter() {
                let last = *session.meta.last_activity.read().await;
                if now.duration_since(last) > ttl && session.exited() {
                    removed.push(id.clone());
                }
            }
        }
        let mut count = 0;
        for id in removed {
            if self.destroy(&id).await {
                count += 1;
            }
        }
        count
    }
}

fn confined_workspace(requested: &std::path::Path) -> anyhow::Result<PathBuf> {
    if !requested.is_absolute() {
        anyhow::bail!(
            "session workspace must be an absolute directory ({})",
            requested.display()
        );
    }
    let anchor = match requested.components().next() {
        Some(component) => PathBuf::from(component.as_os_str()),
        None => PathBuf::from("/"),
    };
    if requested.starts_with(&anchor) {
        let existing = requested.canonicalize().map_err(|error| {
            anyhow::anyhow!(
                "session workspace must be an existing directory ({}): {error}",
                requested.display()
            )
        })?;
        if existing.starts_with(&anchor) {
            if existing.is_dir() {
                return Ok(existing);
            }
            anyhow::bail!(
                "session workspace must be an existing directory ({})",
                existing.display()
            );
        }
        anyhow::bail!(
            "session workspace escapes its root ({})",
            existing.display()
        );
    }
    anyhow::bail!(
        "session workspace escapes its root ({})",
        requested.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    async fn wait_for(mut predicate: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(8), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("OS process condition timed out");
    }

    #[cfg(unix)]
    async fn assert_reclaimed(manager: &SessionManager, session: Arc<Session>) {
        let pid = session.process_id().unwrap() as i32;
        wait_for(|| session.exited() && session.output_finished()).await;
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "owned shell still exists"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
        tokio::time::timeout(Duration::from_secs(8), async {
            while manager.get(&session.meta.session_id).await.is_some() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(session.pty.master.lock().unwrap().is_none());
        assert!(session.pty.writer.lock().unwrap().is_none());
        assert!(session.output_snapshot().is_empty());
        drop(session);
        tokio::time::timeout(Duration::from_secs(3), async {
            while manager.resources().await.reserved_sessions != 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            manager.resources().await,
            SessionResources {
                registered_sessions: 0,
                reserved_sessions: 0,
                waiter_threads: 0,
                reader_threads: 0,
                history_bytes: 0,
            }
        );
    }

    #[cfg(unix)]
    async fn turn_shell(
        manager: &Arc<SessionManager>,
        owner: &crate::owner::TurnOwner,
    ) -> Arc<Session> {
        manager
            .create_turn_command(
                owner.path().to_owned(),
                SessionRootKind::Forge,
                manager.default_workspace.clone(),
                Some("test-turn".into()),
                vec!["/bin/sh".into(), "-i".into()],
                80,
                24,
            )
            .await
            .unwrap()
    }

    #[cfg(unix)]
    async fn shell_marker(session: &Session, marker: &str) {
        // Split the marker in the echoed input so only executed output matches.
        let (a, b) = marker.split_at(marker.len() / 2);
        session
            .write(format!("printf '%s%s\n' '{a}' '{b}'\n").as_bytes())
            .unwrap();
        wait_for(|| {
            session
                .output_snapshot()
                .iter()
                .any(|c| String::from_utf8_lossy(&c.bytes).contains(marker))
        })
        .await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn turn_finish_reuses_shell_then_reclaims_all_resources() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        for _ in 0..4 {
            let owner = crate::owner::TurnOwner::new().unwrap();
            let shell = turn_shell(&manager, &owner).await;
            let pid = shell.process_id();
            shell_marker(&shell, "FIRST_COMMAND").await;
            shell_marker(&shell, "SECOND_COMMAND").await;
            assert_eq!(shell.process_id(), pid);
            assert!(!shell.exited());
            owner.release();
            assert!(manager.destroy(&shell.meta.session_id).await);
            assert!(
                manager.destroy(&shell.meta.session_id).await,
                "idempotent destroy"
            );
            assert_reclaimed(&manager, shell).await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn interrupt_keeps_shell_cancel_kills_foreground_child_and_preserves_human() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        let human = manager
            .create_command_with_size(
                SessionRootKind::Scripts,
                None,
                None,
                vec!["/bin/sh".into(), "-i".into()],
                80,
                24,
            )
            .await
            .unwrap();
        let other_owner = crate::owner::TurnOwner::new().unwrap();
        let other = turn_shell(&manager, &other_owner).await;
        let job = manager
            .create_command_with_size(
                SessionRootKind::Forge,
                None,
                Some("adopted-job".into()),
                vec!["/bin/sh".into(), "-c".into(), "sleep 60".into()],
                80,
                24,
            )
            .await
            .unwrap();
        shell_marker(&human, "HUMAN_BEFORE").await;
        let owner = crate::owner::TurnOwner::new().unwrap();
        let shell = turn_shell(&manager, &owner).await;
        shell.write(b"sleep 60\n").unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        shell.signal_interrupt();
        shell_marker(&shell, "AFTER_CTRL_C").await;
        assert!(!shell.exited());
        let child_file = dir.path().join("child.pid");
        shell
            .write(
                format!(
                    "/bin/sh -c 'sleep 60 & echo $! > {}; wait'\n",
                    child_file.display()
                )
                .as_bytes(),
            )
            .unwrap();
        wait_for(|| child_file.exists()).await;
        let child: i32 = std::fs::read_to_string(&child_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(unsafe { libc::kill(child, 0) }, 0);
        // Dropping a cancelled turn's owner must work without a final HTTP call.
        drop(owner);
        wait_for(|| shell.exited() && shell.output_finished()).await;
        wait_for(|| unsafe { libc::kill(child, 0) } == -1).await;
        assert!(!human.exited());
        assert!(!other.exited());
        assert!(!job.exited());
        shell_marker(&human, "HUMAN_STILL_USABLE").await;
        shell_marker(&other, "OTHER_TURN_USABLE").await;
        assert_eq!(manager.list().await.len(), 3);
        assert!(manager.destroy(&human.meta.session_id).await);
        assert!(manager.destroy(&other.meta.session_id).await);
        assert!(manager.destroy(&job.meta.session_id).await);
        drop((human, other, job));
        assert_reclaimed(&manager, shell).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn normal_finish_destroys_shell_and_its_background_child() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        let owner = crate::owner::TurnOwner::new().unwrap();
        let shell = turn_shell(&manager, &owner).await;
        let child_file = dir.path().join("background.pid");
        shell
            .write(format!("sleep 60 & echo $! > {}\n", child_file.display()).as_bytes())
            .unwrap();
        wait_for(|| child_file.exists()).await;
        let child: i32 = std::fs::read_to_string(child_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        owner.release();
        assert!(manager.destroy(&shell.meta.session_id).await);
        wait_for(|| unsafe { libc::kill(child, 0) } == -1).await;
        assert_reclaimed(&manager, shell).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_cleanup_is_retained_and_retried_after_owner_error() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        let owner = crate::owner::TurnOwner::new().unwrap();
        let shell = turn_shell(&manager, &owner).await;
        manager.failed_destroys.store(2, Ordering::SeqCst);
        let result: anyhow::Result<()> = async move {
            let _owner = owner;
            anyhow::bail!("test turn error")
        }
        .await;
        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert!(manager.get(&shell.meta.session_id).await.is_some());
        assert_reclaimed(&manager, shell).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn aborting_turn_task_drops_owner_and_reaps_all_created_shells() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        let owner = crate::owner::TurnOwner::new().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let turn_manager = manager.clone();
        let task = tokio::spawn(async move {
            let first = turn_shell(&turn_manager, &owner).await;
            let second = turn_shell(&turn_manager, &owner).await;
            tx.send((first, second)).ok().unwrap();
            std::future::pending::<()>().await;
            drop(owner);
        });
        let (first, second) = rx.await.unwrap();
        // A quiet owner is still live, even after several supervisor intervals.
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(!first.exited() && !second.exited());
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        wait_for(|| second.exited() && second.output_finished()).await;
        drop(second);
        assert_reclaimed(&manager, first).await;
    }

    #[tokio::test]
    async fn stale_or_unowned_owner_file_cannot_spawn_a_pty() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        let owner = crate::owner::TurnOwner::new().unwrap();
        owner.release();
        assert!(
            manager
                .create_turn_command(
                    owner.path().to_owned(),
                    SessionRootKind::Forge,
                    dir.path().to_owned(),
                    None,
                    vec![],
                    80,
                    24
                )
                .await
                .is_err()
        );
        assert!(
            manager
                .create_turn_command(
                    dir.path().join("unrelated"),
                    SessionRootKind::Forge,
                    dir.path().to_owned(),
                    None,
                    vec![],
                    80,
                    24
                )
                .await
                .is_err()
        );
        assert_eq!(manager.resources().await.reserved_sessions, 0);
    }

    // A separate OS process owns the lock; killing it bypasses Rust Drop.
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn turn_owner_crash_helper() {
        let Some(path) = std::env::var_os("MEDOUSA_TEST_OWNER_PATH") else {
            return;
        };
        let owner = crate::owner::TurnOwner::new().unwrap();
        std::fs::write(path, owner.path().to_str().unwrap()).unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn confirmed_owner_process_crash_reclaims_shell_without_pid_sweep() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner-path");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "session::tests::turn_owner_crash_helper",
                "--ignored",
            ])
            .env("MEDOUSA_TEST_OWNER_PATH", &path)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        wait_for(|| path.exists()).await;
        let owner_path = std::path::PathBuf::from(std::fs::read_to_string(path).unwrap());
        let manager = SessionManager::new(dir.path().to_owned());
        let shell = manager
            .create_turn_command(
                owner_path.clone(),
                SessionRootKind::Forge,
                dir.path().to_owned(),
                None,
                vec!["/bin/sh".into(), "-i".into()],
                80,
                24,
            )
            .await
            .unwrap();
        shell_marker(&shell, "CRASH_TEST_READY").await;
        child.kill().unwrap();
        child.wait().unwrap();
        assert_reclaimed(&manager, shell).await;
        assert!(!owner_path.exists(), "crashed owner's lease file reclaimed");
    }

    #[tokio::test]
    async fn unknown_or_evicted_destroy_receipt_is_not_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        assert!(!manager.destroy(&SessionId::new()).await);
        let old = SessionId::new();
        manager.destroyed.write().await.push_back(old.clone());
        assert!(manager.destroy(&old).await);
        manager.destroyed.write().await.clear();
        assert!(!manager.destroy(&old).await);
    }

    #[tokio::test]
    async fn host_capacity_rejects_before_spawning_without_evicting_any_session() {
        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(dir.path().to_owned());
        let permits = manager
            .capacity
            .clone()
            .acquire_many_owned(MAX_SESSIONS as u32)
            .await
            .unwrap();
        assert!(
            manager
                .create(SessionRootKind::Scripts, None, None)
                .await
                .is_err()
        );
        assert!(manager.list().await.is_empty());
        drop(permits);
        assert_eq!(manager.resources().await.reserved_sessions, 0);
    }

    #[tokio::test]
    async fn spawn_echo_session() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let session = mgr
            .create(SessionRootKind::Scripts, None, None)
            .await
            .unwrap();
        session.write(b"echo hello\n").unwrap();
        let mut rx = session.output.subscribe();
        let mut collected = String::new();
        for _ in 0..20 {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(chunk)) => {
                    collected.push_str(&String::from_utf8_lossy(&chunk.bytes));
                    if collected.contains("hello") {
                        break;
                    }
                }
                _ => break,
            }
        }
        session.kill();
        assert!(collected.contains("hello"), "got: {collected:?}");
        assert!(
            session
                .output_snapshot()
                .iter()
                .any(|chunk| String::from_utf8_lossy(&chunk.bytes).contains("hello")),
            "output history did not retain the PTY transcript"
        );
    }

    #[tokio::test]
    async fn resizes_the_real_pty_master() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let session = mgr
            .create(SessionRootKind::Scripts, None, None)
            .await
            .unwrap();

        let acknowledged = session.resize(132, 43).unwrap();
        let size = session
            .pty
            .master
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get_size()
            .unwrap();

        session.kill();
        assert_eq!(acknowledged.cols, 132);
        assert_eq!(acknowledged.rows, 43);
        assert_eq!(size.cols, 132);
        assert_eq!(size.rows, 43);
    }

    #[tokio::test]
    async fn starts_at_the_requested_geometry() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let session = mgr
            .create_with_size(SessionRootKind::Scripts, None, None, 156, 61)
            .await
            .unwrap();

        let size = session.size().unwrap();

        session.kill();
        assert_eq!(size.cols, 156);
        assert_eq!(size.rows, 61);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn hosts_a_command_directly_in_one_pty() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let argv = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "printf task-ready".to_string(),
        ];
        let session = mgr
            .create_command_with_size(
                SessionRootKind::Forge,
                None,
                Some("work-1".into()),
                argv.clone(),
                100,
                30,
            )
            .await
            .unwrap();

        let mut output = String::new();
        for _ in 0..100 {
            let bytes = session
                .output_snapshot()
                .into_iter()
                .flat_map(|chunk| chunk.bytes)
                .collect::<Vec<_>>();
            output = String::from_utf8_lossy(&bytes).into_owned();
            if output.contains("task-ready") && session.exited() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        assert_eq!(session.meta.argv, argv);
        assert_eq!(session.exit_code(), Some(0));
        assert!(
            output.contains("task-ready"),
            "direct PTY host lost command output: {output:?}"
        );
    }
}
