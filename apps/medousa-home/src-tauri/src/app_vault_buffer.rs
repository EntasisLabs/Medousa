//! Tauri host adapter for the vault note buffer application core.

use std::sync::Mutex;

use medousa_app_core::{VaultBufferCommand, VaultBufferCore, VaultBufferCoreError, VaultBufferUpdate};
use tauri::State;

#[derive(Default)]
pub struct AppVaultBufferState {
    core: Mutex<VaultBufferCore>,
}

fn map_error(error: VaultBufferCoreError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn app_vault_buffer_dispatch(
    state: State<'_, AppVaultBufferState>,
    command: VaultBufferCommand,
) -> Result<VaultBufferUpdate, String> {
    let mut core = state.core.lock().map_err(|error| error.to_string())?;
    core.dispatch(command).map_err(map_error)
}

#[tauri::command]
pub fn app_vault_buffer_snapshot(
    state: State<'_, AppVaultBufferState>,
    path: Option<String>,
) -> Result<VaultBufferUpdate, String> {
    app_vault_buffer_dispatch(
        state,
        VaultBufferCommand::Snapshot { path },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_app_core::NoteBufferFact;

    #[test]
    fn seed_and_edit_round_trip() {
        let state = AppVaultBufferState::default();
        let mut core = state.core.lock().unwrap();
        core.dispatch(VaultBufferCommand::SeedBuffer {
            buffer: NoteBufferFact {
                path: "notes/a.md".into(),
                content: "hi".into(),
                baseline_content: "hi".into(),
                content_hash: None,
                title: "a".into(),
                dirty: false,
                content_revision: 1,
                conflict: false,
            },
        })
        .unwrap();
        let update = core
            .dispatch(VaultBufferCommand::ApplyEdit {
                path: "notes/a.md".into(),
                content: "hi!".into(),
                title: None,
            })
            .unwrap();
        let VaultBufferUpdate::Snapshot { snapshot } = update else {
            panic!("expected snapshot");
        };
        assert!(snapshot.buffers[0].dirty);
    }
}
