//! Vault Versions — optional Git-backed versioning (off by default).

mod install;
pub mod service;

pub use install::{GitInstallProgress, install_portable_git};
pub use service::{
    GitCommitRequest, GitCommitResponse, GitDetectResponse, GitDiffQuery, GitDiffResponse,
    GitLogEntry, GitLogQuery, GitRestoreRequest, GitStatusResponse, commit_version, detect_git,
    diff_note, ensure_enabled, git_log, git_status, init_repo, resolve_git_binary, restore_note,
    vault_git_enabled,
};
