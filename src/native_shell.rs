//! Native Assistant terminal transport. Tool arguments and jail policy remain
//! shared with shell.run; only process transport and ownership live here.

use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use medousa_session::Session;
#[cfg(not(windows))]
use medousa_session::{SessionId, session::SessionRootKind};
use serde_json::{Value, json};
use stasis::prelude::{Result, StasisError};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use crate::agent_runtime::execution_context::{TurnHandle, active_turn_execution_context};

static EXECUTION: LazyLock<ForgeExecutionService> = LazyLock::new(ForgeExecutionService::new);
static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<OwnedTerminal>>>> =
    LazyLock::new(Default::default);
const PREFIX: &str = "native_shell_";
#[cfg(not(windows))]
const MAX_TERMINALS_PER_TURN: usize = 8;

struct OwnedTerminal {
    owner: TurnHandle,
    terminal: Arc<Session>,
    backend: &'static str,
    sandboxed: bool,
    warning: Option<String>,
    started: Instant,
    max_output: usize,
    observation: Mutex<Observation>,
    stop_reason: Mutex<Option<&'static str>>,
    complete_observed: std::sync::atomic::AtomicBool,
}
#[derive(Default)]
struct Observation {
    cursor: u64,
    retained: usize,
}

fn error(error: impl std::fmt::Display) -> StasisError {
    StasisError::PortFailure(error.to_string())
}

pub(crate) fn is_native_session(id: &str) -> bool {
    id.starts_with(PREFIX)
}

#[cfg(not(windows))]
pub(crate) async fn run(args: Value) -> Result<Value> {
    let owner =
        active_turn_execution_context().ok_or_else(|| error("shell requires an admitted turn"))?;
    let handle = owner.handle();
    // Reserve capacity before a potentially blocking spawn; simultaneous tool
    // calls cannot oversubscribe the turn's terminal limit.
    let mut sessions = SESSIONS.lock().await;
    // Once a command's exit and all output were observed, its ephemeral PTY
    // can be retired when starting the next command. Live terminals stay owned.
    sessions.retain(|_, session| {
        session.owner != handle
            || !session
                .complete_observed
                .load(std::sync::atomic::Ordering::Acquire)
    });
    if sessions
        .values()
        .filter(|session| session.owner == handle)
        .count()
        >= MAX_TERMINALS_PER_TURN
    {
        return Err(error("this turn already owns eight terminals"));
    }
    let id = format!("{PREFIX}{}", uuid::Uuid::new_v4().simple());
    let spawn_id = id.clone();
    let (terminal, launch, profile) = EXECUTION
        .run(ExecutionClass::WorkEnvironment, 256 * 1024, move || {
            Ok((|| -> anyhow::Result<_> {
                let request = crate::shell_sandbox::ShellRunRequest::from_args(&args)
                    .map_err(anyhow::Error::msg)?;
                let launch = crate::shell_sandbox::prepare_shell_command(&request)
                    .map_err(anyhow::Error::msg)?;
                let mut command = portable_pty::CommandBuilder::new(launch.command.get_program());
                for arg in launch.command.get_args() {
                    #[cfg(target_os = "linux")]
                    if launch.backend == "bubblewrap" && arg == "--new-session" {
                        continue;
                    }
                    #[cfg(target_os = "linux")]
                    if launch.backend == "systemd-run" && arg == "--pipe" {
                        command.arg("--pty");
                        continue;
                    }
                    command.arg(arg);
                }
                // The process fallback has always cleared ambient credentials. Preserve
                // that environment policy when converting the prepared command to a PTY.
                if launch.backend == "process" {
                    command.env_clear();
                }
                for (key, value) in launch.command.get_envs() {
                    if let Some(value) = value {
                        command.env(key, value);
                    } else {
                        command.env_remove(key);
                    }
                }
                command.env("TERM", "dumb");
                command.env("PAGER", "cat");
                let terminal = Session::spawn_prepared_command(
                    SessionId(spawn_id),
                    request.profile.resolve_cwd(),
                    SessionRootKind::Workspace,
                    None,
                    request.argv,
                    command,
                    120,
                    40,
                )?;
                Ok((terminal, launch, request.profile))
            })())
        })
        .await
        .map_err(error)?
        .map_err(error)?;
    let owned = Arc::new(OwnedTerminal {
        owner: handle,
        terminal,
        backend: launch.backend,
        sandboxed: launch.sandboxed,
        warning: launch.warning.clone(),
        started: Instant::now(),
        max_output: profile.max_output_bytes,
        observation: Mutex::new(Observation::default()),
        stop_reason: Mutex::new(None),
        complete_observed: std::sync::atomic::AtomicBool::new(false),
    });
    sessions.insert(id.clone(), owned.clone());
    drop(sessions);
    let cancellation = owner.cancellation().clone();
    let deadline = owner.deadline();
    let monitor = owned.clone();
    // Unlike turn child tasks, this supervisor must run its cleanup after the
    // owner's cancellation. It retains no model execution or operator grant.
    tokio::spawn(async move {
        loop {
            if monitor.terminal.exited() && monitor.terminal.output_finished() {
                break;
            }
            let timed_out = profile
                .timeout_ms
                .is_some_and(|ms| monitor.started.elapsed() >= Duration::from_millis(ms));
            if cancellation.is_cancelled()
                || deadline.is_some_and(|at| Instant::now() >= at)
                || timed_out
            {
                *monitor.stop_reason.lock().await =
                    Some(if timed_out { "timed_out" } else { "cancelled" });
                let terminal = monitor.terminal.clone();
                let _ = EXECUTION
                    .run(ExecutionClass::WorkEnvironment, 0, move || {
                        #[cfg(unix)]
                        if let Some(pid) = terminal.process_id() {
                            let _ = medousa_host::force_process_tree_stop_by_pid(pid);
                        }
                        terminal.kill();
                        Ok(())
                    })
                    .await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // Keep Seatbelt's profile alive until the owned process terminates.
        let _ = EXECUTION
            .run(ExecutionClass::WorkEnvironment, 0, move || {
                drop(launch);
                Ok(())
            })
            .await;
        // Completed output remains readable until the owner finishes. A turn
        // that stays open can collect the final exit status with a later poll.
        // The registry owns that retention; retiring a completed command must
        // also release its PTY and replay buffer during a long-running turn.
        drop(monitor);
        cancellation.cancelled().await;
        SESSIONS.lock().await.remove(&id);
    });
    observe_owned(owned, None, 1500, None).await
}

#[cfg(windows)]
pub(crate) async fn run(args: Value) -> Result<Value> {
    // Retain AppContainer/Job fencing until ConPTY can be admitted inside the
    // same Windows security boundary. Never substitute an unjailed terminal.
    EXECUTION
        .run(ExecutionClass::WorkEnvironment, 256 * 1024, move || {
            Ok((|| -> anyhow::Result<_> {
                let request = crate::shell_sandbox::ShellRunRequest::from_args(&args)
                    .map_err(anyhow::Error::msg)?;
                crate::shell_sandbox::run_sandboxed(&request)
                    .map(|result| result.to_json())
                    .map_err(anyhow::Error::msg)
            })())
        })
        .await
        .map_err(error)?
        .map_err(error)
}

async fn owned_session(id: &str) -> Result<Arc<OwnedTerminal>> {
    let owner = active_turn_execution_context()
        .ok_or_else(|| error("terminal control requires an admitted turn"))?;
    let session = SESSIONS
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| error("owned terminal is unavailable"))?;
    if session.owner != owner.handle() {
        return Err(error("terminal belongs to a different turn"));
    }
    if !crate::shell_sandbox::shell_agent_tools_enabled() {
        return Err(error("Shell agent tools are disabled in Settings → Shell"));
    }
    Ok(session)
}

pub(crate) async fn observe(
    id: &str,
    input: Option<String>,
    wait_ms: u64,
    after_sequence: Option<u64>,
) -> Result<Value> {
    observe_owned(owned_session(id).await?, input, wait_ms, after_sequence).await
}

async fn observe_owned(
    session: Arc<OwnedTerminal>,
    input: Option<String>,
    wait_ms: u64,
    after_sequence: Option<u64>,
) -> Result<Value> {
    let mut observation = session
        .observation
        .try_lock()
        .map_err(|_| error("this terminal is already being observed"))?;
    if let Some(input) = input.as_ref() {
        if input.len() > 16 * 1024 {
            return Err(error("terminal input exceeds 16 KiB; send bounded chunks"));
        }
        if session.terminal.exited() {
            return Err(error("terminal command has already exited"));
        }
        let terminal = session.terminal.clone();
        let bytes = input.as_bytes().to_vec();
        EXECUTION
            .run(ExecutionClass::WorkEnvironment, bytes.len(), move || {
                Ok(terminal.write(&bytes))
            })
            .await
            .map_err(error)?
            .map_err(error)?;
    }
    let until = tokio::time::Instant::now() + Duration::from_millis(wait_ms.clamp(100, 15_000));
    while tokio::time::Instant::now() < until
        && !(session.terminal.exited() && session.terminal.output_finished())
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let cursor = after_sequence.unwrap_or(observation.cursor);
    let chunks = session.terminal.output_snapshot();
    let replay_truncated = chunks
        .first()
        .is_some_and(|chunk| cursor != 0 && chunk.sequence > cursor + 1);
    let mut bytes = Vec::new();
    let mut output_truncated = false;
    for chunk in chunks.iter().filter(|chunk| chunk.sequence > cursor) {
        let available = session
            .max_output
            .saturating_sub(observation.retained + bytes.len());
        bytes.extend_from_slice(&chunk.bytes[..chunk.bytes.len().min(available)]);
        output_truncated |= chunk.bytes.len() > available;
    }
    observation.retained += bytes.len();
    observation.cursor = chunks
        .last()
        .map(|chunk| chunk.sequence)
        .unwrap_or(cursor)
        .max(observation.cursor);
    let output = String::from_utf8_lossy(&bytes).into_owned();
    let reason = *session.stop_reason.lock().await;
    let completed = session.terminal.exited() && session.terminal.output_finished();
    if completed {
        session
            .complete_observed
            .store(true, std::sync::atomic::Ordering::Release);
    }
    Ok(json!({
        "ok": true, "session_id": session.terminal.meta.session_id.as_str(),
        "output": output, "stdout": output, "stderr": "",
        "input_written": input.is_some(), "next_sequence": observation.cursor,
        "replay_truncated": replay_truncated, "output_truncated": output_truncated,
        "completed": completed,
        "exit_code": session.terminal.exit_code(), "backend": session.backend,
        "sandboxed": session.sandboxed, "timed_out": reason == Some("timed_out"),
        "cancelled": reason == Some("cancelled"), "duration_ms": session.started.elapsed().as_millis() as u64,
        "warning": session.warning,
        "continuation": "For running commands use cognition_shell_session_run with this session_id and poll=true or input; cognition_shell_session_interrupt sends Ctrl-C. Output and errors share the terminal stream."
    }))
}

pub(crate) async fn interrupt(id: &str) -> Result<Value> {
    let session = owned_session(id).await?;
    let terminal = session.terminal.clone();
    EXECUTION
        .run(ExecutionClass::WorkEnvironment, 0, move || {
            terminal.signal_interrupt();
            Ok(())
        })
        .await
        .map_err(error)?;
    Ok(json!({"ok": true, "session_id": id, "signal": "interrupt"}))
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use crate::agent_runtime::execution_context::{
        TurnExecutionContext, with_turn_execution_context,
    };
    fn context() -> Arc<TurnExecutionContext> {
        let id = format!("test_{}", uuid::Uuid::new_v4().simple());
        Arc::new(
            TurnExecutionContext::from_scope(
                id.clone(),
                crate::request_principal::RequestPrincipal::continuation("user:test"),
                tokio_util::sync::CancellationToken::new(),
                None,
                crate::turn_scope::TurnContinuationScope {
                    turn_correlation_id: id.clone(),
                    session_id: id,
                    identity_user_id: Some("user:test".into()),
                    original_prompt: "test".into(),
                    delivery_target: None,
                    provider: "test".into(),
                    model: "test".into(),
                    response_depth_mode: "concise".into(),
                    supports_ui_artifacts: false,
                    supports_liquid_markdown: false,
                    supports_browser_host: false,
                    browser_driver_id: None,
                    selected_worlds: vec![],
                    channel_surface: None,
                },
            )
            .unwrap(),
        )
    }
    #[tokio::test]
    async fn native_terminal_accepts_stdin_and_reports_the_real_exit() {
        let dir = tempfile::tempdir().unwrap();
        let owner = context();
        let result = with_turn_execution_context(owner.clone(), run(json!({"argv": ["/bin/sh", "-c", "test -t 0 && test -t 1 && test -t 2 || exit 99; printf ready; read -r value; printf 'received:%s' \"$value\"; exit 7"], "cwd": dir.path(), "timeout_ms": 5000}))).await.unwrap();
        assert_eq!(result["completed"], false, "{result}");
        assert!(result["stdout"].as_str().unwrap().contains("ready"));
        let id = result["session_id"].as_str().unwrap();
        let session = SESSIONS.lock().await.get(id).unwrap().clone();
        let last = observe_owned(session, Some("hello\n".into()), 1000, None)
            .await
            .unwrap();
        assert_eq!(last["completed"], true, "{last}");
        assert_eq!(last["exit_code"], 7);
        assert!(last["stdout"].as_str().unwrap().contains("received:hello"));
        let other = context();
        let denied = with_turn_execution_context(other.clone(), owned_session(id))
            .await
            .err()
            .unwrap();
        assert!(denied.to_string().contains("different turn"));
        owner.cancellation().cancel();
        other.cancellation().cancel();
    }
    #[tokio::test]
    async fn native_terminal_deadline_and_output_cap_are_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let owner = context();
        let result = with_turn_execution_context(owner.clone(), run(json!({"argv": ["/bin/sh", "-c", "yes x"], "cwd": dir.path(), "timeout_ms": 150, "max_output_bytes": 1024}))).await.unwrap();
        assert_eq!(result["timed_out"], true, "{result}");
        assert!(result["stdout"].as_str().unwrap().len() <= 1024);
        assert_eq!(result["output_truncated"], true);
        owner.cancellation().cancel();
    }
    #[tokio::test]
    async fn cancelling_the_owner_stops_a_running_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let owner = context();
        let result = with_turn_execution_context(
            owner.clone(),
            run(json!({"argv": ["/bin/sh", "-c", "sleep 30"], "cwd": dir.path()})),
        )
        .await
        .unwrap();
        let session = SESSIONS
            .lock()
            .await
            .get(result["session_id"].as_str().unwrap())
            .unwrap()
            .clone();
        owner.cancellation().cancel();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !session.terminal.exited() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(*session.stop_reason.lock().await, Some("cancelled"));
    }
}
