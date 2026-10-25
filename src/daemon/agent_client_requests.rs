//! ACP client services stay inside the daemon-admitted workspace and session.
//! Approval belongs to the prompt pump; these handlers never issue authority.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use medousa_forge::execution::ExecutionClass;
use medousa_session::Session;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::state::AppState;

const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_TERMINALS: usize = 8;
const MAX_ENV_BYTES: usize = 64 * 1024;

struct OwnedTerminal {
    terminal: Arc<Session>,
    output_limit: usize,
    released: std::sync::atomic::AtomicBool,
}

type TerminalKey = (String, String);
static TERMINALS: LazyLock<Mutex<HashMap<TerminalKey, Arc<OwnedTerminal>>>> =
    LazyLock::new(Default::default);

/// Root is the admitted provider cwd, never an authority selected by ACP params.
pub(crate) async fn handle(
    state: &AppState,
    owner_session: &str,
    cwd: &str,
    method: &str,
    params: Value,
    cancelled: &Arc<Mutex<bool>>,
) -> Result<Value, String> {
    if owner_session.is_empty() || owner_session.len() > 256 {
        return Err("invalid terminal owner session".into());
    }
    if *cancelled.lock().await {
        return Err("agent session was cancelled".into());
    }
    match method {
        "fs/read_text_file" | "fs/write_text_file" => {
            let root = PathBuf::from(cwd);
            let write = method == "fs/write_text_file";
            let cancelled = cancelled.clone();
            state
                .forge_execution
                .run(ExecutionClass::WorkEnvironment, MAX_FILE_BYTES, move || {
                    Ok(if *cancelled.blocking_lock() {
                        Err("agent session was cancelled".to_string())
                    } else {
                        filesystem_request(&root, &params, write)
                    })
                })
                .await
                .map_err(|error| error.to_string())?
        }
        "terminal/create" => create_terminal(state, owner_session, cwd, params, cancelled).await,
        "terminal/output" => {
            let terminal = owned_terminal(owner_session, &params).await?;
            terminal_output(state, terminal).await
        }
        "terminal/wait_for_exit" => {
            let terminal = owned_terminal(owner_session, &params).await?;
            loop {
                if *cancelled.lock().await {
                    stop_terminal(state, terminal.clone()).await;
                    return Err("agent session was cancelled".into());
                }
                if terminal.released.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("terminal was released".into());
                }
                if terminal.terminal.exited() && terminal.terminal.output_finished() {
                    return Ok(json!({"exitCode": terminal.terminal.exit_code()}));
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
        "terminal/kill" => {
            let terminal = owned_terminal(owner_session, &params).await?;
            stop_terminal(state, terminal).await;
            Ok(json!({}))
        }
        "terminal/release" => {
            let key = terminal_key(owner_session, &params)?;
            let terminal = TERMINALS
                .lock()
                .await
                .remove(&key)
                .ok_or_else(|| "terminal does not belong to this agent session".to_string())?;
            terminal
                .released
                .store(true, std::sync::atomic::Ordering::Release);
            stop_terminal(state, terminal).await;
            Ok(json!({}))
        }
        _ => Err(format!("unsupported ACP client method: {method}")),
    }
}

pub(crate) async fn cleanup(state: &AppState, owner_session: &str) {
    let removed = {
        let mut registry = TERMINALS.lock().await;
        let keys: Vec<_> = registry
            .keys()
            .filter(|(owner, _)| owner == owner_session)
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|key| registry.remove(&key))
            .collect::<Vec<_>>()
    };
    for terminal in removed {
        terminal
            .released
            .store(true, std::sync::atomic::Ordering::Release);
        stop_terminal(state, terminal).await;
    }
}

fn text_param<'a>(params: &'a Value, name: &str) -> Result<&'a str, String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.contains('\0'))
        .ok_or_else(|| format!("{name} must be a nonempty string"))
}

fn scoped_path(
    root: &Path,
    requested: &str,
    allow_new_file: bool,
) -> Result<(PathBuf, PathBuf), String> {
    if requested.is_empty() || requested.len() > 16 * 1024 || requested.contains('\0') {
        return Err("invalid workspace path".into());
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("workspace is unavailable: {error}"))?;
    if !root.is_dir() {
        return Err("workspace root is not a directory".into());
    }
    let requested = Path::new(requested);
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let path = match candidate.canonicalize() {
        Ok(path) => path,
        Err(error) if allow_new_file && error.kind() == std::io::ErrorKind::NotFound => {
            // A dangling symlink is not a new file and must not be followed.
            if std::fs::symlink_metadata(&candidate).is_ok() {
                return Err("workspace file is a dangling symlink".into());
            }
            let parent = candidate
                .parent()
                .ok_or_else(|| "file has no parent".to_string())?
                .canonicalize()
                .map_err(|error| format!("file parent is unavailable: {error}"))?;
            let name = candidate
                .file_name()
                .ok_or_else(|| "invalid file name".to_string())?;
            parent.join(name)
        }
        Err(error) => return Err(format!("workspace path is unavailable: {error}")),
    };
    if !path.starts_with(&root) {
        return Err("ACP client paths must stay inside the admitted workspace".into());
    }
    Ok((root, path))
}

fn filesystem_request(root: &Path, params: &Value, write: bool) -> Result<Value, String> {
    let write_content = if write {
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| "content must be a string".to_string())?;
        if content.len() > MAX_FILE_BYTES {
            return Err("ACP text files are limited to 1 MiB".into());
        }
        Some(content)
    } else {
        None
    };
    let (root, path) = scoped_path(root, text_param(params, "path")?, write)?;
    let mut file = open_scoped_file(&root, &path, write)?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("ACP filesystem requests require a regular file".into());
    }
    if let Some(content) = write_content {
        file.set_len(0)
            .and_then(|_| file.write_all(content.as_bytes()))
            .map_err(|error| error.to_string())?;
        return Ok(json!({}));
    }
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("ACP text files are limited to 1 MiB".into());
    }
    let content = String::from_utf8(bytes).map_err(|_| "file is not UTF-8 text".to_string())?;
    let line = match params.get("line") {
        None => 1,
        Some(value) => value
            .as_u64()
            .filter(|line| *line > 0)
            .ok_or_else(|| "line must be a positive integer".to_string())?,
    };
    let limit = match params.get("limit") {
        None => usize::MAX,
        Some(value) => usize::try_from(
            value
                .as_u64()
                .ok_or_else(|| "limit must be a nonnegative integer".to_string())?,
        )
        .map_err(|_| "line limit is too large".to_string())?,
    };
    let offset = usize::try_from(line - 1).map_err(|_| "line is too large".to_string())?;
    let content: String = content
        .split_inclusive('\n')
        .skip(offset)
        .take(limit)
        .collect();
    Ok(json!({"content": content}))
}

#[cfg(unix)]
fn open_scoped_file(root: &Path, path: &Path, write: bool) -> Result<std::fs::File, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "path escapes workspace".to_string())?;
    let names: Vec<_> = relative
        .components()
        .map(|component| match component {
            std::path::Component::Normal(name) => std::ffi::CString::new(name.as_bytes())
                .map_err(|_| "invalid path component".to_string()),
            _ => Err("invalid workspace path component".to_string()),
        })
        .collect::<Result<_, _>>()?;
    let (name, parents) = names
        .split_last()
        .ok_or_else(|| "workspace root is not a file".to_string())?;
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .map_err(|error| error.to_string())?;
    for parent in parents {
        // Each component is opened relative to the already admitted directory;
        // swapping an ancestor for a symlink cannot escape the workspace.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                parent.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        directory = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    let access = if write {
        libc::O_WRONLY | libc::O_CREAT
    } else {
        libc::O_RDONLY
    };
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            access | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    if write {
        use std::os::unix::fs::MetadataExt;
        if file.metadata().map_err(|error| error.to_string())?.nlink() > 1 {
            return Err("ACP writes cannot modify files with multiple hard links".into());
        }
    }
    Ok(file)
}

#[cfg(not(unix))]
fn open_scoped_file(root: &Path, path: &Path, write: bool) -> Result<std::fs::File, String> {
    let (_, checked) = scoped_path(root, &path.to_string_lossy(), write)?;
    std::fs::OpenOptions::new()
        .read(!write)
        .write(write)
        .create(write)
        .open(checked)
        .map_err(|error| error.to_string())
}

struct CommandRequest {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    output_limit: usize,
}

fn parse_command(params: &Value) -> Result<CommandRequest, String> {
    let command = text_param(params, "command")?;
    if command.len() > 16 * 1024 {
        return Err("terminal command is too long".into());
    }
    let mut argv = vec![command.to_string()];
    if let Some(args) = params.get("args") {
        let args = args
            .as_array()
            .filter(|args| args.len() <= 256)
            .ok_or_else(|| "args must contain at most 256 strings".to_string())?;
        for arg in args {
            let arg = arg
                .as_str()
                .filter(|arg| !arg.contains('\0'))
                .ok_or_else(|| {
                    "terminal arguments must be strings without NUL bytes".to_string()
                })?;
            argv.push(arg.to_string());
        }
    }
    if argv.iter().map(String::len).sum::<usize>() > 128 * 1024 {
        return Err("terminal arguments exceed the byte limit".into());
    }
    let mut env = Vec::new();
    let mut env_bytes = 0usize;
    if let Some(values) = params.get("env") {
        let values = values
            .as_array()
            .filter(|values| values.len() <= 64)
            .ok_or_else(|| "env must contain at most 64 name/value pairs".to_string())?;
        for item in values {
            let name = text_param(item, "name")?;
            let value = item
                .get("value")
                .and_then(Value::as_str)
                .filter(|value| !value.contains('\0'))
                .ok_or_else(|| "invalid terminal environment value".to_string())?;
            if !valid_env_name(name) {
                return Err(format!("terminal environment cannot override {name}"));
            }
            env_bytes = env_bytes
                .saturating_add(name.len())
                .saturating_add(value.len());
            if env_bytes > MAX_ENV_BYTES {
                return Err("terminal environment exceeds 64 KiB".into());
            }
            env.push((name.to_string(), value.to_string()));
        }
    }
    let cwd = params
        .get("cwd")
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "cwd must be a nonempty string".to_string())
        })
        .transpose()?;
    let output_limit = match params.get("outputByteLimit") {
        None => MAX_OUTPUT_BYTES,
        Some(value) => value
            .as_u64()
            .filter(|limit| *limit > 0 && *limit <= MAX_OUTPUT_BYTES as u64)
            .map(|limit| limit as usize)
            .ok_or_else(|| "outputByteLimit must be between 1 and 2097152".to_string())?,
    };
    Ok(CommandRequest {
        argv,
        env,
        cwd,
        output_limit,
    })
}

fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    let upper = name.to_ascii_uppercase();
    valid
        && name.len() <= 256
        && !upper.starts_with("LD_")
        && !upper.starts_with("DYLD_")
        && !matches!(upper.as_str(), "PATH" | "BASH_ENV" | "ENV" | "GCONV_PATH")
}

fn terminal_key(owner: &str, params: &Value) -> Result<TerminalKey, String> {
    let terminal = text_param(params, "terminalId")?;
    if terminal.len() > 128 {
        return Err("invalid terminalId".into());
    }
    Ok((owner.to_string(), terminal.to_string()))
}

async fn owned_terminal(owner: &str, params: &Value) -> Result<Arc<OwnedTerminal>, String> {
    TERMINALS
        .lock()
        .await
        .get(&terminal_key(owner, params)?)
        .cloned()
        .ok_or_else(|| "terminal does not belong to this agent session".to_string())
}

async fn terminal_output(state: &AppState, owned: Arc<OwnedTerminal>) -> Result<Value, String> {
    state
        .forge_execution
        .run(ExecutionClass::Observation, MAX_OUTPUT_BYTES, move || {
            let chunks = owned.terminal.output_snapshot();
            let previously_truncated = chunks.first().is_some_and(|chunk| chunk.sequence > 1);
            let mut bytes: Vec<u8> = chunks.into_iter().flat_map(|chunk| chunk.bytes).collect();
            let mut truncated = previously_truncated || bytes.len() > owned.output_limit;
            if bytes.len() > owned.output_limit {
                bytes.drain(..bytes.len() - owned.output_limit);
            }
            let mut output = String::from_utf8_lossy(&bytes).into_owned();
            // Invalid UTF-8 replacement may expand bytes; the wire response
            // still respects the requested byte limit and character boundaries.
            if output.len() > owned.output_limit {
                let mut start = output.len() - owned.output_limit;
                while !output.is_char_boundary(start) {
                    start += 1;
                }
                output.drain(..start);
                truncated = true;
            }
            let mut value = json!({"output": output, "truncated": truncated});
            if owned.terminal.exited() && owned.terminal.output_finished() {
                value["exitStatus"] = json!({"exitCode": owned.terminal.exit_code()});
            }
            Ok(value)
        })
        .await
        .map_err(|error| error.to_string())
}

async fn stop_terminal(state: &AppState, owned: Arc<OwnedTerminal>) {
    let _ = state
        .forge_execution
        .run(ExecutionClass::WorkEnvironment, 0, move || {
            #[cfg(unix)]
            if let Some(pid) = owned.terminal.process_id() {
                let _ = medousa_host::force_process_tree_stop_by_pid(pid);
            }
            owned.terminal.kill();
            Ok(())
        })
        .await;
}

#[cfg(windows)]
async fn create_terminal(
    _state: &AppState,
    _owner: &str,
    _cwd: &str,
    _params: Value,
    _cancelled: &Arc<Mutex<bool>>,
) -> Result<Value, String> {
    // The shared Windows shell uses AppContainer/Job policy; its PTY variant
    // cannot yet retain that policy. Do not substitute an unjailed ConPTY.
    Err("ACP terminals require a workshop with sandboxed native PTY support".into())
}

#[cfg(not(windows))]
async fn create_terminal(
    state: &AppState,
    owner: &str,
    cwd: &str,
    params: Value,
    cancelled: &Arc<Mutex<bool>>,
) -> Result<Value, String> {
    use medousa_session::{SessionId, session::SessionRootKind};
    let requested = parse_command(&params)?;
    let id = format!("acp_terminal_{}", uuid::Uuid::new_v4().simple());
    let key = (owner.to_string(), id.clone());
    let mut registry = TERMINALS.lock().await;
    if registry
        .keys()
        .filter(|(existing_owner, _)| existing_owner == owner)
        .count()
        >= MAX_TERMINALS
    {
        return Err("this agent session already owns eight terminals; release one first".into());
    }
    let root = PathBuf::from(cwd);
    let spawn_id = id.clone();
    let output_limit = requested.output_limit;
    let spawn_cancelled = cancelled.clone();
    let (terminal, launch, timeout) = state
        .forge_execution
        .run(
            ExecutionClass::WorkEnvironment,
            MAX_OUTPUT_BYTES,
            move || {
                Ok((|| -> anyhow::Result<_> {
                    if *spawn_cancelled.blocking_lock() {
                        anyhow::bail!("agent session was cancelled");
                    }
                    let (root, cwd) =
                        scoped_path(&root, requested.cwd.as_deref().unwrap_or("."), false)
                            .map_err(anyhow::Error::msg)?;
                    if !cwd.is_dir() {
                        anyhow::bail!("terminal cwd must be a directory");
                    }
                    let charter = crate::shell_sandbox::ShellCharterDefaults::from_tui_defaults();
                    let mut writable_roots = Vec::new();
                    if charter.writable_roots.is_empty() {
                        writable_roots.push(root.clone());
                    } else {
                        for allowed in &charter.writable_roots {
                            if let Ok(allowed) = allowed.canonicalize() {
                                if allowed.starts_with(&root) {
                                    writable_roots.push(allowed);
                                } else if root.starts_with(&allowed) {
                                    writable_roots.push(root.clone());
                                }
                            }
                        }
                        if writable_roots.is_empty() {
                            anyhow::bail!("workspace is outside Settings → Shell writable roots");
                        }
                        if !writable_roots.iter().any(|root| cwd.starts_with(root)) {
                            anyhow::bail!(
                                "terminal cwd is outside Settings → Shell writable roots"
                            );
                        }
                    }
                    let request = crate::shell_sandbox::ShellRunRequest {
                        argv: requested.argv,
                        profile: crate::shell_sandbox::ShellPermissionProfile {
                            cwd: Some(cwd.clone()),
                            writable_roots,
                            readonly_roots: Vec::new(),
                            network: charter.network,
                            timeout_ms: charter.timeout_ms,
                            max_output_bytes: output_limit.min(charter.max_output_bytes),
                            allowed_binaries: charter.allowed_binaries,
                        },
                    };
                    let launch = crate::shell_sandbox::prepare_shell_command(&request)
                        .map_err(anyhow::Error::msg)?;
                    let mut command =
                        portable_pty::CommandBuilder::new(launch.command.get_program());
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
                    if launch.backend == "process" {
                        command.env_clear();
                    }
                    for (name, value) in launch.command.get_envs() {
                        if let Some(value) = value {
                            command.env(name, value);
                        } else {
                            command.env_remove(name);
                        }
                    }
                    for (name, value) in requested.env {
                        command.env(name, value);
                    }
                    command.env("TERM", "dumb");
                    command.env("PAGER", "cat");
                    if *spawn_cancelled.blocking_lock() {
                        anyhow::bail!("agent session was cancelled");
                    }
                    let terminal = Session::spawn_prepared_command(
                        SessionId(spawn_id),
                        cwd,
                        SessionRootKind::Workspace,
                        None,
                        request.argv,
                        command,
                        120,
                        40,
                    )?;
                    Ok((terminal, launch, request.profile.timeout_ms))
                })())
            },
        )
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let owned = Arc::new(OwnedTerminal {
        terminal,
        output_limit,
        released: std::sync::atomic::AtomicBool::new(false),
    });
    registry.insert(key, owned.clone());
    drop(registry);
    let state = state.clone();
    let cancelled = cancelled.clone();
    let monitor = owned.clone();
    tokio::spawn(async move {
        let started = Instant::now();
        loop {
            if monitor.terminal.exited() && monitor.terminal.output_finished() {
                break;
            }
            if *cancelled.lock().await
                || monitor.released.load(std::sync::atomic::Ordering::Acquire)
                || timeout
                    .is_some_and(|timeout| started.elapsed() >= Duration::from_millis(timeout))
            {
                stop_terminal(&state, monitor.clone()).await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let _ = state
            .forge_execution
            .run(ExecutionClass::WorkEnvironment, 0, move || {
                drop(launch);
                Ok(())
            })
            .await;
    });
    Ok(json!({"terminalId": id}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_canonical_workspace_for_existing_and_new_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("existing"), "hello").unwrap();
        assert!(scoped_path(&root, "existing", false).is_ok());
        assert!(scoped_path(&root, "new", true).is_ok());
        assert!(scoped_path(&root, "../outside", true).is_err());
        assert!(scoped_path(&root, &temp.path().to_string_lossy(), false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escapes_and_swapped_ancestors_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        assert!(scoped_path(&root, "escape/new.txt", true).is_err());
        std::fs::write(outside.join("existing"), "secret").unwrap();
        assert!(scoped_path(&root, "escape/existing", false).is_err());
        let root = root.canonicalize().unwrap();
        assert!(open_scoped_file(&root, &root.join("escape/existing"), false).is_err());
    }

    #[test]
    fn commands_are_argv_without_implicit_shell_interpretation() {
        let command = parse_command(&json!({"command":"printf", "args":["%s", "a; rm -rf nope"], "env":[{"name":"TEST_VALUE", "value":"hello"}]})).unwrap();
        assert_eq!(command.argv, ["printf", "%s", "a; rm -rf nope"]);
        assert_eq!(command.env, [("TEST_VALUE".into(), "hello".into())]);
        assert!(parse_command(&json!({"command":"printf", "args":[12]})).is_err());
        assert!(
            parse_command(
                &json!({"command":"printf", "env":[{"name":"LD_PRELOAD", "value":"evil"}]})
            )
            .is_err()
        );
        assert!(
            parse_command(&json!({"command":"printf", "outputByteLimit":MAX_OUTPUT_BYTES+1}))
                .is_err()
        );
    }

    #[test]
    fn text_reads_honor_line_ranges_and_writes_do_not_follow_escaping_paths() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("text"), "one\ntwo\nthree\n").unwrap();
        let read = filesystem_request(
            temp.path(),
            &json!({"path":"text", "line":2, "limit":1}),
            false,
        )
        .unwrap();
        assert_eq!(read["content"], "two\n");
        assert!(
            filesystem_request(
                temp.path(),
                &json!({"path":"../outside", "content":"no"}),
                true
            )
            .is_err()
        );
        filesystem_request(
            temp.path(),
            &json!({"path":"text", "content":"updated"}),
            true,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(temp.path().join("text")).unwrap(),
            "updated"
        );
    }

    #[test]
    fn invalid_or_oversized_writes_do_not_create_or_truncate_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("text"), "preserve me").unwrap();
        let too_large = "x".repeat(MAX_FILE_BYTES + 1);
        assert!(
            filesystem_request(
                temp.path(),
                &json!({"path":"new", "content":too_large}),
                true
            )
            .is_err()
        );
        assert!(!temp.path().join("new").exists());
        assert!(
            filesystem_request(temp.path(), &json!({"path":"text", "content":null}), true).is_err()
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("text")).unwrap(),
            "preserve me"
        );
    }

    #[cfg(unix)]
    #[test]
    fn hard_linked_files_cannot_mutate_content_outside_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = temp.path().join("outside");
        std::fs::write(&outside, "preserve me").unwrap();
        std::fs::hard_link(&outside, root.join("linked")).unwrap();
        assert!(
            filesystem_request(
                &root,
                &json!({"path":"linked", "content":"overwritten"}),
                true
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "preserve me");
    }
}
