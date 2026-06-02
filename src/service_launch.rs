//! Detached background process spawn for Medousa CLI / setup.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Log file target for a background service.
pub struct BackgroundLog {
    pub path: PathBuf,
}

impl BackgroundLog {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn ensure_parent(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create log directory {}",
                    parent.display()
                )
            })?;
        }
        Ok(())
    }
}

/// Spawn a long-running process in the background (new session on Unix, logs to file).
pub fn spawn_background(
    program: impl AsRef<Path>,
    args: &[impl AsRef<str>],
    log: &BackgroundLog,
    mut configure: impl FnMut(&mut Command),
) -> Result<u32> {
    let mut command = Command::new(program.as_ref());
    for arg in args {
        command.arg(arg.as_ref());
    }
    configure(&mut command);
    spawn_command_background(command, log)
}

/// Spawn a pre-built `Command` detached with stdout/stderr appended to `log`.
pub fn spawn_command_background(mut command: Command, log: &BackgroundLog) -> Result<u32> {
    log.ensure_parent()?;

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log.path)
        .with_context(|| format!("failed to open log file {}", log.path.display()))?;
    let log_file_err = log_file
        .try_clone()
        .context("failed to duplicate log file handle")?;

    command.stdin(Stdio::null());
    command.stdout(Stdio::from(log_file));
    command.stderr(Stdio::from(log_file_err));
    detach_new_session(&mut command);

    let child = command
        .spawn()
        .context("failed to spawn background process")?;
    Ok(child.id())
}

#[cfg(unix)]
fn detach_new_session(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_new_session(_command: &mut Command) {}
