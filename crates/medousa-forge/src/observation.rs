//! Generation-fenced workspace observation (H06.5).
//!
//! Exactness is capture → observe → recheck. The watcher is a hint, never
//! sole post-restart proof.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::execution::{
    MAX_OBSERVATION_CACHE_BYTES, MAX_OBSERVATION_CACHE_ENTRIES, OBSERVATION_CACHE_TTL,
};
use crate::git::GitEngine;
use crate::model::{GitOid, WorkId};

pub const RESUME_UNTRACKED_ENTRY_LIMIT: usize = 100_000;
pub const RESUME_PER_FILE_BYTES: u64 = 1024 * 1024 * 1024;
pub const RESUME_AGGREGATE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const RESUME_WALL_TIME: Duration = Duration::from_secs(30);
pub const ORDINARY_AGGREGATE_BYTES: u64 = 512 * 1024 * 1024;
pub const ORDINARY_WALL_TIME: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationCompleteness {
    Exact,
    ConservativelyDirty,
    Incomplete,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationCapture {
    pub workspace_generation: u64,
    pub watcher_generation: u64,
    pub repository_generation: u64,
    pub watcher_overflow: bool,
}

pub trait GenerationSource {
    fn capture(&self) -> GenerationCapture;
}

impl GenerationSource for GenerationCapture {
    fn capture(&self) -> GenerationCapture {
        self.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceObservation {
    pub observation_generation: u64,
    pub work_id: WorkId,
    pub worktree: PathBuf,
    pub head_oid: Option<String>,
    pub branch: Option<String>,
    pub dirty_digest: String,
    pub changed_paths: Vec<String>,
    pub completeness: ObservationCompleteness,
    pub limits_hit: Vec<String>,
    pub capture: GenerationCapture,
}

#[derive(Debug, Clone)]
struct CachedDigest {
    digest: String,
    bytes: u64,
    stored_at: Instant,
    workspace_generation: u64,
}

pub struct ObservationCache {
    entries: Mutex<HashMap<PathBuf, CachedDigest>>,
    bytes: Mutex<u64>,
}

impl Default for ObservationCache {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            bytes: Mutex::new(0),
        }
    }
}

impl ObservationCache {
    pub fn invalidate(&self, workspace_generation: u64) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, cached| cached.workspace_generation == workspace_generation);
        }
    }

    fn get(&self, path: &Path, workspace_generation: u64) -> Option<String> {
        let entries = self.entries.lock().ok()?;
        let cached = entries.get(path)?;
        if cached.workspace_generation != workspace_generation {
            return None;
        }
        if cached.stored_at.elapsed() > OBSERVATION_CACHE_TTL {
            return None;
        }
        Some(cached.digest.clone())
    }

    fn insert(&self, path: PathBuf, digest: String, bytes: u64, workspace_generation: u64) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let Ok(mut total) = self.bytes.lock() else {
            return;
        };
        while (entries.len() >= MAX_OBSERVATION_CACHE_ENTRIES
            || *total + bytes > MAX_OBSERVATION_CACHE_BYTES as u64)
            && !entries.is_empty()
        {
            if let Some(key) = entries.keys().next().cloned() {
                if let Some(old) = entries.remove(&key) {
                    *total = total.saturating_sub(old.bytes);
                }
            } else {
                break;
            }
        }
        *total = total.saturating_add(bytes);
        entries.insert(
            path,
            CachedDigest {
                digest,
                bytes,
                stored_at: Instant::now(),
                workspace_generation,
            },
        );
    }
}

pub struct WorkspaceObserver {
    cache: ObservationCache,
    next_generation: Mutex<u64>,
}

impl Default for WorkspaceObserver {
    fn default() -> Self {
        Self {
            cache: ObservationCache::default(),
            next_generation: Mutex::new(1),
        }
    }
}

impl WorkspaceObserver {
    pub fn observe_exact(
        &self,
        git: &GitEngine,
        work_id: &WorkId,
        worktree: &Path,
        source: &impl GenerationSource,
        resume: bool,
    ) -> Result<WorkspaceObservation> {
        let capture = source.capture();
        if capture.watcher_overflow {
            return Ok(self.unknown(work_id, worktree, capture, vec!["watcher_overflow".into()]));
        }
        let started = Instant::now();
        let wall = if resume {
            RESUME_WALL_TIME
        } else {
            ORDINARY_WALL_TIME
        };
        let aggregate = if resume {
            RESUME_AGGREGATE_BYTES
        } else {
            ORDINARY_AGGREGATE_BYTES
        };
        let observation = self.observe_once(git, work_id, worktree, &capture, aggregate, wall)?;
        if started.elapsed() > wall {
            return Ok(self.incomplete(
                work_id,
                worktree,
                capture,
                vec!["wall_time".into()],
                observation.dirty_digest,
            ));
        }
        let recheck = source.capture();
        if recheck != capture || recheck.watcher_overflow {
            return Ok(self.unknown(
                work_id,
                worktree,
                capture,
                vec!["generation_changed".into()],
            ));
        }
        Ok(observation)
    }

    fn observe_once(
        &self,
        git: &GitEngine,
        work_id: &WorkId,
        worktree: &Path,
        capture: &GenerationCapture,
        aggregate: u64,
        wall: Duration,
    ) -> Result<WorkspaceObservation> {
        let started = Instant::now();
        let branch = git.current_branch(worktree).ok().flatten();
        let head = git.head_oid(worktree).ok().map(|oid| oid.as_str().to_owned());
        let status = git.status_porcelain(worktree)?;
        let mut hasher = Sha256::new();
        let mut changed = Vec::new();
        let mut hashed = 0u64;
        let mut limits = Vec::new();
        let mut untracked = 0usize;
        for entry in &status {
            changed.push(entry.path.clone());
            hasher.update(entry.path.as_bytes());
            hasher.update(format!("{:?}", entry.kind).as_bytes());
            if matches!(
                entry.kind,
                crate::git::PorcelainKind::Untracked | crate::git::PorcelainKind::Ignored
            ) {
                untracked += 1;
                if untracked > RESUME_UNTRACKED_ENTRY_LIMIT {
                    limits.push("untracked_entries".into());
                    break;
                }
                let path = worktree.join(&entry.path);
                if let Some(digest) = self.cache.get(&path, capture.workspace_generation) {
                    hasher.update(digest.as_bytes());
                } else {
                    match hash_untracked(worktree, &entry.path, RESUME_PER_FILE_BYTES) {
                        Ok((digest, bytes)) => {
                            hashed = hashed.saturating_add(bytes);
                            hasher.update(digest.as_bytes());
                            self.cache.insert(
                                path,
                                digest,
                                bytes,
                                capture.workspace_generation,
                            );
                            if hashed > aggregate {
                                limits.push("aggregate_bytes".into());
                                break;
                            }
                        }
                        Err(_) => limits.push("untracked_hash".into()),
                    }
                }
            }
            if started.elapsed() > wall {
                limits.push("wall_time".into());
                break;
            }
        }
        if let Some(head_oid) = head.as_ref() {
            if let Ok(streamed) = stream_diff_digest(git, worktree, head_oid) {
                hasher.update(streamed.as_bytes());
            } else {
                limits.push("diff_stream".into());
            }
        }
        let completeness = if limits.is_empty() {
            ObservationCompleteness::Exact
        } else {
            ObservationCompleteness::Incomplete
        };
        let generation = {
            let mut next = self
                .next_generation
                .lock()
                .map_err(|_| ForgeError::Store("observation generation poisoned".into()))?;
            let value = *next;
            *next = next.saturating_add(1);
            value
        };
        Ok(WorkspaceObservation {
            observation_generation: generation,
            work_id: work_id.clone(),
            worktree: worktree.to_path_buf(),
            head_oid: head,
            branch,
            dirty_digest: format!("{:x}", hasher.finalize()),
            changed_paths: changed,
            completeness,
            limits_hit: limits,
            capture: capture.clone(),
        })
    }

    fn incomplete(
        &self,
        work_id: &WorkId,
        worktree: &Path,
        capture: GenerationCapture,
        limits: Vec<String>,
        digest: String,
    ) -> WorkspaceObservation {
        WorkspaceObservation {
            observation_generation: 0,
            work_id: work_id.clone(),
            worktree: worktree.to_path_buf(),
            head_oid: None,
            branch: None,
            dirty_digest: digest,
            changed_paths: Vec::new(),
            completeness: ObservationCompleteness::Incomplete,
            limits_hit: limits,
            capture,
        }
    }

    fn unknown(
        &self,
        work_id: &WorkId,
        worktree: &Path,
        capture: GenerationCapture,
        limits: Vec<String>,
    ) -> WorkspaceObservation {
        WorkspaceObservation {
            observation_generation: 0,
            work_id: work_id.clone(),
            worktree: worktree.to_path_buf(),
            head_oid: None,
            branch: None,
            dirty_digest: String::new(),
            changed_paths: Vec::new(),
            completeness: ObservationCompleteness::Unknown,
            limits_hit: limits,
            capture,
        }
    }
}

fn hash_untracked(worktree: &Path, relative: &str, max_bytes: u64) -> Result<(String, u64)> {
    let path = worktree.join(relative);
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ForgeError::Store("untracked path is not a regular file".into()));
    }
    if metadata.len() > max_bytes {
        return Err(ForgeError::Store("untracked file exceeds per-file budget".into()));
    }
    let mut file = std::fs::File::open(&path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > max_bytes {
            return Err(ForgeError::Store("untracked file exceeded hash budget".into()));
        }
        hasher.update(&buf[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}

fn stream_diff_digest(git: &GitEngine, worktree: &Path, head: &str) -> Result<String> {
    let bytes = git.diff_binary_worktree_bounded(worktree, &GitOid::new(head.to_owned()), 8 * 1024 * 1024)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WorkId;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct FlipSource {
        first: GenerationCapture,
        second: GenerationCapture,
        calls: AtomicU64,
    }

    impl GenerationSource for FlipSource {
        fn capture(&self) -> GenerationCapture {
            if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
                self.first.clone()
            } else {
                self.second.clone()
            }
        }
    }

    #[test]
    fn overflow_never_publishes_exact() {
        let observer = WorkspaceObserver::default();
        let git = GitEngine::detect().unwrap();
        let observation = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-test".to_string()),
                Path::new("."),
                &GenerationCapture {
                    workspace_generation: 1,
                    watcher_generation: 1,
                    repository_generation: 1,
                    watcher_overflow: true,
                },
                false,
            )
            .unwrap();
        assert_eq!(observation.completeness, ObservationCompleteness::Unknown);
    }

    #[test]
    fn cm012_generation_change_during_observe_is_unknown() {
        let observer = WorkspaceObserver::default();
        let git = GitEngine::detect().unwrap();
        let observation = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-cm012".to_string()),
                Path::new("."),
                &FlipSource {
                    first: GenerationCapture {
                        workspace_generation: 1,
                        watcher_generation: 1,
                        repository_generation: 1,
                        watcher_overflow: false,
                    },
                    second: GenerationCapture {
                        workspace_generation: 2,
                        watcher_generation: 1,
                        repository_generation: 1,
                        watcher_overflow: false,
                    },
                    calls: AtomicU64::new(0),
                },
                false,
            )
            .unwrap();
        assert_eq!(observation.completeness, ObservationCompleteness::Unknown);
        assert!(observation.limits_hit.contains(&"generation_changed".into()));
    }
}
