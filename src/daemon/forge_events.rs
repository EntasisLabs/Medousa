//! Forge freshness + resumable project event bus for Home.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

const ITEM_CAPACITY: usize = 256;
const PROJECT_CAPACITY: usize = 4_096;
const PROJECT_BROADCAST_CAPACITY: usize = 512;

#[derive(Debug, Clone, Serialize)]
pub struct ForgeStreamEvent {
    pub work_id: String,
    pub state: String,
    pub event_kind: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProjectEventKind {
    Created,
    Changed,
    Renamed,
    Deleted,
    GitStatus,
    Snapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeProjectEvent {
    pub seq: u64,
    pub work_id: String,
    pub kind: ForgeProjectEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ForgeEventBus {
    item_tx: broadcast::Sender<ForgeStreamEvent>,
    project_tx: broadcast::Sender<ForgeProjectEvent>,
    next_seq: Arc<AtomicU64>,
    project_log: Arc<Mutex<VecDeque<ForgeProjectEvent>>>,
    /// Latest known worktree root per work item (for FS observation).
    worktrees: Arc<Mutex<HashMap<String, std::path::PathBuf>>>,
    watcher_generation: Arc<AtomicU64>,
    watcher_overflow: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for ForgeEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeEventBus {
    pub fn new() -> Self {
        let (item_tx, _) = broadcast::channel(ITEM_CAPACITY);
        let (project_tx, _) = broadcast::channel(PROJECT_BROADCAST_CAPACITY);
        Self {
            item_tx,
            project_tx,
            next_seq: Arc::new(AtomicU64::new(0)),
            project_log: Arc::new(Mutex::new(VecDeque::with_capacity(PROJECT_CAPACITY))),
            worktrees: Arc::new(Mutex::new(HashMap::new())),
            watcher_generation: Arc::new(AtomicU64::new(1)),
            watcher_overflow: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn watcher_generation(&self) -> u64 {
        self.watcher_generation.load(Ordering::Relaxed)
    }

    pub fn watcher_overflow(&self) -> bool {
        self.watcher_overflow.load(Ordering::Relaxed)
    }

    pub fn bump_watcher_generation(&self) {
        self.watcher_generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mark_watcher_overflow(&self) {
        self.watcher_overflow.store(true, Ordering::Relaxed);
        self.bump_watcher_generation();
    }

    pub fn publish(&self, work_id: &str, state: &str, event_kind: &str) {
        let _ = self.item_tx.send(ForgeStreamEvent {
            work_id: work_id.to_owned(),
            state: state.to_owned(),
            event_kind: event_kind.to_owned(),
            updated_at: Utc::now(),
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ForgeStreamEvent> {
        self.item_tx.subscribe()
    }

    pub fn remember_worktree(&self, work_id: &str, worktree: impl Into<std::path::PathBuf>) {
        if let Ok(mut guard) = self.worktrees.lock() {
            guard.insert(work_id.to_owned(), worktree.into());
        }
    }

    pub fn worktree_for(&self, work_id: &str) -> Option<std::path::PathBuf> {
        self.worktrees
            .lock()
            .ok()
            .and_then(|guard| guard.get(work_id).cloned())
    }

    pub fn tracked_worktrees(&self) -> Vec<(String, std::path::PathBuf)> {
        self.worktrees
            .lock()
            .map(|guard| guard.iter().map(|(id, path)| (id.clone(), path.clone())).collect())
            .unwrap_or_default()
    }

    pub fn publish_project(
        &self,
        work_id: &str,
        kind: ForgeProjectEventKind,
        path: Option<String>,
        old_path: Option<String>,
        digest: Option<String>,
    ) -> ForgeProjectEvent {
        let event = ForgeProjectEvent {
            seq: self.next_seq.fetch_add(1, Ordering::Relaxed) + 1,
            work_id: work_id.to_owned(),
            kind,
            path,
            old_path,
            digest,
            updated_at: Utc::now(),
        };
        if let Ok(mut log) = self.project_log.lock() {
            log.push_back(event.clone());
            while log.len() > PROJECT_CAPACITY {
                log.pop_front();
            }
        }
        let _ = self.project_tx.send(event.clone());
        event
    }

    pub fn subscribe_project(&self) -> broadcast::Receiver<ForgeProjectEvent> {
        self.project_tx.subscribe()
    }

    pub fn snapshot_project_since(
        &self,
        work_id: &str,
        since: u64,
    ) -> Vec<ForgeProjectEvent> {
        self.project_log
            .lock()
            .map(|log| {
                log.iter()
                    .filter(|event| event.work_id == work_id && event.seq > since)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn latest_project_seq(&self) -> u64 {
        self.next_seq.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_journal_replays_from_cursor_for_one_work_item() {
        let bus = ForgeEventBus::new();
        bus.publish_project(
            "work-a",
            ForgeProjectEventKind::Changed,
            Some("a.rs".into()),
            None,
            Some("digest-a".into()),
        );
        bus.publish_project(
            "work-b",
            ForgeProjectEventKind::Created,
            Some("b.rs".into()),
            None,
            None,
        );
        let third = bus.publish_project(
            "work-a",
            ForgeProjectEventKind::Renamed,
            Some("a2.rs".into()),
            Some("a.rs".into()),
            None,
        );

        let replay = bus.snapshot_project_since("work-a", 1);
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].seq, third.seq);
        assert_eq!(replay[0].path.as_deref(), Some("a2.rs"));
        assert_eq!(replay[0].old_path.as_deref(), Some("a.rs"));
    }
}
