//! Session model + PTY lifecycle (portable-pty).

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};
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
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    kill: Mutex<Box<dyn portable_pty::ChildKiller + Send>>,
    exited: Arc<AtomicBool>,
    exit_code: Arc<Mutex<Option<i32>>>,
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

const OUTPUT_HISTORY_BYTES: usize = 2 * 1024 * 1024;

pub struct Session {
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
        std::fs::create_dir_all(&cwd)?;
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        let size = normalized_pty_size(cols, rows);

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(size)?;

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

        let mut child = pair.slave.spawn_command(cmd)?;
        let killer = child.clone_killer();

        let exited = Arc::new(AtomicBool::new(false));
        let exited_reader = Arc::clone(&exited);
        let exit_code = Arc::new(Mutex::new(None));
        let exit_code_reader = Arc::clone(&exit_code);
        std::thread::spawn(move || {
            if let Ok(status) = child.wait()
                && let Ok(mut code) = exit_code_reader.lock()
            {
                *code = Some(status.exit_code() as i32);
            }
            exited_reader.store(true, Ordering::SeqCst);
        });

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let master = pair.master;

        let (output, _) = broadcast::channel(4096);
        let output_reader = output.clone();
        let output_history = Arc::new(Mutex::new(OutputHistory::default()));
        let history_reader = Arc::clone(&output_history);
        let next_sequence = Arc::new(AtomicU64::new(1));
        let sequence_reader = Arc::clone(&next_sequence);
        std::thread::spawn(move || {
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
                            while history.bytes > OUTPUT_HISTORY_BYTES {
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
        });

        Ok(Arc::new(Self {
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
                master: Mutex::new(master),
                writer: Mutex::new(writer),
                kill: Mutex::new(killer),
                exited,
                exit_code,
            },
        }))
    }

    pub fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let mut writer = self
            .pty
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("pty writer poisoned"))?;
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
        master.resize(size)?;
        master.get_size()
    }

    pub fn size(&self) -> anyhow::Result<PtySize> {
        self.pty
            .master
            .lock()
            .map_err(|_| anyhow::anyhow!("pty master poisoned"))?
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

    pub fn exit_code(&self) -> Option<i32> {
        self.pty.exit_code.lock().ok().and_then(|code| *code)
    }

    pub fn kill(&self) {
        if let Ok(mut k) = self.pty.kill.lock() {
            let _ = k.kill();
        }
    }

    pub async fn touch(&self) {
        *self.meta.last_activity.write().await = Instant::now();
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

pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<Session>>>,
    default_workspace: PathBuf,
}

impl SessionManager {
    pub fn new(default_workspace: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            default_workspace,
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
        let cwd = cwd.unwrap_or_else(|| self.default_workspace.clone());
        let id = SessionId::new();
        let session = Session::spawn_with_size(id.clone(), cwd, root_kind, work_id, cols, rows)?;
        self.sessions.write().await.insert(id, Arc::clone(&session));
        Ok(session)
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
        let cwd = cwd.unwrap_or_else(|| self.default_workspace.clone());
        let id = SessionId::new();
        let session = Session::spawn_command_with_size(
            id.clone(),
            cwd,
            root_kind,
            work_id,
            argv,
            cols,
            rows,
        )?;
        self.sessions.write().await.insert(id, Arc::clone(&session));
        Ok(session)
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

    pub async fn destroy(&self, id: &SessionId) -> bool {
        let Some(session) = self.sessions.write().await.remove(id) else {
            return false;
        };
        session.kill();
        true
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
        let count = removed.len();
        for id in removed {
            self.destroy(&id).await;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let size = session.pty.master.lock().unwrap().get_size().unwrap();

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

        for _ in 0..50 {
            if session.exited() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let output = session
            .output_snapshot()
            .into_iter()
            .flat_map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();
        assert_eq!(session.meta.argv, argv);
        assert_eq!(session.exit_code(), Some(0));
        assert!(String::from_utf8_lossy(&output).contains("task-ready"));
    }
}
