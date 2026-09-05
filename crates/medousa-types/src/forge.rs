//! Runtime-neutral Forge wire types.
//!
//! These records cross work-environment and federation boundaries, so their
//! serde contract must remain available to embedded clients without linking
//! Forge's host filesystem and process implementation.

use serde::{Deserialize, Serialize};

/// Governance over paths an executor may touch, and rules for checkpoint
/// capture. Violations are evidence — they never prove containment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkPolicy {
    /// Normalized git-path globs the executor is allowed to modify. Empty =
    /// everything allowed (violations still computed for report completeness).
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    /// Normalized git-path globs that are always violations.
    #[serde(default)]
    pub denied_paths: Vec<String>,
    /// Additional protected refs beyond the always-protected base ref.
    #[serde(default)]
    pub protected_refs: Vec<String>,
    /// Capture all non-ignored changes into the checkpoint commit.
    #[serde(default = "default_true")]
    pub checkpoint_capture_all: bool,
    /// Maximum bytes for any single captured file (0 = unlimited).
    #[serde(default)]
    pub checkpoint_max_file_bytes: u64,
    /// Maximum total bytes captured (0 = unlimited).
    #[serde(default)]
    pub checkpoint_max_total_bytes: u64,
    /// Path classes excluded from checkpoint capture (normalized git-path globs).
    #[serde(default)]
    pub checkpoint_exclude_paths: Vec<String>,
    /// Include ignored/untracked files in checkpoints (usually false).
    #[serde(default)]
    pub checkpoint_include_ignored: bool,
    /// Scan captured content for likely secrets before committing.
    #[serde(default = "default_true")]
    pub checkpoint_secret_scan: bool,
    /// Risky checkpoints require explicit acknowledgment instead of being
    /// blocked outright.
    #[serde(default)]
    pub checkpoint_allow_risky_with_ack: bool,
}

fn default_true() -> bool {
    true
}

impl Default for WorkPolicy {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            denied_paths: Vec::new(),
            protected_refs: Vec::new(),
            checkpoint_capture_all: true,
            checkpoint_max_file_bytes: 0,
            checkpoint_max_total_bytes: 0,
            checkpoint_exclude_paths: Vec::new(),
            checkpoint_include_ignored: false,
            checkpoint_secret_scan: true,
            checkpoint_allow_risky_with_ack: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    /// Normalized git path.
    pub path: String,
    pub status: ChangeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(default)]
    pub is_binary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    /// Unmerged / conflicted path (`git status` porcelain `u`).
    Unmerged,
}
