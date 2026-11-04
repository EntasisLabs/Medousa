//! Tauri host adapter for the renderer-agnostic shell layout core.
//!
//! The webview dual-writes layout mutations through [`ShellCore`] and may
//! apply returned snapshots. Persistence I/O and LME/browser ports stay in JS.

use std::sync::Mutex;

use medousa_app_core::{ShellCommand, ShellCore, ShellCoreError, ShellUpdate};
use tauri::State;

#[derive(Default)]
pub struct AppShellState {
    core: Mutex<ShellCore>,
}

fn map_error(error: ShellCoreError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn app_shell_dispatch(
    state: State<'_, AppShellState>,
    command: ShellCommand,
) -> Result<ShellUpdate, String> {
    let mut core = state.core.lock().map_err(|error| error.to_string())?;
    core.dispatch(command).map_err(map_error)
}

#[tauri::command]
pub fn app_shell_snapshot(state: State<'_, AppShellState>) -> Result<ShellUpdate, String> {
    app_shell_dispatch(state, ShellCommand::Snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_app_core::ShellTab;

    #[test]
    fn dispatch_open_chat_tab_round_trips() {
        let state = AppShellState::default();
        let mut core = state.core.lock().unwrap();
        let update = core
            .dispatch(ShellCommand::OpenTab {
                tab: ShellTab::Chat {
                    id: "t1".into(),
                    session_id: "ses_a".into(),
                    title: "A".into(),
                },
                group_id: None,
                activate: true,
            })
            .unwrap();
        let ShellUpdate::Snapshot { snapshot } = update;
        assert_eq!(snapshot.layout.tabs.len(), 1);
        assert_eq!(snapshot.layout.tabs[0].id(), "t1");
    }
}
