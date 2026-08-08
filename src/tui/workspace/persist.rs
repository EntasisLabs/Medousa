//! Persist TUI workspace session (layout + tab bindings) under the data dir.

use std::fs;
use std::path::PathBuf;

use super::session::WorkspaceShell;

const WORKSPACE_FILE: &str = "tui_workspace_session_v1.json";

pub fn workspace_session_path() -> PathBuf {
    crate::paths::medousa_data_dir().join(WORKSPACE_FILE)
}

pub fn load_workspace_session() -> Option<WorkspaceShell> {
    let path = workspace_session_path();
    let raw = fs::read_to_string(path).ok()?;
    let mut shell: WorkspaceShell = serde_json::from_str(&raw).ok()?;
    if shell.version == 0 {
        shell.version = 1;
    }
    shell.sanitize();
    Some(shell)
}

pub fn save_workspace_session(shell: &WorkspaceShell) -> std::io::Result<()> {
    let path = workspace_session_path();
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

pub fn clear_workspace_session() -> std::io::Result<()> {
    let path = workspace_session_path();
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
    fn round_trip_workspace_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var_os("MEDOUSA_DATA_DIR");
        // paths::medousa_data_dir respects MEDOUSA_DATA_DIR when set.
        unsafe { std::env::set_var("MEDOUSA_DATA_DIR", dir.path()) };

        let mut shell = WorkspaceShell::bootstrap("sess-a", "Chat A");
        assert!(shell.split_active(SplitDirection::Right, "sess-b"));
        save_workspace_session(&shell).expect("save");

        let loaded = load_workspace_session().expect("load");
        assert_eq!(loaded.pane_count(), 2);
        assert_eq!(loaded.layout().tabs.len(), 2);

        clear_workspace_session().expect("clear");
        assert!(load_workspace_session().is_none());

        match prev {
            Some(v) => unsafe { std::env::set_var("MEDOUSA_DATA_DIR", v) },
            None => unsafe { std::env::remove_var("MEDOUSA_DATA_DIR") },
        }
    }
}
