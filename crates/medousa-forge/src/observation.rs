//! Generation-fenced workspace observation (H06.5 / H06.9).
//!
//! Exactness is capture → observe → recheck. The watcher is a hint, never
//! sole post-restart proof. Truncated or incomplete Git/file results never
//! publish [`ObservationCompleteness::Exact`].

use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::error::{ForgeError, Result};
use crate::execution::{
    MAX_OBSERVATION_CACHE_BYTES, MAX_OBSERVATION_CACHE_ENTRIES, OBSERVATION_CACHE_TTL,
};
use crate::git::GitEngine;
use crate::model::WorkId;

pub const RESUME_UNTRACKED_ENTRY_LIMIT: usize = 100_000;
pub const RESUME_PER_FILE_BYTES: u64 = 1024 * 1024 * 1024;
pub const RESUME_AGGREGATE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const RESUME_WALL_TIME: Duration = Duration::from_secs(30);
pub const ORDINARY_AGGREGATE_BYTES: u64 = 512 * 1024 * 1024;
pub const ORDINARY_WALL_TIME: Duration = Duration::from_secs(5);
pub const DEFAULT_DIFF_HASH_BYTES: u64 = 8 * 1024 * 1024;

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

/// Shared watcher generation/overflow fence used by the daemon event bus and
/// Forge observation. Overflow is sticky until explicitly cleared.
#[derive(Debug, Clone)]
pub struct SharedWatcherFence {
    generation: Arc<AtomicU64>,
    overflow: Arc<AtomicBool>,
}

impl Default for SharedWatcherFence {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedWatcherFence {
    pub fn new() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(1)),
            overflow: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn overflow(&self) -> bool {
        self.overflow.load(Ordering::SeqCst)
    }

    pub fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn mark_overflow(&self) {
        self.overflow.store(true, Ordering::SeqCst);
        self.bump_generation();
    }

    pub fn clear_overflow(&self) {
        self.overflow.store(false, Ordering::SeqCst);
    }

    pub fn bind(
        &self,
        workspace_generation: u64,
        repository_generation: u64,
    ) -> BoundWatcherSource {
        BoundWatcherSource {
            fence: self.clone(),
            workspace_generation,
            repository_generation,
        }
    }
}

/// [`GenerationSource`] that reads live watcher fence state.
#[derive(Debug, Clone)]
pub struct BoundWatcherSource {
    fence: SharedWatcherFence,
    workspace_generation: u64,
    repository_generation: u64,
}

impl GenerationSource for BoundWatcherSource {
    fn capture(&self) -> GenerationCapture {
        GenerationCapture {
            workspace_generation: self.workspace_generation,
            watcher_generation: self.fence.generation(),
            repository_generation: self.repository_generation,
            watcher_overflow: self.fence.overflow(),
        }
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

#[derive(Debug, Clone, Copy)]
pub struct ObservationBudgets {
    pub aggregate_bytes: u64,
    pub per_file_bytes: u64,
    pub wall: Duration,
    pub untracked_entries: usize,
    pub diff_bytes: u64,
}

impl ObservationBudgets {
    pub fn for_resume(resume: bool) -> Self {
        if resume {
            Self {
                aggregate_bytes: RESUME_AGGREGATE_BYTES,
                per_file_bytes: RESUME_PER_FILE_BYTES,
                wall: RESUME_WALL_TIME,
                untracked_entries: RESUME_UNTRACKED_ENTRY_LIMIT,
                diff_bytes: DEFAULT_DIFF_HASH_BYTES,
            }
        } else {
            Self {
                aggregate_bytes: ORDINARY_AGGREGATE_BYTES,
                per_file_bytes: RESUME_PER_FILE_BYTES,
                wall: ORDINARY_WALL_TIME,
                untracked_entries: RESUME_UNTRACKED_ENTRY_LIMIT,
                diff_bytes: DEFAULT_DIFF_HASH_BYTES,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    device: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &std::fs::Metadata) -> Result<Self> {
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let duration = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        Ok(Self {
            len: metadata.len(),
            modified_secs: duration.as_secs(),
            modified_nanos: duration.subsec_nanos(),
            #[cfg(unix)]
            inode: std::os::unix::fs::MetadataExt::ino(metadata),
            #[cfg(unix)]
            device: std::os::unix::fs::MetadataExt::dev(metadata),
        })
    }
}

#[derive(Debug, Clone)]
struct CachedDigest {
    digest: String,
    bytes: u64,
    stored_at: Instant,
    workspace_generation: u64,
    identity: FileIdentity,
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
        if let Ok(mut total) = self.bytes.lock() {
            *total = self
                .entries
                .lock()
                .map(|entries| entries.values().map(|cached| cached.bytes).sum())
                .unwrap_or(0);
        }
    }

    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(
        &self,
        path: &Path,
        workspace_generation: u64,
        identity: &FileIdentity,
    ) -> Option<String> {
        let entries = self.entries.lock().ok()?;
        let cached = entries.get(path)?;
        if cached.workspace_generation != workspace_generation {
            return None;
        }
        if &cached.identity != identity {
            return None;
        }
        if cached.stored_at.elapsed() > OBSERVATION_CACHE_TTL {
            return None;
        }
        Some(cached.digest.clone())
    }

    fn insert(
        &self,
        path: PathBuf,
        digest: String,
        bytes: u64,
        workspace_generation: u64,
        identity: FileIdentity,
    ) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let Ok(mut total) = self.bytes.lock() else {
            return;
        };
        if let Some(old) = entries.remove(&path) {
            *total = total.saturating_sub(old.bytes);
        }
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
                identity,
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
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn invalidate_cache(&self, workspace_generation: u64) {
        self.cache.invalidate(workspace_generation);
    }

    pub fn observe_exact(
        &self,
        git: &GitEngine,
        work_id: &WorkId,
        worktree: &Path,
        source: &impl GenerationSource,
        resume: bool,
    ) -> Result<WorkspaceObservation> {
        self.observe_with_budgets(
            git,
            work_id,
            worktree,
            source,
            ObservationBudgets::for_resume(resume),
        )
    }

    pub fn observe_with_budgets(
        &self,
        git: &GitEngine,
        work_id: &WorkId,
        worktree: &Path,
        source: &impl GenerationSource,
        budgets: ObservationBudgets,
    ) -> Result<WorkspaceObservation> {
        let capture = source.capture();
        if capture.watcher_overflow {
            return Ok(self.unknown(work_id, worktree, capture, vec!["watcher_overflow".into()]));
        }
        let started = Instant::now();
        let observation = self.observe_once(git, work_id, worktree, &capture, budgets, started)?;
        if started.elapsed() > budgets.wall {
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
        budgets: ObservationBudgets,
        started: Instant,
    ) -> Result<WorkspaceObservation> {
        let branch = git.current_branch(worktree).ok().flatten();
        let head = git
            .head_oid(worktree)
            .ok()
            .map(|oid| oid.as_str().to_owned());
        let status = match git.status_porcelain(worktree) {
            Ok(status) => status,
            Err(_) => {
                return Ok(self.incomplete(
                    work_id,
                    worktree,
                    capture.clone(),
                    vec!["git_status".into()],
                    String::new(),
                ));
            }
        };
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
                if untracked > budgets.untracked_entries {
                    limits.push("untracked_entries".into());
                    break;
                }
                if started.elapsed() > budgets.wall {
                    limits.push("wall_time".into());
                    break;
                }
                let remaining = budgets.aggregate_bytes.saturating_sub(hashed);
                let per_file = budgets.per_file_bytes.min(remaining);
                if per_file == 0 {
                    limits.push("aggregate_bytes".into());
                    break;
                }
                match hash_untracked_rooted(
                    worktree,
                    &entry.path,
                    per_file,
                    budgets.wall.saturating_sub(started.elapsed()),
                    &self.cache,
                    capture.workspace_generation,
                ) {
                    Ok(HashOutcome::Cached(digest)) => {
                        hasher.update(digest.as_bytes());
                    }
                    Ok(HashOutcome::Fresh { digest, bytes }) => {
                        hashed = hashed.saturating_add(bytes);
                        hasher.update(digest.as_bytes());
                        if hashed > budgets.aggregate_bytes {
                            limits.push("aggregate_bytes".into());
                            break;
                        }
                    }
                    Err(HashLimit::Aggregate) => {
                        limits.push("aggregate_bytes".into());
                        break;
                    }
                    Err(HashLimit::PerFile) => {
                        limits.push("per_file_bytes".into());
                        break;
                    }
                    Err(HashLimit::WallTime) => {
                        limits.push("wall_time".into());
                        break;
                    }
                    Err(HashLimit::UnsafePath) => {
                        limits.push("unsafe_path".into());
                        break;
                    }
                    Err(HashLimit::Io) => {
                        limits.push("untracked_hash".into());
                        break;
                    }
                }
            }
            if started.elapsed() > budgets.wall {
                limits.push("wall_time".into());
                break;
            }
        }
        if let Some(head_oid) = head.as_ref() {
            match git.hash_diff_binary_worktree_streaming(
                worktree,
                &crate::model::GitOid::new(head_oid.clone()),
                budgets.diff_bytes,
            ) {
                Ok(digest) if digest.truncated => {
                    hasher.update(digest.digest.as_bytes());
                    limits.push("diff_truncated".into());
                }
                Ok(digest) => {
                    hasher.update(digest.digest.as_bytes());
                }
                Err(_) => limits.push("diff_stream".into()),
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

enum HashOutcome {
    Cached(String),
    Fresh { digest: String, bytes: u64 },
}

enum HashLimit {
    Aggregate,
    PerFile,
    WallTime,
    UnsafePath,
    Io,
}

fn hash_untracked_rooted(
    worktree: &Path,
    relative: &str,
    max_bytes: u64,
    wall: Duration,
    cache: &ObservationCache,
    workspace_generation: u64,
) -> std::result::Result<HashOutcome, HashLimit> {
    let path =
        resolve_rooted_regular_file(worktree, relative).map_err(|_| HashLimit::UnsafePath)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| HashLimit::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(HashLimit::UnsafePath);
    }
    let identity = FileIdentity::from_metadata(&metadata).map_err(|_| HashLimit::Io)?;
    if let Some(digest) = cache.get(&path, workspace_generation, &identity) {
        return Ok(HashOutcome::Cached(digest));
    }
    if identity.len > max_bytes {
        return Err(HashLimit::PerFile);
    }
    let deadline = Instant::now() + wall;
    let mut file = std::fs::File::open(&path).map_err(|_| HashLimit::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        if Instant::now() >= deadline {
            return Err(HashLimit::WallTime);
        }
        let read = file.read(&mut buf).map_err(|_| HashLimit::Io)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > max_bytes {
            return Err(if max_bytes == 0 {
                HashLimit::Aggregate
            } else {
                HashLimit::PerFile
            });
        }
        hasher.update(&buf[..read]);
    }
    let digest = format!("{:x}", hasher.finalize());
    cache.insert(path, digest.clone(), total, workspace_generation, identity);
    Ok(HashOutcome::Fresh {
        digest,
        bytes: total,
    })
}

/// Resolve `relative` under `worktree` without following symlink escapes.
pub fn resolve_rooted_regular_file(worktree: &Path, relative: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    if relative.is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ForgeError::Store(format!(
            "unsafe observation path: {relative}"
        )));
    }
    let mut cursor = worktree.to_path_buf();
    let components: Vec<_> = relative_path.components().collect();
    for (index, component) in components.iter().enumerate() {
        cursor.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&cursor).map_err(ForgeError::Io)?;
        let is_last = index + 1 == components.len();
        if metadata.file_type().is_symlink() {
            return Err(ForgeError::Store(format!(
                "observation path crosses symlink: {}",
                cursor.display()
            )));
        }
        if is_last {
            if !metadata.is_file() {
                return Err(ForgeError::Store(format!(
                    "observation path is not a regular file: {}",
                    cursor.display()
                )));
            }
        } else if !metadata.is_dir() {
            return Err(ForgeError::Store(format!(
                "observation path parent is not a directory: {}",
                cursor.display()
            )));
        }
    }
    Ok(cursor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::CheckpointAuthor;
    use crate::model::WorkId;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use tempfile::TempDir;

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

    fn init_repo() -> (TempDir, GitEngine) {
        let tmp = TempDir::new().unwrap();
        let git = GitEngine::detect().unwrap();
        git.run(tmp.path(), &["init", "-b", "main"]).unwrap();
        fs::write(tmp.path().join("tracked.txt"), "tracked\n").unwrap();
        git.run(tmp.path(), &["add", "-A"]).unwrap();
        git.commit_checkpoint(tmp.path(), "initial", &CheckpointAuthor::default())
            .unwrap();
        (tmp, git)
    }

    fn capture(generation: u64) -> GenerationCapture {
        GenerationCapture {
            workspace_generation: generation,
            watcher_generation: generation,
            repository_generation: generation,
            watcher_overflow: false,
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
    fn observe_marks_unknown_when_generation_capture_changes_mid_scan() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        let observation = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-cm012".to_string()),
                tmp.path(),
                &FlipSource {
                    first: capture(1),
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
        assert!(
            observation
                .limits_hit
                .contains(&"generation_changed".into())
        );
    }

    #[test]
    fn concurrent_mutation_via_watcher_fence_marks_unknown() {
        let fence = SharedWatcherFence::new();
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        let source = fence.bind(1, 1);
        let fence_for_bump = fence.clone();
        let path = tmp.path().join("race.txt");
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            fs::write(&path, "mutated").unwrap();
            fence_for_bump.bump_generation();
        });
        // Give the writer a moment, then observe with a source that will flip if bumped.
        thread::sleep(Duration::from_millis(1));
        let before = source.capture();
        let _ = handle.join();
        let after = source.capture();
        assert_ne!(before.watcher_generation, after.watcher_generation);
        let observation = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-race".to_string()),
                tmp.path(),
                &FlipSource {
                    first: before,
                    second: after,
                    calls: AtomicU64::new(0),
                },
                false,
            )
            .unwrap();
        assert_eq!(observation.completeness, ObservationCompleteness::Unknown);
    }

    #[test]
    fn watcher_overflow_from_shared_fence_is_unknown() {
        let fence = SharedWatcherFence::new();
        fence.mark_overflow();
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        let observation = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-overflow".to_string()),
                tmp.path(),
                &fence.bind(1, 1),
                false,
            )
            .unwrap();
        assert_eq!(observation.completeness, ObservationCompleteness::Unknown);
        assert!(observation.limits_hit.contains(&"watcher_overflow".into()));
    }

    #[test]
    fn large_diff_truncation_never_publishes_exact() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        fs::write(tmp.path().join("tracked.txt"), "x".repeat(64 * 1024)).unwrap();
        let budgets = ObservationBudgets {
            aggregate_bytes: ORDINARY_AGGREGATE_BYTES,
            per_file_bytes: RESUME_PER_FILE_BYTES,
            wall: ORDINARY_WALL_TIME,
            untracked_entries: RESUME_UNTRACKED_ENTRY_LIMIT,
            diff_bytes: 64,
        };
        let observation = observer
            .observe_with_budgets(
                &git,
                &WorkId::from("work1-diff".to_string()),
                tmp.path(),
                &capture(1),
                budgets,
            )
            .unwrap();
        assert_eq!(
            observation.completeness,
            ObservationCompleteness::Incomplete
        );
        assert!(observation.limits_hit.contains(&"diff_truncated".into()));
    }

    #[test]
    fn large_file_budget_marks_incomplete() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        fs::write(tmp.path().join("big.bin"), vec![1u8; 8 * 1024]).unwrap();
        let budgets = ObservationBudgets {
            aggregate_bytes: 1024 * 1024,
            per_file_bytes: 1024,
            wall: ORDINARY_WALL_TIME,
            untracked_entries: RESUME_UNTRACKED_ENTRY_LIMIT,
            diff_bytes: DEFAULT_DIFF_HASH_BYTES,
        };
        let observation = observer
            .observe_with_budgets(
                &git,
                &WorkId::from("work1-big".to_string()),
                tmp.path(),
                &capture(1),
                budgets,
            )
            .unwrap();
        assert_eq!(
            observation.completeness,
            ObservationCompleteness::Incomplete
        );
        assert!(observation.limits_hit.contains(&"per_file_bytes".into()));
    }

    #[test]
    fn aggregate_budget_exhaustion_marks_incomplete() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        fs::write(tmp.path().join("a.bin"), vec![2u8; 1500]).unwrap();
        fs::write(tmp.path().join("b.bin"), vec![3u8; 1500]).unwrap();
        let budgets = ObservationBudgets {
            aggregate_bytes: 2000,
            per_file_bytes: 10_000,
            wall: ORDINARY_WALL_TIME,
            untracked_entries: RESUME_UNTRACKED_ENTRY_LIMIT,
            diff_bytes: DEFAULT_DIFF_HASH_BYTES,
        };
        let observation = observer
            .observe_with_budgets(
                &git,
                &WorkId::from("work1-agg".to_string()),
                tmp.path(),
                &capture(1),
                budgets,
            )
            .unwrap();
        assert_eq!(
            observation.completeness,
            ObservationCompleteness::Incomplete
        );
        assert!(
            observation.limits_hit.contains(&"aggregate_bytes".into())
                || observation.limits_hit.contains(&"per_file_bytes".into())
        );
    }

    #[test]
    fn cache_invalidates_when_file_identity_changes() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        let path = tmp.path().join("cache-me.txt");
        fs::write(&path, "one").unwrap();
        let first = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-cache".to_string()),
                tmp.path(),
                &capture(1),
                false,
            )
            .unwrap();
        assert_eq!(first.completeness, ObservationCompleteness::Exact);
        assert!(observer.cache_len() >= 1);
        fs::write(&path, "two").unwrap();
        let second = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-cache".to_string()),
                tmp.path(),
                &capture(1),
                false,
            )
            .unwrap();
        assert_eq!(second.completeness, ObservationCompleteness::Exact);
        assert_ne!(first.dirty_digest, second.dirty_digest);
        observer.invalidate_cache(2);
        assert_eq!(observer.cache_len(), 0);
    }

    #[test]
    fn wall_budget_during_file_hash_marks_incomplete() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        fs::write(tmp.path().join("slow.bin"), vec![9u8; 256 * 1024]).unwrap();
        let budgets = ObservationBudgets {
            aggregate_bytes: ORDINARY_AGGREGATE_BYTES,
            per_file_bytes: RESUME_PER_FILE_BYTES,
            wall: Duration::from_nanos(1),
            untracked_entries: RESUME_UNTRACKED_ENTRY_LIMIT,
            diff_bytes: DEFAULT_DIFF_HASH_BYTES,
        };
        let observation = observer
            .observe_with_budgets(
                &git,
                &WorkId::from("work1-wall".to_string()),
                tmp.path(),
                &capture(1),
                budgets,
            )
            .unwrap();
        assert_ne!(observation.completeness, ObservationCompleteness::Exact);
        assert!(
            observation.limits_hit.contains(&"wall_time".into())
                || observation.completeness == ObservationCompleteness::Incomplete
        );
    }

    #[test]
    fn symlink_escape_is_rejected_as_incomplete() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        let outside = TempDir::new().unwrap();
        fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), tmp.path().join("escape")).unwrap();
            // Porcelain may or may not list the symlink dir contents; force the helper.
            let err = resolve_rooted_regular_file(tmp.path(), "escape/secret.txt");
            assert!(err.is_err());
            fs::write(tmp.path().join("link-file"), "x").unwrap();
            std::os::unix::fs::symlink("link-file", tmp.path().join("sym")).unwrap();
            let observation = observer
                .observe_exact(
                    &git,
                    &WorkId::from("work1-symlink".to_string()),
                    tmp.path(),
                    &capture(1),
                    false,
                )
                .unwrap();
            // Symlink leaf must not be hashed as a regular file; if listed, incomplete.
            if observation
                .changed_paths
                .iter()
                .any(|path| path == "sym" || path.starts_with("escape/"))
            {
                assert_ne!(observation.completeness, ObservationCompleteness::Exact);
            }
        }
    }

    #[test]
    fn shared_observer_reuses_cache_across_captures() {
        let observer = WorkspaceObserver::default();
        let (tmp, git) = init_repo();
        fs::write(tmp.path().join("cached.txt"), "payload").unwrap();
        let first = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-shared".to_string()),
                tmp.path(),
                &capture(1),
                false,
            )
            .unwrap();
        let second = observer
            .observe_exact(
                &git,
                &WorkId::from("work1-shared".to_string()),
                tmp.path(),
                &capture(1),
                false,
            )
            .unwrap();
        assert_eq!(first.completeness, ObservationCompleteness::Exact);
        assert_eq!(second.completeness, ObservationCompleteness::Exact);
        assert_eq!(first.dirty_digest, second.dirty_digest);
        assert!(observer.cache_len() >= 1);
    }
}
