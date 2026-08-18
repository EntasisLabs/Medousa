//! Coalesced vault filesystem watcher (H07 freshness).
//!
//! Watcher events never certify freshness by themselves — they only bump the
//! reconcile epoch / stale flag so the next `ensure_index_fresh` runs a bounded
//! meta reconcile.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use once_cell::sync::Lazy;

use crate::vault::store::PROJECTION;

static WATCHER_STARTED: AtomicBool = AtomicBool::new(false);
static PENDING_HINTS: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Drain coalesced relative path hints (best-effort; may be empty).
pub fn take_pending_path_hints() -> Vec<String> {
    PENDING_HINTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .drain(..)
        .collect()
}

fn push_hint(path: &Path, root: &Path) {
    let Ok(relative) = path.strip_prefix(root) else {
        return;
    };
    let rendered = relative.to_string_lossy().replace('\\', "/");
    if rendered.is_empty() || rendered.starts_with('.') {
        return;
    }
    let mut guard = PENDING_HINTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.len() < 256 && !guard.iter().any(|existing| existing == &rendered) {
        guard.push(rendered);
    }
}

/// Mark projection stale so the next accessor reconciles. Public for tests and
/// manual refresh paths.
pub fn note_external_change() {
    PROJECTION.mark_stale_reconciling();
}

/// Start a recursive watcher on the active user vault root (once per process).
pub fn spawn_vault_root_watcher() {
    if WATCHER_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let root = crate::vault::path::user_vault_root();
    if !root.is_dir() {
        WATCHER_STARTED.store(false, Ordering::Release);
        return;
    }
    std::thread::Builder::new()
        .name("vault-watch".into())
        .spawn(move || run_watcher_loop(root))
        .ok();
}

fn run_watcher_loop(root: PathBuf) {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    }) {
        Ok(watcher) => watcher,
        Err(_) => {
            WATCHER_STARTED.store(false, Ordering::Release);
            return;
        }
    };
    if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
        WATCHER_STARTED.store(false, Ordering::Release);
        return;
    }

    let root = Arc::new(root);
    let mut dirty = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(event)) => {
                if matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) || event.need_rescan()
                {
                    for path in &event.paths {
                        push_hint(path, root.as_path());
                    }
                    dirty = true;
                }
            }
            Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if dirty {
                    dirty = false;
                    note_external_change();
                }
            }
        }
    }
    WATCHER_STARTED.store(false, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_change_marks_projection_stale() {
        let _lock = crate::vault::service::vault_integration_test_lock();
        PROJECTION.clear_stale();
        assert!(!PROJECTION.is_stale());
        note_external_change();
        assert!(PROJECTION.is_stale());
        assert!(PROJECTION.needs_reconcile());
    }

    #[test]
    fn warm_skip_disabled_until_reconcile_certified() {
        let _lock = crate::vault::service::vault_integration_test_lock();
        PROJECTION.mark_stale_reconciling();
        assert!(PROJECTION.needs_reconcile());
        let epoch = PROJECTION.reconcile_epoch();
        PROJECTION.certify_reconcile(epoch);
        assert!(!PROJECTION.needs_reconcile());
        // A newer epoch (watcher) re-opens the fence.
        PROJECTION.mark_stale_reconciling();
        assert!(PROJECTION.needs_reconcile());
    }
}
