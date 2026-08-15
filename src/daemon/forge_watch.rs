//! Debounced Forge worktree observation for project event streams.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use medousa_forge::execution::{ExecutionClass, ForgeExecutionService};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::daemon::forge_events::{ForgeEventBus, ForgeProjectEventKind};

fn relative_worktree_path(worktree: &Path, path: &Path) -> Option<String> {
    let path = path
        .canonicalize()
        .ok()
        .unwrap_or_else(|| path.to_path_buf());
    let worktree = worktree
        .canonicalize()
        .ok()
        .unwrap_or_else(|| worktree.to_path_buf());
    let relative = path.strip_prefix(&worktree).ok()?;
    if relative.components().any(|part| {
        let raw = part.as_os_str();
        raw == ".git" || raw == "node_modules" || raw == "target"
    }) {
        return None;
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn event_kind(kind: &EventKind) -> Option<ForgeProjectEventKind> {
    match kind {
        EventKind::Create(_) => Some(ForgeProjectEventKind::Created),
        EventKind::Modify(_) => Some(ForgeProjectEventKind::Changed),
        EventKind::Remove(_) => Some(ForgeProjectEventKind::Deleted),
        _ => None,
    }
}

fn file_digest(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    use sha2::{Digest, Sha256};
    Some(format!("{:x}", Sha256::digest(bytes)))
}

/// Watch remembered Forge worktrees and publish path-aware project events.
pub fn spawn_forge_worktree_watcher(bus: ForgeEventBus, execution: Arc<ForgeExecutionService>) {
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, PathBuf, ForgeProjectEventKind)>();
        let mut watchers: HashMap<String, RecommendedWatcher> = HashMap::new();
        let mut refresh = tokio::time::interval(Duration::from_secs(2));
        let mut flush = tokio::time::interval(Duration::from_millis(250));
        let mut pending: HashMap<(String, String), ForgeProjectEventKind> = HashMap::new();

        loop {
            tokio::select! {
                _ = refresh.tick() => {
                    let tracked = bus.tracked_worktrees();
                    let live: HashSet<String> = tracked.iter().map(|(id, _)| id.clone()).collect();
                    watchers.retain(|id, _| live.contains(id));
                    for (work_id, worktree) in tracked {
                        if watchers.contains_key(&work_id) || !worktree.is_dir() {
                            continue;
                        }
                        let tx = tx.clone();
                        let overflow_bus = bus.clone();
                        let watched_id = work_id.clone();
                        match notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
                            match result {
                                Ok(event) => {
                                    if event.need_rescan() {
                                        overflow_bus.mark_watcher_overflow();
                                        return;
                                    }
                                    let Some(kind) = event_kind(&event.kind) else { return };
                                    for path in event.paths {
                                        let _ = tx.send((watched_id.clone(), path, kind));
                                    }
                                }
                                Err(err) => {
                                    // Watcher loss / inotify exhaustion / backend errors are
                                    // never proof of cleanliness — mark overflow conservatively.
                                    overflow_bus.mark_watcher_overflow();
                                    tracing::warn!(
                                        error = %err,
                                        work_id = %watched_id,
                                        "forge worktree watcher error; marking overflow"
                                    );
                                }
                            }
                        }) {
                            Ok(mut watcher) => {
                                if watcher.watch(&worktree, RecursiveMode::Recursive).is_ok() {
                                    watchers.insert(work_id, watcher);
                                } else {
                                    bus.mark_watcher_overflow();
                                    tracing::warn!(%work_id, "forge worktree watch failed; marking overflow");
                                }
                            }
                            Err(err) => {
                                bus.mark_watcher_overflow();
                                tracing::warn!(error = %err, %work_id, "forge worktree watcher unavailable");
                            }
                        }
                    }
                }
                message = rx.recv() => {
                    let Some((work_id, path, kind)) = message else { break };
                    let Some(worktree) = bus.worktree_for(&work_id) else { continue };
                    let Some(relative) = relative_worktree_path(&worktree, &path) else { continue };
                    pending.insert((work_id, relative), kind);
                }
                _ = flush.tick() => {
                    if pending.is_empty() {
                        continue;
                    }
                    let batch = std::mem::take(&mut pending);
                    for ((work_id, path), kind) in batch {
                        let digest = if kind == ForgeProjectEventKind::Deleted {
                            None
                        } else {
                            let absolute = bus.worktree_for(&work_id).map(|root| root.join(&path));
                            match absolute {
                                Some(absolute) => match execution
                                    .run(
                                        ExecutionClass::Observation,
                                        64 * 1024,
                                        move || Ok::<_, medousa_forge::error::ForgeError>(file_digest(&absolute)),
                                    )
                                    .await
                                {
                                    Ok(digest) => digest,
                                    Err(_) => None,
                                },
                                None => None,
                            }
                        };
                        bus.bump_watcher_generation();
                        bus.publish_project(
                            &work_id,
                            kind,
                            Some(path),
                            None,
                            digest,
                        );
                    }
                }
            }
        }
    });
}

/// Test helper: publish a synthetic FS-originated change through the bus.
pub fn publish_observed_change(bus: &ForgeEventBus, work_id: &str, relative: &str) {
    bus.bump_watcher_generation();
    bus.publish_project(
        work_id,
        ForgeProjectEventKind::Changed,
        Some(relative.to_owned()),
        None,
        None,
    );
}

/// Test helper: simulate watcher overflow from a lost/rescan event.
pub fn publish_watcher_overflow(bus: &ForgeEventBus) {
    bus.mark_watcher_overflow();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_ignore_git_metadata() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("src/main.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "fn main() {}").unwrap();
        assert_eq!(
            relative_worktree_path(root.path(), &file).as_deref(),
            Some("src/main.rs")
        );
        let git = root.path().join(".git/HEAD");
        std::fs::create_dir_all(git.parent().unwrap()).unwrap();
        std::fs::write(&git, "ref").unwrap();
        assert!(relative_worktree_path(root.path(), &git).is_none());
    }

    #[test]
    fn watcher_overflow_helper_marks_shared_fence() {
        let bus = ForgeEventBus::new();
        let before = bus.watcher_generation();
        publish_watcher_overflow(&bus);
        assert!(bus.watcher_overflow());
        assert!(bus.watcher_generation() > before);
    }

    #[test]
    fn observed_change_bumps_watcher_generation() {
        let bus = ForgeEventBus::new();
        let before = bus.watcher_generation();
        publish_observed_change(&bus, "work-1", "src/a.rs");
        assert!(bus.watcher_generation() > before);
    }
}
