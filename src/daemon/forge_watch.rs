//! Debounced Forge worktree observation for project event streams.

//! Debounced Forge worktree observation for project event streams.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::daemon::forge_events::{ForgeEventBus, ForgeProjectEventKind};

fn relative_worktree_path(worktree: &Path, path: &Path) -> Option<String> {
    let path = path.canonicalize().ok().unwrap_or_else(|| path.to_path_buf());
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

/// Watch remembered Forge worktrees and publish path-aware project events.
pub fn spawn_forge_worktree_watcher(bus: ForgeEventBus) {
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
                        let watched_id = work_id.clone();
                        let watched_root = worktree.clone();
                        match notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
                            let Ok(event) = result else { return };
                            let Some(kind) = event_kind(&event.kind) else { return };
                            for path in event.paths {
                                let _ = tx.send((watched_id.clone(), path, kind));
                            }
                        }) {
                            Ok(mut watcher) => {
                                if watcher.watch(&worktree, RecursiveMode::Recursive).is_ok() {
                                    watchers.insert(work_id, watcher);
                                    let _ = watched_root;
                                }
                            }
                            Err(err) => {
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
                            bus.worktree_for(&work_id).and_then(|root| {
                                let absolute = root.join(&path);
                                std::fs::read(&absolute).ok().map(|bytes| {
                                    use sha2::{Digest, Sha256};
                                    format!("{:x}", Sha256::digest(bytes))
                                })
                            })
                        };
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
    bus.publish_project(
        work_id,
        ForgeProjectEventKind::Changed,
        Some(relative.to_owned()),
        None,
        None,
    );
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
}
