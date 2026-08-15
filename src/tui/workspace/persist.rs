//! Persist TUI workspace session (layout + tab bindings) under the data dir.
//!
//! Scoped per workshop (Home v4 analogue):
//! `{dataDir}/tui_workspaces/{scope}/tui_workspace_session_v1.json`
//!
//! Legacy unscoped `{dataDir}/tui_workspace_session_v1.json` migrates once into
//! the current scope when that scoped file is missing.

use std::fs;
use std::path::{Path, PathBuf};

use super::session::WorkspaceShell;

const WORKSPACE_FILE: &str = "tui_workspace_session_v1.json";
const WORKSPACES_DIR: &str = "tui_workspaces";

pub fn legacy_workspace_session_path() -> PathBuf {
    crate::paths::medousa_data_dir().join(WORKSPACE_FILE)
}

pub fn workspace_session_path_for(scope: &str) -> std::io::Result<PathBuf> {
    let scope = medousa_types::authority_id::WorkshopScopeId::parse(scope)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    Ok(crate::paths::medousa_data_dir()
        .join(WORKSPACES_DIR)
        .join(scope.storage_key().as_str())
        .join(WORKSPACE_FILE))
}

/// Back-compat path helper (unscoped legacy location).
pub fn workspace_session_path() -> PathBuf {
    legacy_workspace_session_path()
}

fn migrate_legacy_into(scope: &str) {
    let Ok(scoped) = workspace_session_path_for(scope) else {
        return;
    };
    if scoped.exists() {
        return;
    }
    let legacy = legacy_workspace_session_path();
    if !legacy.exists() {
        return;
    }
    if let Some(parent) = scoped.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    // Prefer rename; fall back to copy so another scope can still migrate later
    // if rename fails across devices.
    if fs::rename(&legacy, &scoped).is_err() {
        let _ = fs::copy(&legacy, &scoped);
    }
}

pub fn load_workspace_session_for(scope: &str) -> Option<WorkspaceShell> {
    migrate_legacy_into(scope);
    load_from_path(&workspace_session_path_for(scope).ok()?)
}

/// Load legacy unscoped file (tests / migration helpers).
pub fn load_workspace_session() -> Option<WorkspaceShell> {
    load_from_path(&legacy_workspace_session_path())
}

fn load_from_path(path: &Path) -> Option<WorkspaceShell> {
    let raw = fs::read_to_string(path).ok()?;
    let mut shell: WorkspaceShell = serde_json::from_str(&raw).ok()?;
    if shell.version == 0 {
        shell.version = 1;
    }
    shell.sanitize();
    Some(shell)
}

pub fn save_workspace_session_for(scope: &str, shell: &WorkspaceShell) -> std::io::Result<()> {
    save_to_path(&workspace_session_path_for(scope)?, shell)
}

pub fn save_workspace_session(shell: &WorkspaceShell) -> std::io::Result<()> {
    save_to_path(&legacy_workspace_session_path(), shell)
}

fn save_to_path(path: &Path, shell: &WorkspaceShell) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(shell)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn clear_workspace_session_for(scope: &str) -> std::io::Result<()> {
    clear_path(&workspace_session_path_for(scope)?)
}

pub fn clear_workspace_session() -> std::io::Result<()> {
    clear_path(&legacy_workspace_session_path())
}

fn clear_path(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workspace::{SplitDirection, WorkspaceShell};

    #[test]
    fn round_trip_scoped_workspace_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _data_dir = crate::paths::scoped_test_data_dir(dir.path());

        let mut shell = WorkspaceShell::bootstrap("sess-a", "Chat A");
        assert!(shell.split_active(SplitDirection::Right, "sess-b"));
        save_workspace_session_for("personal", &shell).expect("save");

        let loaded = load_workspace_session_for("personal").expect("load");
        assert_eq!(loaded.pane_count(), 2);
        assert_eq!(loaded.layout().tabs.len(), 2);

        // Different scope is independent.
        assert!(load_workspace_session_for("paired-x").is_none());

        clear_workspace_session_for("personal").expect("clear");
        assert!(load_workspace_session_for("personal").is_none());

    }

    #[test]
    fn migrates_legacy_unscoped_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _data_dir = crate::paths::scoped_test_data_dir(dir.path());

        let shell = WorkspaceShell::bootstrap("legacy", "Legacy");
        save_workspace_session(&shell).expect("legacy save");
        assert!(legacy_workspace_session_path().exists());

        let loaded = load_workspace_session_for("personal").expect("migrated load");
        assert_eq!(loaded.layout().tabs.len(), 1);
        assert!(workspace_session_path_for("personal").unwrap().exists());
        // Rename should have moved the legacy file away.
        assert!(!legacy_workspace_session_path().exists());

    }
}
