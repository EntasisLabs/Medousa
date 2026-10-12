//! Detached shell supervision.
//!
//! The turn returns a handle immediately. This module owns the OS process and
//! a Stasis durable wait (`medousa.process`) stored beside it, because Stasis
//! does not own the pid. The wait survives a daemon restart: reconcile completes
//! it when the pid is gone and wakes the same session. Exit or an optional
//! deadline produces `process_completed` or `timed_out` with report authority.
//! Repeated status polls of a handle that already has a wake are refused.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use medousa_types::wake::{
    OpenGap, OpenGapKind, WakeAuthority, WakeGrant, WakeSource, process_event_prompt,
};
use serde_json::{Value, json};
use stasis::domain::runtime::durable_wait::{DurableWaitRecord, DurableWaitStatus};

const SIGNAL_TYPE: &str = "medousa.process";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Running,
    Completed,
    TimedOut,
    Failed,
}

impl ProcessStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::TimedOut => "timed_out",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessRecord {
    pub handle: String,
    pub session_id: String,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_session: Option<String>,
    pub argv: Vec<String>,
    pub pid: u32,
    pub log_path: String,
    pub status: ProcessStatus,
    pub exit_code: Option<i32>,
    pub tail: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub deadline_at: Option<DateTime<Utc>>,
    pub wake_registered: bool,
    pub polls: u32,
    pub wait: DurableWaitRecord,
    /// In-process supervisor task. Not persisted: after a restart every loaded
    /// record is unsupervised until reconcile attaches a watch.
    #[serde(skip)]
    supervised: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessWake {
    pub session_id: String,
    pub profile_id: String,
    pub home_session: Option<String>,
    pub kind: WakeSource,
    pub handle: String,
    pub detail: String,
    pub prompt: String,
    pub grant: WakeGrant,
}

struct Store {
    records: Vec<ProcessRecord>,
    wakes: Vec<ProcessWake>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        Mutex::new(Store {
            records: load_records(),
            wakes: Vec::new(),
        })
    })
}

fn ledger_path() -> PathBuf {
    if let Ok(path) = std::env::var("MEDOUSA_PROCESS_LEDGER_PATH")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    crate::paths::medousa_data_dir().join("process_supervisor.json")
}

fn load_records() -> Vec<ProcessRecord> {
    fs::read_to_string(ledger_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn persist(records: &[ProcessRecord]) {
    let path = ledger_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(records) {
        let _ = fs::write(path, raw);
    }
}

pub struct LaunchRequest {
    pub session_id: String,
    pub profile_id: String,
    pub home_session: Option<String>,
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub deadline: Option<Duration>,
    /// When set, a command that exits inside this window returns inline and
    /// does not register a wake. A command still running after the window is
    /// detached. `None` detaches immediately.
    pub background_after: Option<Duration>,
}

pub async fn launch_detached(request: LaunchRequest) -> Result<Value, String> {
    if request
        .argv
        .first()
        .is_none_or(|program| program.trim().is_empty())
    {
        return Err("background process requires a command".into());
    }
    let log_path = log_path_for_new();
    let mut child = spawn_child(&request, &log_path)?;
    if let Some(after) = request.background_after {
        let deadline = tokio::time::Instant::now() + after;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(inline_result(&request.argv, status)),
                Ok(None) if tokio::time::Instant::now() >= deadline => break,
                Ok(None) => tokio::time::sleep(Duration::from_millis(20)).await,
                Err(err) => return Err(format!("background process wait failed: {err}")),
            }
        }
    }
    supervise(request, child, log_path)
}

fn log_path_for_new() -> PathBuf {
    let dir = ledger_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("process-logs");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("proc_{}.log", uuid::Uuid::new_v4().simple()))
}

fn spawn_child(request: &LaunchRequest, log_path: &Path) -> Result<Child, String> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| err.to_string())?;
    let err_log = log.try_clone().map_err(|err| err.to_string())?;
    let mut command = Command::new(&request.argv[0]);
    if request.argv.len() > 1 {
        command.args(&request.argv[1..]);
    }
    if let Some(cwd) = &request.cwd {
        command.current_dir(cwd);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err_log))
        .spawn()
        .map_err(|err| format!("failed to start background process: {err}"))
}

fn supervise(request: LaunchRequest, mut child: Child, log_path: PathBuf) -> Result<Value, String> {
    let handle = format!("proc_{}", uuid::Uuid::new_v4().simple());
    let pid = child.id();
    let now = Utc::now();
    let deadline_at = request.deadline.and_then(|deadline| {
        chrono::Duration::from_std(deadline)
            .ok()
            .map(|deadline| now + deadline)
    });
    let record = ProcessRecord {
        handle: handle.clone(),
        session_id: request.session_id.clone(),
        profile_id: request.profile_id.clone(),
        home_session: request.home_session.clone(),
        argv: request.argv.clone(),
        pid,
        log_path: log_path.display().to_string(),
        status: ProcessStatus::Running,
        exit_code: None,
        tail: String::new(),
        started_at: now,
        finished_at: None,
        deadline_at,
        wake_registered: true,
        polls: 0,
        supervised: true,
        wait: durable_wait(
            &handle,
            &request.session_id,
            DurableWaitStatus::Pending,
            now,
            deadline_at,
            None,
        ),
    };
    {
        let mut guard = store().lock().unwrap_or_else(|err| err.into_inner());
        guard.records.push(record.clone());
        persist(&guard.records);
    }
    crate::wake_runtime::register_gap(
        &request.session_id,
        OpenGap {
            id: handle.clone(),
            kind: OpenGapKind::RunningProcess,
            summary: format!("running {}", request.argv.join(" ")),
        },
    );
    let handle_for_task = handle.clone();
    let deadline = request.deadline;
    tokio::spawn(async move {
        let wait = tokio::task::spawn_blocking(move || child.wait());
        let end = if let Some(deadline) = deadline {
            match tokio::time::timeout(deadline, wait).await {
                Ok(joined) => match joined {
                    Ok(Ok(status)) => WaitEnd::Exited(status),
                    Ok(Err(err)) => WaitEnd::Failed(err.to_string()),
                    Err(err) => WaitEnd::Failed(err.to_string()),
                },
                Err(_) => WaitEnd::TimedOut,
            }
        } else {
            match wait.await {
                Ok(Ok(status)) => WaitEnd::Exited(status),
                Ok(Err(err)) => WaitEnd::Failed(err.to_string()),
                Err(err) => WaitEnd::Failed(err.to_string()),
            }
        };
        finish_wait(&handle_for_task, &log_path, end);
    });
    Ok(handle_payload(&record))
}

enum WaitEnd {
    Exited(std::process::ExitStatus),
    ExitedAfterRestart,
    TimedOut,
    Failed(String),
}

fn finish_wait(handle: &str, log_path: &Path, end: WaitEnd) {
    if matches!(end, WaitEnd::TimedOut)
        && let Some(pid) = record_pid(handle)
    {
        kill_pid(pid);
    }
    let tail = read_tail(log_path);
    let (status, exit_code, kind, detail) = match end {
        WaitEnd::Exited(exit) => {
            let code = exit.code();
            (
                ProcessStatus::Completed,
                code,
                WakeSource::ProcessCompleted,
                format!(
                    "exit={}",
                    code.map(|code| code.to_string())
                        .unwrap_or_else(|| "signal".into())
                ),
            )
        }
        WaitEnd::ExitedAfterRestart => (
            ProcessStatus::Completed,
            None,
            WakeSource::ProcessCompleted,
            "pid exited after a daemon restart; the exit code was not retained".into(),
        ),
        WaitEnd::TimedOut => (
            ProcessStatus::TimedOut,
            None,
            WakeSource::ProcessTimedOut,
            "deadline elapsed while the process was still running".into(),
        ),
        WaitEnd::Failed(error) => (
            ProcessStatus::Failed,
            None,
            WakeSource::ProcessCompleted,
            error,
        ),
    };
    let durable_status = match kind {
        WakeSource::ProcessTimedOut => DurableWaitStatus::TimedOut,
        _ => DurableWaitStatus::Signaled,
    };
    let wake = {
        let mut guard = store().lock().unwrap_or_else(|err| err.into_inner());
        let Some(record) = guard
            .records
            .iter_mut()
            .find(|record| record.handle == handle)
        else {
            return;
        };
        record.status = status;
        record.exit_code = exit_code;
        record.tail = tail.clone();
        record.finished_at = Some(Utc::now());
        record.wait.status = durable_status;
        record.wait.updated_at = Utc::now();
        record.wait.signal_payload = Some(detail.clone());
        let wake = ProcessWake {
            session_id: record.session_id.clone(),
            profile_id: record.profile_id.clone(),
            home_session: record.home_session.clone(),
            kind,
            handle: handle.to_string(),
            detail: detail.clone(),
            prompt: process_event_prompt(
                kind.as_str(),
                handle,
                &format!("{detail}; tail={tail}"),
                grant_authority(kind),
            ),
            grant: WakeGrant::for_source(kind),
        };
        persist(&guard.records);
        wake
    };
    crate::wake_runtime::close_gap(&wake.session_id, handle);
    emit(wake);
}

fn grant_authority(kind: WakeSource) -> WakeAuthority {
    WakeGrant::for_source(kind).authority
}

fn record_pid(handle: &str) -> Option<u32> {
    store()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .records
        .iter()
        .find(|record| record.handle == handle)
        .map(|record| record.pid)
}

fn emit(wake: ProcessWake) {
    {
        let mut guard = store().lock().unwrap_or_else(|err| err.into_inner());
        guard.wakes.push(wake.clone());
    }
    let Some(state) = DAEMON.get().cloned() else {
        return;
    };
    tokio::spawn(async move {
        dispatch_daemon_wake(state, wake).await;
    });
}

static DAEMON: OnceLock<crate::daemon::state::AppState> = OnceLock::new();

pub fn bind_daemon(state: crate::daemon::state::AppState) {
    let _ = DAEMON.set(state);
    reconcile();
}

pub fn reconcile() {
    let mut completed = Vec::new();
    let mut resume = Vec::new();
    {
        let mut guard = store().lock().unwrap_or_else(|err| err.into_inner());
        for record in &mut guard.records {
            if record.status != ProcessStatus::Running {
                continue;
            }
            if !pid_alive(record.pid) {
                record.status = ProcessStatus::Completed;
                record.finished_at = Some(Utc::now());
                record.wait.status = DurableWaitStatus::Signaled;
                record.wait.updated_at = Utc::now();
                record.tail = read_tail(Path::new(&record.log_path));
                completed.push(record.clone());
                continue;
            }
            if !record.supervised {
                record.supervised = true;
                resume.push(record.clone());
            }
        }
        if !completed.is_empty() {
            persist(&guard.records);
        }
    }
    for record in resume {
        resume_watch(record);
    }
    for record in completed {
        crate::wake_runtime::close_gap(&record.session_id, &record.handle);
        let detail = format!(
            "process {} was no longer running after restart (pid {})",
            record.handle, record.pid
        );
        emit(ProcessWake {
            session_id: record.session_id,
            profile_id: record.profile_id,
            home_session: record.home_session,
            kind: WakeSource::ProcessCompleted,
            prompt: process_event_prompt(
                WakeSource::ProcessCompleted.as_str(),
                &record.handle,
                &detail,
                WakeAuthority::Report,
            ),
            handle: record.handle,
            detail,
            grant: WakeGrant::for_source(WakeSource::ProcessCompleted),
        });
    }
}

fn resume_watch(record: ProcessRecord) {
    let handle = record.handle.clone();
    let log_path = PathBuf::from(&record.log_path);
    let pid = record.pid;
    let deadline_at = record.deadline_at;
    tokio::spawn(async move {
        loop {
            if deadline_at.is_some_and(|deadline| Utc::now() >= deadline) {
                kill_pid(pid);
                tokio::time::sleep(Duration::from_millis(50)).await;
                finish_wait(&handle, &log_path, WaitEnd::TimedOut);
                return;
            }
            if !pid_alive(pid) {
                finish_wait(&handle, &log_path, WaitEnd::ExitedAfterRestart);
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });
}

async fn dispatch_daemon_wake(state: crate::daemon::state::AppState, wake: ProcessWake) {
    if let Some(home) = wake
        .home_session
        .clone()
        .filter(|home| home != &wake.session_id)
    {
        if let Err(error) = crate::agent_messaging::enqueue_process_event(
            &wake.profile_id,
            &wake.session_id,
            &home,
            &wake.prompt,
            wake.kind,
        )
        .await
        {
            tracing::warn!(%error, handle = %wake.handle, "remote process wake retained for delivery");
        }
        return;
    }
    let config =
        crate::daemon::ingest::resolve_session_runtime_config(&state, &wake.session_id).await;
    let mut request = crate::session_mapping::build_interactive_turn_request_for_ingest(
        &wake.session_id,
        wake.prompt.clone(),
        &config.draft_provider,
        &config.draft_model,
        &config.response_depth_mode,
        &config.reasoning_effort,
        None,
        None,
        None,
        None,
    );
    request.persist_user_turn = false;
    request.identity_user_id = Some(wake.profile_id.clone());
    request.agent_mode = Some(medousa_types::AgentModeId::Assistant);
    request.wake = Some(wake.grant);
    let turn_id = format!("process_{}", wake.handle);
    if let Err((status, message)) = crate::daemon::interactive::spawn_turn_ticket(
        &state,
        crate::request_principal::RequestPrincipal::continuation(wake.profile_id),
        turn_id,
        crate::turn_ticket::TurnTicketMode::Interactive,
        request,
        None,
        None,
    )
    .await
    {
        tracing::info!(%status, message, handle = %wake.handle, "process wake deferred until the session can take it");
    }
}

pub fn check(handle: &str) -> Result<Value, String> {
    let mut guard = store().lock().unwrap_or_else(|err| err.into_inner());
    let Some(record) = guard
        .records
        .iter_mut()
        .find(|record| record.handle == handle)
    else {
        return Err(format!(
            "process {handle} is not supervised. Allowed action: start it with cognition_shell_run background=true."
        ));
    };
    record.polls = record.polls.saturating_add(1);
    let polls = record.polls;
    let running = record.status == ProcessStatus::Running;
    let wake_registered = record.wake_registered;
    let payload = status_payload(record);
    persist(&guard.records);
    if running && wake_registered && polls > 1 {
        return Err(format!(
            "process {handle} already has a wake registered. Do not poll. Allowed action: finish the turn and wait for the process_completed or timed_out wake."
        ));
    }
    Ok(payload)
}

pub fn pending_wakes() -> Vec<ProcessWake> {
    store()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .wakes
        .clone()
}

fn handle_payload(record: &ProcessRecord) -> Value {
    json!({
        "handle": record.handle,
        "background": true,
        "status": record.status.as_str(),
        "pid": record.pid,
        "log_path": record.log_path,
        "wake": "process_completed",
        "deadline_at": record.deadline_at,
        "wait_id": record.wait.wait_id,
        "guidance": "The turn can finish. A process_completed or timed_out wake will resume this session. Do not poll this handle.",
    })
}

fn status_payload(record: &ProcessRecord) -> Value {
    json!({
        "handle": record.handle,
        "status": record.status.as_str(),
        "exit_code": record.exit_code,
        "pid": record.pid,
        "log_path": record.log_path,
        "tail": record.tail,
        "wake_registered": record.wake_registered,
        "wait_status": format!("{:?}", record.wait.status),
        "duration_ms": record.finished_at.unwrap_or_else(Utc::now)
            .signed_duration_since(record.started_at)
            .num_milliseconds()
            .max(0),
    })
}

fn inline_result(argv: &[String], status: std::process::ExitStatus) -> Value {
    json!({
        "handle": Value::Null,
        "background": false,
        "status": "completed",
        "exit_code": status.code(),
        "argv": argv,
        "guidance": "The command finished inside the grace window, so no wake was registered.",
    })
}

fn durable_wait(
    handle: &str,
    session_id: &str,
    status: DurableWaitStatus,
    now: DateTime<Utc>,
    deadline_at: Option<DateTime<Utc>>,
    payload: Option<String>,
) -> DurableWaitRecord {
    DurableWaitRecord {
        wait_id: format!("process-wait-{handle}"),
        job_id: format!("process-job-{handle}"),
        signal_type: SIGNAL_TYPE.to_string(),
        correlation_key: format!("{session_id}:{handle}"),
        status,
        deadline_at,
        created_at: now,
        updated_at: now,
        signal_payload: payload,
        consumed_signal_ids: Vec::new(),
    }
}

fn read_tail(path: &Path) -> String {
    let Ok(mut raw) = fs::read_to_string(path) else {
        return String::new();
    };
    const MAX: usize = 2000;
    if raw.len() > MAX {
        raw.replace_range(..raw.len() - MAX, "");
    }
    raw
}

/// `kill(pid, 0)` on Unix, and a query-only process handle on Windows.
/// `/proc` is Linux-only; using it on the Mac coordinator would mark every
/// still-running process finished after a restart.
fn pid_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    #[cfg(unix)]
    {
        // SAFETY: signal 0 does not deliver a signal. A positive pid addresses
        // one process, never a process group.
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const ERROR_ACCESS_DENIED: u32 = 5;
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
            fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
            fn GetLastError() -> u32;
        }
        // SAFETY: the handle is opened only to test that the pid exists, then closed.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            CloseHandle(handle);
            true
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

fn kill_pid(pid: u32) {
    if pid == 0 || pid > i32::MAX as u32 {
        return;
    }
    #[cfg(unix)]
    {
        // SAFETY: SIGKILL goes to this positive pid only.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        const PROCESS_TERMINATE: u32 = 0x0001;
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
            fn TerminateProcess(handle: *mut core::ffi::c_void, code: u32) -> i32;
            fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        }
        // SAFETY: the handle is opened only to terminate this pid, then closed.
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn isolated() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = TEST_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: process tests hold TEST_LOCK and point the ledger at a temp file.
        unsafe {
            std::env::set_var(
                "MEDOUSA_PROCESS_LEDGER_PATH",
                dir.path().join("processes.json"),
            );
        }
        let mut store = store().lock().unwrap_or_else(|err| err.into_inner());
        store.records.clear();
        store.wakes.clear();
        (guard, dir)
    }

    #[tokio::test]
    async fn background_process_wakes_instead_of_requiring_a_poll() {
        let (_lock, _dir) = isolated();
        let value = launch_detached(LaunchRequest {
            session_id: "ses_coordinator".into(),
            profile_id: "user:alice".into(),
            home_session: Some("ses_home".into()),
            argv: vec!["/bin/sleep".into(), "1".into()],
            cwd: None,
            deadline: None,
            background_after: None,
        })
        .await
        .expect("detached");
        let handle = value["handle"].as_str().unwrap().to_string();
        assert!(value["guidance"].as_str().unwrap().contains("Do not poll"));
        assert!(value["wait_id"].as_str().unwrap().contains("process-wait-"));
        let first = check(&handle).expect("first explicit check");
        assert_eq!(first["status"], "running");
        let refused = check(&handle).expect_err("repeated poll");
        assert!(refused.contains("Do not poll"), "{refused}");
        assert!(refused.contains("Allowed action:"), "{refused}");
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            if pending_wakes().iter().any(|wake| wake.handle == handle) {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("process_completed wake was not recorded");
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let wake = pending_wakes()
            .into_iter()
            .find(|wake| wake.handle == handle)
            .unwrap();
        assert_eq!(wake.kind, WakeSource::ProcessCompleted);
        assert_eq!(wake.grant.authority, WakeAuthority::Report);
        assert_eq!(wake.home_session.as_deref(), Some("ses_home"));
        assert!(wake.prompt.contains("[MEDOUSA_PROCESS_EVENT]"));
        assert!(
            crate::wake_runtime::gap_lines("ses_coordinator")
                .iter()
                .all(|gap| !gap.contains(&handle))
        );
    }

    #[tokio::test]
    async fn grace_window_returns_inline_when_the_command_finishes() {
        let (_lock, _dir) = isolated();
        let value = launch_detached(LaunchRequest {
            session_id: "ses_grace".into(),
            profile_id: "user:alice".into(),
            home_session: None,
            argv: vec!["/bin/true".into()],
            cwd: None,
            deadline: None,
            background_after: Some(Duration::from_secs(2)),
        })
        .await
        .expect("grace");
        assert_eq!(value["background"], false);
        assert!(value["handle"].is_null());
    }

    #[test]
    fn pid_probe_sees_this_process_and_not_a_finished_one() {
        assert!(pid_alive(std::process::id()));
        let mut child = Command::new("/bin/true").spawn().expect("true");
        let pid = child.id();
        let _ = child.wait();
        assert!(!pid_alive(pid));
        assert!(!pid_alive(0));
    }

    #[tokio::test]
    async fn deadline_wakes_timed_out_with_report_authority() {
        let (_lock, _dir) = isolated();
        let value = launch_detached(LaunchRequest {
            session_id: "ses_deadline".into(),
            profile_id: "user:alice".into(),
            home_session: None,
            argv: vec!["/bin/sleep".into(), "30".into()],
            cwd: None,
            deadline: Some(Duration::from_millis(200)),
            background_after: None,
        })
        .await
        .expect("detached");
        let handle = value["handle"].as_str().unwrap().to_string();
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            if pending_wakes()
                .iter()
                .any(|wake| wake.handle == handle && wake.kind == WakeSource::ProcessTimedOut)
            {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("timed_out wake was not recorded");
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let wake = pending_wakes()
            .into_iter()
            .find(|wake| wake.handle == handle)
            .unwrap();
        assert_eq!(wake.grant.authority, WakeAuthority::Report);
        assert!(wake.prompt.contains("timed_out"), "{}", wake.prompt);
    }

    #[tokio::test]
    async fn reconcile_finishes_a_dead_pid_and_resumes_a_live_one() {
        let (_lock, dir) = isolated();
        let dead_pid = 2_000_000_000;
        assert!(!pid_alive(dead_pid));
        let mut live = Command::new("/bin/sleep").arg("30").spawn().expect("sleep");
        let live_pid = live.id();
        let now = Utc::now();
        let dead_handle = "proc_dead".to_string();
        let live_handle = "proc_live".to_string();
        {
            let mut guard = store().lock().unwrap_or_else(|err| err.into_inner());
            guard.records.push(seed_record(
                &dead_handle,
                "ses_reconcile",
                dead_pid,
                now,
                None,
                &dir,
            ));
            guard.records.push(seed_record(
                &live_handle,
                "ses_reconcile",
                live_pid,
                now,
                Some(now - chrono::Duration::milliseconds(1)),
                &dir,
            ));
        }
        reconcile();
        let completed = pending_wakes()
            .into_iter()
            .find(|wake| wake.handle == dead_handle)
            .expect("dead pid wake");
        assert_eq!(completed.kind, WakeSource::ProcessCompleted);
        assert!(completed.detail.contains("no longer running after restart"));
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        loop {
            if pending_wakes()
                .iter()
                .any(|wake| wake.handle == live_handle && wake.kind == WakeSource::ProcessTimedOut)
            {
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = live.kill();
                panic!("resumed process was not timed out");
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let reaped = live.try_wait().expect("try_wait");
        assert!(
            reaped.is_some(),
            "deadline did not kill the resumed process {live_pid}"
        );
    }

    fn seed_record(
        handle: &str,
        session_id: &str,
        pid: u32,
        now: DateTime<Utc>,
        deadline_at: Option<DateTime<Utc>>,
        dir: &tempfile::TempDir,
    ) -> ProcessRecord {
        ProcessRecord {
            handle: handle.to_string(),
            session_id: session_id.to_string(),
            profile_id: "user:alice".into(),
            home_session: None,
            argv: vec!["/bin/sleep".into(), "30".into()],
            pid,
            log_path: dir
                .path()
                .join(format!("{handle}.log"))
                .display()
                .to_string(),
            status: ProcessStatus::Running,
            exit_code: None,
            tail: String::new(),
            started_at: now,
            finished_at: None,
            deadline_at,
            wake_registered: true,
            polls: 0,
            supervised: false,
            wait: durable_wait(
                handle,
                session_id,
                DurableWaitStatus::Pending,
                now,
                deadline_at,
                None,
            ),
        }
    }
}
