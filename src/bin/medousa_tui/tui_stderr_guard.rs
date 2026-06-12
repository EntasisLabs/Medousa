//! Redirect process stderr while the TUI owns the terminal.
//!
//! Library init still uses `eprintln!` for daemon diagnostics; during TUI those lines
//! corrupt ratatui. We dup stderr to a log file for the whole TUI process lifetime.

use std::fs::{self, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::PathBuf;

pub struct TuiStderrGuard {
    restored: Option<OwnedFd>,
}

impl TuiStderrGuard {
    pub fn attach() -> io::Result<Self> {
        let path = log_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let log = OpenOptions::new().create(true).append(true).open(path)?;
        let original = unsafe { libc::dup(2) };
        if original == -1 {
            return Err(io::Error::last_os_error());
        }
        let dup_result = unsafe { libc::dup2(log.as_raw_fd(), 2) };
        if dup_result == -1 {
            let err = io::Error::last_os_error();
            unsafe {
                libc::close(original);
            }
            return Err(err);
        }
        Ok(Self {
            restored: Some(unsafe { OwnedFd::from_raw_fd(original) }),
        })
    }
}

impl Drop for TuiStderrGuard {
    fn drop(&mut self) {
        if let Some(original) = self.restored.take() {
            unsafe {
                libc::dup2(original.as_raw_fd(), 2);
            }
        }
    }
}

fn log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("logs")
        .join("tui-runtime.log")
}
